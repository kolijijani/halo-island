//! Stocks (an add-on, off unless switched on): the prices on the Stocks page,
//! from Yahoo Finance. Words, parsing and pacing live in `halo_core::stocks`;
//! this file does the HTTP on one worker thread, which the Mac app runs too
//! (it shares this file as is).
//!
//! The thread starts the first time the Stocks page opens, and asks only
//! while the page is on screen and someone is at the computer (`Watch`):
//! - the index and the list's stocks as the page opens (at most once in a
//!   minute) or the list changes, then every minute while one of their
//!   markets trades, 10 minutes while all are shut;
//! - what the search's words find, once per search, half a second after the
//!   typing stops (`Watch::query`), when the built-in Bursa list isn't enough;
//! - a year of one stock's closes, once a run per stock, when a range longer
//!   than a day is picked;
//! - failures back off from a minute to 10, honouring `Retry-After`. Yahoo
//!   saying "too many" (429) is said as limited, not as an outage.
//!
//! The page closing stops all of it at once: the thread sleeps on a condition
//! variable, at zero CPU, until the page opens again. Nothing is written to
//! disk or the log beyond a line per change of state.

use crate::events::{AppEvent, EventSink};
use crate::services::http::{Client, FetchError, MtaGuard, Response};
use halo_core::net::Failure;
use halo_core::stocks::{self, Board, History};
use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

const USER_AGENT: &str = concat!("HaloStudio/", env!("CARGO_PKG_VERSION"), " (+https://github.com/kolijijani/halo-island)");
const TIMEOUT: Duration = Duration::from_secs(8);
/// The page opening again within this time asks for nothing new: Yahoo's
/// day line moves by 5 minutes.
const REOPEN_GAP: Duration = Duration::from_secs(60);
/// A search or a year that didn't come is asked again after this, while
/// the page still wants it.
const RETRY_GAP: Duration = Duration::from_secs(30);
/// No feed address in this build: looked at again this rarely.
const OFF_GAP: Duration = Duration::from_secs(600);

/// What the Stocks page wants from the worker (`pages::stocks::watch`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Watch {
    /// The page is on screen and someone is at the computer.
    pub open: bool,
    /// The list, after the index.
    pub codes: Vec<String>,
    /// The stock (or the index) whose year of closes the chart wants.
    pub chart: Option<String>,
    /// The search's words, once the typing has rested: what Yahoo Finance
    /// lists by them is wanted.
    pub query: Option<String>,
}

/// What the worker tells the island.
#[derive(Clone, Debug)]
pub enum StocksEvent {
    /// A new answer.
    Board(Board),
    /// The answer hasn't changed since the last one (a 304).
    Same,
    /// The prices couldn't be asked for.
    Failed(Trouble),
    /// What a search found: (symbol, name), for these words.
    Found(String, Vec<(String, String)>),
    /// That search couldn't be made (the search lets a typed symbol through).
    NotFound(String, Trouble),
    History(History),
    /// That code's year of closes couldn't be read.
    NoHistory(String),
}

/// Why the prices didn't come.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trouble {
    /// No internet, a timeout, the feed unreachable or erring, an answer
    /// that isn't prices, or no feed address in this build
    /// (`halo_core::net::Failure`).
    Net(halo_core::net::Failure),
    /// Yahoo Finance asked us to slow down (429): limited for a while.
    Limited,
}

pub struct StocksService {
    shared: Arc<Shared>,
}

impl StocksService {
    pub fn start(sink: EventSink) -> std::io::Result<Self> {
        let shared = Arc::new(Shared { q: Mutex::new(Queue::default()), cv: Condvar::new() });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("halo-stocks".into())
            .spawn(move || worker_main(worker, sink))
            .map_err(|e| std::io::Error::other(format!("could not start the Stocks thread: {e}")))?;
        Ok(Self { shared })
    }

    /// The page opened, closed or wants something else (UI thread safe).
    pub fn set_watch(&self, w: Watch) {
        let mut q = self.shared.lock();
        if q.watch != w {
            q.watch = w;
            drop(q);
            self.shared.cv.notify_all();
        }
    }
}

impl Drop for StocksService {
    fn drop(&mut self) {
        // The worker exits at its next wait; a call in flight ends on its own
        // timeout. Never join from the UI thread.
        self.shared.lock().shutdown = true;
        self.shared.cv.notify_all();
    }
}

#[derive(Default)]
struct Queue {
    watch: Watch,
    shutdown: bool,
}

struct Shared {
    q: Mutex<Queue>,
    cv: Condvar,
}

impl Shared {
    /// The queue holds plain data, so a panic elsewhere can't leave it
    /// inconsistent: recover from poisoning instead of panicking.
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.q.lock().unwrap_or_else(|p| p.into_inner())
    }
}

/// Where answers come from: Yahoo Finance, or `HALO_STOCKS_URL` (a copy of
/// its API, for testing).
fn base_url() -> String {
    match std::env::var("HALO_STOCKS_URL") {
        Ok(u) if u.starts_with("http") => {
            let u = u.trim().to_string();
            if u.ends_with('/') {
                u
            } else {
                u + "/"
            }
        }
        _ => stocks::BASE.to_string(),
    }
}

struct Worker {
    sink: EventSink,
    client: Option<Client>,
    base: String,
    /// The codes the last answer was for, its tag, and when it was asked.
    codes: Vec<String>,
    tag: Option<String>,
    asked: Option<Instant>,
    next_board: Instant,
    /// How soon the last answer said a fresher one comes.
    every_s: Option<u64>,
    fails: u32,
    /// What the last ask said, so the log notes each change once.
    said: Option<&'static str>,
    /// The last search made and when it may be made again after a failure.
    searched: Option<String>,
    next_search: Instant,
    /// Years read (or found missing) this run, and when a failed one may be asked again.
    years: HashMap<String, bool>,
    next_year: HashMap<String, Instant>,
    was_open: bool,
}

fn worker_main(shared: Arc<Shared>, sink: EventSink) {
    let _com = MtaGuard::new();
    let mut w = Worker::new(sink, Instant::now());
    loop {
        let watch = {
            let mut q = shared.lock();
            loop {
                if q.shutdown {
                    return;
                }
                let now = Instant::now();
                let watch = q.watch.clone();
                w.opened(&watch, now);
                match w.next_due(&watch, now) {
                    Some(t) if t <= now => break watch,
                    Some(t) => q = wait(&shared, q, Some(t - now)),
                    None => q = wait(&shared, q, None),
                }
            }
        };
        w.due(&watch);
    }
}

fn wait<'a>(shared: &Shared, g: MutexGuard<'a, Queue>, timeout: Option<Duration>) -> MutexGuard<'a, Queue> {
    match timeout {
        Some(d) => match shared.cv.wait_timeout(g, d) {
            Ok((g, _)) => g,
            Err(p) => p.into_inner().0,
        },
        None => shared.cv.wait(g).unwrap_or_else(|p| p.into_inner()),
    }
}

impl Worker {
    fn new(sink: EventSink, now: Instant) -> Self {
        Self {
            sink,
            client: None,
            base: base_url(),
            codes: Vec::new(),
            tag: None,
            asked: None,
            next_board: now,
            every_s: None,
            fails: 0,
            said: None,
            searched: None,
            next_search: now,
            years: HashMap::new(),
            next_year: HashMap::new(),
            was_open: false,
        }
    }

    fn send(&self, ev: StocksEvent) {
        self.sink.send(AppEvent::Stocks(ev));
    }

    fn client(&mut self) -> Result<&Client, FetchError> {
        if self.client.is_none() {
            // Its own client, with no disk cache: Yahoo marks answers fresh
            // for 5 minutes, which would hide a minute's prices.
            self.client = Some(Client::private(USER_AGENT)?);
        }
        Ok(self.client.as_ref().expect("client was just made"))
    }

    /// The page just opened: the prices are asked for now, unless the last
    /// ask was moments ago (flicking between tabs).
    fn opened(&mut self, watch: &Watch, now: Instant) {
        if watch.open && !self.was_open && self.fails == 0 {
            let soonest = self.asked.map_or(now, |a| a + REOPEN_GAP);
            self.next_board = self.next_board.min(soonest.max(now));
        }
        self.was_open = watch.open;
    }

    /// When the next call is due; None while the page is closed. "At once" is
    /// `now`, the caller's own reading: a fresh `Instant::now()` here is always
    /// a little later, so `t <= now` never held and a new list was never asked
    /// for (0.2.11: "Getting prices" for good).
    fn next_due(&self, watch: &Watch, now: Instant) -> Option<Instant> {
        if !watch.open {
            return None;
        }
        // A changed list is asked for at once.
        let mut t = if watch.codes != self.codes { now } else { self.next_board };
        if watch.query.is_some() && watch.query != self.searched {
            t = t.min(self.next_search.max(now));
        }
        if let Some(code) = watch.chart.as_ref().filter(|c| !self.years.contains_key(*c)) {
            t = t.min(self.next_year.get(code).copied().unwrap_or(now));
        }
        Some(t)
    }

    fn due(&mut self, watch: &Watch) {
        let now = Instant::now();
        if watch.codes != self.codes || self.next_board <= now {
            self.ask_board(&watch.codes);
        }
        if let Some(q) = watch.query.as_ref().filter(|q| self.searched.as_ref() != Some(*q)) {
            if self.next_search <= Instant::now() {
                self.ask_search(q);
            }
        }
        if let Some(code) = watch.chart.as_ref().filter(|c| !self.years.contains_key(*c)) {
            if self.next_year.get(code).is_none_or(|t| *t <= Instant::now()) {
                self.ask_year(code);
            }
        }
    }

    fn get(&mut self, url: &str, tag: Option<&str>) -> Result<Response, FetchError> {
        let etag = tag.map(|t| format!("\"{t}\""));
        let headers: Vec<(&str, &str)> = etag.as_deref().map(|e| vec![("If-None-Match", e)]).unwrap_or_default();
        self.client()?.get_with_headers(url, &headers, TIMEOUT)
    }

    fn note(&mut self, what: &'static str) {
        if self.said != Some(what) {
            self.said = Some(what);
            crate::pinfo!("stocks: {what}");
        }
    }

    fn ask_board(&mut self, codes: &[String]) {
        // No real feed in this build: never ask a placeholder (it could
        // only fail and read as an outage). Said once, checked again rarely.
        if !halo_core::net::configured(&self.base) {
            self.note("no feed address in this build (HALO_STOCKS_URL)");
            self.codes = codes.to_vec();
            self.send(StocksEvent::Failed(Trouble::Net(Failure::NotConfigured)));
            self.next_board = Instant::now() + OFF_GAP;
            return;
        }
        if codes != self.codes {
            // Another list is another answer: its tag can't match.
            self.codes = codes.to_vec();
            self.tag = None;
        }
        let asked: Vec<String> = std::iter::once(stocks::INDEX.to_string()).chain(codes.iter().cloned()).collect();
        let url = stocks::watch_url(&self.base, &asked);
        self.asked = Some(Instant::now());
        let res = self.get(&url, None);
        let next = match res {
            Ok(r) if r.status == 200 => match Board::parse(&r.body, &asked) {
                Some(b) if self.tag.as_deref() == Some(b.tag.as_str()) => {
                    self.fails = 0;
                    self.every_s = Some(b.every_s(crate::clock::unix_now()));
                    self.send(StocksEvent::Same);
                    None
                }
                Some(b) => {
                    self.fails = 0;
                    self.every_s = Some(b.every_s(crate::clock::unix_now()));
                    self.tag = Some(b.tag.clone()).filter(|t| !t.is_empty());
                    self.note("prices coming in");
                    self.send(StocksEvent::Board(b));
                    None
                }
                None => {
                    self.note("the feed sent something that isn't prices");
                    self.failed(Trouble::Net(Failure::Malformed), None)
                }
            },
            Ok(r) if r.status == 429 => {
                self.note("Yahoo Finance is limiting us (429)");
                self.failed(Trouble::Limited, r.retry_after_s)
            }
            // None of the symbols has prices (every one delisted or mistyped).
            Ok(r) if r.status == 404 => {
                self.fails = 0;
                self.send(StocksEvent::Board(Board { tag: String::new(), rows: Vec::new(), missing: asked.clone() }));
                Some(OFF_GAP)
            }
            Ok(r) => {
                self.note("the feed answered with an error");
                crate::pwarn!("stocks: HTTP {}", r.status);
                self.failed(Trouble::Net(Failure::Status(r.status)), r.retry_after_s)
            }
            Err(FetchError::Other(e)) => {
                crate::pwarn!("stocks: {e}");
                self.failed(Trouble::Net(Failure::Unreachable), None)
            }
            Err(e) => {
                let why = crate::services::net::failure(&e);
                self.note(if why == Failure::Offline { "no internet" } else { "the feed can't be reached" });
                self.failed(Trouble::Net(why), None)
            }
        };
        let wait = next.unwrap_or_else(|| Duration::from_secs(stocks::next_ask(self.every_s, self.fails)));
        self.next_board = Instant::now() + wait;
    }

    /// A failed ask: tell the page, back off. `retry_after_s`: the server's own wait.
    fn failed(&mut self, why: Trouble, retry_after_s: Option<u64>) -> Option<Duration> {
        self.fails = self.fails.saturating_add(1);
        self.send(StocksEvent::Failed(why));
        let backoff = stocks::next_ask(self.every_s, self.fails);
        Some(Duration::from_secs(retry_after_s.map_or(backoff, |s| s.clamp(backoff, 600))))
    }

    fn ask_search(&mut self, query: &str) {
        self.searched = Some(query.to_string());
        if !halo_core::net::configured(&self.base) {
            self.send(StocksEvent::NotFound(query.to_string(), Trouble::Net(Failure::NotConfigured)));
            return;
        }
        let why = match self.get(&stocks::search_url(&self.base, query), None) {
            Ok(r) if r.status == 200 => match stocks::parse_search(&r.body) {
                Some(found) => {
                    self.send(StocksEvent::Found(query.to_string(), found));
                    return;
                }
                None => Trouble::Net(Failure::Malformed),
            },
            Ok(r) if r.status == 429 => Trouble::Limited,
            Ok(r) => Trouble::Net(Failure::Status(r.status)),
            Err(e) => Trouble::Net(crate::services::net::failure(&e)),
        };
        crate::pwarn!("stocks: search: {why:?}");
        // The same words may be asked again after a while.
        self.searched = None;
        self.next_search = Instant::now() + RETRY_GAP;
        self.send(StocksEvent::NotFound(query.to_string(), why));
    }

    fn ask_year(&mut self, code: &str) {
        if !halo_core::net::configured(&self.base) {
            self.years.insert(code.to_string(), false);
            self.send(StocksEvent::NoHistory(code.to_string()));
            return;
        }
        let url = stocks::chart_url(&self.base, code);
        match self.get(&url, None) {
            Ok(r) if r.status == 200 => {
                if let Some(h) = History::parse(&r.body) {
                    self.years.insert(code.to_string(), true);
                    self.send(StocksEvent::History(h));
                    return;
                }
                self.years.insert(code.to_string(), false);
            }
            // No year for that code: no use asking again this run.
            Ok(r) if r.status == 404 => {
                self.years.insert(code.to_string(), false);
            }
            Ok(r) => crate::pwarn!("stocks: chart {code}: HTTP {}", r.status),
            Err(e) => crate::pwarn!("stocks: chart {code}: {e}"),
        }
        self.next_year.insert(code.to_string(), Instant::now() + RETRY_GAP);
        self.send(StocksEvent::NoHistory(code.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Sender};

    // The Mac app builds this file too; its `EventSink` takes no window handle.
    #[cfg(windows)]
    fn sink(tx: Sender<AppEvent>) -> EventSink {
        EventSink::new(tx, crate::events::SendHwnd(0))
    }

    #[cfg(not(windows))]
    fn sink(tx: Sender<AppEvent>) -> EventSink {
        EventSink::new(tx)
    }

    fn worker(now: Instant) -> Worker {
        Worker::new(sink(mpsc::channel().0), now)
    }

    fn open(codes: &[&str]) -> Watch {
        Watch { open: true, codes: codes.iter().map(|c| c.to_string()).collect(), ..Watch::default() }
    }

    /// The thread asks only once `next_due` is at or before its own `now`.
    /// 0.2.11 read the clock again for "at once", which is always later, so
    /// the page said "Getting prices" for good.
    #[test]
    fn an_opened_page_is_asked_for_at_once() {
        let now = Instant::now() + Duration::from_millis(5);
        let mut w = worker(now - Duration::from_millis(5));
        let watch = open(&["BBCA", "TLKM"]);
        w.opened(&watch, now);
        assert!(w.next_due(&watch, now).is_some_and(|t| t <= now));
        // The index alone too (an empty list).
        let mut w = worker(now - Duration::from_millis(5));
        let watch = open(&[]);
        w.opened(&watch, now);
        assert!(w.next_due(&watch, now).is_some_and(|t| t <= now));
    }

    #[test]
    fn a_wanted_year_is_asked_for_at_once_while_the_prices_wait() {
        let now = Instant::now();
        let mut w = worker(now);
        let mut watch = open(&["BBCA"]);
        w.codes = watch.codes.clone();
        w.next_board = now + Duration::from_secs(60);
        assert_eq!(w.next_due(&watch, now), Some(now + Duration::from_secs(60)));
        watch.chart = Some("BBCA".into());
        assert_eq!(w.next_due(&watch, now), Some(now));
    }

    /// This build's feed address is a placeholder (`.invalid`): the page is
    /// told it isn't set up, with no request made and no price invented,
    /// and it isn't asked again for minutes.
    #[test]
    fn a_placeholder_feed_is_never_asked_and_says_so() {
        let (tx, rx) = mpsc::channel();
        let now = Instant::now();
        let mut w = Worker::new(sink(tx), now);
        w.base = "https://stocks.local.invalid/".into();
        w.ask_board(&["BBCA".to_string()]);
        let got: Vec<AppEvent> = rx.try_iter().collect();
        assert_eq!(got.len(), 1);
        assert!(matches!(got[0], AppEvent::Stocks(StocksEvent::Failed(Trouble::Net(Failure::NotConfigured)))));
        assert!(w.client.is_none(), "no HTTP client was even made");
        assert!(w.next_board >= now + OFF_GAP);
    }

    #[test]
    fn a_closed_page_asks_for_nothing() {
        let now = Instant::now();
        let w = worker(now);
        assert_eq!(w.next_due(&Watch { open: false, ..open(&["BBCA"]) }, now), None);
    }

    /// The real thread against the live Yahoo Finance (CI's Windows unit
    /// tests run it with `--ignored`): the KLCI and the starters' prices, a
    /// search and a year come in. The self-test shows a made-up market and
    /// asks for nothing, which is how 0.2.11 shipped a thread that never
    /// asked. Silence fails; Yahoo answering badly (a datacenter's address
    /// is often limited) only warns, since the thread asked.
    #[test]
    #[ignore = "asks the live Yahoo Finance"]
    fn the_live_market_comes_in() {
        let (tx, rx) = mpsc::channel();
        let service = StocksService::start(sink(tx)).expect("the Stocks thread starts");
        let codes: Vec<String> = stocks::STARTERS.iter().map(|c| c.to_string()).collect();
        let start = Instant::now();
        // As the page opens on the day's chart it wants the prices only.
        let mut watch = Watch { open: true, codes: codes.clone(), ..Watch::default() };
        service.set_watch(watch.clone());
        let (mut board, mut found, mut year) = (None, None, None);
        while board.is_none() || found.is_none() || year.is_none() {
            if board.is_some() && watch.query.is_none() {
                // Then something is searched for and 1M is picked.
                watch = Watch { query: Some("toyota".into()), chart: Some("1155.KL".into()), ..watch };
                service.set_watch(watch.clone());
            }
            let left = Duration::from_secs(30).saturating_sub(start.elapsed());
            let ev = match rx.recv_timeout(left) {
                Ok(AppEvent::Stocks(ev)) => ev,
                Ok(_) => continue,
                Err(_) => {
                    let missing = [("prices", board.is_none()), ("a search", found.is_none()), ("Maybank's year", year.is_none())];
                    let missing: Vec<&str> = missing.iter().filter(|m| m.1).map(|m| m.0).collect();
                    panic!("no {} from the Stocks thread in 30 s: it never asked Yahoo Finance", missing.join(", "));
                }
            };
            let at = start.elapsed().as_secs_f32();
            match ev {
                StocksEvent::Board(b) => {
                    println!("prices after {at:.2} s: {} rows", b.rows.len());
                    board = Some(b);
                }
                StocksEvent::Found(q, f) => {
                    println!("{q}: {} found after {at:.2} s", f.len());
                    found = Some(f);
                }
                StocksEvent::History(h) => {
                    println!("{}'s year after {at:.2} s", h.code);
                    year = Some(h);
                }
                StocksEvent::Same => {}
                StocksEvent::Failed(why) | StocksEvent::NotFound(_, why) => {
                    println!("::warning::The Stocks thread asked, but Yahoo Finance gave nothing ({why:?}) after {at:.2} s");
                    return;
                }
                StocksEvent::NoHistory(_) => {
                    println!("::warning::The Stocks thread asked, but Yahoo Finance gave no year after {at:.2} s");
                    return;
                }
            }
        }
        let board = board.expect("prices came");
        for code in std::iter::once(stocks::INDEX).chain(stocks::STARTERS) {
            let q = board.quote(code).unwrap_or_else(|| panic!("{code} isn't in the answer"));
            assert!(q.price > 0.0, "{code} has no price");
            println!("  {code} {}", q.price);
        }
        assert!(found.is_some_and(|f| f.iter().any(|(c, _)| c == "7203.T")), "a stock from anywhere");
        assert_eq!(year.map(|h| h.code), Some("1155.KL".to_string()));
    }
}

//! Stocks: up to ten stocks from any exchange and the FTSE Bursa Malaysia
//! KLCI, for the Stocks add-on's page, from Yahoo Finance (no key, no
//! account). Bursa Malaysia comes first: its biggest companies are built in
//! for the search ([`bursa`]), and a new list starts with five of them. Any
//! other stock, index or fund Yahoo Finance lists is a search away.
//!
//! The apps ask three things, and only while the Stocks page is on screen
//! (`services::stocks`):
//!
//! * `v7/finance/spark?symbols=^KLSE,1155.KL`: the index and the list's
//!   stocks, each with its price, the previous close, the day's high, low
//!   and volume, the year's range, its exchange's session and the day's
//!   line, a point every 5 minutes. A few KB, asked every minute while one
//!   of their markets trades, 10 minutes while all are shut.
//! * `v1/finance/search?q=apple`: what Yahoo Finance lists by that name or
//!   code, anywhere in the world, as the person types in the search.
//! * `v8/finance/chart/1155.KL?range=1y&interval=1d`: a year of daily
//!   closes, for the 1M, 6M and 1Y ranges (once per stock per run).
//!
//! This file reads those answers and keeps the watchlist, and does the page's
//! arithmetic (number formats, the search, where a line's points go), so all
//! of it is tested on any OS.

use crate::datetime::{civil_from_days, days_from_civil, weekday_from_days};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where the answers come from. The apps read `HALO_STOCKS_URL` to point a
/// test build at another copy of the same API.
pub const BASE: &str = "https://query1.finance.yahoo.com/";
/// The most stocks a list holds, besides the index.
pub const MAX: usize = 10;
/// The index's symbol: the FTSE Bursa Malaysia KLCI. It always comes first.
pub const INDEX: &str = "^KLSE";
/// What a new list holds: Bursa's biggest names, so the page shows a market
/// the first time it opens. Each is one click from gone.
pub const STARTERS: [&str; 5] = ["1155.KL", "1295.KL", "1023.KL", "5347.KL", "5225.KL"];
/// What a list held while the page showed the Indonesia Stock Exchange
/// (0.2.39 and before): such a list starts over with [`STARTERS`].
const IDX_STARTERS: [&str; 5] = ["BBCA", "BBRI", "BMRI", "TLKM", "ASII"];
/// The minus sign numbers carry (U+2212: as wide as the plus in tabular figures).
const MINUS: char = '\u{2212}';

/// True for a Yahoo Finance symbol: letters, digits and `. ^ = & -`, at
/// most 20, upper case ("1155.KL", "AAPL", "^KLSE", "BRK-B", "F&N.KL").
pub fn is_code(s: &str) -> bool {
    (1..=20).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b"^.=&-".contains(&b)) && s.bytes().any(|b| b.is_ascii_alphanumeric())
}

/// What a person typed as a symbol, if it is one: "aapl " → "AAPL", and a
/// Bursa stock's number on its own, "1155" → "1155.KL".
pub fn code_of(s: &str) -> Option<String> {
    let c = s.trim().to_ascii_uppercase();
    let c = if c.len() == 4 && c.bytes().all(|b| b.is_ascii_digit()) { format!("{c}.KL") } else { c };
    is_code(&c).then_some(c)
}

/// How a symbol reads on the page: a Bursa stock's own short name
/// ("MAYBANK"), the index as "KLCI", anything else as Yahoo writes it.
pub fn ticker(code: &str) -> &str {
    if code == INDEX {
        return "KLCI";
    }
    BURSA.iter().find(|b| b.0 == code).map_or(code, |b| b.1)
}

// ---------------------------------------------------------------- the list

/// What each row's change pill shows; a click on any pill moves all of them
/// to the next, as in Apple's Stocks. `Cap` was the market cap, which
/// Yahoo's keyless answers don't carry: a list saved with it shows percent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Show {
    #[default]
    Percent,
    Points,
    Cap,
}

impl Show {
    pub fn next(self) -> Show {
        match self {
            Show::Percent => Show::Points,
            Show::Points | Show::Cap => Show::Percent,
        }
    }
}
/// The chart's time range.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Range {
    /// Today, from the opening bell.
    #[default]
    #[serde(rename = "1d")]
    Day,
    #[serde(rename = "1m")]
    Month,
    #[serde(rename = "6m")]
    HalfYear,
    #[serde(rename = "1y")]
    Year,
}

impl Range {
    pub const ALL: [Range; 4] = [Range::Day, Range::Month, Range::HalfYear, Range::Year];

    pub fn label(self) -> &'static str {
        match self {
            Range::Day => "1D",
            Range::Month => "1M",
            Range::HalfYear => "6M",
            Range::Year => "1Y",
        }
    }

    /// How many months back the range reaches (None: today only).
    fn months(self) -> Option<i64> {
        match self {
            Range::Day => None,
            Range::Month => Some(1),
            Range::HalfYear => Some(6),
            Range::Year => Some(12),
        }
    }
}

/// The person's list, kept in data.json (`store.stocks`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Watchlist {
    /// The stocks, in the person's order: Yahoo Finance symbols, at most
    /// [`MAX`]. The index isn't one of them; it always leads the page.
    pub codes: Vec<String>,
    /// What the right of the page shows: one of `codes`, or [`INDEX`].
    pub pick: String,
    pub show: Show,
    pub range: Range,
    /// 1 once the list holds Yahoo Finance symbols; 0 (a file from 0.2.39 or
    /// before) holds IDX codes, which [`Watchlist::tidy`] moves over.
    #[serde(rename = "feed", default)]
    pub feed: u32,
}

impl Default for Watchlist {
    fn default() -> Self {
        Self { codes: STARTERS.iter().map(|c| c.to_string()).collect(), pick: INDEX.to_string(), show: Show::default(), range: Range::default(), feed: 1 }
    }
}
impl Watchlist {
    pub fn full(&self) -> bool {
        self.codes.len() >= MAX
    }

    pub fn has(&self, code: &str) -> bool {
        self.codes.iter().any(|c| c == code)
    }

    /// Puts `code` at the end of the list and shows it. False (and nothing
    /// changes) when it isn't a code, is there already, or the list is full.
    pub fn add(&mut self, code: &str) -> bool {
        let Some(code) = code_of(code) else { return false };
        if self.has(&code) || self.full() {
            return false;
        }
        self.pick = code.clone();
        self.codes.push(code);
        true
    }

    /// Takes `code` off the list; the index shows if it was the one showing.
    pub fn remove(&mut self, code: &str) -> bool {
        let before = self.codes.len();
        self.codes.retain(|c| c != code);
        if self.pick == code {
            self.pick = INDEX.to_string();
        }
        self.codes.len() != before
    }

    /// Moves `code` to place `to` (0 = first after the index).
    pub fn move_to(&mut self, code: &str, to: usize) {
        if let Some(i) = self.codes.iter().position(|c| c == code) {
            let c = self.codes.remove(i);
            self.codes.insert(to.min(self.codes.len()), c);
        }
    }

    /// The page's rows: the index, then the stocks.
    pub fn symbols(&self) -> impl Iterator<Item = &str> {
        std::iter::once(INDEX).chain(self.codes.iter().map(String::as_str))
    }

    /// A list read from an older or hand-edited file: codes made upper case,
    /// anything that isn't one dropped, no repeats, at most [`MAX`]; a pick
    /// that's gone becomes the index. A list from the IDX days keeps its
    /// stocks as their Jakarta symbols ("BBCA" → "BBCA.JK"), or starts over
    /// with Bursa's if it was still the starters.
    pub fn tidy(&mut self) {
        if self.feed == 0 {
            self.feed = 1;
            if self.codes == IDX_STARTERS {
                *self = Watchlist { show: self.show, range: self.range, ..Watchlist::default() };
            }
            let idx = |c: &str| c.len() == 4 && c.bytes().all(|b| b.is_ascii_alphabetic());
            for c in self.codes.iter_mut().chain(std::iter::once(&mut self.pick)) {
                if c == "IHSG" {
                    *c = INDEX.to_string();
                } else if idx(c) {
                    *c = format!("{}.JK", c.to_ascii_uppercase());
                }
            }
        }
        if self.show == Show::Cap {
            self.show = Show::Percent;
        }
        let mut seen = Vec::new();
        for c in self.codes.drain(..) {
            if let Some(c) = code_of(&c) {
                if !seen.contains(&c) && seen.len() < MAX {
                    seen.push(c);
                }
            }
        }
        self.codes = seen;
        if self.pick != INDEX && !self.has(&self.pick) {
            self.pick = INDEX.to_string();
        }
    }
}

// ---------------------------------------------------------------- answers

/// One row of an answer: the index or a stock.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quote {
    pub code: String,
    /// The name Yahoo Finance gives ("Malayan Banking Berhad"); the code when it has none.
    pub name: String,
    pub price: f64,
    /// The previous close, which the day's change is from.
    pub prev: f64,
    pub high: Option<f64>,
    pub low: Option<f64>,
    /// Shares traded today.
    pub volume: Option<f64>,
    /// The last 52 weeks' low and high.
    pub w52: Option<(f64, f64)>,
    /// "MYR", "USD": what the price is in.
    pub currency: String,
    /// "Kuala Lumpur", "NasdaqGS".
    pub exchange: String,
    /// Decimal places the price is written with.
    pub digits: u8,
    /// When the price is from (Unix seconds).
    pub at: i64,
    /// The exchange's clock: seconds ahead of UTC.
    pub gmtoff: i64,
    /// The exchange's session: its open and close (Unix seconds).
    pub session: (i64, i64),
    /// The same as minutes of the exchange's day: the day line's width.
    pub span: (u16, u16),
    /// The day's line: (minute of the exchange's day; price), oldest first,
    /// a point every 5 minutes.
    pub spark: Vec<(u16, f64)>,
}

/// Which way a price went since the previous close.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trend {
    Up,
    Down,
    Flat,
}

impl Quote {
    pub fn is_index(&self) -> bool {
        self.code.starts_with('^')
    }

    pub fn change(&self) -> f64 {
        self.price - self.prev
    }

    pub fn change_pct(&self) -> Option<f64> {
        (self.prev > 0.0).then(|| (self.price / self.prev - 1.0) * 100.0)
    }

    pub fn trend(&self) -> Trend {
        trend_of(self.price, self.prev)
    }

    /// What the change pill says for `show`.
    pub fn pill(&self, show: Show) -> String {
        match show {
            Show::Points => fmt_change(self.change(), self.digits),
            Show::Percent | Show::Cap => self.change_pct().map_or_else(|| "\u{2014}".to_string(), fmt_pct),
        }
    }

    /// Its market trades now (`now`: Unix seconds).
    pub fn trading(&self, now: i64) -> bool {
        (self.session.0..self.session.1).contains(&now)
    }

    /// The exchange's day of the price.
    pub fn day(&self) -> i64 {
        (self.at + self.gmtoff).div_euclid(86_400)
    }

    /// "14:32" on the exchange's clock.
    pub fn clock(&self, t: i64) -> String {
        let s = (t + self.gmtoff).rem_euclid(86_400);
        format!("{:02}:{:02}", s / 3600, s % 3600 / 60)
    }

    /// The line under the chart: where its market stands and how old the
    /// price is, on the exchange's clock. `now`: Unix seconds.
    pub fn status(&self, now: i64) -> String {
        let today = (now + self.gmtoff).div_euclid(86_400);
        // A price from an earlier day (overnight, a weekend) says which.
        let earlier = Some(self.day()).filter(|d| *d != today).map(|d| {
            if today - d < 7 {
                WEEKDAYS[weekday_from_days(d) as usize].to_string()
            } else {
                date_label(d)
            }
        });
        let (open, close) = self.session;
        match earlier {
            _ if self.trading(now) => format!("Open \u{b7} as of {}", self.clock(self.at)),
            None if now < open => format!("Opens at {}", self.clock(open)),
            Some(d) if now < open && open - now < 86_400 => format!("Opens at {} \u{b7} prices from {d}", self.clock(open)),
            None if close > 0 => format!("Closed \u{b7} {}", self.clock(self.at)),
            None => "Closed".to_string(),
            Some(d) => format!("Closed \u{b7} prices from {d}"),
        }
    }
}

/// Which way `now` went from `base`, to the price's own precision.
pub fn trend_of(now: f64, base: f64) -> Trend {
    let d = now - base;
    if d.abs() < 1e-9 + base.abs() * 1e-6 {
        Trend::Flat
    } else if d > 0.0 {
        Trend::Up
    } else {
        Trend::Down
    }
}

/// One `spark` answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Board {
    /// The newest price's time, as a tag that moves the chart's line on.
    pub tag: String,
    /// The index first, then the stocks in the order asked.
    pub rows: Vec<Quote>,
    /// Symbols asked for that Yahoo Finance has no prices for.
    pub missing: Vec<String>,
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().trim().to_string()
}

/// A time series' points: (Unix seconds, value), the gaps Yahoo leaves
/// (null) skipped.
fn points(r: &Value) -> Vec<(i64, f64)> {
    let ts = r["timestamp"].as_array().map(Vec::as_slice).unwrap_or_default();
    let close = r["indicators"]["quote"][0]["close"].as_array().map(Vec::as_slice).unwrap_or_default();
    ts.iter().zip(close).filter_map(|(t, c)| Some((t.as_i64()?, num(c)?))).collect()
}

impl Quote {
    /// One symbol's `response` (its `meta`, timestamps and closes).
    fn parse(r: &Value) -> Option<Quote> {
        let m = &r["meta"];
        let code = text(&m["symbol"]);
        let price = num(&m["regularMarketPrice"])?;
        if !is_code(&code) {
            return None;
        }
        let gmtoff = m["gmtoffset"].as_i64().unwrap_or(0);
        let reg = &m["currentTradingPeriod"]["regular"];
        let session = (reg["start"].as_i64().unwrap_or(0), reg["end"].as_i64().unwrap_or(0));
        let minute = |t: i64| ((t + gmtoff).rem_euclid(86_400) / 60) as u16;
        let span = Some((minute(session.0), minute(session.1))).filter(|(a, b)| b > a).unwrap_or((540, 1020));
        let day = (session.0 + gmtoff).div_euclid(86_400);
        let mut spark: Vec<(u16, f64)> = points(r).into_iter().filter(|(t, _)| (t + gmtoff).div_euclid(86_400) == day).map(|(t, c)| (minute(t), c)).collect();
        spark.sort_by_key(|p| p.0);
        let name = [&m["longName"], &m["shortName"]].into_iter().map(text).find(|n| !n.is_empty()).unwrap_or_else(|| code.clone());
        Some(Quote {
            name,
            price,
            prev: num(&m["chartPreviousClose"]).or_else(|| num(&m["previousClose"])).unwrap_or(price),
            high: num(&m["regularMarketDayHigh"]),
            low: num(&m["regularMarketDayLow"]),
            volume: num(&m["regularMarketVolume"]).filter(|v| *v > 0.0),
            w52: num(&m["fiftyTwoWeekLow"]).zip(num(&m["fiftyTwoWeekHigh"])),
            currency: text(&m["currency"]),
            exchange: text(&m["fullExchangeName"]),
            digits: m["priceHint"].as_u64().unwrap_or(2).min(6) as u8,
            at: m["regularMarketTime"].as_i64().unwrap_or(0),
            gmtoff,
            session,
            span,
            spark,
            code,
        })
    }
}

impl Board {
    /// A `spark` answer to `asked`; None for anything that isn't one.
    pub fn parse(json: &str, asked: &[String]) -> Option<Board> {
        let v: Value = serde_json::from_str(json).ok()?;
        let got: Vec<Quote> = v["spark"]["result"].as_array()?.iter().filter_map(|x| Quote::parse(&x["response"][0])).collect();
        if got.is_empty() {
            return None;
        }
        // In the order asked: Yahoo answers in its own.
        let rows: Vec<Quote> = asked.iter().filter_map(|c| got.iter().find(|q| q.code == *c).cloned()).collect();
        let missing = asked.iter().filter(|c| !rows.iter().any(|q| q.code == **c)).cloned().collect();
        let tag = rows.iter().map(|q| q.at).max().unwrap_or(0).to_string();
        Some(Board { tag, rows, missing })
    }

    pub fn quote(&self, code: &str) -> Option<&Quote> {
        self.rows.iter().find(|q| q.code == code)
    }

    /// Seconds until the prices are worth asking again: a minute while a
    /// market on the list trades or opens within half an hour, else 10.
    pub fn every_s(&self, now: i64) -> u64 {
        let soon = |q: &Quote| q.trading(now) || (0..=1800).contains(&(q.session.0 - now));
        if self.rows.iter().any(soon) {
            60
        } else {
            600
        }
    }
}

const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "24 Oct 2025": a date under the chart.
pub fn date_label(day: i64) -> String {
    let (y, m, d) = civil_from_days(day);
    format!("{d} {} {y}", MONTHS[(m - 1) as usize])
}

/// Minute `m` of the day as "14:05".
pub fn minute_label(m: u16) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Bursa Malaysia's biggest companies, built in so the search finds them at
/// once and offline: (symbol, Bursa's short name, the company's name), as
/// Yahoo Finance lists them (checked 14 Nov 2026). A few KB.
pub const BURSA: [(&str, &str, &str); 54] = [
    ("2488.KL", "ABMB", "Alliance Bank Malaysia Berhad"),
    ("1015.KL", "AMBANK", "AMMB Holdings Berhad"),
    ("6399.KL", "ASTRO", "Astro Malaysia Holdings Berhad"),
    ("6888.KL", "AXIATA", "Axiata Group Berhad"),
    ("5258.KL", "BIMB", "Bank Islam Malaysia Berhad"),
    ("1818.KL", "BURSA", "Bursa Malaysia Berhad"),
    ("5099.KL", "CAPITALA", "Capital A Berhad"),
    ("6947.KL", "CDB", "Celcomdigi Berhad"),
    ("1023.KL", "CIMB", "CIMB Group Holdings Berhad"),
    ("7277.KL", "DIALOG", "Dialog Group Berhad"),
    ("3689.KL", "F&N", "Fraser & Neave Holdings Bhd"),
    ("0128.KL", "FRONTKN", "Frontken Corporation Berhad"),
    ("5398.KL", "GAMUDA", "Gamuda Berhad"),
    ("4715.KL", "GENM", "Genting Malaysia Berhad"),
    ("3182.KL", "GENTING", "Genting Berhad"),
    ("3034.KL", "HAPSENG", "Hap Seng Consolidated Berhad"),
    ("5168.KL", "HARTA", "Hartalega Holdings Berhad"),
    ("5819.KL", "HLBANK", "Hong Leong Bank Berhad"),
    ("1082.KL", "HLFG", "Hong Leong Financial Group Berhad"),
    ("5225.KL", "IHH", "IHH Healthcare Berhad"),
    ("3336.KL", "IJM", "IJM Corporation Berhad"),
    ("0166.KL", "INARI", "Inari Amertron Berhad"),
    ("1961.KL", "IOICORP", "IOI Corporation Berhad"),
    ("5235SS.KL", "KLCC", "KLCC Property Holdings Berhad"),
    ("2445.KL", "KLK", "Kuala Lumpur Kepong Berhad"),
    ("6012.KL", "MAXIS", "Maxis Berhad"),
    ("1155.KL", "MAYBANK", "Malayan Banking Berhad"),
    ("1171.KL", "MBSB", "MBSB Berhad"),
    ("3816.KL", "MISC", "MISC Berhad"),
    ("5296.KL", "MRDIY", "Mr D.I.Y. Group (M) Berhad"),
    ("4707.KL", "NESTLE", "Nestlé (Malaysia) Berhad"),
    ("1295.KL", "PBBANK", "Public Bank Berhad"),
    ("5183.KL", "PCHEM", "PETRONAS Chemicals Group Berhad"),
    ("5681.KL", "PETDAG", "PETRONAS Dagangan Berhad"),
    ("6033.KL", "PETGAS", "PETRONAS Gas Berhad"),
    ("8869.KL", "PMETAL", "Press Metal Aluminium Holdings Berhad"),
    ("4065.KL", "PPB", "PPB Group Berhad"),
    ("7084.KL", "QL", "QL Resources Berhad"),
    ("1066.KL", "RHBBANK", "RHB Bank Berhad"),
    ("5285.KL", "SDG", "SD Guthrie Berhad"),
    ("4197.KL", "SIME", "Sime Darby Berhad"),
    ("5263.KL", "SUNCON", "Sunway Construction Group Berhad"),
    ("5211.KL", "SUNWAY", "Sunway Berhad"),
    ("7106.KL", "SUPERMX", "Supermax Corporation Berhad"),
    ("5347.KL", "TENAGA", "Tenaga Nasional Berhad"),
    ("5031.KL", "TIMECOM", "TIME dotCom Berhad"),
    ("4863.KL", "TM", "Telekom Malaysia Berhad"),
    ("7113.KL", "TOPGLOV", "Top Glove Corporation Berhad"),
    ("2089.KL", "UTDPLT", "United Plantations Berhad"),
    ("0097.KL", "VITROX", "ViTrox Corporation Berhad"),
    ("5246.KL", "WPRTS", "Westports Holdings Berhad"),
    ("6742.KL", "YTLPOWR", "YTL Power International Berhad"),
    ("4677.KL", "YTL", "YTL Corporation Berhad"),
    ("0138.KL", "ZETRIX", "Zetrix AI Berhad"),
];

/// Stocks to search through: (symbol, name), Bursa's built in first, then
/// any the online search found this run.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Names {
    pub list: Vec<(String, String)>,
}

/// Bursa's built-in list as [`Names`].
pub fn bursa() -> Names {
    Names { list: BURSA.iter().map(|(c, short, name)| (c.to_string(), format!("{name} \u{b7} {}", &c[..c.len() - 3]))).collect() }
}

impl Names {
    pub fn name(&self, code: &str) -> Option<&str> {
        self.list.iter().find(|(c, _)| c == code).map(|(_, n)| n.as_str())
    }

    /// Adds what an online search found, each once.
    pub fn learn(&mut self, found: &[(String, String)]) {
        for f in found {
            if !self.list.iter().any(|(c, _)| *c == f.0) {
                self.list.push(f.clone());
            }
        }
    }

    /// Up to `n` stocks for what was typed, best first: the code itself,
    /// codes that start with it, names that start with it, a word of a name
    /// that does, then codes and names that hold it anywhere. Stocks in
    /// `skip` (already on the list) are left out.
    pub fn search(&self, query: &str, skip: &[String], n: usize) -> Vec<(String, String)> {
        let q = fold(query);
        if q.is_empty() {
            return Vec::new();
        }
        let upper = q.to_ascii_uppercase();
        let mut hits: Vec<(u8, usize)> = self
            .list
            .iter()
            .enumerate()
            .filter(|(_, (c, _))| !skip.contains(c))
            .filter_map(|(i, (code, name))| {
                let name = fold(name);
                // The symbol ("1155 KL") and, for Bursa, its short name (MAYBANK).
                let tick = fold(ticker(code)).to_ascii_uppercase();
                let code = fold(code).to_ascii_uppercase();
                let rank = if code == upper || code.split(' ').next() == Some(upper.as_str()) || tick == upper {
                    0
                } else if code.starts_with(&upper) || tick.starts_with(&upper) {
                    1
                } else if name.starts_with(&q) {
                    2
                } else if name.split(' ').any(|w| w.starts_with(&q)) {
                    3
                } else if code.contains(&upper) {
                    4
                } else if name.contains(&q) {
                    5
                } else {
                    return None;
                };
                Some((rank, i))
            })
            .collect();
        hits.sort();
        hits.into_iter().take(n).map(|(_, i)| self.list[i].clone()).collect()
    }
}

/// What an online search found: (symbol, "name · exchange"), stocks,
/// funds, indices and currencies only (no futures or options), in Yahoo
/// Finance's order. None for anything that isn't an answer.
pub fn parse_search(json: &str) -> Option<Vec<(String, String)>> {
    let v: Value = serde_json::from_str(json).ok()?;
    let found = v["quotes"]
        .as_array()?
        .iter()
        .filter(|q| matches!(q["quoteType"].as_str(), Some("EQUITY" | "ETF" | "INDEX" | "MUTUALFUND" | "CURRENCY" | "CRYPTOCURRENCY")))
        .filter_map(|q| {
            let code = text(&q["symbol"]).to_ascii_uppercase();
            let name = [&q["longname"], &q["shortname"]].into_iter().map(text).find(|n| !n.is_empty()).unwrap_or_else(|| code.clone());
            let exch = text(&q["exchDisp"]);
            is_code(&code).then(|| (code, if exch.is_empty() { name } else { format!("{name} \u{b7} {exch}") }))
        })
        .collect();
    Some(found)
}

/// Lower case, letters and digits, words split by single spaces.
fn fold(s: &str) -> String {
    let s: String = s.chars().map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { ' ' }).collect();
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A year of one stock's (or the index's) daily closes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct History {
    pub code: String,
    /// Trading days (days since 1970-01-01), oldest first, each with its close.
    pub days: Vec<i64>,
    pub closes: Vec<f64>,
}

impl History {
    /// A `chart` answer (a year of daily closes).
    pub fn parse(json: &str) -> Option<History> {
        let v: Value = serde_json::from_str(json).ok()?;
        let r = &v["chart"]["result"][0];
        let code = text(&r["meta"]["symbol"]);
        let off = r["meta"]["gmtoffset"].as_i64().unwrap_or(0);
        let mut pts: Vec<(i64, f64)> = points(r).into_iter().map(|(t, c)| ((t + off).div_euclid(86_400), c)).collect();
        pts.sort_by_key(|p| p.0);
        pts.dedup_by_key(|p| p.0);
        (!code.is_empty() && pts.len() >= 2).then(|| History { code, days: pts.iter().map(|p| p.0).collect(), closes: pts.iter().map(|p| p.1).collect() })
    }
    /// The closes `range` covers: back to the same date `range` months
    /// before the last one. Empty for [`Range::Day`].
    pub fn window(&self, range: Range) -> (&[i64], &[f64]) {
        let (Some(months), Some(&last)) = (range.months(), self.days.last()) else { return (&[], &[]) };
        let (y, m, d) = civil_from_days(last);
        let back = y as i64 * 12 + (m as i64 - 1) - months;
        let from = days_from_civil(back.div_euclid(12) as i32, (back.rem_euclid(12) + 1) as u8, d.min(28));
        let i = self.days.partition_point(|&x| x < from);
        (&self.days[i..], &self.closes[i..])
    }
}

// ---------------------------------------------------------------- numbers

/// `n` with commas between thousands: 1234567 → "1,234,567".
fn group(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// A price to `digits` places, thousands grouped: "9.94", "1,582.59", "0.045".
pub fn fmt_price(p: f64, digits: u8) -> String {
    if !p.is_finite() {
        return "\u{2014}".to_string();
    }
    let neg = if p < 0.0 { MINUS.to_string() } else { String::new() };
    let scale = 10f64.powi(i32::from(digits));
    let units = (p.abs() * scale).round();
    let whole = (units / scale).trunc();
    let frac = units - whole * scale;
    if digits == 0 {
        format!("{neg}{}", group(whole as u64))
    } else {
        format!("{neg}{}.{:0w$}", group(whole as u64), frac as u64, w = digits as usize)
    }
}

fn sign(d: f64, zero: f64) -> String {
    if d.abs() < zero {
        String::new()
    } else if d > 0.0 {
        "+".to_string()
    } else {
        MINUS.to_string()
    }
}

/// A change in price: "+0.03", "−12.60".
pub fn fmt_change(d: f64, digits: u8) -> String {
    let zero = 0.5 / 10f64.powi(i32::from(digits));
    format!("{}{}", sign(d, zero), fmt_price(d.abs(), digits))
}

/// A change in percent: "+0.82%", "−1.21%", "0.00%".
pub fn fmt_pct(pct: f64) -> String {
    format!("{}{:.2}%", sign(pct, 0.005), pct.abs())
}

/// A large number the short way: "607T", "26.7T", "850B", "94.1M", "8.2K".
/// T, B, M and K are trillion, billion, million and thousand.
pub fn fmt_big(v: f64) -> String {
    if !v.is_finite() {
        return "\u{2014}".to_string();
    }
    let a = v.abs();
    let (x, unit) = match a {
        a if a >= 1e12 => (a / 1e12, "T"),
        a if a >= 1e9 => (a / 1e9, "B"),
        a if a >= 1e6 => (a / 1e6, "M"),
        a if a >= 1e3 => (a / 1e3, "K"),
        a => (a, ""),
    };
    let x = if x >= 100.0 { format!("{x:.0}") } else if x >= 10.0 { format!("{x:.1}") } else { format!("{x:.2}") };
    // "26.0T" reads better as "26T".
    let x = if x.contains('.') { x.trim_end_matches('0').trim_end_matches('.').to_string() } else { x };
    format!("{}{x}{unit}", if v < 0.0 { MINUS.to_string() } else { String::new() })
}


// ---------------------------------------------------------------- pacing

/// Seconds until the prices are asked again: what the last answer said
/// (`every_s`, [`Board::every_s`]), within a minute and 10 minutes; after
/// `failures` failed asks in a row, a minute doubled each time, up to 10.
pub fn next_ask(every_s: Option<u64>, failures: u32) -> u64 {
    if failures > 0 {
        return (60u64 << (failures - 1).min(4)).min(600);
    }
    every_s.unwrap_or(60).clamp(60, 600)
}

fn enc(code: &str) -> String {
    crate::urlenc::encode(code)
}

pub fn watch_url(base: &str, codes: &[String]) -> String {
    format!("{base}v7/finance/spark?symbols={}&range=1d&interval=5m", codes.iter().map(|c| enc(c)).collect::<Vec<_>>().join(","))
}

pub fn search_url(base: &str, query: &str) -> String {
    format!("{base}v1/finance/search?q={}&quotesCount=8&newsCount=0&listsCount=0", enc(query.trim()))
}

pub fn chart_url(base: &str, code: &str) -> String {
    format!("{base}v8/finance/chart/{}?range=1y&interval=1d", enc(code))
}

// ---------------------------------------------------------------- lines

/// The values a line is drawn between: its lowest and highest, the previous
/// close (`base`) included, with room above and below so the line never
/// touches the edges. A flat line gets a band around it.
pub fn bounds(values: impl IntoIterator<Item = f64>, base: Option<f64>) -> Option<(f64, f64)> {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in values.into_iter().chain(base).filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi {
        return None;
    }
    let pad = ((hi - lo) * 0.08).max(hi.abs() * 0.0015).max(1e-6);
    Some((lo - pad, hi + pad))
}

/// Where minute `m` of the exchange's day sits across the day's sessions, 0..1.
pub fn day_x(m: u16, span: (u16, u16)) -> f32 {
    let (a, b) = (f32::from(span.0), f32::from(span.1));
    ((f32::from(m) - a) / (b - a).max(1.0)).clamp(0.0, 1.0)
}

/// `n` points along the line through `pts` (sorted by x), evenly spaced from
/// its first x to its last: the same number for every line, so the page can
/// morph one into the next point for point.
pub fn resample(pts: &[(f32, f32)], n: usize) -> Vec<(f32, f32)> {
    match pts {
        [] => Vec::new(),
        [p] => vec![*p; n],
        _ => {
            let (x0, x1) = (pts[0].0, pts[pts.len() - 1].0);
            let mut j = 0;
            (0..n)
                .map(|i| {
                    let x = x0 + (x1 - x0) * i as f32 / (n - 1).max(1) as f32;
                    while j + 2 < pts.len() && pts[j + 1].0 < x {
                        j += 1;
                    }
                    let (a, b) = (pts[j], pts[j + 1]);
                    let t = if b.0 > a.0 { ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 1.0 };
                    (x, a.1 + (b.1 - a.1) * t)
                })
                .collect()
        }
    }
}

// ---------------------------------------------------------------- samples

/// A made-up answer for the self-test's and the Mac renders' pictures: the
/// KLCI and the starter stocks at minute `now_min` of a trading day on
/// Bursa (`day`: days since 1970), each with the day's line so far. The
/// prices are invented, near the levels of November 2026.
pub fn sample(day: i64, now_min: u16) -> Board {
    const MYT: i64 = 8 * 3600;
    let now_min = now_min.clamp(545, 1020);
    let rows: [(&str, &str, f64, f64, f64); 6] = [
        // code, name, previous close, close now, volume
        (INDEX, "FTSE Bursa Malaysia KLCI", 1_595.19, 1_604.37, 0.0),
        ("1155.KL", "Malayan Banking Berhad", 9.91, 9.94, 18_146_700.0),
        ("1295.KL", "Public Bank Berhad", 4.50, 4.54, 31_420_100.0),
        ("1023.KL", "CIMB Group Holdings Berhad", 7.12, 7.08, 12_903_400.0),
        ("5347.KL", "Tenaga Nasional Berhad", 12.70, 12.76, 6_120_800.0),
        ("5225.KL", "IHH Healthcare Berhad", 8.01, 7.93, 4_305_200.0),
    ];
    let start = day * 86_400 + 9 * 3600 - MYT;
    let rows = rows
        .iter()
        .enumerate()
        .map(|(i, &(code, name, prev, last, volume))| {
            let index = code == INDEX;
            // From the previous close to `last`, wandering as a day's line
            // does: each step keeps most of the last one's lean (seeded by the row).
            let mut seed = 0x9E37_79B9u32.wrapping_mul(i as u32 + 7);
            let mut spark = Vec::new();
            let steps = (now_min - 540) / 5;
            let swing = (last - prev).abs().max(prev * 0.004);
            let mut lean = 0.0;
            for k in 0..=steps {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                lean = lean * 0.82 + (f64::from(seed % 1000) / 1000.0 - 0.5) * swing * 0.35;
                let f = f64::from(k) / f64::from(steps.max(1));
                let p = prev + (last - prev) * f.powf(0.8) + if k == steps { 0.0 } else { lean * (1.0 - f * 0.5) };
                let m = if k == steps { now_min } else { 540 + k * 5 };
                spark.push((m, if k == steps { last } else { (p / 0.01).round() * 0.01 }));
            }
            let (lo, hi) = spark.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| (lo.min(p.1), hi.max(p.1)));
            Quote {
                code: code.to_string(),
                name: name.to_string(),
                price: last,
                prev,
                high: Some(hi),
                low: Some(lo),
                volume: (!index).then_some(volume),
                w52: Some(if index { (1_386.62, 1_655.08) } else { (prev * 0.82, prev * 1.21) }),
                currency: "MYR".into(),
                exchange: "Kuala Lumpur".into(),
                digits: 2,
                at: start + i64::from(now_min - 540) * 60,
                gmtoff: MYT,
                session: (start, start + 8 * 3600),
                span: (540, 1020),
                spark,
            }
        })
        .collect();
    Board { tag: "sample".into(), rows, missing: Vec::new() }
}

/// A made-up year of daily closes for `code`, ending at `last` on day `today`.
pub fn sample_history(code: &str, today: i64, last: f64) -> History {
    let mut seed = code.bytes().fold(0x2545_F491u32, |h, b| (h ^ u32::from(b)).wrapping_mul(0x0100_0193));
    let (mut days, mut closes) = (Vec::new(), Vec::new());
    let mut d = today - 365;
    let mut p = last * 0.86;
    while d <= today {
        if !matches!(weekday_from_days(d), 0 | 6) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            p *= 1.0 + (f64::from(seed % 1000) / 1000.0 - 0.47) * 0.03;
            days.push(d);
            closes.push(p);
        }
        d += 1;
    }
    // Ends on today's close.
    let k = last / closes.last().copied().unwrap_or(last);
    let n = closes.len().max(1) as f64;
    for (i, c) in closes.iter_mut().enumerate() {
        *c *= 1.0 + (k - 1.0) * (i as f64 / n);
    }
    if let Some(c) = closes.last_mut() {
        *c = last;
    }
    History { code: code.to_string(), days, closes }
}


#[cfg(test)]
mod tests {
    use super::*;

    const SPARK: &str = include_str!("../tests/fixtures/stocks/spark.json");
    const CHART: &str = include_str!("../tests/fixtures/stocks/chart-1y.json");
    const SEARCH: &str = include_str!("../tests/fixtures/stocks/search.json");

    fn asked() -> Vec<String> {
        [INDEX, "1155.KL", "AAPL", "ZZZZQ.KL"].map(String::from).to_vec()
    }

    #[test]
    fn reads_a_spark_answer_in_the_order_asked() {
        let b = Board::parse(SPARK, &asked()).expect("an answer");
        assert_eq!(b.rows.iter().map(|q| q.code.as_str()).collect::<Vec<_>>(), [INDEX, "1155.KL", "AAPL"], "Yahoo's order is its own");
        assert_eq!(b.missing, ["ZZZZQ.KL"]);
        let klci = &b.rows[0];
        assert!(klci.is_index());
        assert_eq!(ticker(&klci.code), "KLCI");
        let mbb = b.quote("1155.KL").expect("Maybank");
        assert_eq!((mbb.name.as_str(), mbb.currency.as_str(), mbb.digits), ("Malayan Banking Berhad", "MYR", 2));
        assert_eq!((mbb.price, mbb.prev), (9.94, 9.91));
        assert_eq!(mbb.w52, Some((9.78, 12.42)));
        assert_eq!(mbb.span, (540, 1020), "Bursa: 09:00 to 17:00");
        assert!(mbb.spark.len() > 10 && mbb.spark.windows(2).all(|w| w[0].0 < w[1].0), "a point every 5 minutes, in order");
        assert_eq!(mbb.trend(), Trend::Up);
        assert_eq!(mbb.pill(Show::Percent), "+0.30%");
        assert_eq!(mbb.pill(Show::Points), "+0.03");
        assert_eq!(ticker("1155.KL"), "MAYBANK");
        assert_eq!(b.quote("AAPL").map(|q| q.span), Some((570, 960)), "New York's session on its own clock");
        assert_eq!(Board::parse("{}", &asked()), None);
        assert_eq!(Board::parse(r#"{"spark":{"result":[]}}"#, &asked()), None);
        assert_eq!(Board::parse("<html>", &asked()), None);
    }

    #[test]
    fn status_says_where_its_market_stands() {
        let b = Board::parse(SPARK, &asked()).expect("an answer");
        let q = b.quote("1155.KL").expect("Maybank").clone();
        assert_eq!(q.status(q.session.0 + 3600), format!("Open \u{b7} as of {}", q.clock(q.at)));
        assert_eq!(q.status(q.session.1 + 60), "Closed \u{b7} 16:59");
        // The next morning before the bell: yesterday's prices, and the
        // session the new answer names.
        let next = q.session.0 + 86_400 - 1800;
        let day = WEEKDAYS[weekday_from_days(q.day()) as usize];
        assert_eq!(q.status(next), format!("Closed \u{b7} prices from {day}"), "an old answer knows no next session");
        let morning = Quote { session: (q.session.0 + 86_400, q.session.1 + 86_400), ..q.clone() };
        assert_eq!(morning.status(next), format!("Opens at 09:00 \u{b7} prices from {day}"));
        assert_eq!(q.status(q.session.0 + 12 * 86_400), format!("Closed \u{b7} prices from {}", date_label(q.day())), "after a long holiday");
        assert_eq!(b.every_s(q.session.0 + 3600), 60, "a minute while Bursa trades");
        assert_eq!(b.every_s(q.session.1 + 6 * 3600), 60, "New York trades then");
        assert_eq!(b.every_s(q.session.1 + 14 * 3600), 600, "10 minutes while every market is shut");
    }

    #[test]
    fn numbers_read_as_the_exchange_writes_them() {
        assert_eq!(fmt_price(9.94, 2), "9.94");
        assert_eq!(fmt_price(1582.594, 2), "1,582.59");
        assert_eq!(fmt_price(0.045, 3), "0.045");
        assert_eq!(fmt_price(0.045, 4), "0.0450");
        assert_eq!(fmt_price(179_975.0, 0), "179,975");
        assert_eq!(fmt_price(1.999, 2), "2.00");
        assert_eq!(fmt_change(-12.6, 2), "\u{2212}12.60");
        assert_eq!(fmt_change(0.03, 2), "+0.03");
        assert_eq!(fmt_change(0.001, 2), "0.00");
        assert_eq!(fmt_pct(0.8163), "+0.82%");
        assert_eq!(fmt_pct(-1.2149), "\u{2212}1.21%");
        assert_eq!(fmt_big(353_641_200.0), "354M");
        assert_eq!(fmt_big(8_200.0), "8.2K");
        assert_eq!(group(1_000), "1,000");
    }

    #[test]
    fn symbols_from_any_exchange() {
        for ok in ["1155.KL", "AAPL", "^KLSE", "BRK-B", "F&N.KL", "7203.T", "EURUSD=X", "5235SS.KL"] {
            assert!(is_code(ok), "{ok}");
        }
        for bad in ["", "aapl", "AA PL", "^", "A".repeat(21).as_str()] {
            assert!(!is_code(bad), "{bad}");
        }
        assert_eq!(code_of(" aapl "), Some("AAPL".into()));
        assert_eq!(code_of("1155"), Some("1155.KL".into()), "a Bursa number on its own");
        assert_eq!(code_of("not a code"), None);
        assert!(watch_url(BASE, &[INDEX.into(), "F&N.KL".into()]).ends_with("spark?symbols=%5EKLSE,F%26N.KL&range=1d&interval=5m"));
        assert_eq!(chart_url(BASE, "1155.KL"), "https://query1.finance.yahoo.com/v8/finance/chart/1155.KL?range=1y&interval=1d");
        assert!(search_url(BASE, " toyota motor ").contains("q=toyota%20motor&"));
    }

    #[test]
    fn the_list_holds_ten_stocks_each_once() {
        let mut w = Watchlist::default();
        assert_eq!(w.codes, STARTERS.map(String::from).to_vec());
        assert_eq!(w.pick, INDEX);
        assert!(w.add(" aapl "), "typed in lower case with spaces");
        assert_eq!(w.pick, "AAPL", "a new stock shows");
        assert!(!w.add("AAPL"), "already there");
        assert!(!w.add("A A") && !w.add(""), "not symbols");
        for c in ["7203.T", "0166.KL", "6888.KL", "BBCA.JK"] {
            assert!(w.add(c));
        }
        assert!(w.full());
        assert!(!w.add("MSFT"), "ten is the most");
        assert_eq!(w.symbols().next(), Some(INDEX));
        assert_eq!(w.symbols().count(), 11);
        w.move_to("BBCA.JK", 0);
        assert_eq!(w.codes[0], "BBCA.JK");
        w.pick = "AAPL".into();
        assert!(w.remove("AAPL"));
        assert_eq!(w.pick, INDEX, "the index shows once the stock showing is gone");
        assert!(!w.full());
    }

    #[test]
    fn a_list_from_the_idx_days_moves_over() {
        // The starters as they were: Bursa's instead.
        let mut w: Watchlist = serde_json::from_str(r#"{"codes":["BBCA","BBRI","BMRI","TLKM","ASII"],"pick":"IHSG","show":"cap","range":"6m"}"#).expect("a list");
        w.tidy();
        assert_eq!((w.codes.clone(), w.pick.as_str(), w.show, w.range, w.feed), (STARTERS.map(String::from).to_vec(), INDEX, Show::Percent, Range::HalfYear, 1));
        // A list of the person's own: the same stocks, on Yahoo's Jakarta symbols.
        let mut w: Watchlist = serde_json::from_str(r#"{"codes":["bbca","GOTO","x y"],"pick":"GOTO"}"#).expect("a list");
        w.tidy();
        assert_eq!((w.codes.clone(), w.pick.as_str()), (vec!["BBCA.JK".to_string(), "GOTO.JK".to_string()], "GOTO.JK"));
        // Once moved, never again: a US ticker of four letters stays as it is.
        let mut w: Watchlist = serde_json::from_str(r#"{"codes":["AAPL","1155.KL"],"pick":"MSFT","feed":1}"#).expect("a list");
        w.tidy();
        assert_eq!((w.codes.clone(), w.pick.as_str()), (vec!["AAPL".to_string(), "1155.KL".to_string()], INDEX));
        let w: Watchlist = serde_json::from_str("{}").expect("an empty object");
        assert_eq!(w.feed, 0, "an empty file is from before");
        assert_eq!(Show::Cap.next(), Show::Percent);
    }

    #[test]
    fn search_finds_bursa_first_and_anything_online() {
        let mut n = bursa();
        let codes = |v: Vec<(String, String)>| v.into_iter().map(|p| p.0).collect::<Vec<_>>();
        assert_eq!(codes(n.search("maybank", &[], 5)), vec!["1155.KL"], "Bursa's short name");
        assert_eq!(codes(n.search("1155", &[], 5)), vec!["1155.KL"], "its number");
        assert_eq!(codes(n.search("petronas", &[], 9)), vec!["5183.KL", "5681.KL", "6033.KL"]);
        assert_eq!(codes(n.search("public bank", &[], 9)), vec!["1295.KL"]);
        assert!(n.search("maybank", &["1155.KL".to_string()], 5).is_empty(), "already on the list");
        assert!(n.search("  ", &[], 5).is_empty());
        assert_eq!(n.name("1155.KL"), Some("Malayan Banking Berhad \u{b7} 1155"));
        // Anything worldwide, from Yahoo's own search: no futures.
        let found = parse_search(SEARCH).expect("an answer");
        assert_eq!(found[0], ("TM".to_string(), "Toyota Motor Corporation \u{b7} NYSE".to_string()));
        assert!(found.iter().any(|f| f.0 == "7203.T"));
        assert!(found.iter().all(|f| !f.0.ends_with("=F")));
        n.learn(&found);
        n.learn(&found);
        assert_eq!(codes(n.search("toyota", &[], 9)).iter().filter(|c| *c == "7203.T").count(), 1, "learnt once");
        assert_eq!(parse_search("<html>"), None);
        assert_eq!(BURSA.iter().filter(|b| BURSA.iter().filter(|c| c.0 == b.0).count() > 1).count(), 0, "each once");
        assert!(STARTERS.iter().all(|s| BURSA.iter().any(|b| b.0 == *s)));
    }

    #[test]
    fn a_year_of_closes_is_cut_to_the_range() {
        let h = History::parse(CHART).expect("history");
        assert_eq!(h.code, "1155.KL");
        assert!(h.days.len() > 200 && h.days.windows(2).all(|w| w[0] < w[1]));
        assert!(h.window(Range::Month).1.len() < h.window(Range::HalfYear).1.len());
        assert!(h.window(Range::Day).0.is_empty());
        assert_eq!(History::parse(r#"{"chart":{"result":null,"error":{"code":"Not Found"}}}"#), None);
        let s = sample_history("1155.KL", days_from_civil(2026, 11, 13), 9.94);
        assert_eq!(s.closes.last(), Some(&9.94));
        assert!(s.days.len() > 250 && s.days.iter().all(|d| !matches!(weekday_from_days(*d), 0 | 6)));
    }

    #[test]
    fn asks_no_more_than_once_a_minute() {
        assert_eq!(next_ask(Some(60), 0), 60);
        assert_eq!(next_ask(Some(5), 0), 60, "never more than once a minute");
        assert_eq!(next_ask(Some(86_400), 0), 600);
        assert_eq!(next_ask(None, 0), 60);
        assert_eq!(next_ask(Some(60), 3), 240);
        assert_eq!(next_ask(Some(60), 30), 600);
    }

    #[test]
    fn lines_fit_their_box() {
        let (lo, hi) = bounds([10.0, 12.0], Some(9.0)).expect("bounds");
        assert!(lo < 9.0 && hi > 12.0, "the previous close is inside");
        assert_eq!(bounds([f64::NAN], None), None);
        assert_eq!(day_x(540, (540, 960)), 0.0);
        assert_eq!(day_x(750, (540, 960)), 0.5);
        let r = resample(&[(0.0, 0.0), (1.0, 10.0), (3.0, 10.0)], 4);
        assert_eq!(r, vec![(0.0, 0.0), (1.0, 10.0), (2.0, 10.0), (3.0, 10.0)]);
        assert!(resample(&[], 3).is_empty());
    }

    #[test]
    fn the_sample_is_a_trading_day() {
        let day = days_from_civil(2026, 11, 13);
        let b = sample(day, 13 * 60 + 40);
        assert_eq!(b.rows.len(), 6);
        assert_eq!(b.rows[0].code, INDEX);
        for q in &b.rows {
            assert_eq!(q.spark.last().map(|p| p.1), Some(q.price), "{}: the line ends at the price", q.code);
            assert_eq!(q.spark.first().map(|p| p.0), Some(540));
            assert!(q.spark.windows(2).all(|w| w[0].0 < w[1].0), "{}: minutes in order {:?}", q.code, &q.spark[..4]);
            assert_eq!(q.day(), day);
            assert!(q.trading(q.at));
        }
    }
}

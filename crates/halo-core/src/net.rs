//! Why a request brought no usable answer, in words people can act on, and
//! when to ask again. Shared by every online part (Weather, Stocks, News,
//! add-on downloads) so they all tell the same truth:
//!
//! * no internet (the system says so) is not the server failing: the first
//!   is `Offline`, the second `Unreachable`, `Timeout` or `Status`;
//! * an answer that isn't what was asked for is `Malformed`, never a value;
//! * an address that can't exist (a `.invalid` placeholder, RFC 2606) is
//!   `NotConfigured`: asking it would only look like an outage.
//!
//! Pure: the apps classify their HTTP client's errors with [`no_answer`] and
//! [`answered`], and ask the system whether it is online.

/// Why a request brought nothing to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// The system says this computer has no internet connection.
    Offline,
    /// Online (or unknown), but nothing came back in time.
    Timeout,
    /// Online (or unknown), but the server couldn't be reached: its name
    /// didn't resolve, the connection or its TLS failed.
    Unreachable,
    /// The server answered with this error status.
    Status(u16),
    /// The server answered, with something that isn't the answer.
    Malformed,
    /// This build has no real address for it.
    NotConfigured,
}

impl Failure {
    /// A word or two for a small tile.
    pub fn short(&self) -> String {
        match self {
            Failure::Offline => "Offline".into(),
            Failure::Timeout => "Timed out".into(),
            Failure::Unreachable => "Can't reach it".into(),
            Failure::Status(429) => "Busy, retrying".into(),
            Failure::Status(s) => format!("Error {s}"),
            Failure::Malformed => "Bad answer".into(),
            Failure::NotConfigured => "Not set up".into(),
        }
    }

    /// One sentence, naming the service (`what`: "Open-Meteo").
    pub fn line(&self, what: &str) -> String {
        match self {
            Failure::Offline => "No internet connection. It updates once you're back online.".into(),
            Failure::Timeout => format!("{what} didn't answer in time. Trying again soon."),
            Failure::Unreachable => format!("Can't reach {what}, though you're online. Trying again soon."),
            Failure::Status(429) => format!("{what} is busy. Trying again in a while."),
            Failure::Status(s) if *s >= 500 => format!("{what} has a problem (error {s}). Trying again soon."),
            Failure::Status(s) => format!("{what} refused the request (error {s})."),
            Failure::Malformed => format!("{what} sent an answer Halo Island can't read."),
            Failure::NotConfigured => format!("{what} isn't set up in this version."),
        }
    }

    /// Whether asking again later may help: not for a refusal, a bad answer
    /// or a missing address, which stay until something changes.
    pub fn retries(&self) -> bool {
        match self {
            Failure::Offline | Failure::Timeout | Failure::Unreachable => true,
            Failure::Status(s) => *s >= 500 || *s == 429 || *s == 408,
            Failure::Malformed | Failure::NotConfigured => false,
        }
    }
}

/// No response at all: `timed_out`, or a network error. `online` is what the
/// system says (None: it couldn't tell, so the server gets the blame only
/// as far as the error shows).
pub fn no_answer(timed_out: bool, online: Option<bool>) -> Failure {
    match (online, timed_out) {
        (Some(false), _) => Failure::Offline,
        (_, true) => Failure::Timeout,
        _ => Failure::Unreachable,
    }
}

/// A response with `status`: Ok for 2xx (the caller reads the body), else
/// the failure it is.
pub fn answered(status: u16) -> Result<(), Failure> {
    if (200..300).contains(&status) {
        Ok(())
    } else {
        Err(Failure::Status(status))
    }
}

/// Whether `url` can be asked at all: an http(s) address whose host isn't
/// a reserved name that never resolves (`.invalid`, `.example`, `.test`).
pub fn configured(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else { return false };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").split(':').next().unwrap_or("").trim_end_matches('.').to_ascii_lowercase();
    !host.is_empty() && !["invalid", "example", "test"].iter().any(|tld| host == *tld || host.ends_with(&format!(".{tld}")))
}

/// Seconds before the next try after `fails` failures in a row: `first`,
/// doubling, at most `last`. A server's own `Retry-After` wins within those
/// bounds.
pub fn backoff_s(fails: u32, first: u64, last: u64, retry_after_s: Option<u64>) -> u64 {
    let doubled = first.saturating_mul(1u64 << fails.saturating_sub(1).min(20)).min(last);
    retry_after_s.map_or(doubled, |s| s.clamp(doubled.min(last), last))
}

/// One online reading's whole story, for a part that asks while its page is
/// open (Weather, News): the last good answer (kept, with its age, when a
/// later ask fails), whether an ask is out, why the last one failed, and
/// when it may ask again. Never a made-up value.
#[derive(Clone, Debug, PartialEq)]
pub struct Fetched<T> {
    /// The last good answer and when (unix) it came.
    pub last: Option<(T, i64)>,
    /// An ask is out.
    pub asking: bool,
    /// Why the last ask failed (cleared by a good one).
    pub trouble: Option<Failure>,
    /// Failures in a row, for the backoff.
    pub fails: u32,
    /// Not before this (unix) may it ask again after a failure.
    pub retry_at: Option<i64>,
}

impl<T> Default for Fetched<T> {
    fn default() -> Self {
        Fetched { last: None, asking: false, trouble: None, fails: 0, retry_at: None }
    }
}

impl<T> Fetched<T> {
    /// Whether to ask now: no ask out, the answer missing or `fresh_s` old,
    /// and any backoff over.
    pub fn due(&self, now: i64, fresh_s: i64) -> bool {
        !self.asking && self.retry_at.is_none_or(|t| now >= t) && self.last.as_ref().is_none_or(|(_, at)| now - at >= fresh_s)
    }

    pub fn asked(&mut self) {
        self.asking = true;
    }

    /// The answer, or why there is none. A failure waits `first_s` doubling
    /// to `last_s` before the next ask; one that won't mend by itself
    /// (a refusal, a bad answer) waits `last_s`.
    pub fn came(&mut self, got: Result<T, Failure>, now: i64, first_s: u64, last_s: u64) {
        self.asking = false;
        match got {
            Ok(v) => *self = Fetched { last: Some((v, now)), ..Fetched::default() },
            Err(f) => {
                self.fails = self.fails.saturating_add(1);
                let wait = if f.retries() { backoff_s(self.fails, first_s, last_s, None) } else { last_s };
                self.retry_at = Some(now + wait as i64);
                self.trouble = Some(f);
            }
        }
    }

    /// When (unix) an open page should look again: the retry after a
    /// failure, else when the answer goes stale.
    pub fn next_ask(&self, fresh_s: i64) -> Option<i64> {
        self.retry_at.or_else(|| self.last.as_ref().map(|(_, at)| at + fresh_s))
    }
}

/// "just now", "12 min ago", "3 h ago", "2 d ago": how old something is.
pub fn age(now: i64, at: i64) -> String {
    match (now - at).max(0) / 60 {
        0 => "just now".to_string(),
        m if m < 60 => format!("{m} min ago"),
        m if m < 48 * 60 => format!("{} h ago", m / 60),
        m => format!("{} d ago", m / (24 * 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_internet_is_not_the_servers_fault() {
        assert_eq!(no_answer(true, Some(false)), Failure::Offline);
        assert_eq!(no_answer(false, Some(false)), Failure::Offline);
        // Online, or the system couldn't say: the server didn't answer.
        assert_eq!(no_answer(true, Some(true)), Failure::Timeout);
        assert_eq!(no_answer(true, None), Failure::Timeout);
        assert_eq!(no_answer(false, Some(true)), Failure::Unreachable);
        assert_eq!(no_answer(false, None), Failure::Unreachable);
        assert!(Failure::Offline.line("Open-Meteo").starts_with("No internet"));
        assert!(Failure::Unreachable.line("Open-Meteo").contains("though you're online"));
    }

    #[test]
    fn statuses_and_what_retries() {
        assert_eq!(answered(200), Ok(()));
        assert_eq!(answered(204), Ok(()));
        assert_eq!(answered(404), Err(Failure::Status(404)));
        for f in [Failure::Offline, Failure::Timeout, Failure::Unreachable, Failure::Status(503), Failure::Status(429)] {
            assert!(f.retries(), "{f:?}");
        }
        for f in [Failure::Status(401), Failure::Status(404), Failure::Malformed, Failure::NotConfigured] {
            assert!(!f.retries(), "{f:?}");
        }
        assert_eq!(Failure::Status(503).short(), "Error 503");
        assert!(Failure::Status(401).line("Mireld").contains("refused"));
    }

    #[test]
    fn placeholder_addresses_are_not_asked() {
        assert!(configured("https://api.open-meteo.com/v1/forecast?x=1"));
        assert!(configured("http://127.0.0.1:8080/x"));
        for url in ["https://halo.local.invalid/stocks/live/", "https://x.test/", "https://example/", "ftp://a.com", "", "https:///x", "https://HALO.INVALID./x"] {
            assert!(!configured(url), "{url}");
        }
    }

    #[test]
    fn a_fetch_keeps_its_last_answer_and_backs_off() {
        let mut f: Fetched<u32> = Fetched::default();
        assert!(f.due(0, 60));
        f.asked();
        assert!(!f.due(0, 60), "one ask at a time");
        f.came(Ok(7), 100, 30, 600);
        assert_eq!((f.last, f.next_ask(60)), (Some((7, 100)), Some(160)));
        assert!(!f.due(159, 60) && f.due(160, 60));
        f.came(Err(Failure::Timeout), 200, 30, 600);
        assert_eq!((f.last, f.trouble, f.retry_at), (Some((7, 100)), Some(Failure::Timeout), Some(230)));
        f.came(Err(Failure::Malformed), 230, 30, 600);
        assert_eq!(f.retry_at, Some(830), "a bad answer waits the longest");
        f.came(Ok(8), 900, 30, 600);
        assert_eq!(f, Fetched { last: Some((8, 900)), ..Fetched::default() });
        assert_eq!((age(1000, 1000), age(1000, 280), age(10_000, 0), age(200_000, 0)), ("just now".into(), "12 min ago".into(), "2 h ago".into(), "2 d ago".into()));
    }

    #[test]
    fn backoff_doubles_within_bounds_and_honours_retry_after() {
        let b: Vec<u64> = (1..=7).map(|n| backoff_s(n, 15, 600, None)).collect();
        assert_eq!(b, [15, 30, 60, 120, 240, 480, 600]);
        assert_eq!(backoff_s(0, 15, 600, None), 15);
        assert_eq!(backoff_s(99, 15, 600, None), 600);
        // The server's wait, but never shorter than ours or past the cap.
        assert_eq!(backoff_s(1, 15, 600, Some(120)), 120);
        assert_eq!(backoff_s(3, 15, 600, Some(5)), 60);
        assert_eq!(backoff_s(1, 15, 600, Some(9999)), 600);
    }
}

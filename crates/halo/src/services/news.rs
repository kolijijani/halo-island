//! The Today page's news (`halo_core::news`): one ask of the picked feed on
//! a short-lived thread, which sends [`AppEvent::News`] and ends. No answer is
//! told apart as no internet, a timeout or the feed unreachable; an answer
//! that isn't a feed is `Malformed`. Retries and the kept stories are
//! `halo_core::net::Fetched`'s. Shared by the Windows and Mac apps.

use crate::events::{AppEvent, EventSink};
use crate::services::http::{Client, MtaGuard};
use halo_core::net::Failure;
use halo_core::news::News;
use std::time::Duration;

const USER_AGENT: &str = concat!("HaloIsland/", env!("CARGO_PKG_VERSION"), " (+https://github.com/kolijijani/halo-island)");
const TIMEOUT: Duration = Duration::from_secs(10);
/// A feed's front page is tens of KB; Google News' is about 110 KB.
const MAX_BYTES: u64 = 2 * 1024 * 1024;

pub fn fetch(url: String, sink: EventSink) {
    let s = sink.clone();
    let spawned = std::thread::Builder::new().name("halo-news".into()).spawn(move || {
        let _com = MtaGuard::new();
        let got = read(&url);
        if let Err(f) = &got {
            crate::pwarn!("news: no stories ({f:?})");
        }
        s.send(AppEvent::News(got));
    });
    if spawned.is_err() {
        sink.send(AppEvent::News(Err(Failure::Unreachable)));
    }
}

fn read(url: &str) -> Result<News, Failure> {
    if !halo_core::net::configured(url) {
        return Err(Failure::NotConfigured);
    }
    let client = Client::new(USER_AGENT).map_err(|_| Failure::Unreachable)?;
    let (status, body) = client.get_bytes(url, TIMEOUT, MAX_BYTES, |_| {}).map_err(|e| crate::services::net::failure(&e))?;
    halo_core::net::answered(status)?;
    halo_core::news::parse(&body, crate::clock::unix_now()).map_err(|()| Failure::Malformed)
}

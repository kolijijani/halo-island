//! The Weather widget's one reading (Open-Meteo: no key, no account), on a
//! short-lived thread: it asks once, sends [`AppEvent::Weather`] and ends.

use crate::events::{AppEvent, EventSink};
use crate::services::http::{Client, MtaGuard};
use std::time::Duration;

const USER_AGENT: &str = concat!("HaloIsland/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(10);

pub fn fetch(lat: f64, lng: f64, sink: EventSink) {
    let s = sink.clone();
    let spawned = std::thread::Builder::new().name("halo-weather".into()).spawn(move || {
        let _com = MtaGuard::new();
        let got = Client::new(USER_AGENT)
            .ok()
            .and_then(|c| c.get(&halo_core::halo::weather_url(lat, lng), TIMEOUT).ok())
            .filter(|r| r.status == 200)
            .and_then(|r| halo_core::halo::parse_weather(&r.body));
        if got.is_none() {
            crate::pwarn!("weather: no reading");
        }
        s.send(AppEvent::Weather(got));
    });
    if spawned.is_err() {
        sink.send(AppEvent::Weather(None));
    }
}

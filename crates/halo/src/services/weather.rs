//! The Weather widget's one reading (Open-Meteo: no key, no account), on a
//! short-lived thread: it asks once, sends [`AppEvent::Weather`] and ends.
//! When Windows' location is off or can't tell, [`locate_by_ip`] asks
//! ipwho.is once where the internet address is (`AppEvent::WeatherPlace`).
//! No answer is told apart as no internet (Windows says so), a timeout or
//! Open-Meteo unreachable; an answer that isn't a reading is `Malformed`.
//! Retries and the kept reading are `halo_core::net::Fetched`'s.

use crate::events::{AppEvent, EventSink};
use crate::services::http::{Client, MtaGuard};
use halo_core::net::Failure;
use std::time::Duration;

const USER_AGENT: &str = concat!("HaloIsland/", env!("CARGO_PKG_VERSION"));
const TIMEOUT: Duration = Duration::from_secs(10);

pub fn fetch(lat: f64, lng: f64, sink: EventSink) {
    let s = sink.clone();
    let spawned = std::thread::Builder::new().name("halo-weather".into()).spawn(move || {
        let _com = MtaGuard::new();
        let got = read(lat, lng);
        if let Err(f) = &got {
            crate::pwarn!("weather: no reading ({f:?})");
        }
        s.send(AppEvent::Weather(got));
    });
    if spawned.is_err() {
        sink.send(AppEvent::Weather(Err(Failure::Unreachable)));
    }
}

/// Where the PC is by its internet address: one ask, 10 s at most, on a
/// short-lived thread. Asked only while the Widgets tab is open and the
/// Weather widget has no other place (`App::widgets_shown`).
pub fn locate_by_ip(sink: EventSink) {
    let s = sink.clone();
    let spawned = std::thread::Builder::new().name("halo-weather-ip".into()).spawn(move || {
        let _com = MtaGuard::new();
        let got = (|| {
            let client = Client::private(USER_AGENT).map_err(|_| Failure::Unreachable)?;
            let r = client.get(halo_core::halo::IP_PLACE_URL, TIMEOUT).map_err(|e| crate::services::net::failure(&e))?;
            halo_core::net::answered(r.status)?;
            halo_core::halo::parse_ip_place(&r.body).ok_or(Failure::Malformed)
        })();
        if let Err(f) = &got {
            crate::pwarn!("weather: no place by the internet address ({f:?})");
        }
        s.send(AppEvent::WeatherPlace(got));
    });
    if spawned.is_err() {
        sink.send(AppEvent::WeatherPlace(Err(Failure::Unreachable)));
    }
}

/// Asked uncompressed: Open-Meteo answers `gzip, deflate` with a `deflate`
/// body that Windows' decoder can refuse, which read as "Can't reach it".
fn read(lat: f64, lng: f64) -> Result<halo_core::halo::Weather, Failure> {
    let client = Client::uncompressed(USER_AGENT).map_err(|e| {
        crate::pwarn!("weather: no HTTP client ({e})");
        Failure::Unreachable
    })?;
    let r = client.get(&halo_core::halo::weather_url(lat, lng), TIMEOUT).map_err(|e| {
        // What Windows said (DNS, TLS, decoding…): the log tells them apart, the tile stays short.
        crate::pwarn!("weather: Open-Meteo: {e}");
        crate::services::net::failure(&e)
    })?;
    halo_core::net::answered(r.status)?;
    halo_core::halo::parse_weather(&r.body).ok_or(Failure::Malformed)
}

#[cfg(test)]
mod tests {
    /// Open-Meteo itself, through Windows' HTTP stack as the widget asks it.
    /// It answered `Content-Encoding: deflate` to Windows' default
    /// `Accept-Encoding`; the reading must come back as a reading.
    #[test]
    #[ignore = "asks the live Open-Meteo"]
    fn the_live_weather_comes_in() {
        let _com = crate::services::http::MtaGuard::new();
        match super::read(3.14, 101.69) {
            Ok(w) => assert!((-90.0..=60.0).contains(&w.temp_c), "{w:?}"),
            Err(f) => panic!("no reading from Open-Meteo: {f:?}"),
        }
    }
}

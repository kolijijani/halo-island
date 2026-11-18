//! Where the PC is, for the Prayer times add-on and the Weather widget:
//! Windows' own location service (`Windows.Devices.Geolocation`). Windows
//! decides how (Wi-Fi, GPS where there is one) and whether Halo Island may
//! ask (Settings › Privacy & security › Location).
//!
//! The point only picks the nearest place in the add-on's list (the app
//! does that as soon as it arrives); it is never stored or sent anywhere.

use crate::events::{AppEvent, EventSink};
use crate::services::http::MtaGuard;
use std::sync::mpsc;
use std::time::Duration;
use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator, PositionAccuracy};
use windows::Foundation::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation};

/// Windows may take a while to get a Wi-Fi fix, or the person to answer
/// its question.
const TIMEOUT: Duration = Duration::from_secs(30);

/// What asking Windows for the PC's location came to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Located {
    /// Latitude and longitude, degrees.
    At(f64, f64),
    /// Location is off for this PC, or for desktop apps / Halo Island.
    Denied,
    /// Windows couldn't say where the PC is (no Wi-Fi around, no service).
    Unknown,
    /// No answer within [`TIMEOUT`]: no fix yet, or nobody answered Windows' question.
    TimedOut,
}

/// Ask once; the answer comes back as `AppEvent::Located`. Call it on the
/// UI thread: Windows asks the person (once per app) only from there, and
/// throws from any other thread. The wait happens on a short-lived worker.
pub fn locate(sink: EventSink) {
    let access = Geolocator::RequestAccessAsync().map_err(|e| e.code().0);
    let spawned = std::thread::Builder::new().name("halo-location".into()).spawn(move || {
        let _com = MtaGuard::new();
        let found = ask(access);
        crate::pinfo!(
            "location: {}",
            match found {
                Located::At(..) => "found",
                Located::Denied => "not allowed",
                Located::Unknown => "unknown",
                Located::TimedOut => "timed out",
            }
        );
        sink.send(AppEvent::Located(found));
    });
    if let Err(e) = spawned {
        crate::pwarn!("location: could not start a thread ({e})");
    }
}

/// E_ACCESSDENIED: location is off, or not allowed for desktop apps.
const ACCESS_DENIED: i32 = 0x8007_0005_u32 as i32;

fn ask(access: Result<IAsyncOperation<GeolocationAccessStatus>, i32>) -> Located {
    if let Some(no) = after_access(access.and_then(|op| wait(&op))) {
        return no;
    }
    let Ok(geo) = Geolocator::new() else { return Located::Unknown };
    let _ = geo.SetDesiredAccuracy(PositionAccuracy::Default);
    let point = geo.GetGeopositionAsync().map_err(|e| e.code().0).and_then(|op| wait(&op)).and_then(|pos| {
        let b = pos.Coordinate().and_then(|c| c.Point()).and_then(|p| p.Position()).map_err(|e| e.code().0)?;
        Ok((b.Latitude, b.Longitude))
    });
    position(point)
}

/// Windows' answer to the permission question: None to go on and ask
/// where, else why not. The error is an HRESULT (0: timed out). A question
/// that failed for another reason doesn't settle it: desktop apps could
/// read the position without it before Windows 11 24H2, so it is asked.
fn after_access(r: Result<GeolocationAccessStatus, i32>) -> Option<Located> {
    match r {
        Ok(GeolocationAccessStatus::Denied) | Err(ACCESS_DENIED) => Some(Located::Denied),
        Ok(_) => None,
        Err(0) => Some(Located::TimedOut),
        Err(code) => {
            crate::pwarn!("location: Windows' permission question failed (0x{code:08X}); asking where anyway");
            None
        }
    }
}

/// Windows' answer to where: the point, or why there is none.
fn position(r: Result<(f64, f64), i32>) -> Located {
    match r {
        Ok((lat, lng)) if lat.is_finite() && lng.is_finite() => Located::At(lat, lng),
        Err(ACCESS_DENIED) => Located::Denied,
        Err(0) => Located::TimedOut,
        Err(code) => {
            crate::pwarn!("location: no position (0x{code:08X})");
            Located::Unknown
        }
        Ok(_) => Located::Unknown,
    }
}

/// Wait for a WinRT operation, at most `TIMEOUT` (then cancel it). The
/// error is the operation's HRESULT (0 = timed out or cancelled).
fn wait<T: windows_core::RuntimeType + 'static>(op: &IAsyncOperation<T>) -> Result<T, i32> {
    if op.Status().map_err(|e| e.code().0)? == AsyncStatus::Started {
        let (tx, rx) = mpsc::channel::<()>();
        let done = AsyncOperationCompletedHandler::new(move |_, _| {
            let _ = tx.send(());
            Ok(())
        });
        op.SetCompleted(&done).map_err(|e| e.code().0)?;
        if rx.recv_timeout(TIMEOUT).is_err() {
            let _ = op.Cancel();
            return Err(0);
        }
    }
    match op.Status().map_err(|e| e.code().0)? {
        // For an error, GetResults returns the operation's failure (access denied shows up here).
        AsyncStatus::Completed | AsyncStatus::Error => op.GetResults().map_err(|e| e.code().0),
        _ => Err(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_answer_says_honestly_why_there_is_no_place() {
        // The permission: allowed (or Windows didn't say) goes on to ask where.
        assert_eq!(after_access(Ok(GeolocationAccessStatus::Allowed)), None);
        assert_eq!(after_access(Ok(GeolocationAccessStatus::Unspecified)), None);
        assert_eq!(after_access(Ok(GeolocationAccessStatus::Denied)), Some(Located::Denied));
        assert_eq!(after_access(Err(ACCESS_DENIED)), Some(Located::Denied));
        // Nobody answered Windows' question in time: not "unknown".
        assert_eq!(after_access(Err(0)), Some(Located::TimedOut));
        // RPC_E_WRONG_THREAD (a call off the UI thread): the position is still asked.
        assert_eq!(after_access(Err(0x8001_010E_u32 as i32)), None);
        // Where.
        assert_eq!(position(Ok((3.14, 101.69))), Located::At(3.14, 101.69));
        assert_eq!(position(Ok((f64::NAN, 101.69))), Located::Unknown);
        assert_eq!(position(Err(ACCESS_DENIED)), Located::Denied);
        assert_eq!(position(Err(0)), Located::TimedOut);
        assert_eq!(position(Err(0x8000_4004_u32 as i32)), Located::Unknown);
    }
}

//! Wi-Fi joining and leaving, for its pop-up. Windows' own
//! `NetworkInformation.NetworkStatusChanged` event wakes a worker thread
//! (no poll); it reads where Wi-Fi stands now and, when that is news
//! (`halo_core::popups::wifi_change`: joined, left or another network, never
//! the internet coming and going on the same one), sends [`AppEvent::Wifi`].
//!
//! What it reads: whether a Wi-Fi connection profile is up, its network's
//! name (Windows 11 shows it only to apps the person lets use location: the
//! pop-up then says "Wi-Fi", never a guess), and whether it reaches the
//! internet (Windows' own check: connected to Wi-Fi is not online).
//!
//! Idle cost: the thread sleeps in a blocking `recv`; a burst of events
//! (Windows sends several as a network comes up) is read once after it
//! settles.

use crate::events::{AppEvent, EventSink};
use halo_core::popups::{wifi_change, Wifi};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;
use windows::Foundation::EventRegistrationToken;
use windows::Networking::Connectivity::{
    NetworkAuthenticationType, NetworkConnectivityLevel, NetworkEncryptionType,
    NetworkInformation, NetworkStatusChangedEventHandler,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED};

/// Events this close together are one change: read once after the last.
const SETTLE: Duration = Duration::from_millis(700);

enum Msg {
    Changed,
    Shutdown,
}

pub struct NetworkWatcher {
    tx: Sender<Msg>,
    token: Option<EventRegistrationToken>,
}

impl NetworkWatcher {
    pub fn start(sink: EventSink) -> windows::core::Result<Self> {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("halo-network".into())
            .spawn(move || {
                let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
                // Seed initial state silently (app suppresses HUD on first event), then wait on changes.
                let mut was = read();
                let _ = sink.send(AppEvent::Wifi(was.clone()));
                loop {
                    match rx.recv() {
                        Ok(Msg::Changed) => {}
                        Ok(Msg::Shutdown) | Err(_) => break,
                    }
                    // Let the burst settle.
                    loop {
                        match rx.recv_timeout(SETTLE) {
                            Ok(Msg::Changed) => continue,
                            Ok(Msg::Shutdown) | Err(RecvTimeoutError::Disconnected) => {
                                if com {
                                    unsafe { CoUninitialize() };
                                }
                                return;
                            }
                            Err(RecvTimeoutError::Timeout) => break,
                        }
                    }
                    let now = read();
                    if let Some(said) = wifi_change(&was, &now) {
                        crate::pinfo!("network: Wi-Fi {}", if said.connected { "joined" } else { "left" });
                        sink.send(AppEvent::Wifi(said));
                    }
                    was = now;
                }
                if com {
                    unsafe { CoUninitialize() };
                }
            })
            .map_err(|e| windows::core::Error::new(windows::Win32::Foundation::E_FAIL, format!("network thread: {e}")))?;
        let handler_tx = tx.clone();
        let handler = NetworkStatusChangedEventHandler::new(move |_| {
            let _ = handler_tx.send(Msg::Changed);
            Ok(())
        });
        let token = match NetworkInformation::NetworkStatusChanged(&handler) {
            Ok(t) => Some(t),
            Err(e) => {
                let _ = tx.send(Msg::Shutdown);
                return Err(e);
            }
        };
        Ok(Self { tx, token })
    }
}

impl Drop for NetworkWatcher {
    fn drop(&mut self) {
        if let Some(t) = self.token.take() {
            let _ = NetworkInformation::RemoveNetworkStatusChanged(t);
        }
        let _ = self.tx.send(Msg::Shutdown);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WifiDetails {
    pub connected: bool,
    pub ssid: Option<String>,
    pub internet: bool,
    pub signal_pct: Option<u8>,
    pub security: String,
}

fn auth_label(auth: NetworkAuthenticationType, enc: NetworkEncryptionType) -> String {
    let a = match auth {
        NetworkAuthenticationType::None | NetworkAuthenticationType::Open80211 => "Open",
        NetworkAuthenticationType::SharedKey80211 => "WEP",
        NetworkAuthenticationType::Wpa => "WPA-Enterprise",
        NetworkAuthenticationType::WpaPsk => "WPA-Personal",
        NetworkAuthenticationType::Rsna => "WPA2-Enterprise",
        NetworkAuthenticationType::RsnaPsk => "WPA2-Personal",
        NetworkAuthenticationType::Ihv => "Custom / Vendor",
        _ => {
            if auth.0 == 10 {
                "WPA3-Enterprise"
            } else if auth.0 == 11 || auth.0 == 12 {
                "WPA3-SAE"
            } else {
                "Secured"
            }
        }
    };
    if matches!(auth, NetworkAuthenticationType::None | NetworkAuthenticationType::Open80211)
        || enc == NetworkEncryptionType::None
    {
        a.to_string()
    } else if enc == NetworkEncryptionType::Gcmp || enc == NetworkEncryptionType::Gcmp256 {
        format!("{a} (GCMP)")
    } else {
        a.to_string()
    }
}

/// Live snapshot of the active Wi-Fi connection profile (signal bars, SSID, security).
pub fn query_details() -> WifiDetails {
    let profiles = match NetworkInformation::GetConnectionProfiles() {
        Ok(p) => p,
        Err(_) => return WifiDetails::default(),
    };
    let size = profiles.Size().unwrap_or(0);
    for i in 0..size {
        let Ok(p) = profiles.GetAt(i) else { continue };
        if !p.IsWlanConnectionProfile().unwrap_or(false) {
            continue;
        }
        let level = p.GetNetworkConnectivityLevel().unwrap_or(NetworkConnectivityLevel::None);
        if level == NetworkConnectivityLevel::None {
            continue;
        }
        let ssid = p
            .WlanConnectionProfileDetails()
            .ok()
            .and_then(|d| d.GetConnectedSsid().ok())
            .map(|s| s.to_string_lossy())
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                p.ProfileName()
                    .ok()
                    .map(|s| s.to_string_lossy())
                    .filter(|s| !s.trim().is_empty())
            });
        let signal_pct = p
            .GetSignalBars()
            .ok()
            .and_then(|b| b.Value().ok())
            .map(|bars| bars.min(5) * 20);
        let security = p
            .NetworkSecuritySettings()
            .ok()
            .map(|sec| {
                let auth = sec.NetworkAuthenticationType().unwrap_or(NetworkAuthenticationType::Unknown);
                let enc = sec.NetworkEncryptionType().unwrap_or(NetworkEncryptionType::Unknown);
                auth_label(auth, enc)
            })
            .unwrap_or_default();
        return WifiDetails {
            connected: true,
            ssid,
            internet: level == NetworkConnectivityLevel::InternetAccess,
            signal_pct,
            security,
        };
    }
    WifiDetails::default()
}

/// Where Wi-Fi stands now. Any failure reads as "not on Wi-Fi", which only
/// a later real change can contradict.
fn read() -> Wifi {
    let d = query_details();
    Wifi {
        connected: d.connected,
        ssid: d.ssid,
        internet: d.internet,
    }
}

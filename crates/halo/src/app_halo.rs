//! Halo Island's own app parts (`halo_core::halo`): the Pro gate, the drink
//! water reminder, the clipboard list, cloud sync and the Weather widget's
//! reading. A child of `app`, so it reaches the app's state.
//!
//! What runs at rest, each only while the person switched it on:
//! * Drink water: one `SetTimer` one-shot per reminder (45-90 min apart).
//! * Clipboard: Windows' own `WM_CLIPBOARDUPDATE` (no poll), read once per copy.
//! * Cloud sync: a copy of two files at each save and one read at start.
//! The Weather widget reads once when the Widgets tab opens and the last
//! reading is over 30 minutes old.

use super::{App, TIMER_WATER};
use crate::events::AppEvent;
use crate::icons::Icon;
use crate::pages::PageKind;
use crate::state::NoticeAction;
use crate::paths;
use halo_core::geom::palette;
use halo_core::halo;
use std::path::PathBuf;
use windows::Win32::System::DataExchange::{AddClipboardFormatListener, RemoveClipboardFormatListener};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

impl App {
    /// After any settings change: Pro's parts stop without Pro, and the
    /// water timer and clipboard listener follow their switches.
    pub(super) fn sync_halo(&mut self) {
        if self.st.settings.pro_gate() {
            pinfo!("halo pro: off, its parts stop");
        }
        self.sync_water();
        self.sync_clips();
    }

    // ------------------------------------------------------------ water

    pub(super) fn sync_water(&mut self) {
        let every = self.st.settings.water_every_min;
        unsafe {
            if every == 0 || self.selftest.is_some() {
                let _ = KillTimer(self.hwnd, TIMER_WATER);
            } else {
                SetTimer(self.hwnd, TIMER_WATER, every * 60_000, None);
            }
        }
    }

    /// The water timer: a gentle notice with today's glasses, and +1 glass
    /// on a click. Nobody there: it waits for the next one.
    pub(super) fn water_due(&mut self) {
        if self.locked || self.display_off || self.fullscreen_hidden || self.st.notice.is_some() {
            return;
        }
        let day = self.st.local.day_key();
        let n = self.st.store.halo_water.today(&day);
        let goal = self.st.settings.water_goal;
        if n >= goal {
            return;
        }
        self.st.notify(Icon::Coffee, palette::CYAN, "Drink water", halo::fmt_glasses(n, goal), 8.0, Some(NoticeAction::OpenPage(PageKind::Widgets)));
        self.request_frame();
    }

    // ------------------------------------------------------------ clipboard

    pub(super) fn sync_clips(&mut self) {
        let on = self.st.settings.clips && self.selftest.is_none();
        if on == self.clips_listening {
            return;
        }
        self.clips_listening = on;
        unsafe {
            let r = if on { AddClipboardFormatListener(self.helper) } else { RemoveClipboardFormatListener(self.helper) };
            if let Err(e) = r {
                pwarn!("clipboard: listener {} failed ({e})", if on { "on" } else { "off" });
            }
        }
    }

    /// Something was copied (`WM_CLIPBOARDUPDATE`, the helper window's).
    pub(super) fn clip_copied(&mut self) {
        if !self.st.settings.clips {
            return;
        }
        if let Some(t) = crate::services::clipboard::read_text(self.helper) {
            let cap = halo::clip_cap(self.st.settings.pro);
            if self.st.clips.push(&t, cap) && self.expanded && self.st.page == PageKind::Widgets {
                self.request_frame();
            }
        }
    }

    // ------------------------------------------------------------ cloud sync

    /// OneDrive's folder, where Windows says it is. None without OneDrive.
    fn sync_dir() -> Option<PathBuf> {
        let root = std::env::var_os("OneDrive").or_else(|| std::env::var_os("OneDriveConsumer"))?;
        Some(PathBuf::from(root).join("Halo Island"))
    }

    fn mtime_ms(p: &std::path::Path) -> u64 {
        std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as u64)
    }

    /// At each save: this PC's two files go to the synced folder.
    pub(super) fn cloud_push(&self) {
        if !(self.st.settings.cloud_sync && self.st.settings.pro) || self.selftest.is_some() {
            return;
        }
        let Some(dir) = Self::sync_dir() else { return };
        let _ = std::fs::create_dir_all(&dir);
        for p in [paths::settings_path(), paths::store_path()] {
            if let Some(name) = p.file_name() {
                if let Err(e) = std::fs::copy(&p, dir.join(name)) {
                    pwarn!("cloud sync: {} not copied ({e})", name.to_string_lossy());
                }
            }
        }
    }

    /// At start: a newer copy from another PC replaces this one's, and is
    /// loaded. Returns whether anything came.
    pub(super) fn cloud_pull(&mut self) -> bool {
        if !(self.st.settings.cloud_sync && self.st.settings.pro) || self.selftest.is_some() {
            return false;
        }
        let Some(dir) = Self::sync_dir() else { return false };
        let mut came = false;
        for p in [paths::settings_path(), paths::store_path()] {
            let Some(name) = p.file_name() else { continue };
            let other = dir.join(name);
            if other.exists() && halo::newer_wins(Self::mtime_ms(&p), Self::mtime_ms(&other)) && std::fs::copy(&other, &p).is_ok() {
                pinfo!("cloud sync: {} from another PC", name.to_string_lossy());
                came = true;
            }
        }
        if came {
            let (s, _): (halo_core::settings::Settings, _) = halo_core::store::load_json(&paths::settings_path());
            let (d, _): (halo_core::store::Store, _) = halo_core::store::load_json(&paths::store_path());
            self.st.settings = s.sanitized();
            self.saved_settings = self.st.settings.clone();
            self.st.store = d;
        }
        came
    }

    // ------------------------------------------------------------ weather

    /// The Widgets tab came up: read the weather if it's stale. Where: the
    /// Prayer times place when there is one, else ask Windows once.
    pub(super) fn widgets_shown(&mut self) {
        if self.selftest.is_some() || !self.st.settings.widgets.contains(&halo::Widget::Weather) {
            return;
        }
        if self.st.weather.as_ref().is_some_and(|(_, at)| self.st.unix - at < halo::WEATHER_FRESH_S) {
            return;
        }
        let at = self.st.prayer_site().map(|s| (s.lat, s.lng)).or(self.st.weather_at);
        match at {
            Some((lat, lng)) => crate::services::weather::fetch(lat, lng, self.sink.clone()),
            None => crate::services::location::locate(self.sink.clone()),
        }
    }

    pub(super) fn on_weather(&mut self, ev: AppEvent) {
        if let AppEvent::Weather(Some(w)) = ev {
            self.st.weather = Some((w, self.st.unix));
            self.request_frame();
        }
    }
}

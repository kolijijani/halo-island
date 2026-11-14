//! Halo Island's own app parts (`halo_core::halo`): the Pro gate, the drink
//! water reminder, the clipboard list, cloud sync, the Weather widget's
//! reading and the Today page's news. A child of `app`, so it reaches the app's state.
//!
//! What runs at rest, each only while the person switched it on:
//! * Drink water: one `SetTimer` one-shot per reminder (45-90 min apart).
//! * Clipboard: Windows' own `WM_CLIPBOARDUPDATE` (no poll), read once per copy.
//! * Cloud sync: a copy of two files at each save and one read at start.
//! The Today page's news reads when the page opens and the last read is over
//! 15 minutes old, and retries after a failure only while it stays open.
//! The Weather widget reads when the Widgets tab opens and the last reading
//! is over 30 minutes old; a failed read is tried again (30 s doubling to 10
//! minutes) only while the tab stays open (`pages::widgets::next_wake`).
//! Without a place, Windows' location is asked as the tab opens; when it's
//! off or can't tell, ipwho.is once a run (10 s at most). The last good
//! reading is kept in data.json and shown with its age while offline.

use super::{App, TIMER_WATER};
use crate::activities::Mode;
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
            let (mut s, _): (halo_core::settings::Settings, _) = halo_core::store::load_json(&paths::settings_path());
            let (d, _): (halo_core::store::Store, _) = halo_core::store::load_json(&paths::store_path());
            // This install's license stays this install's.
            s.licensed = self.st.settings.licensed;
            self.st.settings = s.sanitized();
            self.saved_settings = self.st.settings.clone();
            self.st.store = d;
        }
        came
    }

    // ------------------------------------------------------------ weather

    /// The Widgets tab is up (`opened`: it just opened; else a frame while
    /// it shows, a retry's wake among them): read the weather when it's due.
    /// Where: [`AppState::weather_place`]. With none, Windows is asked as the
    /// tab opens (not again on every frame after a refusal); when Windows
    /// says no or can't tell, the internet address is asked, once a run.
    pub(super) fn widgets_shown(&mut self, opened: bool) {
        if self.selftest.is_some() || !self.st.settings.widgets.contains(&halo::Widget::Weather) || !self.st.weather.due(self.st.unix, halo::WEATHER_FRESH_S) {
            return;
        }
        match self.st.weather_place() {
            Some((lat, lng, _)) => {
                self.st.weather.asked();
                crate::services::weather::fetch(lat, lng, self.sink.clone());
            }
            None if self.st.prayer.locate == crate::state::Locate::Asking => {}
            None if matches!(self.st.prayer.locate, crate::state::Locate::Denied | crate::state::Locate::Unknown) => self.weather_by_ip(),
            None if opened => self.locate(),
            None => {}
        }
    }

    /// Ask where the internet address is, once a run (and again only after
    /// a failure that may mend, the next time the tab opens).
    pub(super) fn weather_by_ip(&mut self) {
        let retry = matches!(self.st.weather_ip, Some(Err(f)) if f.retries());
        if self.st.weather_ip_asking || (self.st.weather_ip.is_some() && !retry) || self.selftest.is_some() {
            return;
        }
        self.st.weather_ip_asking = true;
        crate::services::weather::locate_by_ip(self.sink.clone());
    }

    // ------------------------------------------------------------ licenses

    /// The license server's answer to a code: Pro on with a token that
    /// checked out here, else its reason under the field.
    pub(super) fn on_license(&mut self, r: crate::services::license::Redeemed) {
        use crate::services::license::Redeemed;
        self.st.ui.pro_redeeming = false;
        match r {
            Redeemed::Token { token, grant } => {
                if crate::services::license::keep(&token) {
                    self.st.ui.license = Some(Ok(grant.clone()));
                    self.st.settings.licensed = true;
                    self.st.settings.pro = true;
                    self.st.ui.pro_key_input.clear();
                    self.st.ui.pro_trouble = None;
                    let what = if grant.kind == "trial" { "Trial Pro is on" } else { "Halo Pro is on" };
                    self.st.notify(Icon::Check, palette::GREEN, what, "Every Pro part is on for this PC", 5.0, None);
                    self.st.cmd(crate::commands::Command::SaveSettings);
                } else {
                    self.st.ui.pro_trouble = Some("The license couldn't be kept on this PC.".into());
                }
            }
            Redeemed::Refused(w) => self.st.ui.pro_trouble = Some(w.into()),
            Redeemed::Trouble(t) => self.st.ui.pro_trouble = Some(t),
            Redeemed::NoServer => self.st.ui.pro_trouble = Some("This version has no license server set up.".into()),
        }
        self.request_frame();
    }

    // ------------------------------------------------------------ news

    /// The Today page is up (as it opens, and on its wakes): read the news
    /// when it's due. Nobody at the PC, or the self-test: nothing is asked.
    pub(super) fn today_shown(&mut self) {
        if self.selftest.is_some() || self.locked || self.display_off || !self.st.news.due(self.st.unix, halo_core::news::FRESH_S) {
            return;
        }
        self.st.news.asked();
        crate::services::news::fetch(halo_core::news::feed_url(&self.st.settings.news_feed), self.sink.clone());
    }

    pub(super) fn on_news(&mut self, got: Result<halo_core::news::News, halo_core::net::Failure>) {
        self.st.news.came(got, self.st.unix, halo_core::news::RETRY_FIRST_S, halo_core::news::RETRY_LAST_S);
        self.request_frame();
    }

    pub(super) fn on_weather(&mut self, ev: AppEvent) {
        match ev {
            AppEvent::Weather(got) => {
                let good = got.is_ok();
                self.st.weather.came(got, self.st.unix, halo::WEATHER_RETRY_FIRST_S, halo::WEATHER_RETRY_LAST_S);
                // Kept for the next time it's offline, after a restart too.
                if good {
                    self.st.store.weather_last = self.st.weather.last;
                    self.st.save();
                }
            }
            AppEvent::WeatherPlace(got) => {
                self.st.weather_ip_asking = false;
                self.st.weather_ip = Some(got);
                if self.mode == Mode::Expanded(PageKind::Widgets) {
                    self.widgets_shown(false);
                }
            }
            _ => {}
        }
        self.request_frame();
    }
}

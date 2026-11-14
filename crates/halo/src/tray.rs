//! Notification-area icon and its menu (also shown on right-click of the island).

use crate::events::WM_APP_TRAY;
use crate::updates::UpdateLine;
use halo_core::settings::{IdleStyle, Settings, Theme};
use windows::core::{Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW};
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct Tray {
    hwnd: HWND,
}

impl Tray {
    pub fn add(hwnd: HWND) -> Result<Tray> {
        unsafe {
            // The small-icon size: halo.ico's own 16-24 px mascot frames.
            let icon = Some(crate::window::app_icon(true)).filter(|i| !i.is_invalid()).map_or_else(|| LoadIconW(None, IDI_APPLICATION), Ok)?;
            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_APP_TRAY,
                hIcon: icon,
                ..Default::default()
            };
            let tip: Vec<u16> = format!("{} {}", crate::version::APP_NAME, crate::version::VERSION).encode_utf16().collect();
            nid.szTip[..tip.len()].copy_from_slice(&tip);
            if !Shell_NotifyIconW(NIM_ADD, &nid).as_bool() {
                return Err(windows::core::Error::from_win32());
            }
        }
        Ok(Tray { hwnd })
    }

    pub fn remove(self) {
        unsafe {
            let nid = NOTIFYICONDATAW { cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32, hWnd: self.hwnd, uID: 1, ..Default::default() };
            let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuCmd {
    ToggleIdleStyle(IdleStyle),
    SetTheme(Theme),
    ToggleSystemHistory,
    /// Halo Mascot: Halo's eyes in place of the idle clock.
    ToggleMascot,
    ToggleLyrics,
    TogglePronunciation,
    /// true: hide in full-screen apps; false: stay on top of them.
    SetHideFullscreen(bool),
    /// Show over games (frame rate, GPU and CPU on the island over a game).
    ToggleGameStats,
    /// Show over games for a few seconds at a time instead of all along.
    ToggleGamePeek,
    ToggleSounds,
    TogglePinned,
    /// "Move island to the centre": back from where it was dragged.
    CentreIsland,
    ToggleAutostart,
    /// "Advanced settings…": the Settings window.
    ShowSettings,
    /// A line under Halo Pro: the Settings window on Halo Pro (the key).
    ShowPro,
    OpenDataFolder,
    /// "What's New": the island's About sheet with this version's lines.
    WhatsNew,
    /// "About Halo Island 0.2.3 (…)": the island's About sheet, not a URL.
    About,
    /// "Lyrics by LRCLIB": opens lrclib.net.
    LyricsCredit,
    AppleMusicToken,
    AppleMusicHelp,
    AppleMusicForget,
    RecordScreen,
    /// "Update to Halo Island 0.2.5…" (`crate::updates`).
    Update,
    /// "Check for updates…": read the update file now (`crate::updates`).
    CheckForUpdates,
    Quit,
}

pub enum MenuItem {
    Item { label: String, cmd: MenuCmd, checked: bool },
    Sub { label: String, items: Vec<MenuItem> },
    Separator,
}

fn item(label: &str, cmd: MenuCmd, checked: bool) -> MenuItem {
    MenuItem::Item { label: label.to_string(), cmd, checked }
}

/// An idle look as the menu names it. In the Halo theme the eyes are Halo's own.
fn idle_label(s: IdleStyle, theme: Theme) -> &'static str {
    match s {
        IdleStyle::Clock => "Clock",
        IdleStyle::Eyes if theme == Theme::Halo => "Halo",
        IdleStyle::Eyes => "Eyes",
        IdleStyle::Minimal => "Minimal",
        IdleStyle::Hidden => "Hidden until hover",
        IdleStyle::Status => "Status bar",
    }
}

/// `update`: a later version the update file offers, or Check for updates…
/// (`crate::updates::menu_line`).
pub fn menu_items(s: &Settings, pinned: bool, recorder: bool, update: UpdateLine) -> Vec<MenuItem> {
    // The look the theme draws is the one checked: outside Pill, Status shows the clock.
    let shown = crate::pill::status::shown(s.theme, s.idle_style);
    let style = |st: IdleStyle| item(idle_label(st, s.theme), MenuCmd::ToggleIdleStyle(st), shown == st);
    // A theme Halo Pro keeps while Pro is off: "(Halo Pro)" behind its name.
    let theme = |label: &str, t: Theme| {
        let locked = !t.free() && !s.pro;
        item(&if locked { format!("{label} (Halo Pro)") } else { label.to_string() }, MenuCmd::SetTheme(t), s.theme == t)
    };
    let fullscreen = |label: &str, hide: bool| item(label, MenuCmd::SetHideFullscreen(hide), s.hide_in_fullscreen == hide);
    let mut items = vec![
        item("Advanced settings…", MenuCmd::ShowSettings, false),
        MenuItem::Separator,
        item("Keep island open", MenuCmd::TogglePinned, pinned),
        MenuItem::Sub {
            label: "Theme".into(),
            items: vec![theme("Classic", Theme::Classic), theme("Halo", Theme::Halo), theme("Glass", Theme::Glass), theme("Pill", Theme::Pill), theme("Diorama", Theme::Diorama)],
        },
        // The status bar is listed only in Pill, as on the Settings page: the
        // other themes would draw the clock for it (Diorama its picture).
        MenuItem::Sub { label: "Idle look".into(), items: crate::pill::status::looks(s.theme).iter().map(|l| style(*l)).collect() },
        item("Halo Mascot", MenuCmd::ToggleMascot, s.mascot),
        MenuItem::Separator,
        item("Synced lyrics", MenuCmd::ToggleLyrics, s.lyrics),
        item("Lyrics pronunciation (romaji, pinyin, Korean)", MenuCmd::TogglePronunciation, s.lyrics_pronunciation),
        apple_music_menu(),
        item("Sounds", MenuCmd::ToggleSounds, s.sounds),
        MenuItem::Sub {
            label: "In full-screen apps".into(),
            items: vec![
                fullscreen("Hide", true),
                fullscreen("Stay on top", false),
                MenuItem::Separator,
                item(if s.pro { "Show FPS, GPU and CPU over games" } else { "Show FPS, GPU and CPU over games (Halo Pro)" }, MenuCmd::ToggleGameStats, s.game_stats),
                item("Only for a few seconds (no added delay)", MenuCmd::ToggleGamePeek, s.game_peek),
            ],
        },
        item("Start with Windows", MenuCmd::ToggleAutostart, crate::autostart::is_enabled()),
        MenuItem::Separator,
        item("Open data folder", MenuCmd::OpenDataFolder, false),
        item("What's New", MenuCmd::WhatsNew, false),
        // The lyrics credit stays right above the version line.
        item("Lyrics by LRCLIB", MenuCmd::LyricsCredit, false),
        // Which version and build this is, as the Mac menu says it.
        item(&crate::version::about_label(), MenuCmd::About, false),
        // Halo Pro last: the way to its key, then what it unlocks, a line
        // each. Plain items, not greyed ones: Windows hover-highlights a
        // greyed item but keeps its grey text, which can't be read on the
        // highlight (least of all in high contrast). Enabled, they take the
        // menu's own text and selection colours in every theme and under
        // the keyboard, and each opens Settings › Halo Pro.
        MenuItem::Separator,
        MenuItem::Sub { label: "Halo Pro".into(), items: pro_items(s.pro) },
        MenuItem::Separator,
        item(&format!("Quit {}", crate::version::APP_NAME), MenuCmd::Quit, false),
    ];
    // Only once the island was dragged off the middle: the way back, under Keep island open.
    if s.island_x.is_some() {
        let at = items.iter().position(|i| matches!(i, MenuItem::Item { cmd: MenuCmd::TogglePinned, .. })).map_or(items.len(), |i| i + 1);
        items.insert(at, item("Move island to the centre", MenuCmd::CentreIsland, false));
    }
    // Only while the System add-on is on, with Halo Pro: its page's history, under Sounds.
    if s.system_on() && s.pro {
        let at = items.iter().position(|i| matches!(i, MenuItem::Item { cmd: MenuCmd::ToggleSounds, .. })).unwrap_or(items.len());
        items.insert(at, item("Keep System page history", MenuCmd::ToggleSystemHistory, s.system_history));
    }
    // Only once the separate recorder app is installed: until then the
    // island doesn't mention recording at all.
    if recorder {
        items.splice(0..0, [item("Record this screen", MenuCmd::RecordScreen, false), MenuItem::Separator]);
    }
    match update {
        // A new version first, until it is installed.
        UpdateLine::Offer(v) => {
            items.splice(0..0, [item(&format!("Update to {} {v}…", crate::version::APP_NAME), MenuCmd::Update, false), MenuItem::Separator]);
        }
        // Otherwise a look on request, under the version line.
        UpdateLine::Check => {
            let at = items.iter().position(|i| matches!(i, MenuItem::Item { cmd: MenuCmd::About, .. })).map_or(items.len(), |i| i + 1);
            items.insert(at, item("Check for updates…", MenuCmd::CheckForUpdates, false));
        }
        UpdateLine::None => {}
    }
    items
}

/// Apple Music lyrics: the person's token, from the clipboard.
fn apple_music_menu() -> MenuItem {
    let on = crate::paths::apple_music_token_path().exists();
    let mut items = vec![
        item("Use token from clipboard", MenuCmd::AppleMusicToken, false),
        item("How to get the token", MenuCmd::AppleMusicHelp, false),
    ];
    if on {
        items.push(MenuItem::Separator);
        items.push(item("Remove token", MenuCmd::AppleMusicForget, false));
    }
    MenuItem::Sub { label: if on { "Apple Music lyrics: on".into() } else { "Apple Music lyrics".into() }, items }
}

/// The Halo Pro submenu: Activate (or that it's on), a line, then what it
/// unlocks; every item opens Settings › Halo Pro.
fn pro_items(pro: bool) -> Vec<MenuItem> {
    let head = if pro { "Halo Pro is on…" } else { "Activate Halo Pro…" };
    let mut items = vec![item(head, MenuCmd::ShowPro, false), MenuItem::Separator];
    items.extend(PRO_LINES.iter().map(|l| item(l, MenuCmd::ShowPro, false)));
    items
}

/// What Halo Pro unlocks, one line each, as the menu's Halo Pro item says.
pub const PRO_LINES: [&str; 7] = [
    "Pro: all 5 widgets",
    "500 clipboard items",
    "world clock, moon & countdown",
    "custom themes",
    "Glass, Pill & Diorama themes",
    "AI Usage & Downloader add-ons",
    "cloud sync",
];

fn build(menu: HMENU, items: &[MenuItem], cmds: &mut Vec<MenuCmd>) {
    for it in items {
        unsafe {
            match it {
                MenuItem::Item { label, cmd, checked } => {
                    cmds.push(*cmd);
                    let id = cmds.len();
                    let w: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                    let flags = MF_STRING | if *checked { MF_CHECKED } else { MF_UNCHECKED };
                    let _ = AppendMenuW(menu, flags, id, PCWSTR(w.as_ptr()));
                }
                MenuItem::Sub { label, items } => {
                    if let Ok(sub) = CreatePopupMenu() {
                        build(sub, items, cmds);
                        let w: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                        let _ = AppendMenuW(menu, MF_POPUP, sub.0 as usize, PCWSTR(w.as_ptr()));
                    }
                }
                MenuItem::Separator => {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                }
            }
        }
    }
}

/// Show the menu at `pt` (screen pixels); returns the chosen command.
pub fn show_menu(owner: HWND, pt: POINT, items: &[MenuItem]) -> Option<MenuCmd> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let mut cmds = Vec::new();
        build(menu, items, &mut cmds);
        // Required so the menu closes when clicking elsewhere; afterwards the
        // previous app gets its focus back.
        let prev = GetForegroundWindow();
        let _ = SetForegroundWindow(owner);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, owner, None);
        let _ = PostMessageW(owner, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        let idx = chosen.0 as usize;
        let cmd = if idx == 0 { None } else { cmds.get(idx - 1).copied() };
        let launches = matches!(
            cmd,
            Some(MenuCmd::ShowSettings | MenuCmd::ShowPro | MenuCmd::OpenDataFolder | MenuCmd::About | MenuCmd::LyricsCredit | MenuCmd::AppleMusicHelp)
        );
        if !launches && !prev.is_invalid() && prev != owner && IsWindow(prev).as_bool() && GetForegroundWindow() == owner {
            let _ = SetForegroundWindow(prev);
        }
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fullscreen_choices(s: &Settings) -> Vec<(String, MenuCmd, bool)> {
        let items = menu_items(s, false, false, UpdateLine::None);
        let Some(MenuItem::Sub { items, .. }) = items.into_iter().find(|i| matches!(i, MenuItem::Sub { label, .. } if label == "In full-screen apps")) else {
            panic!("no full-screen submenu");
        };
        items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, cmd, checked } => Some((label, cmd, checked)),
                _ => None,
            })
            .collect()
    }

    /// The mascot icon is built into the exe (`res/halo.rc`) in every size
    /// the shell asks for, with its own transparent corners: the taskbar,
    /// the notification area and the exe's own icon all read it from here.
    #[test]
    fn the_mascot_icon_is_in_the_exe_at_every_size() {
        use windows::Win32::Graphics::Gdi::{DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS};
        let hinst = unsafe { GetModuleHandleW(None) }.unwrap();
        for px in [16, 20, 24, 32, 40, 48, 64, 256] {
            let icon = unsafe { LoadImageW(hinst, PCWSTR(1 as _), IMAGE_ICON, px, px, LR_DEFAULTCOLOR) }.unwrap_or_else(|e| panic!("no {px} px icon: {e}"));
            let icon = HICON(icon.0);
            let mut info = ICONINFO::default();
            unsafe { GetIconInfo(icon, &mut info) }.unwrap();
            let mut bm = BITMAP::default();
            unsafe { GetObjectW(info.hbmColor, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut _ as *mut _)) };
            assert_eq!((bm.bmWidth, bm.bmHeight, bm.bmBitsPixel), (px, px, 32), "{px} px frame");
            let mut bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: px, biHeight: -px, biPlanes: 1, biBitCount: 32, ..Default::default() }, ..Default::default() };
            let mut bgra = vec![0u8; (px * px * 4) as usize];
            let dc = unsafe { windows::Win32::Graphics::Gdi::GetDC(None) };
            unsafe { GetDIBits(dc, info.hbmColor, 0, px as u32, Some(bgra.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS) };
            unsafe { windows::Win32::Graphics::Gdi::ReleaseDC(None, dc) };
            let at = |x: i32, y: i32| &bgra[((y * px + x) * 4) as usize..][..4];
            // A clear corner, grey fur, and the yellow capsule eyes.
            assert_eq!(at(0, 0)[3], 0, "{px} px corner is see-through");
            let fur = at(px / 2, px / 2);
            assert!(fur[3] >= 250 && fur[1] >= 60 && fur[1] <= 140 && fur[2] >= 60 && fur[2] <= 140, "{px} px fur {fur:?}");
            let mut eye = false;
            for y in 0..px {
                for x in 0..px {
                    let p = at(x, y);
                    eye |= p[3] >= 250 && p[2] > 180 && p[1] > 170 && p[0] < 175;
                }
            }
            assert!(eye, "{px} px has no yellow eye");
            unsafe {
                let _ = DeleteObject(info.hbmColor);
                let _ = DeleteObject(info.hbmMask);
                let _ = DestroyIcon(icon);
            }
        }
        assert!(!crate::window::app_icon(true).is_invalid() && !crate::window::app_icon(false).is_invalid());
    }

    /// Every Halo Pro line is a plain item that opens Settings › Halo Pro:
    /// none greyed, so hover and keyboard focus keep the menu's own
    /// readable colours (high contrast too).
    #[test]
    fn halo_pros_lines_are_readable_items_that_open_its_settings() {
        for pro in [false, true] {
            let items = menu_items(&Settings { pro, ..Settings::default() }, false, false, UpdateLine::None);
            let Some(MenuItem::Sub { items, .. }) = items.into_iter().find(|i| matches!(i, MenuItem::Sub { label, .. } if label == "Halo Pro")) else {
                panic!("no Halo Pro submenu");
            };
            let lines: Vec<(String, MenuCmd)> = items
                .into_iter()
                .filter_map(|i| match i {
                    MenuItem::Item { label, cmd, checked } => {
                        assert!(!checked);
                        Some((label, cmd))
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(lines.len(), 1 + PRO_LINES.len());
            assert_eq!(lines[0].0, if pro { "Halo Pro is on…" } else { "Activate Halo Pro…" });
            assert!(lines.iter().all(|(_, cmd)| *cmd == MenuCmd::ShowPro));
        }
    }

    #[test]
    fn the_menu_says_which_version_this_is() {
        let items = menu_items(&Settings::default(), false, false, UpdateLine::None);
        let labels: Vec<(String, MenuCmd)> = items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, cmd, .. } => Some((label, cmd)),
                _ => None,
            })
            .collect();
        let at = |cmd: MenuCmd| labels.iter().position(|l| l.1 == cmd);
        let about = at(MenuCmd::About).expect("no About line");
        assert!(labels[about].0.starts_with(&format!("About Halo Island {}", env!("CARGO_PKG_VERSION"))), "{}", labels[about].0);
        // Right above Quit, with the lyrics credit kept above it.
        assert_eq!(at(MenuCmd::Quit), Some(about + 1));
        assert_eq!(at(MenuCmd::LyricsCredit), Some(about - 1));
    }

    #[test]
    fn a_new_version_comes_first_until_it_is_installed() {
        let s = Settings::default();
        let first = |update: UpdateLine| match menu_items(&s, false, true, update).into_iter().next() {
            Some(MenuItem::Item { label, cmd, .. }) => Some((label, cmd)),
            _ => None,
        };
        assert_eq!(first(UpdateLine::Offer("0.2.5".into())), Some(("Update to Halo Island 0.2.5…".to_string(), MenuCmd::Update)));
        assert_eq!(first(UpdateLine::None).map(|(_, cmd)| cmd), Some(MenuCmd::RecordScreen));
        assert_eq!(first(UpdateLine::Check).map(|(_, cmd)| cmd), Some(MenuCmd::RecordScreen));
    }

    #[test]
    fn check_for_updates_sits_under_the_version_line() {
        let cmds = |update: UpdateLine| -> Vec<MenuCmd> {
            menu_items(&Settings::default(), false, false, update)
                .into_iter()
                .filter_map(|i| match i {
                    MenuItem::Item { cmd, .. } => Some(cmd),
                    _ => None,
                })
                .collect()
        };
        let items = cmds(UpdateLine::Check);
        let about = items.iter().position(|c| *c == MenuCmd::About).expect("no About line");
        assert_eq!(&items[about..], &[MenuCmd::About, MenuCmd::CheckForUpdates, MenuCmd::Quit]);
        // Not while a version is on offer (its line comes first), nor with the checks off.
        assert!(!cmds(UpdateLine::Offer("0.2.5".into())).contains(&MenuCmd::CheckForUpdates));
        assert!(!cmds(UpdateLine::None).contains(&MenuCmd::CheckForUpdates));
    }

    #[test]
    fn the_way_back_to_the_centre_shows_once_the_island_moved() {
        let cmds = |s: &Settings| -> Vec<MenuCmd> {
            menu_items(s, false, false, UpdateLine::None)
                .into_iter()
                .filter_map(|i| match i {
                    MenuItem::Item { cmd, .. } => Some(cmd),
                    _ => None,
                })
                .collect()
        };
        assert!(!cmds(&Settings::default()).contains(&MenuCmd::CentreIsland));
        let moved = cmds(&Settings { island_x: Some(0.8), ..Settings::default() });
        let pin = moved.iter().position(|c| *c == MenuCmd::TogglePinned).expect("no Keep island open");
        assert_eq!(moved.get(pin + 1), Some(&MenuCmd::CentreIsland));
    }

    #[test]
    fn halo_mascot_is_a_checked_item_under_idle_look() {
        let at = |s: &Settings| {
            let items = menu_items(s, false, false, UpdateLine::None);
            let i = items.iter().position(|i| matches!(i, MenuItem::Item { cmd: MenuCmd::ToggleMascot, .. })).expect("no Halo Mascot");
            assert!(matches!(&items[i - 1], MenuItem::Sub { label, .. } if label == "Idle look"));
            matches!(&items[i], MenuItem::Item { checked: true, .. })
        };
        assert!(at(&Settings::default()), "on by default");
        assert!(!at(&Settings { mascot: false, ..Settings::default() }));
    }

    #[test]
    fn system_history_is_in_the_menu_only_while_system_is_on() {
        let has = |s: &Settings| menu_items(s, false, false, UpdateLine::None).iter().position(|i| matches!(i, MenuItem::Item { cmd: MenuCmd::ToggleSystemHistory, .. }));
        let mut s = Settings::default();
        assert_eq!(has(&s), None, "System is off by default");
        s.set_addon(halo_core::addons::SYSTEM.id, true);
        assert_eq!(has(&s), None, "the long history is Halo Pro's");
        s.pro = true;
        let at = has(&s).expect("no System history item");
        // Right above Sounds, where it always was.
        let items = menu_items(&s, false, false, UpdateLine::None);
        assert!(matches!(&items[at + 1], MenuItem::Item { cmd: MenuCmd::ToggleSounds, .. }));
    }

    #[test]
    fn record_this_screen_shows_only_with_the_recorder() {
        let s = Settings::default();
        let first = |recorder: bool| match menu_items(&s, false, recorder, UpdateLine::None).into_iter().next() {
            Some(MenuItem::Item { label, cmd, .. }) => Some((label, cmd)),
            _ => None,
        };
        assert_eq!(first(true), Some(("Record this screen".to_string(), MenuCmd::RecordScreen)));
        assert_ne!(first(false).map(|(_, cmd)| cmd), Some(MenuCmd::RecordScreen));
    }

    fn idle_choices(s: &Settings) -> Vec<(String, bool)> {
        let items = menu_items(s, false, false, UpdateLine::None);
        let Some(MenuItem::Sub { items, .. }) = items.into_iter().find(|i| matches!(i, MenuItem::Sub { label, .. } if label == "Idle look")) else {
            panic!("no idle look submenu");
        };
        items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, checked, .. } => Some((label, checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn status_bar_is_listed_only_in_the_pill_theme() {
        let pill = Settings { theme: Theme::Pill, idle_style: IdleStyle::Status, ..Settings::default() };
        let labels: Vec<String> = idle_choices(&pill).into_iter().map(|c| c.0).collect();
        assert_eq!(labels, ["Clock", "Eyes", "Minimal", "Hidden until hover", "Status bar"]);
        assert_eq!(idle_choices(&pill).last(), Some(&("Status bar".to_string(), true)));
        // Another theme draws the clock for the status bar: Clock is checked,
        // and Status comes back with Pill.
        for theme in [Theme::Classic, Theme::Halo, Theme::Glass, Theme::Diorama] {
            let s = Settings { theme, ..pill.clone() };
            let choices = idle_choices(&s);
            assert_eq!(choices.len(), if theme == Theme::Diorama { 3 } else { 4 }, "{theme:?}");
            assert_eq!(choices[0], ("Clock".to_string(), true), "{theme:?}");
            assert!(choices.iter().skip(1).all(|c| !c.1), "{theme:?}");
            assert!(choices.iter().all(|c| c.0 != "Status bar"), "{theme:?}");
        }
        // Diorama's looks by name: its picture with the time, the picture
        // alone, and nothing until the pointer comes. It has no eyes: a
        // setting left on Eyes shows the picture alone, so Minimal is checked.
        let diorama = Settings { theme: Theme::Diorama, idle_style: IdleStyle::Minimal, ..Settings::default() };
        let looks = [("Clock".to_string(), false), ("Minimal".to_string(), true), ("Hidden until hover".to_string(), false)];
        assert_eq!(idle_choices(&diorama), looks);
        assert_eq!(idle_choices(&Settings { idle_style: IdleStyle::Eyes, ..diorama }), looks);
    }

    /// Where a theme comes in the menu. No `_` arm: a theme added to the
    /// enum stops this compiling until it has a place, and then the test
    /// below asks the menu for it.
    fn place(t: Theme) -> usize {
        match t {
            Theme::Classic => 0,
            Theme::Halo => 1,
            Theme::Glass => 2,
            Theme::Pill => 3,
            Theme::Diorama => 4,
        }
    }

    fn theme_choices(s: &Settings) -> Vec<(String, MenuCmd, bool)> {
        let items = menu_items(s, false, false, UpdateLine::None);
        let Some(MenuItem::Sub { items, .. }) = items.into_iter().find(|i| matches!(i, MenuItem::Sub { label, .. } if label == "Theme")) else {
            panic!("no theme submenu");
        };
        items
            .into_iter()
            .filter_map(|i| match i {
                MenuItem::Item { label, cmd, checked } => Some((label, cmd, checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_theme_submenu_has_every_theme_and_checks_the_one_in_use() {
        let names = ["Classic", "Halo", "Glass", "Pill", "Diorama"];
        // Without Pro, its themes say so behind their names.
        let free = theme_choices(&Settings::default());
        assert_eq!(free.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["Classic", "Halo", "Glass (Halo Pro)", "Pill (Halo Pro)", "Diorama (Halo Pro)"]);
        let choices = theme_choices(&Settings { pro: true, ..Settings::default() });
        assert_eq!(choices.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), names);
        for (i, (label, cmd, _)) in choices.iter().enumerate() {
            let MenuCmd::SetTheme(theme) = *cmd else { panic!("{label} doesn't set a theme") };
            assert_eq!(place(theme), i, "{label}");
            // Picked, it is the one checked.
            let checked: Vec<bool> = theme_choices(&Settings { theme, ..Settings::default() }).into_iter().map(|c| c.2).collect();
            assert_eq!(checked, (0..names.len()).map(|j| j == i).collect::<Vec<_>>(), "{label}");
        }
    }

    #[test]
    fn start_with_windows_follows_the_full_screen_submenu() {
        // Over maximized windows (Step aside) is gone: nothing sits between them.
        let items = menu_items(&Settings::default(), false, false, UpdateLine::None);
        let fs = items.iter().position(|i| matches!(i, MenuItem::Sub { label, .. } if label == "In full-screen apps")).expect("full-screen submenu");
        assert!(matches!(items.get(fs + 1), Some(MenuItem::Item { cmd: MenuCmd::ToggleAutostart, .. })));
    }

    #[test]
    fn full_screen_submenu_checks_the_current_choice() {
        let mut s = Settings::default();
        for hide in [true, false] {
            s.hide_in_fullscreen = hide;
            let choices = fullscreen_choices(&s);
            assert_eq!(
                choices,
                vec![
                    ("Hide".to_string(), MenuCmd::SetHideFullscreen(true), hide),
                    ("Stay on top".to_string(), MenuCmd::SetHideFullscreen(false), !hide),
                    ("Show FPS, GPU and CPU over games (Halo Pro)".to_string(), MenuCmd::ToggleGameStats, false),
                    ("Only for a few seconds (no added delay)".to_string(), MenuCmd::ToggleGamePeek, false),
                ]
            );
        }
    }
}

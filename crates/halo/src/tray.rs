//! Notification-area icon and its menu (also shown on right-click of the island).

use crate::events::WM_APP_TRAY;
use crate::updates::UpdateLine;
use halo_core::settings::Settings;
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
    ToggleSystemHistory,
    TogglePronunciation,
    ToggleSounds,
    TogglePinned,
    /// "Move island to the centre": back from where it was dragged.
    CentreIsland,
    /// "Advanced settings…": the Settings window.
    ShowSettings,
    /// A line under Halo Pro: the Settings window on Halo Pro (the key).
    ShowPro,
    /// "What's New": the island's About sheet with this version's lines.
    WhatsNew,
    /// "About Halo Island 0.2.3 (…)": the island's About sheet, not a URL.
    About,
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

/// `update`: a later version the update file offers, or Check for updates…
/// (`crate::updates::menu_line`).
pub fn menu_items(s: &Settings, pinned: bool, recorder: bool, update: UpdateLine) -> Vec<MenuItem> {
    let mut items = vec![
        item("Advanced settings…", MenuCmd::ShowSettings, false),
        MenuItem::Separator,
        item("Keep island open", MenuCmd::TogglePinned, pinned),
        MenuItem::Separator,
        item("Lyrics pronunciation (romaji, pinyin, Korean)", MenuCmd::TogglePronunciation, s.lyrics_pronunciation),
        item("Sounds", MenuCmd::ToggleSounds, s.sounds),
        MenuItem::Separator,
        item("What's New", MenuCmd::WhatsNew, false),
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
        let launches = matches!(cmd, Some(MenuCmd::ShowSettings | MenuCmd::ShowPro | MenuCmd::About));
        if !launches && !prev.is_invalid() && prev != owner && IsWindow(prev).as_bool() && GetForegroundWindow() == owner {
            let _ = SetForegroundWindow(prev);
        }
        cmd
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Right above Quit, under What's New.
        assert_eq!(at(MenuCmd::Quit), Some(about + 1));
        assert_eq!(at(MenuCmd::WhatsNew), Some(about - 1));
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
}

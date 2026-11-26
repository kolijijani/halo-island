//! System Tray notification area host (`Shell_TrayWnd` proxy).
//!
//! When the Windows taskbar is hidden, third-party notification area icons
//! (Discord, Steam, OneDrive, Slack, etc.) would otherwise be unreachable.
//! While the `^` Background Apps flyout is open, we register a temporary
//! top-most `Shell_TrayWnd` above Explorer's and broadcast `TaskbarCreated`
//! so running apps re-send their `Shell_NotifyIcon` (`WM_COPYDATA`) state to us.
//!
//! Hosting runs *only* while the flyout is open (0% CPU, 0 threads, 0 timers at
//! rest, and avoids breaking Windows App SDK's `IsShownInSwitchers` check when
//! idle). Closing the flyout destroys the proxy window and broadcasts
//! `TaskbarCreated` again so all icons return to Explorer immediately.

use std::cell::RefCell;

use crate::gfx::{Image, ImageData};
use windows::core::{w, GUID, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBRUSH, HDC,
};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const WM_TRAY_HOST_DATA: u32 = WM_APP + 0x42;

pub const WM_MOUSEMOVE_MSG: u32 = 0x0200;
pub const WM_LBUTTONDOWN_MSG: u32 = 0x0201;
pub const WM_LBUTTONUP_MSG: u32 = 0x0202;
pub const WM_RBUTTONDOWN_MSG: u32 = 0x0204;
pub const WM_RBUTTONUP_MSG: u32 = 0x0205;
pub const WM_CONTEXTMENU_MSG: u32 = 0x007B;
pub const NIN_SELECT_MSG: u32 = 0x0400;
pub const NIN_POPUPOPEN_MSG: u32 = 0x0406;
pub const NIN_POPUPCLOSE_MSG: u32 = 0x0407;

#[derive(Clone)]
pub struct TrayIcon {
    pub hwnd: u32,
    pub uid: u32,
    pub guid: Option<GUID>,
    pub callback: u32,
    pub version: u32,
    pub hicon: u32,
    pub tip: String,
    pub name: String,
    pub hidden: bool,
    pub image: Option<Image>,
    pub place: RECT,
}

struct Host {
    hwnd: HWND,
    icons: Vec<TrayIcon>,
    notify: HWND,
}

thread_local! {
    static HOST: RefCell<Option<Host>> = const { RefCell::new(None) };
}

const TIMER_TOP: usize = 1;

fn taskbar_created_msg() -> u32 {
    unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }
}

fn broadcast_taskbar_created() {
    unsafe {
        let _ = SendNotifyMessageW(HWND_BROADCAST, taskbar_created_msg(), WPARAM(0), LPARAM(0));
    }
}

/// Returns the real Explorer `Shell_TrayWnd` and its owning shell PID, never our proxy window.
pub fn explorer_taskbar() -> Option<(HWND, u32)> {
    unsafe {
        let shell = GetShellWindow();
        if shell.0.is_null() {
            return None;
        }
        let mut shell_pid = 0u32;
        if GetWindowThreadProcessId(shell, Some(&mut shell_pid)) == 0 || shell_pid == 0 {
            return None;
        }
        let mut h = HWND::default();
        loop {
            h = FindWindowExW(HWND::default(), h, w!("Shell_TrayWnd"), None).ok()?;
            if h.0.is_null() {
                return None;
            }
            let mut pid = 0u32;
            if GetWindowThreadProcessId(h, Some(&mut pid)) != 0 && pid == shell_pid {
                return Some((h, shell_pid));
            }
        }
    }
}

pub fn running() -> bool {
    HOST.with(|h| h.try_borrow().map(|h| h.is_some()).unwrap_or(true))
}

pub fn owns_window(hwnd: HWND) -> bool {
    HOST.with(|cell| {
        cell.try_borrow()
            .ok()
            .is_some_and(|h| h.as_ref().is_some_and(|h| h.hwnd == hwnd))
    })
}

/// Start hosting the notification area while the tray flyout is open.
/// `notify` receives `WM_TRAY_HOST_DATA` whenever the icon list changes.
pub fn start(notify: HWND) -> bool {
    if running() {
        HOST.with(|h| {
            if let Ok(mut h) = h.try_borrow_mut() {
                if let Some(h) = h.as_mut() {
                    h.notify = notify;
                }
            }
        });
        return true;
    }
    unsafe {
        if explorer_taskbar().is_none() {
            return false;
        }
        let Ok(hinst) = GetModuleHandleW(None) else {
            return false;
        };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            lpszClassName: w!("Shell_TrayWnd"),
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            w!("Shell_TrayWnd"),
            w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            HWND::default(),
            HMENU::default(),
            hinst,
            None,
        ) else {
            return false;
        };
        let _ = ChangeWindowMessageFilterEx(hwnd, WM_COPYDATA, MSGFLT_ALLOW, None);
        let _ = ChangeWindowMessageFilterEx(hwnd, taskbar_created_msg(), MSGFLT_ALLOW, None);
        HOST.with(|h| {
            *h.borrow_mut() = Some(Host {
                hwnd,
                icons: Vec::new(),
                notify,
            });
        });
        let _ = SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        // Keep proxy above Explorer's Shell_TrayWnd ONLY while flyout is open.
        let _ = SetTimer(hwnd, TIMER_TOP, 500, None);
        broadcast_taskbar_created();
    }
    true
}

/// Stop hosting and return all notification icons to Explorer.
pub fn stop() {
    let host = HOST.with(|h| h.borrow_mut().take());
    if let Some(h) = host {
        unsafe {
            let _ = KillTimer(h.hwnd, TIMER_TOP);
            let _ = DestroyWindow(h.hwnd);
        }
        broadcast_taskbar_created();
    }
}

pub fn with_icons<R>(f: impl FnOnce(Option<&mut Vec<TrayIcon>>) -> R) -> R {
    HOST.with(|h| match h.try_borrow_mut() {
        Ok(mut h) => f(h.as_mut().map(|h| &mut h.icons)),
        Err(_) => f(None),
    })
}

pub(crate) fn wstr(b: &[u8], off: usize, chars: usize) -> String {
    let end = (off + chars * 2).min(b.len());
    let v: Vec<u16> = b[off.min(end)..end]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&c| c != 0)
        .collect();
    String::from_utf16_lossy(&v)
}

pub(crate) fn u32_at(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .unwrap_or(0)
}

pub(crate) fn guid_at(b: &[u8], o: usize) -> Option<GUID> {
    let s = b.get(o..o + 16)?;
    let g = GUID::from_values(
        u32::from_le_bytes(s[0..4].try_into().ok()?),
        u16::from_le_bytes(s[4..6].try_into().ok()?),
        u16::from_le_bytes(s[6..8].try_into().ok()?),
        s[8..16].try_into().ok()?,
    );
    (g != GUID::zeroed()).then_some(g)
}

/// Rasterize an `HICON` into premultiplied BGRA [`Image`] by drawing on black
/// and white backdrops to recover exact per-pixel alpha for both 32-bit and masked icons.
pub fn icon_image(hicon: u32, px: u32) -> Option<Image> {
    if hicon == 0 || px == 0 {
        return None;
    }
    let h = HICON(hicon as usize as *mut _);
    let draw = |bg: u8| -> Option<Vec<u8>> {
        unsafe {
            let dc = CreateCompatibleDC(HDC::default());
            if dc.0.is_null() {
                return None;
            }
            let bmi = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: px as i32,
                    biHeight: -(px as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let Ok(bmp) = CreateDIBSection(dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
            else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, bmp);
            std::ptr::write_bytes(bits as *mut u8, bg, (px * px * 4) as usize);
            let ok = DrawIconEx(dc, 0, 0, h, px as i32, px as i32, 0, HBRUSH::default(), DI_NORMAL).is_ok();
            let _ = GdiFlush();
            let v =
                std::slice::from_raw_parts(bits as *const u8, (px * px * 4) as usize).to_vec();
            let _ = SelectObject(dc, old);
            let _ = DeleteObject(bmp);
            let _ = DeleteDC(dc);
            ok.then_some(v)
        }
    };
    let (b, wh) = (draw(0)?, draw(255)?);
    let mut out = vec![0u8; b.len()];
    for i in (0..b.len()).step_by(4) {
        let d = (0..3)
            .map(|k| wh[i + k] as i32 - b[i + k] as i32)
            .max()
            .unwrap_or(0);
        let a = (255 - d).clamp(0, 255) as u8;
        for k in 0..3 {
            out[i + k] = b[i + k].min(a);
        }
        out[i + 3] = a;
    }
    Some(ImageData::new(px, px, out))
}

pub(crate) fn is_live_window(hwnd: u32) -> bool {
    hwnd != 0 && unsafe { IsWindow(HWND(hwnd as usize as *mut _)).as_bool() }
}

fn process_image_path(hwnd: u32) -> Option<String> {
    unsafe {
        let h = HWND(hwnd as usize as *mut _);
        let mut pid = 0u32;
        if GetWindowThreadProcessId(h, Some(&mut pid)) == 0 || pid == 0 {
            return None;
        }
        let proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(
            proc,
            PROCESS_NAME_WIN32,
            PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
        .is_ok();
        let _ = CloseHandle(proc);
        if !ok || len == 0 {
            return None;
        }
        Some(String::from_utf16_lossy(&buf[..len as usize]))
    }
}

fn app_name(hwnd: u32) -> String {
    let Some(path) = process_image_path(hwnd) else {
        return String::new();
    };
    let stem = std::path::Path::new(&path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

fn fallback_icon_image(hwnd: u32) -> Option<Image> {
    if !is_live_window(hwnd) {
        return None;
    }
    let path = process_image_path(hwnd);
    crate::services::dock_apps::extract_app_icon(HWND(hwnd as usize as *mut _), path.as_deref(), None)
}

/// Parse `Shell_NotifyIcon` `WM_COPYDATA` payload (`dwData == 1`).
pub(crate) fn on_notify_slice(icons: &mut Vec<TrayIcon>, b: &[u8], decode_icon: bool) -> bool {
    if b.len() < 8 + 24 {
        return false;
    }
    let msg = u32_at(b, 4);
    let n = &b[8..];
    let (hwnd, uid, flags) = (u32_at(n, 4), u32_at(n, 8), u32_at(n, 12));
    let guid = if flags & 0x20 != 0 {
        guid_at(n, 936)
    } else {
        None
    };
    let pos = icons.iter().position(|i| match (guid, i.guid) {
        (Some(g), Some(ig)) => g == ig,
        _ => i.hwnd == hwnd && i.uid == uid,
    });
    match msg {
        0 | 1 => {
            // NIM_ADD / NIM_MODIFY
            let i = match pos {
                Some(p) => p,
                None => {
                    icons.push(TrayIcon {
                        hwnd,
                        uid,
                        guid,
                        callback: 0,
                        version: 0,
                        hicon: 0,
                        tip: String::new(),
                        name: if decode_icon {
                            app_name(hwnd)
                        } else {
                            String::new()
                        },
                        hidden: false,
                        image: None,
                        place: RECT::default(),
                    });
                    icons.len() - 1
                }
            };
            let ic = &mut icons[i];
            if flags & 1 != 0 {
                ic.callback = u32_at(n, 16);
            }
            if flags & 2 != 0 {
                let hicon = u32_at(n, 20);
                if hicon != ic.hicon || ic.image.is_none() {
                    ic.hicon = hicon;
                    if decode_icon {
                        ic.image = icon_image(hicon, 32);
                    }
                }
            }
            if decode_icon && ic.image.is_none() {
                ic.image = fallback_icon_image(ic.hwnd);
            }
            if flags & 4 != 0 {
                ic.tip = wstr(n, 24, 128);
            }
            if flags & 8 != 0 {
                let (state, mask) = (u32_at(n, 280), u32_at(n, 284));
                if mask & 1 != 0 {
                    ic.hidden = state & 1 != 0;
                }
            }
            true
        }
        2 => {
            // NIM_DELETE
            if let Some(p) = pos {
                icons.remove(p);
                true
            } else {
                false
            }
        }
        4 => {
            // NIM_SETVERSION
            if let Some(p) = pos {
                icons[p].version = u32_at(n, 800);
                true
            } else {
                false
            }
        }
        _ => pos.is_some(),
    }
}

/// Handle `Shell_NotifyIconGetRect` (`dwData == 3`).
pub(crate) fn on_identify_slice(icons: &[TrayIcon], b: &[u8]) -> isize {
    let (msg, hwnd, uid, guid) = (u32_at(b, 4), u32_at(b, 16), u32_at(b, 20), guid_at(b, 24));
    let Some(ic) = icons.iter().find(|i| match (guid, i.guid) {
        (Some(g), Some(ig)) => g == ig,
        _ => i.hwnd == hwnd && i.uid == uid,
    }) else {
        return 0;
    };
    let r = ic.place;
    let mk = |x: i32, y: i32| ((x & 0xFFFF) | ((y & 0xFFFF) << 16)) as isize;
    match msg {
        1 => mk(r.left, r.top),
        2 => mk(r.right, r.bottom),
        _ => 0,
    }
}

unsafe extern "system" fn wndproc(h: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_COPYDATA => {
            let cds = &*(lp.0 as *const COPYDATASTRUCT);
            let bytes = if cds.lpData.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize)
            };
            let r = HOST.with(|host| {
                let Ok(mut g) = host.try_borrow_mut() else {
                    return None;
                };
                let host = g.as_mut()?;
                match cds.dwData {
                    1 => {
                        let ok = on_notify_slice(&mut host.icons, bytes, true);
                        if ok && !host.notify.0.is_null() {
                            let _ = PostMessageW(
                                host.notify,
                                WM_TRAY_HOST_DATA,
                                WPARAM(0),
                                LPARAM(0),
                            );
                        }
                        Some(LRESULT(ok as isize))
                    }
                    3 => Some(LRESULT(on_identify_slice(&host.icons, bytes))),
                    _ => {
                        drop(g);
                        let Some((ex, _)) = explorer_taskbar() else {
                            return Some(LRESULT(0));
                        };
                        let mut result = 0usize;
                        if SendMessageTimeoutW(
                            ex,
                            WM_COPYDATA,
                            wp,
                            lp,
                            SMTO_ABORTIFHUNG,
                            200,
                            Some(&mut result),
                        )
                        .0 == 0
                        {
                            Some(LRESULT(0))
                        } else {
                            Some(LRESULT(result as isize))
                        }
                    }
                }
            });
            return r.unwrap_or(LRESULT(0));
        }
        WM_TIMER => {
            let _ = SetWindowPos(
                h,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        _ => {}
    }
    DefWindowProcW(h, msg, wp, lp)
}

/// Forward mouse input to a notification area icon using Explorer's protocol
/// (`NOTIFYICON_VERSION_4` packs screen cursor coordinates into `wParam`).
pub fn send_mouse(ic: &TrayIcon, mouse_msg: u32) {
    if ic.callback == 0 {
        return;
    }
    let mut p = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
        let target = HWND(ic.hwnd as usize as *mut _);
        let mut pid = 0u32;
        GetWindowThreadProcessId(target, Some(&mut pid));
        if pid != 0 {
            let _ = AllowSetForegroundWindow(pid);
        }
        let (wp, lp) = if ic.version >= 4 {
            (
                ((p.x & 0xFFFF) | ((p.y & 0xFFFF) << 16)) as usize,
                ((mouse_msg & 0xFFFF) | ((ic.uid & 0xFFFF) << 16)) as isize,
            )
        } else {
            (ic.uid as usize, mouse_msg as isize)
        };
        let _ = SendNotifyMessageW(target, ic.callback, WPARAM(wp), LPARAM(lp));
    }
}

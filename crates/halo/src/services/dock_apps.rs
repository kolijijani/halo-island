//! Running-apps watcher for the bottom dock (`dock`).
//!
//! Uses `SetWinEventHook` (`WINEVENT_OUTOFCONTEXT`) on the UI thread — zero
//! background threads, zero polling timers, and completely unhooked when the
//! dock is turned off.

use crate::events::{AppEvent, EventSink};
use crate::gfx::{Image, ImageData};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use windows::core::PWSTR;
use windows::Win32::Foundation::{BOOL, CloseHandle, HMODULE, HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, BI_RGB, BITMAPINFO, BITMAPINFOHEADER,
    DIB_RGB_COLORS, HGDIOBJ,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::*;

#[link(name = "user32")]
unsafe extern "system" {
    fn PrivateExtractIconsW(
        szfilename: *const u16,
        niconindex: i32,
        cxicon: i32,
        cyicon: i32,
        phicon: *mut HICON,
        piconid: *mut u32,
        nicons: u32,
        flags: u32,
    ) -> u32;
}

const ICON_PX: u32 = 128;
const MAX_ICON_CACHE: usize = 64;
const MAX_APPS: usize = 24;

#[derive(Clone, Debug)]
pub struct RunningApp {
    pub key: String,
    pub name: String,
    pub exe: Option<String>,
    pub hwnd: HWND,
    pub minimized: bool,
    pub focused: bool,
    pub icon: Option<Image>,
    pub thumb: Option<Image>,
    pub badge: u16,
    pub progress: Option<f32>,
}

impl RunningApp {
    pub fn visual_eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.name == other.name
            && self.exe == other.exe
            && self.hwnd == other.hwnd
            && self.minimized == other.minimized
            && self.focused == other.focused
            && self.icon.as_ref().map(|i| i.id) == other.icon.as_ref().map(|i| i.id)
            && self.thumb.as_ref().map(|i| i.id) == other.thumb.as_ref().map(|i| i.id)
            && self.badge == other.badge
            && self.progress.map(|p| (p * 100.0).round() as i32)
                == other.progress.map(|p| (p * 100.0).round() as i32)
    }
}

struct HookState {
    sink: EventSink,
    pending: Cell<bool>,
}

thread_local! {
    static HOOK: RefCell<Option<HookState>> = const { RefCell::new(None) };
}

pub struct DockAppsWatcher {
    hooks: Vec<HWINEVENTHOOK>,
    icons: HashMap<String, Option<Image>>,
    order: Vec<String>,
    self_pid: u32,
}

impl DockAppsWatcher {
    /// Start watching top-level window lifecycle and foreground changes on the UI thread.
    pub fn start(sink: EventSink) -> Self {
        HOOK.with(|h| {
            *h.borrow_mut() = Some(HookState { sink, pending: Cell::new(false) });
        });
        let ranges = [
            (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
            (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
            (EVENT_OBJECT_CREATE, EVENT_OBJECT_HIDE),
        ];
        let mut hooks = Vec::with_capacity(ranges.len());
        for (min, max) in ranges {
            let hh = unsafe {
                SetWinEventHook(min, max, HMODULE::default(), Some(on_win_event), 0, 0, WINEVENT_OUTOFCONTEXT)
            };
            if !hh.is_invalid() {
                hooks.push(hh);
            }
        }
        Self {
            hooks,
            icons: HashMap::new(),
            order: Vec::new(),
            self_pid: unsafe { GetCurrentProcessId() },
        }
    }

    /// Enumerate taskbar-eligible running apps in stable dock order.
    pub fn refresh(&mut self) -> Vec<RunningApp> {
        HOOK.with(|h| {
            if let Some(st) = h.borrow().as_ref() {
                st.pending.set(false);
            }
        });

        let fg = unsafe {
            let raw = GetForegroundWindow();
            if raw.is_invalid() {
                raw
            } else {
                let root = GetAncestor(raw, GA_ROOT);
                if root.is_invalid() { raw } else { root }
            }
        };

        struct Collect {
            self_pid: u32,
            raw: Vec<(HWND, String, String, Option<String>, bool)>,
        }

        unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
            let ctx = unsafe { &mut *(lp.0 as *mut Collect) };
            if let Some(item) = inspect_window(hwnd, ctx.self_pid) {
                ctx.raw.push(item);
            }
            BOOL(1)
        }

        let mut ctx = Collect { self_pid: self.self_pid, raw: Vec::new() };
        unsafe {
            let _ = EnumWindows(Some(enum_cb), LPARAM(&mut ctx as *mut Collect as isize));
        }

        // Group multiple windows of the same executable into one dock item.
        let mut by_key: HashMap<String, RunningApp> = HashMap::new();
        for (hwnd, key, name, exe_path, minimized) in ctx.raw {
            let is_fg = hwnd == fg;
            if let Some(existing) = by_key.get_mut(&key) {
                if is_fg {
                    existing.hwnd = hwnd;
                    existing.focused = true;
                    existing.minimized = minimized;
                } else if existing.minimized && !minimized {
                    existing.hwnd = hwnd;
                    existing.minimized = false;
                }
                continue;
            }
            let icon = self.cached_icon(&key, hwnd, exe_path.as_deref());
            by_key.insert(
                key.clone(),
                RunningApp {
                    key,
                    name,
                    exe: exe_path,
                    hwnd,
                    minimized,
                    focused: is_fg,
                    icon,
                    thumb: None,
                    badge: 0,
                    progress: None,
                },
            );
        }

        // Maintain stable left-to-right ordering: keep existing apps in place,
        // drop closed ones, append newly opened apps at the end.
        self.order.retain(|k| by_key.contains_key(k));
        for k in by_key.keys() {
            if !self.order.iter().any(|o| o == k) {
                self.order.push(k.clone());
            }
        }

        self.order
            .iter()
            .filter_map(|k| by_key.remove(k))
            .take(MAX_APPS)
            .collect()
    }

    fn cached_icon(&mut self, key: &str, hwnd: HWND, exe_path: Option<&str>) -> Option<Image> {
        if let Some(hit) = self.icons.get(key) {
            return hit.clone();
        }
        if self.icons.len() >= MAX_ICON_CACHE {
            self.icons.clear();
        }
        let img = extract_app_icon(hwnd, exe_path);
        self.icons.insert(key.to_string(), img.clone());
        img
    }
}

impl Drop for DockAppsWatcher {
    fn drop(&mut self) {
        for h in self.hooks.drain(..) {
            unsafe {
                let _ = UnhookWinEvent(h);
            }
        }
        HOOK.with(|h| {
            *h.borrow_mut() = None;
        });
    }
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_invalid() {
        return;
    }
    let is_obj = matches!(
        event,
        EVENT_OBJECT_CREATE | EVENT_OBJECT_DESTROY | EVENT_OBJECT_SHOW | EVENT_OBJECT_HIDE
    );
    if is_obj && (id_object != OBJID_WINDOW.0 || id_child != 0) {
        return;
    }
    // Ignore child controls when still alive.
    if event != EVENT_OBJECT_DESTROY {
        let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
        if !root.is_invalid() && root != hwnd {
            return;
        }
    }
    HOOK.with(|h| {
        if let Some(st) = h.borrow().as_ref() {
            if !st.pending.replace(true) {
                st.sink.send(AppEvent::DockAppsChanged);
            }
        }
    });
}

/// Bring a running app window to the foreground, restoring it if minimized.
pub fn focus_window(hwnd: HWND) {
    unsafe {
        if hwnd.is_invalid() || !IsWindow(hwnd).as_bool() {
            return;
        }
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
    }
}

fn inspect_window(hwnd: HWND, self_pid: u32) -> Option<(HWND, String, String, Option<String>, bool)> {
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 || pid == self_pid {
            return None;
        }

        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
        if (ex & WS_EX_TOOLWINDOW.0) != 0 && (ex & WS_EX_APPWINDOW.0) == 0 {
            return None;
        }
        if (ex & WS_EX_NOACTIVATE.0) != 0 {
            return None;
        }
        if let Ok(owner) = GetWindow(hwnd, GW_OWNER) {
            if !owner.is_invalid() && (ex & WS_EX_APPWINDOW.0) == 0 {
                return None;
            }
        }

        // Skip cloaked UWP / other-virtual-desktop windows.
        let mut cloaked = 0u32;
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        )
        .is_ok()
            && cloaked != 0
        {
            return None;
        }

        let title_len = GetWindowTextLengthW(hwnd);
        if title_len <= 0 {
            return None;
        }

        let mut cls_buf = [0u16; 64];
        let cls_n = GetClassNameW(hwnd, &mut cls_buf) as usize;
        let cls = String::from_utf16_lossy(&cls_buf[..cls_n]);
        if matches!(
            cls.as_str(),
            "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Windows.UI.Core.CoreWindow"
        ) {
            return None;
        }

        let mut title_buf = vec![0u16; (title_len as usize).min(128) + 1];
        let tn = GetWindowTextW(hwnd, &mut title_buf) as usize;
        let title = String::from_utf16_lossy(&title_buf[..tn]).trim().to_string();
        if title.is_empty() || title == "Program Manager" || title == "Windows Input Experience" {
            return None;
        }

        let exe_path = process_exe_path(pid);
        let key = exe_path
            .as_ref()
            .map(|p| p.to_ascii_lowercase())
            .unwrap_or_else(|| format!("hwnd:{:#x}", hwnd.0 as usize));
        let name = exe_path
            .as_deref()
            .and_then(friendly_exe_name)
            .unwrap_or_else(|| shorten_title(&title));
        let minimized = IsIconic(hwnd).as_bool();
        Some((hwnd, key, name, exe_path, minimized))
    }
}

fn process_exe_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = vec![0u16; 512];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok || len == 0 {
            return None;
        }
        Some(OsString::from_wide(&buf[..len as usize]).to_string_lossy().into_owned())
    }
}

fn friendly_exe_name(path: &str) -> Option<String> {
    let stem = std::path::Path::new(path).file_stem()?.to_str()?.trim();
    if stem.is_empty() || stem.eq_ignore_ascii_case("ApplicationFrameHost") {
        return None;
    }
    let mut chars = stem.chars();
    let first = chars.next()?;
    let mut out = String::with_capacity(stem.len());
    for c in first.to_uppercase() {
        out.push(c);
    }
    out.push_str(chars.as_str());
    Some(out)
}

fn shorten_title(title: &str) -> String {
    let part = title.rsplit(" - ").next().unwrap_or(title).trim();
    let s = if part.is_empty() { title } else { part };
    s.chars().take(28).collect()
}

fn extract_app_icon(hwnd: HWND, exe_path: Option<&str>) -> Option<Image> {
    unsafe {
        let mut owned = false;
        let mut hicon = HICON::default();
        let mut from_hi_res = false;

        // 1. Prefer high-resolution 256×256 / 128×128 icon embedded directly in the executable
        // so Windows uses the crisp 256×256 PNG frame instead of the legacy 32×32 WM_GETICON.
        if let Some(path) = exe_path {
            if !path.to_ascii_lowercase().ends_with("applicationframehost.exe") {
                let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                for px in [256i32, 128, 64] {
                    let mut out = HICON::default();
                    let mut id = 0u32;
                    let n = PrivateExtractIconsW(
                        wide.as_ptr(),
                        0,
                        px,
                        px,
                        &mut out,
                        &mut id,
                        1,
                        0,
                    );
                    if n != 0 && n != u32::MAX && !out.is_invalid() {
                        hicon = out;
                        owned = true;
                        from_hi_res = px >= 128;
                        break;
                    }
                }
            }
        }

        // 2. Fallback to live window icon (UWP / dynamic window icons).
        if hicon.is_invalid() {
            for kind in [ICON_BIG, ICON_SMALL2, ICON_SMALL] {
                let mut res = 0usize;
                if SendMessageTimeoutW(
                    hwnd,
                    WM_GETICON,
                    WPARAM(kind as usize),
                    LPARAM(0),
                    SMTO_ABORTIFHUNG,
                    40,
                    Some(&mut res),
                )
                .0 != 0
                    && res != 0
                {
                    hicon = HICON(res as *mut _);
                    break;
                }
            }
        }

        if hicon.is_invalid() {
            let cls = GetClassLongPtrW(hwnd, GCLP_HICON);
            if cls != 0 {
                hicon = HICON(cls as *mut _);
            }
        }

        // 3. Final fallback to shell file info icon.
        if hicon.is_invalid() {
            if let Some(path) = exe_path {
                let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                let mut info = SHFILEINFOW::default();
                let ok = SHGetFileInfoW(
                    windows::core::PCWSTR(wide.as_ptr()),
                    windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES(0),
                    Some(&mut info),
                    std::mem::size_of::<SHFILEINFOW>() as u32,
                    SHGFI_ICON | SHGFI_LARGEICON,
                );
                if ok != 0 && !info.hIcon.is_invalid() {
                    hicon = info.hIcon;
                    owned = true;
                }
            }
        }

        if hicon.is_invalid() {
            return None;
        }
        let img = if from_hi_res {
            // Render at 256×256 and 2×2 box-downsample to 128×128 for supersampled sharpness.
            hicon_to_image_supersampled(hicon, ICON_PX)
        } else {
            hicon_to_image(hicon, ICON_PX)
        };
        if owned {
            let _ = DestroyIcon(hicon);
        }
        img
    }
}

fn hicon_to_image_supersampled(hicon: HICON, target: u32) -> Option<Image> {
    let raw = hicon_to_image(hicon, target * 2)?;
    let tw = target as usize;
    let sw = (target * 2) as usize;
    let mut out = vec![0u8; tw * tw * 4];
    for y in 0..tw {
        for x in 0..tw {
            let mut acc = [0u32; 4];
            for dy in 0..2 {
                for dx in 0..2 {
                    let si = ((y * 2 + dy) * sw + (x * 2 + dx)) * 4;
                    acc[0] += raw.pixels[si] as u32;
                    acc[1] += raw.pixels[si + 1] as u32;
                    acc[2] += raw.pixels[si + 2] as u32;
                    acc[3] += raw.pixels[si + 3] as u32;
                }
            }
            let di = (y * tw + x) * 4;
            out[di] = ((acc[0] + 2) / 4) as u8;
            out[di + 1] = ((acc[1] + 2) / 4) as u8;
            out[di + 2] = ((acc[2] + 2) / 4) as u8;
            out[di + 3] = ((acc[3] + 2) / 4) as u8;
        }
    }
    Some(ImageData::new(target, target, out))
}

fn hicon_to_image(hicon: HICON, size: u32) -> Option<Image> {
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32), // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let Ok(hbmp) = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) else {
            let _ = DeleteDC(hdc);
            return None;
        };
        if bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(hbmp.0));
            let _ = DeleteDC(hdc);
            return None;
        }
        let prev = SelectObject(hdc, HGDIOBJ(hbmp.0));
        let len = (size * size * 4) as usize;
        std::ptr::write_bytes(bits as *mut u8, 0, len);
        let drawn = DrawIconEx(hdc, 0, 0, hicon, size as i32, size as i32, 0, None, DI_NORMAL).is_ok();
        let mut pixels = vec![0u8; len];
        if drawn {
            std::ptr::copy_nonoverlapping(bits as *const u8, pixels.as_mut_ptr(), len);
        }
        SelectObject(hdc, prev);
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        let _ = DeleteDC(hdc);
        if !drawn {
            return None;
        }

        let has_alpha = pixels.chunks_exact(4).any(|p| p[3] != 0);
        if has_alpha {
            for p in pixels.chunks_exact_mut(4) {
                let a = p[3] as u32;
                if a < 255 {
                    p[0] = ((p[0] as u32 * a + 127) / 255) as u8;
                    p[1] = ((p[1] as u32 * a + 127) / 255) as u8;
                    p[2] = ((p[2] as u32 * a + 127) / 255) as u8;
                }
            }
        } else {
            // Legacy icon with zero alpha byte: mark non-black pixels opaque.
            let mut any = false;
            for p in pixels.chunks_exact_mut(4) {
                if p[0] != 0 || p[1] != 0 || p[2] != 0 {
                    p[3] = 255;
                    any = true;
                }
            }
            if !any {
                return None;
            }
        }
        Some(ImageData::new(size, size, pixels))
    }
}

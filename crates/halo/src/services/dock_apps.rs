//! Running-apps watcher for the bottom dock (`dock`).
//!
//! Uses `SetWinEventHook` (`WINEVENT_OUTOFCONTEXT`) on the UI thread — zero
//! background threads, zero polling timers, and completely unhooked when the
//! dock is turned off.

use crate::events::{AppEvent, EventSink};
use crate::gfx::{Image, ImageData};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_void, OsString};
use std::os::windows::ffi::OsStringExt;
use windows::core::{Interface, BSTR, PCWSTR, PWSTR};
use windows::Win32::Foundation::{BOOL, CloseHandle, HMODULE, HWND, LPARAM, SIZE, WPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetObjectW, SelectObject, BI_RGB, BITMAP,
    BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::EnhancedStorage::{
    PKEY_AppUserModel_ID, PKEY_AppUserModel_RelaunchCommand, PKEY_Link_TargetParsingPath,
};
use windows::Win32::Storage::Packaging::Appx::GetApplicationUserModelId;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows::Win32::UI::Shell::PropertiesSystem::{
    IPropertyStore, SHGetPropertyStoreForWindow, SHGetPropertyStoreFromParsingName, GPS_DEFAULT, PROPERTYKEY,
};
use windows::Win32::UI::Shell::{
    IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON,
    SHGFI_LARGEICON, SIGDN_NORMALDISPLAY, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
};
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
    pub aumid: Option<String>,
    pub launch: Option<String>,
    pub profile_directory: Option<String>,
    pub pinned: bool,
    pub running: bool,
    pub hwnd: HWND,
    pub windows: Vec<(HWND, String)>,
    pub minimized: bool,
    pub focused: bool,
    pub icon: Option<Image>,
    pub thumb: Option<Image>,
    pub badge: u16,
    pub progress: Option<f32>,
}

impl Default for RunningApp {
    fn default() -> Self {
        Self {
            key: String::new(),
            name: String::new(),
            exe: None,
            aumid: None,
            launch: None,
            profile_directory: None,
            pinned: false,
            running: true,
            hwnd: HWND::default(),
            windows: Vec::new(),
            minimized: false,
            focused: false,
            icon: None,
            thumb: None,
            badge: 0,
            progress: None,
        }
    }
}

impl PartialEq for RunningApp {
    fn eq(&self, other: &Self) -> bool {
        self.visual_eq(other)
    }
}

impl RunningApp {
    pub fn visual_eq(&self, other: &Self) -> bool {
        self.key == other.key
            && self.name == other.name
            && self.exe == other.exe
            && self.aumid == other.aumid
            && self.launch == other.launch
            && self.pinned == other.pinned
            && self.running == other.running
            && self.hwnd == other.hwnd
            && self.windows.len() == other.windows.len()
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

#[derive(Clone)]
struct PinnedMeta {
    name: String,
    exe: Option<String>,
    aumid: Option<String>,
    profile_directory: Option<String>,
    icon: Option<Image>,
}

pub struct DockAppsWatcher {
    hooks: Vec<HWINEVENTHOOK>,
    icons: HashMap<String, Option<Image>>,
    pinned_cache: HashMap<String, PinnedMeta>,
    procs: HashMap<u32, (Option<String>, Option<String>)>,
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
            pinned_cache: HashMap::new(),
            procs: HashMap::new(),
            order: Vec::new(),
            self_pid: unsafe { GetCurrentProcessId() },
        }
    }

    /// Enumerate pinned, up to 3 recent unpinned apps, and running unpinned apps in stable dock order.
    pub fn refresh_with_pinned_and_recent(
        &mut self,
        pinned_specs: &[String],
        recent_specs: &mut Vec<String>,
    ) -> Vec<RunningApp> {
        HOOK.with(|h| {
            if let Some(st) = h.borrow().as_ref() {
                st.pending.set(false);
            }
        });
        if self.procs.len() > 256 {
            self.procs.clear();
        }

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
            raw: Vec<InspectedWin>,
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

        let mut raw_wins = ctx.raw;
        for w in &mut raw_wins {
            let (p_exe, p_aumid) = self.process_info(w.pid);
            if w.exe.is_none() {
                w.exe = p_exe;
            }
            if w.aumid.is_none() {
                w.aumid = p_aumid;
            }
        }

        // 1. Build pinned items in exact user-configured order.
        let mut pinned_items: Vec<RunningApp> = Vec::with_capacity(pinned_specs.len());
        for spec in pinned_specs {
            let spec_trim = spec.trim();
            if spec_trim.is_empty() {
                continue;
            }
            let key = format!("pin:{}", spec_trim.to_ascii_lowercase());
            if pinned_items.iter().any(|p| p.key == key) {
                continue;
            }
            let meta = self.resolve_pinned_meta(&key, spec_trim);
            if meta.icon.is_none() && meta.exe.is_none() && meta.aumid.is_none() {
                continue;
            }
            pinned_items.push(RunningApp {
                key,
                name: meta.name,
                exe: meta.exe,
                aumid: meta.aumid,
                launch: Some(spec_trim.to_string()),
                profile_directory: meta.profile_directory,
                pinned: true,
                running: false,
                hwnd: HWND::default(),
                windows: Vec::new(),
                minimized: false,
                focused: false,
                icon: meta.icon,
                thumb: None,
                badge: 0,
                progress: None,
            });
        }

        // 2. Match running windows against pinned items or group into unpinned running apps.
        let mut by_key: HashMap<String, RunningApp> = HashMap::new();
        for w in raw_wins {
            let is_fg = w.hwnd == fg;
            let title_badge = badge_from_title(&w.title);
            if let Some(pin_idx) = best_pinned_match(&pinned_items, &w) {
                let pin = &mut pinned_items[pin_idx];
                if !pin.running {
                    pin.running = true;
                    pin.hwnd = w.hwnd;
                    pin.minimized = w.minimized;
                    pin.focused = is_fg;
                    if pin.exe.is_none() {
                        pin.exe = w.exe.clone();
                    }
                    if pin.icon.is_none() {
                        pin.icon = self.cached_icon(&pin.key, w.hwnd, pin.exe.as_deref(), pin.launch.as_deref());
                    }
                } else if is_fg {
                    pin.hwnd = w.hwnd;
                    pin.focused = true;
                    pin.minimized = w.minimized;
                } else if pin.minimized && !w.minimized {
                    pin.hwnd = w.hwnd;
                    pin.minimized = false;
                }
                pin.badge = pin.badge.saturating_add(title_badge);
                pin.windows.push((w.hwnd, w.title));
                continue;
            }

            let run_id = running_identity(&w);
            let key = format!("run:{run_id}");
            let launch = match (&w.aumid, &w.exe) {
                (Some(a), _) => Some(format!("shell:AppsFolder\\{a}")),
                (None, Some(e)) => Some(e.clone()),
                _ => None,
            };
            if let Some(existing) = by_key.get_mut(&key) {
                if is_fg {
                    existing.hwnd = w.hwnd;
                    existing.focused = true;
                    existing.minimized = w.minimized;
                } else if existing.minimized && !w.minimized {
                    existing.hwnd = w.hwnd;
                    existing.minimized = false;
                }
                existing.badge = existing.badge.saturating_add(title_badge);
                existing.windows.push((w.hwnd, w.title));
                continue;
            }
            let icon = self.cached_icon(&key, w.hwnd, w.exe.as_deref(), launch.as_deref());
            let name = friendly_app_name(w.exe.as_deref(), w.aumid.as_deref(), &w.title);
            by_key.insert(
                key.clone(),
                RunningApp {
                    key,
                    name,
                    exe: w.exe,
                    aumid: w.aumid,
                    launch,
                    profile_directory: w.profile_directory,
                    pinned: false,
                    running: true,
                    hwnd: w.hwnd,
                    windows: vec![(w.hwnd, w.title)],
                    minimized: w.minimized,
                    focused: is_fg,
                    icon,
                    thumb: None,
                    badge: title_badge,
                    progress: None,
                },
            );
        }

        // Maintain stable left-to-right ordering for unpinned running apps.
        self.order.retain(|k| by_key.contains_key(k));
        for k in by_key.keys() {
            if !self.order.iter().any(|o| o == k) {
                self.order.push(k.clone());
            }
        }

        let mut running_unpinned = Vec::new();
        for k in &self.order {
            if let Some(app) = by_key.remove(k) {
                if let Some(spec) = app.launch.as_ref().or(app.exe.as_ref()) {
                    recent_specs.retain(|r| !r.eq_ignore_ascii_case(spec));
                    recent_specs.insert(0, spec.clone());
                }
                running_unpinned.push(app);
            }
        }
        // Prune any recent specs that are now pinned.
        recent_specs.retain(|r| {
            !pinned_items.iter().any(|p| {
                p.launch.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(r))
                    || p.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(r))
            })
        });
        recent_specs.truncate(3);

        // Build up to 3 recent unpinned items (running if matched, otherwise idle recent).
        let mut recent_items: Vec<RunningApp> = Vec::new();
        for spec in recent_specs.iter() {
            let spec_trim = spec.trim();
            if spec_trim.is_empty() {
                continue;
            }
            if let Some(pos) = running_unpinned.iter().position(|r| {
                r.launch.as_deref().is_some_and(|l| l.eq_ignore_ascii_case(spec_trim))
                    || r.exe.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(spec_trim))
            }) {
                recent_items.push(running_unpinned.remove(pos));
                continue;
            }
            let key = format!("rec:{}", spec_trim.to_ascii_lowercase());
            let meta = self.resolve_pinned_meta(&key, spec_trim);
            if meta.icon.is_none() && meta.exe.is_none() && meta.aumid.is_none() {
                continue;
            }
            recent_items.push(RunningApp {
                key,
                name: meta.name,
                exe: meta.exe,
                aumid: meta.aumid,
                launch: Some(spec_trim.to_string()),
                profile_directory: meta.profile_directory,
                pinned: false,
                running: false,
                hwnd: HWND::default(),
                windows: Vec::new(),
                minimized: false,
                focused: false,
                icon: meta.icon,
                thumb: None,
                badge: 0,
                progress: None,
            });
        }

        let mut out = pinned_items;
        for app in recent_items.into_iter().chain(running_unpinned) {
            if out.len() >= MAX_APPS {
                break;
            }
            out.push(app);
        }
        out
    }

    fn process_info(&mut self, pid: u32) -> (Option<String>, Option<String>) {
        if let Some(c) = self.procs.get(&pid) {
            return c.clone();
        }
        let exe = process_exe_path(pid);
        let aumid = process_aumid(pid);
        let r = (exe, aumid);
        self.procs.insert(pid, r.clone());
        r
    }

    fn resolve_pinned_meta(&mut self, key: &str, spec: &str) -> PinnedMeta {
        if let Some(hit) = self.pinned_cache.get(key) {
            return hit.clone();
        }
        if self.pinned_cache.len() >= MAX_ICON_CACHE {
            self.pinned_cache.clear();
        }
        const APPS_PREFIX: &str = "shell:AppsFolder\\";
        let mut aumid = None;
        let mut exe = None;
        let mut profile_directory = None;
        if spec.len() > APPS_PREFIX.len()
            && spec.is_char_boundary(APPS_PREFIX.len())
            && spec[..APPS_PREFIX.len()].eq_ignore_ascii_case(APPS_PREFIX)
        {
            let id = spec[APPS_PREFIX.len()..].to_string();
            if let Some(t) = shell_link_target(spec).filter(|t| t.to_ascii_lowercase().ends_with(".exe")) {
                exe = Some(t);
            }
            if id.eq_ignore_ascii_case("Microsoft.Windows.Explorer") {
                let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
                exe = Some(format!("{windir}\\explorer.exe"));
            }
            aumid = Some(id);
        } else if spec.to_ascii_lowercase().ends_with(".lnk") {
            aumid = shell_item_aumid(spec);
            exe = shell_link_target(spec);
            let (sc_aumid, sc_prof) = shortcut_identity(spec);
            aumid = sc_aumid.or(aumid);
            profile_directory = sc_prof;
        } else {
            exe = Some(spec.to_string());
        }

        let icon = extract_app_icon(HWND::default(), exe.as_deref(), Some(spec));
        let mut name = shell_display_name(spec)
            .unwrap_or_else(|| friendly_app_name(exe.as_deref(), aumid.as_deref(), spec));
        if exe.is_some() && name.to_ascii_lowercase().ends_with(".exe") {
            name = friendly_app_name(exe.as_deref(), None, &name);
        }
        let meta = PinnedMeta {
            name,
            exe,
            aumid,
            profile_directory,
            icon,
        };
        self.pinned_cache.insert(key.to_string(), meta.clone());
        meta
    }

    fn cached_icon(&mut self, key: &str, hwnd: HWND, exe_path: Option<&str>, spec: Option<&str>) -> Option<Image> {
        if let Some(hit) = self.icons.get(key) {
            return hit.clone();
        }
        if self.icons.len() >= MAX_ICON_CACHE {
            self.icons.clear();
        }
        let img = extract_app_icon(hwnd, exe_path, spec);
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

/// Send `WM_CLOSE` to all windows belonging to a running app in the dock.
pub fn close_windows(windows: &[(HWND, String)], primary: HWND) {
    unsafe {
        if windows.is_empty() {
            if !primary.is_invalid() && IsWindow(primary).as_bool() {
                let _ = PostMessageW(primary, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
            return;
        }
        for &(h, _) in windows {
            if !h.is_invalid() && IsWindow(h).as_bool() {
                let _ = PostMessageW(h, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Extract Chromium's explicitly selected profile directory from launch arguments.
pub fn extract_profile_directory(args: &str) -> Option<String> {
    let mut args = args.trim().chars().peekable();
    while args.peek().is_some() {
        while args.peek().is_some_and(|c| c.is_whitespace()) {
            args.next();
        }
        let mut token = String::new();
        let mut quote = None;
        while let Some(&c) = args.peek() {
            if let Some(q) = quote {
                args.next();
                if c == q {
                    quote = None;
                } else {
                    token.push(c);
                }
            } else if c == '"' || c == '\'' {
                quote = Some(c);
                args.next();
            } else if c.is_whitespace() {
                break;
            } else {
                token.push(c);
                args.next();
            }
        }
        if let Some((key, value)) = token.split_once('=') {
            if key.eq_ignore_ascii_case("--profile-directory") && !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

/// Squirrel apps are pinned through `...\App\Update.exe` while their windows belong to `...\App\app-1.2.3\App.exe`.
pub fn launcher_of(pinned: &str, running: &str) -> bool {
    let p = std::path::Path::new(pinned);
    let (Some(dir), Some(file)) = (p.parent(), p.file_name()) else {
        return false;
    };
    let prefix = format!("{}\\", dir.to_string_lossy());
    if running.len() <= prefix.len()
        || !running.is_char_boundary(prefix.len())
        || !running[..prefix.len()].eq_ignore_ascii_case(&prefix)
    {
        return false;
    }
    let file = file.to_string_lossy();
    let run_file = std::path::Path::new(running)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    file.eq_ignore_ascii_case("Update.exe") || file.eq_ignore_ascii_case(&run_file)
}

pub fn match_score(
    pin_exe: Option<&str>,
    pin_aumid: Option<&str>,
    pin_profile: Option<&str>,
    win_exe: Option<&str>,
    win_aumid: Option<&str>,
    win_window_aumid: Option<&str>,
    win_profile: Option<&str>,
) -> Option<u8> {
    let same_exe = pin_exe
        .zip(win_exe)
        .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b) || launcher_of(a, b));
    if let Some(profile) = pin_profile {
        if !same_exe {
            return None;
        }
        if let Some(actual) = win_profile {
            return profile.eq_ignore_ascii_case(actual).then_some(4);
        }
        if let Some((_, suffix)) = win_window_aumid.and_then(|s| s.rsplit_once('.')) {
            if profile.replace(' ', "").eq_ignore_ascii_case(suffix) {
                return Some(4);
            }
        }
    }
    if let Some(pin) = pin_aumid {
        if win_window_aumid.is_some_and(|id| pin.eq_ignore_ascii_case(id)) {
            return Some(3);
        }
        if win_window_aumid.is_none() && win_aumid.is_some_and(|id| pin.eq_ignore_ascii_case(id)) {
            return Some(2);
        }
    }
    if pin_profile.is_some() && (win_window_aumid.is_some() || win_profile.is_some()) {
        return None;
    }
    if same_exe {
        return Some(1);
    }
    None
}

struct InspectedWin {
    hwnd: HWND,
    pid: u32,
    exe: Option<String>,
    aumid: Option<String>,
    window_aumid: Option<String>,
    profile_directory: Option<String>,
    title: String,
    minimized: bool,
}

fn best_pinned_match(items: &[RunningApp], w: &InspectedWin) -> Option<usize> {
    let (mut high, mut owner) = (0u8, None);
    for (i, it) in items.iter().enumerate().filter(|(_, it)| it.pinned) {
        if let Some(score) = match_score(
            it.exe.as_deref(),
            it.aumid.as_deref(),
            it.profile_directory.as_deref(),
            w.exe.as_deref(),
            w.aumid.as_deref(),
            w.window_aumid.as_deref(),
            w.profile_directory.as_deref(),
        ) {
            if score > high {
                high = score;
                owner = Some(i);
            } else if score == high {
                owner = None;
            }
        }
    }
    owner
}

fn running_identity(w: &InspectedWin) -> String {
    if let Some(profile) = &w.profile_directory {
        let app = w.aumid.as_deref().or(w.exe.as_deref()).unwrap_or("unknown");
        return format!("{app}#profile:{profile}").to_ascii_lowercase();
    }
    w.aumid
        .clone()
        .or_else(|| w.exe.clone())
        .unwrap_or_else(|| format!("hwnd:{:#x}", w.hwnd.0 as usize))
        .to_ascii_lowercase()
}

fn inspect_window(hwnd: HWND, self_pid: u32) -> Option<InspectedWin> {
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

        let (window_aumid, relaunch) = window_property_identity(hwnd);
        let mut aumid = window_aumid.clone();
        let mut exe = None;
        if cls == "ApplicationFrameWindow" {
            let mut inner = (pid, 0u32);
            unsafe extern "system" fn child_cb(c: HWND, l: LPARAM) -> BOOL {
                let st = unsafe { &mut *(l.0 as *mut (u32, u32)) };
                let mut cp = 0u32;
                unsafe { GetWindowThreadProcessId(c, Some(&mut cp)) };
                if cp != 0 && cp != st.0 {
                    st.1 = cp;
                    return BOOL(0);
                }
                BOOL(1)
            }
            let _ = EnumChildWindows(hwnd, Some(child_cb), LPARAM(&mut inner as *mut _ as isize));
            if inner.1 != 0 {
                pid = inner.1;
                exe = process_exe_path(pid);
                aumid = aumid.or_else(|| process_aumid(pid));
            }
            if aumid.is_none() {
                return None;
            }
        }

        let profile_directory = relaunch.as_deref().and_then(extract_profile_directory);
        let minimized = IsIconic(hwnd).as_bool();
        Some(InspectedWin {
            hwnd,
            pid,
            exe,
            aumid,
            window_aumid,
            profile_directory,
            title,
            minimized,
        })
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

fn process_aumid(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let ok = GetApplicationUserModelId(h, &mut len, PWSTR(buf.as_mut_ptr())).is_ok();
        let _ = CloseHandle(h);
        if !ok || len == 0 {
            return None;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(len as usize);
        let s = String::from_utf16_lossy(&buf[..n]);
        (!s.is_empty()).then_some(s)
    }
}

fn read_prop_string(ps: &IPropertyStore, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let pv = ps.GetValue(key).ok()?;
        let s = BSTR::try_from(&pv).ok()?.to_string();
        (!s.is_empty()).then_some(s)
    }
}

fn store_prop_string(parsing: &str, key: &PROPERTYKEY) -> Option<String> {
    unsafe {
        let w: Vec<u16> = parsing.encode_utf16().chain(Some(0)).collect();
        let ps: IPropertyStore = SHGetPropertyStoreFromParsingName(PCWSTR(w.as_ptr()), None, GPS_DEFAULT).ok()?;
        read_prop_string(&ps, key)
    }
}

fn shell_link_target(parsing: &str) -> Option<String> {
    store_prop_string(parsing, &PKEY_Link_TargetParsingPath)
}

fn shell_item_aumid(parsing: &str) -> Option<String> {
    store_prop_string(parsing, &PKEY_AppUserModel_ID)
}

fn window_property_identity(h: HWND) -> (Option<String>, Option<String>) {
    unsafe {
        let Ok(ps) = SHGetPropertyStoreForWindow::<_, IPropertyStore>(h) else {
            return (None, None);
        };
        (
            read_prop_string(&ps, &PKEY_AppUserModel_ID),
            read_prop_string(&ps, &PKEY_AppUserModel_RelaunchCommand),
        )
    }
}

fn shortcut_identity(parsing: &str) -> (Option<String>, Option<String>) {
    use windows::Win32::System::Com::{CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ};
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    unsafe {
        let Ok(link) = CoCreateInstance::<_, IShellLinkW>(&ShellLink, None, CLSCTX_INPROC_SERVER) else {
            return (None, None);
        };
        let w: Vec<u16> = parsing.encode_utf16().chain(Some(0)).collect();
        let Ok(persist) = link.cast::<IPersistFile>() else {
            return (None, None);
        };
        if persist.Load(PCWSTR(w.as_ptr()), STGM_READ).is_err() {
            return (None, None);
        }
        let aumid = link
            .cast::<IPropertyStore>()
            .ok()
            .and_then(|store| read_prop_string(&store, &PKEY_AppUserModel_ID));
        let mut args = [0u16; 2048];
        let args = link.GetArguments(&mut args).ok().map(|_| {
            let n = args.iter().position(|&c| c == 0).unwrap_or(args.len());
            String::from_utf16_lossy(&args[..n])
        });
        (aumid, args.as_deref().and_then(extract_profile_directory))
    }
}

pub fn shell_display_name(parsing: &str) -> Option<String> {
    unsafe {
        let w: Vec<u16> = parsing.encode_utf16().chain(Some(0)).collect();
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).ok()?;
        let p = item.GetDisplayName(SIGDN_NORMALDISPLAY).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const c_void));
        s.filter(|s| !s.trim().is_empty())
    }
}

pub fn friendly_app_name(exe: Option<&str>, aumid: Option<&str>, fallback: &str) -> String {
    if let Some(a) = aumid {
        if let Some(n) = shell_display_name(&format!("shell:AppsFolder\\{a}")) {
            return n;
        }
    }
    if let Some(e) = exe {
        if let Some(n) = shell_display_name(e) {
            return n.trim_end_matches(".exe").to_string();
        }
        if let Some(n) = friendly_exe_name(e) {
            return n;
        }
    }
    shorten_title(fallback)
}

fn shell_parsing_icon(parsing: &str, max_px: u32) -> Option<Image> {
    unsafe {
        let w: Vec<u16> = parsing.encode_utf16().chain(Some(0)).collect();
        let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None).ok()?;
        let f: IShellItemImageFactory = item.cast().ok()?;
        let hbm = f
            .GetImage(
                SIZE {
                    cx: max_px as i32,
                    cy: max_px as i32,
                },
                SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK,
            )
            .ok()?;
        let img = hbitmap_to_image(hbm, max_px);
        let _ = DeleteObject(HGDIOBJ(hbm.0));
        img
    }
}

unsafe fn hbitmap_to_image(hbm: HBITMAP, target: u32) -> Option<Image> {
    let mut bm = BITMAP::default();
    if GetObjectW(
        HGDIOBJ(hbm.0),
        std::mem::size_of::<BITMAP>() as i32,
        Some(&mut bm as *mut _ as *mut c_void),
    ) == 0
        || bm.bmBits.is_null()
        || bm.bmBitsPixel != 32
    {
        return None;
    }
    let (w, h, stride) = (
        bm.bmWidth as usize,
        bm.bmHeight.unsigned_abs() as usize,
        bm.bmWidthBytes as usize,
    );
    if w == 0 || h == 0 {
        return None;
    }
    let mut v = vec![0u8; w * h * 4];
    let src = bm.bmBits as *const u8;
    for y in 0..h {
        let sy = if bm.bmHeight > 0 { h - 1 - y } else { y };
        std::ptr::copy_nonoverlapping(src.add(sy * stride), v.as_mut_ptr().add(y * w * 4), w * 4);
    }
    for p in v.chunks_exact_mut(4) {
        let a = p[3];
        p[0] = p[0].min(a);
        p[1] = p[1].min(a);
        p[2] = p[2].min(a);
    }
    if w as u32 <= target && h as u32 <= target {
        return Some(ImageData::new(w as u32, h as u32, v));
    }
    let tw = target as usize;
    let mut out = vec![0u8; tw * tw * 4];
    for ty in 0..tw {
        let sy = (ty * h / tw).min(h - 1);
        for tx in 0..tw {
            let sx = (tx * w / tw).min(w - 1);
            let si = (sy * w + sx) * 4;
            let di = (ty * tw + tx) * 4;
            out[di..di + 4].copy_from_slice(&v[si..si + 4]);
        }
    }
    Some(ImageData::new(target, target, out))
}

fn extract_app_icon(hwnd: HWND, exe_path: Option<&str>, spec: Option<&str>) -> Option<Image> {
    if let Some(s) = spec {
        if s.len() > 6 && s[..6].eq_ignore_ascii_case("shell:") || s.to_ascii_lowercase().ends_with(".lnk") {
            if let Some(img) = shell_parsing_icon(s, ICON_PX) {
                return Some(img);
            }
        }
    }
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

/// Extract unread notification count from a window title prefix: `"(12) Inbox"` → `12`, `"(99+) Chats"` → `99`.
pub fn badge_from_title(title: &str) -> u16 {
    let t = title.trim_start();
    let Some(rest) = t.strip_prefix('(').or_else(|| t.strip_prefix('[')) else {
        return 0;
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() || digits.len() > 4 {
        return 0;
    }
    let after = &rest[digits.len()..];
    let after = after.strip_prefix('+').unwrap_or(after);
    if !after.starts_with(") ") && !after.starts_with(")\t") && !after.starts_with("] ") && !after.starts_with("]\t") {
        return 0;
    }
    digits.parse::<u16>().unwrap_or(0)
}

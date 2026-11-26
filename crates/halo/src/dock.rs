//! Floating bottom dock shelf (Direction A foundation): Direct2D + DirectComposition
//! layered tool window (`Halo.Dock`) sharing the island's D3D11 device with
//! non-blocking `Present(0, 0)`, rounded-pill hit mask, right-click tuck/peek
//! handle, fullscreen hide, and zero idle CPU when settled.

use crate::gfx::{Canvas, Gfx, HitMask, Image, SharedD3d, TextStyle, Weight};
use crate::glass::GlassParams;
use crate::icons::Icon;
use crate::pages::PageKind;
use crate::services::dock_apps::RunningApp;
use crate::window::MonitorInfo;
use halo_core::geom::{Color, Point, Rect};
use halo_core::look::{self, Look};
use halo_core::settings::{DockBadgeMode, DockGlassMaterial, DockIconSize, NotchTheme, NotchType};
use halo_core::spring::Spring;
use halo_core::svgpath::Seg;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::Shell::{DragAcceptFiles, DragFinish, DragQueryFileW, DragQueryPoint, HDROP};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CLASS: PCWSTR = w!("Halo.Dock");
pub const STACK_CLASS: PCWSTR = w!("Halo.DockStack");
pub const FLYOUT_CLASS: PCWSTR = w!("Halo.DockFlyout");

/// Tracks whether Halo Island currently hid the Windows system taskbar so we
/// always restore it cleanly when the dock turns off or the process exits.
static TASKBAR_HIDDEN_BY_HALO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Not exported by windows-rs 0.58's WindowsAndMessaging module.
const WM_MOUSELEAVE: u32 = 0x02A3;

/// Fixed surface height in DIPs: room for the 58 DIP shelf, 4 DIP bottom gap,
/// magnification headroom (Phase 3), and floating item label without resizing HWND.
pub const SURF_H: f32 = 136.0;
pub const MAX_SURF_W: f32 = 1360.0;
pub const SIDE_MARGIN: f32 = 24.0;
pub const SHELF_H: f32 = 58.0;
pub const SHELF_R: f32 = 20.0;
pub const FLOAT_GAP: f32 = 4.0;
pub const PEEK_W: f32 = 120.0;
pub const PEEK_H: f32 = 4.0;
pub const PEEK_HIT_H: f32 = 10.0;
pub const SLOT_MAX: f32 = 44.0;
pub const SLOT_MIN: f32 = 28.0;
pub const SLOT_GAP: f32 = 8.0;
pub const SHELF_PAD_X: f32 = 12.0;
pub const SEP_W: f32 = 12.0;
pub const MAX_MAG: f32 = 1.48;
pub const MAG_RADIUS_SLOTS: f32 = 2.6;
const MASK_INTERVAL: f64 = 1.0 / 30.0;
const TUCK_EPS: f32 = 0.004;
const MAG_EPS: f32 = 0.004;
/// Duration (seconds) and peak amplitude (DIPs) of the macOS parabolic launch/click bounce.
pub const BOUNCE_DURATION: f32 = 0.72;
pub const BOUNCE_PEAK_Y: f32 = 25.0;

/// Parabolic 2-hop damped bounce offset (`<= 0.0`, upward in DIPs) for normalized `progress` (`0.0..1.0`).
/// Matches `full_ecosystem.html` (`-27 * |sin(p * 2π)| * exp(-1.9 * p)`) and returns `0.0` at rest.
pub fn bounce_offset_y(progress: f32) -> f32 {
    if !(0.0001..0.9999).contains(&progress) {
        return 0.0;
    }
    let hops = (progress * std::f32::consts::TAU).sin().abs();
    let decay = (-1.9 * progress).exp();
    -BOUNCE_PEAK_Y * hops * decay
}

const TIP_SHADOW: &[(f32, f32)] = &[(-2.0, 0.32), (0.0, 0.24), (3.0, 0.11), (7.0, 0.03), (11.0, 0.0)];

/// Resolve the active [`Look`] for the dock from the island's theme settings.
pub fn resolve_dock_look(
    theme: NotchTheme,
    ty: NotchType,
    app_light: bool,
    diorama_night: bool,
    custom: Option<&'static Look>,
) -> &'static Look {
    match theme {
        NotchTheme::Glass => look::glass(app_light),
        NotchTheme::Halo => &look::HALO,
        NotchTheme::Diorama => look::diorama(diorama_night),
        NotchTheme::Classic => {
            if let Some(c) = custom {
                c
            } else if ty == NotchType::Pill {
                &look::PILL
            } else {
                look::halo(app_light)
            }
        }
    }
}

/// Build [`GlassParams`] for a floating rounded capsule (`shoulder = -1.0`) so the
/// dock shelf renders through Halo Island's GPU `res/glass.hlsl` liquid-glass pass.
pub fn dock_glass_params(body: Rect, radius: f32, look: &Look, scale: f32) -> GlassParams {
    dock_glass_params_with_material(body, radius, look, scale, DockGlassMaterial::Dynamic)
}

/// Build customizable [`GlassParams`] tuning refraction tint, rim specular, dispersion,
/// and diffusion to match the chosen [`DockGlassMaterial`].
pub fn dock_glass_params_with_material(
    body: Rect,
    radius: f32,
    look: &Look,
    scale: f32,
    mat: DockGlassMaterial,
) -> GlassParams {
    let th = halo_core::glass::thickness(body.h);
    let mix = if look.light { 1.0 } else { 0.0 };
    let (mut ground_dark, mut ground_light) = if look.light {
        (look::GLASS_DARK.ground, look.ground)
    } else {
        (look.ground, look::GLASS_LIGHT.ground)
    };

    let (tint_dark, tint_light, rim, glow, frost_mix, saturation, dispersion) = match mat {
        DockGlassMaterial::Dynamic => (
            0.34,
            0.42,
            0.80 + 0.15 * mix,
            0.09 + 0.01 * mix,
            th.t,
            1.45,
            0.03,
        ),
        DockGlassMaterial::SmokedDark => {
            ground_dark = Color::hex(0x05070A);
            (
                0.62,
                0.58,
                0.92,
                0.12,
                (th.t + 0.12).min(1.0),
                1.15,
                0.015,
            )
        }
        DockGlassMaterial::CrystalLight => {
            ground_light = Color::hex(0xF8FAFC);
            (
                0.18,
                0.24,
                0.96,
                0.14,
                (th.t * 0.72).max(0.15),
                1.60,
                0.055,
            )
        }
        DockGlassMaterial::FrostedMatte => (
            0.44,
            0.50,
            0.70,
            0.06,
            (th.t + 0.35).min(1.0),
            1.20,
            0.01,
        ),
    };

    GlassParams {
        body,
        radius,
        shoulder: -1.0,
        bevel: th.bevel,
        mag: th.mag,
        frost_mix,
        light_mix: mix,
        tint_dark,
        tint_light,
        ground_dark,
        ground_light,
        rim,
        glow,
        light_dir: (-0.6, -0.8),
        shadow_alpha: halo_core::glass::shadow_alpha(0.38, frost_mix),
        shadow_reach: th.shadow_reach,
        shadow_drop: th.shadow_drop,
        saturation,
        dispersion,
        lens: 1.0,
        use_copy: false,
        probe: false,
        copy_origin: (0.0, 0.0),
        probe_x: crate::glass::probe_x(body, scale),
    }
}

/// Whether the bottom dock should be active (Halo Pro required, disabled in selftest).
pub fn dock_allowed(enabled: bool, pro: bool, in_selftest: bool) -> bool {
    enabled && pro && !in_selftest
}

/// User-configurable Dock preferences synced from `Settings`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DockConfig {
    pub icon_size: DockIconSize,
    pub glass_material: DockGlassMaterial,
    pub badge_mode: DockBadgeMode,
    pub magnification: bool,
    pub bottom_most: bool,
    pub show_apps: bool,
    pub show_downloads: bool,
    pub show_tray: bool,
}

impl Default for DockConfig {
    fn default() -> Self {
        Self {
            icon_size: DockIconSize::Default,
            glass_material: DockGlassMaterial::Dynamic,
            badge_mode: DockBadgeMode::Count,
            magnification: true,
            bottom_most: true,
            show_apps: true,
            show_downloads: true,
            show_tray: true,
        }
    }
}

impl DockConfig {
    pub fn slot_max(self) -> f32 {
        match self.icon_size {
            DockIconSize::Compact => 36.0,
            DockIconSize::Default => SLOT_MAX,
            DockIconSize::Large => 52.0,
        }
    }

    pub fn shelf_h(self) -> f32 {
        match self.icon_size {
            DockIconSize::Compact => 50.0,
            DockIconSize::Default => SHELF_H,
            DockIconSize::Large => 66.0,
        }
    }
}

/// Snapshot of system status shown in the dock's right-side System Tray Capsule.
/// Driven passively from `AppState` (0 background threads/timers; dirty only on change).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DockTrayState {
    pub hour: u8,
    pub minute: u8,
    pub weekday: u8,
    pub day: u8,
    pub month: u8,
    pub use_24h: bool,
    pub volume_pct: u8,
    pub muted: bool,
    pub volume_known: bool,
    pub has_battery: bool,
    pub battery_pct: u8,
    pub charging: bool,
    pub on_ac: bool,
    pub wifi_connected: Option<bool>,
    pub weather_temp_c: Option<i16>,
    pub weather_code: Option<u16>,
    pub tasks_open: u16,
    pub shelf_count: u16,
    pub downloads_active: u16,
    pub download_progress: Option<u8>,
    pub focus_active: bool,
}

/// Live strings for the Control Center (kept out of the `Copy` tray state).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DockLive {
    pub ssid: String,
    pub internet: Option<bool>,
    pub bt_name: String,
    pub bt_battery: Option<u8>,
}

impl DockTrayState {
    pub fn clock_text(&self) -> String {
        if self.use_24h {
            format!("{:02}:{:02}", self.hour % 24, self.minute % 60)
        } else {
            let h12 = match self.hour % 12 {
                0 => 12,
                h => h,
            };
            format!("{h12}:{:02}", self.minute % 60)
        }
    }

    pub fn date_text(&self) -> Option<String> {
        if self.day == 0 || self.month == 0 {
            return None;
        }
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let wd = DAYS[(self.weekday as usize) % 7];
        let mo = MONTHS[((self.month.saturating_sub(1)) as usize) % 12];
        Some(format!("{wd}, {} {mo}", self.day))
    }

    pub fn weather_short(&self) -> &'static str {
        match self.weather_code.unwrap_or(0) {
            0..=1 => "Clear",
            2..=3 => "Cloudy",
            45 | 48 => "Fog",
            51..=67 | 80..=82 => "Rain",
            71..=77 | 85..=86 => "Snow",
            95..=99 => "Storm",
            _ => "Local",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DockTrayZone {
    Weather,
    Chevron,
    Status,
    Clock,
}

const TRAY_CHEVRON_W: f32 = 24.0;

pub fn tray_capsule_width(tray: &DockTrayState) -> f32 {
    let weather_w = if tray.weather_temp_c.is_some() { 70.0 } else { 0.0 };
    let status_w = (if tray.has_battery { 120.0 } else { 76.0 }) + TRAY_CHEVRON_W + 4.0;
    let clock_w = if tray.day > 0 && tray.month > 0 { 72.0 } else { 52.0 };
    weather_w + status_w + clock_w + if weather_w > 0.0 { 8.0 } else { 4.0 }
}

fn tray_chevron_and_status_rects(status_r: Rect) -> (Rect, Rect) {
    let cw = TRAY_CHEVRON_W.min((status_r.w * 0.32).max(18.0));
    let chev = Rect::new(status_r.x, status_r.y, cw, status_r.h);
    let sx = (chev.right() + 4.0).min(status_r.right());
    let stat = Rect::new(sx, status_r.y, (status_r.right() - sx).max(16.0), status_r.h);
    (chev, stat)
}

fn tray_zone_rects(tray_rect: Rect, tray: &DockTrayState) -> (Option<Rect>, Rect, Rect) {
    let gap = 4.0;
    let has_weather = tray.weather_temp_c.is_some();
    let weather_w = if has_weather { 66.0f32.min(tray_rect.w * 0.28) } else { 0.0 };
    let clock_w = if tray.day > 0 && tray.month > 0 { 70.0f32 } else { 50.0f32 }.min(tray_rect.w * 0.36);

    let mut x = tray_rect.x;
    let weather_r = if has_weather {
        let r = Rect::new(x, tray_rect.y, weather_w, tray_rect.h);
        x = r.right() + gap;
        Some(r)
    } else {
        None
    };

    let status_w = (tray_rect.right() - x - clock_w - gap).max(if tray.has_battery { 76.0 } else { 46.0 });
    let status_r = Rect::new(x, tray_rect.y, status_w.min((tray_rect.right() - x - clock_w - gap).max(20.0)), tray_rect.h);
    let clock_r = Rect::new(status_r.right() + gap, tray_rect.y, (tray_rect.right() - (status_r.right() + gap)).max(24.0), tray_rect.h);
    (weather_r, status_r, clock_r)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DockCmd {
    ToggleTuck,
    OpenSettings,
    DisableDock,
    ActivateWindow(HWND),
    LaunchApp(String),
    KeepInDock(String),
    RemoveFromDock(String),
    ShowInExplorer(String),
    QuitApp(Vec<(HWND, String)>, HWND),
    OpenRecycleBin,
    EmptyRecycleBin,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DockMenuTarget {
    Shelf,
    App(RunningApp),
    Trash,
}

#[derive(Clone, Debug, PartialEq)]
pub enum DockAction {
    None,
    RequestFrame,
    OpenLaunchpad,
    ToggleIsland,
    OpenPage(PageKind),
    OpenSettings,
    ToggleAppWindow { hwnd: HWND, minimized: bool, focused: bool, icon_rect: Rect },
    LaunchPinnedApp(String),
    ToggleDownloadsStack(Rect),
    OpenRecycleBin,
    ToggleControlCenter(Rect),
    ToggleTray(Rect),
    ShowMenu(POINT, bool, DockMenuTarget),
    DropToShelf(Vec<String>),
    DropToRecycleBin(Vec<String>),
    OpenFilesWithApp { hwnd: HWND, exe: Option<String>, files: Vec<String> },
    ReorderPinned(Vec<String>),
    RemovePinnedApp(String),
}

#[derive(Clone, Copy)]
struct AnchorSlot {
    icon: Icon,
    label: &'static str,
    accent: Color,
    action: SlotAction,
    active: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SlotAction {
    Launchpad,
    ToggleIsland,
    Page(PageKind),
    Settings,
}

const ANCHOR_SLOTS: &[AnchorSlot] = &[
    AnchorSlot { icon: Icon::AppWindow, label: "Launchpad & Search", accent: Color::hex(0x38BDF8), action: SlotAction::Launchpad, active: false },
    AnchorSlot { icon: Icon::AppWindow, label: "Halo Island", accent: Color::hex(0x0A84FF), action: SlotAction::ToggleIsland, active: true },
    AnchorSlot { icon: Icon::Tray, label: "Shelf", accent: Color::hex(0x5E5CE6), action: SlotAction::Page(PageKind::Shelf), active: false },
    AnchorSlot { icon: Icon::ListChecks, label: "Tasks", accent: Color::hex(0x30D158), action: SlotAction::Page(PageKind::Tasks), active: false },
    AnchorSlot { icon: Icon::Clock, label: "Focus", accent: Color::hex(0xFF9F0A), action: SlotAction::Page(PageKind::Focus), active: false },
    AnchorSlot { icon: Icon::Gear, label: "Dock Settings", accent: Color::hex(0x8E8E93), action: SlotAction::Settings, active: false },
];

/// Register `Halo.Dock` once at process startup.
pub fn register(wndproc: crate::window::WndProc) -> Result<()> {
    register_class(CLASS, wndproc)
}

/// Register `Halo.DockStack` (the transient Downloads stack popover) once at startup.
pub fn register_stack(wndproc: crate::window::WndProc) -> Result<()> {
    register_class(STACK_CLASS, wndproc)
}

/// Register `Halo.DockFlyout` (the transient Control Center popover) once at startup.
pub fn register_flyout(wndproc: crate::window::WndProc) -> Result<()> {
    register_class(FLYOUT_CLASS, wndproc)
}

/// Hide or restore the Windows system taskbar (`Shell_TrayWnd` + `Shell_SecondaryTrayWnd`).
/// When `hide` is `false`, restores the taskbar only if Halo previously hid it.
pub fn set_system_taskbar_hidden(hide: bool) {
    use std::sync::atomic::Ordering;
    if cfg!(test) {
        TASKBAR_HIDDEN_BY_HALO.store(hide, Ordering::SeqCst);
        return;
    }
    if hide {
        TASKBAR_HIDDEN_BY_HALO.store(true, Ordering::Release);
    } else if !TASKBAR_HIDDEN_BY_HALO.swap(false, Ordering::AcqRel) {
        return;
    }
    let cmd = if hide { SW_HIDE } else { SW_SHOWNA };
    for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")] {
        let mut after = HWND::default();
        for _ in 0..16 {
            let Ok(bar) = (unsafe { FindWindowExW(HWND::default(), after, class, PCWSTR::null()) }) else {
                break;
            };
            if bar.is_invalid() {
                break;
            }
            after = bar;
            if crate::services::tray_host::owns_window(bar) {
                continue;
            }
            unsafe {
                let _ = ShowWindow(bar, cmd);
            }
        }
    }
}

/// Whether Halo currently has the Windows system taskbar hidden.
pub fn is_system_taskbar_hidden() -> bool {
    TASKBAR_HIDDEN_BY_HALO.load(std::sync::atomic::Ordering::Acquire)
}

pub(crate) fn register_class(class: PCWSTR, wndproc: crate::window::WndProc) -> Result<()> {
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_DBLCLKS,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst.into(),
            hIcon: crate::window::app_icon(false),
            hIconSm: crate::window::app_icon(true),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(windows::core::Error::from_win32());
        }
    }
    Ok(())
}

/// Physical pixel bounds `(x, y, w, h)` for the dock surface on `mon`.
pub fn surface_rect(mon: &MonitorInfo) -> (i32, i32, i32, i32) {
    surface_rect_for(mon, false)
}

/// Physical pixel bounds `(x, y, w, h)` for the dock surface on `mon`.
/// When `bottom_most` is `true` (e.g. Windows taskbar hidden by dock), anchors flush
/// to `mon.rect.bottom` so the dock sits at the very bottom edge of the screen and
/// never floats high up over open application windows.
pub fn surface_rect_for(mon: &MonitorInfo, bottom_most: bool) -> (i32, i32, i32, i32) {
    let scale = mon.scale().max(1.0);
    let mon_w = (mon.rect.right - mon.rect.left).max(1);
    let max_dip = ((mon_w as f32 / scale) - 2.0 * SIDE_MARGIN).clamp(200.0, MAX_SURF_W);
    let w = ((max_dip * scale).ceil() as i32).clamp(1, mon_w);
    let h = ((SURF_H * scale).ceil() as i32).max(1);
    let x = mon.center_x() - w / 2;
    let bottom = if bottom_most { mon.rect.bottom } else { mon.bottom() };
    let y = bottom - h;
    (x, y, w, h)
}

/// Raised-cosine parabolic magnification scale (`1.0..=MAX_MAG`) for a slot
/// whose resting center is `dist_x` DIPs from the cursor.
pub fn mag_scale(dist_x: f32, step: f32) -> f32 {
    let radius = (step * MAG_RADIUS_SLOTS).max(1.0);
    let d = dist_x.abs();
    if d >= radius {
        1.0
    } else {
        let t = 0.5 * (1.0 + (std::f32::consts::PI * d / radius).cos());
        1.0 + (MAX_MAG - 1.0) * t
    }
}

/// Single-row slot size in DIPs for `count` items inside `surf_w` DIPs:
/// shrinks proportionally at high counts (e.g. 30 apps) so icons never wrap or clip.
pub fn slot_metrics(surf_w: f32, count: usize) -> (f32, f32) {
    slot_metrics_with_seps(surf_w, count, 0)
}

fn slot_metrics_with_seps(surf_w: f32, count: usize, sep_count: usize) -> (f32, f32) {
    slot_metrics_with_seps_and_tray(surf_w, count, sep_count, 0.0)
}

fn slot_metrics_with_seps_and_tray_max(surf_w: f32, count: usize, sep_count: usize, tray_w: f32, slot_max: f32) -> (f32, f32) {
    let n = count.max(1) as f32;
    let sep = sep_count as f32 * SEP_W;
    let avail = (surf_w - 2.0 * SHELF_PAD_X - 16.0 - sep - tray_w).max(64.0);
    let natural = n * slot_max + (n - 1.0) * SLOT_GAP;
    if natural <= avail {
        (slot_max, SLOT_GAP)
    } else {
        let ratio = (avail / natural).clamp(SLOT_MIN / slot_max.max(1.0), 1.0);
        let gap = (SLOT_GAP * ratio).max(4.0);
        let size = ((avail - (n - 1.0) * gap) / n).clamp(SLOT_MIN * 0.75, slot_max);
        (size, gap)
    }
}

fn slot_metrics_with_seps_and_tray(surf_w: f32, count: usize, sep_count: usize, tray_w: f32) -> (f32, f32) {
    slot_metrics_with_seps_and_tray_max(surf_w, count, sep_count, tray_w, SLOT_MAX)
}

/// Floating shelf rectangle in surface DIPs (`tuck_t`: `0.0` shown, `1.0` tucked).
pub fn shelf_rect(surf_w: f32, count: usize, tuck_t: f32) -> Rect {
    let n = count.max(1) as f32;
    let (slot, gap) = slot_metrics(surf_w, count);
    let shelf_h = (slot + 14.0).clamp(42.0, SHELF_H);
    let shelf_w = (n * slot + (n - 1.0) * gap + 2.0 * SHELF_PAD_X).min((surf_w - 8.0).max(64.0));
    let x = (surf_w - shelf_w) * 0.5;
    let base_y = SURF_H - FLOAT_GAP - shelf_h;
    let slide = tuck_t.clamp(0.0, 1.0) * (shelf_h + FLOAT_GAP + 4.0);
    Rect::new(x, base_y + slide, shelf_w, shelf_h)
}

/// Magnified dock layout: returns `(shelf_rect, optional_divider_x, slot_rects)`.
/// Icons grow upward from the shelf baseline and extra width is damped so the
/// shelf never overflows `surf_w`.
pub fn layout_slots(
    surf_w: f32,
    anchor_count: usize,
    app_count: usize,
    scales: &[f32],
    tuck_t: f32,
) -> (Rect, Option<f32>, Vec<Rect>) {
    let (shelf, sep1, _, rects) = layout_slots_with_stack(surf_w, anchor_count, app_count, 0, scales, tuck_t);
    (shelf, sep1, rects)
}

/// Magnified dock layout with trailing stack slots (e.g. Downloads) separated by
/// a second vertical divider. Returns `(shelf_rect, sep1_x, sep2_x, slot_rects)`.
pub fn layout_slots_with_stack(
    surf_w: f32,
    anchor_count: usize,
    app_count: usize,
    stack_count: usize,
    scales: &[f32],
    tuck_t: f32,
) -> (Rect, Option<f32>, Option<f32>, Vec<Rect>) {
    let (shelf, sep1, sep2, _, rects, _) = layout_slots_full(surf_w, anchor_count, app_count, stack_count, 0.0, scales, tuck_t);
    (shelf, sep1, sep2, rects)
}

/// Full dock layout including the trailing System Tray Capsule (`tray_w > 0.0`).
/// Returns `(shelf, sep1_x, sep2_x, sep3_x, slot_rects, tray_rect)`.
pub fn layout_slots_full(
    surf_w: f32,
    anchor_count: usize,
    app_count: usize,
    stack_count: usize,
    tray_w: f32,
    scales: &[f32],
    tuck_t: f32,
) -> (Rect, Option<f32>, Option<f32>, Option<f32>, Vec<Rect>, Option<Rect>) {
    layout_slots_configured(surf_w, anchor_count, app_count, stack_count, tray_w, scales, tuck_t, SLOT_MAX, SHELF_H)
}

#[allow(clippy::too_many_arguments)]
pub fn layout_slots_configured(
    surf_w: f32,
    anchor_count: usize,
    app_count: usize,
    stack_count: usize,
    tray_w: f32,
    scales: &[f32],
    tuck_t: f32,
    slot_max: f32,
    max_shelf_h: f32,
) -> (Rect, Option<f32>, Option<f32>, Option<f32>, Vec<Rect>, Option<Rect>) {
    let total = anchor_count + app_count + stack_count;
    let has_sep1 = anchor_count > 0 && app_count > 0;
    let has_sep2 = stack_count > 0 && (anchor_count + app_count) > 0;
    let has_tray = tray_w > 0.0 && surf_w >= tray_w + 180.0;
    let eff_tray_w = if has_tray { tray_w } else { 0.0 };
    let has_sep3 = has_tray && total > 0;
    let sep_count = usize::from(has_sep1) + usize::from(has_sep2) + usize::from(has_sep3);
    let (base_slot, base_gap) = slot_metrics_with_seps_and_tray_max(surf_w, total, sep_count, eff_tray_w, slot_max);
    let sep_w = sep_count as f32 * SEP_W;
    let slot_seps = usize::from(has_sep1) + usize::from(has_sep2);
    let gap_count = total.saturating_sub(1).saturating_sub(slot_seps) as f32;
    let base_w = total as f32 * base_slot + gap_count * base_gap + sep_w + eff_tray_w + 2.0 * SHELF_PAD_X;
    let max_shelf_w = (surf_w - 8.0).max(64.0);

    let mut raw_extra = 0.0f32;
    for i in 0..total {
        let s = scales.get(i).copied().unwrap_or(1.0).max(1.0);
        raw_extra += base_slot * (s - 1.0);
    }
    let avail_extra = (max_shelf_w - base_w).max(0.0);
    let damp = if raw_extra > 0.001 && raw_extra > avail_extra {
        avail_extra / raw_extra
    } else {
        1.0
    };

    let shelf_h = (base_slot + 14.0).clamp(42.0, max_shelf_h);
    let shelf_w = (base_w + raw_extra * damp).min(max_shelf_w);
    let x0 = (surf_w - shelf_w) * 0.5;
    let base_y = SURF_H - FLOAT_GAP - shelf_h;
    let slide = tuck_t.clamp(0.0, 1.0) * (shelf_h + FLOAT_GAP + 4.0);
    let shelf = Rect::new(x0, base_y + slide, shelf_w, shelf_h);
    let bottom_pad = (shelf_h - base_slot) * 0.5;

    let stack_start = anchor_count + app_count;
    let mut rects = Vec::with_capacity(total);
    let mut sep1_x = None;
    let mut sep2_x = None;
    let mut sep3_x = None;
    let mut x = shelf.x + SHELF_PAD_X;
    for i in 0..total {
        if has_sep1 && i == anchor_count {
            sep1_x = Some(x + SEP_W * 0.5);
            x += SEP_W;
        } else if has_sep2 && i == stack_start {
            sep2_x = Some(x + SEP_W * 0.5);
            x += SEP_W;
        }
        let s = 1.0 + (scales.get(i).copied().unwrap_or(1.0).max(1.0) - 1.0) * damp;
        let sz = base_slot * s;
        let y = shelf.bottom() - bottom_pad - sz;
        rects.push(Rect::new(x, y, sz, sz));
        x += sz;
        let next_is_sep = (has_sep1 && i + 1 == anchor_count) || (has_sep2 && i + 1 == stack_start);
        if i + 1 < total && !next_is_sep {
            x += base_gap;
        }
    }
    let tray_rect = if has_tray {
        if has_sep3 {
            sep3_x = Some(x + SEP_W * 0.5);
            x += SEP_W;
        }
        let th = (shelf_h - 18.0).clamp(28.0, 38.0);
        let ty = shelf.y + (shelf_h - th) * 0.5;
        let tw = (shelf.right() - SHELF_PAD_X - x).max(0.0).min(eff_tray_w);
        if tw >= 48.0 {
            Some(Rect::new(x, ty, tw, th))
        } else {
            sep3_x = None;
            None
        }
    } else {
        None
    };
    (shelf, sep1_x, sep2_x, sep3_x, rects, tray_rect)
}

/// Bottom 4 DIP peek handle shown while tucked (`CHOSEN.md` §3).
pub fn peek_rect(surf_w: f32) -> Rect {
    let w = PEEK_W.min((surf_w - 16.0).max(48.0));
    Rect::new((surf_w - w) * 0.5, SURF_H - PEEK_H - 2.0, w, PEEK_H)
}

/// Forgiving click target around the 4 DIP peek handle.
pub fn peek_hit_rect(surf_w: f32) -> Rect {
    let w = (PEEK_W + 20.0).min(surf_w.max(48.0));
    Rect::new((surf_w - w) * 0.5, SURF_H - PEEK_HIT_H, w, PEEK_HIT_H)
}

/// Layered hit-test mask for the dock window: strictly wraps the visible shelf
/// capsule (or the tucked peek handle) so empty desktop areas stay click-through.
pub fn hit_mask_for(surf_w: f32, count: usize, tucked: bool, tuck_t: f32) -> HitMask {
    let r = shelf_rect(surf_w, count, tuck_t);
    hit_mask_for_shelf(surf_w, r, 0.0, tucked, tuck_t)
}

fn hit_mask_for_shelf(surf_w: f32, shelf: Rect, lift: f32, tucked: bool, tuck_t: f32) -> HitMask {
    if tucked && tuck_t >= 0.85 {
        HitMask { rect: Some(peek_hit_rect(surf_w)), notch: None, pill: None }
    } else {
        let lift = lift.max(0.0);
        let r = Rect::new(shelf.x, (shelf.y - lift).max(0.0), shelf.w, shelf.h + lift);
        HitMask { rect: None, notch: None, pill: Some((r, SHELF_R.min(shelf.h * 0.5))) }
    }
}

#[cfg(test)]
fn slot_rect(shelf: Rect, index: usize, count: usize, surf_w: f32) -> Rect {
    let (slot, gap) = slot_metrics(surf_w, count);
    let x = shelf.x + SHELF_PAD_X + index as f32 * (slot + gap);
    let y = shelf.y + (shelf.h - slot) * 0.5;
    Rect::new(x, y, slot, slot)
}

fn mag_spring(value: f32) -> Spring {
    Spring::new(value, 460.0, 38.0)
}

const SLOT_SHIFT_EPS: f32 = 0.25;

fn shift_spring(value: f32) -> Spring {
    Spring::new(value, 520.0, 38.0)
}

#[derive(Clone, Copy, Debug)]
struct PoofPuff {
    angle: f32,
    dist: f32,
    r0: f32,
    r1: f32,
    delay: f32,
}

#[derive(Clone, Debug)]
struct PoofEffect {
    center: Point,
    elapsed: f32,
    duration: f32,
    puffs: [PoofPuff; 9],
}

impl PoofEffect {
    fn new(center: Point) -> Self {
        const TAU: f32 = std::f32::consts::TAU;
        let puffs = [
            PoofPuff { angle: 0.0, dist: 0.0, r0: 10.0, r1: 22.0, delay: 0.0 },
            PoofPuff { angle: 0.0 * TAU / 8.0 + 0.15, dist: 22.0, r0: 6.5, r1: 14.5, delay: 0.01 },
            PoofPuff { angle: 1.0 * TAU / 8.0 + 0.22, dist: 25.0, r0: 6.0, r1: 13.5, delay: 0.03 },
            PoofPuff { angle: 2.0 * TAU / 8.0 + 0.08, dist: 21.0, r0: 7.0, r1: 15.0, delay: 0.02 },
            PoofPuff { angle: 3.0 * TAU / 8.0 + 0.28, dist: 26.0, r0: 5.5, r1: 12.5, delay: 0.04 },
            PoofPuff { angle: 4.0 * TAU / 8.0 + 0.12, dist: 23.0, r0: 6.5, r1: 14.0, delay: 0.01 },
            PoofPuff { angle: 5.0 * TAU / 8.0 + 0.25, dist: 25.5, r0: 6.0, r1: 13.0, delay: 0.03 },
            PoofPuff { angle: 6.0 * TAU / 8.0 + 0.06, dist: 21.5, r0: 7.0, r1: 15.0, delay: 0.02 },
            PoofPuff { angle: 7.0 * TAU / 8.0 + 0.20, dist: 24.0, r0: 5.8, r1: 13.2, delay: 0.04 },
        ];
        Self { center, elapsed: 0.0, duration: 0.42, puffs }
    }

    fn step(&mut self, dt: f32) -> bool {
        self.elapsed += dt.max(0.0);
        self.elapsed < self.duration
    }
}

pub struct Dock {
    /// Dropped before `hwnd` so DirectComposition detaches cleanly first.
    gfx: Option<Gfx>,
    glass: crate::glass::Glass,
    glass_moving: bool,
    pub hwnd: HWND,
    at: (i32, i32, i32, i32),
    dpi: u32,
    light: bool,
    theme_look: &'static Look,
    look: &'static Look,
    shown: bool,
    fs_hidden: bool,
    tucked: bool,
    tuck: Spring,
    apps: Vec<RunningApp>,
    thumbs: std::collections::HashMap<isize, (Image, Option<RECT>)>,
    scales: Vec<Spring>,
    slot_x_offsets: Vec<Spring>,
    bounces: Vec<f32>,
    launching: std::collections::HashMap<String, f32>,
    poofs: Vec<PoofEffect>,
    tracking: bool,
    mouse: Option<Point>,
    hovered: Option<usize>,
    pressed: Option<usize>,
    drag_start: Option<Point>,
    dragging_app: bool,
    drag_out: bool,
    tray: DockTrayState,
    hovered_tray: Option<DockTrayZone>,
    pressed_tray: Option<DockTrayZone>,
    stack_open: bool,
    ctrl_open: bool,
    preview_open: bool,
    bottom_most: bool,
    cfg: DockConfig,
    dirty: bool,
    mask: Option<(HitMask, f64)>,
}

impl Drop for Dock {
    fn drop(&mut self) {
        set_system_taskbar_hidden(false);
        if let Some(gfx) = self.gfx.as_mut() {
            self.glass.stop(self.hwnd, gfx);
        }
        let visual = self.gfx.as_ref().and_then(|g| g.visual_hwnd());
        self.gfx = None;
        unsafe {
            if let Some(v) = visual {
                let _ = DestroyWindow(v);
            }
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

impl Dock {
    pub fn new(mon: &MonitorInfo, light: bool, shared: Option<SharedD3d>) -> Result<Self> {
        Self::with_look(mon, look::glass(light), shared)
    }

    pub fn with_look(mon: &MonitorInfo, look: &'static Look, shared: Option<SharedD3d>) -> Result<Self> {
        let (x, y, w, h) = surface_rect_for(mon, true);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS,
                w!("Halo Dock"),
                WS_POPUP,
                x,
                y,
                w,
                h,
                None,
                None,
                GetModuleHandleW(None)?,
                None,
            )?
        };
        let mut gfx = match Gfx::new(mon.dpi as f32, hwnd) {
            Ok(g) => g.secondary(),
            Err(e) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(e);
            }
        };
        gfx.set_shared_d3d(shared);
        let _ = gfx.set_surface(w, h);
        unsafe {
            DragAcceptFiles(hwnd, true);
        }
        Ok(Self {
            gfx: Some(gfx),
            glass: crate::glass::Glass::new_secondary(look.light),
            glass_moving: false,
            hwnd,
            at: (x, y, w, h),
            dpi: mon.dpi,
            light: look.light,
            theme_look: look,
            look,
            shown: false,
            fs_hidden: false,
            tucked: false,
            tuck: Spring::smooth(0.0),
            apps: Vec::new(),
            thumbs: std::collections::HashMap::new(),
            scales: (0..ANCHOR_SLOTS.len() + 2).map(|_| mag_spring(1.0)).collect(),
            slot_x_offsets: (0..ANCHOR_SLOTS.len() + 2).map(|_| shift_spring(0.0)).collect(),
            bounces: vec![0.0; ANCHOR_SLOTS.len() + 2],
            launching: std::collections::HashMap::new(),
            poofs: Vec::new(),
            tracking: false,
            mouse: None,
            hovered: None,
            pressed: None,
            drag_start: None,
            dragging_app: false,
            drag_out: false,
            tray: DockTrayState::default(),
            hovered_tray: None,
            pressed_tray: None,
            stack_open: false,
            ctrl_open: false,
            preview_open: false,
            bottom_most: true,
            cfg: DockConfig::default(),
            dirty: true,
            mask: None,
        })
    }

    /// Synchronize the dock's active [`Look`] with the island's theme; returns `true` only if it changed.
    pub fn set_look(&mut self, look: &'static Look) -> bool {
        if self.theme_look == look {
            return false;
        }
        self.theme_look = look;
        self.look = look;
        self.light = look.light;
        self.glass.app_light = look.light;
        self.dirty = true;
        true
    }

    /// Start or stop the live DXGI screen copy behind the dock when Liquid Glass is active.
    pub fn sync_glass(
        &mut self,
        allow_copy: bool,
        sink: &crate::events::EventSink,
        monitor: isize,
        shared: Option<SharedD3d>,
    ) {
        let Some(gfx) = self.gfx.as_mut() else { return };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        let want = allow_copy
            && !self.fs_hidden
            && !self.tucked
            && self.theme_look.material == look::Material::Glass;
        if want && !gfx.has_gpu() {
            let _ = gfx.set_surface(self.at.2, self.at.3);
            gfx.ensure_gpu_ready();
        }
        let on = want && matches!(gfx.renderer(), crate::gfx::Renderer::Gpu { .. });
        let (x, y, w, h) = self.at;
        let rect = RECT { left: x, top: y, right: x + w, bottom: y + h };
        self.glass.sync(on, self.hwnd, sink, monitor, rect, gfx);
    }

    /// Receive a new desktop frame behind the dock; returns `true` if the dock must redraw.
    pub fn on_backdrop(&mut self) -> bool {
        let Some(gfx) = self.gfx.as_mut() else { return false };
        if self.glass.on_backdrop(self.hwnd, gfx) {
            self.dirty = true;
            return true;
        }
        false
    }

    pub fn set_stack_open(&mut self, open: bool) {
        if self.stack_open != open {
            self.stack_open = open;
            self.dirty = true;
        }
    }

    pub fn set_ctrl_open(&mut self, open: bool) {
        if self.ctrl_open != open {
            self.ctrl_open = open;
            self.dirty = true;
        }
    }

    pub fn set_preview_open(&mut self, open: bool) {
        if self.preview_open != open {
            self.preview_open = open;
            self.dirty = true;
        }
    }

    /// Currently hovered running app (and its screen rect) for live window hover preview.
    pub fn hovered_running_app(&self) -> Option<(RunningApp, Rect)> {
        if self.tucked || self.fs_hidden || self.dragging_app || !self.cfg.show_apps {
            return None;
        }
        let idx = self.hovered?;
        if idx < ANCHOR_SLOTS.len() {
            return None;
        }
        let app_idx = idx - ANCHOR_SLOTS.len();
        let app = self.apps.get(app_idx)?;
        if !app.running || (app.windows.is_empty() && app.hwnd.is_invalid()) {
            return None;
        }
        Some((app.clone(), self.slot_screen_rect(idx)))
    }

    /// Look up a running app and its current screen rect by key (for keeping an open preview synced).
    pub fn running_app_by_key(&self, key: &str) -> Option<(RunningApp, Rect)> {
        if !self.cfg.show_apps {
            return None;
        }
        let app_idx = self.apps.iter().position(|a| a.key == key && a.running)?;
        let app = &self.apps[app_idx];
        if app.windows.is_empty() && app.hwnd.is_invalid() {
            return None;
        }
        Some((app.clone(), self.slot_screen_rect(ANCHOR_SLOTS.len() + app_idx)))
    }

    pub fn set_bottom_most(&mut self, mon: &MonitorInfo, bottom_most: bool) {
        self.bottom_most = bottom_most;
        self.cfg.bottom_most = bottom_most;
        let next = surface_rect_for(mon, bottom_most);
        if next != self.at {
            self.at = next;
            self.mask = None;
            self.dirty = true;
            if let Some(g) = self.gfx.as_mut() {
                let _ = g.set_surface(next.2, next.3);
            }
        }
    }

    pub fn set_config(&mut self, mon: &MonitorInfo, cfg: DockConfig) -> bool {
        if self.cfg == cfg && self.bottom_most == cfg.bottom_most {
            return false;
        }
        self.cfg = cfg;
        self.set_bottom_most(mon, cfg.bottom_most);
        let total = self.total_slots();
        self.scales.resize_with(total, || mag_spring(1.0));
        self.slot_x_offsets.resize_with(total, || shift_spring(0.0));
        self.bounces.resize(total, 0.0);
        if !cfg.magnification {
            self.reset_mag_targets(true);
        }
        if self.hovered.is_some_and(|i| i >= total) {
            self.hovered = None;
        }
        if !cfg.show_tray {
            self.hovered_tray = None;
            self.pressed_tray = None;
        }
        self.mask = None;
        self.dirty = true;
        true
    }

    /// Update the right-side System Tray Capsule snapshot; returns `true` only if it changed.
    pub fn set_tray(&mut self, tray: DockTrayState) -> bool {
        if self.tray == tray {
            return false;
        }
        self.tray = tray;
        self.dirty = true;
        true
    }

    pub fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    pub fn surf_w_dip(&self) -> f32 {
        self.at.2 as f32 / self.scale().max(1.0)
    }

    /// True only while a visual change, tuck, magnification spring, reorder shift, launch bounce,
    /// or poof cloud needs vblank frames; false at rest so `FrameClock` parks at 0% CPU.
    pub fn needs_frame(&self) -> bool {
        !self.fs_hidden
            && (self.dirty
                || self.glass_moving
                || !self.tuck.is_settled_eps(TUCK_EPS)
                || self.scales.iter().any(|s| !s.is_settled_eps(MAG_EPS))
                || self.slot_x_offsets.iter().any(|s| !s.is_settled_eps(SLOT_SHIFT_EPS))
                || self.bounces.iter().any(|&b| b > 0.0)
                || !self.launching.is_empty()
                || !self.poofs.is_empty())
    }

    pub fn trigger_bounce(&mut self, index: usize, instant: bool) {
        let total = self.total_slots();
        self.bounces.resize(total, 0.0);
        if !instant {
            if let Some(b) = self.bounces.get_mut(index) {
                *b = 0.001;
                self.dirty = true;
            }
        }
    }

    /// Mark an app as launching by spec / exe / key so its icon bounces repeatedly until
    /// a window appears or timeout passes.
    pub fn trigger_launch(&mut self, spec: &str, instant: bool) {
        if instant {
            return;
        }
        let clean = spec.trim().to_ascii_lowercase();
        if clean.is_empty() {
            return;
        }
        self.launching.insert(clean.clone(), 3.5);
        if let Some(i) = self.find_slot_by_spec(&clean) {
            self.trigger_bounce(i, false);
        }
        self.dirty = true;
    }

    /// Stop launch bounce for matching app spec or exe.
    #[allow(dead_code)]
    pub fn cancel_launch(&mut self, spec: &str) {
        let clean = spec.trim().to_ascii_lowercase();
        self.launching.remove(&clean);
    }

    /// Locate slot index for a given app spec, exe, or key.
    pub fn find_slot_by_spec(&self, spec: &str) -> Option<usize> {
        let clean = spec.trim().to_ascii_lowercase();
        let stem = std::path::Path::new(&clean)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&clean)
            .to_ascii_lowercase();
        self.visible_apps().iter().position(|a| {
            a.launch.as_ref().is_some_and(|l| l.to_ascii_lowercase() == clean || l.to_ascii_lowercase().contains(&stem))
                || a.exe.as_ref().is_some_and(|e| e.to_ascii_lowercase() == clean || e.to_ascii_lowercase().contains(&stem))
                || a.key.to_ascii_lowercase() == clean
                || a.key.to_ascii_lowercase().contains(&stem)
                || a.name.to_ascii_lowercase().contains(&stem)
        }).map(|idx| ANCHOR_SLOTS.len() + idx)
    }

    fn visible_apps(&self) -> &[RunningApp] {
        if self.cfg.show_apps {
            &self.apps
        } else {
            &[]
        }
    }

    fn stack_count(&self) -> usize {
        usize::from(self.cfg.show_downloads) + 1 // Downloads stack (optional) + Trash (always)
    }

    fn downloads_slot_index(&self) -> Option<usize> {
        self.cfg.show_downloads.then_some(ANCHOR_SLOTS.len() + self.visible_apps().len())
    }

    fn trash_slot_index(&self) -> usize {
        ANCHOR_SLOTS.len() + self.visible_apps().len() + usize::from(self.cfg.show_downloads)
    }

    fn effective_tray_w(&self) -> f32 {
        if self.cfg.show_tray {
            tray_capsule_width(&self.tray)
        } else {
            0.0
        }
    }

    pub fn total_slots(&self) -> usize {
        ANCHOR_SLOTS.len() + self.visible_apps().len() + self.stack_count()
    }

    fn slot_screen_rect(&self, index: usize) -> Rect {
        let sw = self.surf_w_dip();
        let scales = self.current_scales();
        let tray_w = self.effective_tray_w();
        let (_, _, _, _, rects, _) = layout_slots_configured(
            sw,
            ANCHOR_SLOTS.len(),
            self.visible_apps().len(),
            self.stack_count(),
            tray_w,
            &scales,
            self.tuck.value,
            self.cfg.slot_max(),
            self.cfg.shelf_h(),
        );
        let r = rects.get(index).copied().unwrap_or(Rect::new(sw * 0.5 - 22.0, SURF_H - 58.0, 44.0, 44.0));
        let sc = self.scale().max(1.0);
        Rect::new(
            self.at.0 as f32 + r.x * sc,
            self.at.1 as f32 + r.y * sc,
            r.w * sc,
            r.h * sc,
        )
    }

    /// Replace the running-apps slice; returns `true` only if the dock changed visually.
    /// Preserves any captured window thumbnail while an app remains minimized.
    pub fn set_apps(&mut self, mut apps: Vec<RunningApp>) -> bool {
        // Prune thumbnails for windows that are no longer open.
        self.thumbs.retain(|&raw, _| {
            apps.iter().any(|a| {
                a.hwnd.0 as isize == raw || a.windows.iter().any(|(w, _)| w.0 as isize == raw)
            })
        });
        // Check if any launching app has now appeared as running with a window or process
        self.launching.retain(|target, _| {
            let running = apps.iter().any(|a| {
                if !a.running {
                    return false;
                }
                let t = target.to_ascii_lowercase();
                a.launch.as_ref().is_some_and(|l| l.to_ascii_lowercase() == t || l.to_ascii_lowercase().contains(&t))
                    || a.exe.as_ref().is_some_and(|e| e.to_ascii_lowercase() == t || e.to_ascii_lowercase().contains(&t))
                    || a.key.to_ascii_lowercase() == t
                    || a.name.to_ascii_lowercase() == t
            });
            !running
        });
        for next in &mut apps {
            if next.minimized && next.thumb.is_none() {
                if let Some((img, _)) = self.thumbs.get(&(next.hwnd.0 as isize)) {
                    next.thumb = Some(img.clone());
                } else if let Some(prev) = self.apps.iter().find(|a| a.hwnd == next.hwnd) {
                    next.thumb = prev.thumb.clone();
                }
            }
        }
        if self.apps.len() == apps.len() && self.apps.iter().zip(&apps).all(|(a, b)| a.visual_eq(b)) {
            return false;
        }
        self.apps = apps;
        let total = self.total_slots();
        self.scales.resize_with(total, || mag_spring(1.0));
        self.slot_x_offsets.resize_with(total, || shift_spring(0.0));
        self.bounces.resize(total, 0.0);
        if self.hovered.is_some_and(|i| i >= total) {
            self.hovered = None;
        }
        if self.pressed.is_some_and(|i| i != usize::MAX && i >= total) {
            self.pressed = None;
        }
        self.update_mag_targets(false);
        self.dirty = true;
        true
    }

    /// Attach a captured window thumbnail together with its exact pre-minimize screen `RECT`
    /// so restoring a normal or maximized window lands at the exact pixel frame without jumping.
    pub fn set_app_thumb_with_rect(&mut self, hwnd: HWND, thumb: Image, rect: RECT) {
        if self.thumbs.len() >= 24 && !self.thumbs.contains_key(&(hwnd.0 as isize)) {
            if let Some(&old) = self.thumbs.keys().next() {
                self.thumbs.remove(&old);
            }
        }
        self.thumbs.insert(hwnd.0 as isize, (thumb.clone(), Some(rect)));
        if let Some(app) = self
            .apps
            .iter_mut()
            .find(|a| a.hwnd == hwnd || a.windows.iter().any(|(w, _)| *w == hwnd))
        {
            app.thumb = Some(thumb);
            self.dirty = true;
        }
    }

    /// Return the cached window thumbnail for `hwnd`, if captured when minimized.
    pub fn app_thumb(&self, hwnd: HWND) -> Option<Image> {
        self.thumbs
            .get(&(hwnd.0 as isize))
            .map(|(img, _)| img.clone())
            .or_else(|| self.apps.iter().find(|a| a.hwnd == hwnd).and_then(|a| a.thumb.clone()))
    }

    /// Return the exact pre-minimize screen `RECT` cached for `hwnd`, if known.
    pub fn app_thumb_rect(&self, hwnd: HWND) -> Option<RECT> {
        self.thumbs.get(&(hwnd.0 as isize)).and_then(|(_, rc)| *rc)
    }

    /// Return the screen-pixel icon rect for the dock slot owning `hwnd`, if present on the dock.
    pub fn icon_screen_rect_for_hwnd(&self, hwnd: HWND) -> Option<Rect> {
        let idx = self
            .visible_apps()
            .iter()
            .position(|a| a.hwnd == hwnd || a.windows.iter().any(|(w, _)| *w == hwnd))?;
        Some(self.slot_screen_rect(ANCHOR_SLOTS.len() + idx))
    }

    fn current_scales(&self) -> Vec<f32> {
        self.scales.iter().map(|s| s.value).collect()
    }

    fn reset_mag_targets(&mut self, instant: bool) {
        for s in &mut self.scales {
            if instant {
                s.snap(1.0);
            } else {
                s.set_target(1.0);
            }
        }
    }

    fn update_mag_targets(&mut self, instant: bool) {
        let total = self.total_slots();
        self.scales.resize_with(total, || mag_spring(1.0));
        if !self.cfg.magnification || self.tucked || self.tuck.value >= 0.25 || self.fs_hidden {
            self.reset_mag_targets(instant);
            return;
        }
        let Some(p) = self.mouse else {
            self.reset_mag_targets(instant);
            return;
        };
        let sw = self.surf_w_dip();
        let rest_scales = vec![1.0f32; total];
        let tray_w = self.effective_tray_w();
        let app_len = self.visible_apps().len();
        let stack_n = self.stack_count();
        let (rest_shelf, _, _, sep3_x, rest_rects, _) = layout_slots_configured(
            sw,
            ANCHOR_SLOTS.len(),
            app_len,
            stack_n,
            tray_w,
            &rest_scales,
            self.tuck.value,
            self.cfg.slot_max(),
            self.cfg.shelf_h(),
        );
        let icons_right = sep3_x.unwrap_or_else(|| rest_shelf.right());
        let active_band = Rect::new(
            rest_shelf.x - 16.0,
            (rest_shelf.y - 36.0).max(0.0),
            (icons_right - rest_shelf.x + 20.0).max(32.0),
            rest_shelf.h + 44.0,
        );
        if !active_band.contains(p) {
            self.reset_mag_targets(instant);
            return;
        }
        let sep_count = usize::from(app_len > 0) + stack_n + usize::from(sep3_x.is_some());
        let eff_tray = if sep3_x.is_some() { tray_w } else { 0.0 };
        let (base_slot, base_gap) = slot_metrics_with_seps_and_tray_max(sw, total, sep_count, eff_tray, self.cfg.slot_max());
        let step = base_slot + base_gap;
        for (i, r) in rest_rects.iter().enumerate() {
            let target = mag_scale(p.x - (r.x + r.w * 0.5), step);
            if instant {
                self.scales[i].snap(target);
            } else {
                self.scales[i].set_target(target);
            }
        }
    }

    fn tray_status_screen_rect(&self) -> Rect {
        let sw = self.surf_w_dip();
        let scales = self.current_scales();
        let tray_w = self.effective_tray_w();
        let (_, _, _, _, _, tray_rect) = layout_slots_configured(
            sw,
            ANCHOR_SLOTS.len(),
            self.visible_apps().len(),
            self.stack_count(),
            tray_w,
            &scales,
            self.tuck.value,
            self.cfg.slot_max(),
            self.cfg.shelf_h(),
        );
        let tr = tray_rect.unwrap_or(Rect::new(sw - 220.0, SURF_H - 52.0, 180.0, 38.0));
        let (_, status_r, _) = tray_zone_rects(tr, &self.tray);
        let sc = self.scale().max(1.0);
        Rect::new(
            self.at.0 as f32 + status_r.x * sc,
            self.at.1 as f32 + status_r.y * sc,
            status_r.w * sc,
            status_r.h * sc,
        )
    }

    pub fn set_monitor(&mut self, mon: &MonitorInfo, light: bool) {
        let next = surface_rect_for(mon, self.bottom_most);
        if next != self.at || mon.dpi != self.dpi || light != self.light {
            self.at = next;
            self.dpi = mon.dpi;
            self.light = light;
            self.mask = None;
            self.dirty = true;
            if let Some(g) = self.gfx.as_mut() {
                g.set_dpi(mon.dpi as f32);
                let _ = g.set_surface(next.2, next.3);
            }
        }
    }

    pub fn set_fullscreen_hidden(&mut self, hide: bool) {
        if self.fs_hidden == hide {
            return;
        }
        self.fs_hidden = hide;
        if hide {
            self.mouse = None;
            self.hovered = None;
            self.pressed = None;
            self.hovered_tray = None;
            self.pressed_tray = None;
            self.tracking = false;
            self.reset_mag_targets(true);
            if self.shown {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                if let Some(g) = self.gfx.as_mut() {
                    g.set_shown(false);
                }
            }
        } else {
            self.dirty = true;
            if self.shown {
                unsafe {
                    let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                }
                if let Some(g) = self.gfx.as_mut() {
                    g.set_shown(true);
                }
                self.raise_topmost();
            }
        }
    }

    pub fn raise_topmost(&self) {
        if self.shown && !self.fs_hidden {
            unsafe {
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
        }
    }

    pub fn set_tucked(&mut self, tucked: bool, instant: bool) {
        if self.tucked == tucked {
            return;
        }
        self.tucked = tucked;
        let goal = if tucked { 1.0 } else { 0.0 };
        if instant {
            self.tuck.snap(goal);
        } else {
            self.tuck.set_target(goal);
        }
        self.hovered = None;
        self.pressed = None;
        self.hovered_tray = None;
        self.pressed_tray = None;
        if tucked {
            self.reset_mag_targets(instant);
        }
        self.mask = None;
        self.dirty = true;
    }

    pub fn toggle_tuck(&mut self, instant: bool) {
        self.set_tucked(!self.tucked, instant);
    }

    fn slot_at(&self, p: Point) -> Option<usize> {
        if self.tucked && self.tuck.value >= 0.5 {
            return None;
        }
        let sw = self.surf_w_dip();
        let scales = self.current_scales();
        let tray_w = self.effective_tray_w();
        let (_, _, _, _, rects, _) = layout_slots_configured(
            sw,
            ANCHOR_SLOTS.len(),
            self.visible_apps().len(),
            self.stack_count(),
            tray_w,
            &scales,
            self.tuck.value,
            self.cfg.slot_max(),
            self.cfg.shelf_h(),
        );
        rects.iter().position(|r| r.inset(-3.0, -6.0).contains(p))
    }

    fn tray_zone_at(&self, p: Point) -> Option<DockTrayZone> {
        if !self.cfg.show_tray || (self.tucked && self.tuck.value >= 0.5) {
            return None;
        }
        let sw = self.surf_w_dip();
        let scales = self.current_scales();
        let tray_w = self.effective_tray_w();
        let (_, _, _, _, _, tray_rect) = layout_slots_configured(
            sw,
            ANCHOR_SLOTS.len(),
            self.visible_apps().len(),
            self.stack_count(),
            tray_w,
            &scales,
            self.tuck.value,
            self.cfg.slot_max(),
            self.cfg.shelf_h(),
        );
        let tr = tray_rect?;
        if !tr.inset(-2.0, -4.0).contains(p) {
            return None;
        }
        let (weather_r, status_r, clock_r) = tray_zone_rects(tr, &self.tray);
        if let Some(wr) = weather_r {
            if wr.inset(-2.0, -4.0).contains(p) {
                return Some(DockTrayZone::Weather);
            }
        }
        if clock_r.inset(-2.0, -4.0).contains(p) {
            Some(DockTrayZone::Clock)
        } else if status_r.inset(-2.0, -4.0).contains(p) {
            let (chev_r, _) = tray_chevron_and_status_rects(status_r);
            if chev_r.inset(-2.0, -4.0).contains(p) {
                Some(DockTrayZone::Chevron)
            } else {
                Some(DockTrayZone::Status)
            }
        } else {
            Some(DockTrayZone::Status)
        }
    }

    pub fn handle_message(&mut self, hwnd: HWND, msg: u32, _wp: WPARAM, lp: LPARAM, instant: bool) -> (Option<LRESULT>, DockAction) {
        if hwnd != self.hwnd {
            return (None, DockAction::None);
        }
        let to_dip = |lp: LPARAM, scale: f32| {
            let x = (lp.0 & 0xffff) as u16 as i16 as f32;
            let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as f32;
            Point::new(x / scale, y / scale)
        };
        match msg {
            WM_MOUSEACTIVATE => (Some(LRESULT(MA_NOACTIVATE as isize)), DockAction::None),
            WM_DPICHANGED => (Some(LRESULT(0)), DockAction::None),
            WM_MOUSEMOVE => {
                if self.fs_hidden {
                    return (Some(LRESULT(0)), DockAction::None);
                }
                if !self.tracking {
                    self.tracking = true;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: self.hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                }
                let p = to_dip(lp, self.scale());
                let moved = self.mouse != Some(p);
                self.mouse = Some(p);

                // Drag-to-reorder & drag-out-to-unpin for pinned app slots.
                if let (Some(slot_idx), Some(start)) = (self.pressed, self.drag_start) {
                    if self.cfg.show_apps
                        && slot_idx >= ANCHOR_SLOTS.len()
                        && slot_idx < ANCHOR_SLOTS.len() + self.apps.len()
                    {
                        let app_idx = slot_idx - ANCHOR_SLOTS.len();
                        if self.apps.get(app_idx).is_some_and(|a| a.pinned) {
                            let dx = (p.x - start.x).abs();
                            let dy = (p.y - start.y).abs();
                            if !self.dragging_app && (dx > 7.0 || dy > 7.0) {
                                self.dragging_app = true;
                                self.hovered = None;
                            }
                            if self.dragging_app {
                                let sw = self.surf_w_dip();
                                let scales = self.current_scales();
                                let tray_w = self.effective_tray_w();
                                let (shelf, _, _, _, rects, _) = layout_slots_configured(
                                    sw,
                                    ANCHOR_SLOTS.len(),
                                    self.apps.len(),
                                    self.stack_count(),
                                    tray_w,
                                    &scales,
                                    self.tuck.value,
                                    self.cfg.slot_max(),
                                    self.cfg.shelf_h(),
                                );
                                self.drag_out = !self.apps[app_idx].running && p.y < shelf.y - 54.0;
                                if !self.drag_out {
                                    let pinned_end = self.apps.iter().take_while(|a| a.pinned).count();
                                    let mut target_app = app_idx;
                                    for j in 0..pinned_end {
                                        let Some(r) = rects.get(ANCHOR_SLOTS.len() + j) else { continue };
                                        let cx = r.x + r.w * 0.5;
                                        if (j < app_idx && p.x < cx) || (j > app_idx && p.x > cx) {
                                            target_app = j;
                                            break;
                                        }
                                    }
                                    if target_app != app_idx {
                                        let total = self.total_slots();
                                        self.slot_x_offsets.resize_with(total, || shift_spring(0.0));
                                        let item = self.apps.remove(app_idx);
                                        self.apps.insert(target_app, item);
                                        if !instant {
                                            if app_idx < target_app {
                                                for k in app_idx..target_app {
                                                    let cur = ANCHOR_SLOTS.len() + k;
                                                    let prev = cur + 1;
                                                    if let (Some(old_r), Some(new_r)) = (rects.get(prev), rects.get(cur)) {
                                                        self.slot_x_offsets[cur].value += old_r.x - new_r.x;
                                                        self.slot_x_offsets[cur].set_target(0.0);
                                                    }
                                                }
                                            } else {
                                                for k in (target_app + 1)..=app_idx {
                                                    let cur = ANCHOR_SLOTS.len() + k;
                                                    let prev = cur - 1;
                                                    if let (Some(old_r), Some(new_r)) = (rects.get(prev), rects.get(cur)) {
                                                        self.slot_x_offsets[cur].value += old_r.x - new_r.x;
                                                        self.slot_x_offsets[cur].set_target(0.0);
                                                    }
                                                }
                                            }
                                        }
                                        self.slot_x_offsets[ANCHOR_SLOTS.len() + target_app].snap(0.0);
                                        self.pressed = Some(ANCHOR_SLOTS.len() + target_app);
                                    }
                                }
                                self.dirty = true;
                                return (Some(LRESULT(0)), DockAction::RequestFrame);
                            }
                        }
                    }
                }

                let next_hover = self.slot_at(p);
                let next_tray = if next_hover.is_none() { self.tray_zone_at(p) } else { None };
                let hover_changed = next_hover != self.hovered || next_tray != self.hovered_tray;
                self.hovered = next_hover;
                self.hovered_tray = next_tray;
                if moved && !self.tucked {
                    self.update_mag_targets(instant);
                }
                if hover_changed || (moved && !self.tucked) {
                    self.dirty = true;
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                (Some(LRESULT(0)), DockAction::None)
            }
            WM_MOUSELEAVE => {
                if self.dragging_app {
                    return (Some(LRESULT(0)), DockAction::None);
                }
                self.tracking = false;
                self.mouse = None;
                self.drag_start = None;
                let had_ui = self.hovered.take().is_some()
                    || self.pressed.take().is_some()
                    || self.hovered_tray.take().is_some()
                    || self.pressed_tray.take().is_some();
                let had_mag = self.scales.iter().any(|s| s.target > 1.0 + MAG_EPS || s.value > 1.0 + MAG_EPS);
                self.reset_mag_targets(instant);
                if had_ui || had_mag {
                    self.dirty = true;
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                (Some(LRESULT(0)), DockAction::None)
            }
            WM_LBUTTONDOWN => {
                let p = to_dip(lp, self.scale());
                unsafe {
                    SetCapture(self.hwnd);
                }
                self.drag_start = Some(p);
                self.dragging_app = false;
                self.drag_out = false;
                if self.tucked && peek_hit_rect(self.surf_w_dip()).contains(p) {
                    self.pressed = Some(usize::MAX);
                    return (Some(LRESULT(0)), DockAction::None);
                }
                let hit = self.slot_at(p);
                let tray_hit = if hit.is_none() { self.tray_zone_at(p) } else { None };
                if hit != self.pressed || tray_hit != self.pressed_tray {
                    self.pressed = hit;
                    self.pressed_tray = tray_hit;
                    self.dirty = true;
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                (Some(LRESULT(0)), DockAction::None)
            }
            WM_LBUTTONUP => {
                let p = to_dip(lp, self.scale());
                unsafe {
                    let _ = ReleaseCapture();
                }
                let down = self.pressed.take();
                let down_tray = self.pressed_tray.take();
                let was_dragging = self.dragging_app;
                let was_drag_out = self.drag_out;
                self.drag_start = None;
                self.dragging_app = false;
                self.drag_out = false;
                self.dirty = true;
                if was_dragging {
                    if let Some(slot_idx) = down {
                        let app_idx = slot_idx.saturating_sub(ANCHOR_SLOTS.len());
                        let sw = self.surf_w_dip();
                        let scales = self.current_scales();
                        let tray_w = self.effective_tray_w();
                        let (_, _, _, _, old_rects, _) = layout_slots_configured(
                            sw,
                            ANCHOR_SLOTS.len(),
                            self.visible_apps().len(),
                            self.stack_count(),
                            tray_w,
                            &scales,
                            self.tuck.value,
                            self.cfg.slot_max(),
                            self.cfg.shelf_h(),
                        );
                        if was_drag_out {
                            if let Some(app) = self.apps.get(app_idx).filter(|a| a.pinned && !a.running) {
                                if let Some(spec) = app.launch.clone().or_else(|| app.exe.clone()) {
                                    if !instant {
                                        let base_y = old_rects.get(slot_idx).map_or(44.0, |r| r.y + r.h * 0.5);
                                        let poof_x = p.x.clamp(28.0, (sw - 28.0).max(28.0));
                                        let poof_y = p.y.clamp(24.0, base_y.max(24.0));
                                        self.poofs.push(PoofEffect::new(Point::new(poof_x, poof_y)));
                                    }
                                    self.apps.remove(app_idx);
                                    let total = self.total_slots();
                                    self.scales.resize_with(total, || mag_spring(1.0));
                                    self.slot_x_offsets.resize_with(total, || shift_spring(0.0));
                                    self.bounces.resize(total, 0.0);
                                    if !instant {
                                        let new_scales = self.current_scales();
                                        let (_, _, _, _, new_rects, _) = layout_slots_configured(
                                            sw,
                                            ANCHOR_SLOTS.len(),
                                            self.visible_apps().len(),
                                            self.stack_count(),
                                            tray_w,
                                            &new_scales,
                                            self.tuck.value,
                                            self.cfg.slot_max(),
                                            self.cfg.shelf_h(),
                                        );
                                        for (ni, new_r) in new_rects.iter().enumerate() {
                                            let oi = if ni < slot_idx { ni } else { ni + 1 };
                                            if let Some(old_r) = old_rects.get(oi) {
                                                self.slot_x_offsets[ni].value += old_r.x - new_r.x;
                                                self.slot_x_offsets[ni].set_target(0.0);
                                            }
                                        }
                                    }
                                    return (Some(LRESULT(0)), DockAction::RemovePinnedApp(spec));
                                }
                            }
                        }
                        if !instant {
                            let total = self.total_slots();
                            self.slot_x_offsets.resize_with(total, || shift_spring(0.0));
                            if let (Some(base_r), Some(off)) =
                                (old_rects.get(slot_idx), self.slot_x_offsets.get_mut(slot_idx))
                            {
                                let delta = (p.x - (base_r.x + base_r.w * 0.5)).clamp(-sw * 0.5, sw * 0.5);
                                off.value = delta;
                                off.set_target(0.0);
                            }
                        }
                        let new_order: Vec<String> = self
                            .apps
                            .iter()
                            .filter(|a| a.pinned)
                            .filter_map(|a| a.launch.clone().or_else(|| a.exe.clone()))
                            .collect();
                        return (Some(LRESULT(0)), DockAction::ReorderPinned(new_order));
                    }
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                if down == Some(usize::MAX) && self.tucked && peek_hit_rect(self.surf_w_dip()).contains(p) {
                    self.set_tucked(false, instant);
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                if let Some(z) = down_tray.filter(|&z| Some(z) == self.tray_zone_at(p)) {
                    let act = match z {
                        DockTrayZone::Weather | DockTrayZone::Clock => DockAction::OpenPage(PageKind::Today),
                        DockTrayZone::Chevron => DockAction::ToggleTray(self.tray_status_screen_rect()),
                        DockTrayZone::Status => DockAction::ToggleControlCenter(self.tray_status_screen_rect()),
                    };
                    return (Some(LRESULT(0)), act);
                }
                if let Some(i) = down.filter(|&i| Some(i) == self.slot_at(p)) {
                    self.trigger_bounce(i, instant);
                    if i < ANCHOR_SLOTS.len() {
                        let act = match ANCHOR_SLOTS[i].action {
                            SlotAction::Launchpad => DockAction::OpenLaunchpad,
                            SlotAction::ToggleIsland => DockAction::ToggleIsland,
                            SlotAction::Page(pk) => DockAction::OpenPage(pk),
                            SlotAction::Settings => DockAction::OpenSettings,
                        };
                        return (Some(LRESULT(0)), act);
                    }
                    let vis_apps = self.visible_apps();
                    if let Some(app) = vis_apps.get(i - ANCHOR_SLOTS.len()) {
                        if !app.running {
                            if let Some(spec) = app.launch.clone().or_else(|| app.exe.clone()) {
                                return (Some(LRESULT(0)), DockAction::LaunchPinnedApp(spec));
                            }
                        }
                        let icon_rect = self.slot_screen_rect(i);
                        return (
                            Some(LRESULT(0)),
                            DockAction::ToggleAppWindow {
                                hwnd: app.hwnd,
                                minimized: app.minimized,
                                focused: app.focused,
                                icon_rect,
                            },
                        );
                    }
                    if Some(i) == self.downloads_slot_index() {
                        let icon_rect = self.slot_screen_rect(i);
                        return (Some(LRESULT(0)), DockAction::ToggleDownloadsStack(icon_rect));
                    }
                    if i == self.trash_slot_index() {
                        return (Some(LRESULT(0)), DockAction::OpenRecycleBin);
                    }
                }
                (Some(LRESULT(0)), DockAction::RequestFrame)
            }
            WM_DROPFILES => {
                let hdrop = HDROP(_wp.0 as *mut core::ffi::c_void);
                let mut pt = POINT::default();
                let mut files = Vec::new();
                unsafe {
                    let _ = DragQueryPoint(hdrop, &mut pt);
                    let count = DragQueryFileW(hdrop, u32::MAX, None);
                    for idx in 0..count {
                        let len = DragQueryFileW(hdrop, idx, None) as usize;
                        if len == 0 {
                            continue;
                        }
                        let mut buf = vec![0u16; len + 1];
                        let n = DragQueryFileW(hdrop, idx, Some(&mut buf)) as usize;
                        if n > 0 {
                            files.push(String::from_utf16_lossy(&buf[..n.min(len)]));
                        }
                    }
                    DragFinish(hdrop);
                }
                if files.is_empty() {
                    return (Some(LRESULT(0)), DockAction::None);
                }
                let scale = self.scale().max(1.0);
                let p = Point::new(pt.x as f32 / scale, pt.y as f32 / scale);
                if let Some(i) = self.slot_at(p) {
                    self.trigger_bounce(i, instant);
                    if i == self.trash_slot_index() {
                        return (Some(LRESULT(0)), DockAction::DropToRecycleBin(files));
                    }
                    let vis_apps = self.visible_apps();
                    if i >= ANCHOR_SLOTS.len() && i < ANCHOR_SLOTS.len() + vis_apps.len() {
                        let app = &vis_apps[i - ANCHOR_SLOTS.len()];
                        return (
                            Some(LRESULT(0)),
                            DockAction::OpenFilesWithApp {
                                hwnd: app.hwnd,
                                exe: app.exe.clone(),
                                files,
                            },
                        );
                    }
                }
                (Some(LRESULT(0)), DockAction::DropToShelf(files))
            }
            WM_RBUTTONUP => {
                let p = to_dip(lp, self.scale());
                let target = match self.slot_at(p) {
                    Some(i) if i == self.trash_slot_index() => DockMenuTarget::Trash,
                    Some(i) => {
                        let vis_apps = self.visible_apps();
                        if i >= ANCHOR_SLOTS.len() && i < ANCHOR_SLOTS.len() + vis_apps.len() {
                            DockMenuTarget::App(vis_apps[i - ANCHOR_SLOTS.len()].clone())
                        } else {
                            DockMenuTarget::Shelf
                        }
                    }
                    None => DockMenuTarget::Shelf,
                };
                let mut pt = POINT {
                    x: (lp.0 & 0xffff) as u16 as i16 as i32,
                    y: ((lp.0 >> 16) & 0xffff) as u16 as i16 as i32,
                };
                unsafe {
                    let _ = ClientToScreen(self.hwnd, &mut pt);
                }
                (Some(LRESULT(0)), DockAction::ShowMenu(pt, self.tucked, target))
            }
            WM_CAPTURECHANGED => {
                self.drag_start = None;
                self.dragging_app = false;
                self.drag_out = false;
                if self.pressed.take().is_some() || self.pressed_tray.take().is_some() {
                    self.dirty = true;
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                (Some(LRESULT(0)), DockAction::None)
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                let hand = self.hovered.is_some()
                    || self.hovered_tray.is_some()
                    || (self.tucked && self.mouse.is_some_and(|p| peek_hit_rect(self.surf_w_dip()).contains(p)));
                unsafe {
                    let id = if hand { IDC_HAND } else { IDC_ARROW };
                    if let Ok(c) = LoadCursorW(None, id) {
                        SetCursor(c);
                    }
                }
                (Some(LRESULT(1)), DockAction::None)
            }
            _ => (None, DockAction::None),
        }
    }

    pub fn paint(&mut self, now: f64, dt: f32, instant: bool, shared: Option<SharedD3d>) {
        if self.fs_hidden {
            return;
        }
        if instant {
            self.tuck.snap(self.tuck.target);
            for s in &mut self.scales {
                s.snap(s.target);
            }
            for o in &mut self.slot_x_offsets {
                o.snap(0.0);
            }
            self.bounces.fill(0.0);
            self.launching.clear();
            self.poofs.clear();
        } else {
            self.tuck.step_eps(dt, TUCK_EPS);
            for s in &mut self.scales {
                s.step_eps(dt, MAG_EPS);
            }
            for o in &mut self.slot_x_offsets {
                o.step_eps(dt, SLOT_SHIFT_EPS);
            }
            if !self.poofs.is_empty() {
                self.poofs.retain_mut(|poof| poof.step(dt));
            }
            // Step launch timers and keep bouncing active launching apps
            if !self.launching.is_empty() {
                self.launching.retain(|_, time_left| {
                    *time_left -= dt;
                    *time_left > 0.0
                });
                for (target, _) in &self.launching {
                    if let Some(i) = self.find_slot_by_spec(target) {
                        if let Some(b) = self.bounces.get_mut(i) {
                            if *b <= 0.0 {
                                *b = 0.001;
                            }
                        }
                    }
                }
            }
            for b in &mut self.bounces {
                if *b > 0.0 {
                    *b += dt / BOUNCE_DURATION;
                    if *b >= 1.0 {
                        *b = 0.0;
                    }
                }
            }
        }
        let (x, y, w, h) = self.at;
        let scale = self.scale().max(1.0);
        let surf_w = self.surf_w_dip();
        let tuck_t = self.tuck.value.clamp(0.0, 1.0);
        let spec_x = self.mouse.map(|p| p.x);
        let hovered = self.hovered;
        let pressed = self.pressed;
        let hovered_tray = self.hovered_tray;
        let pressed_tray = self.pressed_tray;
        let tray = self.tray;
        let tray_w = self.effective_tray_w();
        let scales = self.current_scales();
        let vis_apps_len = self.visible_apps().len();
        let stack_n = self.stack_count();
        let (shelf, sep_x, sep2_x, sep3_x, mut slot_rects, tray_rect) = layout_slots_configured(
            surf_w,
            ANCHOR_SLOTS.len(),
            vis_apps_len,
            stack_n,
            tray_w,
            &scales,
            tuck_t,
            self.cfg.slot_max(),
            self.cfg.shelf_h(),
        );
        for (i, r) in slot_rects.iter_mut().enumerate() {
            let dx = self.slot_x_offsets.get(i).map_or(0.0, |s| s.value);
            if dx != 0.0 {
                r.x += dx;
            }
            let dy = self.bounces.get(i).copied().map_or(0.0, bounce_offset_y);
            if dy != 0.0 {
                r.y += dy;
            }
        }
        let max_lift = slot_rects.iter().fold(0.0f32, |m, r| m.max((shelf.y - r.y).max(0.0)));
        let stack_open = self.stack_open;
        let ctrl_open = self.ctrl_open;
        let show_apps = self.cfg.show_apps;
        let show_downloads = self.cfg.show_downloads;
        let badge_mode = self.cfg.badge_mode;
        let glass_mat = self.cfg.glass_material;
        let drag_pos = if self.dragging_app { self.mouse } else { None };
        let drag_out = self.drag_out;

        let Some(gfx) = self.gfx.as_mut() else { return };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if let Err(e) = gfx.set_surface(w, h) {
            crate::pwarn!("dock: set_surface failed ({e})");
            return;
        }
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        let theme_look = self.theme_look;
        let (look, glass_params) = if tuck_t < 0.995 && theme_look.material == look::Material::Glass {
            let r = SHELF_R.min(shelf.h * 0.5);
            self.glass.app_light = theme_look.light;
            let (light, _gp, moving) = self.glass.frame(shelf, r, -1.0, true, now, dt, scale, gfx);
            self.glass_moving = moving;
            let gl = look::glass(light);
            self.look = gl;
            self.light = light;
            gfx.set_look(gl);
            let custom_gp = dock_glass_params_with_material(shelf, r, gl, scale, glass_mat);
            gfx.prepare_glass(&custom_gp, full);
            (gl, Some(custom_gp))
        } else {
            self.glass_moving = false;
            self.look = theme_look;
            self.light = theme_look.light;
            gfx.set_look(theme_look);
            (theme_look, None)
        };
        let apps_slice: &[RunningApp] = if show_apps { &self.apps } else { &[] };
        match gfx.begin(full) {
            Ok(mut c) => {
                draw_dock(
                    &mut c,
                    surf_w,
                    look,
                    glass_params.as_ref(),
                    spec_x,
                    tuck_t,
                    shelf,
                    sep_x,
                    sep2_x,
                    sep3_x,
                    &slot_rects,
                    apps_slice,
                    show_downloads,
                    badge_mode,
                    stack_open,
                    ctrl_open,
                    self.preview_open,
                    hovered,
                    pressed,
                    drag_pos,
                    drag_out,
                    &self.poofs,
                    tray_rect,
                    &tray,
                    hovered_tray,
                    pressed_tray,
                );
                if let Err(e) = gfx.end() {
                    crate::pwarn!("dock: EndDraw failed ({e})");
                }
            }
            Err(e) => {
                crate::pwarn!("dock: begin failed ({e})");
                return;
            }
        }

        let changed_in_begin = gfx.take_renderer_changed();
        if let Err(e) = gfx.present(x, y) {
            crate::pwarn!("dock: present failed ({e})");
            return;
        }
        let lost_in_present = gfx.take_renderer_changed();
        if changed_in_begin || lost_in_present {
            self.mask = None;
        }

        if gfx.needs_hit_mask() {
            let mask = hit_mask_for_shelf(surf_w, shelf, max_lift, self.tucked, tuck_t);
            let settled = self.tuck.is_settled_eps(TUCK_EPS)
                && self.scales.iter().all(|s| s.is_settled_eps(MAG_EPS))
                && self.bounces.iter().all(|&b| b <= 0.0);
            let due = match &self.mask {
                None => true,
                Some((m, at)) => *m != mask && (settled || now - at >= MASK_INTERVAL),
            };
            if due {
                let prev = self.mask.map(|(m, _)| m);
                match gfx.push_hit_mask(x, y, &mask, prev.as_ref()) {
                    Ok(()) => self.mask = Some((mask, now)),
                    Err(e) => {
                        crate::pwarn!("dock: hit mask failed ({e})");
                        self.mask = None;
                    }
                }
            }
        }

        if !self.shown {
            self.shown = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            gfx.set_shown(true);
        }
        self.dirty = lost_in_present;
    }
}

fn draw_glass_divider(c: &mut Canvas<'_>, sx: f32, shelf: Rect, light: bool) {
    let div_h = (shelf.h * 0.62).clamp(22.0, 38.0);
    let div_y = shelf.y + (shelf.h - div_h) * 0.5;
    let mid_y = div_y + div_h * 0.5;
    let col = if light { Color::black(1.0) } else { Color::white(1.0) };
    let peak = if light { 0.18 } else { 0.28 };
    let top_seg = crate::pill::shape::segments(Rect::new(sx - 0.5, div_y, 1.0, div_h * 0.5), 0.5);
    let bot_seg = crate::pill::shape::segments(Rect::new(sx - 0.5, mid_y, 1.0, div_h * 0.5), 0.5);
    c.fill_path_fade(&top_seg, col, div_y, mid_y, 0.0, peak);
    c.fill_path_fade(&bot_seg, col, mid_y, div_y + div_h, peak, 0.0);
}

fn draw_liquid_glass_shelf(c: &mut Canvas<'_>, surf_w: f32, shelf: Rect, r: f32, look: &Look, glass: Option<&GlassParams>, spec_x: Option<f32>) {
    if let Some(gp) = glass {
        // True Halo Island Glass theme shader pass: live frosted + lens-refracted backdrop,
        // directional rim highlight, inner edge glow, and adaptive drop shadow from res/glass.hlsl.
        c.glass(gp, Rect::new(0.0, 0.0, surf_w, SURF_H));
    } else {
        // Non-Glass theme body (Classic, Halo, Pill, Diorama, Custom): 100% identical to Halo Island's pill::shape::draw_body.
        crate::pill::shape::draw_body(c, shelf, r, look.ground, Some(look.border));
    }

    // Subtle pointer-reactive #dockSpecular highlight along the top edge while hovering.
    if let Some(sx) = spec_x {
        let light = look.light;
        let segs = crate::pill::shape::segments(shelf, r);
        c.push_outline_mask(&segs);
        let sx = sx.clamp(shelf.x + r, (shelf.right() - r).max(shelf.x + r));
        let spec_col = look.accent.unwrap_or(Color::white(1.0));
        let glow_tint = Color::lerp(Color::white(1.0), spec_col, if look.accent.is_some() { 0.22 } else { 0.0 });
        let mul = if light { 0.65 } else { 0.55 };
        for (gw, gh, ga) in [(250.0f32, 62.0f32, 0.044f32), (170.0, 40.0, 0.058), (108.0, 24.0, 0.072), (58.0, 13.0, 0.082)] {
            let gr = Rect::new(sx - gw * 0.5, shelf.y - gh * 0.44, gw, gh);
            c.fill_rrect(gr, gh * 0.5, glow_tint.with_alpha(ga * mul));
        }
        c.pop_shape_mask();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DockBadgeStyle {
    Red,
    Blue,
}

/// Whether a running app key belongs to a known desktop browser (for download progress ring).
pub fn is_browser_key(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    matches!(
        k.as_str(),
        "chrome" | "msedge" | "firefox" | "brave" | "opera" | "vivaldi" | "arc" | "zen" | "chromium"
    )
}

/// Compute the notification badge pill rectangle anchored at the top-right corner of a tile.
pub fn badge_rect(tile: Rect, count: u16) -> Rect {
    let h = 16.0f32.min((tile.h * 0.42).max(13.0));
    let w = if count >= 100 {
        h + 10.0
    } else if count >= 10 {
        h + 5.0
    } else {
        h
    };
    Rect::new(tile.right() - w + 3.0, tile.y - 3.0, w, h)
}

/// Radius of the circular download progress ring wrapped around a dock tile.
pub fn progress_ring_radius(tile: Rect) -> f32 {
    tile.w * 0.5 + 3.0
}

fn draw_squircle_shadow(c: &mut Canvas<'_>, tile: Rect, tr: f32, light: bool) {
    let a1 = if light { 0.14 } else { 0.34 };
    let a2 = if light { 0.06 } else { 0.16 };
    c.fill_rrect(Rect::new(tile.x, tile.y + 3.0, tile.w, tile.h), tr, Color::black(a1));
    c.fill_rrect(Rect::new(tile.x - 1.0, tile.y + 5.0, tile.w + 2.0, tile.h + 1.0), tr + 1.0, Color::black(a2));
}

fn draw_active_dot(c: &mut Canvas<'_>, center: Point, radius: f32, ink: Color, alpha: f32) {
    c.fill_circle(center, radius * 2.1, ink.with_alpha(alpha * 0.24));
    c.fill_circle(center, radius, ink.with_alpha(alpha));
}

/// Draw the 9-dot colorful Launchpad squircle icon (3×3 rounded squares matching the mockup).
fn draw_launchpad_icon(c: &mut Canvas<'_>, tile: Rect, tr: f32, light: bool, is_hover: bool) {
    const DOTS: [u32; 9] = [
        0x38BDF8, 0xF5A524, 0x4ADE80,
        0xF43F5E, 0xA78BFA, 0x2DD4BF,
        0xFBBF24, 0x60A5FA, 0xE879F9,
    ];
    let bg = if light {
        Color::rgba(0.92, 0.94, 0.98, 0.94)
    } else {
        Color::hex(0x131D2E).with_alpha(0.96)
    };
    c.fill_rrect(tile, tr, bg);
    let tile_segs = crate::pill::shape::segments(tile, tr);
    c.fill_path_fade(
        &tile_segs,
        Color::white(1.0),
        tile.y,
        tile.y + tile.h * 0.55,
        if light { 0.36 } else { 0.18 },
        0.0,
    );
    let rim = if is_hover {
        Color::hex(0x38BDF8).with_alpha(0.75)
    } else if light {
        Color::black(0.14)
    } else {
        Color::white(0.24)
    };
    c.stroke_rrect(tile, tr, rim, 1.1);

    let pad = tile.w * 0.17;
    let inner = (tile.w - pad * 2.0).max(6.0);
    let gap = inner * 0.13;
    let cell = ((inner - gap * 2.0) / 3.0).max(2.0);
    let cell_r = (cell * 0.30).clamp(1.2, 3.5);
    for (idx, &hex) in DOTS.iter().enumerate() {
        let row = (idx / 3) as f32;
        let col = (idx % 3) as f32;
        let x = tile.x + pad + col * (cell + gap);
        let y = tile.y + pad + row * (cell + gap);
        c.fill_rrect(Rect::new(x, y, cell, cell), cell_r, Color::hex(hex));
    }
}

/// Draw a macOS-style top-right notification badge pill or minimalist dot based on [`DockBadgeMode`].
fn draw_dock_badge(c: &mut Canvas<'_>, tile: Rect, count: u16, style: DockBadgeStyle, mode: DockBadgeMode) {
    if count == 0 || mode == DockBadgeMode::Hidden {
        return;
    }
    let (base, glow) = match style {
        DockBadgeStyle::Red => (Color::hex(0xE62E3D), Color::hex(0xE62E3D).with_alpha(0.42)),
        DockBadgeStyle::Blue => (Color::hex(0x2563EB), Color::hex(0x2563EB).with_alpha(0.42)),
    };

    if mode == DockBadgeMode::Dot {
        let dot_sz = 8.5f32.min(tile.w * 0.22);
        let dot_r = dot_sz * 0.5;
        let dot_rect = Rect::new(tile.right() - dot_sz + 1.0, tile.y - 1.0, dot_sz, dot_sz);
        c.fill_rrect(Rect::new(dot_rect.x, dot_rect.y + 1.0, dot_sz, dot_sz), dot_r, glow);
        c.fill_rrect(dot_rect, dot_r, base);
        c.stroke_rrect(dot_rect, dot_r, Color::white(0.45), 1.0);
        return;
    }

    let br = badge_rect(tile, count);
    let r = br.h * 0.5;
    c.fill_rrect(Rect::new(br.x, br.y + 1.5, br.w, br.h), r, glow);
    c.fill_rrect(br, r, base);
    let segs = crate::pill::shape::segments(br, r);
    c.fill_path_fade(&segs, Color::white(1.0), br.y, br.y + br.h * 0.55, 0.28, 0.0);
    c.stroke_rrect(br, r, Color::white(0.28), 1.1);

    let label = if count > 99 { "99+".to_string() } else { count.to_string() };
    let st = TextStyle::new((br.h * 0.62).clamp(9.0, 11.0))
        .weight(Weight::Bold)
        .center()
        .color(Color::white(1.0));
    c.text(&label, br, &st);
}

/// Draw a circular download progress ring around a dock slot tile (`.dl-ring-svg`).
fn draw_progress_ring(c: &mut Canvas<'_>, tile: Rect, progress: f32) {
    let p = progress.clamp(0.0, 1.0);
    let center = Point::new(tile.x + tile.w * 0.5, tile.y + tile.h * 0.5);
    let radius = progress_ring_radius(tile);
    c.ring(
        center,
        radius,
        2.5,
        p,
        Color::hex(0x38BDF8).with_alpha(0.24),
        Color::hex(0x38BDF8).with_alpha(0.96),
    );
}

/// Compute the miniature window card and corner app-badge rects for a minimized window slot (`.min-thumb`).
pub fn minimized_card_rects(tile: Rect) -> (Rect, Rect, Rect) {
    let card_w = (tile.w * 0.94).max(18.0);
    let card_h = (tile.h * 0.78).max(14.0);
    let card_x = tile.x + (tile.w - card_w) * 0.5;
    let card_y = tile.y + (tile.h - card_h) * 0.5 - 1.0;
    let card = Rect::new(card_x, card_y, card_w, card_h);
    let bar_h = (card_h * 0.22).clamp(5.0, 8.5);
    let bar = Rect::new(card.x, card.y, card.w, bar_h);
    let badge_sz = (tile.w * 0.40).clamp(12.0, 22.0);
    let badge = Rect::new(tile.right() - badge_sz + 2.0, tile.bottom() - badge_sz + 1.0, badge_sz, badge_sz);
    (card, bar, badge)
}

/// Draw the macOS Downloads Folder Stack tile icon (`#dockDownloads`: stacked document sheets
/// peeking out of a vivid sky-blue folder pocket with downward arrow).
fn draw_downloads_stack_icon(
    c: &mut Canvas<'_>,
    tile: Rect,
    tr: f32,
    accent: Color,
    light: bool,
    is_hover: bool,
    stack_open: bool,
) {
    let base_a = if stack_open { 0.28 } else if is_hover { 0.22 } else { 0.14 };
    c.fill_rrect(tile, tr, accent.with_alpha(base_a));

    // Back folder tab
    let tab_w = tile.w * 0.36;
    let tab_h = tile.h * 0.18;
    let tab = Rect::new(tile.x + tile.w * 0.14, tile.y + tile.h * 0.17, tab_w, tab_h);
    c.fill_rrect(tab, (tab_h * 0.4).max(2.0), Color::hex(0x0284C7).with_alpha(0.82));

    // Two stacked white document sheets peeking out above the folder pocket
    let doc1 = Rect::new(tile.x + tile.w * 0.21, tile.y + tile.h * 0.15, tile.w * 0.54, tile.h * 0.36);
    let doc2 = Rect::new(tile.x + tile.w * 0.26, tile.y + tile.h * 0.18, tile.w * 0.54, tile.h * 0.36);
    c.fill_rrect(doc1, 3.0, Color::hex(0xCBD5E1).with_alpha(0.92));
    c.fill_rrect(doc2, 3.0, Color::white(0.98));

    // Front folder pocket (#38bdf8 -> #0284c7)
    let pocket = Rect::new(
        tile.x + tile.w * 0.10,
        tile.y + tile.h * 0.33,
        tile.w * 0.80,
        tile.h * 0.54,
    );
    let pr = (pocket.h * 0.24).clamp(4.0, 8.0);
    c.fill_rrect(pocket, pr, Color::hex(0x0284C7).with_alpha(0.95));
    let p_segs = crate::pill::shape::segments(pocket, pr);
    c.fill_path_fade(&p_segs, Color::hex(0x38BDF8), pocket.y, pocket.bottom(), 0.92, 0.0);
    c.stroke_rrect(pocket, pr, Color::white(0.36), 1.0);

    // Crisp white download arrow centered on the folder pocket
    let ic_sz = (pocket.h * 0.58).clamp(11.0, 18.0);
    let ic_box = Rect::new(
        pocket.x + (pocket.w - ic_sz) * 0.5,
        pocket.y + (pocket.h - ic_sz) * 0.5,
        ic_sz,
        ic_sz,
    );
    c.icon(Icon::Download, ic_box, Color::white(0.98));

    // Outer squircle bevel & rim
    let tile_segs = crate::pill::shape::segments(tile, tr);
    c.fill_path_fade(&tile_segs, Color::white(1.0), tile.y, tile.y + tile.h * 0.45, if light { 0.24 } else { 0.14 }, 0.0);
    let tile_rim = if is_hover || stack_open {
        Color::hex(0x38BDF8).with_alpha(0.74)
    } else if light {
        Color::black(0.11)
    } else {
        Color::white(0.18)
    };
    c.stroke_rrect(tile, tr, tile_rim, 1.0);
}

/// Draw the macOS Trash / Recycle Bin squircle tile icon (`shell:RecycleBinFolder`).
fn draw_trash_slot_icon(c: &mut Canvas<'_>, tile: Rect, tr: f32, light: bool, is_hover: bool) {
    let bg = if light {
        Color::rgba(0.91, 0.93, 0.96, 0.92)
    } else {
        Color::hex(0x1E2633).with_alpha(0.92)
    };
    c.fill_rrect(tile, tr, bg);
    let tile_segs = crate::pill::shape::segments(tile, tr);
    c.fill_path_fade(
        &tile_segs,
        Color::white(1.0),
        tile.y,
        tile.y + tile.h * 0.52,
        if light { 0.34 } else { 0.18 },
        0.0,
    );
    let rim = if is_hover {
        Color::hex(0x38BDF8).with_alpha(0.72)
    } else if light {
        Color::black(0.12)
    } else {
        Color::white(0.22)
    };
    c.stroke_rrect(tile, tr, rim, 1.0);
    let ic_box = tile.inset(tile.w * 0.24, tile.h * 0.24);
    let ic_col = if light { Color::hex(0x334155) } else { Color::hex(0xCBD5E1) };
    c.icon(Icon::Trash, ic_box, ic_col);
}

fn draw_poof_effects(c: &mut Canvas<'_>, poofs: &[PoofEffect], light: bool) {
    for poof in poofs {
        for puff in &poof.puffs {
            let avail = (poof.duration - puff.delay).max(0.05);
            let local_t = ((poof.elapsed - puff.delay) / avail).clamp(0.0, 1.0);
            if local_t <= 0.0 || local_t >= 1.0 {
                continue;
            }
            let ease_out = 1.0 - (1.0 - local_t).powi(3);
            let dist = puff.dist * ease_out;
            let px = poof.center.x + puff.angle.cos() * dist;
            let py = poof.center.y + puff.angle.sin() * dist - local_t * 6.0;
            let r = if local_t < 0.35 {
                let s = local_t / 0.35;
                puff.r0 + (puff.r1 - puff.r0) * (1.0 - (1.0 - s).powi(2))
            } else {
                let s = (local_t - 0.35) / 0.65;
                puff.r1 * (1.0 - 0.45 * s * s)
            };
            let alpha = if local_t < 0.15 {
                local_t / 0.15
            } else {
                (1.0 - (local_t - 0.15) / 0.85).powf(1.75)
            };
            if r <= 0.5 || alpha <= 0.01 {
                continue;
            }
            c.fill_circle(Point::new(px, py + 1.5), r * 1.05, Color::black(alpha * 0.16));
            let cloud = if light {
                Color::rgba(0.93, 0.95, 0.98, alpha * 0.92)
            } else {
                Color::rgba(0.96, 0.97, 1.0, alpha * 0.88)
            };
            c.fill_circle(Point::new(px, py), r, cloud);
        }
    }
}

fn dragged_slot_rect(base_sr: Rect, drag_pos: Point, drag_out: bool) -> Rect {
    let lifted_y = if drag_out {
        (drag_pos.y - base_sr.h * 0.5).clamp(6.0, (base_sr.y - 18.0).max(6.0))
    } else {
        base_sr.y - 5.0
    };
    Rect::new(drag_pos.x - base_sr.w * 0.5, lifted_y, base_sr.w, base_sr.h)
}

#[allow(clippy::too_many_arguments)]
fn draw_dock(
    c: &mut Canvas<'_>,
    surf_w: f32,
    look: &Look,
    glass: Option<&GlassParams>,
    spec_x: Option<f32>,
    tuck_t: f32,
    shelf: Rect,
    sep_x: Option<f32>,
    sep2_x: Option<f32>,
    sep3_x: Option<f32>,
    slot_rects: &[Rect],
    apps: &[RunningApp],
    show_downloads: bool,
    badge_mode: DockBadgeMode,
    stack_open: bool,
    ctrl_open: bool,
    preview_open: bool,
    hovered: Option<usize>,
    pressed: Option<usize>,
    drag_pos: Option<Point>,
    drag_out: bool,
    poofs: &[PoofEffect],
    tray_rect: Option<Rect>,
    tray: &DockTrayState,
    hovered_tray: Option<DockTrayZone>,
    pressed_tray: Option<DockTrayZone>,
) {
    let light = look.light;
    let theme_ink = look.ink;
    let theme_accent = look.accent.unwrap_or(Color::hex(0x0A84FF));
    c.unthemed(|c| {
        if tuck_t < 0.995 {
            let alpha = (1.0 - tuck_t).clamp(0.0, 1.0);
            c.with_alpha(alpha, |c| {
                let r = SHELF_R.min(shelf.h * 0.5);
                draw_liquid_glass_shelf(c, surf_w, shelf, r, look, glass, spec_x);

                let dot_y = shelf.bottom() - 4.5;
                let dot_ink = theme_ink;

                for (i, slot) in ANCHOR_SLOTS.iter().enumerate() {
                    let Some(&sr) = slot_rects.get(i) else { continue };
                    let is_hover = hovered == Some(i);
                    let is_press = pressed == Some(i);
                    let tile = if is_press { sr.inset(1.5, 1.5) } else { sr };
                    let tr = (tile.w * 0.25).clamp(8.0, 15.0);
                    draw_squircle_shadow(c, tile, tr, light);

                    if slot.action == SlotAction::Launchpad {
                        draw_launchpad_icon(c, tile, tr, light, is_hover);
                    } else {
                        let slot_acc = if slot.action == SlotAction::ToggleIsland && look.accent.is_some() {
                            theme_accent
                        } else {
                            slot.accent
                        };
                        let fill_a = if is_press { 0.96 } else if is_hover { 0.92 } else { 0.86 };
                        c.fill_rrect(tile, tr, slot_acc.with_alpha(fill_a));
                        // Top inner bevel sheen + subtle bottom depth (macOS Sequoia rich squircle tile)
                        let tile_segs = crate::pill::shape::segments(tile, tr);
                        c.fill_path_fade(
                            &tile_segs,
                            Color::white(1.0),
                            tile.y,
                            tile.y + tile.h * 0.55,
                            0.34,
                            0.0,
                        );
                        c.fill_path_fade(
                            &tile_segs,
                            Color::black(1.0),
                            tile.y + tile.h * 0.52,
                            tile.bottom(),
                            0.0,
                            0.26,
                        );
                        let tile_rim = if is_hover {
                            Color::white(0.52)
                        } else {
                            Color::white(0.28)
                        };
                        c.stroke_rrect(tile, tr, tile_rim, 1.0);

                        let icon_box = tile.inset(tile.w * 0.22, tile.h * 0.22);
                        c.icon(slot.icon, icon_box.offset(0.0, 1.0), Color::black(0.28));
                        c.icon(slot.icon, icon_box, Color::white(0.97));
                    }

                    match slot.action {
                        SlotAction::Page(PageKind::Tasks) if tray.tasks_open > 0 => {
                            draw_dock_badge(c, tile, tray.tasks_open, DockBadgeStyle::Red, badge_mode);
                        }
                        SlotAction::Page(PageKind::Shelf) if tray.shelf_count > 0 => {
                            draw_dock_badge(c, tile, tray.shelf_count, DockBadgeStyle::Blue, badge_mode);
                        }
                        _ => {}
                    }

                    if slot.active {
                        draw_active_dot(c, Point::new(sr.x + sr.w * 0.5, dot_y), 2.1, dot_ink, 0.90);
                    }
                }

                if let Some(sx) = sep_x {
                    draw_glass_divider(c, sx, shelf, light);
                }

                let dl_frac = tray.download_progress.map(|p| (p as f32 / 100.0).clamp(0.0, 1.0));
                let mut browser_ring_drawn = false;
                let dragged_j = drag_pos.and_then(|_| {
                    pressed
                        .filter(|&i| i >= ANCHOR_SLOTS.len() && i < ANCHOR_SLOTS.len() + apps.len())
                        .map(|i| i - ANCHOR_SLOTS.len())
                });

                // Render stationary/shifting apps first, then the dragged app last so it floats on top.
                let order = (0..apps.len())
                    .filter(|&j| Some(j) != dragged_j)
                    .chain(dragged_j);

                for j in order {
                    let app = &apps[j];
                    let idx = ANCHOR_SLOTS.len() + j;
                    let Some(&base_sr) = slot_rects.get(idx) else { continue };
                    let is_hover = hovered == Some(idx);
                    let is_press = pressed == Some(idx);
                    let is_dragged = Some(j) == dragged_j;
                    let sr = if let Some(dp) = drag_pos.filter(|_| is_dragged) {
                        dragged_slot_rect(base_sr, dp, drag_out)
                    } else {
                        base_sr
                    };
                    // Draw subtle divider between pinned and unpinned/recent apps when both exist.
                    if j > 0 && !app.pinned && apps[j - 1].pinned && !is_dragged {
                        if let Some(&prev_sr) = slot_rects.get(idx - 1) {
                            draw_glass_divider(c, (prev_sr.right() + base_sr.x) * 0.5, shelf, light);
                        }
                    }
                    let tile = if is_press && !is_dragged { sr.inset(1.5, 1.5) } else { sr };
                    let tr = (tile.w * 0.25).clamp(8.0, 15.0);
                    let item_alpha = if is_dragged && drag_out { 0.50 } else { 1.0 };
                    c.with_alpha(item_alpha, |c| {
                        let ring_p = app.progress.or_else(|| {
                            if !browser_ring_drawn && is_browser_key(&app.key) {
                                dl_frac
                            } else {
                                None
                            }
                        });
                        if let Some(p) = ring_p {
                            if app.progress.is_none() {
                                browser_ring_drawn = true;
                            }
                            draw_progress_ring(c, tile, p);
                        }

                        if let Some(img) = app.icon.as_ref() {
                            // Crisp full-bleed app icon with soft drop-shadow (.dock-icon-img in mockup)
                            draw_squircle_shadow(c, tile.inset(2.0, 2.0), tr, light);
                            c.image_hq(img, tile.inset(0.5, 0.5));
                            if is_hover {
                                c.stroke_rrect(tile, tr, theme_accent.with_alpha(0.42), 1.0);
                            }
                        } else {
                            draw_squircle_shadow(c, tile, tr, light);
                            let back = if is_hover {
                                if light { Color::black(0.10) } else { Color::white(0.18) }
                            } else if app.focused {
                                if light { Color::black(0.07) } else { Color::white(0.12) }
                            } else if light {
                                Color::black(0.08)
                            } else {
                                Color::white(0.12)
                            };
                            c.fill_rrect(tile, tr, back);
                            let tile_segs = crate::pill::shape::segments(tile, tr);
                            c.fill_path_fade(
                                &tile_segs,
                                Color::white(1.0),
                                tile.y,
                                tile.y + tile.h * 0.45,
                                if light { 0.20 } else { 0.12 },
                                0.0,
                            );
                            let icon_box = tile.inset(tile.w * 0.24, tile.h * 0.24);
                            c.icon(Icon::AppWindow, icon_box, theme_ink);
                            let r_c = if is_hover {
                                theme_accent.with_alpha(0.64)
                            } else if app.focused {
                                if light { Color::black(0.15) } else { Color::white(0.26) }
                            } else if light {
                                Color::black(0.08)
                            } else {
                                Color::white(0.13)
                            };
                            c.stroke_rrect(tile, tr, r_c, 1.0);
                        }

                        if app.badge > 0 {
                            draw_dock_badge(c, tile, app.badge, DockBadgeStyle::Red, badge_mode);
                        }

                        // Active running-app indicator dot (only when running; pinned idle apps have no dot).
                        if app.running {
                            let dot_r = if app.focused { 2.3 } else if app.minimized { 1.6 } else { 1.9 };
                            let dot_a = if app.focused { 0.96 } else if app.minimized { 0.55 } else { 0.78 };
                            draw_active_dot(c, Point::new(sr.x + sr.w * 0.5, dot_y), dot_r, dot_ink, dot_a);
                        }
                    });
                }

                if let Some(sx) = sep2_x {
                    draw_glass_divider(c, sx, shelf, light);
                }

                let dl_idx = show_downloads.then_some(ANCHOR_SLOTS.len() + apps.len());
                let trash_idx = ANCHOR_SLOTS.len() + apps.len() + usize::from(show_downloads);
                if let Some(dl_idx) = dl_idx {
                    if let Some(&sr) = slot_rects.get(dl_idx) {
                        let is_hover = hovered == Some(dl_idx);
                        let is_press = pressed == Some(dl_idx);
                        let tile = if is_press { sr.inset(1.5, 1.5) } else { sr };
                        let tr = (tile.w * 0.25).clamp(8.0, 15.0);
                        let accent = look.accent.unwrap_or(Color::hex(0x007AFF));
                        if let Some(p) = dl_frac {
                            draw_progress_ring(c, tile, p);
                        }
                        draw_squircle_shadow(c, tile, tr, light);
                        draw_downloads_stack_icon(c, tile, tr, accent, light, is_hover, stack_open);
                        if tray.downloads_active > 0 {
                            draw_dock_badge(c, tile, tray.downloads_active, DockBadgeStyle::Blue, badge_mode);
                        }
                        if stack_open {
                            draw_active_dot(c, Point::new(sr.x + sr.w * 0.5, dot_y), 2.1, accent, 0.95);
                        }
                    }
                }
                if let Some(&sr) = slot_rects.get(trash_idx) {
                    let is_hover = hovered == Some(trash_idx);
                    let is_press = pressed == Some(trash_idx);
                    let tile = if is_press { sr.inset(1.5, 1.5) } else { sr };
                    let tr = (tile.w * 0.25).clamp(8.0, 15.0);
                    draw_squircle_shadow(c, tile, tr, light);
                    draw_trash_slot_icon(c, tile, tr, light, is_hover);
                }

                if let Some(sx) = sep3_x {
                    draw_glass_divider(c, sx, shelf, light);
                }

                if let Some(tr) = tray_rect {
                    // Outer recessed glass well (#dockSysTray: rgba(8, 12, 20, 0.34), border rgba(255,255,255,0.11)).
                    let well = tr.inset(-3.0, -3.0);
                    let well_r = (well.h * 0.38).min(16.0);
                    let well_bg = if light {
                        Color::black(0.055)
                    } else {
                        Color::rgba(0.03, 0.05, 0.08, 0.36)
                    };
                    let well_rim = if light { Color::black(0.08) } else { Color::white(0.12) };
                    c.fill_rrect(well, well_r, well_bg);
                    c.stroke_rrect(well, well_r, well_rim, 1.0);

                    let (weather_r, full_status_r, clock_r) = tray_zone_rects(tr, tray);
                    let (chev_r, status_r) = tray_chevron_and_status_rects(full_status_r);
                    let ink = theme_ink;
                    let dim = theme_ink.with_alpha(if light { 0.56 } else { 0.64 });

                    let zones = [
                        weather_r.map(|r| (DockTrayZone::Weather, r)),
                        Some((DockTrayZone::Chevron, chev_r)),
                        Some((DockTrayZone::Status, status_r)),
                        Some((DockTrayZone::Clock, clock_r)),
                    ];
                    for (zone, zr) in zones.into_iter().flatten() {
                        let is_h = hovered_tray == Some(zone);
                        let is_p = pressed_tray == Some(zone);
                        let is_act = zone == DockTrayZone::Status && ctrl_open;
                        let bg = if is_p || is_act {
                            if light { Color::black(0.13) } else { Color::white(0.19) }
                        } else if is_h {
                            if light { Color::black(0.08) } else { Color::white(0.13) }
                        } else if light {
                            Color::white(0.42)
                        } else {
                            Color::white(0.06)
                        };
                        let rim = if is_act {
                            theme_accent.with_alpha(0.62)
                        } else if is_h {
                            theme_accent.with_alpha(0.42)
                        } else if light {
                            Color::black(0.07)
                        } else {
                            Color::white(0.10)
                        };
                        let pill_r = (zr.h * 0.36).min(12.0);
                        c.fill_rrect(zr, pill_r, bg);
                        c.stroke_rrect(zr, pill_r, rim, 1.0);
                    }

                    // Weather pill (temperature + short condition).
                    if let (Some(wr), Some(temp_c)) = (weather_r, tray.weather_temp_c) {
                        let sun_r = 4.2;
                        let sun_c = Point::new(wr.x + 11.0, wr.y + wr.h * 0.5 - 1.0);
                        c.fill_circle(sun_c, sun_r, Color::hex(0xFBBF24));
                        let temp_st = TextStyle { size: 10.5, ..TextStyle::caption().weight(Weight::Semibold).color(ink) };
                        let cond_st = TextStyle { size: 8.5, ..TextStyle::caption().color(dim) };
                        let tx = wr.x + 20.0;
                        let tw = (wr.right() - tx - 4.0).max(16.0);
                        c.text(&format!("{temp_c}°C"), Rect::new(tx, wr.y + 3.0, tw, 13.0), &temp_st);
                        c.text(tray.weather_short(), Rect::new(tx, wr.y + 15.0, tw, 11.0), &cond_st);
                    }

                    // System Tray ^ Chevron button (Background Apps host flyout).
                    {
                        let cx = chev_r.x + chev_r.w * 0.5;
                        let cy = chev_r.y + chev_r.h * 0.5;
                        let chev_col = if hovered_tray == Some(DockTrayZone::Chevron) { ink } else { dim };
                        let segs = [
                            Seg::MoveTo(cx - 4.2, cy + 1.8),
                            Seg::LineTo(cx, cy - 2.4),
                            Seg::LineTo(cx + 4.2, cy + 1.8),
                        ];
                        c.path(&segs, None, Some((chev_col, 1.6)));
                    }

                    // Status cluster: Wi-Fi (+ green dot), Volume + %, and Battery capsule + %.
                    let mut ix = status_r.x + 8.0;
                    let icon_sz = 12.5;
                    let iy = status_r.y + (status_r.h - icon_sz) * 0.5;
                    let (wifi_ic, wifi_c) = match tray.wifi_connected {
                        Some(false) => (Icon::WifiOff, dim),
                        _ => (Icon::Wifi, ink),
                    };
                    c.icon(wifi_ic, Rect::new(ix, iy, icon_sz, icon_sz), wifi_c);
                    if tray.wifi_connected == Some(true) {
                        c.fill_circle(Point::new(ix + icon_sz - 1.5, iy + icon_sz - 1.5), 2.3, Color::hex(0x4ADE80));
                    }
                    ix += icon_sz + 7.0;

                    let (vol_ic, vol_c) = if tray.muted || (tray.volume_known && tray.volume_pct == 0) {
                        (Icon::VolumeMute, dim)
                    } else if tray.volume_known && tray.volume_pct < 45 {
                        (Icon::VolumeLow, ink)
                    } else {
                        (Icon::Volume, ink)
                    };
                    c.icon(vol_ic, Rect::new(ix, iy, icon_sz, icon_sz), vol_c);
                    ix += icon_sz + 3.0;
                    if tray.volume_known && ix + 24.0 < status_r.right() {
                        let vol_st = TextStyle { size: 9.5, ..TextStyle::caption().weight(Weight::Medium).color(dim) };
                        c.text(&format!("{}%", tray.volume_pct), Rect::new(ix, status_r.y, 24.0, status_r.h), &vol_st);
                        ix += 24.0;
                    } else {
                        ix += 3.0;
                    }

                    if tray.has_battery && ix + 24.0 < status_r.right() {
                        let bat_c = if tray.charging || tray.on_ac {
                            Color::hex(0x30D158)
                        } else if tray.battery_pct <= 20 {
                            Color::hex(0xFF453A)
                        } else {
                            Color::hex(0x4ADE80)
                        };
                        let bat_w = 15.0;
                        let bat_h = 8.5;
                        let by = status_r.y + (status_r.h - bat_h) * 0.5;
                        let bat_body = Rect::new(ix, by, bat_w, bat_h);
                        c.stroke_rrect(bat_body, 2.2, dim, 1.0);
                        let fill_w = ((bat_w - 3.0) * (tray.battery_pct.min(100) as f32 / 100.0)).max(1.5);
                        c.fill_rrect(Rect::new(ix + 1.5, by + 1.5, fill_w, bat_h - 3.0), 1.2, bat_c);
                        c.fill_rrect(Rect::new(ix + bat_w + 0.5, by + 2.5, 1.5, 3.5), 0.7, dim);
                        ix += bat_w + 5.0;
                        let pct_w = (status_r.right() - ix - 3.0).max(0.0);
                        if pct_w >= 18.0 {
                            let pct_st = TextStyle { size: 9.5, ..TextStyle::caption().weight(Weight::Medium).color(ink) };
                            c.text(&format!("{}%", tray.battery_pct), Rect::new(ix, status_r.y, pct_w, status_r.h), &pct_st);
                        }
                    }

                    // Clock & Date stack zone (+ amber Focus Mode indicator dot .tcl-focus-dot.on).
                    if tray.focus_active {
                        let dot_c = Point::new(clock_r.x + 8.5, clock_r.y + 9.5);
                        c.fill_circle(dot_c, 4.8, Color::hex(0xF5A524).with_alpha(0.26));
                        c.fill_circle(dot_c, 2.7, Color::hex(0xF5A524));
                    }
                    if let Some(date_s) = tray.date_text() {
                        let time_st = TextStyle { size: 11.0, ..TextStyle::caption().weight(Weight::Semibold).center().color(ink) };
                        let date_st = TextStyle { size: 8.5, ..TextStyle::caption().center().color(dim) };
                        let time_x = if tray.focus_active { clock_r.x + 6.0 } else { clock_r.x };
                        let time_w = (clock_r.right() - time_x).max(16.0);
                        c.text(&tray.clock_text(), Rect::new(time_x, clock_r.y + 2.5, time_w, 14.0), &time_st);
                        c.text(&date_s, Rect::new(clock_r.x, clock_r.y + 15.0, clock_r.w, 12.0), &date_st);
                    } else {
                        let clock_st = TextStyle::caption().weight(Weight::Semibold).center().color(ink);
                        c.text(&tray.clock_text(), clock_r, &clock_st);
                    }
                }

                let tip: Option<(&str, Rect)> = if let Some(i) = hovered {
                    let label = if i < ANCHOR_SLOTS.len() {
                        Some(ANCHOR_SLOTS[i].label)
                    } else if Some(i) == dl_idx {
                        Some("Downloads")
                    } else if i == trash_idx {
                        Some("Trash")
                    } else {
                        apps.get(i - ANCHOR_SLOTS.len()).and_then(|a| {
                            if preview_open && a.running {
                                None
                            } else {
                                Some(a.name.as_str())
                            }
                        })
                    };
                    label.zip(slot_rects.get(i).copied())
                } else if let (Some(i), Some(dp)) = (pressed, drag_pos) {
                    let label = if drag_out {
                        Some("Remove from Dock")
                    } else {
                        apps.get(i.saturating_sub(ANCHOR_SLOTS.len())).map(|a| a.name.as_str())
                    };
                    label.zip(slot_rects.get(i).map(|&sr| dragged_slot_rect(sr, dp, drag_out)))
                } else if let (Some(z), Some(tr)) = (hovered_tray, tray_rect) {
                    let (weather_r, full_status_r, clock_r) = tray_zone_rects(tr, tray);
                    let (chev_r, status_r) = tray_chevron_and_status_rects(full_status_r);
                    match z {
                        DockTrayZone::Weather => weather_r.map(|r| ("Weather", r)),
                        DockTrayZone::Chevron => Some(("Background Apps", chev_r)),
                        DockTrayZone::Status => Some(("System Status", status_r)),
                        DockTrayZone::Clock => Some(("Today", clock_r)),
                    }
                } else {
                    None
                };
                if let Some((label, sr)) = tip {
                    let st = TextStyle::caption()
                        .weight(Weight::Medium)
                        .center()
                        .color(theme_ink);
                    let (tw, _) = c.measure(label, &st, 220.0);
                    let pw = (tw + 20.0).max(54.0);
                    let ph = 25.0;
                    let px = (sr.x + sr.w * 0.5 - pw * 0.5).clamp(4.0, (surf_w - pw - 4.0).max(4.0));
                    let py = (sr.y.min(shelf.y) - ph - 10.0).max(2.0);
                    let pill = Rect::new(px, py, pw, ph);
                    let tip_r = 9.0f32.min(ph * 0.5);
                    let tip_bg = if look.ground == look::HALO.ground || look.frame.is_some() {
                        look.ground.with_alpha(0.95)
                    } else if light {
                        Color::rgba(0.96, 0.97, 0.99, 0.92)
                    } else {
                        Color::rgba(0.08, 0.10, 0.15, 0.90)
                    };
                    let tip_rim = if light { Color::black(0.12) } else { Color::white(0.20) };
                    c.rrect_shadow(pill.offset(0.0, 2.0), tip_r, Color::black(0.9), TIP_SHADOW);
                    c.fill_rrect(pill, tip_r, tip_bg);
                    let tip_segs = crate::pill::shape::segments(pill, tip_r);
                    c.fill_path_fade(&tip_segs, Color::white(1.0), pill.y, pill.y + pill.h * 0.55, if light { 0.30 } else { 0.16 }, 0.0);
                    c.stroke_rrect(pill, tip_r, tip_rim, 1.0);
                    // Downward pointer caret (.dock-tip::after)
                    let cx = (sr.x + sr.w * 0.5).clamp(pill.x + tip_r + 4.0, pill.right() - tip_r - 4.0);
                    let cy = pill.bottom();
                    let caret = [
                        Seg::MoveTo(cx - 4.5, cy - 0.5),
                        Seg::LineTo(cx, cy + 4.2),
                        Seg::LineTo(cx + 4.5, cy - 0.5),
                        Seg::Close,
                    ];
                    c.path(&caret, Some(tip_bg), Some((tip_rim, 1.0)));
                    c.text(label, pill, &st);
                }
                if !poofs.is_empty() {
                    draw_poof_effects(c, poofs, light);
                }
            });
        }

        if tuck_t > 0.05 {
            let alpha = ((tuck_t - 0.05) / 0.95).clamp(0.0, 1.0);
            c.with_alpha(alpha, |c| {
                let pr = peek_rect(surf_w);
                let fill = theme_ink.with_alpha(if light { 0.42 } else { 0.48 });
                c.fill_rrect(pr, pr.h * 0.5, fill);
            });
        }
    });
}

/// Show the dock's right-click context menu without permanently stealing focus.
pub fn show_menu(owner: HWND, pt: POINT, tucked: bool, target: &DockMenuTarget) -> Option<DockCmd> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let mut cmds: Vec<(usize, DockCmd)> = Vec::new();
        let mut next_id = 1usize;
        let mut push_item = |label: &str, cmd: DockCmd| {
            let id = next_id;
            next_id += 1;
            let w: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
            let _ = AppendMenuW(menu, MF_STRING, id, PCWSTR(w.as_ptr()));
            cmds.push((id, cmd));
        };

        match target {
            DockMenuTarget::App(app) => {
                if app.running {
                    for &(h, ref title) in app.windows.iter().take(12) {
                        let t: String = if title.chars().count() > 48 {
                            title.chars().take(47).chain(Some('…')).collect()
                        } else {
                            title.clone()
                        };
                        push_item(&t, DockCmd::ActivateWindow(h));
                    }
                    if !app.windows.is_empty() {
                        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                    }
                }
                if let Some(spec) = app.launch.clone().or_else(|| app.exe.clone()) {
                    push_item(if app.running { "New Window" } else { "Open" }, DockCmd::LaunchApp(spec.clone()));
                    if app.pinned {
                        push_item("Remove from Dock", DockCmd::RemoveFromDock(spec));
                    } else {
                        push_item("Keep in Dock", DockCmd::KeepInDock(spec));
                    }
                }
                if let Some(exe) = app.exe.as_ref().filter(|e| std::path::Path::new(e).exists()) {
                    push_item("Show in Explorer", DockCmd::ShowInExplorer(exe.clone()));
                }
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                push_item("Dock Settings…", DockCmd::OpenSettings);
                if app.running {
                    let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                    push_item("Quit", DockCmd::QuitApp(app.windows.clone(), app.hwnd));
                }
            }
            DockMenuTarget::Trash => {
                push_item("Open Recycle Bin", DockCmd::OpenRecycleBin);
                push_item("Empty Recycle Bin", DockCmd::EmptyRecycleBin);
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                push_item("Dock Settings…", DockCmd::OpenSettings);
            }
            DockMenuTarget::Shelf => {
                push_item(if tucked { "Show shelf" } else { "Hide shelf" }, DockCmd::ToggleTuck);
                push_item("Dock Settings…", DockCmd::OpenSettings);
                let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
                push_item("Turn off Bottom Dock", DockCmd::DisableDock);
            }
        }

        let prev = GetForegroundWindow();
        let _ = SetForegroundWindow(owner);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, owner, None);
        let _ = PostMessageW(owner, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        let cmd = cmds.into_iter().find_map(|(id, c)| (id == chosen.0 as usize).then_some(c));
        if cmd != Some(DockCmd::OpenSettings)
            && !prev.is_invalid()
            && prev != owner
            && IsWindow(prev).as_bool()
            && GetForegroundWindow() == owner
        {
            let _ = SetForegroundWindow(prev);
        }
        cmd
    }
}

/// Recent entry inside the Downloads stack popover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DownloadItem {
    pub name: String,
    pub path: std::path::PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StackAction {
    None,
    RequestFrame,
    OpenPath(String),
    Close,
}

const STACK_W: f32 = 326.0;
const STACK_PAD: f32 = 12.0;
const STACK_HDR_H: f32 = 38.0;
const STACK_ROW_H: f32 = 46.0;
const STACK_R: f32 = 18.0;
const STACK_MAX_ITEMS: usize = 8;

/// Compute the fan-out horizontal curve indent for item `i` of `count` in the Downloads stack,
/// creating a subtle macOS fan cascade where upper cards arc outward from the bottom anchor.
pub fn stack_fan_indent(index: usize, count: usize) -> f32 {
    if count <= 1 {
        return 0.0;
    }
    // Distance from bottom card (0 = bottom card closest to dock, 1 = top card)
    let from_bottom = (count.saturating_sub(1 + index)) as f32 / (count - 1) as f32;
    (from_bottom * from_bottom * 8.0).round()
}

/// Compute the elevated card rectangle for item `i` of `count` inside `card_dip`.
pub fn stack_fan_card_rect(card: Rect, index: usize, count: usize) -> Rect {
    let indent = stack_fan_indent(index, count);
    let y = STACK_PAD + STACK_HDR_H + 6.0 + index as f32 * STACK_ROW_H;
    Rect::new(
        STACK_PAD + indent,
        y,
        (card.w - 2.0 * STACK_PAD - indent * 1.4).max(80.0),
        STACK_ROW_H - 5.0,
    )
}

/// Resolve the file-type badge accent color and short kind tag for a download item.
pub fn download_file_badge(item: &DownloadItem) -> (Color, &'static str) {
    if item.is_dir {
        return (Color::hex(0x0284C7), "FOLDER");
    }
    let ext = item
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "exe" | "msi" | "dmg" | "pkg" => (Color::hex(0x0A84FF), "APP"),
        "pdf" | "doc" | "docx" | "txt" | "md" => (Color::hex(0xEF4444), "DOC"),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" => (Color::hex(0x10B981), "IMG"),
        "mp3" | "wav" | "flac" | "ogg" | "m4a" | "mp4" | "mkv" | "mov" => (Color::hex(0x8B5CF6), "MEDIA"),
        "zip" | "7z" | "rar" | "tar" | "gz" => (Color::hex(0xF5A524), "ZIP"),
        _ => (Color::hex(0x64748B), "FILE"),
    }
}

/// Lazily scan `dir` for the newest `limit` non-hidden files/folders.
/// Called only when the user opens the Downloads stack (0 idle threads/watchers).
pub fn scan_downloads_in(dir: &std::path::Path, limit: usize) -> Vec<DownloadItem> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut rows: Vec<(std::time::SystemTime, DownloadItem)> = Vec::new();
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.is_empty() || name.starts_with('.') || name.eq_ignore_ascii_case("desktop.ini") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let modified = meta.modified().unwrap_or(std::time::UNIX_EPOCH);
        let is_dir = meta.is_dir();
        let size = if is_dir { 0 } else { meta.len() };
        rows.push((modified, DownloadItem { name, path: entry.path(), is_dir, size }));
    }
    rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
    rows.into_iter().take(limit).map(|(_, item)| item).collect()
}

pub fn format_file_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    let b = bytes as f64;
    if b >= GB {
        format!("{:.1} GB", b / GB)
    } else if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}

/// Physical pixel bounds `(x, y, w, h)` for the Downloads stack card anchored above `anchor_screen`.
pub fn stack_surface_rect(mon: &MonitorInfo, anchor_screen: Rect, item_count: usize) -> (i32, i32, i32, i32) {
    let scale = mon.scale().max(1.0);
    let rows = item_count.clamp(1, STACK_MAX_ITEMS) as f32;
    let dip_h = STACK_PAD * 2.0 + STACK_HDR_H + rows * STACK_ROW_H + 6.0;
    let w = ((STACK_W * scale).ceil() as i32).max(120);
    let h = ((dip_h * scale).ceil() as i32).max(80);
    let gap = (10.0 * scale).round() as i32;
    let cx = (anchor_screen.x + anchor_screen.w * 0.5).round() as i32;
    let min_x = mon.work.left + (8.0 * scale) as i32;
    let max_x = (mon.work.right - w - (8.0 * scale) as i32).max(min_x);
    let x = (cx - w / 2).clamp(min_x, max_x);
    let y = (anchor_screen.y.round() as i32 - h - gap).max(mon.work.top + (8.0 * scale) as i32);
    (x, y, w, h)
}

/// Transient Downloads stack popover (`Halo.DockStack`): exists only while open.
pub struct DockStack {
    gfx: Option<Gfx>,
    pub hwnd: HWND,
    at: (i32, i32, i32, i32),
    dpi: u32,
    light: bool,
    look: &'static Look,
    dir: std::path::PathBuf,
    items: Vec<DownloadItem>,
    hovered: Option<usize>, // usize::MAX = "Open Folder" header button
    tracking: bool,
    shown: bool,
    dirty: bool,
}

impl Drop for DockStack {
    fn drop(&mut self) {
        let visual = self.gfx.as_ref().and_then(|g| g.visual_hwnd());
        self.gfx = None;
        unsafe {
            if let Some(v) = visual {
                let _ = DestroyWindow(v);
            }
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

impl DockStack {
    pub fn open(mon: &MonitorInfo, anchor_screen: Rect, look: &'static Look, shared: Option<SharedD3d>) -> Result<Self> {
        let dir = crate::paths::media_downloads_dir();
        let items = scan_downloads_in(&dir, STACK_MAX_ITEMS);
        let (x, y, w, h) = stack_surface_rect(mon, anchor_screen, items.len());
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                STACK_CLASS,
                w!("Halo Downloads Stack"),
                WS_POPUP,
                x,
                y,
                w,
                h,
                None,
                None,
                GetModuleHandleW(None)?,
                None,
            )?
        };
        let mut gfx = match Gfx::new(mon.dpi as f32, hwnd) {
            Ok(g) => g.secondary(),
            Err(e) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(e);
            }
        };
        gfx.set_shared_d3d(shared);
        let _ = gfx.set_surface(w, h);
        Ok(Self {
            gfx: Some(gfx),
            hwnd,
            at: (x, y, w, h),
            dpi: mon.dpi,
            light: look.light,
            look,
            dir,
            items,
            hovered: None,
            tracking: false,
            shown: false,
            dirty: true,
        })
    }

    pub fn set_look(&mut self, look: &'static Look) -> bool {
        if self.look == look && self.light == look.light {
            return false;
        }
        self.look = look;
        self.light = look.light;
        self.dirty = true;
        true
    }

    pub fn needs_frame(&self) -> bool {
        self.dirty
    }

    fn scale(&self) -> f32 {
        (self.dpi as f32 / 96.0).max(1.0)
    }

    fn card_dip(&self) -> Rect {
        let s = self.scale();
        Rect::new(0.0, 0.0, self.at.2 as f32 / s, self.at.3 as f32 / s)
    }

    fn open_btn_rect(&self) -> Rect {
        let card = self.card_dip();
        Rect::new(card.w - STACK_PAD - 58.0, STACK_PAD + 4.0, 58.0, 24.0)
    }

    fn row_rect(&self, i: usize) -> Rect {
        stack_fan_card_rect(self.card_dip(), i, self.items.len().max(1))
    }

    fn hit_test(&self, p: Point) -> Option<usize> {
        if self.open_btn_rect().contains(p) {
            return Some(usize::MAX);
        }
        for i in 0..self.items.len() {
            if self.row_rect(i).contains(p) {
                return Some(i);
            }
        }
        None
    }

    pub fn handle_message(&mut self, hwnd: HWND, msg: u32, _wp: WPARAM, lp: LPARAM) -> (Option<LRESULT>, StackAction) {
        if hwnd != self.hwnd {
            return (None, StackAction::None);
        }
        let to_dip = |lp: LPARAM, scale: f32| {
            let x = (lp.0 & 0xffff) as u16 as i16 as f32;
            let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as f32;
            Point::new(x / scale, y / scale)
        };
        match msg {
            WM_MOUSEACTIVATE => (Some(LRESULT(MA_NOACTIVATE as isize)), StackAction::None),
            WM_MOUSEMOVE => {
                if !self.tracking {
                    self.tracking = true;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: self.hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                }
                let next = self.hit_test(to_dip(lp, self.scale()));
                if next != self.hovered {
                    self.hovered = next;
                    self.dirty = true;
                    return (Some(LRESULT(0)), StackAction::RequestFrame);
                }
                (Some(LRESULT(0)), StackAction::None)
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                if self.hovered.take().is_some() {
                    self.dirty = true;
                    return (Some(LRESULT(0)), StackAction::RequestFrame);
                }
                (Some(LRESULT(0)), StackAction::None)
            }
            WM_LBUTTONUP => {
                let hit = self.hit_test(to_dip(lp, self.scale()));
                match hit {
                    Some(usize::MAX) => {
                        return (Some(LRESULT(0)), StackAction::OpenPath(self.dir.to_string_lossy().into_owned()));
                    }
                    Some(i) => {
                        if let Some(item) = self.items.get(i) {
                            return (Some(LRESULT(0)), StackAction::OpenPath(item.path.to_string_lossy().into_owned()));
                        }
                    }
                    None => {}
                }
                (Some(LRESULT(0)), StackAction::None)
            }
            WM_RBUTTONUP => (Some(LRESULT(0)), StackAction::Close),
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                unsafe {
                    let id = if self.hovered.is_some() { IDC_HAND } else { IDC_ARROW };
                    if let Ok(c) = LoadCursorW(None, id) {
                        SetCursor(c);
                    }
                }
                (Some(LRESULT(1)), StackAction::None)
            }
            _ => (None, StackAction::None),
        }
    }

    pub fn paint(&mut self, shared: Option<SharedD3d>) {
        if !self.dirty {
            return;
        }
        let (x, y, w, h) = self.at;
        let scale = self.scale();
        let card = self.card_dip();
        let open_btn = self.open_btn_rect();
        let look = self.look;
        let light = look.light;
        let hovered = self.hovered;
        let Some(gfx) = self.gfx.as_mut() else { return };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if gfx.set_surface(w, h).is_err() {
            return;
        }
        gfx.set_look(look);
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        let body = card.inset(1.0, 1.0);
        let glass_params = if look.material == look::Material::Glass {
            let gp = dock_glass_params(body, STACK_R, look, scale);
            gfx.prepare_glass(&gp, full);
            Some(gp)
        } else {
            None
        };
        if let Ok(mut c) = gfx.begin(full) {
            c.unthemed(|c| {
                let ink = look.ink;
                let accent = look.accent.unwrap_or(Color::hex(0x0A84FF));
                let sub = ink.with_alpha(0.56);
                let rim = if light { Color::black(0.14) } else { Color::white(0.20) };
                if let Some(gp) = glass_params.as_ref() {
                    c.glass(gp, card);
                } else {
                    crate::pill::shape::draw_body(c, body, STACK_R, look.ground, Some(look.border));
                }

                // Header (.stk-head: icon + "Downloads Stack" + count subtitle + "Open" button)
                let sky = Color::hex(0x38BDF8);
                c.icon(Icon::Download, Rect::new(STACK_PAD + 2.0, STACK_PAD + 6.0, 15.0, 15.0), sky);
                let hdr_st = TextStyle::body().weight(Weight::Semibold).color(ink);
                c.text("Downloads Stack", Rect::new(STACK_PAD + 23.0, STACK_PAD + 3.0, 136.0, 22.0), &hdr_st);
                let sub_label = format!("{} recent", self.items.len());
                let sub_st = TextStyle::caption().color(sub);
                c.text(&sub_label, Rect::new(STACK_PAD + 23.0, STACK_PAD + 20.0, 110.0, 14.0), &sub_st);

                let btn_bg = if hovered == Some(usize::MAX) {
                    accent.with_alpha(0.28)
                } else if light {
                    Color::black(0.06)
                } else {
                    Color::white(0.10)
                };
                c.fill_rrect(open_btn, 8.0, btn_bg);
                c.stroke_rrect(open_btn, 8.0, if hovered == Some(usize::MAX) { accent.with_alpha(0.55) } else { rim }, 1.0);
                let btn_st = TextStyle::caption().weight(Weight::Medium).center().color(ink);
                c.text("Open", open_btn, &btn_st);

                let div_y = STACK_PAD + STACK_HDR_H;
                c.fill_rrect(Rect::new(STACK_PAD, div_y, card.w - 2.0 * STACK_PAD, 1.0), 0.5, rim.with_alpha(0.10));

                if self.items.is_empty() {
                    let empty_st = TextStyle::caption().center().color(sub);
                    c.text("No recent downloads", Rect::new(STACK_PAD, div_y + 10.0, card.w - 2.0 * STACK_PAD, 28.0), &empty_st);
                } else {
                    let count = self.items.len();
                    let name_st = TextStyle::caption().weight(Weight::Semibold).color(ink);
                    let info_st = TextStyle::new(10.5).color(sub);
                    for (i, item) in self.items.iter().enumerate() {
                        let rr = stack_fan_card_rect(card, i, count);
                        let is_h = hovered == Some(i);
                        let card_r = 12.0;

                        // Fan-out card background & rim (.stk-item / .stk-item.hov)
                        let card_bg = if is_h {
                            accent.with_alpha(if light { 0.18 } else { 0.24 })
                        } else if light {
                            Color::black(0.04)
                        } else {
                            Color::white(0.05)
                        };
                        let card_rim = if is_h {
                            accent.with_alpha(0.52)
                        } else if light {
                            Color::black(0.08)
                        } else {
                            Color::white(0.08)
                        };
                        c.fill_rrect(rr, card_r, card_bg);
                        let r_segs = crate::pill::shape::segments(rr, card_r);
                        c.fill_path_fade(&r_segs, Color::white(1.0), rr.y, rr.y + rr.h * 0.5, if light { 0.22 } else { 0.10 }, 0.0);
                        c.stroke_rrect(rr, card_r, card_rim, 1.0);

                        // Left squircle file-type icon badge (.stk-ico)
                        let (badge_c, kind_tag) = download_file_badge(item);
                        let ico_box = Rect::new(rr.x + 7.0, rr.y + (rr.h - 30.0) * 0.5, 30.0, 30.0);
                        c.fill_rrect(ico_box, 8.0, badge_c.with_alpha(if light { 0.18 } else { 0.24 }));
                        c.stroke_rrect(ico_box, 8.0, badge_c.with_alpha(0.45), 1.0);
                        let ic = if item.is_dir { Icon::Folder } else { Icon::File };
                        c.icon(ic, ico_box.inset(7.0, 7.0), badge_c);

                        // Right "NEW" tag on newest item (.stk-tag)
                        let right_pad = if i == 0 {
                            let tag_r = Rect::new(rr.right() - 42.0, rr.y + (rr.h - 17.0) * 0.5, 34.0, 17.0);
                            c.fill_rrect(tag_r, 5.5, Color::hex(0x2ED573).with_alpha(0.20));
                            let tag_st = TextStyle::new(9.0).weight(Weight::Bold).center().color(Color::hex(0x4ADE80));
                            c.text("NEW", tag_r, &tag_st);
                            48.0
                        } else {
                            10.0
                        };

                        // Two-line metadata (.stk-name + .stk-info)
                        let tx = ico_box.right() + 9.0;
                        let tw = (rr.right() - tx - right_pad).max(30.0);
                        c.text(&item.name, Rect::new(tx, rr.y + 5.0, tw, 16.0), &name_st);
                        let info = if item.is_dir {
                            "Folder · Downloads".to_string()
                        } else {
                            format!("{} · {kind_tag}", format_file_size(item.size))
                        };
                        c.text(&info, Rect::new(tx, rr.y + 21.0, tw, 14.0), &info_st);
                    }
                }
            });
            let _ = gfx.end();
        }
        let _ = gfx.present(x, y);
        if gfx.needs_hit_mask() {
            let mask = HitMask { rect: None, notch: None, pill: Some((card, STACK_R)) };
            let _ = gfx.push_hit_mask(x, y, &mask, None);
        }
        if !self.shown {
            self.shown = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            gfx.set_shown(true);
        }
        self.dirty = false;
    }
}

// ---------------------------------------------------------------------------
// Launchpad Overlay Window (searchable paged AppsFolder grid)
// ---------------------------------------------------------------------------

const LAUNCHPAD_CLASS: PCWSTR = w!("Halo.DockLaunchpad");
const LP_W: f32 = 920.0;
const LP_H: f32 = 590.0;
const LP_R: f32 = 28.0;
const LP_PAD_X: f32 = 34.0;
pub const LP_COLS: usize = 7;
pub const LP_ROWS: usize = 5;
pub const LP_PER_PAGE: usize = LP_COLS * LP_ROWS;
const LP_SEARCH_W: f32 = 280.0;
const LP_SEARCH_H: f32 = 34.0;
const LP_SEARCH_Y: f32 = 22.0;
const LP_GRID_TOP: f32 = 74.0;
const LP_GRID_H: f32 = 460.0;
const LP_DOTS_Y: f32 = 556.0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LaunchpadAction {
    None,
    RequestFrame,
    LaunchApp(String),
    TogglePin(String),
    OpenSettings,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchpadZone {
    Search,
    SettingsBtn,
    Cell(usize),
    Dot(usize),
}

pub fn register_launchpad(
    wndproc: unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT,
) -> Result<()> {
    unsafe {
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: GetModuleHandleW(None)?.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: LAUNCHPAD_CLASS,
            ..Default::default()
        };
        let _ = RegisterClassExW(&wc);
        Ok(())
    }
}

pub fn launchpad_surface_rect(mon: &MonitorInfo) -> (i32, i32, i32, i32) {
    let scale = mon.scale();
    let w = (LP_W * scale).ceil() as i32;
    let h = (LP_H * scale).ceil() as i32;
    let mon_w = (mon.rect.right - mon.rect.left).max(w);
    let mon_h = (mon.rect.bottom - mon.rect.top).max(h);
    let x = mon.rect.left + (mon_w - w) / 2;
    let y = (mon.work.bottom - h - (84.0 * scale).round() as i32)
        .clamp(mon.rect.top + (32.0 * scale) as i32, mon.rect.top + (mon_h - h) / 2);
    (x, y, w, h)
}

pub fn launchpad_search_rect(card: Rect) -> Rect {
    Rect::new(card.x + (card.w - LP_SEARCH_W) * 0.5, card.y + LP_SEARCH_Y, LP_SEARCH_W, LP_SEARCH_H)
}

pub fn launchpad_settings_btn_rect(card: Rect) -> Rect {
    Rect::new(card.right() - LP_PAD_X - 32.0, card.y + LP_SEARCH_Y + 1.0, 32.0, 32.0)
}

pub fn launchpad_cell_rect(card: Rect, cell: usize) -> Rect {
    let gw = card.w - LP_PAD_X * 2.0;
    let cell_w = gw / LP_COLS as f32;
    let cell_h = LP_GRID_H / LP_ROWS as f32;
    let r = cell / LP_COLS;
    let c = cell % LP_COLS;
    Rect::new(
        card.x + LP_PAD_X + c as f32 * cell_w,
        card.y + LP_GRID_TOP + r as f32 * cell_h,
        cell_w,
        cell_h,
    )
}

pub fn launchpad_dot_rect(card: Rect, dot: usize, total_pages: usize) -> Rect {
    let gap = 18.0;
    let total_w = gap * total_pages.saturating_sub(1) as f32;
    let x0 = card.x + (card.w - total_w) * 0.5;
    let cx = x0 + dot as f32 * gap;
    let cy = card.y + LP_DOTS_Y;
    Rect::new(cx - 8.0, cy - 8.0, 16.0, 16.0)
}

pub struct DockLaunchpad {
    gfx: Option<Gfx>,
    hwnd: HWND,
    at: (i32, i32, i32, i32),
    dpi: u32,
    light: bool,
    look: &'static Look,
    apps: Vec<crate::services::dock_apps::LaunchpadApp>,
    pinned: Vec<String>,
    query: String,
    shown_indices: Vec<usize>,
    page: usize,
    sel: usize,
    hovered: Option<LaunchpadZone>,
    pressed: Option<LaunchpadZone>,
    tracking: bool,
    shown: bool,
    dirty: bool,
}

impl Drop for DockLaunchpad {
    fn drop(&mut self) {
        self.gfx.take();
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
}

impl DockLaunchpad {
    pub fn open(
        mon: &MonitorInfo,
        pinned: Vec<String>,
        look: &'static Look,
        shared: Option<SharedD3d>,
    ) -> Result<Self> {
        let (x, y, w, h) = launchpad_surface_rect(mon);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                LAUNCHPAD_CLASS,
                w!("Halo Launchpad"),
                WS_POPUP,
                x,
                y,
                w,
                h,
                None,
                None,
                GetModuleHandleW(None)?,
                None,
            )?
        };
        let mut gfx = match Gfx::new(mon.dpi as f32, hwnd) {
            Ok(g) => g.secondary(),
            Err(e) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(e);
            }
        };
        gfx.set_shared_d3d(shared);
        let _ = gfx.set_surface(w, h);
        let apps = crate::services::dock_apps::launchpad_snapshot();
        crate::services::dock_apps::refresh_launchpad_async(hwnd);
        let mut lp = Self {
            gfx: Some(gfx),
            hwnd,
            at: (x, y, w, h),
            dpi: mon.dpi,
            light: look.light,
            look,
            apps,
            pinned,
            query: String::new(),
            shown_indices: Vec::new(),
            page: 0,
            sel: 0,
            hovered: None,
            pressed: None,
            tracking: false,
            shown: false,
            dirty: true,
        };
        lp.refilter();
        Ok(lp)
    }

    pub fn needs_frame(&self) -> bool {
        self.dirty
    }

    pub fn set_look(&mut self, look: &'static Look) -> bool {
        if std::ptr::eq(self.look, look) && self.light == look.light {
            return false;
        }
        self.look = look;
        self.light = look.light;
        self.dirty = true;
        true
    }

    pub fn set_pinned(&mut self, pinned: Vec<String>) -> bool {
        if self.pinned == pinned {
            return false;
        }
        self.pinned = pinned;
        self.dirty = true;
        true
    }

    pub fn sync_catalog(&mut self) -> bool {
        if !crate::services::dock_apps::take_launchpad_dirty() {
            return false;
        }
        let next = crate::services::dock_apps::launchpad_snapshot();
        if self.apps == next {
            return false;
        }
        self.apps = next;
        self.refilter();
        self.dirty = true;
        true
    }

    pub fn set_apps_for_test(&mut self, apps: Vec<crate::services::dock_apps::LaunchpadApp>) {
        self.apps = apps;
        self.refilter();
        self.dirty = true;
    }

    pub fn pages(&self) -> usize {
        self.shown_indices.len().div_ceil(LP_PER_PAGE).max(1)
    }

    pub fn page(&self) -> usize {
        self.page
    }

    pub fn shown_len(&self) -> usize {
        self.shown_indices.len()
    }

    fn refilter(&mut self) {
        let q = self.query.trim().to_ascii_lowercase();
        if q.is_empty() {
            self.shown_indices = (0..self.apps.len()).collect();
        } else {
            let mut hits: Vec<(u8, usize)> = self
                .apps
                .iter()
                .enumerate()
                .filter_map(|(i, a)| {
                    crate::services::dock_apps::launchpad_rank(&a.name.to_ascii_lowercase(), &q).map(|r| (r, i))
                })
                .collect();
            hits.sort();
            self.shown_indices = hits.into_iter().map(|(_, i)| i).collect();
        }
        let pages = self.pages();
        self.page = self.page.min(pages.saturating_sub(1));
        self.sel = self.sel.min(self.shown_indices.len().saturating_sub(1));
    }

    pub fn set_query(&mut self, q: String) {
        if self.query == q {
            return;
        }
        self.query = q;
        self.page = 0;
        self.sel = 0;
        self.refilter();
        self.dirty = true;
    }

    pub fn step_page(&mut self, delta: i32) {
        let next = (self.page as i32 + delta).clamp(0, self.pages() as i32 - 1) as usize;
        if next != self.page {
            self.page = next;
            self.sel = next * LP_PER_PAGE;
            self.dirty = true;
        }
    }

    pub fn move_selection(&mut self, delta: i32) {
        if self.shown_indices.is_empty() {
            return;
        }
        let next = (self.sel as i32 + delta).clamp(0, self.shown_indices.len() as i32 - 1) as usize;
        self.sel = next;
        self.page = next / LP_PER_PAGE;
        self.dirty = true;
    }

    fn scale(&self) -> f32 {
        (self.dpi.max(96) as f32) / 96.0
    }

    fn card_dip(&self) -> Rect {
        Rect::new(0.0, 0.0, LP_W, LP_H)
    }

    fn app_at_cell(&self, cell: usize) -> Option<&crate::services::dock_apps::LaunchpadApp> {
        if cell >= LP_PER_PAGE {
            return None;
        }
        let idx = *self.shown_indices.get(self.page * LP_PER_PAGE + cell)?;
        self.apps.get(idx)
    }

    fn hit_test(&self, p: Point) -> Option<LaunchpadZone> {
        let card = self.card_dip();
        if !card.contains(p) {
            return None;
        }
        if launchpad_search_rect(card).contains(p) {
            return Some(LaunchpadZone::Search);
        }
        if launchpad_settings_btn_rect(card).contains(p) {
            return Some(LaunchpadZone::SettingsBtn);
        }
        let pages = self.pages();
        if pages > 1 {
            for d in 0..pages {
                if launchpad_dot_rect(card, d, pages).contains(p) {
                    return Some(LaunchpadZone::Dot(d));
                }
            }
        }
        for cell in 0..LP_PER_PAGE {
            if self.app_at_cell(cell).is_some() && launchpad_cell_rect(card, cell).contains(p) {
                return Some(LaunchpadZone::Cell(cell));
            }
        }
        None
    }

    pub fn handle_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> (Option<LRESULT>, LaunchpadAction) {
        if hwnd != self.hwnd {
            return (None, LaunchpadAction::None);
        }
        let to_dip = |lp: LPARAM, scale: f32| {
            let x = (lp.0 & 0xffff) as u16 as i16 as f32;
            let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as f32;
            Point::new(x / scale, y / scale)
        };
        match msg {
            crate::events::WM_APP_EVENT => {
                if self.sync_catalog() {
                    return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                }
                (Some(LRESULT(0)), LaunchpadAction::None)
            }
            WM_KILLFOCUS => (Some(LRESULT(0)), LaunchpadAction::Close),
            WM_ACTIVATE if (wp.0 & 0xffff) == 0 => (Some(LRESULT(0)), LaunchpadAction::Close),
            WM_MOUSEWHEEL => {
                let delta = ((wp.0 >> 16) & 0xffff) as u16 as i16;
                if delta != 0 {
                    self.step_page(if delta < 0 { 1 } else { -1 });
                    return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                }
                (Some(LRESULT(0)), LaunchpadAction::None)
            }
            WM_KEYDOWN => {
                let vk = wp.0 as u32;
                match vk {
                    0x1B => {
                        // Escape clears search first; if empty, closes Launchpad.
                        if !self.query.is_empty() {
                            self.set_query(String::new());
                            return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                        }
                        return (Some(LRESULT(0)), LaunchpadAction::Close);
                    }
                    0x0D => {
                        // Enter launches the currently selected app.
                        if let Some(&idx) = self.shown_indices.get(self.sel) {
                            if let Some(app) = self.apps.get(idx) {
                                return (Some(LRESULT(0)), LaunchpadAction::LaunchApp(app.spec.clone()));
                            }
                        }
                    }
                    0x25 => {
                        self.move_selection(-1);
                        return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                    }
                    0x27 => {
                        self.move_selection(1);
                        return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                    }
                    0x26 => {
                        self.move_selection(-(LP_COLS as i32));
                        return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                    }
                    0x28 => {
                        self.move_selection(LP_COLS as i32);
                        return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                    }
                    0x21 => {
                        self.step_page(-1);
                        return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                    }
                    0x22 => {
                        self.step_page(1);
                        return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                    }
                    _ => {}
                }
                (Some(LRESULT(0)), LaunchpadAction::None)
            }
            WM_CHAR => {
                let ch = char::from_u32(wp.0 as u32).unwrap_or('\0');
                if ch == '\u{8}' {
                    if !self.query.is_empty() {
                        let mut q = self.query.clone();
                        q.pop();
                        self.set_query(q);
                        return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                    }
                } else if !ch.is_control() && self.query.chars().count() < 36 {
                    let mut q = self.query.clone();
                    q.push(ch);
                    self.set_query(q);
                    return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                }
                (Some(LRESULT(0)), LaunchpadAction::None)
            }
            WM_MOUSEMOVE => {
                if !self.tracking {
                    self.tracking = true;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: self.hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                }
                let p = to_dip(lp, self.scale());
                let next = self.hit_test(p);
                if next != self.hovered {
                    self.hovered = next;
                    self.dirty = true;
                    return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                }
                (Some(LRESULT(0)), LaunchpadAction::None)
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                if self.hovered.take().is_some() || self.pressed.take().is_some() {
                    self.dirty = true;
                    return (Some(LRESULT(0)), LaunchpadAction::RequestFrame);
                }
                (Some(LRESULT(0)), LaunchpadAction::None)
            }
            WM_LBUTTONDOWN => {
                let p = to_dip(lp, self.scale());
                self.pressed = self.hit_test(p);
                self.dirty = true;
                (Some(LRESULT(0)), LaunchpadAction::RequestFrame)
            }
            WM_LBUTTONUP => {
                let p = to_dip(lp, self.scale());
                let hit = self.hit_test(p);
                let prev = self.pressed.take();
                self.dirty = true;
                match (prev, hit) {
                    (_, Some(LaunchpadZone::SettingsBtn)) => (Some(LRESULT(0)), LaunchpadAction::OpenSettings),
                    (_, Some(LaunchpadZone::Dot(d))) => {
                        if d < self.pages() && d != self.page {
                            self.page = d;
                            self.sel = d * LP_PER_PAGE;
                        }
                        (Some(LRESULT(0)), LaunchpadAction::RequestFrame)
                    }
                    (_, Some(LaunchpadZone::Cell(cell))) => {
                        if let Some(app) = self.app_at_cell(cell) {
                            return (Some(LRESULT(0)), LaunchpadAction::LaunchApp(app.spec.clone()));
                        }
                        (Some(LRESULT(0)), LaunchpadAction::RequestFrame)
                    }
                    (_, Some(LaunchpadZone::Search)) => (Some(LRESULT(0)), LaunchpadAction::RequestFrame),
                    (None, None) => (Some(LRESULT(0)), LaunchpadAction::Close),
                    _ => (Some(LRESULT(0)), LaunchpadAction::RequestFrame),
                }
            }
            WM_RBUTTONUP => {
                let p = to_dip(lp, self.scale());
                if let Some(LaunchpadZone::Cell(cell)) = self.hit_test(p) {
                    if let Some(app) = self.app_at_cell(cell) {
                        return (Some(LRESULT(0)), LaunchpadAction::TogglePin(app.spec.clone()));
                    }
                }
                (Some(LRESULT(0)), LaunchpadAction::Close)
            }
            _ => (None, LaunchpadAction::None),
        }
    }

    pub fn paint(&mut self, shared: Option<SharedD3d>) {
        if !self.dirty {
            return;
        }
        let card = self.card_dip();
        let (x, y, w, h) = self.at;
        let scale = self.scale();
        let light = self.light;
        let look = self.look;
        let hovered = self.hovered;
        let pressed = self.pressed;
        let page = self.page;
        let pages = self.pages();
        let sel = self.sel;
        let query = self.query.clone();
        let apps = self.apps.clone();
        let shown_indices = self.shown_indices.clone();
        let pinned = self.pinned.clone();

        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if gfx.set_surface(w, h).is_err() {
            return;
        }
        gfx.set_look(look);
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        let body = card.inset(1.0, 1.0);
        let glass_params = if look.material == look::Material::Glass {
            let gp = dock_glass_params(body, LP_R, look, scale);
            gfx.prepare_glass(&gp, full);
            Some(gp)
        } else {
            None
        };

        let ink = look.ink;
        let sub = ink.with_alpha(if light { 0.58 } else { 0.68 });
        let accent = look.accent.unwrap_or(Color::hex(0x0A84FF));
        let card_fill = if light { Color::white(0.55) } else { Color::white(0.08) };
        let card_rim = if light { Color::black(0.08) } else { Color::white(0.14) };

        if let Ok(mut c) = gfx.begin(full) {
            c.unthemed(|c| {
                // 1. Liquid Glass Launchpad container
                if let Some(gp) = glass_params.as_ref() {
                    c.glass(gp, card);
                } else {
                    crate::pill::shape::draw_body(c, body, LP_R, look.ground, Some(look.border));
                }

                // 2. Top Search Pill
                let sr = launchpad_search_rect(card);
                let sr_hov = hovered == Some(LaunchpadZone::Search);
                c.fill_rrect(
                    sr,
                    sr.h * 0.5,
                    if !query.is_empty() || sr_hov {
                        if light { Color::white(0.78) } else { Color::white(0.14) }
                    } else {
                        card_fill
                    },
                );
                c.stroke_rrect(
                    sr,
                    sr.h * 0.5,
                    if !query.is_empty() { accent.with_alpha(0.65) } else { card_rim },
                    1.0,
                );
                if query.is_empty() {
                    let ph_st = TextStyle {
                        size: 13.0,
                        ..TextStyle::body().weight(Weight::Medium).center().color(sub)
                    };
                    c.text("🔍  Search Apps", Rect::new(sr.x + 12.0, sr.y + 7.0, sr.w - 24.0, 20.0), &ph_st);
                } else {
                    let q_st = TextStyle {
                        size: 13.0,
                        ..TextStyle::body().weight(Weight::Semibold).color(ink)
                    };
                    c.text(
                        &format!("🔍  {query}"),
                        Rect::new(sr.x + 14.0, sr.y + 7.0, sr.w - 28.0, 20.0),
                        &q_st,
                    );
                }

                // 3. Top-right Dock Settings button + hint
                let hint_st = TextStyle {
                    size: 11.0,
                    ..TextStyle::caption().weight(Weight::Medium).color(sub)
                };
                c.text(
                    "Right-click app to pin/unpin",
                    Rect::new(card.x + LP_PAD_X, card.y + LP_SEARCH_Y + 9.0, 210.0, 16.0),
                    &hint_st,
                );
                let sb = launchpad_settings_btn_rect(card);
                let sb_hov = hovered == Some(LaunchpadZone::SettingsBtn);
                c.fill_rrect(
                    sb,
                    sb.h * 0.5,
                    if sb_hov { accent.with_alpha(0.22) } else { card_fill },
                );
                c.stroke_rrect(sb, sb.h * 0.5, if sb_hov { accent.with_alpha(0.55) } else { card_rim }, 1.0);
                c.icon(
                    Icon::Gear,
                    Rect::new(sb.x + 8.0, sb.y + 8.0, 16.0, 16.0),
                    if sb_hov { accent } else { ink },
                );

                // 4. App Grid (7 x 5 = 35 per page)
                if shown_indices.is_empty() {
                    let msg = if apps.is_empty() {
                        "Loading apps from Windows Start menu…"
                    } else {
                        "No matching apps"
                    };
                    let empty_st = TextStyle {
                        size: 16.0,
                        ..TextStyle::body().weight(Weight::Medium).center().color(sub)
                    };
                    c.text(
                        msg,
                        Rect::new(card.x + 60.0, card.y + LP_GRID_TOP + LP_GRID_H * 0.44, card.w - 120.0, 28.0),
                        &empty_st,
                    );
                } else {
                    let start = page * LP_PER_PAGE;
                    for cell in 0..LP_PER_PAGE {
                        let Some(&ai) = shown_indices.get(start + cell) else {
                            break;
                        };
                        let Some(app) = apps.get(ai) else {
                            continue;
                        };
                        let cr = launchpad_cell_rect(card, cell);
                        let is_hov = hovered == Some(LaunchpadZone::Cell(cell));
                        let is_press = pressed == Some(LaunchpadZone::Cell(cell));
                        let is_sel = !query.is_empty() && (start + cell) == sel;
                        let is_pin = pinned.iter().any(|p| p.eq_ignore_ascii_case(&app.spec));

                        let tile = Rect::new(cr.x + 6.0, cr.y + 3.0, cr.w - 12.0, cr.h - 6.0);
                        if is_hov || is_sel {
                            let bg = if is_press {
                                accent.with_alpha(0.26)
                            } else if is_hov {
                                if light { Color::black(0.07) } else { Color::white(0.11) }
                            } else {
                                accent.with_alpha(0.16)
                            };
                            c.fill_rrect(tile, 14.0, bg);
                            c.stroke_rrect(
                                tile,
                                14.0,
                                if is_sel { accent.with_alpha(0.55) } else { card_rim },
                                1.0,
                            );
                        }

                        let icon_sz = if is_press { 44.0 } else if is_hov { 50.0 } else { 48.0 };
                        let icon_r = Rect::new(
                            cr.x + (cr.w - icon_sz) * 0.5,
                            cr.y + 8.0 + (48.0 - icon_sz) * 0.5,
                            icon_sz,
                            icon_sz,
                        );
                        if let Some(img) = app.icon.as_ref() {
                            c.image(img, icon_r, icon_sz * 0.22);
                        } else {
                            c.fill_rrect(icon_r, icon_sz * 0.22, accent.with_alpha(0.22));
                            c.stroke_rrect(icon_r, icon_sz * 0.22, accent.with_alpha(0.48), 1.0);
                            let ch = app.name.chars().next().unwrap_or('A').to_ascii_uppercase();
                            let l_st = TextStyle {
                                size: 20.0,
                                ..TextStyle::body().weight(Weight::Bold).center().color(ink)
                            };
                            c.text(
                                &ch.to_string(),
                                Rect::new(icon_r.x, icon_r.y + 11.0, icon_r.w, 26.0),
                                &l_st,
                            );
                        }

                        // Pinned indicator badge in top-right of icon when kept in dock
                        if is_pin {
                            let pin_c = Point::new(icon_r.right() - 3.0, icon_r.y + 3.0);
                            c.fill_circle(pin_c, 4.5, accent);
                        }

                        let mut lbl = app.name.clone();
                        if lbl.chars().count() > 16 {
                            lbl = lbl.chars().take(15).collect::<String>() + "…";
                        }
                        let lbl_st = TextStyle {
                            size: 11.5,
                            ..TextStyle::caption()
                                .weight(if is_hov || is_sel { Weight::Semibold } else { Weight::Medium })
                                .center()
                                .color(ink)
                        };
                        c.text(
                            &lbl,
                            Rect::new(cr.x + 4.0, cr.y + 60.0, cr.w - 8.0, 18.0),
                            &lbl_st,
                        );
                    }
                }

                // 5. Page Dots at bottom
                if pages > 1 {
                    for d in 0..pages {
                        let dr = launchpad_dot_rect(card, d, pages);
                        let center = Point::new(dr.x + dr.w * 0.5, dr.y + dr.h * 0.5);
                        let is_cur = d == page;
                        let is_h = hovered == Some(LaunchpadZone::Dot(d));
                        let col = if is_cur {
                            accent
                        } else if is_h {
                            ink.with_alpha(0.65)
                        } else {
                            sub.with_alpha(0.45)
                        };
                        c.fill_circle(center, if is_cur { 4.5 } else { 3.5 }, col);
                    }
                }
            });
            let _ = gfx.end();
        }
        let _ = gfx.present(x, y);
        if gfx.needs_hit_mask() {
            let mask = HitMask { rect: None, notch: None, pill: Some((card, LP_R)) };
            let _ = gfx.push_hit_mask(x, y, &mask, None);
        }
        if !self.shown {
            self.shown = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOW);
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE);
                let _ = SetForegroundWindow(self.hwnd);
            }
            gfx.set_shown(true);
        }
        self.dirty = false;
    }
}

// ============================================================================
// POPOVER 2: SYSTEM TRAY CONTROL CENTER (`#trayFlyout` in full_ecosystem.html)
// ============================================================================

pub const FLYOUT_W: f32 = 348.0;
pub const FLYOUT_H: f32 = 416.0;
pub const FLYOUT_PAD: f32 = 15.0;
pub const FLYOUT_R: f32 = 20.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CcView {
    #[default]
    Main,
    Wifi,
    Battery,
    Calendar,
    Ai,
    Tray,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DockAiRow {
    pub name: String,
    pub pct: u8,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DockCcExtra {
    pub saver: bool,
    pub cpu_pct: u8,
    pub power_w: Option<f32>,
    pub ai_rows: Vec<DockAiRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FlyoutAction {
    None,
    RequestFrame,
    SetVolume(f32),
    SetBrightness(u8),
    ToggleMute,
    ToggleFocus,
    OpenUri(&'static str),
    OpenSystemPage,
    OpenAiPage,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlyoutZone {
    TileWifi,
    TileBluetooth,
    TileFocus,
    TileSnip,
    NetworkBox,
    BrightnessSlider,
    VolumeIcon,
    VolumeSlider,
    SubWifi,
    SubBattery,
    SubCalendar,
    SubAi,
    SubTray,
    FooterSystem,
    SubBack,
    SubAction1,
    SubAction2,
    CalPrev,
    CalNext,
    CalToday,
    TrayCell(usize),
}

/// Physical pixel bounds `(x, y, w, h)` for the Control Center flyout anchored above `anchor_screen`.
pub fn flyout_surface_rect(mon: &MonitorInfo, anchor_screen: Rect) -> (i32, i32, i32, i32) {
    let scale = mon.scale().max(1.0);
    let w = ((FLYOUT_W * scale).ceil() as i32).max(180);
    let h = ((FLYOUT_H * scale).ceil() as i32).max(160);
    let gap = (10.0 * scale).round() as i32;
    let cx = (anchor_screen.x + anchor_screen.w * 0.5).round() as i32;
    let min_x = mon.rect.left + (8.0 * scale) as i32;
    let max_x = (mon.rect.right - w - (8.0 * scale) as i32).max(min_x);
    let x = (cx - (w * 78) / 100).clamp(min_x, max_x);
    let y = (anchor_screen.y.round() as i32 - h - gap).max(mon.rect.top + (8.0 * scale) as i32);
    (x, y, w, h)
}

/// 2×2 Quick Tile rectangle inside `card` (`idx` in `0..4`).
pub fn flyout_tile_rect(card: Rect, idx: usize) -> Rect {
    let gap = 9.0;
    let inner_w = (card.w - 2.0 * FLYOUT_PAD).max(80.0);
    let tile_w = (inner_w - gap) * 0.5;
    let tile_h = 50.0;
    let col = (idx % 2) as f32;
    let row = (idx / 2) as f32;
    Rect::new(
        FLYOUT_PAD + col * (tile_w + gap),
        FLYOUT_PAD + row * (tile_h + gap),
        tile_w,
        tile_h,
    )
}

/// Recessed Available Networks card rectangle (`.cc-net-box`).
pub fn flyout_net_box_rect(card: Rect) -> Rect {
    Rect::new(FLYOUT_PAD, 132.0, (card.w - 2.0 * FLYOUT_PAD).max(80.0), 78.0)
}

/// Display Brightness slider card rectangle (`.cc-bright-card`).
pub fn flyout_brightness_card_rect(card: Rect) -> Rect {
    Rect::new(FLYOUT_PAD, 218.0, (card.w - 2.0 * FLYOUT_PAD).max(80.0), 58.0)
}

/// Interactive pill track inside the Display Brightness slider card.
pub fn flyout_brightness_track_rect(card: Rect) -> Rect {
    let bc = flyout_brightness_card_rect(card);
    Rect::new(bc.x + 12.0, bc.y + 30.0, (bc.w - 24.0).max(40.0), 20.0)
}

/// Sound Master Volume slider card rectangle (`.cc-slider-card`).
pub fn flyout_slider_card_rect(card: Rect) -> Rect {
    Rect::new(FLYOUT_PAD, 284.0, (card.w - 2.0 * FLYOUT_PAD).max(80.0), 58.0)
}

/// Interactive pill track inside the Sound slider card (`.cc-sl-track`).
pub fn flyout_slider_track_rect(card: Rect) -> Rect {
    let sc = flyout_slider_card_rect(card);
    Rect::new(sc.x + 12.0, sc.y + 30.0, (sc.w - 24.0).max(40.0), 20.0)
}

/// Speaker mute-toggle hit box inside `.cc-sl-top`.
pub fn flyout_volume_icon_rect(card: Rect) -> Rect {
    let sc = flyout_slider_card_rect(card);
    Rect::new(sc.x + 8.0, sc.y + 5.0, 28.0, 22.0)
}

/// Quick sub-flyout pill button (`idx` in `0..5`: Wi-Fi, Battery, Calendar, AI, Tray ^).
pub fn flyout_sub_pill_rect(card: Rect, idx: usize) -> Rect {
    let gap = 5.0;
    let inner_w = (card.w - 2.0 * FLYOUT_PAD).max(120.0);
    let w = (inner_w - 4.0 * gap) * 0.20;
    Rect::new(FLYOUT_PAD + idx as f32 * (w + gap), 350.0, w, 24.0)
}

pub const CC_TRAY_COLS: usize = 5;
pub const CC_TRAY_ROWS: usize = 4;
pub const CC_TRAY_MAX: usize = CC_TRAY_COLS * CC_TRAY_ROWS;

/// Cell rectangle inside the `CcView::Tray` 5×4 notification area grid (`idx` in `0..20`).
pub fn cc_tray_cell_rect(card: Rect, idx: usize) -> Rect {
    let gap = 8.0;
    let inner_w = (card.w - 2.0 * FLYOUT_PAD).max(150.0);
    let cell_w = (inner_w - (CC_TRAY_COLS as f32 - 1.0) * gap) / (CC_TRAY_COLS as f32);
    let cell_h = 46.0;
    let col = (idx % CC_TRAY_COLS) as f32;
    let row = (idx / CC_TRAY_COLS) as f32;
    Rect::new(
        FLYOUT_PAD + col * (cell_w + gap),
        90.0 + row * (cell_h + gap),
        cell_w,
        cell_h,
    )
}

/// Footer status row rectangle (`.cc-foot`).
pub fn flyout_footer_rect(card: Rect) -> Rect {
    Rect::new(FLYOUT_PAD, 382.0, (card.w - 2.0 * FLYOUT_PAD).max(80.0), 20.0)
}

fn cc_back_rect() -> Rect {
    Rect::new(FLYOUT_PAD, FLYOUT_PAD, 138.0, 26.0)
}

fn cc_sub_action_rect(card: Rect, row: usize) -> Rect {
    let y = card.h - FLYOUT_PAD - (2 - row) as f32 * 34.0 + 4.0;
    Rect::new(FLYOUT_PAD, y, (card.w - 2.0 * FLYOUT_PAD).max(80.0), 28.0)
}

fn cc_cal_nav_rects(card: Rect) -> (Rect, Rect, Rect) {
    let y = 78.0;
    let title = Rect::new(FLYOUT_PAD, y, 160.0, 26.0);
    let prev = Rect::new(card.w - FLYOUT_PAD - 56.0, y, 26.0, 26.0);
    let next = Rect::new(card.w - FLYOUT_PAD - 26.0, y, 26.0, 26.0);
    (title, prev, next)
}

fn cc_today_ymd() -> (i32, u8, u8) {
    let dt = crate::clock::local_shown();
    (dt.year.max(2024), dt.month.clamp(1, 12), dt.day.clamp(1, 31))
}

fn cc_days_in_month(y: i32, m: u8) -> i32 {
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][(m.clamp(1, 12) - 1) as usize]
}

/// 0 = Sunday (Sakamoto).
fn cc_weekday(y: i32, m: u8, d: i32) -> i32 {
    const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let mi = (m.clamp(1, 12) - 1) as usize;
    let y = if m < 3 { y - 1 } else { y };
    (y + y / 4 - y / 100 + y / 400 + T[mi] + d).rem_euclid(7)
}

/// Transient System Tray Control Center popover (`Halo.DockFlyout`).
pub struct DockControlCenter {
    gfx: Option<Gfx>,
    pub hwnd: HWND,
    at: (i32, i32, i32, i32),
    dpi: u32,
    light: bool,
    look: &'static Look,
    tray: DockTrayState,
    live: DockLive,
    pub view: CcView,
    pub brightness_pct: u8,
    pub extra: DockCcExtra,
    pub cal_year: i32,
    pub cal_month: u8,
    tray_hosted: bool,
    tray_opened_at: std::time::Instant,
    tray_shown: Vec<(u32, u32)>,
    tray_hot: Option<usize>,
    hovered: Option<FlyoutZone>,
    dragging_vol: bool,
    dragging_bright: bool,
    tracking: bool,
    shown: bool,
    dirty: bool,
}

impl Drop for DockControlCenter {
    fn drop(&mut self) {
        self.stop_tray_host();
        let visual = self.gfx.as_ref().and_then(|g| g.visual_hwnd());
        self.gfx = None;
        unsafe {
            if let Some(v) = visual {
                let _ = DestroyWindow(v);
            }
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

impl DockControlCenter {
    pub fn open(
        mon: &MonitorInfo,
        anchor_screen: Rect,
        tray: DockTrayState,
        live: DockLive,
        look: &'static Look,
        shared: Option<SharedD3d>,
    ) -> Result<Self> {
        let (x, y, w, h) = flyout_surface_rect(mon, anchor_screen);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                FLYOUT_CLASS,
                w!("Halo Control Center"),
                WS_POPUP,
                x,
                y,
                w,
                h,
                None,
                None,
                GetModuleHandleW(None)?,
                None,
            )?
        };
        let mut gfx = match Gfx::new(mon.dpi as f32, hwnd) {
            Ok(g) => g.secondary(),
            Err(e) => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                return Err(e);
            }
        };
        gfx.set_shared_d3d(shared);
        let _ = gfx.set_surface(w, h);
        let (cal_year, cal_month, _) = cc_today_ymd();
        let brightness_pct = crate::services::power::query_brightness().unwrap_or(80);
        Ok(Self {
            gfx: Some(gfx),
            hwnd,
            at: (x, y, w, h),
            dpi: mon.dpi,
            light: look.light,
            look,
            tray,
            live,
            view: CcView::Main,
            brightness_pct,
            extra: DockCcExtra::default(),
            cal_year,
            cal_month,
            tray_hosted: false,
            tray_opened_at: std::time::Instant::now(),
            tray_shown: Vec::new(),
            tray_hot: None,
            hovered: None,
            dragging_vol: false,
            dragging_bright: false,
            tracking: false,
            shown: false,
            dirty: true,
        })
    }

    fn with_tray_icon(&self, cell: usize, f: impl FnOnce(&crate::services::tray_host::TrayIcon)) {
        let Some(&(h, u)) = self.tray_shown.get(cell) else { return };
        crate::services::tray_host::with_icons(|icons| {
            if let Some(ic) = icons.and_then(|v| v.iter().find(|i| i.hwnd == h && i.uid == u)) {
                f(ic);
            }
        });
    }

    fn update_tray_hover(&mut self, next: Option<usize>) {
        if next == self.tray_hot {
            return;
        }
        if let Some(old) = self.tray_hot.take() {
            self.with_tray_icon(old, |ic| {
                if ic.version >= 4 {
                    crate::services::tray_host::send_mouse(ic, crate::services::tray_host::NIN_POPUPCLOSE_MSG);
                }
            });
        }
        self.tray_hot = next;
        if let Some(new_cell) = next {
            self.with_tray_icon(new_cell, |ic| {
                crate::services::tray_host::send_mouse(ic, crate::services::tray_host::WM_MOUSEMOVE_MSG);
                if ic.version >= 4 {
                    crate::services::tray_host::send_mouse(ic, crate::services::tray_host::NIN_POPUPOPEN_MSG);
                }
            });
        }
    }

    fn stop_tray_host(&mut self) {
        self.update_tray_hover(None);
        if self.tray_hosted || crate::services::tray_host::running() {
            unsafe {
                let _ = KillTimer(self.hwnd, 77);
            }
            crate::services::tray_host::stop();
            self.tray_hosted = false;
        }
    }

    pub fn set_view(&mut self, view: CcView) {
        if self.view == view {
            return;
        }
        if self.view == CcView::Tray && view != CcView::Tray {
            self.stop_tray_host();
        }
        self.view = view;
        self.hovered = None;
        if view == CcView::Tray {
            self.tray_opened_at = std::time::Instant::now();
            self.tray_hosted = crate::services::tray_host::start(self.hwnd);
            unsafe {
                let _ = SetTimer(self.hwnd, 77, 500, None);
            }
        }
        self.dirty = true;
    }

    pub fn set_brightness_pct(&mut self, pct: u8) -> bool {
        let clamped = pct.min(100);
        if self.dragging_bright || self.brightness_pct == clamped {
            return false;
        }
        self.brightness_pct = clamped;
        self.dirty = true;
        true
    }

    pub fn set_extra(&mut self, extra: DockCcExtra) -> bool {
        if self.extra == extra {
            return false;
        }
        self.extra = extra;
        self.dirty = true;
        true
    }

    pub fn step_calendar_month(&mut self, delta: i32) {
        let total = self.cal_year * 12 + (self.cal_month.clamp(1, 12) as i32 - 1) + delta;
        self.cal_year = total.div_euclid(12);
        self.cal_month = (total.rem_euclid(12) + 1) as u8;
        self.dirty = true;
    }

    pub fn set_live(&mut self, live: DockLive) -> bool {
        if self.live == live {
            return false;
        }
        self.live = live;
        self.dirty = true;
        true
    }

    pub fn set_look(&mut self, look: &'static Look) -> bool {
        if self.look == look && self.light == look.light {
            return false;
        }
        self.look = look;
        self.light = look.light;
        self.dirty = true;
        true
    }

    pub fn set_tray(&mut self, tray: DockTrayState) -> bool {
        if self.tray == tray {
            return false;
        }
        self.tray = tray;
        self.dirty = true;
        true
    }

    pub fn needs_frame(&self) -> bool {
        self.dirty
    }

    fn scale(&self) -> f32 {
        (self.dpi as f32 / 96.0).max(1.0)
    }

    fn card_dip(&self) -> Rect {
        let s = self.scale();
        Rect::new(0.0, 0.0, self.at.2 as f32 / s, self.at.3 as f32 / s)
    }

    fn hit_test(&self, p: Point) -> Option<FlyoutZone> {
        let card = self.card_dip();
        if self.view != CcView::Main {
            if cc_back_rect().contains(p) {
                return Some(FlyoutZone::SubBack);
            }
            if self.view == CcView::Calendar {
                let (title_r, prev_r, next_r) = cc_cal_nav_rects(card);
                if prev_r.contains(p) {
                    return Some(FlyoutZone::CalPrev);
                }
                if next_r.contains(p) {
                    return Some(FlyoutZone::CalNext);
                }
                if title_r.contains(p) {
                    return Some(FlyoutZone::CalToday);
                }
            }
            if self.view == CcView::Tray {
                for cell in 0..self.tray_shown.len().min(CC_TRAY_MAX) {
                    if cc_tray_cell_rect(card, cell).contains(p) {
                        return Some(FlyoutZone::TrayCell(cell));
                    }
                }
            }
            if cc_sub_action_rect(card, 0).contains(p) {
                return Some(FlyoutZone::SubAction1);
            }
            if cc_sub_action_rect(card, 1).contains(p) {
                return Some(FlyoutZone::SubAction2);
            }
            return None;
        }
        if flyout_brightness_track_rect(card).inset(-4.0, -4.0).contains(p) {
            return Some(FlyoutZone::BrightnessSlider);
        }
        if flyout_slider_track_rect(card).inset(-4.0, -4.0).contains(p) {
            return Some(FlyoutZone::VolumeSlider);
        }
        if flyout_volume_icon_rect(card).contains(p) {
            return Some(FlyoutZone::VolumeIcon);
        }
        for (i, z) in [
            FlyoutZone::TileWifi,
            FlyoutZone::TileBluetooth,
            FlyoutZone::TileFocus,
            FlyoutZone::TileSnip,
        ]
        .into_iter()
        .enumerate()
        {
            if flyout_tile_rect(card, i).contains(p) {
                return Some(z);
            }
        }
        if flyout_net_box_rect(card).contains(p) {
            return Some(FlyoutZone::NetworkBox);
        }
        for (i, z) in [
            FlyoutZone::SubWifi,
            FlyoutZone::SubBattery,
            FlyoutZone::SubCalendar,
            FlyoutZone::SubAi,
            FlyoutZone::SubTray,
        ]
        .into_iter()
        .enumerate()
        {
            if flyout_sub_pill_rect(card, i).contains(p) {
                return Some(z);
            }
        }
        let ft = flyout_footer_rect(card);
        if ft.contains(p) {
            if p.x >= ft.right() - 96.0 {
                return Some(FlyoutZone::FooterSystem);
            }
            return Some(FlyoutZone::SubBattery);
        }
        None
    }

    fn volume_at_x(&self, x: f32) -> f32 {
        let tr = flyout_slider_track_rect(self.card_dip());
        ((x - tr.x) / tr.w.max(1.0)).clamp(0.0, 1.0)
    }

    fn brightness_at_x(&self, x: f32) -> u8 {
        let tr = flyout_brightness_track_rect(self.card_dip());
        let frac = ((x - tr.x) / tr.w.max(1.0)).clamp(0.0, 1.0);
        (frac * 100.0).round() as u8
    }

    pub fn handle_message(&mut self, hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> (Option<LRESULT>, FlyoutAction) {
        if hwnd != self.hwnd {
            return (None, FlyoutAction::None);
        }
        let to_dip = |lp: LPARAM, scale: f32| {
            let x = (lp.0 & 0xffff) as u16 as i16 as f32;
            let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as f32;
            Point::new(x / scale, y / scale)
        };
        match msg {
            WM_MOUSEACTIVATE => (Some(LRESULT(MA_NOACTIVATE as isize)), FlyoutAction::None),
            m if m == crate::services::tray_host::WM_TRAY_HOST_DATA => {
                if self.view == CcView::Tray {
                    self.dirty = true;
                    return (Some(LRESULT(0)), FlyoutAction::RequestFrame);
                }
                (Some(LRESULT(0)), FlyoutAction::None)
            }
            WM_TIMER => {
                if self.view == CcView::Tray {
                    self.dirty = true;
                    return (Some(LRESULT(0)), FlyoutAction::RequestFrame);
                }
                (Some(LRESULT(0)), FlyoutAction::None)
            }
            WM_MOUSEWHEEL if self.view == CcView::Calendar => {
                let delta = ((wp.0 >> 16) & 0xffff) as u16 as i16;
                if delta != 0 {
                    self.step_calendar_month(if delta > 0 { -1 } else { 1 });
                    return (Some(LRESULT(0)), FlyoutAction::RequestFrame);
                }
                (Some(LRESULT(0)), FlyoutAction::None)
            }
            WM_MOUSEMOVE => {
                if !self.tracking {
                    self.tracking = true;
                    unsafe {
                        let mut tme = TRACKMOUSEEVENT {
                            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                            dwFlags: TME_LEAVE,
                            hwndTrack: self.hwnd,
                            dwHoverTime: 0,
                        };
                        let _ = TrackMouseEvent(&mut tme);
                    }
                }
                let p = to_dip(lp, self.scale());
                if self.dragging_bright {
                    let pct = self.brightness_at_x(p.x);
                    if self.brightness_pct != pct {
                        self.brightness_pct = pct;
                        self.dirty = true;
                    }
                    return (Some(LRESULT(0)), FlyoutAction::SetBrightness(pct));
                }
                if self.dragging_vol {
                    let v = self.volume_at_x(p.x);
                    let pct = (v * 100.0).round() as u8;
                    if self.tray.volume_pct != pct || self.tray.muted {
                        self.tray.volume_pct = pct;
                        self.tray.muted = pct == 0;
                        self.tray.volume_known = true;
                        self.dirty = true;
                    }
                    return (Some(LRESULT(0)), FlyoutAction::SetVolume(v));
                }
                let next = self.hit_test(p);
                if self.view == CcView::Tray {
                    let cell = match next {
                        Some(FlyoutZone::TrayCell(i)) => Some(i),
                        _ => None,
                    };
                    self.update_tray_hover(cell);
                }
                if next != self.hovered {
                    self.hovered = next;
                    self.dirty = true;
                    return (Some(LRESULT(0)), FlyoutAction::RequestFrame);
                }
                (Some(LRESULT(0)), FlyoutAction::None)
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                if self.view == CcView::Tray {
                    self.update_tray_hover(None);
                }
                if !self.dragging_vol && !self.dragging_bright && self.hovered.take().is_some() {
                    self.dirty = true;
                    return (Some(LRESULT(0)), FlyoutAction::RequestFrame);
                }
                (Some(LRESULT(0)), FlyoutAction::None)
            }
            WM_LBUTTONDOWN => {
                let p = to_dip(lp, self.scale());
                match self.hit_test(p) {
                    Some(FlyoutZone::BrightnessSlider) => {
                        self.dragging_bright = true;
                        unsafe {
                            SetCapture(self.hwnd);
                        }
                        let pct = self.brightness_at_x(p.x);
                        self.brightness_pct = pct;
                        self.dirty = true;
                        return (Some(LRESULT(0)), FlyoutAction::SetBrightness(pct));
                    }
                    Some(FlyoutZone::VolumeSlider) => {
                        self.dragging_vol = true;
                        unsafe {
                            SetCapture(self.hwnd);
                        }
                        let v = self.volume_at_x(p.x);
                        let pct = (v * 100.0).round() as u8;
                        self.tray.volume_pct = pct;
                        self.tray.muted = pct == 0;
                        self.tray.volume_known = true;
                        self.dirty = true;
                        return (Some(LRESULT(0)), FlyoutAction::SetVolume(v));
                    }
                    Some(FlyoutZone::TrayCell(cell)) => {
                        self.with_tray_icon(cell, |ic| {
                            crate::services::tray_host::send_mouse(ic, crate::services::tray_host::WM_LBUTTONDOWN_MSG);
                        });
                    }
                    _ => {}
                }
                (Some(LRESULT(0)), FlyoutAction::None)
            }
            WM_LBUTTONUP => {
                let p = to_dip(lp, self.scale());
                if self.dragging_bright {
                    self.dragging_bright = false;
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    let pct = self.brightness_at_x(p.x);
                    self.brightness_pct = pct;
                    self.dirty = true;
                    return (Some(LRESULT(0)), FlyoutAction::SetBrightness(pct));
                }
                if self.dragging_vol {
                    self.dragging_vol = false;
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    let v = self.volume_at_x(p.x);
                    self.dirty = true;
                    return (Some(LRESULT(0)), FlyoutAction::SetVolume(v));
                }
                let act = match self.hit_test(p) {
                    Some(FlyoutZone::TileWifi) => FlyoutAction::OpenUri("ms-availablenetworks:"),
                    Some(FlyoutZone::NetworkBox) | Some(FlyoutZone::SubWifi) => {
                        self.set_view(CcView::Wifi);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::SubBattery) => {
                        self.set_view(CcView::Battery);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::SubCalendar) => {
                        let (y, m, _) = cc_today_ymd();
                        self.cal_year = y;
                        self.cal_month = m;
                        self.set_view(CcView::Calendar);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::SubAi) => {
                        self.set_view(CcView::Ai);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::SubTray) => {
                        self.set_view(CcView::Tray);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::SubBack) => {
                        self.set_view(CcView::Main);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::TrayCell(cell)) => {
                        self.with_tray_icon(cell, |ic| {
                            crate::services::tray_host::send_mouse(ic, crate::services::tray_host::WM_LBUTTONUP_MSG);
                            if ic.version >= 3 {
                                crate::services::tray_host::send_mouse(ic, crate::services::tray_host::NIN_SELECT_MSG);
                            }
                        });
                        FlyoutAction::Close
                    }
                    Some(FlyoutZone::CalPrev) => {
                        self.step_calendar_month(-1);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::CalNext) => {
                        self.step_calendar_month(1);
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::CalToday) => {
                        let (y, m, _) = cc_today_ymd();
                        self.cal_year = y;
                        self.cal_month = m;
                        self.dirty = true;
                        FlyoutAction::RequestFrame
                    }
                    Some(FlyoutZone::SubAction1) => match self.view {
                        CcView::Wifi => FlyoutAction::OpenUri("ms-availablenetworks:"),
                        CcView::Battery => FlyoutAction::OpenUri("ms-settings:powersleep"),
                        CcView::Calendar => FlyoutAction::OpenUri("ms-actioncenter:"),
                        CcView::Ai => FlyoutAction::OpenAiPage,
                        CcView::Tray => {
                            self.stop_tray_host();
                            self.tray_opened_at = std::time::Instant::now();
                            self.tray_hosted = crate::services::tray_host::start(self.hwnd);
                            self.dirty = true;
                            FlyoutAction::RequestFrame
                        }
                        CcView::Main => FlyoutAction::None,
                    },
                    Some(FlyoutZone::SubAction2) => match self.view {
                        CcView::Wifi => FlyoutAction::OpenUri("ms-settings:network-wifi"),
                        CcView::Battery => FlyoutAction::OpenSystemPage,
                        CcView::Calendar => FlyoutAction::OpenUri("ms-settings:dateandtime"),
                        CcView::Ai => FlyoutAction::OpenAiPage,
                        CcView::Tray => FlyoutAction::OpenUri("ms-settings:taskbar"),
                        CcView::Main => FlyoutAction::None,
                    },
                    Some(FlyoutZone::TileBluetooth) => FlyoutAction::OpenUri("ms-settings:bluetooth"),
                    Some(FlyoutZone::TileFocus) => FlyoutAction::ToggleFocus,
                    Some(FlyoutZone::TileSnip) => FlyoutAction::OpenUri("ms-screenclip:"),
                    Some(FlyoutZone::BrightnessSlider) => {
                        let pct = self.brightness_at_x(p.x);
                        self.brightness_pct = pct;
                        self.dirty = true;
                        FlyoutAction::SetBrightness(pct)
                    }
                    Some(FlyoutZone::VolumeIcon) => FlyoutAction::ToggleMute,
                    Some(FlyoutZone::VolumeSlider) => FlyoutAction::SetVolume(self.volume_at_x(p.x)),
                    Some(FlyoutZone::FooterSystem) => FlyoutAction::OpenSystemPage,
                    None => FlyoutAction::None,
                };
                (Some(LRESULT(0)), act)
            }
            WM_CAPTURECHANGED => {
                self.dragging_vol = false;
                self.dragging_bright = false;
                (Some(LRESULT(0)), FlyoutAction::None)
            }
            WM_RBUTTONUP => {
                if self.view == CcView::Tray {
                    let p = to_dip(lp, self.scale());
                    if let Some(FlyoutZone::TrayCell(cell)) = self.hit_test(p) {
                        self.with_tray_icon(cell, |ic| {
                            crate::services::tray_host::send_mouse(ic, crate::services::tray_host::WM_RBUTTONDOWN_MSG);
                            crate::services::tray_host::send_mouse(ic, crate::services::tray_host::WM_RBUTTONUP_MSG);
                            if ic.version >= 3 {
                                crate::services::tray_host::send_mouse(ic, crate::services::tray_host::WM_CONTEXTMENU_MSG);
                            }
                        });
                        return (Some(LRESULT(0)), FlyoutAction::None);
                    }
                }
                if self.view != CcView::Main {
                    self.set_view(CcView::Main);
                    return (Some(LRESULT(0)), FlyoutAction::RequestFrame);
                }
                (Some(LRESULT(0)), FlyoutAction::Close)
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                unsafe {
                    let id = if self.hovered.is_some() || self.dragging_vol || self.dragging_bright {
                        IDC_HAND
                    } else {
                        IDC_ARROW
                    };
                    if let Ok(c) = LoadCursorW(None, id) {
                        SetCursor(c);
                    }
                }
                (Some(LRESULT(1)), FlyoutAction::None)
            }
            _ => (None, FlyoutAction::None),
        }
    }

    pub fn paint(&mut self, shared: Option<SharedD3d>) {
        if !self.dirty {
            return;
        }
        let (x, y, w, h) = self.at;
        let scale = self.scale();
        let card = self.card_dip();
        let look = self.look;
        let light = look.light;
        let tray = self.tray;
        let live = self.live.clone();
        let view = self.view;
        let bright_pct = self.brightness_pct.min(100);
        let extra = self.extra.clone();
        let cal_year = self.cal_year;
        let cal_month = self.cal_month.clamp(1, 12);
        let hovered = self.hovered;
        let tray_hosted = self.tray_hosted;
        let tray_elapsed = self.tray_opened_at.elapsed().as_secs_f32();
        let tray_hot = self.tray_hot;
        let tray_icons: Vec<crate::services::tray_host::TrayIcon> = if view == CcView::Tray {
            let mut shown = Vec::new();
            let list = crate::services::tray_host::with_icons(|icons| {
                let Some(icons) = icons else { return Vec::new() };
                let mut out = Vec::new();
                for ic in icons.iter_mut() {
                    if ic.image.is_none() || ic.hidden {
                        continue;
                    }
                    let idx = out.len();
                    if idx < CC_TRAY_MAX {
                        let cr = cc_tray_cell_rect(card, idx);
                        let l = x + (cr.x * scale).round() as i32;
                        let t = y + (cr.y * scale).round() as i32;
                        ic.place = RECT {
                            left: l,
                            top: t,
                            right: l + (cr.w * scale).round() as i32,
                            bottom: t + (cr.h * scale).round() as i32,
                        };
                    }
                    shown.push((ic.hwnd, ic.uid));
                    out.push(ic.clone());
                }
                out
            });
            self.tray_shown = shown;
            list
        } else {
            Vec::new()
        };
        let Some(gfx) = self.gfx.as_mut() else { return };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if gfx.set_surface(w, h).is_err() {
            return;
        }
        gfx.set_look(look);
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        let body = card.inset(1.0, 1.0);
        let glass_params = if look.material == look::Material::Glass {
            let gp = dock_glass_params(body, FLYOUT_R, look, scale);
            gfx.prepare_glass(&gp, full);
            Some(gp)
        } else {
            None
        };
        if let Ok(mut c) = gfx.begin(full) {
            c.unthemed(|c| {
                let ink = look.ink;
                let sub = ink.with_alpha(if light { 0.58 } else { 0.68 });
                let accent = look.accent.unwrap_or(Color::hex(0x0A84FF));
                if let Some(gp) = glass_params.as_ref() {
                    c.glass(gp, card);
                } else {
                    crate::pill::shape::draw_body(c, body, FLYOUT_R, look.ground, Some(look.border));
                }

                let wifi_on = tray.wifi_connected != Some(false);
                let wifi_sub: &str = if !wifi_on {
                    "Offline"
                } else if live.ssid.is_empty() {
                    "Connected"
                } else {
                    &live.ssid
                };

                if view != CcView::Main {
                    let back_r = cc_back_rect();
                    let back_hov = hovered == Some(FlyoutZone::SubBack);
                    let back_bg = if back_hov {
                        if light { Color::black(0.10) } else { Color::white(0.14) }
                    } else if light {
                        Color::black(0.05)
                    } else {
                        Color::white(0.07)
                    };
                    c.fill_rrect(back_r, 8.0, back_bg);
                    let back_st = TextStyle {
                        size: 11.5,
                        ..TextStyle::caption().weight(Weight::Semibold).color(if back_hov { accent } else { ink })
                    };
                    c.text("← Control Center", Rect::new(back_r.x + 8.0, back_r.y + 4.0, back_r.w - 12.0, 18.0), &back_st);

                    let view_title = match view {
                        CcView::Wifi => "WI-FI & NETWORK",
                        CcView::Battery => "BATTERY & POWER",
                        CcView::Calendar => "CALENDAR",
                        CcView::Ai => "AI QUOTA & USAGE",
                        CcView::Tray => "BACKGROUND APPS (^)",
                        CcView::Main => "",
                    };
                    let vt_st = TextStyle {
                        size: 10.0,
                        ..TextStyle::caption().weight(Weight::Bold).right().color(sub.with_alpha(0.75))
                    };
                    c.text(view_title, Rect::new(card.w - FLYOUT_PAD - 150.0, FLYOUT_PAD + 5.0, 150.0, 16.0), &vt_st);
                    c.fill_rrect(
                        Rect::new(FLYOUT_PAD, 47.0, card.w - 2.0 * FLYOUT_PAD, 1.0),
                        0.5,
                        if light { Color::black(0.08) } else { Color::white(0.10) },
                    );

                    match view {
                        CcView::Wifi => {
                            let hero = Rect::new(FLYOUT_PAD, 58.0, card.w - 2.0 * FLYOUT_PAD, 92.0);
                            c.fill_rrect(hero, 14.0, if light { Color::black(0.05) } else { Color::white(0.07) });
                            c.stroke_rrect(hero, 14.0, if light { Color::black(0.08) } else { Color::white(0.10) }, 1.0);
                            let cir_c = Point::new(hero.x + 28.0, hero.y + 32.0);
                            let ac = if wifi_on { Color::hex(0x2563EB) } else { Color::hex(0x64748B) };
                            c.fill_circle(cir_c, 16.0, ac);
                            c.icon(
                                if wifi_on { Icon::Wifi } else { Icon::WifiOff },
                                Rect::new(cir_c.x - 8.0, cir_c.y - 8.0, 16.0, 16.0),
                                Color::white(1.0),
                            );
                            let t_st = TextStyle { size: 13.5, ..TextStyle::caption().weight(Weight::Semibold).color(ink) };
                            c.text(wifi_sub, Rect::new(hero.x + 54.0, hero.y + 13.0, hero.w - 66.0, 18.0), &t_st);
                            let net_txt = match (wifi_on, live.internet) {
                                (false, _) => "Wi-Fi adapter offline",
                                (true, Some(true)) => "Connected · Internet reachable",
                                (true, Some(false)) => "Connected · No internet access",
                                (true, None) => "Connected · Checking reachability…",
                            };
                            let s_st = TextStyle { size: 11.0, ..TextStyle::caption().color(sub) };
                            c.text(net_txt, Rect::new(hero.x + 54.0, hero.y + 33.0, hero.w - 66.0, 15.0), &s_st);
                            let sig_txt = if wifi_on { "Signal: Strong (802.11ax / Wi-Fi)" } else { "Radio switched off" };
                            c.text(sig_txt, Rect::new(hero.x + 14.0, hero.y + 62.0, hero.w - 28.0, 15.0), &s_st);

                            let info_r = Rect::new(FLYOUT_PAD, 160.0, card.w - 2.0 * FLYOUT_PAD, 156.0);
                            c.fill_rrect(info_r, 14.0, if light { Color::black(0.04) } else { Color::rgba(0.0, 0.0, 0.0, 0.24) });
                            let h_st = TextStyle { size: 10.0, ..TextStyle::caption().weight(Weight::Bold).color(sub.with_alpha(0.65)) };
                            c.text("NETWORK DETAILS", Rect::new(info_r.x + 12.0, info_r.y + 10.0, 180.0, 14.0), &h_st);
                            let rows = [
                                ("Primary SSID", if wifi_on { wifi_sub } else { "None" }),
                                ("Internet Status", if !wifi_on { "Offline" } else if live.internet == Some(false) { "No internet" } else { "Online" }),
                                ("Bluetooth Link", if live.bt_name.is_empty() { "Standby" } else { &live.bt_name }),
                                ("Security", if wifi_on { "WPA2 / WPA3 Personal" } else { "—" }),
                            ];
                            let l_st = TextStyle { size: 11.0, ..TextStyle::caption().color(sub) };
                            let v_st = TextStyle { size: 11.0, ..TextStyle::caption().weight(Weight::Semibold).right().color(ink) };
                            for (idx, (k, val)) in rows.into_iter().enumerate() {
                                let ry = info_r.y + 34.0 + idx as f32 * 28.0;
                                c.text(k, Rect::new(info_r.x + 12.0, ry, 130.0, 16.0), &l_st);
                                c.text(val, Rect::new(info_r.right() - 170.0, ry, 158.0, 16.0), &v_st);
                            }
                        }
                        CcView::Battery => {
                            let hero = Rect::new(FLYOUT_PAD, 58.0, card.w - 2.0 * FLYOUT_PAD, 90.0);
                            c.fill_rrect(hero, 14.0, if light { Color::black(0.05) } else { Color::white(0.07) });
                            c.stroke_rrect(hero, 14.0, if light { Color::black(0.08) } else { Color::white(0.10) }, 1.0);
                            let pct_str = if tray.has_battery {
                                format!("{}%", tray.battery_pct)
                            } else {
                                "AC".to_string()
                            };
                            let big_st = TextStyle { size: 28.0, ..TextStyle::caption().weight(Weight::Bold).color(ink) };
                            c.text(&pct_str, Rect::new(hero.x + 14.0, hero.y + 12.0, 110.0, 34.0), &big_st);
                            let (status_lbl, badge_col) = if !tray.has_battery {
                                ("Desktop AC Power", Color::hex(0x34C759))
                            } else if tray.charging {
                                ("Charging · AC Connected", Color::hex(0x34C759))
                            } else if tray.on_ac {
                                ("Plugged In · Holding Charge", Color::hex(0x34C759))
                            } else if tray.battery_pct <= 20 {
                                ("Low Battery · Discharging", Color::hex(0xFF9F0A))
                            } else {
                                ("Running on Battery", Color::hex(0x38BDF8))
                            };
                            let st_st = TextStyle { size: 11.5, ..TextStyle::caption().weight(Weight::Semibold).color(badge_col) };
                            c.text(status_lbl, Rect::new(hero.x + 14.0, hero.y + 52.0, hero.w - 28.0, 16.0), &st_st);

                            let tile_w = (card.w - 2.0 * FLYOUT_PAD - 12.0) / 3.0;
                            let pwr_str = extra
                                .power_w
                                .filter(|w| *w > 0.1)
                                .map(|w| format!("{w:.1} W"))
                                .unwrap_or_else(|| format!("{}% CPU", extra.cpu_pct));
                            let flow = [
                                ("Source", if tray.on_ac || !tray.has_battery { "AC Adapter" } else { "Battery" }),
                                ("System Draw", pwr_str.as_str()),
                                ("Energy Mode", if extra.saver { "Battery Saver" } else { "Balanced" }),
                            ];
                            let fl_st = TextStyle { size: 10.0, ..TextStyle::caption().color(sub) };
                            let fv_st = TextStyle { size: 12.0, ..TextStyle::caption().weight(Weight::Semibold).color(ink) };
                            for (i, (k, v)) in flow.into_iter().enumerate() {
                                let tx = FLYOUT_PAD + i as f32 * (tile_w + 6.0);
                                let tr = Rect::new(tx, 158.0, tile_w, 64.0);
                                c.fill_rrect(tr, 12.0, if light { Color::black(0.05) } else { Color::white(0.06) });
                                c.text(k, Rect::new(tr.x + 10.0, tr.y + 10.0, tr.w - 16.0, 14.0), &fl_st);
                                c.text(v, Rect::new(tr.x + 10.0, tr.y + 30.0, tr.w - 16.0, 18.0), &fv_st);
                            }

                            let br_info = Rect::new(FLYOUT_PAD, 232.0, card.w - 2.0 * FLYOUT_PAD, 82.0);
                            c.fill_rrect(br_info, 12.0, if light { Color::black(0.04) } else { Color::rgba(0.0, 0.0, 0.0, 0.24) });
                            c.text(
                                "DISPLAY & AUDIO TELEMETRY",
                                Rect::new(br_info.x + 12.0, br_info.y + 10.0, 200.0, 14.0),
                                &TextStyle { size: 9.5, ..TextStyle::caption().weight(Weight::Bold).color(sub.with_alpha(0.65)) },
                            );
                            let line1 = format!("Panel Brightness: {bright_pct}%");
                            let line2 = format!(
                                "Audio Output: {}",
                                if tray.muted { "Muted".to_string() } else { format!("{}%", tray.volume_pct) }
                            );
                            let info_st = TextStyle { size: 11.0, ..TextStyle::caption().color(ink) };
                            c.text(&line1, Rect::new(br_info.x + 12.0, br_info.y + 32.0, br_info.w - 24.0, 16.0), &info_st);
                            c.text(&line2, Rect::new(br_info.x + 12.0, br_info.y + 52.0, br_info.w - 24.0, 16.0), &info_st);
                        }
                        CcView::Calendar => {
                            const MONTHS: [&str; 12] = [
                                "January", "February", "March", "April", "May", "June",
                                "July", "August", "September", "October", "November", "December",
                            ];
                            const DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
                            let (ty, tm, td) = cc_today_ymd();
                            let wd = DAYS[cc_weekday(ty, tm, td as i32) as usize];
                            let today_line = format!("{wd}, {td} {} {ty}", MONTHS[(tm - 1) as usize]);
                            let td_st = TextStyle { size: 12.0, ..TextStyle::caption().weight(Weight::Semibold).color(sub) };
                            c.text(&today_line, Rect::new(FLYOUT_PAD, 54.0, card.w - 2.0 * FLYOUT_PAD, 18.0), &td_st);

                            let (title_r, prev_r, next_r) = cc_cal_nav_rects(card);
                            let away = (cal_year, cal_month) != (ty, tm);
                            if hovered == Some(FlyoutZone::CalToday) && away {
                                c.fill_rrect(title_r, 7.0, if light { Color::black(0.08) } else { Color::white(0.12) });
                            }
                            let m_title = format!("{} {}", MONTHS[(cal_month - 1) as usize], cal_year);
                            let mt_st = TextStyle {
                                size: 13.5,
                                ..TextStyle::caption().weight(Weight::Bold).color(if away { accent } else { ink })
                            };
                            c.text(&m_title, Rect::new(title_r.x + 4.0, title_r.y + 4.0, title_r.w, 20.0), &mt_st);

                            for (btn_r, z, lbl) in [(prev_r, FlyoutZone::CalPrev, "‹"), (next_r, FlyoutZone::CalNext, "›")] {
                                let is_h = hovered == Some(z);
                                c.fill_rrect(
                                    btn_r,
                                    7.0,
                                    if is_h {
                                        if light { Color::black(0.12) } else { Color::white(0.16) }
                                    } else if light {
                                        Color::black(0.05)
                                    } else {
                                        Color::white(0.08)
                                    },
                                );
                                let b_st = TextStyle { size: 14.0, ..TextStyle::caption().weight(Weight::Bold).center().color(ink) };
                                c.text(lbl, Rect::new(btn_r.x, btn_r.y + 2.0, btn_r.w, btn_r.h), &b_st);
                            }

                            let grid_w = card.w - 2.0 * FLYOUT_PAD;
                            let cell_w = grid_w / 7.0;
                            let head_y = 110.0;
                            let hd_st = TextStyle { size: 10.5, ..TextStyle::caption().weight(Weight::Semibold).center().color(sub) };
                            for (i, d) in ["S", "M", "T", "W", "T", "F", "S"].into_iter().enumerate() {
                                c.text(d, Rect::new(FLYOUT_PAD + i as f32 * cell_w, head_y, cell_w, 16.0), &hd_st);
                            }

                            let first = cc_weekday(cal_year, cal_month, 1);
                            let n_days = cc_days_in_month(cal_year, cal_month);
                            let (py, pm) = if cal_month == 1 { (cal_year - 1, 12) } else { (cal_year, cal_month - 1) };
                            let prev_n = cc_days_in_month(py, pm);
                            let row_h = 31.0;
                            let start_y = 130.0;
                            for cell in 0..42 {
                                let col = cell % 7;
                                let row = cell / 7;
                                let d = cell - first + 1;
                                let (num, inside) = if d < 1 {
                                    (prev_n + d, false)
                                } else if d > n_days {
                                    (d - n_days, false)
                                } else {
                                    (d, true)
                                };
                                let cr = Rect::new(FLYOUT_PAD + col as f32 * cell_w, start_y + row as f32 * row_h, cell_w, row_h);
                                let is_today = inside && (cal_year, cal_month, d) == (ty, tm, td as i32);
                                if is_today {
                                    c.fill_circle(cr.center(), 12.5, accent);
                                }
                                let col_txt = if is_today {
                                    Color::white(1.0)
                                } else if !inside {
                                    sub.with_alpha(0.32)
                                } else if col == 0 || col == 6 {
                                    sub
                                } else {
                                    ink
                                };
                                let d_st = TextStyle {
                                    size: 11.5,
                                    ..TextStyle::caption()
                                        .weight(if is_today { Weight::Bold } else { Weight::Regular })
                                        .center()
                                        .color(col_txt)
                                };
                                c.text(&num.to_string(), Rect::new(cr.x, cr.y + 7.0, cr.w, 16.0), &d_st);
                            }
                        }
                        CcView::Ai => {
                            let mut ry = 58.0;
                            if extra.ai_rows.is_empty() {
                                let empty_r = Rect::new(FLYOUT_PAD, ry, card.w - 2.0 * FLYOUT_PAD, 110.0);
                                c.fill_rrect(empty_r, 14.0, if light { Color::black(0.05) } else { Color::white(0.06) });
                                let t_st = TextStyle { size: 12.5, ..TextStyle::caption().weight(Weight::Semibold).center().color(ink) };
                                let s_st = TextStyle { size: 11.0, ..TextStyle::caption().center().color(sub) };
                                c.text("AI Usage Telemetry Ready", Rect::new(empty_r.x + 12.0, empty_r.y + 30.0, empty_r.w - 24.0, 18.0), &t_st);
                                c.text(
                                    "Open the AI Usage tab to inspect Claude Code, Codex, Gemini, and Antigravity quotas.",
                                    Rect::new(empty_r.x + 16.0, empty_r.y + 54.0, empty_r.w - 32.0, 32.0),
                                    &s_st,
                                );
                            } else {
                                for row in extra.ai_rows.iter().take(4) {
                                    let cr = Rect::new(FLYOUT_PAD, ry, card.w - 2.0 * FLYOUT_PAD, 60.0);
                                    c.fill_rrect(cr, 12.0, if light { Color::black(0.05) } else { Color::white(0.06) });
                                    c.stroke_rrect(cr, 12.0, if light { Color::black(0.08) } else { Color::white(0.09) }, 1.0);
                                    let bar_col = if row.pct >= 90 {
                                        Color::hex(0xFF453A)
                                    } else if row.pct >= 70 {
                                        Color::hex(0xFF9F0A)
                                    } else {
                                        Color::hex(0x34C759)
                                    };
                                    c.fill_circle(Point::new(cr.x + 14.0, cr.y + 16.0), 4.0, bar_col);
                                    let n_st = TextStyle { size: 12.0, ..TextStyle::caption().weight(Weight::Semibold).color(ink) };
                                    c.text(&row.name, Rect::new(cr.x + 24.0, cr.y + 8.0, 160.0, 16.0), &n_st);
                                    let p_st = TextStyle { size: 11.5, ..TextStyle::caption().weight(Weight::Bold).right().color(bar_col) };
                                    c.text(&format!("{}%", row.pct), Rect::new(cr.right() - 68.0, cr.y + 8.0, 56.0, 16.0), &p_st);
                                    let d_st = TextStyle { size: 10.5, ..TextStyle::caption().color(sub) };
                                    c.text(&row.detail, Rect::new(cr.x + 12.0, cr.y + 26.0, cr.w - 24.0, 14.0), &d_st);
                                    let tr = Rect::new(cr.x + 12.0, cr.y + 44.0, cr.w - 24.0, 6.0);
                                    c.fill_rrect(tr, 3.0, if light { Color::black(0.12) } else { Color::white(0.12) });
                                    let fw = (tr.w * (row.pct.min(100) as f32 / 100.0)).max(6.0);
                                    c.fill_rrect(Rect::new(tr.x, tr.y, fw.min(tr.w), tr.h), 3.0, bar_col);
                                    ry += 66.0;
                                }
                            }
                        }
                        CcView::Tray => {
                            let n = tray_icons.len();
                            let hot_tip = tray_hot
                                .and_then(|idx| tray_icons.get(idx))
                                .and_then(|ic| {
                                    ic.tip
                                        .lines()
                                        .next()
                                        .map(str::trim)
                                        .filter(|t| !t.is_empty())
                                        .map(str::to_string)
                                        .or_else(|| (!ic.name.is_empty()).then(|| ic.name.clone()))
                                });
                            let hdr_title = hot_tip.unwrap_or_else(|| "System Tray · Background Apps".to_string());
                            let hdr_st = TextStyle {
                                size: 12.5,
                                ..TextStyle::caption().weight(Weight::Semibold).color(ink)
                            };
                            c.text(
                                &hdr_title,
                                Rect::new(FLYOUT_PAD, 58.0, card.w - 2.0 * FLYOUT_PAD - 44.0, 18.0),
                                &hdr_st,
                            );
                            if n > 0 {
                                let cnt_st = TextStyle {
                                    size: 11.5,
                                    ..TextStyle::caption().weight(Weight::Bold).right().color(accent)
                                };
                                c.text(
                                    &format!("{n}"),
                                    Rect::new(card.w - FLYOUT_PAD - 40.0, 58.0, 40.0, 18.0),
                                    &cnt_st,
                                );
                            }
                            if n == 0 {
                                let empty_r = Rect::new(FLYOUT_PAD, 92.0, card.w - 2.0 * FLYOUT_PAD, 140.0);
                                c.fill_rrect(empty_r, 14.0, if light { Color::black(0.05) } else { Color::white(0.06) });
                                let msg = if !tray_hosted {
                                    "The notification area could not be hosted."
                                } else if tray_elapsed < 1.4 {
                                    "Loading background apps…"
                                } else {
                                    "Nothing running in the background."
                                };
                                let m_st = TextStyle {
                                    size: 12.5,
                                    ..TextStyle::caption().weight(Weight::Semibold).center().color(ink)
                                };
                                let s_st = TextStyle {
                                    size: 11.0,
                                    ..TextStyle::caption().center().color(sub)
                                };
                                c.text(msg, Rect::new(empty_r.x + 12.0, empty_r.y + 44.0, empty_r.w - 24.0, 20.0), &m_st);
                                c.text(
                                    "Left-click an icon to open, or right-click for its tray menu.",
                                    Rect::new(empty_r.x + 16.0, empty_r.y + 68.0, empty_r.w - 32.0, 28.0),
                                    &s_st,
                                );
                            } else {
                                for (idx, ic) in tray_icons.iter().take(CC_TRAY_MAX).enumerate() {
                                    let cr = cc_tray_cell_rect(card, idx);
                                    let is_h = hovered == Some(FlyoutZone::TrayCell(idx));
                                    let cell_bg = if is_h {
                                        accent.with_alpha(if light { 0.18 } else { 0.28 })
                                    } else if light {
                                        Color::black(0.05)
                                    } else {
                                        Color::white(0.07)
                                    };
                                    let cell_rim = if is_h {
                                        accent.with_alpha(0.55)
                                    } else if light {
                                        Color::black(0.08)
                                    } else {
                                        Color::white(0.09)
                                    };
                                    c.fill_rrect(cr, 11.0, cell_bg);
                                    c.stroke_rrect(cr, 11.0, cell_rim, 1.0);
                                    if let Some(img) = ic.image.as_ref() {
                                        let sz = if is_h { 24.0 } else { 22.0 };
                                        let ir = Rect::new(
                                            cr.x + (cr.w - sz) * 0.5,
                                            cr.y + (cr.h - sz) * 0.5,
                                            sz,
                                            sz,
                                        );
                                        c.image(img, ir, 4.0);
                                    }
                                }
                            }
                        }
                        CcView::Main => {}
                    }

                    let actions = match view {
                        CcView::Wifi => [("Available Networks…", FlyoutZone::SubAction1), ("Network & Internet Settings…", FlyoutZone::SubAction2)],
                        CcView::Battery => [("Power & Battery Settings…", FlyoutZone::SubAction1), ("Open System Monitor →", FlyoutZone::SubAction2)],
                        CcView::Calendar => [("Notifications Center", FlyoutZone::SubAction1), ("Date & Time Settings…", FlyoutZone::SubAction2)],
                        CcView::Ai => [("Open AI Usage Tab →", FlyoutZone::SubAction1), ("Configure Providers…", FlyoutZone::SubAction2)],
                        CcView::Tray => [("Refresh Tray Icons", FlyoutZone::SubAction1), ("Taskbar & Notification Settings…", FlyoutZone::SubAction2)],
                        CcView::Main => [("", FlyoutZone::SubAction1), ("", FlyoutZone::SubAction2)],
                    };
                    for (idx, (lbl, z)) in actions.into_iter().enumerate() {
                        let ar = cc_sub_action_rect(card, idx);
                        let is_h = hovered == Some(z);
                        c.fill_rrect(
                            ar,
                            8.0,
                            if is_h {
                                accent.with_alpha(if light { 0.16 } else { 0.24 })
                            } else if light {
                                Color::black(0.05)
                            } else {
                                Color::white(0.07)
                            },
                        );
                        let a_st = TextStyle {
                            size: 11.5,
                            ..TextStyle::caption().weight(Weight::Medium).color(if is_h { accent } else { ink })
                        };
                        c.text(lbl, Rect::new(ar.x + 10.0, ar.y + 6.0, ar.w - 20.0, 16.0), &a_st);
                    }
                    return;
                }

                // 1. 2x2 Quick Tiles Grid (.cc-grid: Wi-Fi, Bluetooth, Focus Mode, Screen Snip)
                let bt_sub = if live.bt_name.is_empty() {
                    "Not connected".to_string()
                } else if let Some(b) = live.bt_battery {
                    format!("{} · {b}%", live.bt_name)
                } else {
                    live.bt_name.clone()
                };
                let bt_on = !live.bt_name.is_empty();
                let tiles: [(FlyoutZone, Icon, &str, &str, Option<Color>); 4] = [
                    (
                        FlyoutZone::TileWifi,
                        if wifi_on { Icon::Wifi } else { Icon::WifiOff },
                        "Wi-Fi",
                        wifi_sub,
                        if wifi_on { Some(Color::hex(0x2563EB)) } else { None },
                    ),
                    (
                        FlyoutZone::TileBluetooth,
                        Icon::Bluetooth,
                        "Bluetooth",
                        &bt_sub,
                        if bt_on { Some(Color::hex(0x2563EB)) } else { None },
                    ),
                    (
                        FlyoutZone::TileFocus,
                        Icon::Moon,
                        "Focus Mode",
                        if tray.focus_active { "Active" } else { "Off" },
                        if tray.focus_active { Some(Color::hex(0xF5A524)) } else { None },
                    ),
                    (
                        FlyoutZone::TileSnip,
                        Icon::Area,
                        "Screen Snip",
                        "Win + Shift + S",
                        None,
                    ),
                ];

                for (i, (zone, icon, lbl, sub_txt, active_col)) in tiles.into_iter().enumerate() {
                    let tr = flyout_tile_rect(card, i);
                    let is_h = hovered == Some(zone);
                    let tile_r = 14.0;
                    if let Some(ac) = active_col {
                        c.fill_rrect(tr.offset(0.0, 2.0), tile_r, ac.with_alpha(0.32));
                        c.fill_rrect(tr, tile_r, ac.with_alpha(if is_h { 0.96 } else { 0.90 }));
                        let t_segs = crate::pill::shape::segments(tr, tile_r);
                        c.fill_path_fade(&t_segs, Color::white(1.0), tr.y, tr.y + tr.h * 0.55, 0.24, 0.0);
                        c.stroke_rrect(tr, tile_r, Color::white(0.28), 1.0);
                    } else {
                        let bg = if is_h {
                            if light { Color::black(0.10) } else { Color::white(0.13) }
                        } else if light {
                            Color::black(0.05)
                        } else {
                            Color::white(0.07)
                        };
                        let b_rim = if light { Color::black(0.08) } else { Color::white(0.09) };
                        c.fill_rrect(tr, tile_r, bg);
                        c.stroke_rrect(tr, tile_r, b_rim, 1.0);
                    }

                    // Circle icon badge (.cc-cir)
                    let cir_c = Point::new(tr.x + 24.0, tr.y + tr.h * 0.5);
                    let cir_bg = if active_col.is_some() {
                        Color::white(0.20)
                    } else if light {
                        Color::black(0.08)
                    } else {
                        Color::white(0.14)
                    };
                    c.fill_circle(cir_c, 15.0, cir_bg);
                    let ic_col = if active_col.is_some() { Color::white(1.0) } else { ink };
                    c.icon(icon, Rect::new(cir_c.x - 7.5, cir_c.y - 7.5, 15.0, 15.0), ic_col);

                    let lbl_col = if active_col.is_some() { Color::white(1.0) } else { ink };
                    let sub_col = if active_col.is_some() { Color::white(0.82) } else { sub };
                    let lbl_st = TextStyle { size: 12.0, ..TextStyle::caption().weight(Weight::Semibold).color(lbl_col) };
                    let sub_st = TextStyle { size: 10.5, ..TextStyle::caption().color(sub_col) };
                    let tx = tr.x + 48.0;
                    let tw = (tr.right() - tx - 8.0).max(24.0);
                    c.text(lbl, Rect::new(tx, tr.y + 9.0, tw, 15.0), &lbl_st);
                    c.text(sub_txt, Rect::new(tx, tr.y + 25.0, tw, 14.0), &sub_st);
                }

                // 2. Available Networks Box (.cc-net-box)
                let nb = flyout_net_box_rect(card);
                let nb_hov = hovered == Some(FlyoutZone::NetworkBox);
                let nb_bg = if light {
                    Color::black(if nb_hov { 0.08 } else { 0.05 })
                } else {
                    Color::rgba(0.0, 0.0, 0.0, if nb_hov { 0.34 } else { 0.26 })
                };
                c.fill_rrect(nb, 14.0, nb_bg);
                c.stroke_rrect(nb, 14.0, if light { Color::black(0.08) } else { Color::white(0.09) }, 1.0);

                let hdr_st = TextStyle { size: 9.5, ..TextStyle::caption().weight(Weight::Semibold).color(sub.with_alpha(0.54)) };
                let hdr_r_st = TextStyle { size: 9.5, ..TextStyle::caption().weight(Weight::Semibold).right().color(sub.with_alpha(0.54)) };
                c.text("AVAILABLE NETWORKS", Rect::new(nb.x + 11.0, nb.y + 7.0, 150.0, 13.0), &hdr_st);
                c.text(
                    if wifi_on { "Wi-Fi" } else { "Offline" },
                    Rect::new(nb.right() - 135.0, nb.y + 7.0, 124.0, 13.0),
                    &hdr_r_st,
                );

                // Row 1: Connected primary network + green pill badge
                let row1_st = TextStyle { size: 11.5, ..TextStyle::caption().weight(Weight::Semibold).color(ink) };
                let row1_txt = if !wifi_on {
                    "No active Wi-Fi".to_string()
                } else if live.ssid.is_empty() {
                    "✓ Connected".to_string()
                } else {
                    format!("✓ {}", live.ssid)
                };
                c.text(&row1_txt, Rect::new(nb.x + 11.0, nb.y + 26.0, 170.0, 16.0), &row1_st);
                let badge = Rect::new(nb.right() - 78.0, nb.y + 25.0, 67.0, 17.0);
                let green = Color::hex(0x4ADE80);
                c.fill_rrect(badge, 8.5, green.with_alpha(0.16));
                let b_st = TextStyle { size: 9.5, ..TextStyle::caption().weight(Weight::Semibold).center().color(green) };
                c.text(if wifi_on { "Connected" } else { "Settings" }, badge, &b_st);

                // Row 2: live internet reachability (Windows connection profile)
                let row2_st = TextStyle { size: 11.0, ..TextStyle::caption().color(sub) };
                c.text("Internet", Rect::new(nb.x + 11.0, nb.y + 46.0, 150.0, 15.0), &row2_st);
                let sec_st = TextStyle { size: 10.0, ..TextStyle::caption().right().color(sub.with_alpha(0.58)) };
                let net_txt = match (wifi_on, live.internet) {
                    (false, _) => "Offline",
                    (true, Some(true)) => "Online",
                    (true, Some(false)) => "No internet",
                    (true, None) => "Checking…",
                };
                c.text(net_txt, Rect::new(nb.right() - 98.0, nb.y + 46.0, 87.0, 15.0), &sec_st);

                // 3. Interactive Display Brightness Slider Card (.cc-bright-card)
                let bc = flyout_brightness_card_rect(card);
                let card_bg = if light { Color::black(0.05) } else { Color::white(0.06) };
                let card_rim = if light { Color::black(0.08) } else { Color::white(0.09) };
                c.fill_rrect(bc, 14.0, card_bg);
                c.stroke_rrect(bc, 14.0, card_rim, 1.0);
                c.icon(Icon::Sun, Rect::new(bc.x + 12.0, bc.y + 8.0, 14.0, 14.0), ink);
                let sl_lbl_st = TextStyle { size: 11.5, ..TextStyle::caption().weight(Weight::Semibold).color(ink) };
                let sl_val_st = TextStyle { size: 11.5, ..TextStyle::caption().weight(Weight::Semibold).right().color(ink) };
                c.text("Display · Brightness", Rect::new(bc.x + 32.0, bc.y + 7.0, 175.0, 16.0), &sl_lbl_st);
                c.text(&format!("{bright_pct}%"), Rect::new(bc.right() - 72.0, bc.y + 7.0, 60.0, 16.0), &sl_val_st);

                let b_track = flyout_brightness_track_rect(card);
                let b_r = b_track.h * 0.5;
                c.fill_rrect(b_track, b_r, if light { Color::black(0.14) } else { Color::rgba(0.0, 0.0, 0.0, 0.38) });
                c.stroke_rrect(b_track, b_r, if light { Color::black(0.10) } else { Color::white(0.11) }, 1.0);
                let b_frac = (bright_pct as f32 / 100.0).clamp(0.0, 1.0);
                let b_fill_w = (b_track.w * b_frac).max(b_track.h);
                let b_fill_r = Rect::new(b_track.x, b_track.y, b_fill_w.min(b_track.w), b_track.h);
                let fill_col = if light { accent } else { Color::white(0.95) };
                c.fill_rrect(b_fill_r, b_r, fill_col);
                let b_knob_cx = (b_fill_r.right() - b_r).max(b_track.x + b_r);
                c.fill_circle(
                    Point::new(b_knob_cx, b_track.y + b_r),
                    6.5,
                    if light { Color::white(0.92) } else { Color::hex(0x0F172A).with_alpha(0.28) },
                );

                // 4. Interactive Sound Master Volume Slider Card (.cc-slider-card)
                let sc = flyout_slider_card_rect(card);
                c.fill_rrect(sc, 14.0, card_bg);
                c.stroke_rrect(sc, 14.0, card_rim, 1.0);

                let vol_pct = if tray.muted { 0 } else if tray.volume_known { tray.volume_pct.min(100) } else { 64 };
                let vol_ic = if tray.muted || vol_pct == 0 {
                    Icon::VolumeMute
                } else if vol_pct < 45 {
                    Icon::VolumeLow
                } else {
                    Icon::Volume
                };
                c.icon(vol_ic, Rect::new(sc.x + 12.0, sc.y + 8.0, 14.0, 14.0), ink);
                c.text("Sound · Master Audio", Rect::new(sc.x + 32.0, sc.y + 7.0, 175.0, 16.0), &sl_lbl_st);
                let val_txt = if tray.muted { "Muted".to_string() } else { format!("{vol_pct}%") };
                c.text(&val_txt, Rect::new(sc.right() - 72.0, sc.y + 7.0, 60.0, 16.0), &sl_val_st);

                let track = flyout_slider_track_rect(card);
                let tr_r = track.h * 0.5;
                c.fill_rrect(track, tr_r, if light { Color::black(0.14) } else { Color::rgba(0.0, 0.0, 0.0, 0.38) });
                c.stroke_rrect(track, tr_r, if light { Color::black(0.10) } else { Color::white(0.11) }, 1.0);

                let frac = (vol_pct as f32 / 100.0).clamp(0.0, 1.0);
                let fill_w = (track.w * frac).max(track.h);
                let fill_r = Rect::new(track.x, track.y, fill_w.min(track.w), track.h);
                c.fill_rrect(fill_r, tr_r, fill_col);
                let knob_cx = (fill_r.right() - tr_r).max(track.x + tr_r);
                let knob_cy = track.y + tr_r;
                c.fill_circle(
                    Point::new(knob_cx, knob_cy),
                    6.5,
                    if light { Color::white(0.92) } else { Color::hex(0x0F172A).with_alpha(0.28) },
                );

                // 5. Sub-flyout Quick Pills (Wi-Fi, Battery, Calendar, AI, Tray ^)
                let sub_pills = [
                    (FlyoutZone::SubWifi, "Wi-Fi"),
                    (FlyoutZone::SubBattery, "Battery"),
                    (FlyoutZone::SubCalendar, "Calendar"),
                    (FlyoutZone::SubAi, "AI"),
                    (FlyoutZone::SubTray, "Tray ^"),
                ];
                for (idx, (z, lbl)) in sub_pills.into_iter().enumerate() {
                    let pr = flyout_sub_pill_rect(card, idx);
                    let is_h = hovered == Some(z);
                    let p_bg = if is_h {
                        accent.with_alpha(if light { 0.18 } else { 0.26 })
                    } else if light {
                        Color::black(0.05)
                    } else {
                        Color::white(0.07)
                    };
                    c.fill_rrect(pr, 8.0, p_bg);
                    c.stroke_rrect(pr, 8.0, if is_h { accent.with_alpha(0.50) } else { card_rim }, 1.0);
                    let p_st = TextStyle {
                        size: 10.5,
                        ..TextStyle::caption().weight(Weight::Semibold).center().color(if is_h { accent } else { ink })
                    };
                    c.text(lbl, Rect::new(pr.x, pr.y + 5.0, pr.w, 15.0), &p_st);
                }

                // 6. Footer Status Bar (.cc-foot: battery charging status + System page link)
                let ft = flyout_footer_rect(card);
                let bat_txt = if tray.has_battery {
                    let mode = if tray.charging || tray.on_ac { "AC Charging" } else { "Battery" };
                    format!("⚡ {}% {mode}", tray.battery_pct)
                } else {
                    "⚡ AC Power Connected".to_string()
                };
                let ft_l_st = TextStyle { size: 11.0, ..TextStyle::caption().weight(Weight::Semibold).color(Color::hex(0x4ADE80)) };
                c.text(&bat_txt, Rect::new(ft.x, ft.y + 2.0, ft.w * 0.68, ft.h), &ft_l_st);

                let sys_hov = hovered == Some(FlyoutZone::FooterSystem);
                let ft_r_st = TextStyle {
                    size: 11.0,
                    ..TextStyle::caption()
                        .weight(if sys_hov { Weight::Semibold } else { Weight::Medium })
                        .right()
                        .color(if sys_hov { ink } else { sub })
                };
                c.text("System →", Rect::new(ft.right() - 96.0, ft.y + 2.0, 96.0, ft.h), &ft_r_st);
            });
            let _ = gfx.end();
        }
        let _ = gfx.present(x, y);
        if gfx.needs_hit_mask() {
            let mask = HitMask { rect: None, notch: None, pill: Some((card, FLYOUT_R)) };
            let _ = gfx.push_hit_mask(x, y, &mask, None);
        }
        if !self.shown {
            self.shown = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            gfx.set_shown(true);
        }
        self.dirty = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(w: i32, h: i32, work_bottom: i32, dpi: u32) -> MonitorInfo {
        MonitorInfo {
            handle: 1,
            rect: RECT { left: 0, top: 0, right: w, bottom: h },
            work: RECT { left: 0, top: 0, right: w, bottom: work_bottom },
            dpi,
        }
    }

    #[test]
    fn surface_sits_above_docked_taskbar_and_at_edge_when_autohidden() {
        let docked = mon(1920, 1080, 1032, 96);
        let (x1, y1, w1, h1) = surface_rect(&docked);
        assert_eq!(y1 + h1, 1032);
        assert_eq!(x1 + w1 / 2, 960);

        let autohide = mon(1920, 1080, 1080, 96);
        let (_, y2, _, h2) = surface_rect(&autohide);
        assert_eq!(y2 + h2, 1080);
    }

    #[test]
    fn thirty_apps_fit_single_row_at_100_and_125_dpi() {
        for dpi in [96u32, 120u32] {
            let m = mon(1920, 1080, 1080, dpi);
            let (_, _, w, _) = surface_rect(&m);
            let surf_w = w as f32 / m.scale();
            let shelf = shelf_rect(surf_w, 30, 0.0);
            assert!(shelf.x >= 0.0 && shelf.right() <= surf_w, "dpi {dpi}: shelf {shelf:?} exceeds {surf_w}");
            let last = slot_rect(shelf, 29, 30, surf_w);
            assert!(last.right() <= shelf.right() - SHELF_PAD_X + 0.5, "dpi {dpi}: last slot clips {last:?} vs {shelf:?}");
        }
    }

    #[test]
    fn hit_mask_leaves_desktop_above_and_beside_shelf_click_through() {
        let surf_w = 1360.0;
        let shown = hit_mask_for(surf_w, 5, false, 0.0);
        let shelf = shelf_rect(surf_w, 5, 0.0);
        assert!(shown.contains(Point::new(surf_w * 0.5, shelf.y + shelf.h * 0.5)));
        assert!(!shown.contains(Point::new(10.0, shelf.y + 10.0)), "beside shelf must be click-through");
        assert!(!shown.contains(Point::new(surf_w * 0.5, 10.0)), "above shelf must be click-through");

        let tucked = hit_mask_for(surf_w, 5, true, 1.0);
        assert!(tucked.contains(Point::new(surf_w * 0.5, SURF_H - 2.0)));
        assert!(!tucked.contains(Point::new(surf_w * 0.5, SURF_H - 40.0)), "tucked shelf area must be click-through");
    }

    #[test]
    fn dock_window_paints_rests_at_zero_frames_hides_in_fullscreen_and_drops_cleanly() {
        unsafe extern "system" fn test_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        let _ = crate::window::register_classes(test_proc, test_proc);
        let _ = register(test_proc);

        let m = crate::window::pick_monitor(halo_core::settings::MonitorChoice::Primary);
        let mut dock = Dock::new(&m, false, None).expect("create dock");
        let hwnd = dock.hwnd;
        assert!(dock.needs_frame(), "initial frame must be dirty");

        let ex = unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 };
        let need = WS_EX_LAYERED.0 | WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_NOACTIVATE.0;
        assert_eq!(ex & need, need, "dock ex-style {ex:#x}");

        dock.paint(1.0, 1.0 / 60.0, true, None);
        assert!(!dock.needs_frame(), "settled dock must request 0 frames at rest");
        assert!(unsafe { IsWindowVisible(hwnd).as_bool() });

        dock.set_tucked(true, true);
        assert!(dock.needs_frame());
        dock.paint(1.1, 1.0 / 60.0, true, None);
        assert!(!dock.needs_frame(), "tucked dock must request 0 frames at rest");

        dock.set_fullscreen_hidden(true);
        assert!(!dock.needs_frame());
        assert!(!unsafe { IsWindowVisible(hwnd).as_bool() }, "fullscreen must hide dock");

        dock.set_fullscreen_hidden(false);
        assert!(unsafe { IsWindowVisible(hwnd).as_bool() }, "exiting fullscreen must show dock");

        drop(dock);
        assert!(!unsafe { IsWindow(hwnd).as_bool() }, "dropping dock must destroy HWND");
    }

    #[test]
    fn parabolic_magnification_is_symmetric_peaked_and_bounded() {
        let step = SLOT_MAX + SLOT_GAP;
        assert!((mag_scale(0.0, step) - MAX_MAG).abs() < 1e-4);
        assert!((mag_scale(step, step) - mag_scale(-step, step)).abs() < 1e-5);
        assert!(mag_scale(0.0, step) > mag_scale(step, step));
        assert!(mag_scale(step, step) > mag_scale(2.0 * step, step));
        assert_eq!(mag_scale(MAG_RADIUS_SLOTS * step + 1.0, step), 1.0);
    }

    #[test]
    fn magnified_layout_stays_within_surface_at_thirty_items() {
        for dpi in [96u32, 120u32] {
            let m = mon(1920, 1080, 1080, dpi);
            let (_, _, w, _) = surface_rect(&m);
            let surf_w = w as f32 / m.scale();
            let mut scales = vec![1.0f32; 30];
            scales[15] = MAX_MAG;
            scales[14] = 1.28;
            scales[16] = 1.28;
            let (shelf, sep_x, rects) = layout_slots(surf_w, 5, 25, &scales, 0.0);
            assert!(sep_x.is_some());
            assert!(shelf.x >= 0.0 && shelf.right() <= surf_w, "dpi {dpi}: shelf {shelf:?} > {surf_w}");
            assert_eq!(rects.len(), 30);
            assert!(rects[15].y < rects[0].y, "magnified slot must grow upward");
            assert!(rects[29].right() <= shelf.right() - SHELF_PAD_X + 0.5);
        }
    }

    #[test]
    fn stack_layout_adds_second_divider_and_formats_sizes() {
        let scales = vec![1.0f32; 9];
        let (shelf, sep1, sep2, rects) = layout_slots_with_stack(1360.0, 5, 3, 1, &scales, 0.0);
        assert!(sep1.is_some());
        assert!(sep2.is_some());
        assert!(sep2.unwrap() > sep1.unwrap());
        assert_eq!(rects.len(), 9);
        assert!(rects[8].right() <= shelf.right() - SHELF_PAD_X + 0.5);

        assert_eq!(format_file_size(512), "512 B");
        assert_eq!(format_file_size(2048), "2 KB");
        assert_eq!(format_file_size(3 * 1024 * 1024), "3.0 MB");

        let m = mon(1920, 1080, 1032, 96);
        let anchor = Rect::new(1100.0, 960.0, 44.0, 44.0);
        let (sx, sy, sw, sh) = stack_surface_rect(&m, anchor, 4);
        assert!(sx >= m.work.left && sx + sw <= m.work.right);
        assert!(sy + sh < anchor.y as i32);
    }

    #[test]
    fn pro_gating_and_tray_capsule_layout_and_clock() {
        assert!(!dock_allowed(false, true, false));
        assert!(!dock_allowed(true, false, false));
        assert!(!dock_allowed(true, true, true));
        assert!(dock_allowed(true, true, false));

        let t24 = DockTrayState { hour: 13, minute: 5, use_24h: true, has_battery: true, battery_pct: 84, ..Default::default() };
        assert_eq!(t24.clock_text(), "13:05");
        let t12 = DockTrayState { hour: 0, minute: 9, use_24h: false, ..t24 };
        assert_eq!(t12.clock_text(), "12:09");

        let scales = vec![1.0f32; 9];
        let tw = tray_capsule_width(&t24);
        let (shelf, sep1, sep2, sep3, rects, tray_r) = layout_slots_full(1360.0, 5, 3, 1, tw, &scales, 0.0);
        assert!(sep1.is_some() && sep2.is_some() && sep3.is_some());
        let tr = tray_r.expect("tray rect present");
        assert!(tr.x > sep3.unwrap());
        assert!(tr.right() <= shelf.right() - SHELF_PAD_X + 0.5);
        assert!(rects.last().unwrap().right() < sep3.unwrap());
        let (_, status_r, clock_r) = tray_zone_rects(tr, &t24);
        assert!(status_r.right() < clock_r.x);
        assert!(clock_r.right() <= tr.right() + 0.5);
        let dated = DockTrayState { weekday: 3, day: 25, month: 11, weather_temp_c: Some(31), weather_code: Some(0), ..t24 };
        assert_eq!(dated.date_text().as_deref(), Some("Wed, 25 Nov"));
        assert_eq!(dated.weather_short(), "Clear");
        assert!(tray_capsule_width(&dated) > tray_capsule_width(&t24));
    }

    #[test]
    fn dock_theme_sync_and_liquid_glass_params_match_island_themes() {
        assert_eq!(resolve_dock_look(NotchTheme::Glass, NotchType::Notch, false, false, None), &look::GLASS_DARK);
        assert_eq!(resolve_dock_look(NotchTheme::Glass, NotchType::Notch, true, false, None), &look::GLASS_LIGHT);
        assert_eq!(resolve_dock_look(NotchTheme::Halo, NotchType::Notch, false, false, None), &look::HALO);
        assert_eq!(resolve_dock_look(NotchTheme::Diorama, NotchType::Pill, false, false, None), &look::DIORAMA_DAY);
        assert_eq!(resolve_dock_look(NotchTheme::Diorama, NotchType::Pill, false, true, None), &look::DIORAMA_NIGHT);
        assert_eq!(resolve_dock_look(NotchTheme::Classic, NotchType::Pill, false, false, None), &look::PILL);

        let shelf = Rect::new(120.0, 70.0, 640.0, SHELF_H);
        let gp_dark = dock_glass_params(shelf, SHELF_R, &look::GLASS_DARK, 1.25);
        assert!(gp_dark.shoulder < 0.0, "dock glass must use floating capsule SDF (shoulder < 0)");
        assert_eq!(gp_dark.light_mix, 0.0);
        assert_eq!(gp_dark.ground_dark, look::GLASS_DARK.ground);
        assert_eq!(gp_dark.rim, 0.80);
        assert_eq!(gp_dark.glow, 0.09);
        assert_eq!(gp_dark.light_dir, (-0.6, -0.8));

        let gp_halo = dock_glass_params(shelf, SHELF_R, &look::HALO, 1.0);
        assert_eq!(gp_halo.light_mix, 1.0);
        assert_eq!(gp_halo.ground_light, look::HALO.ground);
        assert!((gp_halo.rim - 0.95).abs() < 1e-4);

        // Phase E: Glass Material variations and badge modes
        let gp_smoked = dock_glass_params_with_material(shelf, SHELF_R, &look::GLASS_DARK, 1.0, DockGlassMaterial::SmokedDark);
        assert_eq!(gp_smoked.tint_dark, 0.62);
        assert_eq!(gp_smoked.ground_dark, Color::hex(0x05070A));
        assert_eq!(gp_smoked.rim, 0.92);

        let gp_crystal = dock_glass_params_with_material(shelf, SHELF_R, &look::GLASS_LIGHT, 1.0, DockGlassMaterial::CrystalLight);
        assert_eq!(gp_crystal.tint_light, 0.24);
        assert_eq!(gp_crystal.ground_light, Color::hex(0xF8FAFC));
        assert_eq!(gp_crystal.dispersion, 0.055);

        let gp_matte = dock_glass_params_with_material(shelf, SHELF_R, &look::GLASS_DARK, 1.0, DockGlassMaterial::FrostedMatte);
        assert_eq!(gp_matte.glow, 0.06);
        assert_eq!(gp_matte.saturation, 1.20);

        // Parabolic launch/click bounce curve: 0 at rest/end, negative upward lift during hops, decaying amplitude.
        assert_eq!(bounce_offset_y(0.0), 0.0);
        assert_eq!(bounce_offset_y(1.0), 0.0);
        let hop1 = bounce_offset_y(0.25);
        let hop2 = bounce_offset_y(0.75);
        assert!(hop1 < -10.0, "first bounce hop must lift icon upward: {hop1}");
        assert!(hop2 < -3.0 && hop2 > hop1, "second bounce hop must decay: {hop2} vs {hop1}");
    }

    #[test]
    fn launch_bounce_tracking_and_auto_settle() {
        if unsafe { GetModuleHandleW(PCWSTR::null()) }.is_err() {
            return;
        }
        unsafe extern "system" fn test_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        let _ = crate::window::register_classes(test_proc, test_proc);
        let _ = register(test_proc);
        let m = crate::window::pick_monitor(halo_core::settings::MonitorChoice::Primary);
        let mut dock = Dock::new(&m, false, None).expect("open dock");
        let sample = vec![
            RunningApp {
                key: "calc".into(),
                name: "Calculator".into(),
                exe: Some("calc.exe".into()),
                launch: Some("calc.exe".into()),
                running: false,
                ..Default::default()
            },
        ];
        dock.set_apps(sample.clone());
        let calc_idx = ANCHOR_SLOTS.len();
        assert_eq!(dock.find_slot_by_spec("calc.exe"), Some(calc_idx));

        // Trigger launch
        dock.trigger_launch("calc.exe", false);
        assert!(dock.needs_frame());
        assert!(dock.bounces[calc_idx] > 0.0);

        // App becomes running in next refresh -> launch bounces stop
        let mut running_sample = sample;
        running_sample[0].running = true;
        dock.set_apps(running_sample);
        assert!(dock.launching.is_empty(), "launching map must clear once app runs");
    }

    #[test]
    fn launchpad_slot_badges_and_progress_ring_geometry() {
        assert_eq!(ANCHOR_SLOTS[0].label, "Launchpad & Search");
        assert!(ANCHOR_SLOTS[0].action == SlotAction::Launchpad);
        assert!(ANCHOR_SLOTS.len() == 6);

        assert!(is_browser_key("chrome"));
        assert!(is_browser_key("MSEdge"));
        assert!(!is_browser_key("notepad"));

        let tile = Rect::new(200.0, 80.0, 44.0, 44.0);
        let b1 = badge_rect(tile, 3);
        let b2 = badge_rect(tile, 42);
        let b3 = badge_rect(tile, 120);
        assert!(b1.w == b1.h && b2.w > b1.w && b3.w > b2.w);
        assert!(b1.right() > tile.right() && b1.y < tile.y);
        assert!(progress_ring_radius(tile) > tile.w * 0.5);
    }

    #[test]
    fn fan_stack_cards_and_minimized_window_thumbnail_geometry() {
        let card = Rect::new(0.0, 0.0, STACK_W, 260.0);
        let top_row = stack_fan_card_rect(card, 0, 4);
        let bot_row = stack_fan_card_rect(card, 3, 4);
        assert!(top_row.x > bot_row.x, "top fan card must cascade inward relative to bottom anchor card");
        assert!(top_row.y < bot_row.y);
        assert!(bot_row.right() <= card.w - STACK_PAD + 0.5);

        let exe_item = DownloadItem {
            name: "Halo-Island-Setup.exe".into(),
            path: std::path::PathBuf::from("Halo-Island-Setup.exe"),
            is_dir: false,
            size: 4_200_000,
        };
        let (_, tag) = download_file_badge(&exe_item);
        assert_eq!(tag, "APP");

        let tile = Rect::new(300.0, 74.0, 44.0, 44.0);
        let (min_card, min_bar, min_badge) = minimized_card_rects(tile);
        assert!(min_card.w < tile.w && min_card.h < tile.h);
        assert_eq!(min_bar.y, min_card.y);
        assert!(min_badge.right() >= tile.right() && min_badge.bottom() >= tile.bottom());
    }

    #[test]
    fn control_center_flyout_layout_and_bottom_most_taskbar_auto_hide() {
        let m = mon(1920, 1080, 1032, 96);
        // When bottom_most is enabled (taskbar hidden), dock anchors flush to mon.rect.bottom (1080), not work.bottom (1032).
        let (_, y_docked, _, h_docked) = surface_rect_for(&m, false);
        assert_eq!(y_docked + h_docked, 1032);
        let (_, y_bottom, _, h_bottom) = surface_rect_for(&m, true);
        assert_eq!(y_bottom + h_bottom, 1080);

        set_system_taskbar_hidden(true);
        assert!(is_system_taskbar_hidden());
        set_system_taskbar_hidden(false);
        assert!(!is_system_taskbar_hidden());

        let anchor = Rect::new(1310.0, 1018.0, 120.0, 38.0);
        let (fx, fy, fw, fh) = flyout_surface_rect(&m, anchor);
        assert_eq!(fw, FLYOUT_W as i32);
        assert_eq!(fh, FLYOUT_H as i32);
        assert!(fx >= m.rect.left && fx + fw <= m.rect.right);
        assert!(fy + fh < anchor.y as i32);

        let card = Rect::new(0.0, 0.0, FLYOUT_W, FLYOUT_H);
        let t0 = flyout_tile_rect(card, 0);
        let t1 = flyout_tile_rect(card, 1);
        let t2 = flyout_tile_rect(card, 2);
        let t3 = flyout_tile_rect(card, 3);
        assert!(t1.x > t0.right() && t2.y > t0.bottom() && t3.x == t1.x && t3.y == t2.y);

        let nb = flyout_net_box_rect(card);
        let bc = flyout_brightness_card_rect(card);
        let bt = flyout_brightness_track_rect(card);
        let sc = flyout_slider_card_rect(card);
        let tr = flyout_slider_track_rect(card);
        let p0 = flyout_sub_pill_rect(card, 0);
        let p3 = flyout_sub_pill_rect(card, 3);
        let ft = flyout_footer_rect(card);
        assert!(nb.y > t2.bottom());
        assert!(bc.y > nb.bottom());
        assert!(bt.x > bc.x && bt.right() < bc.right());
        assert!(sc.y > bc.bottom());
        assert!(tr.x > sc.x && tr.right() < sc.right());
        assert!(p0.y > sc.bottom() && p3.right() <= card.w - FLYOUT_PAD + 0.5);
        assert!(ft.y > p0.bottom() && ft.bottom() <= card.h);

        // Calendar month stepping wraps December -> January and January -> December cleanly.
        assert_eq!(cc_days_in_month(2024, 2), 29);
        assert_eq!(cc_days_in_month(2026, 2), 28);
        assert_eq!(cc_days_in_month(2026, 11), 30);
    }

    #[test]
    fn pinned_app_aumid_squirrel_and_profile_matching() {
        use crate::services::dock_apps::{extract_profile_directory, launcher_of, match_score};
        assert_eq!(
            extract_profile_directory(r#"--profile-directory="Profile 1" --no-first-run"#).as_deref(),
            Some("Profile 1")
        );
        assert!(launcher_of(
            r"C:\Users\me\AppData\Local\Discord\Update.exe",
            r"C:\Users\me\AppData\Local\Discord\app-1.0.9015\Discord.exe"
        ));
        assert!(!launcher_of(
            r"C:\Users\me\AppData\Local\Discord\Update.exe",
            r"C:\Program Files\Google\Chrome\Application\chrome.exe"
        ));
        // Window AUMID exact match (score 3) beats process AUMID match (score 2) and exe match (score 1).
        assert_eq!(
            match_score(
                None,
                Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
                None,
                Some(r"C:\Program Files\WindowsApps\WindowsTerminal.exe"),
                Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
                Some("Microsoft.WindowsTerminal_8wekyb3d8bbwe!App"),
                None,
            ),
            Some(3)
        );
        assert_eq!(
            match_score(
                Some(r"C:\Users\me\AppData\Local\Discord\Update.exe"),
                None,
                None,
                Some(r"C:\Users\me\AppData\Local\Discord\app-1.0.9015\Discord.exe"),
                None,
                None,
                None,
            ),
            Some(1)
        );
    }

    #[test]
    fn title_badges_trash_slot_and_drag_reorder_work() {
        use crate::services::dock_apps::badge_from_title;
        assert_eq!(badge_from_title("(12) Inbox - Gmail"), 12);
        assert_eq!(badge_from_title("(99+) WhatsApp"), 99);
        assert_eq!(badge_from_title("[3] Slack - General"), 3);
        assert_eq!(badge_from_title(" Halo Island"), 0);
        assert_eq!(badge_from_title("(not a number) Title"), 0);

        // Trash slot is always included after Downloads stack (stack_count = 2 when show_downloads = true, 1 when false).
        let scales = vec![1.0f32; 11];
        let (shelf, sep1, sep2, rects) = layout_slots_with_stack(1360.0, 6, 3, 2, &scales, 0.0);
        assert!(sep1.is_some() && sep2.is_some());
        assert_eq!(rects.len(), 11);
        assert!(rects[10].x > rects[9].right(), "Trash slot must sit to the right of Downloads stack");
        assert!(rects[10].right() <= shelf.right() - SHELF_PAD_X + 0.5);
    }

    #[test]
    fn launchpad_search_ranking_junk_filter_and_paging_work() {
        use crate::services::dock_apps::{is_launchpad_junk, launchpad_rank, LaunchpadApp};

        assert!(is_launchpad_junk("Uninstall Zoom", "Zoom.Uninstall"));
        assert!(is_launchpad_junk("Release Notes", "app.html"));
        assert!(!is_launchpad_junk("Visual Studio Code", "Microsoft.VisualStudioCode"));

        // Prefix match (0) outranks word-boundary match (1) and substring match (2).
        assert_eq!(launchpad_rank("code", "co"), Some(0));
        assert_eq!(launchpad_rank("visual studio code", "co"), Some(1));
        assert_eq!(launchpad_rank("discord", "co"), Some(2));
        assert_eq!(launchpad_rank("notepad", "co"), None);

        unsafe extern "system" fn test_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
        }
        let _ = register_launchpad(test_proc);
        let m = crate::window::pick_monitor(halo_core::settings::MonitorChoice::Primary);
        let mut lp = DockLaunchpad::open(&m, Vec::new(), &halo_core::look::CLASSIC, None).expect("open launchpad");
        let sample: Vec<LaunchpadApp> = (0..40)
            .map(|i| LaunchpadApp {
                name: format!("App {i:02}"),
                spec: format!("shell:AppsFolder\\App.{i:02}"),
                icon: None,
            })
            .collect();
        lp.set_apps_for_test(sample);
        assert_eq!(lp.pages(), 2);
        assert_eq!(lp.page(), 0);
        lp.step_page(1);
        assert_eq!(lp.page(), 1);
        lp.set_query("App 1".into());
        assert_eq!(lp.shown_len(), 10);
        assert_eq!(lp.pages(), 1);
    }

    #[test]
    fn tray_host_notify_protocol_and_grid_layout_work() {
        use crate::services::tray_host::{on_identify_slice, on_notify_slice, TrayIcon};

        let mut icons: Vec<TrayIcon> = Vec::new();
        // Build a synthetic SHELLTRAYDATA (8-byte header + NOTIFYICONDATA32) for NIM_ADD (msg = 0).
        let mut buf = vec![0u8; 960];
        buf[4..8].copy_from_slice(&0u32.to_le_bytes()); // NIM_ADD
        let n = &mut buf[8..];
        n[4..8].copy_from_slice(&0x1234u32.to_le_bytes()); // hwnd
        n[8..12].copy_from_slice(&7u32.to_le_bytes()); // uid
        // flags: NIF_MESSAGE (1) | NIF_TIP (4) | NIF_STATE (8)
        n[12..16].copy_from_slice(&(1u32 | 4 | 8).to_le_bytes());
        n[16..20].copy_from_slice(&0x8001u32.to_le_bytes()); // callback
        for (i, u) in "Discord\0".encode_utf16().enumerate() {
            let off = 24 + i * 2;
            n[off..off + 2].copy_from_slice(&u.to_le_bytes());
        }
        assert!(on_notify_slice(&mut icons, &buf, false));
        assert_eq!(icons.len(), 1);
        assert_eq!(icons[0].hwnd, 0x1234);
        assert_eq!(icons[0].uid, 7);
        assert_eq!(icons[0].callback, 0x8001);
        assert_eq!(icons[0].tip, "Discord");
        assert!(!icons[0].hidden);

        // NIM_SETVERSION (msg = 4) -> version 4 at offset 800
        buf[4..8].copy_from_slice(&4u32.to_le_bytes());
        buf[8 + 800..8 + 804].copy_from_slice(&4u32.to_le_bytes());
        assert!(on_notify_slice(&mut icons, &buf, false));
        assert_eq!(icons[0].version, 4);

        // Shell_NotifyIconGetRect (dwData = 3) returns packed screen rect coordinates.
        icons[0].place = RECT { left: 100, top: 200, right: 148, bottom: 246 };
        let mut id_buf = vec![0u8; 48];
        id_buf[4..8].copy_from_slice(&1u32.to_le_bytes()); // msg 1 -> (left, top)
        id_buf[16..20].copy_from_slice(&0x1234u32.to_le_bytes());
        id_buf[20..24].copy_from_slice(&7u32.to_le_bytes());
        let packed_lt = on_identify_slice(&icons, &id_buf);
        assert_eq!(packed_lt & 0xFFFF, 100);
        assert_eq!((packed_lt >> 16) & 0xFFFF, 200);

        // NIM_DELETE (msg = 2) removes the icon.
        buf[4..8].copy_from_slice(&2u32.to_le_bytes());
        assert!(on_notify_slice(&mut icons, &buf, false));
        assert!(icons.is_empty());

        // Verify 5x4 tray grid cells fit cleanly inside Control Center flyout card.
        let card = Rect::new(0.0, 0.0, FLYOUT_W, FLYOUT_H);
        let c0 = cc_tray_cell_rect(card, 0);
        let c4 = cc_tray_cell_rect(card, 4);
        let c19 = cc_tray_cell_rect(card, 19);
        assert!(c4.right() <= card.w - FLYOUT_PAD + 0.5);
        assert!(c19.bottom() <= cc_sub_action_rect(card, 0).y);
        assert!(c0.w >= 48.0 && c0.h >= 40.0);
    }

    #[test]
    fn drag_reorder_spring_and_poof_effect_settle() {
        let mut poof = PoofEffect::new(Point::new(120.0, 36.0));
        assert!(poof.step(0.10));
        assert!(poof.step(0.20));
        assert!(!poof.step(0.20)); // elapsed >= 0.42s -> expires for 0% CPU at rest

        let mut sp = shift_spring(48.0);
        sp.set_target(0.0);
        assert!(!sp.is_settled_eps(SLOT_SHIFT_EPS));
        for _ in 0..120 {
            sp.step_eps(1.0 / 120.0, SLOT_SHIFT_EPS);
        }
        assert!(sp.is_settled_eps(SLOT_SHIFT_EPS));
        assert!(sp.value.abs() <= SLOT_SHIFT_EPS);

        let base = Rect::new(200.0, 72.0, 48.0, 48.0);
        let in_shelf = dragged_slot_rect(base, Point::new(260.0, 80.0), false);
        assert!((in_shelf.y - (base.y - 5.0)).abs() < 0.01);
        let pulled_out = dragged_slot_rect(base, Point::new(260.0, 20.0), true);
        assert!(pulled_out.y < in_shelf.y);
    }
}

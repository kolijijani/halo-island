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
use halo_core::settings::{NotchTheme, NotchType};
use halo_core::spring::Spring;
use halo_core::svgpath::Seg;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CLASS: PCWSTR = w!("Halo.Dock");
pub const STACK_CLASS: PCWSTR = w!("Halo.DockStack");

/// Not exported by windows-rs 0.58's WindowsAndMessaging module.
const WM_MOUSELEAVE: u32 = 0x02A3;

/// Fixed surface height in DIPs: room for the 58 DIP shelf, 8 DIP bottom gap,
/// magnification headroom (Phase 3), and floating item label without resizing HWND.
pub const SURF_H: f32 = 136.0;
pub const MAX_SURF_W: f32 = 1360.0;
pub const SIDE_MARGIN: f32 = 24.0;
pub const SHELF_H: f32 = 58.0;
pub const SHELF_R: f32 = 20.0;
pub const FLOAT_GAP: f32 = 8.0;
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

/// Downward drop (DIPs) and multi-stop Gaussian contact + ambient shadow ramp
/// around the floating shelf capsule, matching the macOS Sequoia liquid glass mockup.
const SHELF_SHADOW_DROP: f32 = 4.0;
const SHELF_SHADOW: &[(f32, f32)] = &[
    (-5.0, 0.48),
    (-3.5, 0.42),
    (-2.0, 0.36),
    (-0.5, 0.29),
    (1.0, 0.22),
    (3.5, 0.15),
    (7.0, 0.085),
    (11.5, 0.040),
    (16.5, 0.015),
    (22.0, 0.0),
];
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
    let th = halo_core::glass::thickness(body.h);
    let mix = if look.light { 1.0 } else { 0.0 };
    let (ground_dark, ground_light) = if look.light {
        (look::GLASS_DARK.ground, look.ground)
    } else {
        (look.ground, look::GLASS_LIGHT.ground)
    };
    GlassParams {
        body,
        radius,
        shoulder: -1.0,
        bevel: th.bevel,
        mag: th.mag,
        frost_mix: th.t,
        light_mix: mix,
        tint_dark: 0.34,
        tint_light: 0.42,
        ground_dark,
        ground_light,
        rim: 0.86 + 0.12 * mix,
        glow: 0.11 + 0.02 * mix,
        light_dir: (-0.45, -0.89),
        shadow_alpha: halo_core::glass::shadow_alpha(0.38, th.t),
        shadow_reach: th.shadow_reach,
        shadow_drop: th.shadow_drop,
        saturation: 1.45,
        dispersion: 0.03,
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
    Status,
    Clock,
}

pub fn tray_capsule_width(tray: &DockTrayState) -> f32 {
    let weather_w = if tray.weather_temp_c.is_some() { 70.0 } else { 0.0 };
    let status_w = if tray.has_battery { 120.0 } else { 76.0 };
    let clock_w = if tray.day > 0 && tray.month > 0 { 72.0 } else { 52.0 };
    weather_w + status_w + clock_w + if weather_w > 0.0 { 8.0 } else { 4.0 }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockCmd {
    ToggleTuck,
    OpenSettings,
    DisableDock,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum DockAction {
    None,
    RequestFrame,
    OpenLaunchpad,
    ToggleIsland,
    OpenPage(PageKind),
    OpenSettings,
    ToggleAppWindow { hwnd: HWND, minimized: bool, focused: bool, icon_rect: Rect },
    ToggleDownloadsStack(Rect),
    ShowMenu(POINT, bool),
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

fn register_class(class: PCWSTR, wndproc: crate::window::WndProc) -> Result<()> {
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
    let scale = mon.scale().max(1.0);
    let mon_w = (mon.rect.right - mon.rect.left).max(1);
    let max_dip = ((mon_w as f32 / scale) - 2.0 * SIDE_MARGIN).clamp(200.0, MAX_SURF_W);
    let w = ((max_dip * scale).ceil() as i32).clamp(1, mon_w);
    let h = ((SURF_H * scale).ceil() as i32).max(1);
    let x = mon.center_x() - w / 2;
    let y = mon.bottom() - h;
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

fn slot_metrics_with_seps_and_tray(surf_w: f32, count: usize, sep_count: usize, tray_w: f32) -> (f32, f32) {
    let n = count.max(1) as f32;
    let sep = sep_count as f32 * SEP_W;
    let avail = (surf_w - 2.0 * SHELF_PAD_X - 16.0 - sep - tray_w).max(64.0);
    let natural = n * SLOT_MAX + (n - 1.0) * SLOT_GAP;
    if natural <= avail {
        (SLOT_MAX, SLOT_GAP)
    } else {
        let ratio = (avail / natural).clamp(SLOT_MIN / SLOT_MAX, 1.0);
        let gap = (SLOT_GAP * ratio).max(4.0);
        let size = ((avail - (n - 1.0) * gap) / n).clamp(SLOT_MIN * 0.75, SLOT_MAX);
        (size, gap)
    }
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
    let total = anchor_count + app_count + stack_count;
    let has_sep1 = anchor_count > 0 && app_count > 0;
    let has_sep2 = stack_count > 0 && (anchor_count + app_count) > 0;
    let has_tray = tray_w > 0.0 && surf_w >= tray_w + 180.0;
    let eff_tray_w = if has_tray { tray_w } else { 0.0 };
    let has_sep3 = has_tray && total > 0;
    let sep_count = usize::from(has_sep1) + usize::from(has_sep2) + usize::from(has_sep3);
    let (base_slot, base_gap) = slot_metrics_with_seps_and_tray(surf_w, total, sep_count, eff_tray_w);
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

    let shelf_h = (base_slot + 14.0).clamp(42.0, SHELF_H);
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

pub struct Dock {
    /// Dropped before `hwnd` so DirectComposition detaches cleanly first.
    gfx: Option<Gfx>,
    pub hwnd: HWND,
    at: (i32, i32, i32, i32),
    dpi: u32,
    light: bool,
    look: &'static Look,
    shown: bool,
    fs_hidden: bool,
    tucked: bool,
    tuck: Spring,
    apps: Vec<RunningApp>,
    scales: Vec<Spring>,
    tracking: bool,
    mouse: Option<Point>,
    hovered: Option<usize>,
    pressed: Option<usize>,
    tray: DockTrayState,
    hovered_tray: Option<DockTrayZone>,
    pressed_tray: Option<DockTrayZone>,
    stack_open: bool,
    dirty: bool,
    mask: Option<(HitMask, f64)>,
}

impl Drop for Dock {
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

impl Dock {
    pub fn new(mon: &MonitorInfo, light: bool, shared: Option<SharedD3d>) -> Result<Self> {
        Self::with_look(mon, look::glass(light), shared)
    }

    pub fn with_look(mon: &MonitorInfo, look: &'static Look, shared: Option<SharedD3d>) -> Result<Self> {
        let (x, y, w, h) = surface_rect(mon);
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
        Ok(Self {
            gfx: Some(gfx),
            hwnd,
            at: (x, y, w, h),
            dpi: mon.dpi,
            light: look.light,
            look,
            shown: false,
            fs_hidden: false,
            tucked: false,
            tuck: Spring::smooth(0.0),
            apps: Vec::new(),
            scales: (0..ANCHOR_SLOTS.len() + 1).map(|_| mag_spring(1.0)).collect(),
            tracking: false,
            mouse: None,
            hovered: None,
            pressed: None,
            tray: DockTrayState::default(),
            hovered_tray: None,
            pressed_tray: None,
            stack_open: false,
            dirty: true,
            mask: None,
        })
    }

    /// Synchronize the dock's active [`Look`] with the island's theme; returns `true` only if it changed.
    pub fn set_look(&mut self, look: &'static Look) -> bool {
        if self.look == look && self.light == look.light {
            return false;
        }
        self.look = look;
        self.light = look.light;
        self.dirty = true;
        true
    }

    pub fn set_stack_open(&mut self, open: bool) {
        if self.stack_open != open {
            self.stack_open = open;
            self.dirty = true;
        }
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

    /// True only while a visual change, tuck, or magnification spring needs vblank frames;
    /// false at rest so `FrameClock` parks at 0% CPU.
    pub fn needs_frame(&self) -> bool {
        !self.fs_hidden
            && (self.dirty
                || !self.tuck.is_settled_eps(TUCK_EPS)
                || self.scales.iter().any(|s| !s.is_settled_eps(MAG_EPS)))
    }

    pub fn total_slots(&self) -> usize {
        ANCHOR_SLOTS.len() + self.apps.len() + 1
    }

    fn slot_screen_rect(&self, index: usize) -> Rect {
        let sw = self.surf_w_dip();
        let scales = self.current_scales();
        let tray_w = tray_capsule_width(&self.tray);
        let (_, _, _, _, rects, _) =
            layout_slots_full(sw, ANCHOR_SLOTS.len(), self.apps.len(), 1, tray_w, &scales, self.tuck.value);
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
        for next in &mut apps {
            if next.minimized && next.thumb.is_none() {
                if let Some(prev) = self.apps.iter().find(|a| a.hwnd == next.hwnd) {
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

    /// Attach a captured window thumbnail to the slot for `hwnd` when minimizing into the dock.
    pub fn set_app_thumb(&mut self, hwnd: HWND, thumb: Image) {
        if let Some(app) = self.apps.iter_mut().find(|a| a.hwnd == hwnd) {
            app.thumb = Some(thumb);
            self.dirty = true;
        }
    }

    /// Return the cached window thumbnail for `hwnd`, if captured when minimized.
    pub fn app_thumb(&self, hwnd: HWND) -> Option<Image> {
        self.apps.iter().find(|a| a.hwnd == hwnd).and_then(|a| a.thumb.clone())
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
        if self.tucked || self.tuck.value >= 0.25 || self.fs_hidden {
            self.reset_mag_targets(instant);
            return;
        }
        let Some(p) = self.mouse else {
            self.reset_mag_targets(instant);
            return;
        };
        let sw = self.surf_w_dip();
        let rest_scales = vec![1.0f32; total];
        let tray_w = tray_capsule_width(&self.tray);
        let (rest_shelf, _, _, sep3_x, rest_rects, _) =
            layout_slots_full(sw, ANCHOR_SLOTS.len(), self.apps.len(), 1, tray_w, &rest_scales, self.tuck.value);
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
        let sep_count = usize::from(!self.apps.is_empty()) + 1 + usize::from(sep3_x.is_some());
        let eff_tray = if sep3_x.is_some() { tray_w } else { 0.0 };
        let (base_slot, base_gap) = slot_metrics_with_seps_and_tray(sw, total, sep_count, eff_tray);
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

    pub fn set_monitor(&mut self, mon: &MonitorInfo, light: bool) {
        let next = surface_rect(mon);
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
        let tray_w = tray_capsule_width(&self.tray);
        let (_, _, _, _, rects, _) =
            layout_slots_full(sw, ANCHOR_SLOTS.len(), self.apps.len(), 1, tray_w, &scales, self.tuck.value);
        rects.iter().position(|r| r.inset(-3.0, -6.0).contains(p))
    }

    fn tray_zone_at(&self, p: Point) -> Option<DockTrayZone> {
        if self.tucked && self.tuck.value >= 0.5 {
            return None;
        }
        let sw = self.surf_w_dip();
        let scales = self.current_scales();
        let tray_w = tray_capsule_width(&self.tray);
        let (_, _, _, _, _, tray_rect) =
            layout_slots_full(sw, ANCHOR_SLOTS.len(), self.apps.len(), 1, tray_w, &scales, self.tuck.value);
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
            Some(DockTrayZone::Status)
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
                self.tracking = false;
                self.mouse = None;
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
                self.dirty = true;
                if down == Some(usize::MAX) && self.tucked && peek_hit_rect(self.surf_w_dip()).contains(p) {
                    self.set_tucked(false, instant);
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                if let Some(z) = down_tray.filter(|&z| Some(z) == self.tray_zone_at(p)) {
                    let page = match z {
                        DockTrayZone::Weather | DockTrayZone::Clock => PageKind::Today,
                        DockTrayZone::Status => PageKind::System,
                    };
                    return (Some(LRESULT(0)), DockAction::OpenPage(page));
                }
                if let Some(i) = down.filter(|&i| Some(i) == self.slot_at(p)) {
                    if i < ANCHOR_SLOTS.len() {
                        let act = match ANCHOR_SLOTS[i].action {
                            SlotAction::Launchpad => DockAction::OpenLaunchpad,
                            SlotAction::ToggleIsland => DockAction::ToggleIsland,
                            SlotAction::Page(pk) => DockAction::OpenPage(pk),
                            SlotAction::Settings => DockAction::OpenSettings,
                        };
                        return (Some(LRESULT(0)), act);
                    }
                    if let Some(app) = self.apps.get(i - ANCHOR_SLOTS.len()) {
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
                    if i == ANCHOR_SLOTS.len() + self.apps.len() {
                        let icon_rect = self.slot_screen_rect(i);
                        return (Some(LRESULT(0)), DockAction::ToggleDownloadsStack(icon_rect));
                    }
                }
                (Some(LRESULT(0)), DockAction::RequestFrame)
            }
            WM_RBUTTONUP => {
                let mut pt = POINT {
                    x: (lp.0 & 0xffff) as u16 as i16 as i32,
                    y: ((lp.0 >> 16) & 0xffff) as u16 as i16 as i32,
                };
                unsafe {
                    let _ = ClientToScreen(self.hwnd, &mut pt);
                }
                (Some(LRESULT(0)), DockAction::ShowMenu(pt, self.tucked))
            }
            WM_CAPTURECHANGED => {
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
        } else {
            self.tuck.step_eps(dt, TUCK_EPS);
            for s in &mut self.scales {
                s.step_eps(dt, MAG_EPS);
            }
        }
        let (x, y, w, h) = self.at;
        let scale = self.scale().max(1.0);
        let surf_w = self.surf_w_dip();
        let tuck_t = self.tuck.value.clamp(0.0, 1.0);
        let look = self.look;
        let spec_x = self.mouse.map(|p| p.x);
        let hovered = self.hovered;
        let pressed = self.pressed;
        let hovered_tray = self.hovered_tray;
        let pressed_tray = self.pressed_tray;
        let tray = self.tray;
        let tray_w = tray_capsule_width(&tray);
        let scales = self.current_scales();
        let (shelf, sep_x, sep2_x, sep3_x, slot_rects, tray_rect) =
            layout_slots_full(surf_w, ANCHOR_SLOTS.len(), self.apps.len(), 1, tray_w, &scales, tuck_t);
        let max_lift = slot_rects.iter().fold(0.0f32, |m, r| m.max((shelf.y - r.y).max(0.0)));
        let stack_open = self.stack_open;

        let Some(gfx) = self.gfx.as_mut() else { return };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if let Err(e) = gfx.set_surface(w, h) {
            crate::pwarn!("dock: set_surface failed ({e})");
            return;
        }
        gfx.set_look(look);
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        let glass_params = if tuck_t < 0.995 && look.frame.is_none() {
            let r = SHELF_R.min(shelf.h * 0.5);
            let gp = dock_glass_params(shelf, r, look, scale);
            gfx.prepare_glass(&gp, full);
            Some(gp)
        } else {
            None
        };
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
                    &self.apps,
                    stack_open,
                    hovered,
                    pressed,
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
                && self.scales.iter().all(|s| s.is_settled_eps(MAG_EPS));
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
    let light = look.light;
    if look.frame.is_some() {
        // Diorama theme: raised 3D picture frame around the themed pane, with liquid glass sheen.
        crate::pill::shape::draw_body(c, shelf, r, look.ground.with_alpha(0.94), Some(look.border));
    } else {
        // Multi-stop ambient + contact drop shadow (0 24px 56px rgba(0,0,0,0.68)).
        c.rrect_shadow(shelf.offset(0.0, SHELF_SHADOW_DROP), r, Color::black(0.96), SHELF_SHADOW);

        // Island GPU Glass shader pass (or smoked glass fallback).
        if let Some(gp) = glass {
            c.glass(gp, Rect::new(0.0, 0.0, surf_w, SURF_H));
        }

        // Theme-tinted translucent frosted glass body (#dockShelf base).
        let is_halo_cream = look.ground == look::HALO.ground;
        let base = if is_halo_cream {
            look.ground.with_alpha(0.56)
        } else if light {
            Color::lerp(Color::rgba(0.95, 0.96, 0.99, 1.0), look.ground, 0.45).with_alpha(0.54)
        } else {
            Color::lerp(Color::rgba(0.07, 0.09, 0.14, 1.0), look.ground, 0.35).with_alpha(0.58)
        };
        c.fill_rrect(shelf, r, base);
    }

    // Clipped Liquid Glass shading layers: top specular sheen, bottom depth, and pointer-reactive #dockSpecular.
    let segs = crate::pill::shape::segments(shelf, r);
    c.push_outline_mask(&segs);

    // 1. Top-to-mid liquid glass reflection (linear-gradient 180deg, rgba(255,255,255,0.17) 0% -> 38%).
    let top_sheen_a = if light { 0.38 } else { 0.19 };
    c.fill_path_fade(&segs, Color::white(1.0), shelf.y, shelf.y + shelf.h * 0.44, top_sheen_a, 0.0);

    // 2. Mid-to-bottom deep glass grounding (38% -> 100%).
    let depth_col = if light { look.ink } else { Color::hex(0x050811) };
    let depth_a = if light { 0.06 } else { 0.34 };
    c.fill_path_fade(&segs, depth_col, shelf.y + shelf.h * 0.36, shelf.bottom(), 0.0, depth_a);

    // 3. #dockSpecular: radial-gradient(250px 62px at var(--spec-x, 50%) 0%, rgba(255,255,255,0.26), transparent 72%).
    let sx = spec_x.unwrap_or(shelf.center().x).clamp(shelf.x + r, (shelf.right() - r).max(shelf.x + r));
    let spec_col = look.accent.unwrap_or(Color::white(1.0));
    let glow_tint = Color::lerp(Color::white(1.0), spec_col, if look.accent.is_some() { 0.22 } else { 0.0 });
    for (gw, gh, ga) in [(240.0f32, 52.0f32, 0.042f32), (164.0, 36.0, 0.055), (104.0, 22.0, 0.068), (56.0, 12.0, 0.075)] {
        let gr = Rect::new(sx - gw * 0.5, shelf.y - gh * 0.42, gw, gh);
        c.fill_rrect(gr, gh * 0.5, glow_tint.with_alpha(if light { ga * 1.15 } else { ga }));
    }

    // 4. Crisp top specular highlight rim (inset 0 1px 0 rgba(255,255,255,0.46)).
    let inner = shelf.inset(1.0, 1.0);
    let inner_r = (r - 1.0).max(1.0);
    let top_band = Rect::new(inner.x + inner_r * 0.35, inner.y, (inner.w - inner_r * 0.70).max(1.0), 1.5);
    c.fill_rrect(top_band, 0.75, Color::white(if light { 0.68 } else { 0.44 }));

    // 5. Subtle bottom inner reflection (inset 0 -1px 0 rgba(255,255,255,0.10)).
    let bot_band = Rect::new(inner.x + inner_r * 0.45, inner.bottom() - 1.0, (inner.w - inner_r * 0.90).max(1.0), 1.0);
    c.fill_rrect(bot_band, 0.5, Color::white(if light { 0.28 } else { 0.11 }));

    c.pop_shape_mask();

    // 6. Outer glass border rim (1px solid rgba(255, 255, 255, 0.22) or theme border).
    if look.frame.is_none() {
        let outer_rim = if light {
            look.border.with_alpha(look.border.a.max(0.14))
        } else {
            Color::white(0.22)
        };
        c.stroke_rrect(shelf, r, outer_rim, 1.0);
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

/// Draw a macOS-style top-right notification badge pill (`.dock-badge` / `.dock-badge.blue`).
fn draw_dock_badge(c: &mut Canvas<'_>, tile: Rect, count: u16, style: DockBadgeStyle) {
    if count == 0 {
        return;
    }
    let br = badge_rect(tile, count);
    let r = br.h * 0.5;
    let (base, glow) = match style {
        DockBadgeStyle::Red => (Color::hex(0xE62E3D), Color::hex(0xE62E3D).with_alpha(0.42)),
        DockBadgeStyle::Blue => (Color::hex(0x2563EB), Color::hex(0x2563EB).with_alpha(0.42)),
    };
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

/// Draw a macOS Sequoia minimized window thumbnail card (`.min-thumb`) with live screenshot
/// (or stylized window preview) and corner app-icon badge (`.min-thumb-badge`).
fn draw_minimized_window_tile(
    c: &mut Canvas<'_>,
    tile: Rect,
    app: &RunningApp,
    theme_ink: Color,
    theme_accent: Color,
    light: bool,
    is_hover: bool,
) {
    let (card, bar, badge) = minimized_card_rects(tile);
    let cr = (card.h * 0.22).clamp(5.0, 9.0);
    draw_squircle_shadow(c, card, cr, light);

    let bg = if light {
        Color::rgba(0.93, 0.95, 0.98, 0.95)
    } else {
        Color::hex(0x141821).with_alpha(0.96)
    };
    c.fill_rrect(card, cr, bg);

    // Body area below the mini window titlebar
    let body = Rect::new(card.x + 1.5, bar.bottom() + 1.0, (card.w - 3.0).max(4.0), (card.bottom() - bar.bottom() - 2.5).max(4.0));
    if let Some(thumb) = app.thumb.as_ref() {
        c.image(thumb, body, (cr - 2.0).max(2.5));
    } else {
        // Stylized mini window preview (.min-thumb-art + .min-thumb-lines) when minimized before dock start
        let art_sz = (body.h - 4.0).clamp(6.0, body.w * 0.42);
        let art = Rect::new(body.x + 2.5, body.y + (body.h - art_sz) * 0.5, art_sz, art_sz);
        c.fill_rrect(art, (art_sz * 0.24).max(2.0), theme_accent.with_alpha(0.85));
        let lx = art.right() + 3.5;
        let lw = (body.right() - lx - 2.5).max(4.0);
        let lh = 2.2f32.min(body.h * 0.16);
        let line_c = theme_ink.with_alpha(if light { 0.36 } else { 0.44 });
        for (k, frac) in [1.0f32, 0.74, 0.52].iter().enumerate() {
            let ly = body.y + 3.0 + k as f32 * (lh + 2.4);
            if ly + lh <= body.bottom() {
                c.fill_rrect(Rect::new(lx, ly, lw * frac, lh), lh * 0.5, line_c);
            }
        }
    }

    // Mini window titlebar (.min-thumb-bar) with 3 window control dots
    let bar_bg = if light { Color::black(0.07) } else { Color::white(0.10) };
    c.fill_rrect(bar, cr, bar_bg);
    let dot_r = (bar.h * 0.18).clamp(1.0, 1.7);
    let dot_y = bar.y + bar.h * 0.5;
    let dot_c = theme_ink.with_alpha(0.48);
    for k in 0..3 {
        let dx = bar.right() - 4.0 - k as f32 * (dot_r * 2.6);
        if dx > bar.x + 4.0 {
            c.fill_circle(Point::new(dx, dot_y), dot_r, dot_c);
        }
    }

    // Top inner specular bevel & outer rim
    let segs = crate::pill::shape::segments(card, cr);
    c.fill_path_fade(&segs, Color::white(1.0), card.y, card.y + card.h * 0.45, if light { 0.32 } else { 0.18 }, 0.0);
    let rim = if is_hover {
        theme_accent.with_alpha(0.76)
    } else if light {
        Color::black(0.18)
    } else {
        Color::white(0.28)
    };
    c.stroke_rrect(card, cr, rim, 1.0);

    // Corner app icon badge (.min-thumb-badge)
    let br = (badge.w * 0.28).clamp(3.5, 6.5);
    c.fill_rrect(badge.offset(0.0, 1.5), br, Color::black(0.45));
    if let Some(img) = app.icon.as_ref() {
        c.image_hq(img, badge);
    } else {
        let badge_bg = if light { Color::white(0.94) } else { Color::hex(0x1E293B) };
        c.fill_rrect(badge, br, badge_bg);
        c.stroke_rrect(badge, br, Color::white(0.25), 0.9);
        c.icon(Icon::AppWindow, badge.inset(badge.w * 0.18, badge.h * 0.18), theme_ink);
    }
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
    stack_open: bool,
    hovered: Option<usize>,
    pressed: Option<usize>,
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
                            draw_dock_badge(c, tile, tray.tasks_open, DockBadgeStyle::Red);
                        }
                        SlotAction::Page(PageKind::Shelf) if tray.shelf_count > 0 => {
                            draw_dock_badge(c, tile, tray.shelf_count, DockBadgeStyle::Blue);
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

                for (j, app) in apps.iter().enumerate() {
                    let idx = ANCHOR_SLOTS.len() + j;
                    let Some(&sr) = slot_rects.get(idx) else { continue };
                    let is_hover = hovered == Some(idx);
                    let is_press = pressed == Some(idx);
                    let tile = if is_press { sr.inset(1.5, 1.5) } else { sr };
                    let tr = (tile.w * 0.25).clamp(8.0, 15.0);

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

                    if app.minimized {
                        draw_minimized_window_tile(c, tile, app, theme_ink, theme_accent, light, is_hover);
                    } else if let Some(img) = app.icon.as_ref() {
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
                        draw_dock_badge(c, tile, app.badge, DockBadgeStyle::Red);
                    }

                    // Active running-app indicator dot (focused = brighter & slightly larger).
                    let dot_r = if app.focused { 2.3 } else if app.minimized { 1.6 } else { 1.9 };
                    let dot_a = if app.focused { 0.96 } else if app.minimized { 0.55 } else { 0.78 };
                    draw_active_dot(c, Point::new(sr.x + sr.w * 0.5, dot_y), dot_r, dot_ink, dot_a);
                }

                if let Some(sx) = sep2_x {
                    draw_glass_divider(c, sx, shelf, light);
                }

                let dl_idx = ANCHOR_SLOTS.len() + apps.len();
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
                        draw_dock_badge(c, tile, tray.downloads_active, DockBadgeStyle::Blue);
                    }
                    if stack_open {
                        draw_active_dot(c, Point::new(sr.x + sr.w * 0.5, dot_y), 2.1, accent, 0.95);
                    }
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

                    let (weather_r, status_r, clock_r) = tray_zone_rects(tr, tray);
                    let ink = theme_ink;
                    let dim = theme_ink.with_alpha(if light { 0.56 } else { 0.64 });

                    let zones = [
                        weather_r.map(|r| (DockTrayZone::Weather, r)),
                        Some((DockTrayZone::Status, status_r)),
                        Some((DockTrayZone::Clock, clock_r)),
                    ];
                    for (zone, zr) in zones.into_iter().flatten() {
                        let is_h = hovered_tray == Some(zone);
                        let is_p = pressed_tray == Some(zone);
                        let bg = if is_p {
                            if light { Color::black(0.13) } else { Color::white(0.19) }
                        } else if is_h {
                            if light { Color::black(0.08) } else { Color::white(0.13) }
                        } else if light {
                            Color::white(0.42)
                        } else {
                            Color::white(0.06)
                        };
                        let rim = if is_h {
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

                    // Clock & Date stack zone.
                    if let Some(date_s) = tray.date_text() {
                        let time_st = TextStyle { size: 11.0, ..TextStyle::caption().weight(Weight::Semibold).center().color(ink) };
                        let date_st = TextStyle { size: 8.5, ..TextStyle::caption().center().color(dim) };
                        c.text(&tray.clock_text(), Rect::new(clock_r.x, clock_r.y + 2.5, clock_r.w, 14.0), &time_st);
                        c.text(&date_s, Rect::new(clock_r.x, clock_r.y + 15.0, clock_r.w, 12.0), &date_st);
                    } else {
                        let clock_st = TextStyle::caption().weight(Weight::Semibold).center().color(ink);
                        c.text(&tray.clock_text(), clock_r, &clock_st);
                    }
                }

                let tip: Option<(&str, Rect)> = if let Some(i) = hovered {
                    let label = if i < ANCHOR_SLOTS.len() {
                        Some(ANCHOR_SLOTS[i].label)
                    } else if i == dl_idx {
                        Some("Downloads")
                    } else {
                        apps.get(i - ANCHOR_SLOTS.len()).map(|a| a.name.as_str())
                    };
                    label.zip(slot_rects.get(i).copied())
                } else if let (Some(z), Some(tr)) = (hovered_tray, tray_rect) {
                    let (weather_r, status_r, clock_r) = tray_zone_rects(tr, tray);
                    match z {
                        DockTrayZone::Weather => weather_r.map(|r| ("Weather", r)),
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
pub fn show_menu(owner: HWND, pt: POINT, tucked: bool) -> Option<DockCmd> {
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let toggle_label: Vec<u16> = if tucked { "Show shelf\0" } else { "Hide shelf\0" }.encode_utf16().collect();
        let settings_label: Vec<u16> = "Dock settings…\0".encode_utf16().collect();
        let off_label: Vec<u16> = "Turn off Bottom Dock\0".encode_utf16().collect();
        let _ = AppendMenuW(menu, MF_STRING, 1, PCWSTR(toggle_label.as_ptr()));
        let _ = AppendMenuW(menu, MF_STRING, 2, PCWSTR(settings_label.as_ptr()));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, 3, PCWSTR(off_label.as_ptr()));

        let prev = GetForegroundWindow();
        let _ = SetForegroundWindow(owner);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, owner, None);
        let _ = PostMessageW(owner, WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);

        let cmd = match chosen.0 as usize {
            1 => Some(DockCmd::ToggleTuck),
            2 => Some(DockCmd::OpenSettings),
            3 => Some(DockCmd::DisableDock),
            _ => None,
        };
        if cmd != Some(DockCmd::OpenSettings) && !prev.is_invalid() && prev != owner && IsWindow(prev).as_bool() && GetForegroundWindow() == owner {
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
        let glass_params = if look.frame.is_none() {
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
                if look.frame.is_some() {
                    crate::pill::shape::draw_body(c, body, STACK_R, look.ground.with_alpha(0.95), Some(look.border));
                } else {
                    if let Some(gp) = glass_params.as_ref() {
                        c.glass(gp, card);
                    }
                    let bg = if look.ground == look::HALO.ground {
                        look.ground.with_alpha(0.68)
                    } else if light {
                        Color::rgba(0.96, 0.97, 0.99, 0.64)
                    } else {
                        Color::rgba(0.08, 0.10, 0.15, 0.68)
                    };
                    c.fill_rrect(body, STACK_R, bg);
                    let segs = crate::pill::shape::segments(body, STACK_R);
                    c.fill_path_fade(&segs, Color::white(1.0), body.y, body.y + body.h * 0.38, if light { 0.34 } else { 0.16 }, 0.0);
                    c.stroke_rrect(body, STACK_R, rim, 1.0);
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

        let gp_halo = dock_glass_params(shelf, SHELF_R, &look::HALO, 1.0);
        assert_eq!(gp_halo.light_mix, 1.0);
        assert_eq!(gp_halo.ground_light, look::HALO.ground);
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
}

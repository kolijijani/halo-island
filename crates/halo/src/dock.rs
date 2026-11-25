//! Floating bottom dock shelf (Direction A foundation): Direct2D + DirectComposition
//! layered tool window (`Halo.Dock`) sharing the island's D3D11 device with
//! non-blocking `Present(0, 0)`, rounded-pill hit mask, right-click tuck/peek
//! handle, fullscreen hide, and zero idle CPU when settled.

use crate::gfx::{Canvas, Gfx, HitMask, SharedD3d, TextStyle, Weight};
use crate::icons::Icon;
use crate::pages::PageKind;
use crate::window::MonitorInfo;
use halo_core::geom::{palette, Color, Point, Rect};
use halo_core::look;
use halo_core::spring::Spring;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const CLASS: PCWSTR = w!("Halo.Dock");

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
const MASK_INTERVAL: f64 = 1.0 / 30.0;
const TUCK_EPS: f32 = 0.004;

/// Shadow ramp around the floating shelf capsule (distance outside body in DIPs, alpha).
const SHELF_SHADOW: &[(f32, f32)] = &[(-2.0, 0.34), (0.0, 0.28), (5.0, 0.14), (12.0, 0.04), (20.0, 0.0)];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockCmd {
    ToggleTuck,
    OpenSettings,
    DisableDock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DockAction {
    None,
    RequestFrame,
    ToggleIsland,
    OpenPage(PageKind),
    OpenSettings,
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

#[derive(Clone, Copy)]
enum SlotAction {
    ToggleIsland,
    Page(PageKind),
    Settings,
}

const ANCHOR_SLOTS: &[AnchorSlot] = &[
    AnchorSlot { icon: Icon::AppWindow, label: "Halo Island", accent: Color::hex(0x0A84FF), action: SlotAction::ToggleIsland, active: true },
    AnchorSlot { icon: Icon::Tray, label: "Shelf", accent: Color::hex(0x5E5CE6), action: SlotAction::Page(PageKind::Shelf), active: false },
    AnchorSlot { icon: Icon::ListChecks, label: "Tasks", accent: Color::hex(0x30D158), action: SlotAction::Page(PageKind::Tasks), active: false },
    AnchorSlot { icon: Icon::Clock, label: "Focus", accent: Color::hex(0xFF9F0A), action: SlotAction::Page(PageKind::Focus), active: false },
    AnchorSlot { icon: Icon::Gear, label: "Dock Settings", accent: Color::hex(0x8E8E93), action: SlotAction::Settings, active: false },
];

/// Register `Halo.Dock` once at process startup.
pub fn register(wndproc: crate::window::WndProc) -> Result<()> {
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
            lpszClassName: CLASS,
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

/// Single-row slot size in DIPs for `count` items inside `surf_w` DIPs:
/// shrinks proportionally at high counts (e.g. 30 apps) so icons never wrap or clip.
pub fn slot_metrics(surf_w: f32, count: usize) -> (f32, f32) {
    let n = count.max(1) as f32;
    let avail = (surf_w - 2.0 * SHELF_PAD_X - 16.0).max(64.0);
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
    if tucked && tuck_t >= 0.85 {
        HitMask { rect: Some(peek_hit_rect(surf_w)), notch: None, pill: None }
    } else {
        let r = shelf_rect(surf_w, count, tuck_t);
        HitMask { rect: None, notch: None, pill: Some((r, SHELF_R.min(r.h * 0.5))) }
    }
}

fn slot_rect(shelf: Rect, index: usize, count: usize, surf_w: f32) -> Rect {
    let (slot, gap) = slot_metrics(surf_w, count);
    let x = shelf.x + SHELF_PAD_X + index as f32 * (slot + gap);
    let y = shelf.y + (shelf.h - slot) * 0.5;
    Rect::new(x, y, slot, slot)
}

pub struct Dock {
    /// Dropped before `hwnd` so DirectComposition detaches cleanly first.
    gfx: Option<Gfx>,
    pub hwnd: HWND,
    at: (i32, i32, i32, i32),
    dpi: u32,
    light: bool,
    shown: bool,
    fs_hidden: bool,
    tucked: bool,
    tuck: Spring,
    tracking: bool,
    mouse: Option<Point>,
    hovered: Option<usize>,
    pressed: Option<usize>,
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
            light,
            shown: false,
            fs_hidden: false,
            tucked: false,
            tuck: Spring::smooth(0.0),
            tracking: false,
            mouse: None,
            hovered: None,
            pressed: None,
            dirty: true,
            mask: None,
        })
    }

    pub fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    pub fn surf_w_dip(&self) -> f32 {
        self.at.2 as f32 / self.scale().max(1.0)
    }

    /// True only while a visual change or tuck animation needs vblank frames;
    /// false at rest so `FrameClock` parks at 0% CPU.
    pub fn needs_frame(&self) -> bool {
        !self.fs_hidden && (self.dirty || !self.tuck.is_settled_eps(TUCK_EPS))
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
            self.tracking = false;
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
        let shelf = shelf_rect(sw, ANCHOR_SLOTS.len(), self.tuck.value);
        if !shelf.contains(p) {
            return None;
        }
        (0..ANCHOR_SLOTS.len()).find(|&i| slot_rect(shelf, i, ANCHOR_SLOTS.len(), sw).inset(-2.0, -4.0).contains(p))
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
                self.mouse = Some(p);
                let next_hover = self.slot_at(p);
                if next_hover != self.hovered {
                    self.hovered = next_hover;
                    self.dirty = true;
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                (Some(LRESULT(0)), DockAction::None)
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                self.mouse = None;
                if self.hovered.take().is_some() || self.pressed.take().is_some() {
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
                if hit != self.pressed {
                    self.pressed = hit;
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
                self.dirty = true;
                if down == Some(usize::MAX) && self.tucked && peek_hit_rect(self.surf_w_dip()).contains(p) {
                    self.set_tucked(false, instant);
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                if let Some(i) = down.filter(|&i| Some(i) == self.slot_at(p)) {
                    let act = match ANCHOR_SLOTS[i].action {
                        SlotAction::ToggleIsland => DockAction::ToggleIsland,
                        SlotAction::Page(pk) => DockAction::OpenPage(pk),
                        SlotAction::Settings => DockAction::OpenSettings,
                    };
                    return (Some(LRESULT(0)), act);
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
                if self.pressed.take().is_some() {
                    self.dirty = true;
                    return (Some(LRESULT(0)), DockAction::RequestFrame);
                }
                (Some(LRESULT(0)), DockAction::None)
            }
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                let hand = self.hovered.is_some()
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
        } else {
            self.tuck.step_eps(dt, TUCK_EPS);
        }
        let (x, y, w, h) = self.at;
        let surf_w = self.surf_w_dip();
        let tuck_t = self.tuck.value.clamp(0.0, 1.0);
        let light = self.light;
        let hovered = self.hovered;
        let pressed = self.pressed;

        let Some(gfx) = self.gfx.as_mut() else { return };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if let Err(e) = gfx.set_surface(w, h) {
            crate::pwarn!("dock: set_surface failed ({e})");
            return;
        }
        gfx.set_look(look::glass(light));
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        match gfx.begin(full) {
            Ok(mut c) => {
                draw_dock(&mut c, surf_w, light, tuck_t, hovered, pressed);
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
            let mask = hit_mask_for(surf_w, ANCHOR_SLOTS.len(), self.tucked, tuck_t);
            let settled = self.tuck.is_settled_eps(TUCK_EPS);
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

fn draw_dock(c: &mut Canvas<'_>, surf_w: f32, light: bool, tuck_t: f32, hovered: Option<usize>, pressed: Option<usize>) {
    c.unthemed(|c| {
        if tuck_t < 0.995 {
            let alpha = (1.0 - tuck_t).clamp(0.0, 1.0);
            c.with_alpha(alpha, |c| {
                let shelf = shelf_rect(surf_w, ANCHOR_SLOTS.len(), tuck_t);
                let r = SHELF_R.min(shelf.h * 0.5);
                c.rrect_shadow(shelf, r, Color::black(1.0), SHELF_SHADOW);
                let ground = if light {
                    Color::rgba(0.95, 0.96, 0.98, 0.84)
                } else {
                    Color::rgba(0.11, 0.12, 0.15, 0.82)
                };
                let rim = if light { Color::black(0.10) } else { Color::white(0.16) };
                c.fill_rrect(shelf, r, ground);
                c.stroke_rrect(shelf, r, rim, 1.0);

                for (i, slot) in ANCHOR_SLOTS.iter().enumerate() {
                    let sr = slot_rect(shelf, i, ANCHOR_SLOTS.len(), surf_w);
                    let is_hover = hovered == Some(i);
                    let is_press = pressed == Some(i);
                    let tile = if is_press { sr.inset(1.5, 1.5) } else { sr };
                    let tr = (tile.w * 0.26).clamp(8.0, 12.0);
                    let fill_a = if is_press { 0.30 } else if is_hover { 0.24 } else { 0.15 };
                    c.fill_rrect(tile, tr, slot.accent.with_alpha(fill_a));
                    let tile_rim = if is_hover {
                        slot.accent.with_alpha(0.55)
                    } else if light {
                        Color::black(0.08)
                    } else {
                        Color::white(0.12)
                    };
                    c.stroke_rrect(tile, tr, tile_rim, 1.0);

                    let icon_box = tile.inset(tile.w * 0.24, tile.h * 0.24);
                    let icon_ink = if light { Color::hex(0x1C1C1E) } else { palette::TEXT };
                    c.icon(slot.icon, icon_box, icon_ink);

                    if slot.active {
                        let dot_y = shelf.bottom() - 4.0;
                        let dot_c = if light { Color::hex(0x1C1C1E).with_alpha(0.75) } else { Color::white(0.85) };
                        c.fill_circle(Point::new(sr.x + sr.w * 0.5, dot_y), 2.0, dot_c);
                    }
                }

                if let Some(i) = hovered.filter(|&i| i < ANCHOR_SLOTS.len()) {
                    let label = ANCHOR_SLOTS[i].label;
                    let sr = slot_rect(shelf, i, ANCHOR_SLOTS.len(), surf_w);
                    let st = TextStyle::caption()
                        .weight(Weight::Medium)
                        .center()
                        .color(if light { Color::hex(0x1C1C1E) } else { palette::TEXT });
                    let (tw, _) = c.measure(label, &st, 220.0);
                    let pw = (tw + 18.0).max(52.0);
                    let ph = 24.0;
                    let px = (sr.x + sr.w * 0.5 - pw * 0.5).clamp(4.0, (surf_w - pw - 4.0).max(4.0));
                    let py = (shelf.y - ph - 8.0).max(2.0);
                    let pill = Rect::new(px, py, pw, ph);
                    let tip_bg = if light {
                        Color::rgba(0.97, 0.97, 0.99, 0.94)
                    } else {
                        Color::rgba(0.14, 0.15, 0.18, 0.94)
                    };
                    let tip_rim = if light { Color::black(0.10) } else { Color::white(0.14) };
                    c.fill_rrect(pill, ph * 0.5, tip_bg);
                    c.stroke_rrect(pill, ph * 0.5, tip_rim, 1.0);
                    c.text(label, pill, &st);
                }
            });
        }

        if tuck_t > 0.05 {
            let alpha = ((tuck_t - 0.05) / 0.95).clamp(0.0, 1.0);
            c.with_alpha(alpha, |c| {
                let pr = peek_rect(surf_w);
                let fill = if light { Color::black(0.38) } else { Color::white(0.45) };
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
}

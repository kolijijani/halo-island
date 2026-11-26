//! Live Window Hover Preview popover (`Halo.DockPreview`) for the bottom dock.
//!
//! Uses Windows DWM live thumbnails (`DwmRegisterThumbnail`) hosted in click-through
//! owned `WS_POPUP` surfaces aligned strictly inside each glass preview card. DWM
//! composites live video/app content directly on the GPU at monitor refresh rate with
//! 0% CPU wakeups in Halo, and every thumbnail handle + window is unregistered and
//! destroyed immediately on hide/leave (0% CPU/GPU at rest).

use crate::dock::{dock_glass_params, register_class};
use crate::gfx::{Canvas, Gfx, HitMask, Image, SharedD3d, TextStyle, Weight};
use crate::icons::Icon;
use crate::services::dock_apps::RunningApp;
use crate::window::MonitorInfo;
use halo_core::geom::{Color, Point, Rect};
use halo_core::look::{self, Look};
use halo_core::svgpath::Seg;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmQueryThumbnailSourceSize, DwmRegisterThumbnail, DwmSetWindowAttribute, DwmUnregisterThumbnail,
    DwmUpdateThumbnailProperties, DWMWCP_ROUNDSMALL, DWMWA_TRANSITIONS_FORCEDISABLED,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWM_THUMBNAIL_PROPERTIES, DWM_TNP_OPACITY, DWM_TNP_RECTDESTINATION,
    DWM_TNP_SOURCECLIENTAREAONLY, DWM_TNP_VISIBLE,
};
use windows::Win32::Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT};
use windows::Win32::UI::WindowsAndMessaging::*;

pub const PREVIEW_CLASS: PCWSTR = w!("Halo.DockPreview");
const DWM_THUMB_CLASS: PCWSTR = w!("Halo.DockPreviewThumb");
const WM_MOUSELEAVE: u32 = 0x02A3;

pub const PREVIEW_MAX_CARDS: usize = 4;
const CARD_W: f32 = 216.0;
const CARD_H: f32 = 156.0;
const CARD_GAP: f32 = 10.0;
const OUTER_PAD: f32 = 10.0;
const OUTER_R: f32 = 18.0;
const CARD_R: f32 = 13.0;
const HDR_H: f32 = 28.0;
const WELL_PAD_X: f32 = 8.0;
const WELL_PAD_BOTTOM: f32 = 8.0;
const ANCHOR_GAP: f32 = 10.0;

#[derive(Clone, Debug)]
pub struct PreviewWindow {
    pub hwnd: HWND,
    pub title: String,
    pub minimized: bool,
    pub focused: bool,
    pub fallback_thumb: Option<Image>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreviewAction {
    None,
    RequestFrame,
    FocusWindow { hwnd: HWND, minimized: bool, icon_rect: Rect },
    CloseWindow(HWND),
    MouseEnter,
    MouseLeave,
    Dismiss,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewHit {
    Card(usize),
    Close(usize),
}

struct LiveThumb {
    host: HWND,
    handle: isize,
    card_idx: usize,
}

impl Drop for LiveThumb {
    fn drop(&mut self) {
        unsafe {
            let _ = DwmUnregisterThumbnail(self.handle);
            let _ = DestroyWindow(self.host);
        }
    }
}

/// Click-through, no-activate, zero-idle-CPU wndproc for each live DWM thumbnail host.
unsafe extern "system" fn dwm_thumb_wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);
            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

/// Register both `Halo.DockPreview` (glass card) and `Halo.DockPreviewThumb` (live DWM host).
pub fn register(wndproc: crate::window::WndProc) -> Result<()> {
    register_class(PREVIEW_CLASS, wndproc)?;
    register_class(DWM_THUMB_CLASS, dwm_thumb_wndproc)
}

/// Compute screen pixel rect `(x, y, w, h)` for the preview strip centered above `anchor_screen`.
pub fn preview_surface_rect(mon: &MonitorInfo, anchor_screen: Rect, count: usize) -> (i32, i32, i32, i32) {
    let n = count.clamp(1, PREVIEW_MAX_CARDS);
    let scale = (mon.dpi as f32 / 96.0).max(1.0);
    let max_work_dip = ((mon.work.right - mon.work.left) as f32 / scale - 16.0).max(CARD_W + 2.0 * OUTER_PAD);
    let card_w = ((max_work_dip - 2.0 * OUTER_PAD - CARD_GAP * (n.saturating_sub(1) as f32)) / n as f32)
        .min(CARD_W)
        .max(136.0);
    let w_dip = 2.0 * OUTER_PAD + card_w * n as f32 + CARD_GAP * (n.saturating_sub(1) as f32);
    let h_dip = 2.0 * OUTER_PAD + CARD_H;
    let w_px = (w_dip * scale).round() as i32;
    let h_px = (h_dip * scale).round() as i32;
    let anchor_cx = anchor_screen.x + anchor_screen.w * 0.5;
    let margin = (8.0 * scale).round() as i32;
    let min_x = mon.work.left + margin;
    let max_x = (mon.work.right - w_px - margin).max(min_x);
    let x = (anchor_cx - w_px as f32 * 0.5).round() as i32;
    let x = x.clamp(min_x, max_x);
    let gap_px = (ANCHOR_GAP * scale).round() as i32;
    let min_y = mon.work.top + margin;
    let y = (anchor_screen.y.round() as i32 - h_px - gap_px).max(min_y);
    (x, y, w_px, h_px)
}

/// Compute aspect-ratio-preserving destination rect centered inside `well`.
pub fn fit_aspect_rect(well: Rect, src_w: i32, src_h: i32) -> Rect {
    if src_w <= 0 || src_h <= 0 || well.w <= 1.0 || well.h <= 1.0 {
        return well;
    }
    let sw = src_w as f32;
    let sh = src_h as f32;
    let ratio = (well.w / sw).min(well.h / sh).min(1.0);
    let fw = (sw * ratio).max(16.0).min(well.w);
    let fh = (sh * ratio).max(12.0).min(well.h);
    Rect::new(well.x + (well.w - fw) * 0.5, well.y + (well.h - fh) * 0.5, fw, fh)
}

fn card_rect_in(surface_dip: Rect, idx: usize, count: usize) -> Rect {
    let n = count.clamp(1, PREVIEW_MAX_CARDS);
    let total_gaps = CARD_GAP * (n.saturating_sub(1) as f32);
    let cw = ((surface_dip.w - 2.0 * OUTER_PAD - total_gaps) / n as f32).max(120.0);
    let x = OUTER_PAD + idx as f32 * (cw + CARD_GAP);
    Rect::new(x, OUTER_PAD, cw, CARD_H)
}

fn close_btn_rect(card: Rect) -> Rect {
    Rect::new(card.right() - 25.0, card.y + 4.0, 20.0, 20.0)
}

fn thumb_well_rect(card: Rect) -> Rect {
    Rect::new(
        card.x + WELL_PAD_X,
        card.y + HDR_H + 2.0,
        (card.w - 2.0 * WELL_PAD_X).max(20.0),
        (card.h - HDR_H - WELL_PAD_BOTTOM - 2.0).max(20.0),
    )
}

pub fn hit_test_cards(surface_dip: Rect, count: usize, p: Point) -> Option<PreviewHit> {
    let n = count.min(PREVIEW_MAX_CARDS);
    for i in 0..n {
        let cr = card_rect_in(surface_dip, i, n);
        if cr.contains(p) {
            if close_btn_rect(cr).contains(p) {
                return Some(PreviewHit::Close(i));
            }
            return Some(PreviewHit::Card(i));
        }
    }
    None
}

/// Build the preview window list for a `RunningApp` (filtering out dead HWNDs).
pub fn collect_preview_windows<F>(app: &RunningApp, fallback_thumb: F) -> Vec<PreviewWindow>
where
    F: Fn(HWND) -> Option<Image>,
{
    let mut out = Vec::new();
    let pairs: Vec<(HWND, String)> = if !app.windows.is_empty() {
        app.windows.clone()
    } else if !app.hwnd.is_invalid() {
        vec![(app.hwnd, app.name.clone())]
    } else {
        Vec::new()
    };
    for (hwnd, title) in pairs {
        if hwnd.is_invalid() {
            continue;
        }
        let alive = cfg!(test) || unsafe { IsWindow(hwnd).as_bool() };
        if !alive {
            continue;
        }
        let minimized = if cfg!(test) {
            app.minimized
        } else {
            unsafe { IsIconic(hwnd).as_bool() }
        };
        let focused = if cfg!(test) {
            app.focused && hwnd == app.hwnd
        } else {
            unsafe { GetForegroundWindow() == hwnd }
        };
        let label = if title.trim().is_empty() {
            app.name.clone()
        } else {
            title
        };
        out.push(PreviewWindow {
            hwnd,
            title: label,
            minimized,
            focused,
            fallback_thumb: fallback_thumb(hwnd),
        });
        if out.len() >= PREVIEW_MAX_CARDS {
            break;
        }
    }
    out
}

pub struct DockPreview {
    live_thumbs: Vec<LiveThumb>,
    gfx: Option<Gfx>,
    pub hwnd: HWND,
    pub app_key: String,
    app_name: String,
    app_icon: Option<Image>,
    anchor_screen: Rect,
    at: (i32, i32, i32, i32),
    dpi: u32,
    look: &'static Look,
    windows: Vec<PreviewWindow>,
    hovered: Option<PreviewHit>,
    tracking: bool,
    shown: bool,
    dirty: bool,
    thumbs_dirty: bool,
}

impl Drop for DockPreview {
    fn drop(&mut self) {
        // Unregister and destroy all live DWM thumbnails first.
        self.live_thumbs.clear();
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

impl DockPreview {
    pub fn open(
        mon: &MonitorInfo,
        app: &RunningApp,
        windows: Vec<PreviewWindow>,
        anchor_screen: Rect,
        look: &'static Look,
        shared: Option<SharedD3d>,
    ) -> Result<Self> {
        let count = windows.len().clamp(1, PREVIEW_MAX_CARDS);
        let (x, y, w, h) = preview_surface_rect(mon, anchor_screen, count);
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                PREVIEW_CLASS,
                w!("Halo Window Preview"),
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
            live_thumbs: Vec::new(),
            gfx: Some(gfx),
            hwnd,
            app_key: app.key.clone(),
            app_name: app.name.clone(),
            app_icon: app.icon.clone(),
            anchor_screen,
            at: (x, y, w, h),
            dpi: mon.dpi,
            look,
            windows,
            hovered: None,
            tracking: false,
            shown: false,
            dirty: true,
            thumbs_dirty: true,
        })
    }

    pub fn set_look(&mut self, look: &'static Look) -> bool {
        if self.look == look {
            return false;
        }
        self.look = look;
        self.dirty = true;
        true
    }

    pub fn needs_frame(&self) -> bool {
        self.dirty || self.thumbs_dirty
    }

    fn scale(&self) -> f32 {
        (self.dpi as f32 / 96.0).max(1.0)
    }

    fn surface_dip(&self) -> Rect {
        let s = self.scale();
        Rect::new(0.0, 0.0, self.at.2 as f32 / s, self.at.3 as f32 / s)
    }

    /// Update the preview's window list and anchor in-place. Returns `false` if no windows remain.
    pub fn update_windows(
        &mut self,
        mon: &MonitorInfo,
        app: &RunningApp,
        windows: Vec<PreviewWindow>,
        anchor_screen: Rect,
    ) -> bool {
        if windows.is_empty() {
            return false;
        }
        let hwnds_changed = self.windows.len() != windows.len()
            || self.windows.iter().zip(windows.iter()).any(|(a, b)| a.hwnd != b.hwnd);
        let meta_changed = hwnds_changed
            || self.app_key != app.key
            || self.windows.iter().zip(windows.iter()).any(|(a, b)| {
                a.title != b.title || a.minimized != b.minimized || a.focused != b.focused
            });
        let next_at = preview_surface_rect(mon, anchor_screen, windows.len());
        let rect_changed = next_at != self.at;
        self.app_key = app.key.clone();
        self.app_name = app.name.clone();
        self.app_icon = app.icon.clone();
        self.anchor_screen = anchor_screen;
        self.windows = windows;
        if self.hovered.is_some_and(|h| match h {
            PreviewHit::Card(i) | PreviewHit::Close(i) => i >= self.windows.len(),
        }) {
            self.hovered = None;
        }
        if rect_changed {
            self.at = next_at;
            self.dpi = mon.dpi;
            if let Some(gfx) = self.gfx.as_mut() {
                gfx.set_dpi(mon.dpi as f32);
                let _ = gfx.set_surface(next_at.2, next_at.3);
            }
            unsafe {
                let _ = SetWindowPos(
                    self.hwnd,
                    HWND_TOPMOST,
                    next_at.0,
                    next_at.1,
                    next_at.2,
                    next_at.3,
                    SWP_NOACTIVATE,
                );
            }
        }
        if hwnds_changed || rect_changed {
            self.thumbs_dirty = true;
            self.dirty = true;
        } else if meta_changed {
            self.dirty = true;
        }
        true
    }

    /// True if screen cursor `pt` is inside the preview card, the hovered dock slot, or
    /// the vertical bridge corridor between them (prevents flicker while moving mouse up).
    pub fn contains_screen_point_or_bridge(&self, pt: POINT) -> bool {
        bridge_contains_point(self.at, self.anchor_screen, pt)
    }

    pub fn handle_message(&mut self, hwnd: HWND, msg: u32, _wp: WPARAM, lp: LPARAM) -> (Option<LRESULT>, PreviewAction) {
        if hwnd != self.hwnd {
            return (None, PreviewAction::None);
        }
        let to_dip = |lp: LPARAM, scale: f32| {
            let x = (lp.0 & 0xffff) as u16 as i16 as f32;
            let y = ((lp.0 >> 16) & 0xffff) as u16 as i16 as f32;
            Point::new(x / scale, y / scale)
        };
        match msg {
            WM_MOUSEACTIVATE => (Some(LRESULT(MA_NOACTIVATE as isize)), PreviewAction::None),
            WM_MOUSEMOVE => {
                let was_tracking = self.tracking;
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
                let next = hit_test_cards(self.surface_dip(), self.windows.len(), to_dip(lp, self.scale()));
                if next != self.hovered {
                    self.hovered = next;
                    self.dirty = true;
                    return (
                        Some(LRESULT(0)),
                        if !was_tracking {
                            PreviewAction::MouseEnter
                        } else {
                            PreviewAction::RequestFrame
                        },
                    );
                }
                (
                    Some(LRESULT(0)),
                    if !was_tracking {
                        PreviewAction::MouseEnter
                    } else {
                        PreviewAction::None
                    },
                )
            }
            WM_MOUSELEAVE => {
                self.tracking = false;
                if self.hovered.take().is_some() {
                    self.dirty = true;
                }
                (Some(LRESULT(0)), PreviewAction::MouseLeave)
            }
            WM_LBUTTONUP => {
                let hit = hit_test_cards(self.surface_dip(), self.windows.len(), to_dip(lp, self.scale()));
                match hit {
                    Some(PreviewHit::Close(i)) => {
                        if let Some(w) = self.windows.get(i) {
                            return (Some(LRESULT(0)), PreviewAction::CloseWindow(w.hwnd));
                        }
                    }
                    Some(PreviewHit::Card(i)) => {
                        if let Some(w) = self.windows.get(i) {
                            return (
                                Some(LRESULT(0)),
                                PreviewAction::FocusWindow {
                                    hwnd: w.hwnd,
                                    minimized: w.minimized,
                                    icon_rect: self.anchor_screen,
                                },
                            );
                        }
                    }
                    None => {}
                }
                (Some(LRESULT(0)), PreviewAction::None)
            }
            WM_MBUTTONUP => {
                let hit = hit_test_cards(self.surface_dip(), self.windows.len(), to_dip(lp, self.scale()));
                if let Some(PreviewHit::Card(i) | PreviewHit::Close(i)) = hit {
                    if let Some(w) = self.windows.get(i) {
                        return (Some(LRESULT(0)), PreviewAction::CloseWindow(w.hwnd));
                    }
                }
                (Some(LRESULT(0)), PreviewAction::None)
            }
            WM_RBUTTONUP => (Some(LRESULT(0)), PreviewAction::Dismiss),
            WM_SETCURSOR if (lp.0 & 0xffff) as u32 == HTCLIENT => {
                unsafe {
                    let id = if self.hovered.is_some() { IDC_HAND } else { IDC_ARROW };
                    if let Ok(c) = LoadCursorW(None, id) {
                        SetCursor(c);
                    }
                }
                (Some(LRESULT(1)), PreviewAction::None)
            }
            _ => (None, PreviewAction::None),
        }
    }

    fn has_live_thumb(&self, card_idx: usize) -> bool {
        self.live_thumbs.iter().any(|t| t.card_idx == card_idx)
    }

    fn sync_dwm_thumbs(&mut self) {
        self.live_thumbs.clear();
        self.thumbs_dirty = false;
        if cfg!(test) {
            return;
        }
        let owner = self
            .gfx
            .as_ref()
            .and_then(|g| g.visual_hwnd())
            .unwrap_or(self.hwnd);
        let Ok(hinst) = (unsafe { GetModuleHandleW(None) }) else {
            return;
        };
        let scale = self.scale();
        let surf = self.surface_dip();
        let n = self.windows.len().min(PREVIEW_MAX_CARDS);
        let (sx, sy, _, _) = self.at;

        for i in 0..n {
            let win = &self.windows[i];
            if win.hwnd.is_invalid() || !unsafe { IsWindow(win.hwnd).as_bool() } {
                continue;
            }
            let card = card_rect_in(surf, i, n);
            let well = thumb_well_rect(card);
            let well_px = Rect::new(
                sx as f32 + well.x * scale,
                sy as f32 + well.y * scale,
                (well.w * scale).round().max(16.0),
                (well.h * scale).round().max(12.0),
            );
            let host = match unsafe {
                CreateWindowExW(
                    WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                    DWM_THUMB_CLASS,
                    w!("Halo Live Preview"),
                    WS_POPUP,
                    well_px.x.round() as i32,
                    well_px.y.round() as i32,
                    well_px.w.round() as i32,
                    well_px.h.round() as i32,
                    owner,
                    None,
                    hinst,
                    None,
                )
            } {
                Ok(h) => h,
                Err(_) => continue,
            };

            unsafe {
                let no_trans: BOOL = BOOL(1);
                let _ = DwmSetWindowAttribute(
                    host,
                    DWMWA_TRANSITIONS_FORCEDISABLED,
                    &no_trans as *const _ as *const _,
                    std::mem::size_of::<BOOL>() as u32,
                );
                let corner = DWMWCP_ROUNDSMALL;
                let _ = DwmSetWindowAttribute(
                    host,
                    DWMWA_WINDOW_CORNER_PREFERENCE,
                    &corner as *const _ as *const _,
                    std::mem::size_of_val(&corner) as u32,
                );
            }

            let Ok(handle) = (unsafe { DwmRegisterThumbnail(host, win.hwnd) }) else {
                unsafe {
                    let _ = DestroyWindow(host);
                }
                continue;
            };

            let src_sz = unsafe { DwmQueryThumbnailSourceSize(handle) }.ok();
            let Some(sz) = src_sz.filter(|s| s.cx > 0 && s.cy > 0) else {
                unsafe {
                    let _ = DwmUnregisterThumbnail(handle);
                    let _ = DestroyWindow(host);
                }
                continue;
            };

            let fitted = fit_aspect_rect(well_px, sz.cx, sz.cy);
            let fx = fitted.x.round() as i32;
            let fy = fitted.y.round() as i32;
            let fw = fitted.w.round().max(16.0) as i32;
            let fh = fitted.h.round().max(12.0) as i32;

            let props = DWM_THUMBNAIL_PROPERTIES {
                dwFlags: DWM_TNP_RECTDESTINATION
                    | DWM_TNP_VISIBLE
                    | DWM_TNP_OPACITY
                    | DWM_TNP_SOURCECLIENTAREAONLY,
                rcDestination: RECT { left: 0, top: 0, right: fw, bottom: fh },
                rcSource: RECT::default(),
                opacity: 255,
                fVisible: BOOL(1),
                fSourceClientAreaOnly: BOOL(0),
            };

            if unsafe { DwmUpdateThumbnailProperties(handle, &props) }.is_err() {
                unsafe {
                    let _ = DwmUnregisterThumbnail(handle);
                    let _ = DestroyWindow(host);
                }
                continue;
            }

            unsafe {
                let _ = SetWindowPos(
                    host,
                    HWND_TOPMOST,
                    fx,
                    fy,
                    fw,
                    fh,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
            self.live_thumbs.push(LiveThumb { host, handle, card_idx: i });
        }
    }

    pub fn paint(&mut self, shared: Option<SharedD3d>) {
        if !self.dirty && !self.thumbs_dirty {
            return;
        }
        let (x, y, w, h) = self.at;
        let scale = self.scale();
        let surf = self.surface_dip();
        let look = self.look;
        let light = look.light;
        let hovered = self.hovered;
        let n = self.windows.len().min(PREVIEW_MAX_CARDS);

        // Pre-query which cards currently have live DWM thumbs (first paint syncs them right after present).
        let live_flags: Vec<bool> = (0..n).map(|i| self.has_live_thumb(i)).collect();
        let first_sync = self.thumbs_dirty;

        let Some(gfx) = self.gfx.as_mut() else { return };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if gfx.set_surface(w, h).is_err() {
            return;
        }
        gfx.set_look(look);
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        let body = surf.inset(1.0, 1.0);
        let glass_params = if look.material == look::Material::Glass {
            let gp = dock_glass_params(body, OUTER_R, look, scale);
            gfx.prepare_glass(&gp, full);
            Some(gp)
        } else {
            None
        };

        if let Ok(mut c) = gfx.begin(full) {
            c.unthemed(|c| {
                let ink = look.ink;
                let sub = ink.with_alpha(0.56);
                let accent = look.accent.unwrap_or(Color::hex(0x0A84FF));
                if let Some(gp) = glass_params.as_ref() {
                    c.glass(gp, surf);
                } else {
                    crate::pill::shape::draw_body(c, body, OUTER_R, look.ground, Some(look.border));
                }

                let title_st = TextStyle::new(11.5).weight(Weight::Semibold).color(ink);
                let badge_st = TextStyle::new(9.5).weight(Weight::Medium).center().color(sub);

                for i in 0..n {
                    let win = &self.windows[i];
                    let cr = card_rect_in(surf, i, n);
                    let is_card_h = matches!(hovered, Some(PreviewHit::Card(k) | PreviewHit::Close(k)) if k == i);
                    let is_close_h = hovered == Some(PreviewHit::Close(i));

                    let card_bg = if is_card_h {
                        accent.with_alpha(if light { 0.14 } else { 0.20 })
                    } else if light {
                        Color::black(0.04)
                    } else {
                        Color::white(0.06)
                    };
                    let card_rim = if is_card_h {
                        accent.with_alpha(0.62)
                    } else if win.focused {
                        accent.with_alpha(0.36)
                    } else if light {
                        Color::black(0.10)
                    } else {
                        Color::white(0.14)
                    };
                    c.fill_rrect(cr, CARD_R, card_bg);
                    c.stroke_rrect(cr, CARD_R, card_rim, 1.0);

                    // Header: app icon + window title + close button [x]
                    let ico_r = Rect::new(cr.x + 8.0, cr.y + 6.0, 16.0, 16.0);
                    if let Some(img) = self.app_icon.as_ref() {
                        c.image_hq(img, ico_r);
                    } else {
                        c.icon(Icon::AppWindow, ico_r, accent);
                    }

                    let close_r = close_btn_rect(cr);
                    let title_x = ico_r.right() + 6.0;
                    let title_w = (close_r.x - title_x - 4.0).max(24.0);
                    c.text(&win.title, Rect::new(title_x, cr.y + 6.5, title_w, 16.0), &title_st);

                    if is_card_h || is_close_h {
                        let close_bg = if is_close_h {
                            Color::hex(0xEF4444).with_alpha(0.92)
                        } else if light {
                            Color::black(0.08)
                        } else {
                            Color::white(0.12)
                        };
                        c.fill_rrect(close_r, 6.0, close_bg);
                        let x_col = if is_close_h { Color::white(0.98) } else { ink.with_alpha(0.76) };
                        draw_close_x(c, close_r, x_col);
                    }

                    // Thumbnail well (behind live DWM host, or renders cached fallback / icon)
                    let well = thumb_well_rect(cr);
                    let well_bg = if light { Color::black(0.07) } else { Color::black(0.32) };
                    let well_rim = if light { Color::black(0.08) } else { Color::white(0.09) };
                    c.fill_rrect(well, 8.0, well_bg);
                    c.stroke_rrect(well, 8.0, well_rim, 1.0);

                    if !first_sync && !live_flags[i] {
                        if let Some(img) = win.fallback_thumb.as_ref() {
                            c.image_hq(img, well.inset(1.0, 1.0));
                        } else {
                            let big_ico = Rect::new(
                                well.x + (well.w - 38.0) * 0.5,
                                well.y + (well.h - 38.0) * 0.5 - if win.minimized { 8.0 } else { 0.0 },
                                38.0,
                                38.0,
                            );
                            if let Some(img) = self.app_icon.as_ref() {
                                c.image_hq(img, big_ico);
                            } else {
                                c.icon(Icon::AppWindow, big_ico, sub);
                            }
                            if win.minimized {
                                c.text(
                                    "Minimized",
                                    Rect::new(well.x + 6.0, big_ico.bottom() + 4.0, well.w - 12.0, 14.0),
                                    &badge_st,
                                );
                            }
                        }
                    }
                }
            });
            let _ = gfx.end();
        }
        let _ = gfx.present(x, y);
        if gfx.needs_hit_mask() {
            let mask = HitMask { rect: None, notch: None, pill: Some((surf, OUTER_R)) };
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

        if first_sync {
            self.sync_dwm_thumbs();
            // If any card couldn't attach a live DWM thumbnail, repaint once so its fallback image/icon shows.
            let missing_live = (0..n).any(|i| !self.has_live_thumb(i));
            if missing_live {
                self.dirty = true;
                self.paint(None);
            }
        }
    }
}

fn draw_close_x(c: &mut Canvas<'_>, r: Rect, col: Color) {
    let cx = r.x + r.w * 0.5;
    let cy = r.y + r.h * 0.5;
    let d = 3.6;
    let segs = [
        Seg::MoveTo(cx - d, cy - d),
        Seg::LineTo(cx + d, cy + d),
        Seg::MoveTo(cx + d, cy - d),
        Seg::LineTo(cx - d, cy + d),
    ];
    c.path(&segs, None, Some((col, 1.5)));
}

pub fn bridge_contains_point(preview_at: (i32, i32, i32, i32), anchor: Rect, pt: POINT) -> bool {
    let (px, py, pw, ph) = preview_at;
    let in_preview = pt.x >= px - 4 && pt.x <= px + pw + 4 && pt.y >= py - 4 && pt.y <= py + ph + 4;
    if in_preview {
        return true;
    }
    let ax0 = anchor.x.floor() as i32 - 12;
    let ay0 = anchor.y.floor() as i32 - 8;
    let ax1 = (anchor.x + anchor.w).ceil() as i32 + 12;
    let ay1 = (anchor.y + anchor.h).ceil() as i32 + 16;
    if pt.x >= ax0 && pt.x <= ax1 && pt.y >= ay0 && pt.y <= ay1 {
        return true;
    }
    // Vertical bridge corridor connecting preview bottom to dock icon top.
    let bx0 = px.min(ax0) - 10;
    let bx1 = (px + pw).max(ax1) + 10;
    let by0 = py + ph - 2;
    let by1 = ay0 + 6;
    by1 >= by0 && pt.x >= bx0 && pt.x <= bx1 && pt.y >= by0 && pt.y <= by1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_mon() -> MonitorInfo {
        MonitorInfo {
            handle: 0,
            rect: RECT { left: 0, top: 0, right: 1920, bottom: 1080 },
            work: RECT { left: 0, top: 0, right: 1920, bottom: 1040 },
            dpi: 96,
        }
    }

    #[test]
    fn preview_surface_clamps_to_monitor() {
        let mon = test_mon();
        let left_anchor = Rect::new(10.0, 990.0, 48.0, 48.0);
        let (x, y, w, h) = preview_surface_rect(&mon, left_anchor, 3);
        assert!(x >= mon.work.left + 8);
        assert!(x + w <= mon.work.right - 8);
        assert!(y >= mon.work.top + 8);
        assert!(y + h < left_anchor.y as i32);

        let right_anchor = Rect::new(1860.0, 990.0, 48.0, 48.0);
        let (rx, _, rw, _) = preview_surface_rect(&mon, right_anchor, 4);
        assert!(rx + rw <= mon.work.right - 8);
    }

    #[test]
    fn fit_aspect_rect_preserves_ratio_and_centers() {
        let well = Rect::new(10.0, 30.0, 200.0, 100.0);
        let wide = fit_aspect_rect(well, 1920, 1080);
        assert!((wide.w / wide.h - 16.0 / 9.0).abs() < 0.05);
        assert!((wide.y - well.y).abs() < 0.5);
        assert!(wide.x > well.x);

        let tall = fit_aspect_rect(well, 400, 800);
        assert!((tall.w / tall.h - 0.5).abs() < 0.05);
        assert!((tall.h - well.h).abs() < 0.5);
        assert!(tall.x > well.x);
    }

    #[test]
    fn card_hit_test_distinguishes_card_and_close_button() {
        let surf = Rect::new(0.0, 0.0, 2.0 * OUTER_PAD + 2.0 * CARD_W + CARD_GAP, 2.0 * OUTER_PAD + CARD_H);
        let c0 = card_rect_in(surf, 0, 2);
        let c1 = card_rect_in(surf, 1, 2);
        let close1 = close_btn_rect(c1);
        assert_eq!(
            hit_test_cards(surf, 2, Point::new(c0.x + 40.0, c0.y + 80.0)),
            Some(PreviewHit::Card(0))
        );
        assert_eq!(
            hit_test_cards(surf, 2, Point::new(close1.x + 10.0, close1.y + 10.0)),
            Some(PreviewHit::Close(1))
        );
        assert_eq!(hit_test_cards(surf, 2, Point::new(2.0, 2.0)), None);
    }

    #[test]
    fn bridge_contains_gap_between_preview_and_anchor() {
        let preview_at = (400, 800, 236, 176); // bottom = 976
        let anchor = Rect::new(494.0, 990.0, 48.0, 48.0); // top = 990
        assert!(bridge_contains_point(preview_at, anchor, POINT { x: 518, y: 982 }));
        assert!(bridge_contains_point(preview_at, anchor, POINT { x: 450, y: 860 }));
        assert!(bridge_contains_point(preview_at, anchor, POINT { x: 510, y: 1010 }));
        assert!(!bridge_contains_point(preview_at, anchor, POINT { x: 100, y: 500 }));
    }
}

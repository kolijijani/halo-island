//! Smooth 72-slice S-curve Genie minimize/restore overlay for the bottom dock.
//!
//! Zero cost at rest: the click-through overlay HWND (`Halo.DockGenie`) and its
//! captured window bitmap exist only for the ~340 ms duration of a minimize or
//! restore transition, and are dropped immediately on completion.

use crate::gfx::{Gfx, Image, ImageData, SharedD3d};
use crate::window::MonitorInfo;
use halo_core::geom::Rect;
use halo_core::look;
use windows::core::{w, Result, PCWSTR};
use windows::Win32::Foundation::{BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DwmSetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS, DWMWA_TRANSITIONS_FORCEDISABLED,
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject, SetStretchBltMode, StretchBlt, BI_RGB,
    BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, HALFTONE, HDC, HGDIOBJ, SRCCOPY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

#[link(name = "user32")]
unsafe extern "system" {
    fn PrintWindow(hwnd: HWND, hdcblt: HDC, nflags: u32) -> BOOL;
}

pub const CLASS: PCWSTR = w!("Halo.DockGenie");
pub const GENIE_SLICES: usize = 72;
pub const GENIE_DURATION: f32 = 0.34;
const MAX_CAPTURE_PX: i32 = 640;
const PW_RENDERFULLCONTENT: u32 = 0x0000_0002;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenieDir {
    Minimize,
    Restore,
}

#[derive(Clone, Copy, Debug)]
pub struct GenieSlice {
    pub src: Rect,
    pub dst: Rect,
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Compute the 72 horizontal S-curve slices mapping `src_win` into `dst_icon`
/// at normalized animation time `t` (`0.0..=1.0`).
pub fn genie_slices(
    t: f32,
    dir: GenieDir,
    src_win: Rect,
    dst_icon: Rect,
    img_w: f32,
    img_h: f32,
) -> Vec<GenieSlice> {
    let p = match dir {
        GenieDir::Minimize => t.clamp(0.0, 1.0),
        GenieDir::Restore => 1.0 - t.clamp(0.0, 1.0),
    };
    let bend = smoothstep(0.0, 0.55, p);
    let suck = smoothstep(0.26, 1.0, p);

    let y_top = lerp(src_win.y, dst_icon.y, suck.powf(1.18));
    let y_bot = lerp(src_win.bottom(), dst_icon.bottom(), suck.powf(0.62));

    let mut out = Vec::with_capacity(GENIE_SLICES);
    let n = GENIE_SLICES as f32;
    for i in 0..GENIE_SLICES {
        let u0 = i as f32 / n;
        let u1 = (i + 1) as f32 / n;
        let u = (u0 + u1) * 0.5;

        // S-curve funnel: bottom edge (u -> 1) pinches first into the dock icon.
        let pinch = smoothstep(0.0, 1.0, u).powf(1.35) * bend;
        let k = (pinch + suck * (1.0 - pinch) * 0.94).clamp(0.0, 1.0);

        let x0 = lerp(src_win.x, dst_icon.x, k);
        let x1 = lerp(src_win.right(), dst_icon.right(), k);
        let sy0 = lerp(y_top, y_bot, u0);
        let sy1 = lerp(y_top, y_bot, u1);

        out.push(GenieSlice {
            src: Rect::new(0.0, u0 * img_h, img_w, (u1 - u0) * img_h),
            dst: Rect::new(x0, sy0, (x1 - x0).max(1.0), (sy1 - sy0 + 0.6).max(1.0)),
        });
    }
    out
}

/// Register the transient click-through `Halo.DockGenie` overlay class once.
pub fn register() -> Result<()> {
    unsafe extern "system" fn genie_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        match msg {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
            _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
        }
    }
    unsafe {
        let hinst = GetModuleHandleW(None)?;
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(genie_proc),
            hInstance: hinst.into(),
            lpszClassName: CLASS,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(windows::core::Error::from_win32());
        }
    }
    Ok(())
}

/// Capture a downscaled premultiplied-BGRA snapshot of `hwnd` plus its screen bounds.
pub fn capture_window(hwnd: HWND) -> Option<(Image, RECT)> {
    unsafe {
        if hwnd.is_invalid() || !IsWindow(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return None;
        }
        let mut rc = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rc as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
        {
            GetWindowRect(hwnd, &mut rc).ok()?;
        }
        let w = (rc.right - rc.left).max(0);
        let h = (rc.bottom - rc.top).max(0);
        if w < 16 || h < 16 {
            return None;
        }

        // Capture full window into an off-screen DIB, then halftone-downscale if large.
        let src_dc = CreateCompatibleDC(None);
        if src_dc.is_invalid() {
            return None;
        }
        let src_bmi = dib_info(w, h);
        let mut src_bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let Ok(src_bmp) = CreateDIBSection(src_dc, &src_bmi, DIB_RGB_COLORS, &mut src_bits, None, 0) else {
            let _ = DeleteDC(src_dc);
            return None;
        };
        if src_bits.is_null() {
            let _ = DeleteObject(HGDIOBJ(src_bmp.0));
            let _ = DeleteDC(src_dc);
            return None;
        }
        let prev_src = SelectObject(src_dc, HGDIOBJ(src_bmp.0));
        let ok = PrintWindow(hwnd, src_dc, PW_RENDERFULLCONTENT).as_bool()
            || PrintWindow(hwnd, src_dc, 0).as_bool();
        if !ok {
            SelectObject(src_dc, prev_src);
            let _ = DeleteObject(HGDIOBJ(src_bmp.0));
            let _ = DeleteDC(src_dc);
            return None;
        }

        let scale = (MAX_CAPTURE_PX as f32 / w.max(h) as f32).min(1.0);
        let dw = ((w as f32 * scale).round() as i32).max(16);
        let dh = ((h as f32 * scale).round() as i32).max(16);

        let mut pixels = vec![0u8; (dw * dh * 4) as usize];
        if dw == w && dh == h {
            std::ptr::copy_nonoverlapping(src_bits as *const u8, pixels.as_mut_ptr(), pixels.len());
        } else {
            let dst_dc = CreateCompatibleDC(None);
            let dst_bmi = dib_info(dw, dh);
            let mut dst_bits: *mut std::ffi::c_void = std::ptr::null_mut();
            if !dst_dc.is_invalid() {
                if let Ok(dst_bmp) = CreateDIBSection(dst_dc, &dst_bmi, DIB_RGB_COLORS, &mut dst_bits, None, 0) {
                    if !dst_bits.is_null() {
                        let prev_dst = SelectObject(dst_dc, HGDIOBJ(dst_bmp.0));
                        let _ = SetStretchBltMode(dst_dc, HALFTONE);
                        let _ = StretchBlt(dst_dc, 0, 0, dw, dh, src_dc, 0, 0, w, h, SRCCOPY);
                        std::ptr::copy_nonoverlapping(dst_bits as *const u8, pixels.as_mut_ptr(), pixels.len());
                        SelectObject(dst_dc, prev_dst);
                    }
                    let _ = DeleteObject(HGDIOBJ(dst_bmp.0));
                }
                let _ = DeleteDC(dst_dc);
            }
        }

        SelectObject(src_dc, prev_src);
        let _ = DeleteObject(HGDIOBJ(src_bmp.0));
        let _ = DeleteDC(src_dc);

        // PrintWindow leaves alpha = 0 on opaque GDI/DX windows; force opaque alpha.
        let mut non_black = false;
        for px in pixels.chunks_exact_mut(4) {
            if px[0] != 0 || px[1] != 0 || px[2] != 0 {
                non_black = true;
            }
            px[3] = 255;
        }
        if !non_black {
            return None;
        }
        Some((ImageData::new(dw as u32, dh as u32, pixels), rc))
    }
}

fn dib_info(w: i32, h: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Query the restored (normal) screen bounds of `hwnd` even while it is minimized (`IsIconic`).
pub fn restored_window_rect(hwnd: HWND) -> Option<RECT> {
    unsafe {
        if hwnd.is_invalid() || !IsWindow(hwnd).as_bool() {
            return None;
        }
        let mut wp = WINDOWPLACEMENT {
            length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
            ..Default::default()
        };
        GetWindowPlacement(hwnd, &mut wp).ok()?;
        let rc = wp.rcNormalPosition;
        if rc.right - rc.left >= 16 && rc.bottom - rc.top >= 16 {
            Some(rc)
        } else {
            None
        }
    }
}

/// Minimize `hwnd` with Windows DWM's built-in transition disabled so only our Genie plays.
pub fn minimize_window_no_dwm(hwnd: HWND) {
    unsafe {
        if hwnd.is_invalid() || !IsWindow(hwnd).as_bool() {
            return;
        }
        let off: BOOL = BOOL(1);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &off as *const BOOL as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );
        let _ = ShowWindow(hwnd, SW_MINIMIZE);
        let on: BOOL = BOOL(0);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &on as *const BOOL as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );
    }
}

/// Restore `hwnd` instantly at the end of a Genie restore animation without a second DWM animation.
pub fn restore_window_no_dwm(hwnd: HWND) {
    unsafe {
        if hwnd.is_invalid() || !IsWindow(hwnd).as_bool() {
            return;
        }
        let off: BOOL = BOOL(1);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &off as *const BOOL as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let _ = SetForegroundWindow(hwnd);
        let on: BOOL = BOOL(0);
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED,
            &on as *const BOOL as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );
    }
}

pub struct DockGenie {
    gfx: Option<Gfx>,
    hwnd: HWND,
    at: (i32, i32, i32, i32),
    img: Image,
    src_win: Rect,
    dst_icon: Rect,
    dir: GenieDir,
    elapsed: f32,
    shown: bool,
    restore_target: Option<HWND>,
}

impl Drop for DockGenie {
    fn drop(&mut self) {
        if let Some(target) = self.restore_target.take() {
            restore_window_no_dwm(target);
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

impl DockGenie {
    /// Start a transient Genie animation covering `mon` from `win_screen` to `icon_screen`.
    pub fn start(
        mon: &MonitorInfo,
        img: Image,
        win_screen: RECT,
        icon_screen: Rect,
        dir: GenieDir,
        shared: Option<SharedD3d>,
    ) -> Result<Self> {
        let x = mon.rect.left;
        let y = mon.rect.top;
        let w = (mon.rect.right - mon.rect.left).max(1);
        let h = (mon.rect.bottom - mon.rect.top).max(1);
        let scale = mon.scale().max(1.0);

        let src_win = Rect::new(
            (win_screen.left - x) as f32 / scale,
            (win_screen.top - y) as f32 / scale,
            (win_screen.right - win_screen.left).max(1) as f32 / scale,
            (win_screen.bottom - win_screen.top).max(1) as f32 / scale,
        );
        let dst_icon = Rect::new(
            (icon_screen.x - x as f32) / scale,
            (icon_screen.y - y as f32) / scale,
            (icon_screen.w / scale).max(12.0),
            (icon_screen.h / scale).max(12.0),
        );

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT,
                CLASS,
                w!("Halo Dock Genie"),
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
            img,
            src_win,
            dst_icon,
            dir,
            elapsed: 0.0,
            shown: false,
            restore_target: None,
        })
    }

    /// Attach a minimized window to be restored at the exact end of `GenieDir::Restore`
    /// so the real window never appears underneath while the Genie is still expanding.
    pub fn with_restore_target(mut self, target: HWND) -> Self {
        self.restore_target = Some(target);
        self
    }

    /// Advance and paint one frame. Returns `false` when the animation finishes
    /// so the caller drops `DockGenie` immediately (zero idle cost).
    pub fn tick_and_paint(&mut self, dt: f32, shared: Option<SharedD3d>) -> bool {
        self.elapsed += dt.clamp(0.0, 0.05);
        let t = (self.elapsed / GENIE_DURATION).clamp(0.0, 1.0);
        if t >= 1.0 {
            if let Some(target) = self.restore_target.take() {
                restore_window_no_dwm(target);
            }
            return false;
        }
        let (x, y, w, h) = self.at;
        let Some(gfx) = self.gfx.as_mut() else {
            return false;
        };
        if !gfx.has_gpu() && shared.is_some() {
            gfx.set_shared_d3d(shared);
        }
        if gfx.set_surface(w, h).is_err() {
            return false;
        }
        gfx.set_look(look::glass(false));
        let full = RECT { left: 0, top: 0, right: w, bottom: h };
        let slices = genie_slices(
            t,
            self.dir,
            self.src_win,
            self.dst_icon,
            self.img.width as f32,
            self.img.height as f32,
        );
        let p = match self.dir {
            GenieDir::Minimize => t,
            GenieDir::Restore => 1.0 - t,
        };
        let alpha = if p > 0.84 {
            (1.0 - (p - 0.84) / 0.16).clamp(0.0, 1.0)
        } else {
            1.0
        };
        if let Ok(mut c) = gfx.begin(full) {
            c.unthemed(|c| {
                c.with_alpha(alpha, |c| {
                    for s in &slices {
                        c.image_slice(&self.img, s.src, s.dst);
                    }
                });
            });
            let _ = gfx.end();
        }
        let _ = gfx.present(x, y);
        if !self.shown {
            self.shown = true;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(self.hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
            }
            gfx.set_shown(true);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn genie_slices_start_at_window_and_funnel_into_icon() {
        let win = Rect::new(200.0, 100.0, 900.0, 600.0);
        let icon = Rect::new(920.0, 1000.0, 44.0, 44.0);

        let start = genie_slices(0.0, GenieDir::Minimize, win, icon, 640.0, 426.0);
        assert_eq!(start.len(), GENIE_SLICES);
        assert!((start[0].dst.x - win.x).abs() < 1e-3);
        assert!((start[0].dst.w - win.w).abs() < 1e-3);

        // Mid-animation: bottom slice pinches narrower than top slice (S-curve funnel).
        let mid = genie_slices(0.45, GenieDir::Minimize, win, icon, 640.0, 426.0);
        assert!(mid[GENIE_SLICES - 1].dst.w < mid[0].dst.w, "bottom must funnel narrower than top");

        // End of minimize: converges near icon rect.
        let end = genie_slices(1.0, GenieDir::Minimize, win, icon, 640.0, 426.0);
        assert!((end[0].dst.y - icon.y).abs() < 1.0);
        assert!((end[GENIE_SLICES - 1].dst.right() - icon.right()).abs() < 35.0);

        // Restore at t=1.0 returns to full window rect.
        let restored = genie_slices(1.0, GenieDir::Restore, win, icon, 640.0, 426.0);
        assert!((restored[0].dst.x - win.x).abs() < 1e-3);
        assert!((restored[0].dst.w - win.w).abs() < 1e-3);
    }
}

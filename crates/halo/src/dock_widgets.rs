//! Desktop Widgets (macOS Sonoma / Tahoe style) for Halo Island.
//!
//! 2x2 grid of Liquid Glass tiles sitting flush on the desktop wallpaper (`HWND_BOTTOM`),
//! under open windows. 0% CPU at rest, redraws only when state changes or on 5s tick.
//! Supports Clock, Digital Clock, Date/Calendar, Battery, System (CPU/RAM/Disk), and Hidden.

use std::f32::consts::PI;
use std::time::Instant;

use halo_core::datetime::LocalDateTime;
use halo_core::geom::{Color, Point, Rect};
use halo_core::look::Look;
use windows::core::{w, PCWSTR, Result};
use windows::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::clock;
use crate::gfx::{Align, Canvas, Gfx, SharedD3d, TextStyle, Weight};
use crate::window::MonitorInfo;

pub const WIDGETS_CLASS: PCWSTR = w!("Halo.DockWidgets");

pub const WIDGET_TILE: f32 = 164.0;
pub const WIDGET_GAP: f32 = 14.0;
pub const WIDGET_RADIUS: f32 = 26.0;
pub const WIDGET_MARGIN: f32 = 22.0;

const DAYS: [&str; 7] = ["SUNDAY", "MONDAY", "TUESDAY", "WEDNESDAY", "THURSDAY", "FRIDAY", "SATURDAY"];
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

const RED: Color = Color::hex(0xFF453A);
const GREEN: Color = Color::hex(0x30D158);
const YELLOW: Color = Color::hex(0xFFD60A);
const BLUE: Color = Color::hex(0x0A84FF);
const PURPLE: Color = Color::hex(0xBF5AF2);
const ORANGE: Color = Color::hex(0xFF9F0A);

/// Register `Halo.DockWidgets` window class once at startup.
pub fn register_widgets(wndproc: crate::window::WndProc) -> Result<()> {
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
            lpszClassName: WIDGETS_CLASS,
            ..Default::default()
        };
        if RegisterClassExW(&wc) == 0 {
            return Err(windows::core::Error::from_win32());
        }
    }
    Ok(())
}

fn ft(t: FILETIME) -> u64 {
    ((t.dwHighDateTime as u64) << 32) | t.dwLowDateTime as u64
}

fn query_power() -> (u8, bool, bool) {
    let mut ps = SYSTEM_POWER_STATUS::default();
    if unsafe { GetSystemPowerStatus(&mut ps) }.is_ok() {
        if ps.BatteryFlag != 128 && ps.BatteryLifePercent <= 100 {
            let ac = ps.ACLineStatus == 1;
            return (ps.BatteryLifePercent, ac, ac && (ps.BatteryFlag & 8 != 0));
        }
    }
    (100, true, false)
}

fn query_memory() -> f32 {
    let mut m = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_ok() {
        return m.dwMemoryLoad as f32 / 100.0;
    }
    0.0
}

fn query_disk() -> f32 {
    let mut free = 0u64;
    let mut total = 0u64;
    if unsafe { GetDiskFreeSpaceExW(w!("C:\\"), None, Some(&mut total), Some(&mut free)) }.is_ok() && total > 0 {
        return 1.0 - free as f32 / total as f32;
    }
    0.0
}

#[derive(Clone, Debug, PartialEq)]
pub enum WidgetsAction {
    SaveLayout {
        items: Vec<String>,
        position: String,
        offset: Option<(i32, i32)>,
    },
    OpenSettings,
}

pub struct DockWidgets {
    pub hwnd: HWND,
    gfx: Option<Gfx>,
    dpi: u32,
    light: bool,
    look: &'static Look,
    pub items: Vec<String>,
    pub position: String,
    pub offset: Option<(i32, i32)>,
    pos: (i32, i32),
    size_px: i32,
    dragging: bool,
    drag_start: (i32, i32),
    initial_pos: (i32, i32),
    cpu_prev: Option<(u64, u64)>,
    cpu: f32,
    mem: f32,
    disk: f32,
    last_poll: Instant,
    dirty: bool,
}

impl Drop for DockWidgets {
    fn drop(&mut self) {
        self.gfx.take();
        if !self.hwnd.is_invalid() {
            unsafe {
                let _ = DestroyWindow(self.hwnd);
            }
            self.hwnd = HWND::default();
        }
    }
}

impl DockWidgets {
    pub fn open(
        mon: &MonitorInfo,
        items: Vec<String>,
        position: String,
        offset: Option<(i32, i32)>,
        look: &'static Look,
        shared: Option<SharedD3d>,
    ) -> Result<Self> {
        let scale = mon.scale().max(1.0);
        let size_dip = 2.0 * WIDGET_MARGIN + 2.0 * WIDGET_TILE + WIDGET_GAP;
        let size_px = (size_dip * scale).ceil() as i32;
        let pos = Self::compute_pos(mon, size_px, scale, &position, offset);

        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                WIDGETS_CLASS,
                w!("Halo Desktop Widgets"),
                WS_POPUP,
                pos.0,
                pos.1,
                size_px,
                size_px,
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
        let _ = gfx.set_surface(size_px, size_px);

        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            let _ = SetWindowPos(hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }

        let mut w = Self {
            hwnd,
            gfx: Some(gfx),
            dpi: mon.dpi,
            light: look.light,
            look,
            items,
            position,
            offset,
            pos,
            size_px,
            dragging: false,
            drag_start: (0, 0),
            initial_pos: (0, 0),
            cpu_prev: None,
            cpu: 0.0,
            mem: 0.0,
            disk: 0.0,
            last_poll: Instant::now(),
            dirty: true,
        };
        w.sample_telemetry();
        Ok(w)
    }

    fn compute_pos(mon: &MonitorInfo, size_px: i32, scale: f32, position: &str, offset: Option<(i32, i32)>) -> (i32, i32) {
        let wa = mon.work;
        let margin_gap = ((20.0 - WIDGET_MARGIN) * scale) as i32;
        let top_gap = ((16.0 - WIDGET_MARGIN) * scale) as i32;

        match position {
            "top-left" => (wa.left + margin_gap, wa.top + top_gap),
            "bottom-right" => (wa.right - margin_gap - size_px, wa.bottom - margin_gap - size_px),
            "bottom-left" => (wa.left + margin_gap, wa.bottom - margin_gap - size_px),
            "custom" => {
                if let Some(off) = offset {
                    let max_x = (wa.right - size_px).max(wa.left);
                    let max_y = (wa.bottom - size_px).max(wa.top);
                    (off.0.clamp(wa.left, max_x), off.1.clamp(wa.top, max_y))
                } else {
                    (wa.right - margin_gap - size_px, wa.top + top_gap)
                }
            }
            _ => (wa.right - margin_gap - size_px, wa.top + top_gap),
        }
    }

    pub fn set_raised(&self, raised: bool) {
        if self.hwnd.is_invalid() {
            return;
        }
        let after = if raised { HWND_TOPMOST } else { HWND_BOTTOM };
        unsafe {
            let _ = SetWindowPos(self.hwnd, after, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    pub fn widget_at(&self, slot: usize) -> &str {
        if slot < self.items.len() {
            &self.items[slot]
        } else {
            match slot {
                0 => "clock",
                1 => "date",
                2 => "battery",
                3 => "system",
                _ => "none",
            }
        }
    }

    pub fn tile_rect(&self, slot: usize) -> Rect {
        let (col, row) = (slot % 2, slot / 2);
        let x = WIDGET_MARGIN + col as f32 * (WIDGET_TILE + WIDGET_GAP);
        let y = WIDGET_MARGIN + row as f32 * (WIDGET_TILE + WIDGET_GAP);
        Rect::new(x, y, WIDGET_TILE, WIDGET_TILE)
    }

    pub fn tile_at_point(&self, pt: Point) -> Option<usize> {
        (0..4).find(|&i| {
            if self.widget_at(i) == "none" {
                return false;
            }
            self.tile_rect(i).contains(pt)
        })
    }

    pub fn slot_at_point(&self, pt: Point) -> Option<usize> {
        (0..4).find(|&i| self.tile_rect(i).contains(pt))
    }

    pub fn needs_frame(&self) -> bool {
        self.dirty || self.dragging
    }

    pub fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    pub fn sample_telemetry(&mut self) {
        let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
        if unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) }.is_ok() {
            let id = ft(idle);
            let tot = ft(kernel) + ft(user);
            if let Some((pi, pt)) = self.cpu_prev {
                let dtot = tot.saturating_sub(pt);
                let didle = id.saturating_sub(pi);
                if dtot > 0 {
                    self.cpu = (1.0 - didle as f32 / dtot as f32).clamp(0.0, 1.0);
                }
            }
            self.cpu_prev = Some((id, tot));
        }
        self.mem = query_memory();
        self.disk = query_disk();
        self.last_poll = Instant::now();
    }

    pub fn check_periodic_tick(&mut self) -> bool {
        if self.last_poll.elapsed().as_secs() >= 5 {
            self.sample_telemetry();
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn set_items(&mut self, items: Vec<String>) -> bool {
        if self.items != items {
            self.items = items;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn set_position_and_offset(&mut self, mon: &MonitorInfo, position: String, offset: Option<(i32, i32)>) -> bool {
        if self.position != position || self.offset != offset {
            self.position = position;
            self.offset = offset;
            let scale = mon.scale().max(1.0);
            self.pos = Self::compute_pos(mon, self.size_px, scale, &self.position, self.offset);
            unsafe {
                let _ = SetWindowPos(self.hwnd, HWND_BOTTOM, self.pos.0, self.pos.1, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
            }
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn set_look(&mut self, look: &'static Look) -> bool {
        if self.look.ground != look.ground || self.light != look.light {
            self.look = look;
            self.light = look.light;
            self.dirty = true;
            true
        } else {
            false
        }
    }

    pub fn paint(&mut self, mon: &MonitorInfo, look: &'static Look) {
        let scale = mon.scale().max(1.0);
        let size_dip = 2.0 * WIDGET_MARGIN + 2.0 * WIDGET_TILE + WIDGET_GAP;
        let size_px = (size_dip * scale).ceil() as i32;

        if let Some(gfx) = self.gfx.as_mut() {
            if size_px != self.size_px {
                self.size_px = size_px;
                let _ = gfx.set_surface(size_px, size_px);
            }
        }

        let widgets: [String; 4] = [
            self.widget_at(0).to_string(),
            self.widget_at(1).to_string(),
            self.widget_at(2).to_string(),
            self.widget_at(3).to_string(),
        ];
        let cpu = self.cpu;
        let mem = self.mem;
        let disk = self.disk;
        let pos = self.pos;

        let Some(gfx) = self.gfx.as_mut() else { return };
        let dirty_rect = RECT { left: 0, top: 0, right: self.size_px, bottom: self.size_px };
        let Ok(mut c) = gfx.begin(dirty_rect) else { return };

        let t = clock::local_now();

        for i in 0..4 {
            let kind = &widgets[i];
            if kind == "none" {
                continue;
            }
            let tr = Self::tile_rect_for_index(i);

            // True Halo Island Liquid Glass / Theme card
            if look.material == halo_core::look::Material::Glass {
                let gp = crate::dock::dock_glass_params(tr, WIDGET_RADIUS, look, scale);
                c.glass(&gp, tr);
            } else {
                crate::pill::shape::draw_body(&mut c, tr, WIDGET_RADIUS, look.ground, Some(look.border));
            }

            match kind.as_str() {
                "clock" => Self::draw_analog_clock(&mut c, tr, &t, look),
                "digital_clock" => Self::draw_digital_clock(&mut c, tr, &t, look),
                "date" | "calendar" => Self::draw_date_tile(&mut c, tr, &t, look),
                "battery" => Self::draw_battery_tile(&mut c, tr, look),
                "system" => Self::draw_system_tile_stats(&mut c, tr, look, cpu, mem, disk),
                _ => {}
            }
        }

        drop(c);
        let _ = gfx.end();
        let _ = gfx.present(pos.0, pos.1);
        self.dirty = false;

        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }

    pub fn tile_rect_for_index(idx: usize) -> Rect {
        let (col, row) = (idx % 2, idx / 2);
        let x = WIDGET_MARGIN + col as f32 * (WIDGET_TILE + WIDGET_GAP);
        let y = WIDGET_MARGIN + row as f32 * (WIDGET_TILE + WIDGET_GAP);
        Rect::new(x, y, WIDGET_TILE, WIDGET_TILE)
    }

    fn draw_analog_clock(c: &mut Canvas<'_>, tr: Rect, t: &LocalDateTime, _look: &Look) {
        let center = Point::new(tr.x + tr.w * 0.5, tr.y + tr.h * 0.5);
        let r = 62.0;

        // Dial markers
        for i in 0..12 {
            let a = i as f32 * PI / 6.0;
            let (sa, ca) = (a.sin(), -a.cos());
            let r0 = if i % 3 == 0 { r - 9.0 } else { r - 5.0 };
            let p0 = Point::new(center.x + r0 * sa, center.y + r0 * ca);
            let p1 = Point::new(center.x + r * sa, center.y + r * ca);
            let w = if i % 3 == 0 { 2.5 } else { 1.5 };
            let col = if i % 3 == 0 { Color::white(0.9) } else { Color::white(0.5) };
            c.line(p0, p1, col, w);
        }

        let hr = (t.hour % 12) as f32 + t.minute as f32 / 60.0;
        let min = t.minute as f32 + t.second as f32 / 60.0;
        let sec = t.second as f32;

        // Hands
        Self::draw_hand(c, center, hr * PI / 6.0, 36.0, 4.0, Color::white(1.0), 6.0);
        Self::draw_hand(c, center, min * PI / 30.0, 52.0, 2.5, Color::white(1.0), 8.0);
        Self::draw_hand(c, center, sec * PI / 30.0, 56.0, 1.5, ORANGE, 12.0);

        c.fill_circle(center, 3.5, ORANGE);
    }

    fn draw_hand(c: &mut Canvas<'_>, center: Point, angle: f32, len: f32, width: f32, col: Color, tail: f32) {
        let (sa, ca) = (angle.sin(), -angle.cos());
        let p0 = Point::new(center.x - tail * sa, center.y - tail * ca);
        let p1 = Point::new(center.x + len * sa, center.y + len * ca);
        c.line(p0, p1, col, width);
    }

    fn draw_digital_clock(c: &mut Canvas<'_>, tr: Rect, t: &LocalDateTime, _look: &Look) {
        let day_idx = (t.weekday() as usize).min(6);
        let m_idx = (t.month as usize).saturating_sub(1).min(11);
        let header = format!("{}, {} {}", DAYS[day_idx], t.day, MONTHS[m_idx]).to_uppercase();

        let head_st = TextStyle::new(10.0).weight(Weight::Bold).color(RED).align(Align::Center);
        c.text(&header, Rect::new(tr.x + 8.0, tr.y + 24.0, tr.w - 16.0, 14.0), &head_st);

        let time_str = format!("{:02}:{:02}", t.hour, t.minute);
        let time_st = TextStyle::new(38.0).weight(Weight::Bold).color(Color::white(1.0)).align(Align::Center);
        c.text(&time_str, Rect::new(tr.x + 8.0, tr.y + 54.0, tr.w - 16.0, 44.0), &time_st);

        let ampm = if t.hour >= 12 { "PM" } else { "AM" };
        let sec_str = format!("{:02}s  {}", t.second, ampm);
        let sec_st = TextStyle::new(12.0).weight(Weight::Medium).color(Color::white(0.66)).align(Align::Center);
        c.text(&sec_str, Rect::new(tr.x + 8.0, tr.y + 104.0, tr.w - 16.0, 16.0), &sec_st);
    }

    fn draw_date_tile(c: &mut Canvas<'_>, tr: Rect, t: &LocalDateTime, _look: &Look) {
        let day_idx = (t.weekday() as usize).min(6);
        let m_idx = (t.month as usize).saturating_sub(1).min(11);

        let head_st = TextStyle::new(12.0).weight(Weight::Bold).color(RED).align(Align::Center);
        c.text(DAYS[day_idx], Rect::new(tr.x + 8.0, tr.y + 24.0, tr.w - 16.0, 16.0), &head_st);

        let day_str = format!("{}", t.day);
        let day_st = TextStyle::new(56.0).weight(Weight::Regular).color(Color::white(1.0)).align(Align::Center);
        c.text(&day_str, Rect::new(tr.x + 8.0, tr.y + 44.0, tr.w - 16.0, 60.0), &day_st);

        let mon_st = TextStyle::new(13.0).weight(Weight::Medium).color(Color::white(0.8)).align(Align::Center);
        c.text(MONTHS[m_idx], Rect::new(tr.x + 8.0, tr.y + 122.0, tr.w - 16.0, 18.0), &mon_st);
    }

    fn draw_battery_tile(c: &mut Canvas<'_>, tr: Rect, _look: &Look) {
        let (pct, plugged, charging) = query_power();
        let center = Point::new(tr.x + tr.w * 0.5, tr.y + tr.h * 0.44);
        let r = 46.0;
        let w = 11.0;

        let col = if pct <= 20 && !plugged { RED } else { GREEN };
        c.ring(center, r, w, pct as f32 / 100.0, Color::white(0.16), col);

        let pct_str = format!("{pct}%");
        let pct_st = TextStyle::new(24.0).weight(Weight::Semibold).color(Color::white(1.0)).align(Align::Center);
        c.text(&pct_str, Rect::new(center.x - 40.0, center.y - 14.0, 80.0, 28.0), &pct_st);

        let state = if charging { "Charging" } else if plugged { "Power Adapter" } else { "Battery" };
        let state_st = TextStyle::new(10.0).weight(Weight::Medium).color(Color::white(0.66)).align(Align::Center);
        c.text(state, Rect::new(tr.x + 8.0, tr.y + 124.0, tr.w - 16.0, 16.0), &state_st);
    }

    fn draw_system_tile_stats(c: &mut Canvas<'_>, tr: Rect, _look: &Look, cpu: f32, mem: f32, disk: f32) {
        let head_st = TextStyle::new(11.0).weight(Weight::Bold).color(Color::white(0.56));
        c.text("SYSTEM", Rect::new(tr.x + 16.0, tr.y + 16.0, 100.0, 14.0), &head_st);

        let pad = 16.0;
        let bw = tr.w - 2.0 * pad;
        let mut ry = tr.y + 40.0;

        let items = [
            ("CPU", cpu, BLUE),
            ("RAM", mem, PURPLE),
            ("SSD", disk, YELLOW),
        ];

        for (lbl, val, col) in items {
            let l_st = TextStyle::new(11.0).weight(Weight::Semibold).color(Color::white(0.88));
            c.text(lbl, Rect::new(tr.x + pad, ry, 60.0, 14.0), &l_st);

            let pct_s = format!("{:.0}%", (val * 100.0).round());
            let r_st = TextStyle::new(11.0).weight(Weight::Medium).color(Color::white(0.56)).align(Align::Right);
            c.text(&pct_s, Rect::new(tr.x + tr.w - pad - 60.0, ry, 60.0, 14.0), &r_st);

            let by = ry + 16.0;
            let bg_r = Rect::new(tr.x + pad, by, bw, 6.0);
            c.fill_rrect(bg_r, 3.0, Color::white(0.14));

            let bar_w = (bw * val).clamp(6.0, bw);
            let fg_r = Rect::new(tr.x + pad, by, bar_w, 6.0);
            c.fill_rrect(fg_r, 3.0, col);

            ry += 36.0;
        }
    }

    pub fn handle_message(&mut self, hwnd: HWND, msg: u32, _wp: WPARAM, lp: LPARAM, mon: &MonitorInfo) -> (Option<LRESULT>, Option<WidgetsAction>) {
        let scale = mon.scale().max(1.0);
        let dip_pt = |lp: LPARAM| -> Point {
            let px = (lp.0 & 0xFFFF) as i16 as f32;
            let py = ((lp.0 >> 16) & 0xFFFF) as i16 as f32;
            Point::new(px / scale, py / scale)
        };

        match msg {
            WM_LBUTTONDOWN => {
                let pt = dip_pt(lp);
                if self.tile_at_point(pt).is_some() {
                    let mut p = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut p);
                        let _ = SetCapture(hwnd);
                    }
                    self.dragging = true;
                    self.drag_start = (p.x, p.y);
                    self.initial_pos = self.pos;
                    return (Some(LRESULT(0)), None);
                }
            }
            WM_MOUSEMOVE => {
                if self.dragging {
                    let mut p = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut p);
                    }
                    let dx = p.x - self.drag_start.0;
                    let dy = p.y - self.drag_start.1;
                    self.pos = (self.initial_pos.0 + dx, self.initial_pos.1 + dy);
                    unsafe {
                        let _ = SetWindowPos(self.hwnd, HWND_BOTTOM, self.pos.0, self.pos.1, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                    }
                    self.dirty = true;
                    return (Some(LRESULT(0)), None);
                }
            }
            WM_LBUTTONUP => {
                if self.dragging {
                    self.dragging = false;
                    unsafe {
                        let _ = ReleaseCapture();
                    }
                    let mut p = POINT::default();
                    unsafe {
                        let _ = GetCursorPos(&mut p);
                    }
                    let moved = (p.x - self.drag_start.0).abs() > 4 || (p.y - self.drag_start.1).abs() > 4;
                    if moved {
                        let act = self.snap_or_save_drag(mon);
                        return (Some(LRESULT(0)), act);
                    }
                }

                let pt = dip_pt(lp);
                if let Some(slot) = self.tile_at_point(pt) {
                    let target = match self.widget_at(slot) {
                        "clock" | "digital_clock" => "ms-clock:",
                        "date" | "calendar" => "outlookcal:",
                        "battery" => "ms-settings:batterysaver",
                        "system" => "taskmgr.exe",
                        _ => "",
                    };
                    if !target.is_empty() {
                        crate::shell::open_dock_spec(target);
                    }
                    return (Some(LRESULT(0)), None);
                }
            }
            WM_RBUTTONUP => {
                let mut p = POINT::default();
                unsafe {
                    let _ = GetCursorPos(&mut p);
                }
                let mut client_pt = p;
                unsafe {
                    let _ = ScreenToClient(hwnd, &mut client_pt);
                }
                let dip = Point::new(client_pt.x as f32 / scale, client_pt.y as f32 / scale);
                let slot = self.slot_at_point(dip).unwrap_or(0);
                let act = self.show_context_menu(p, slot, mon);
                return (Some(LRESULT(0)), act);
            }
            WM_NCHITTEST => {
                let mut screen_pt = POINT {
                    x: (lp.0 & 0xFFFF) as i16 as i32,
                    y: ((lp.0 >> 16) & 0xFFFF) as i16 as i32,
                };
                unsafe {
                    let _ = ScreenToClient(hwnd, &mut screen_pt);
                }
                let dip = Point::new(screen_pt.x as f32 / scale, screen_pt.y as f32 / scale);
                if self.tile_at_point(dip).is_some() {
                    return (Some(LRESULT(HTCLIENT as isize)), None);
                } else {
                    return (Some(LRESULT(HTTRANSPARENT as isize)), None);
                }
            }
            WM_WINDOWPOSCHANGING => {
                let p = unsafe { &mut *(lp.0 as *mut WINDOWPOS) };
                p.hwndInsertAfter = HWND_BOTTOM;
                p.flags &= !SWP_NOZORDER;
                return (Some(LRESULT(0)), None);
            }
            _ => {}
        }

        (None, None)
    }

    fn snap_or_save_drag(&mut self, mon: &MonitorInfo) -> Option<WidgetsAction> {
        let scale = mon.scale().max(1.0);
        let wa = mon.work;
        let margin_gap = ((20.0 - WIDGET_MARGIN) * scale) as i32;
        let top_gap = ((16.0 - WIDGET_MARGIN) * scale) as i32;

        let tr = (wa.right - margin_gap - self.size_px, wa.top + top_gap);
        let tl = (wa.left + margin_gap, wa.top + top_gap);
        let br = (wa.right - margin_gap - self.size_px, wa.bottom - margin_gap - self.size_px);
        let bl = (wa.left + margin_gap, wa.bottom - margin_gap - self.size_px);

        let thresh = 80.0 * scale;
        let dist = |p1: (i32, i32), p2: (i32, i32)| {
            let dx = (p1.0 - p2.0) as f32;
            let dy = (p1.1 - p2.1) as f32;
            (dx * dx + dy * dy).sqrt()
        };

        if dist(self.pos, tr) < thresh {
            self.pos = tr;
            self.position = "top-right".into();
            self.offset = None;
        } else if dist(self.pos, tl) < thresh {
            self.pos = tl;
            self.position = "top-left".into();
            self.offset = None;
        } else if dist(self.pos, br) < thresh {
            self.pos = br;
            self.position = "bottom-right".into();
            self.offset = None;
        } else if dist(self.pos, bl) < thresh {
            self.pos = bl;
            self.position = "bottom-left".into();
            self.offset = None;
        } else {
            self.position = "custom".into();
            let max_x = (wa.right - self.size_px).max(wa.left);
            let max_y = (wa.bottom - self.size_px).max(wa.top);
            self.pos = (self.pos.0.clamp(wa.left, max_x), self.pos.1.clamp(wa.top, max_y));
            self.offset = Some(self.pos);
        }

        unsafe {
            let _ = SetWindowPos(self.hwnd, HWND_BOTTOM, self.pos.0, self.pos.1, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
        }
        self.dirty = true;

        Some(WidgetsAction::SaveLayout {
            items: self.items.clone(),
            position: self.position.clone(),
            offset: self.offset,
        })
    }

    fn show_context_menu(&mut self, pt: POINT, slot: usize, mon: &MonitorInfo) -> Option<WidgetsAction> {
        let cur_kind = self.widget_at(slot).to_string();
        let cur_pos = self.position.clone();
        let slot_name = match slot {
            0 => "Top Left",
            1 => "Top Right",
            2 => "Bottom Left",
            3 => "Bottom Right",
            _ => "Tile",
        };
        let slot_label = format!("Tile {} - {} ({})", slot + 1, slot_name, match cur_kind.as_str() {
            "clock" => "Analog Clock",
            "digital_clock" => "Digital Clock",
            "date" | "calendar" => "Calendar & Date",
            "battery" => "Battery",
            "system" => "System",
            _ => "Hidden",
        });

        unsafe {
            let menu = CreatePopupMenu().ok()?;
            let mut cmds: Vec<(usize, Box<dyn FnOnce(&mut DockWidgets, &MonitorInfo) -> Option<WidgetsAction>>)> = Vec::new();
            let mut next_id = 1usize;

            let mut push_item = |label: &str, checked: bool, enabled: bool, f: Box<dyn FnOnce(&mut DockWidgets, &MonitorInfo) -> Option<WidgetsAction>>| {
                let id = next_id;
                next_id += 1;
                let w: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
                let mut flags = MF_STRING;
                if checked {
                    flags |= MF_CHECKED;
                }
                if !enabled {
                    flags |= MF_DISABLED | MF_GRAYED;
                }
                let _ = AppendMenuW(menu, flags, id, PCWSTR(w.as_ptr()));
                cmds.push((id, f));
            };

            push_item(&slot_label, false, false, Box::new(|_, _| None));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

            let set_widget = |kind: &'static str| -> Box<dyn FnOnce(&mut DockWidgets, &MonitorInfo) -> Option<WidgetsAction>> {
                Box::new(move |w, _| {
                    while w.items.len() <= slot {
                        w.items.push("none".into());
                    }
                    w.items[slot] = kind.into();
                    w.dirty = true;
                    Some(WidgetsAction::SaveLayout {
                        items: w.items.clone(),
                        position: w.position.clone(),
                        offset: w.offset,
                    })
                })
            };

            push_item("Analog Clock", cur_kind == "clock", true, set_widget("clock"));
            push_item("Digital Clock", cur_kind == "digital_clock", true, set_widget("digital_clock"));
            push_item("Calendar & Date", cur_kind == "date" || cur_kind == "calendar", true, set_widget("date"));
            push_item("Battery", cur_kind == "battery", true, set_widget("battery"));
            push_item("System (CPU / RAM / Disk)", cur_kind == "system", true, set_widget("system"));
            push_item("Hide this tile", cur_kind == "none", true, set_widget("none"));

            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());

            let set_pos = |p: &'static str| -> Box<dyn FnOnce(&mut DockWidgets, &MonitorInfo) -> Option<WidgetsAction>> {
                Box::new(move |w, mon| {
                    w.position = p.into();
                    w.offset = None;
                    let scale = mon.scale().max(1.0);
                    w.pos = Self::compute_pos(mon, w.size_px, scale, &w.position, None);
                    let _ = SetWindowPos(w.hwnd, HWND_BOTTOM, w.pos.0, w.pos.1, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                    w.dirty = true;
                    Some(WidgetsAction::SaveLayout {
                        items: w.items.clone(),
                        position: w.position.clone(),
                        offset: None,
                    })
                })
            };

            push_item("Snap: Top Right", cur_pos == "top-right", true, set_pos("top-right"));
            push_item("Snap: Top Left", cur_pos == "top-left", true, set_pos("top-left"));
            push_item("Snap: Bottom Right", cur_pos == "bottom-right", true, set_pos("bottom-right"));
            push_item("Snap: Bottom Left", cur_pos == "bottom-left", true, set_pos("bottom-left"));

            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            push_item("Reset Widgets Layout", false, true, Box::new(|w, mon| {
                w.items = vec!["clock".into(), "date".into(), "battery".into(), "system".into()];
                w.position = "top-right".into();
                w.offset = None;
                let scale = mon.scale().max(1.0);
                w.pos = Self::compute_pos(mon, w.size_px, scale, &w.position, None);
                let _ = SetWindowPos(w.hwnd, HWND_BOTTOM, w.pos.0, w.pos.1, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
                w.dirty = true;
                Some(WidgetsAction::SaveLayout {
                    items: w.items.clone(),
                    position: w.position.clone(),
                    offset: None,
                })
            }));

            push_item("Dock Settings…", false, true, Box::new(|_, _| Some(WidgetsAction::OpenSettings)));

            let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY, pt.x, pt.y, 0, self.hwnd, None);
            let _ = DestroyMenu(menu);

            if chosen.0 > 0 {
                if let Some((_, f)) = cmds.into_iter().find(|(id, _)| *id == chosen.0 as usize) {
                    return f(self, mon);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dock_widgets_grid_geometry() {
        let r0 = DockWidgets::tile_rect_for_index(0);
        let r1 = DockWidgets::tile_rect_for_index(1);
        let r2 = DockWidgets::tile_rect_for_index(2);
        let r3 = DockWidgets::tile_rect_for_index(3);

        assert_eq!(r0.w, WIDGET_TILE);
        assert_eq!(r0.h, WIDGET_TILE);
        assert_eq!(r1.x, r0.x + WIDGET_TILE + WIDGET_GAP);
        assert_eq!(r1.y, r0.y);
        assert_eq!(r2.x, r0.x);
        assert_eq!(r2.y, r0.y + WIDGET_TILE + WIDGET_GAP);
        assert_eq!(r3.x, r1.x);
        assert_eq!(r3.y, r2.y);
    }

    #[test]
    fn test_dock_widgets_positions_and_anchors() {
        let mon = MonitorInfo {
            handle: 1,
            rect: RECT { left: 0, top: 0, right: 1920, bottom: 1080 },
            work: RECT { left: 0, top: 0, right: 1920, bottom: 1040 },
            dpi: 96,
        };
        let scale = 1.0;
        let size_px = 350;

        let tr = DockWidgets::compute_pos(&mon, size_px, scale, "top-right", None);
        let tl = DockWidgets::compute_pos(&mon, size_px, scale, "top-left", None);
        let br = DockWidgets::compute_pos(&mon, size_px, scale, "bottom-right", None);
        let bl = DockWidgets::compute_pos(&mon, size_px, scale, "bottom-left", None);

        assert!(tr.0 > tl.0);
        assert_eq!(tr.1, tl.1);
        assert!(br.1 > tr.1);
        assert_eq!(br.0, tr.0);
        assert_eq!(bl.0, tl.0);
        assert_eq!(bl.1, br.1);
    }
}

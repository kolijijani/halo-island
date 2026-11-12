//! Widgets: Halo Island's own page. Left, the widget slots (Settings ›
//! General › Widgets tab: 2 free, up to 6 with Halo Pro); right, today's
//! glasses of water and, while Clipboard is on, what was copied (5 free;
//! with Pro every item, and a search). Nothing here moves by itself.

use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::state::AppState;
use crate::ui::{wid, wid_n, PillStyle, Ui, PILL_GAP, PILL_SMALL};
use halo_core::geom::{palette, Color, Point, Rect};
use halo_core::halo::{self, Widget};

const HEIGHT: f32 = 196.0;
const TILE_H: f32 = 58.0;
const TILE_GAP: f32 = 8.0;
const RIGHT_W: f32 = 230.0;
const CLIP_H: f32 = 22.0;

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let right = Rect::new(area.right() - RIGHT_W, area.y, RIGHT_W, area.h);
    let left = Rect::new(area.x, area.y, (right.x - 18.0 - area.x).max(0.0), area.h);
    draw_tiles(ui, st, left);
    draw_water(ui, st, Rect::new(right.x, right.y, right.w, 52.0));
    draw_clips(ui, st, Rect::new(right.x, right.y + 62.0, right.w, right.h - 62.0));
}

// ------------------------------------------------------------ widgets

fn draw_tiles(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let list = st.settings.widgets.clone();
    if list.is_empty() {
        ui.c.text("Add widgets in Settings › General.", r, &TextStyle::footnote().color(palette::TEXT_DIM).center());
        return;
    }
    let cols = 2;
    let w = (r.w - TILE_GAP) / cols as f32;
    for (i, wdg) in list.iter().enumerate() {
        let (col, row) = (i % cols, i / cols);
        let t = Rect::new(r.x + col as f32 * (w + TILE_GAP), r.y + row as f32 * (TILE_H + TILE_GAP), w, TILE_H);
        ui.c.fill_rrect(t, 12.0, palette::SURFACE);
        ui.c.text(wdg.title(), Rect::new(t.x + 12.0, t.y + 8.0, t.w - 24.0, 15.0), &TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_DIM));
        let (big, small) = tile_text(st, *wdg);
        ui.c.text(&big, Rect::new(t.x + 12.0, t.y + 24.0, t.w - 24.0, 20.0), &TextStyle::headline().tabular());
        ui.c.text(&small, Rect::new(t.x + 12.0, t.y + 42.0, t.w - 24.0, 14.0), &TextStyle::caption().color(palette::TEXT_DIM));
        if *wdg == Widget::Moon {
            moon(ui, Point::new(t.right() - 24.0, t.center().y), 11.0, halo::moon_age(st.unix));
        }
    }
}

fn tile_text(st: &AppState, w: Widget) -> (String, String) {
    match w {
        Widget::Weather => match &st.weather {
            Some((r, _)) => (format!("{:.0}°C", r.temp_c), halo::weather_words(r.code).to_string()),
            None => ("—".into(), "Reading…".into()),
        },
        Widget::Calendar => {
            const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            let l = &st.local;
            (format!("{} {}", l.day, MONTHS[(l.month.clamp(1, 12) - 1) as usize]), DAYS[l.weekday() as usize % 7].to_string())
        }
        Widget::WorldClock => {
            let parts: Vec<String> = halo::CITIES.iter().map(|(c, off)| format!("{} {}", &c[..3], halo::clock_at(st.unix, *off))).collect();
            (parts[..2].join("  "), parts[2..].join("  "))
        }
        Widget::Moon => {
            let age = halo::moon_age(st.unix);
            (format!("{:.0}%", halo::moon_lit(age) * 100.0), halo::moon_name(age).to_string())
        }
        Widget::Countdown => match halo::days_until(&st.local.day_key(), &st.settings.countdown_day) {
            Some(d) if d >= 0 => (format!("{d} days"), st.settings.countdown_label.clone()),
            Some(_) => ("Done".into(), st.settings.countdown_label.clone()),
            None => ("—".into(), "No date set".into()),
        },
    }
}

/// The moon, lit by `age` (0 new, 0.5 full): a disc and the dark part's
/// ellipse over it.
fn moon(ui: &mut Ui, c: Point, r: f32, age: f64) {
    ui.c.fill_circle(c, r, Color::white(0.18));
    let lit = halo::moon_lit(age) as f32;
    if lit < 0.02 {
        return;
    }
    let waxing = age < 0.5;
    // The lit part: a half disc and an ellipse that adds or takes away.
    let k = (2.0 * lit - 1.0).abs();
    let half = Rect::new(if waxing { c.x } else { c.x - r }, c.y - r, r, 2.0 * r);
    ui.c.clip(half);
    ui.c.fill_circle(c, r, Color::white(0.92));
    ui.c.unclip();
    let e = Rect::from_center(c, 2.0 * r * k, 2.0 * r);
    ui.c.fill_rrect(e, (r * k).min(r), if lit > 0.5 { Color::white(0.92) } else { Color::white(0.18) });
}

// ------------------------------------------------------------ water

fn draw_water(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let day = st.local.day_key();
    let goal = st.settings.water_goal;
    let n = st.store.halo_water.today(&day);
    ui.c.fill_rrect(r, 12.0, palette::SURFACE);
    ui.c.ring(Point::new(r.x + 24.0, r.center().y), 12.0, 4.0, (n as f32 / goal.max(1) as f32).min(1.0), Color::white(0.12), palette::CYAN);
    ui.c.text("Water", Rect::new(r.x + 46.0, r.y + 8.0, 100.0, 16.0), &TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_DIM));
    ui.c.text(&halo::fmt_glasses(n, goal), Rect::new(r.x + 46.0, r.y + 24.0, 120.0, 20.0), &TextStyle::body().weight(Weight::Semibold).tabular());
    let plus = Rect::new(r.right() - 10.0 - PILL_SMALL, r.center().y - PILL_SMALL / 2.0, PILL_SMALL, PILL_SMALL);
    let minus = Rect::new(plus.x - PILL_GAP - PILL_SMALL, plus.y, PILL_SMALL, PILL_SMALL);
    let mut delta = 0;
    if ui.pill(wid("widgets.water.minus"), minus, "−", None, PillStyle::Utility).clicked {
        delta = -1;
    }
    if ui.pill(wid("widgets.water.plus"), plus, "+", None, PillStyle::Primary(palette::CYAN)).clicked {
        delta = 1;
    }
    if delta != 0 {
        st.store.halo_water.add(&day, delta);
        st.cmd(Command::SaveStore);
        ui.request_repaint();
    }
}

// ------------------------------------------------------------ clipboard

fn draw_clips(ui: &mut Ui, st: &mut AppState, r: Rect) {
    if !st.settings.clips {
        ui.c.text("Clipboard is off (Settings › General).", Rect::new(r.x, r.y, r.w, 18.0), &TextStyle::footnote().color(palette::TEXT_DIM));
        return;
    }
    let pro = st.settings.pro;
    let mut y = r.y;
    if pro {
        let mut q = std::mem::take(&mut st.ui.clip_query);
        ui.text_input(wid("widgets.clips.search"), Rect::new(r.x, y, r.w, PILL_SMALL), &mut q, "Search what you copied", 60);
        st.ui.clip_query = q;
        y += PILL_SMALL + 6.0;
    } else {
        ui.c.text("Clipboard · 5 items", Rect::new(r.x, y, r.w, 16.0), &TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_DIM));
        y += 18.0;
    }
    let found: Vec<String> = st.clips.find(&st.ui.clip_query).into_iter().map(str::to_string).collect();
    if found.is_empty() {
        ui.c.text("Nothing copied yet.", Rect::new(r.x, y, r.w, 18.0), &TextStyle::footnote().color(palette::TEXT_FAINT));
        return;
    }
    let fits = ((r.bottom() - y) / CLIP_H).floor().max(0.0) as usize;
    for (i, t) in found.iter().take(fits).enumerate() {
        let row = Rect::new(r.x, y + i as f32 * CLIP_H, r.w, CLIP_H - 2.0);
        let resp = ui.button(wid_n("widgets.clip", i as u64), row);
        if resp.hovered {
            ui.c.fill_rrect(row, 6.0, palette::SURFACE);
        }
        let line: String = t.lines().next().unwrap_or("").chars().take(60).collect();
        ui.c.text(&line, row.inset(6.0, 0.0), &TextStyle::footnote());
        // Click: it goes back on the clipboard.
        if resp.clicked {
            st.cmd(Command::ClipBack(t.clone()));
        }
    }
}

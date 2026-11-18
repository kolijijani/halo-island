//! Widgets: Halo Island's own page. Left, the widget slots (Settings ›
//! General › Widgets tab: 2 free, all 5 with Halo Pro); right, today's
//! glasses of water and, while Clipboard is on, what was copied (5 free;
//! with Pro every item, and a search). As the tab opens
//! (`pages::entrance`) the widget tiles rise in one after another, a light
//! crossing each, their drawings popping in (the clock's hands sweep round);
//! then the water card, its ring and water filling, and the clipboard rows.
//! For a few seconds after (`entrance::alive`) the weather drawing and the
//! water's surface move, then the tab rests: nothing moves by itself after.
//! A click on the Weather tile picks its city from the Prayer picker's
//! places (places.json and the website's world list), or goes back to
//! where the PC is.

use super::entrance::{alive, hover, rise, scaled, shine, still, Entrance};
use super::PageKind;
use crate::commands::Command;
use crate::icons::Icon;
use crate::state::CityFile;
use halo_core::prayer::{self, world, PlaceList};
use crate::gfx::{TextStyle, Weight};
use crate::state::AppState;
use crate::ui::{wid, wid_n, PillStyle, Ui, PILL_GAP, PILL_SMALL};
use halo_core::geom::{palette, Color, Point, Rect};
use halo_core::halo::{self, Widget};
use halo_core::spring::bouncy_at;

const HEIGHT: f32 = 196.0;
const TILE_H: f32 = 58.0;
const TILE_GAP: f32 = 8.0;
const RIGHT_W: f32 = 230.0;
const CLIP_H: f32 = 22.0;

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

/// While the tab is open, the Weather widget's next try after a failure
/// (the app reads it then, `App::widgets_shown`).
pub fn next_wake(st: &AppState) -> Option<f64> {
    let at = st.weather.retry_at.filter(|_| st.settings.widgets.contains(&Widget::Weather))?;
    Some(st.now + (at - st.unix).max(0) as f64 + 0.05)
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    let right = Rect::new(area.right() - RIGHT_W, area.y, RIGHT_W, area.h);
    let left = Rect::new(area.x, area.y, (right.x - 18.0 - area.x).max(0.0), area.h);
    if st.ui.weather_pick.is_some() {
        draw_city_picker(ui, st, left);
    } else {
        draw_tiles(ui, st, left);
    }
    draw_water(ui, st, Rect::new(right.x, right.y, right.w, 52.0));
    draw_clips(ui, st, Rect::new(right.x, right.y + 62.0, right.w, right.h - 62.0));
}

// ------------------------------------------------------------ widgets

fn draw_tiles(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let list = st.settings.widgets.clone();
    let e = Entrance::of(ui, st, PageKind::Widgets);
    if list.is_empty() {
        let ic = Point::new(r.center().x, r.center().y - 16.0);
        scaled(ui, ic, e.pop(0.05), |ui| ui.c.icon(Icon::Addons, Rect::from_center(ic, 22.0, 22.0), Color::white(0.35)));
        ui.c.text("Add widgets in Settings › General.", r.offset(0.0, 8.0), &TextStyle::footnote().color(palette::TEXT_DIM).center());
        return;
    }
    let live = alive(ui, st, PageKind::Widgets);
    let cols = 2;
    let w = (r.w - TILE_GAP) / cols as f32;
    for (i, wdg) in list.iter().enumerate() {
        let (col, row) = (i % cols, i / cols);
        let t = Rect::new(r.x + col as f32 * (w + TILE_GAP), r.y + row as f32 * (TILE_H + TILE_GAP), w, TILE_H);
        let n = i.min(5) as f32;
        let (hover_k, pressed, open) = if *wdg == Widget::Weather {
            let b = ui.button(wid("widgets.weather"), t);
            (hover(ui, st, wid("widgets.weather"), b.hovered), b.pressed, b.clicked)
        } else {
            (0.0, false, false)
        };
        let tint = tile_tint(st, *wdg);
        let dip = if pressed { 0.98 } else { 1.0 };
        scaled(ui, t.center(), dip, |ui| {
            rise(ui, e.pop(0.06 * n), |ui| tile_face(ui, t, tint, hover_k));
        });
        shine(ui, e, t, 12.0, 0.3 + 0.06 * n);
        let title_st = TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_DIM);
        ui.c.text(wdg.title(), Rect::new(t.x + 12.0, t.y + 8.0, t.w - 24.0, 15.0), &title_st);
        // Where the weather is for, after the title (an estimate says so).
        if let Some((_, _, place)) = st.weather_place().filter(|_| *wdg == Widget::Weather) {
            let x = t.x + 12.0 + ui.c.measure(wdg.title(), &title_st, t.w).0.ceil() + 6.0;
            ui.c.text(&place, Rect::new(x, t.y + 8.0, (t.right() - 28.0 - x).max(0.0), 15.0), &TextStyle::caption().color(palette::TEXT_FAINT));
        }
        if open {
            st.ui.weather_pick = Some(String::new());
            st.ui.weather_found = None;
        }
        let (big, small) = tile_text(st, *wdg);
        let (big_st, small_st) = (TextStyle::headline().tabular(), TextStyle::caption().color(palette::TEXT_DIM));
        ui.c.text(&big, Rect::new(t.x + 12.0, t.y + 24.0, t.w - 24.0, 20.0), &big_st);
        ui.c.text(&small, Rect::new(t.x + 12.0, t.y + 42.0, t.w - 24.0, 14.0), &small_st);
        // Its drawing on the right, where the words leave room for it.
        let words = ui.c.measure(&big, &big_st, t.w).0.max(ui.c.measure(&small, &small_st, t.w).0);
        let c = Point::new(t.right() - 22.0, t.center().y + 2.0);
        let k = e.pop(0.12 + 0.06 * n);
        match wdg {
            _ if t.x + 12.0 + words > c.x - 18.0 => {}
            Widget::Moon => scaled(ui, c, k, |ui| {
                ui.c.fill_circle(c, 15.0, Color::white(0.05));
                moon(ui, c, 11.0, halo::moon_age(st.unix));
            }),
            Widget::Weather => scaled(ui, c, k, |ui| weather_glyph(ui, st, c, live)),
            Widget::WorldClock => clock_face(ui, st, c, e.draw(0.14 + 0.06 * n, 0.7), tint),
            Widget::Calendar => scaled(ui, c, k, |ui| calendar_glyph(ui, c, tint)),
            Widget::Countdown => scaled(ui, c, k, |ui| ui.c.icon(Icon::Timer, Rect::from_center(c, 18.0, 18.0), tint)),
        }
        // The Weather tile opens a picker: a pin shows it, nudging in under the pointer.
        if hover_k > 0.0 {
            let p = Rect::new(t.right() - 20.0, t.y + 7.0 + 2.0 * (1.0 - hover_k), 12.0, 12.0);
            ui.c.icon(Icon::Pin, p, tint.with_alpha(hover_k));
        }
    }
}

/// A tile's colour: the sky's for the weather, a calendar's red, and so on.
fn tile_tint(st: &AppState, w: Widget) -> Color {
    match w {
        Widget::Weather => match st.weather.last.as_ref().map(|(w, _)| w.code) {
            Some(0) => palette::YELLOW,
            Some(95..=99) => palette::PURPLE,
            Some(_) => palette::CYAN,
            None => palette::TEXT_FAINT,
        },
        Widget::Calendar => palette::RED,
        Widget::WorldClock => palette::BLUE,
        Widget::Moon => Color::white(0.85),
        Widget::Countdown => palette::ORANGE,
    }
}

/// A tile's face: the surface, a glow of its colour from the top right, a
/// highlight along the top edge (light from above), and under the pointer
/// a brighter face and an outline in its colour.
fn tile_face(ui: &mut Ui, t: Rect, tint: Color, hover: f32) {
    ui.c.fill_rrect(t, 12.0, Color::lerp(palette::SURFACE, palette::SURFACE_HOVER, hover * 0.6));
    // ponytail: the glow is two flat discs placed clear of the rounded corner, not a radial gradient; add a gradient brush if it looks banded.
    let g = Point::new(t.right() - 24.0, t.y + 24.0);
    ui.c.fill_circle(g, 14.0, tint.with_alpha(0.05 + 0.04 * hover));
    ui.c.fill_circle(g, 9.0, tint.with_alpha(0.05 + 0.04 * hover));
    ui.c.fill_rect(Rect::new(t.x + 12.0, t.y + 0.5, t.w - 24.0, 1.0), Color::white(0.07));
    if hover > 0.0 {
        ui.c.stroke_rrect(t, 12.0, tint.with_alpha(0.35 * hover), 1.0);
    }
}

/// The weather as a drawing: a sun whose rays turn, a cloud that drifts or
/// rain that falls, for a few seconds as the tab opens (`entrance::alive`).
fn weather_glyph(ui: &mut Ui, st: &AppState, c: Point, live: Option<(f32, f32)>) {
    let (t, k) = live.unwrap_or((0.0, 0.0));
    let code = st.weather.last.as_ref().map(|(w, _)| w.code);
    let cloud = |ui: &mut Ui, c: Point, a: f32| {
        ui.c.fill_circle(Point::new(c.x - 5.0, c.y + 1.0), 6.0, Color::white(a));
        ui.c.fill_circle(Point::new(c.x + 2.0, c.y - 2.0), 7.5, Color::white(a));
        ui.c.fill_rrect(Rect::new(c.x - 11.0, c.y + 1.0, 21.0, 7.0), 3.5, Color::white(a));
    };
    match code {
        None => ui.c.icon(Icon::Pin, Rect::from_center(c, 16.0, 16.0), palette::TEXT_FAINT),
        Some(0) => {
            ui.c.fill_circle(c, 9.0, palette::YELLOW.with_alpha(0.22 + 0.06 * (t * 2.0).sin() * k));
            ui.c.fill_circle(c, 6.0, palette::YELLOW);
            let turn = t * 0.5 * k;
            for i in 0..8 {
                let a = turn + i as f32 * std::f32::consts::TAU / 8.0;
                let (s, co) = a.sin_cos();
                ui.c.line(Point::new(c.x + co * 9.0, c.y + s * 9.0), Point::new(c.x + co * 12.0, c.y + s * 12.0), palette::YELLOW.with_alpha(0.85), 1.6);
            }
        }
        Some(code) => {
            let wet = matches!(code, 51..=67 | 80..=82 | 95..=99);
            let snow = matches!(code, 71..=77 | 85 | 86);
            let dx = (t * 0.9).sin() * 1.5 * k;
            if wet || snow {
                for i in 0..3 {
                    let x = c.x - 5.0 + i as f32 * 5.0;
                    let fall = ((t * 1.6 + i as f32 * 0.33).fract() * k) * 5.0;
                    let y = c.y + 8.0 + fall;
                    let ink = palette::CYAN.with_alpha(0.9 - 0.5 * fall / 5.0);
                    if snow {
                        ui.c.fill_circle(Point::new(x, y + 1.0), 1.3, Color::white(0.85));
                    } else {
                        ui.c.line(Point::new(x, y), Point::new(x - 1.0, y + 3.0), ink, 1.4);
                    }
                }
            } else if code <= 3 {
                // Partly cloudy: the sun behind.
                ui.c.fill_circle(Point::new(c.x + 5.0, c.y - 5.0), 5.5, palette::YELLOW.with_alpha(0.9));
            }
            cloud(ui, Point::new(c.x + dx, c.y - 2.0), if code == 45 || code == 48 { 0.5 } else { 0.85 });
        }
    }
}

/// A small clock face at the PC's time, its hands sweeping round from
/// twelve as the tab opens (`k`).
fn clock_face(ui: &mut Ui, st: &AppState, c: Point, k: f32, tint: Color) {
    if k <= 0.0 {
        return;
    }
    let l = &st.local;
    ui.c.with_alpha(k.min(1.0), |cv| {
        cv.fill_circle(c, 13.0, Color::white(0.06));
        cv.stroke_circle(c, 13.0, tint.with_alpha(0.6), 1.2);
    });
    let hand = |ui: &mut Ui, turns: f32, len: f32, w: f32, col: Color| {
        let a = turns * k * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        ui.c.line(c, Point::new(c.x + a.cos() * len, c.y + a.sin() * len), col, w);
    };
    let (h, m) = (l.hour as f32 % 12.0, l.minute as f32);
    hand(ui, (h + m / 60.0) / 12.0, 6.5, 2.0, palette::TEXT);
    hand(ui, m / 60.0, 9.5, 1.4, tint);
    ui.c.fill_circle(c, 1.8, tint);
}

/// A calendar leaf: a red top on a light page.
fn calendar_glyph(ui: &mut Ui, c: Point, tint: Color) {
    let page = Rect::from_center(c, 22.0, 22.0);
    ui.c.fill_rrect(page, 5.0, Color::white(0.12));
    ui.c.fill_rrect(Rect::new(page.x, page.y, page.w, 7.0), 3.0, tint);
    for i in 0..3 {
        ui.c.fill_rect(Rect::new(page.x + 5.0 + i as f32 * 5.0, page.y + 11.0, 3.0, 3.0), Color::white(0.4));
        ui.c.fill_rect(Rect::new(page.x + 5.0 + i as f32 * 5.0, page.y + 16.0, 3.0, 3.0), Color::white(0.25));
    }
}

fn tile_text(st: &AppState, w: Widget) -> (String, String) {
    match w {
        Widget::Weather => {
            use crate::state::Locate;
            let placed = st.weather_place().is_some();
            let locating = !placed && (st.prayer.locate == Locate::Asking || st.weather_ip_asking);
            match st.prayer.locate {
                // No place to read it for, and no reading kept: say why
                // instead of "can't reach it", and what to do.
                _ if placed || locating || st.weather.last.is_some() => halo::weather_tile(&st.weather, st.unix, locating),
                Locate::Denied => ("—".into(), "Location off · click to pick a city".into()),
                // The internet-address guess couldn't go out either.
                _ if st.weather_ip == Some(Err(halo_core::net::Failure::Offline)) => ("—".into(), "Offline".into()),
                Locate::TimedOut => ("—".into(), "Location timed out · click to pick a city".into()),
                _ if st.weather_ip.is_some() => ("—".into(), "Location unknown · click to pick a city".into()),
                _ => halo::weather_tile(&st.weather, st.unix, locating),
            }
        }
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

/// The Weather widget's city: typed, picked from the Prayer picker's
/// places and the world's cities (the website's files, asked as the Prayer
/// picker asks them), or "Use my location" to follow the PC again.
fn draw_city_picker(ui: &mut Ui, st: &mut AppState, r: Rect) {
    const ROW: f32 = 26.0;
    let accent = st.accent;
    let mut q = st.ui.weather_pick.take().unwrap_or_default();
    let here = "Use my location";
    let here_w = ui.pill_width(here, false, true);
    let done_w = ui.pill_width("Done", false, true);
    let done = Rect::new(r.right() - done_w, r.y + 2.0, done_w, PILL_SMALL);
    let here_r = Rect::new(done.x - PILL_GAP - here_w, done.y, here_w, PILL_SMALL);
    let field = Rect::new(r.x, r.y, (here_r.x - 8.0 - r.x).max(0.0), PILL_SMALL + 4.0);
    let resp = ui.text_input_row(wid("widgets.city"), field, &mut q, "Type a city, like Kuala Lumpur", 40, accent, Some(Icon::Search));
    let file = world::file_for(&world::fold(&q));
    if let Some(f) = &file {
        if !st.prayer.cities.contains_key(f) {
            st.prayer.cities.insert(f.clone(), CityFile::Asking);
            st.cmd(Command::GetCities(f.clone()));
        }
    }
    let city_file = file.as_ref().and_then(|f| st.prayer.cities.get(f)).cloned();
    let world_here = match &city_file {
        Some(CityFile::Here(w)) => Some(w.clone()),
        _ => None,
    };
    let key = format!("{q}\u{0}{}", world_here.is_some());
    if st.ui.weather_found.as_ref().is_none_or(|(k, _)| *k != key) {
        let empty = PlaceList { places: Vec::new() };
        let list = st.prayer.places.clone();
        let found = prayer::search(list.as_deref().unwrap_or(&empty), world_here.as_deref(), &q, 4);
        st.ui.weather_found = Some((key, found));
    }
    let found = st.ui.weather_found.as_ref().map(|f| f.1.clone()).unwrap_or_default();
    let mut picked = None;
    let top = r.y + PILL_SMALL + 14.0;
    if found.is_empty() {
        let line = match (&city_file, q.trim().is_empty()) {
            (_, true) => match &st.settings.weather_city {
                Some(c) => format!("The weather is for {}. Type another city.", c.name),
                None => "The weather follows where this PC is. Type a city to pick one.".into(),
            },
            (Some(CityFile::Asking), _) => "Looking up cities\u{2026}".into(),
            (Some(CityFile::Failed(f)), _) => f.line("The city list"),
            _ => "No city with that name".into(),
        };
        ui.c.text(&line, Rect::new(r.x, top, r.w, 32.0), &TextStyle::footnote().color(palette::TEXT_DIM).wrap().top());
    }
    for (i, p) in found.iter().enumerate() {
        let row = Rect::new(r.x, top + i as f32 * ROW, r.w, ROW - 2.0);
        let b = ui.button(wid_n("widgets.city.row", i as u64), row);
        if b.hovered || b.pressed {
            ui.c.fill_rrect(row, 6.0, palette::SURFACE);
        }
        let lw = ui.c.measure(&p.label, &TextStyle::footnote(), row.w).0.ceil();
        ui.c.text(&p.label, Rect::new(row.x + 6.0, row.y, lw + 2.0, row.h), &TextStyle::footnote().weight(Weight::Semibold));
        ui.c.text(&p.prov, Rect::new(row.x + lw + 14.0, row.y, (row.w - lw - 20.0).max(0.0), row.h), &TextStyle::footnote().color(palette::TEXT_DIM));
        if b.clicked {
            picked = Some(p.clone());
        }
    }
    if resp.submitted {
        picked = picked.or_else(|| found.first().cloned());
    }
    let mut close = resp.cancelled || ui.pill(wid("widgets.city.done"), done, "Done", None, PillStyle::Utility).clicked;
    let follow = ui.pill(wid("widgets.city.here"), here_r, here, None, PillStyle::UtilityAccent(accent)).clicked;
    if picked.is_some() || follow {
        st.settings.weather_city = picked.map(|p| halo::WeatherCity { name: p.label, lat: p.lat, lng: p.lng });
        // Another place: a new reading, asked at once (the old one stays kept on disk).
        st.weather = Default::default();
        st.cmd(Command::SaveSettings);
        if follow {
            // Where the PC is now, asked again whatever Windows said before.
            st.weather_at = None;
            st.cmd(Command::Locate);
        }
        close = true;
    }
    if close {
        st.ui.weather_found = None;
        st.cmd(Command::BlurText);
    } else {
        st.ui.weather_pick = Some(q);
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

/// Today's water: the ring, "Water · 63%" (or "Goal reached")
/// over the glasses drunk large and the goal beside them, then − and +.
/// As the tab opens the card rises in, the ring draws to today's share and
/// the buttons pop (`pages::entrance`); the words and numbers stay put.
/// Inside the ring the water stands at today's share, its surface swaying
/// for a few seconds (`entrance::alive`); a press on − or + settles it to
/// the new level on the spring and sends a ripple out of the ring. The card
/// has a faint cyan wash rising from its bottom to the same share.
fn draw_water(ui: &mut Ui, st: &mut AppState, r: Rect) {
    let day = st.local.day_key();
    let goal = st.settings.water_goal;
    let n = st.store.halo_water.today(&day);
    let share = n as f32 / goal.max(1) as f32;
    let e = Entrance::of(ui, st, PageKind::Widgets);
    let live = alive(ui, st, PageKind::Widgets);
    let (ring, minus_k, plus_k) = (e.draw(0.08, 0.6), e.pop(0.14), e.pop(0.20));
    // Since the last press: the level going from where it was on the
    // spring, and a ripple.
    let tap = st.ui.water_tap.map(|(at, from)| ((ui.now - at) as f32, from)).filter(|(t, _)| *t < 0.8);
    if tap.is_some() {
        ui.request_anim();
    } else {
        st.ui.water_tap = None;
    }
    let level = match tap {
        Some((t, from)) => from + (share.min(1.0) - from) * bouncy_at(t),
        None => share.min(1.0) * ring,
    };
    let done = n >= goal;
    let c = Point::new(r.x + 25.0, r.center().y);
    rise(ui, e.pop(0.0), |ui| {
        ui.c.fill_rrect(r, 12.0, palette::SURFACE);
        // The wash: from the card's bottom to today's share of it, inside its rounded corners.
        let wash = (r.h - 12.0) * level;
        if wash > 0.5 {
            ui.c.fill_rect(Rect::new(r.x + 12.0, r.bottom() - 6.0 - wash, r.w - 24.0, wash), palette::CYAN.with_alpha(0.05));
        }
        ui.c.fill_rect(Rect::new(r.x + 12.0, r.y + 0.5, r.w - 24.0, 1.0), Color::white(0.07));
        if done {
            ui.c.stroke_rrect(r, 12.0, palette::CYAN.with_alpha(0.3 * ring), 1.0);
        }
    });
    shine(ui, e, r, 12.0, 0.5);
    water_in_ring(ui, c, 10.5, level, live, tap.map(|t| t.0));
    ui.c.ring(c, 14.0, 4.0, level, Color::white(0.12), palette::CYAN);
    if let Some((t, _)) = tap {
        let p = t / 0.8;
        ui.c.stroke_circle(c, 14.0 + 10.0 * p, palette::CYAN.with_alpha(0.5 * (1.0 - p)), 1.5);
    }

    let x = r.x + 50.0;
    let head = if done { "Goal reached".to_string() } else { format!("Water \u{b7} {:.0}%", share * 100.0) };
    let head_c = if done { palette::CYAN } else { palette::TEXT_DIM };
    ui.c.text(&head, Rect::new(x, r.y + 7.0, 100.0, 15.0), &TextStyle::caption().weight(Weight::Semibold).color(head_c));
    let big = TextStyle::value().tabular();
    let count = n.to_string();
    let cw = ui.c.measure(&count, &big, 60.0).0.ceil();
    ui.c.text(&count, Rect::new(x, r.y + 22.0, cw + 2.0, 24.0), &big);
    let of = format!("/ {goal} glasses");
    ui.c.text(&of, Rect::new(x + cw + 4.0, r.y + 26.0, 100.0, 18.0), &TextStyle::footnote().tabular().color(palette::TEXT_DIM));

    let plus = Rect::new(r.right() - 10.0 - PILL_SMALL, r.center().y - PILL_SMALL / 2.0, PILL_SMALL, PILL_SMALL);
    let minus = Rect::new(plus.x - PILL_GAP - PILL_SMALL, plus.y, PILL_SMALL, PILL_SMALL);
    let mut delta = 0;
    if scaled(ui, minus.center(), minus_k, |ui| ui.pill(wid("widgets.water.minus"), minus, "", Some(Icon::Minus), PillStyle::Utility)).clicked {
        delta = -1;
    }
    // A glow behind + while there's water to drink.
    if !done {
        scaled(ui, plus.center(), plus_k, |ui| ui.c.fill_circle(plus.center(), PILL_SMALL / 2.0 + 3.0, palette::CYAN.with_alpha(0.14)));
    }
    if scaled(ui, plus.center(), plus_k, |ui| ui.pill(wid("widgets.water.plus"), plus, "", Some(Icon::Plus), PillStyle::Primary(palette::CYAN))).clicked {
        delta = 1;
    }
    if delta != 0 {
        st.store.halo_water.add(&day, delta);
        st.cmd(Command::SaveStore);
        ui.request_repaint();
        if !still(st) {
            st.ui.water_tap = Some((ui.now, level));
        }
    }
}

/// The water inside the ring (radius `r`), filled to `level` 0..1: a disc
/// clipped below a surface that sways for a while after the tab opens
/// (`live`) or a press (`tap`: seconds since).
fn water_in_ring(ui: &mut Ui, c: Point, r: f32, level: f32, live: Option<(f32, f32)>, tap: Option<f32>) {
    if level <= 0.01 {
        return;
    }
    let (t, k) = live.unwrap_or((0.0, 0.0));
    let slosh = tap.map_or(0.0, |t| (1.0 - t / 0.8).max(0.0));
    let amp = 1.2 * k + 2.0 * slosh;
    let top = c.y + r - 2.0 * r * level.min(1.0);
    // The surface: a few columns, each a little higher or lower on the wave.
    const COLS: usize = 6;
    let w = 2.0 * r / COLS as f32;
    for i in 0..COLS {
        let x = c.x - r + i as f32 * w;
        let y = top + amp * (t * 3.0 + i as f32 * 0.9 + tap.unwrap_or(0.0) * 12.0).sin();
        ui.c.clip(Rect::new(x, y, w + 0.5, c.y + r - y));
        ui.c.fill_circle(c, r, palette::CYAN.with_alpha(0.32));
        ui.c.unclip();
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
    let e = Entrance::of(ui, st, PageKind::Widgets);
    let fits = ((r.bottom() - y) / CLIP_H).floor().max(0.0) as usize;
    for (i, t) in found.iter().take(fits).enumerate() {
        let row = Rect::new(r.x, y + i as f32 * CLIP_H, r.w, CLIP_H - 2.0);
        let id = wid_n("widgets.clip", i as u64);
        let resp = ui.button(id, row);
        let h = hover(ui, st, id, resp.hovered);
        // Each row rises in after the water card, one after another; under
        // the pointer it lights and shows it goes back on the clipboard.
        let k = e.pop(0.24 + 0.05 * i.min(6) as f32);
        rise(ui, k, |ui| {
            ui.c.fill_rrect(row, 6.0, Color::lerp(Color::white(0.03), palette::SURFACE_HOVER, h));
            let dot = Point::new(row.x + 9.0, row.center().y);
            ui.c.fill_circle(dot, 2.0 + 0.5 * h, Color::lerp(Color::white(0.3), st.accent, h));
        });
        if h > 0.0 {
            let ic = Rect::new(row.right() - 18.0 + 3.0 * (1.0 - h), row.center().y - 6.0, 12.0, 12.0);
            ui.c.icon(Icon::Clipboard, ic, st.accent.with_alpha(h));
        }
        let line: String = t.lines().next().unwrap_or("").chars().take(60).collect();
        let text_r = Rect::new(row.x + 18.0, row.y, (row.w - 24.0 - 16.0 * h).max(0.0), row.h);
        ui.c.text(&line, text_r, &TextStyle::footnote());
        // Click: it goes back on the clipboard.
        if resp.clicked {
            st.cmd(Command::ClipBack(t.clone()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Locate;
    use halo_core::net::Failure;
    use halo_core::settings::Settings;
    use halo_core::store::Store;

    fn weather(st: &AppState) -> (String, String) {
        tile_text(st, Widget::Weather)
    }

    #[test]
    fn the_weather_tile_says_why_it_has_no_reading() {
        let mut st = AppState::new(Settings::default(), Store::default());
        // Still asking Windows.
        st.prayer.locate = Locate::Asking;
        assert_eq!(weather(&st).1, "Reading…");
        // Windows said no: the permission, not the network.
        st.prayer.locate = Locate::Denied;
        assert_eq!(weather(&st).1, "Location off · click to pick a city");
        // No answer in time.
        st.prayer.locate = Locate::TimedOut;
        assert_eq!(weather(&st).1, "Location timed out · click to pick a city");
        // Windows couldn't tell, and the internet address couldn't either.
        st.prayer.locate = Locate::Unknown;
        st.weather_ip = Some(Err(Failure::Unreachable));
        assert_eq!(weather(&st).1, "Location unknown · click to pick a city");
        // ...because there is no internet: that, not "unknown".
        st.weather_ip = Some(Err(Failure::Offline));
        assert_eq!(weather(&st), ("—".into(), "Offline".into()));
        // A place: Open-Meteo's own failures and its reading.
        st.weather_at = Some((3.14, 101.69));
        st.weather.came(Err(Failure::Timeout), 0, 30, 600);
        assert_eq!(weather(&st), ("—".into(), "Timed out".into()));
        st.weather.came(Err(Failure::Unreachable), 30, 30, 600);
        assert_eq!(weather(&st).1, "Can't reach it");
        st.weather.came(Ok(halo::Weather { temp_c: 27.4, code: 2 }), 60, 30, 600);
        st.unix = 60;
        assert_eq!(weather(&st), ("27°C".into(), "Cloudy".into()));
    }
}

//! Today page: the date, and the important news of the day.
//!
//! On the left the weekday, the date and this month's weeks (today filled);
//! on the right the top stories of the feed picked in Settings › General
//! (`halo_core::news`): each its headline, its source and how long ago, and
//! one key point under it. A click opens the story in the browser. The list
//! scrolls with the wheel. On an island narrower than [`COMPACT_BELOW`] the
//! weeks make way.
//!
//! The feed is asked only while this page is on screen (`App::today_shown`):
//! when it opens and the last read is over 15 minutes old, and after a
//! failure 30 s doubling to 10 minutes, only while it stays open. Before an
//! answer it says it's getting the news; without internet, or when the feed
//! times out, errs or sends something that isn't a feed, it says which, and
//! keeps the last stories with their age. Nothing is made up: a feed with no
//! current story says so.
//!
//! As the page opens (`pages::entrance`) an accent rule draws under the
//! date, the weekend and this week's band rise in, today's disc pops, the
//! feed's dot pops and the rule under the news heading draws across; the
//! stories rise in one after another as cards, each one's accent bar growing
//! down. The words stay put. For a few seconds after (`entrance::alive`)
//! today's halo breathes and the feed's dot pings, then the page rests.
//! The list glides to a wheel notch; a story under the pointer brightens.

use super::entrance::{alive, hover, rise, scaled, still, Entrance};
use super::PageKind;
use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::{wid_n, Ui};
use halo_core::datetime::{days_from_civil, weekday_from_days, LocalDateTime};
use halo_core::geom::{palette, Color, Point, Rect};
use halo_core::net::Failure;
use halo_core::news::{News, Story};

const HEIGHT: f32 = 200.0;

/// Below this content width the weeks make way for the news (the island can
/// be made as narrow as 480).
const COMPACT_BELOW: f32 = 480.0;
const CELL_W: f32 = 23.0;
const GRID_W: f32 = 7.0 * CELL_W;
const CELL_H: f32 = 22.0;
/// The initials row, under the weekday and the date.
const GRID_Y: f32 = 48.0;
const INITIALS_H: f32 = 16.0;
const COL_GAP: f32 = 20.0;

const WEEKDAY_LONG: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
const MONTH_LONG: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
/// Monday-first initials.
const INITIALS: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];

/// The news column: its heading, then a story each (headline, source line
/// and key point), the key point dropped where a story has none.
const HEAD_H: f32 = 22.0;
const TITLE_H: f32 = 19.0;
const META_H: f32 = 16.0;
const POINT_H: f32 = 17.0;
const STORY_GAP: f32 = 8.0;
const ROW_INSET: f32 = 8.0;
/// A story's words, clear of its accent bar.
const TEXT_INSET: f32 = 12.0;
/// How far one wheel notch scrolls the list.
const WHEEL_STEP: f32 = 48.0;

pub fn height(_st: &AppState) -> f32 {
    HEIGHT
}

/// While the page is open, its next look at the feed: a retry after a
/// failure, or the stories going stale (the app reads it then,
/// `App::today_shown`).
pub fn next_wake(st: &AppState) -> Option<f64> {
    let at = st.news.next_ask(halo_core::news::FRESH_S)?;
    Some(st.now + (at - st.unix).max(0) as f64 + 0.05)
}

pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect) {
    // Fixed layout height: the island's spring may still be opening.
    let area = Rect::new(area.x, area.y, area.w, HEIGHT);
    let compact = area.w < COMPACT_BELOW;
    let x = if compact { area.x } else { area.x + GRID_W + COL_GAP };
    let right = Rect::new(x, area.y, (area.right() - x).max(0.0), area.h);
    let e = Entrance::of(ui, st, PageKind::Today);
    let live = alive(ui, st, PageKind::Today);
    if !compact {
        draw_weeks_grid(ui, st, Rect::new(area.x, area.y, GRID_W, area.h), e, live);
    }
    draw_news(ui, st, right, e, live);
}

// ------------------------------------------------------------------ the weeks

/// The weekday, the date and this month's weeks, today filled. As the page
/// opens an accent rule draws under the date, the weekend columns and this
/// week's band rise in and today's disc pops with its halo, which breathes
/// for a few seconds (`entrance::alive`) before it rests.
fn draw_weeks_grid(ui: &mut Ui, st: &AppState, r: Rect, e: Entrance, live: Option<(f32, f32)>) {
    let t = st.local;
    if !valid_date(&t) {
        return;
    }
    let today = days_from_civil(t.year, t.month, t.day);
    let weekday = WEEKDAY_LONG[weekday_from_days(today) as usize % 7].to_uppercase();
    ui.c.text(&weekday, Rect::new(r.x, r.y, r.w, 16.0), &TextStyle::caption().weight(Weight::Bold).color(st.accent));
    let date = format!("{} {}", t.day, MONTH_LONG[(t.month.clamp(1, 12) - 1) as usize]);
    ui.c.text(&date, Rect::new(r.x, r.y + 16.0, r.w, 24.0), &TextStyle::value());
    let rule = e.draw(0.08, 0.5);
    if rule > 0.0 {
        let dw = ui.c.measure(&date, &TextStyle::value(), r.w).0.ceil();
        ui.c.fill_rrect(Rect::new(r.x, r.y + 42.0, (dw * rule).max(2.0), 2.0), 1.0, st.accent.with_alpha(0.6));
    }

    let top = r.y + GRID_Y + INITIALS_H;
    let first = days_from_civil(t.year, t.month, 1);
    let offset = (weekday_from_days(first) as i64 + 6) % 7;
    let days = halo_core::datetime::days_in_month(t.year, t.month) as i64;
    let rows = ((offset + days + 6) / 7) as f32;
    // The weekend's two columns, a shade lighter.
    let weekend = Rect::new(r.x + 5.0 * CELL_W + 1.0, r.y + GRID_Y - 3.0, 2.0 * CELL_W - 2.0, INITIALS_H + rows * CELL_H + 3.0);
    rise(ui, e.pop(0.1), |ui| ui.c.fill_rrect(weekend, 9.0, Color::white(0.035)));
    // This week: a band drawn across behind its row.
    let i = offset + t.day as i64 - 1;
    let band_w = GRID_W * e.draw(0.16, 0.5);
    if band_w > 1.0 {
        let band = Rect::new(r.x, top + (i / 7) as f32 * CELL_H - 1.0, band_w, 22.0);
        ui.c.fill_rrect(band, (band_w / 2.0).min(11.0), st.accent.with_alpha(0.13));
    }

    let initials_st = TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_FAINT).center();
    for (col, s) in INITIALS.iter().enumerate() {
        ui.c.text(s, Rect::new(r.x + col as f32 * CELL_W, r.y + GRID_Y, CELL_W, INITIALS_H), &initials_st);
    }
    let num_st = TextStyle::footnote().tabular().center();
    for d in 1..=days {
        let i = offset + d - 1;
        let (row, col) = ((i / 7) as f32, (i % 7) as usize);
        let cell = Rect::new(r.x + col as f32 * CELL_W, top + row * CELL_H, CELL_W, 20.0);
        let label = d.to_string();
        if d == t.day as i64 {
            let c = cell.center();
            let breathe = live.map_or(0.0, |(t, k)| (t * 2.4).sin() * 0.8 * k);
            scaled(ui, c, e.pop(0.24), |ui| {
                ui.c.fill_circle(c, 12.0 + breathe, st.accent.with_alpha(0.16));
                ui.c.fill_circle(c, 9.5, st.accent);
                // Light from above on the disc.
                ui.c.fill_circle(Point::new(c.x, c.y - 3.0), 5.0, Color::white(0.12));
            });
            ui.c.text(&label, cell, &num_st.weight(Weight::Bold).color(ink_on(st.accent)));
        } else {
            let ink = if d < t.day as i64 {
                Color::white(0.32)
            } else if col >= 5 {
                palette::TEXT_DIM
            } else {
                Color::white(0.9)
            };
            ui.c.text(&label, cell, &num_st.color(ink));
        }
    }
}

// ------------------------------------------------------------------ the news

/// What the news column says when it has no stories to list.
fn empty_line(st: &AppState) -> Option<String> {
    let n = &st.news;
    match (&n.last, &n.trouble) {
        (Some((news, _)), _) if !news.stories.is_empty() => None,
        (Some((news, _)), None) => Some(format!("No new stories from {} in the last day.", news.source)),
        (_, Some(f)) => Some(failure_line(*f)),
        (None, None) => Some("Getting the news\u{2026}".into()),
    }
}

fn failure_line(f: Failure) -> String {
    match f {
        Failure::NotConfigured => "Pick a news source in Settings › General.".into(),
        f => f.line("The news feed"),
    }
}

/// The heading's right end: when it was read, or why the last ask failed.
fn status(st: &AppState) -> (String, bool) {
    let n = &st.news;
    match (&n.last, &n.trouble) {
        (Some((_, at)), Some(f)) => (format!("{} · {}", f.short(), halo_core::net::age(st.unix, *at)), true),
        (Some((_, at)), None) => (format!("Updated {}", halo_core::net::age(st.unix, *at)), false),
        _ => (String::new(), false),
    }
}

/// One story's height: its headline and source line, and its key point when it has one.
fn story_h(s: &Story) -> f32 {
    TITLE_H + META_H + if s.point.is_some() { POINT_H } else { 0.0 } + STORY_GAP
}

fn draw_news(ui: &mut Ui, st: &mut AppState, r: Rect, e: Entrance, live: Option<(f32, f32)>) {
    if r.w < 120.0 {
        return;
    }
    let source = st.news.last.as_ref().map_or_else(|| feed_name(&st.settings.news_feed), |(n, _)| n.source.clone());
    let head = Rect::new(r.x + ROW_INSET, r.y + 2.0, r.w - 2.0 * ROW_INSET, HEAD_H - 2.0);
    let (said, warn) = status(st);
    // The feed's dot: accent while it's fresh (a ping ripples out of it for
    // a few seconds as the page opens), orange after a failure, faint before an answer.
    let dot = Point::new(head.x + 3.0, head.y + 7.0);
    let fresh = st.news.last.is_some() && !warn;
    let dot_c = if warn { palette::ORANGE } else if fresh { st.accent } else { palette::TEXT_FAINT };
    if let Some((t, k)) = live.filter(|_| fresh) {
        let p = (t * 0.7).fract();
        ui.c.fill_circle(dot, 3.0 + 6.0 * p, dot_c.with_alpha(0.45 * (1.0 - p) * k));
    }
    scaled(ui, dot, e.pop(0.05), |ui| ui.c.fill_circle(dot, 3.0, dot_c));
    let words = Rect::new(head.x + 11.0, head.y, head.w - 11.0, head.h);
    ui.c.text(&format!("TOP STORIES · {}", source.to_uppercase()), words, &TextStyle::caption().weight(Weight::Bold).color(st.accent));
    if !said.is_empty() {
        ui.c.text(&said, head, &TextStyle::caption().color(if warn { palette::ORANGE } else { palette::TEXT_FAINT }).right());
    }
    // A hairline in the accent under the heading, drawn across as the page
    // opens, its bright head leading.
    let rule = e.draw(0.05, 0.6);
    if rule > 0.0 {
        let y = r.y + HEAD_H - 2.0;
        ui.c.fill_rect(Rect::new(head.x, y, head.w * rule, 1.0), st.accent.with_alpha(0.35));
        if rule < 1.0 {
            ui.c.fill_rect(Rect::new(head.x + head.w * rule - 18.0, y, 18.0, 1.0), st.accent.with_alpha(0.8 * (1.0 - rule)));
        }
    }
    let list = Rect::new(r.x, r.y + HEAD_H, r.w, r.h - HEAD_H);
    if let Some(line) = empty_line(st) {
        let style = TextStyle::body().color(palette::TEXT_DIM).wrap().top();
        ui.c.text(&line, Rect::new(list.x + ROW_INSET, list.y + 6.0, list.w - 2.0 * ROW_INSET, list.h - 6.0), &style);
        if st.news.last.is_none() && st.news.trouble.is_none() {
            draw_skeleton(ui, Rect::new(list.x, list.y + 34.0, list.w, list.h - 34.0), e, live);
        }
        return;
    }
    let Some(news): Option<News> = st.news.last.as_ref().map(|(n, _)| n.clone()) else { return };
    let total: f32 = news.stories.iter().map(story_h).sum();
    let max = (total - list.h).max(0.0);
    let notches = ui.wheel_in(list);
    st.ui.today_scroll = (st.ui.today_scroll - notches * WHEEL_STEP).clamp(0.0, max);
    let scroll = glide(ui, st, max);
    ui.c.clip(list);
    let mut y = list.y - scroll;
    for (i, s) in news.stories.iter().enumerate() {
        let h = story_h(s);
        let row = Rect::new(list.x, y, list.w, h - STORY_GAP / 2.0);
        y += h;
        if row.bottom() <= list.y || row.y >= list.bottom() {
            continue;
        }
        // Only a story wholly in view takes clicks.
        let whole = row.y >= list.y - 0.5 && row.bottom() <= list.bottom() + 0.5;
        let id = wid_n("today.story", i as u64);
        let (hovered, pressed) = match (&s.link, whole) {
            (Some(link), true) => {
                let resp = ui.button(id, row);
                if resp.clicked {
                    st.cmd(Command::OpenUrl(link.clone()));
                }
                (resp.hovered, resp.pressed)
            }
            _ => (false, false),
        };
        let h = hover(ui, st, id, hovered);
        let n = i.min(5) as f32;
        draw_story(ui, st, row, s, h, pressed, (e.pop(0.12 + 0.06 * n), e.draw(0.2 + 0.06 * n, 0.45)));
    }
    ui.c.unclip();
    // Where the list is: a thin thumb on the right edge, brighter under the pointer.
    if max > 0.0 {
        let th = (list.h * list.h / total).max(18.0);
        let ty = list.y + (list.h - th) * scroll / max;
        let a = if ui.hovered(list) { 0.3 } else { 0.14 };
        rise(ui, e.pop(0.3), |ui| ui.c.fill_rrect(Rect::new(list.right() - 3.0, ty, 2.0, th), 1.0, Color::white(a)));
    }
}

/// Where the list shows this frame: gliding to `today_scroll` rather than
/// jumping a wheel notch (under 0.2 s), at once with motion off.
fn glide(ui: &mut Ui, st: &mut AppState, max: f32) -> f32 {
    let target = st.ui.today_scroll;
    let (shown, at) = st.ui.today_scroll_shown;
    let dt = (ui.now - at).clamp(0.0, 0.1) as f32;
    let mut now = (target + (shown - target) * (-dt * 22.0).exp()).clamp(0.0, max);
    if (now - target).abs() < 0.3 || still(st) {
        now = target;
    } else {
        ui.request_anim();
    }
    st.ui.today_scroll_shown = (now, ui.now);
    now
}

/// A story as a card: a faint face that rises in, an accent bar on its left
/// that grows down (`k`: the face's pop, the bar's draw). Under the pointer
/// the face and the bar brighten and an arrow says it opens; pressed, it
/// dips a little.
fn draw_story(ui: &mut Ui, st: &AppState, r: Rect, s: &Story, hover: f32, pressed: bool, (face, bar): (f32, f32)) {
    let dip = if pressed { 0.985 } else { 1.0 };
    scaled(ui, r.center(), dip, |ui| {
        rise(ui, face, |ui| {
            ui.c.fill_rrect(r, 10.0, Color::white(0.035 + 0.075 * hover));
            if hover > 0.0 {
                ui.c.stroke_rrect(r, 10.0, st.accent.with_alpha(0.25 * hover), 1.0);
            }
        });
        if bar > 0.0 {
            let bh = (r.h - 14.0) * bar;
            ui.c.fill_rrect(Rect::new(r.x + 3.0, r.y + 7.0, 2.5 + hover, bh), 1.5, st.accent.with_alpha(0.55 + 0.45 * hover));
        }
    });
    let x = r.x + TEXT_INSET;
    let w = (r.w - TEXT_INSET - ROW_INSET).max(0.0);
    let link_w = if s.link.is_some() { 18.0 } else { 0.0 };
    ui.c.text(&s.title, Rect::new(x, r.y + 1.0, (w - link_w).max(0.0), TITLE_H), &TextStyle::body().weight(Weight::Semibold));
    if hover > 0.0 && s.link.is_some() {
        let a = Rect::new(r.right() - ROW_INSET - 12.0 + 3.0 * (1.0 - hover), r.y + 4.0, 12.0, 12.0);
        ui.c.icon(Icon::Open, a, st.accent.with_alpha(hover));
    }
    let when = s.at.map(|t| format!(" · {}", halo_core::net::age(st.unix, t))).unwrap_or_default();
    ui.c.text(&format!("{}{when}", s.source), Rect::new(x, r.y + 1.0 + TITLE_H, w, META_H), &TextStyle::caption().color(palette::TEXT_FAINT));
    if let Some(p) = &s.point {
        ui.c.text(p, Rect::new(x, r.y + 1.0 + TITLE_H + META_H, w, POINT_H), &TextStyle::footnote().color(palette::TEXT_DIM));
    }
}

/// While the first answer is on its way: three story shapes, rising in one
/// after another, a light sweeping over them for a few seconds.
fn draw_skeleton(ui: &mut Ui, r: Rect, e: Entrance, live: Option<(f32, f32)>) {
    let w = (r.w - TEXT_INSET - ROW_INSET).max(0.0);
    for i in 0..3 {
        let y = r.y + i as f32 * 42.0;
        if y + 34.0 > r.bottom() {
            break;
        }
        rise(ui, e.pop(0.15 + 0.07 * i as f32), |ui| {
            ui.c.fill_rrect(Rect::new(r.x, y, r.w, 36.0), 10.0, Color::white(0.03));
            ui.c.fill_rrect(Rect::new(r.x + TEXT_INSET, y + 8.0, w * [0.82, 0.7, 0.76][i], 9.0), 4.5, Color::white(0.07));
            ui.c.fill_rrect(Rect::new(r.x + TEXT_INSET, y + 22.0, w * 0.36, 6.0), 3.0, Color::white(0.05));
        });
    }
    if let Some((t, k)) = live {
        let x = r.x - 40.0 + (r.w + 80.0) * (t * 0.6).fract();
        ui.c.clip(Rect::new(r.x + 10.0, r.y, (r.w - 20.0).max(0.0), r.h));
        ui.c.fill_rect(Rect::new(x - 20.0, r.y, 40.0, 3.0 * 42.0 - 6.0), Color::white(0.04 * k));
        ui.c.unclip();
    }
}

/// The picked feed's name before its first answer.
fn feed_name(setting: &str) -> String {
    match halo_core::news::preset_index(setting) {
        Some(i) => halo_core::news::PRESETS[i].0.to_string(),
        None => "News".into(),
    }
}

/// The app fills `local` from GetLocalTime; guard against the zeroed default.
fn valid_date(t: &LocalDateTime) -> bool {
    (1..=12).contains(&t.month) && t.day >= 1
}

/// Text colour that stays legible on an accent fill.
fn ink_on(fill: Color) -> Color {
    if fill.luminance() > 0.6 {
        palette::ON_ACCENT_DARK
    } else {
        palette::ON_ACCENT
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo_core::settings::Settings;
    use halo_core::store::Store;

    fn state() -> AppState {
        AppState::new(Settings::default(), Store::default())
    }

    /// Loading, success, empty, offline, timeout and malformed: each says
    /// what it is, and a failure keeps the last stories with their age.
    #[test]
    fn the_news_column_says_where_it_stands() {
        let mut st = state();
        st.unix = 1_794_564_000;
        assert_eq!(empty_line(&st).as_deref(), Some("Getting the news…"));
        assert_eq!(feed_name(&st.settings.news_feed), "BBC World");
        let (first, last) = (halo_core::news::RETRY_FIRST_S, halo_core::news::RETRY_LAST_S);
        st.news.came(Err(Failure::Offline), st.unix, first, last);
        assert!(empty_line(&st).unwrap().starts_with("No internet"));
        st.news.came(Err(Failure::Timeout), st.unix, first, last);
        assert_eq!(empty_line(&st).unwrap(), "The news feed didn't answer in time. Trying again soon.");
        st.news.came(Err(Failure::Malformed), st.unix, first, last);
        assert_eq!(empty_line(&st).unwrap(), "The news feed sent an answer Halo Island can't read.");
        st.news.came(Ok(halo_core::news::sample(st.unix)), st.unix, first, last);
        assert_eq!((empty_line(&st), status(&st)), (None, ("Updated just now".into(), false)));
        // A later failure keeps the stories and says why, with their age.
        st.unix += 1800;
        st.news.came(Err(Failure::Timeout), st.unix, first, last);
        assert_eq!((empty_line(&st), status(&st)), (None, ("Timed out · 30 min ago".into(), true)));
        // A feed with no current story: an honest empty list.
        st.news.came(Ok(News { source: "Quiet".into(), stories: vec![] }), st.unix, first, last);
        assert_eq!(empty_line(&st).as_deref(), Some("No new stories from Quiet in the last day."));
        // No real address: where to pick one.
        st.news = Default::default();
        st.news.came(Err(Failure::NotConfigured), st.unix, first, last);
        assert_eq!(empty_line(&st).as_deref(), Some("Pick a news source in Settings › General."));
    }

    #[test]
    fn the_page_looks_again_only_when_due() {
        let mut st = state();
        st.unix = 1_000;
        st.now = 50.0;
        assert_eq!(next_wake(&st), None, "nothing asked yet: the page asks as it opens");
        st.news.came(Ok(halo_core::news::sample(st.unix)), st.unix, 30, 600);
        assert_eq!(next_wake(&st), Some(50.0 + halo_core::news::FRESH_S as f64 + 0.05));
        st.news.came(Err(Failure::Offline), st.unix, 30, 600);
        assert_eq!(next_wake(&st), Some(50.0 + 30.0 + 0.05));
    }
}

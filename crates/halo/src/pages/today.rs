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

use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::state::AppState;
use crate::ui::{wid_n, Ui};
use halo_core::datetime::{days_from_civil, weekday_from_days, LocalDateTime};
use halo_core::geom::{palette, Color, Rect};
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
    if !compact {
        draw_weeks_grid(ui, st, Rect::new(area.x, area.y, GRID_W, area.h));
    }
    draw_news(ui, st, right);
}

// ------------------------------------------------------------------ the weeks

/// The weekday, the date and this month's weeks, today filled.
fn draw_weeks_grid(ui: &mut Ui, st: &AppState, r: Rect) {
    let t = st.local;
    if !valid_date(&t) {
        return;
    }
    let today = days_from_civil(t.year, t.month, t.day);
    let weekday = WEEKDAY_LONG[weekday_from_days(today) as usize % 7].to_uppercase();
    ui.c.text(&weekday, Rect::new(r.x, r.y, r.w, 16.0), &TextStyle::caption().weight(Weight::Bold).color(st.accent));
    let date = format!("{} {}", t.day, MONTH_LONG[(t.month.clamp(1, 12) - 1) as usize]);
    ui.c.text(&date, Rect::new(r.x, r.y + 16.0, r.w, 24.0), &TextStyle::value());

    let initials_st = TextStyle::caption().weight(Weight::Semibold).color(palette::TEXT_FAINT).center();
    for (col, s) in INITIALS.iter().enumerate() {
        ui.c.text(s, Rect::new(r.x + col as f32 * CELL_W, r.y + GRID_Y, CELL_W, INITIALS_H), &initials_st);
    }
    let top = r.y + GRID_Y + INITIALS_H;
    let first = days_from_civil(t.year, t.month, 1);
    let offset = (weekday_from_days(first) as i64 + 6) % 7;
    let num_st = TextStyle::footnote().tabular().center();
    for d in 1..=halo_core::datetime::days_in_month(t.year, t.month) as i64 {
        let i = offset + d - 1;
        let (row, col) = ((i / 7) as f32, (i % 7) as usize);
        let cell = Rect::new(r.x + col as f32 * CELL_W, top + row * CELL_H, CELL_W, 20.0);
        let label = d.to_string();
        if d == t.day as i64 {
            ui.c.fill_circle(cell.center(), 9.5, st.accent);
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

fn draw_news(ui: &mut Ui, st: &mut AppState, r: Rect) {
    if r.w < 120.0 {
        return;
    }
    let source = st.news.last.as_ref().map_or_else(|| feed_name(&st.settings.news_feed), |(n, _)| n.source.clone());
    let head = Rect::new(r.x + ROW_INSET, r.y + 2.0, r.w - 2.0 * ROW_INSET, HEAD_H - 2.0);
    ui.c.text(&format!("TOP STORIES · {}", source.to_uppercase()), head, &TextStyle::caption().weight(Weight::Bold).color(st.accent));
    let (said, warn) = status(st);
    if !said.is_empty() {
        ui.c.text(&said, head, &TextStyle::caption().color(if warn { palette::ORANGE } else { palette::TEXT_FAINT }).right());
    }
    let list = Rect::new(r.x, r.y + HEAD_H, r.w, r.h - HEAD_H);
    if let Some(line) = empty_line(st) {
        let style = TextStyle::body().color(palette::TEXT_DIM).wrap().top();
        ui.c.text(&line, Rect::new(list.x + ROW_INSET, list.y + 6.0, list.w - 2.0 * ROW_INSET, list.h - 6.0), &style);
        return;
    }
    let Some(news): Option<News> = st.news.last.as_ref().map(|(n, _)| n.clone()) else { return };
    let total: f32 = news.stories.iter().map(story_h).sum();
    let max = (total - list.h).max(0.0);
    let notches = ui.wheel_in(list);
    st.ui.today_scroll = (st.ui.today_scroll - notches * WHEEL_STEP).clamp(0.0, max);
    ui.c.clip(list);
    let mut y = list.y - st.ui.today_scroll;
    for (i, s) in news.stories.iter().enumerate() {
        let h = story_h(s);
        let row = Rect::new(list.x, y, list.w, h - STORY_GAP / 2.0);
        y += h;
        if row.bottom() <= list.y || row.y >= list.bottom() {
            continue;
        }
        // Only a story wholly in view takes clicks.
        let whole = row.y >= list.y - 0.5 && row.bottom() <= list.bottom() + 0.5;
        let hovered = match (&s.link, whole) {
            (Some(link), true) => {
                let resp = ui.button(wid_n("today.story", i as u64), row);
                if resp.clicked {
                    st.cmd(Command::OpenUrl(link.clone()));
                }
                resp.hovered
            }
            _ => false,
        };
        draw_story(ui, st, row, s, hovered);
    }
    ui.c.unclip();
}

fn draw_story(ui: &mut Ui, st: &AppState, r: Rect, s: &Story, hovered: bool) {
    if hovered {
        ui.c.fill_rrect(r, 8.0, palette::SURFACE_HOVER);
    }
    let x = r.x + ROW_INSET;
    let w = (r.w - 2.0 * ROW_INSET).max(0.0);
    ui.c.text(&s.title, Rect::new(x, r.y + 1.0, w, TITLE_H), &TextStyle::body().weight(Weight::Semibold));
    let when = s.at.map(|t| format!(" · {}", halo_core::net::age(st.unix, t))).unwrap_or_default();
    ui.c.text(&format!("{}{when}", s.source), Rect::new(x, r.y + 1.0 + TITLE_H, w, META_H), &TextStyle::caption().color(palette::TEXT_FAINT));
    if let Some(p) = &s.point {
        ui.c.text(p, Rect::new(x, r.y + 1.0 + TITLE_H + META_H, w, POINT_H), &TextStyle::footnote().color(palette::TEXT_DIM));
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

//! Expanded-island pages. Each page module exposes:
//!
//! ```ignore
//! pub fn height(st: &AppState) -> f32;                        // content height (DIPs)
//! pub fn draw(ui: &mut Ui, st: &mut AppState, area: Rect);    // draw + handle input
//! ```
//!
//! Pages mutate `AppState` directly for local state and push
//! [`crate::commands::Command`]s for side effects (media control, opening
//! files, saving, …).

pub mod addons;
pub mod caffeine;
pub mod card;
pub mod downloads;
pub mod downloads_offer;
pub mod entrance;
pub mod focus;
pub mod games;
pub mod media;
pub mod notifications;
pub mod prayer;
pub mod record;
pub mod script;
pub mod settings;
pub mod shelf;
pub mod stocks;
pub mod system;
pub mod tasks;
pub mod today;
pub mod usage;
pub mod widgets;

use crate::commands::Command;
use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::state::AppState;
use crate::ui::{wid, wid_n, Input, Ui};
use halo_core::datetime::{fmt_clock, fmt_date_short};
use halo_core::geom::{lerp, palette, Color, Point, Rect};

/// Height of the header band (tabs + clock) above the page content. The
/// header row itself is a fixed 36 DIPs ([`header_rect`]); the rest is the
/// 12 DIP gap between the tab pills and the first line of content.
pub const HEADER_H: f32 = 50.0;
/// Height of the header row (the 28 DIP tab pills sit centred in it).
const HEADER_ROW_H: f32 = 36.0;
/// Horizontal padding inside the expanded island, also used below the content.
pub const PAD: f32 = 18.0;

/// The header row inside the expanded island `body`. Fixed size, so changing
/// [`HEADER_H`] only moves the content, never the tabs.
pub fn header_rect(body: Rect) -> Rect {
    Rect::new(body.x + PAD - 6.0, body.y + 6.0, body.w - 2.0 * PAD + 12.0, HEADER_ROW_H)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageKind {
    Media,
    Today,
    Shelf,
    Tasks,
    Focus,
    /// AI usage: how much of each AI coding plan is left (`pages::usage`).
    /// Part of Studio Pro: in the row with it, until switched off.
    Ai,
    /// The System add-on's page, while it is on ([`tabs`]).
    System,
    /// Show over games and its frame rate: Windows' only, with System ([`tabs`]).
    Games,
    /// The teleprompter: a script read under the webcam.
    Script,
    /// Screen recording with Halo Island Recorder, while its add-on is on
    /// ([`tabs`]): what to record, then the recording under way.
    Record,
    /// Prayer times, while that add-on is on ([`tabs`]).
    Prayer,
    /// The KLCI and the person's stocks, while the Stocks add-on is on ([`tabs`]).
    Stocks,
    /// The day's caffeine, while the Caffeine add-on is on ([`tabs`]).
    Caffeine,
    /// Links to save as video or sound, while the Downloader add-on is on ([`tabs`]).
    Downloads,
    /// Which apps' notifications show on the island, while Notifications is
    /// on ([`tabs`]; Windows' Store copy only).
    Notifications,
    /// The "+" at the end of the tab row: features to switch on, each of
    /// which downloads only then (`halo_core::addons`).
    Addons,
    /// Behind the gear on the right of the header, never in the tab row.
    Settings,
    /// Halo Island: the widget slots, water and the clipboard (`widgets`),
    /// while the Widgets add-on is on ([`tabs`]).
    Widgets,
}

impl PageKind {
    /// The tab row's order: the island's own pages, then the add-ons'
    /// pages and the "+".
    pub const ALL: [PageKind; 18] = [
        PageKind::Media,
        PageKind::Widgets,
        PageKind::Today,
        PageKind::Shelf,
        PageKind::Tasks,
        PageKind::Focus,
        PageKind::Ai,
        PageKind::System,
        PageKind::Games,
        PageKind::Script,
        PageKind::Record,
        PageKind::Prayer,
        PageKind::Stocks,
        PageKind::Caffeine,
        PageKind::Downloads,
        PageKind::Notifications,
        PageKind::Addons,
        PageKind::Settings,
    ];

    pub fn key(self) -> &'static str {
        match self {
            PageKind::Media => "media",
            PageKind::Focus => "focus",
            PageKind::Ai => "ai",
            PageKind::Shelf => "shelf",
            PageKind::Tasks => "tasks",
            PageKind::Today => "today",
            PageKind::System => "system",
            PageKind::Games => "games",
            PageKind::Script => "script",
            PageKind::Record => "record",
            PageKind::Prayer => "prayer",
            PageKind::Stocks => "stocks",
            PageKind::Caffeine => "caffeine",
            PageKind::Downloads => "downloads",
            PageKind::Notifications => "notifications",
            PageKind::Addons => "addons",
            PageKind::Settings => "settings",
            PageKind::Widgets => "widgets",
        }
    }

    pub fn from_key(s: &str) -> PageKind {
        // Health (and the Water page before it) left the app: Music, as for any key it doesn't know.
        let s = s.trim();
        Self::ALL.into_iter().find(|p| p.key().eq_ignore_ascii_case(s)).unwrap_or(PageKind::Media)
    }

    pub fn title(self) -> &'static str {
        match self {
            PageKind::Media => "Music",
            PageKind::Focus => "Focus",
            PageKind::Ai => "AI",
            PageKind::Shelf => "Shelf",
            PageKind::Tasks => "Tasks",
            PageKind::Today => "Today",
            PageKind::System => "System",
            PageKind::Games => "Games",
            PageKind::Script => "Script",
            PageKind::Record => "Record",
            PageKind::Prayer => "Prayer",
            PageKind::Stocks => "Stocks",
            PageKind::Caffeine => "Caffeine",
            PageKind::Downloads => "Downloads",
            PageKind::Notifications => "Notifications",
            PageKind::Addons => "Add-ons",
            PageKind::Settings => "Settings",
            PageKind::Widgets => "Widgets",
        }
    }

    pub fn icon(self) -> Icon {
        match self {
            PageKind::Media => Icon::Music,
            PageKind::Focus => Icon::Timer,
            PageKind::Ai => Icon::Ai,
            PageKind::Shelf => Icon::Tray,
            PageKind::Tasks => Icon::ListChecks,
            PageKind::Today => Icon::Calendar,
            PageKind::System => Icon::Gauge,
            PageKind::Games => Icon::Gamepad,
            PageKind::Script => Icon::Script,
            PageKind::Record => Icon::Recorder,
            PageKind::Prayer => Icon::Moon,
            PageKind::Stocks => Icon::Stocks,
            PageKind::Caffeine => Icon::Coffee,
            PageKind::Downloads => Icon::Download,
            PageKind::Notifications => Icon::Bell,
            PageKind::Addons => Icon::Addons,
            PageKind::Settings => Icon::Gear,
            PageKind::Widgets => Icon::App,
        }
    }

    /// Accent tint for the tab's tile in the in-island overflow popover.
    pub fn tint(self) -> Color {
        match self {
            PageKind::Media => palette::PINK,
            PageKind::Widgets => palette::CYAN,
            PageKind::Today => palette::RED,
            PageKind::Shelf => palette::BLUE,
            PageKind::Tasks => palette::ORANGE,
            PageKind::Focus => Color::hex(0x5E5CE6),
            PageKind::Ai => palette::PURPLE,
            PageKind::System => Color::hex(0x636366),
            PageKind::Games => palette::PINK,
            PageKind::Script => Color::hex(0xA2845E),
            PageKind::Record => palette::RED,
            PageKind::Prayer => Color::hex(0x30B0A0),
            PageKind::Stocks => palette::GREEN,
            PageKind::Caffeine => Color::hex(0xA2845E),
            PageKind::Downloads => palette::BLUE,
            PageKind::Notifications => palette::ORANGE,
            PageKind::Addons => Color::hex(0x0A84FF),
            PageKind::Settings => Color::hex(0x8E8E93),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    /// Position in the tab row (for slide direction). Settings comes last:
    /// its gear sits right of the tabs.
    pub fn tab_index(self) -> usize {
        self.index()
    }

    /// Has a tab in the header's row (Settings has the gear instead).
    pub fn tabbed(self) -> bool {
        self != PageKind::Settings
    }

    /// The island's own pages, which Settings can take off the row, and
    /// Notifications, which goes off with its tab (it's offered only where
    /// the copy can read notifications). The add-ons' pages come and go
    /// with their switches, AI's with Studio Pro.
    pub const HIDEABLE: [PageKind; 6] = [PageKind::Media, PageKind::Today, PageKind::Shelf, PageKind::Tasks, PageKind::Focus, PageKind::Notifications];

    pub fn hideable(self) -> bool {
        Self::HIDEABLE.contains(&self)
    }
}

/// The tabs in the header: every page but Settings, AI only with Studio Pro,
/// and System (with Games on Windows), Script, Record, Prayer and Stocks only while their
/// add-ons are on.
/// Music, Today, Shelf, Tasks and Focus unless Settings took them off the
/// row; one of those still shows while its page is open (a file dropped on
/// the island opens the Shelf). Notifications wherever the copy can read
/// them, unless Settings switched it off. Add-ons is last, in both apps.
pub fn tabs(st: &AppState) -> Vec<PageKind> {
    row(st, Some(st.page))
}

/// The tabs Settings can switch off in this copy (its tabs row, and Tabs in
/// the Settings window): the island's own pages, and Notifications, the
/// last of [`PageKind::HIDEABLE`], only where the copy can read them
/// (`AppState::notify_can`).
pub fn hideable_tabs(st: &AppState) -> &'static [PageKind] {
    let all = &PageKind::HIDEABLE;
    if st.notify_can {
        all
    } else {
        &all[..all.len() - 1]
    }
}

/// Where the island goes when its page leaves the row (an add-on switched
/// off, a tab hidden): the first tab, Music unless it's hidden.
pub fn first_tab(st: &AppState) -> PageKind {
    // Never empty: Add-ons always has a tab.
    row(st, None)[0]
}

fn row(st: &AppState, open: Option<PageKind>) -> Vec<PageKind> {
    let mut visible: Vec<PageKind> = PageKind::ALL
        .into_iter()
        .filter(|&p| match p {
            PageKind::Ai => st.ai_usage_on(),
            PageKind::System => st.system_on(),
            PageKind::Games => st.games_on(),
            PageKind::Prayer => st.prayer_on(),
            PageKind::Script => st.script_on(),
            PageKind::Stocks => st.stocks_on(),
            PageKind::Caffeine => st.caffeine_on(),
            PageKind::Widgets => st.widgets_on(),
            PageKind::Downloads => st.downloader_on(),
            PageKind::Record => st.record_on(),
            PageKind::Notifications => st.notifications_on(),
            p if p.hideable() => !st.tab_hidden(p) || open == Some(p),
            p => p.tabbed(),
        })
        .collect();
    if !st.settings.tab_order.is_empty() {
        let custom = &st.settings.tab_order;
        visible.sort_by_key(|&p| {
            if p == PageKind::Addons {
                (2usize, PageKind::ALL.len())
            } else if let Some(pos) = custom.iter().position(|k| k.eq_ignore_ascii_case(p.key())) {
                (0usize, pos)
            } else {
                (1usize, p.index())
            }
        });
    }
    visible
}

/// The tab after (or before) `p` among [`tabs`], wrapping around.
pub fn step_tab(p: PageKind, st: &AppState, forward: bool) -> PageKind {
    let tabs = tabs(st);
    let n = tabs.len();
    let i = tabs.iter().position(|t| *t == p).unwrap_or(0);
    tabs[if forward { (i + 1) % n } else { (i + n - 1) % n }]
}

pub fn content_height(p: PageKind, st: &AppState) -> f32 {
    match p {
        PageKind::Media => media::height(st),
        PageKind::Focus => focus::height(st),
        PageKind::Ai => usage::height(st),
        PageKind::Shelf => shelf::height(st),
        PageKind::Tasks => tasks::height(st),
        PageKind::Today => today::height(st),
        PageKind::System => system::height(st),
        PageKind::Games => games::height(st),
        PageKind::Script => script::height(st),
        PageKind::Record => record::height(st),
        PageKind::Prayer => prayer::height(st),
        PageKind::Stocks => stocks::height(st),
        PageKind::Caffeine => caffeine::height(st),
        PageKind::Downloads => downloads::height(st),
        PageKind::Notifications => notifications::height(st),
        PageKind::Addons => addons::height(st),
        PageKind::Settings => settings::height(st),
        PageKind::Widgets => widgets::height(st),
    }
}

/// The next time (mono seconds) the expanded `page` changes on its own and
/// needs a frame without input, if any (e.g. an Undo pill expiring). Meant
/// for the app's wake timer; until it asks, such changes show on the next
/// repaint.
pub fn next_wake(page: PageKind, st: &AppState) -> Option<f64> {
    let wake = match page {
        PageKind::Tasks => tasks::next_wake(st),
        PageKind::Shelf => shelf::next_wake(st),
        PageKind::Script => script::next_wake(st),
        PageKind::Record => record::next_wake(st),
        PageKind::Prayer => prayer::next_wake(st),
        PageKind::Today => today::next_wake(st),
        PageKind::Downloads => downloads::next_wake(st),
        PageKind::Widgets => widgets::next_wake(st),
        PageKind::Stocks => stocks::next_wake(st),
        _ => None,
    };
    // Halo beside the pin in his theme blinks and flicks an ear now and
    // then; asleep at night, he stays still.
    let halo = (!crate::activities::night(st)).then(|| crate::face::next_rest_move(st.now));
    let toast = st.ui.toast.as_ref().map(|(_, until)| (until - TOAST_FADE_S).max(st.now));
    [wake, halo, toast].into_iter().flatten().reduce(f64::min)
}

/// A page switch in progress: the page we came from, 0..1 progress, and the
/// slide direction (+1 = moving to a tab on the right).
#[derive(Clone, Copy, Debug)]
pub struct PageTransition {
    pub from: PageKind,
    pub t: f32,
    pub dir: f32,
}

/// How far (DIPs) pages slide during a switch.
const SLIDE: f32 = 20.0;
/// The old page is gone by this much of the switch...
const OUT_END: f32 = 0.4;
/// ...while the new one fades in over this window.
const IN_START: f32 = 0.1;
const IN_LEN: f32 = 0.5;
/// The header's selection pill and labels settle by this much of the switch.
const TABS_END: f32 = 0.6;

/// Draw the header and the active page inside the expanded island `body`.
/// During a page switch the old page fades out sliding away while the new
/// one fades in; the old page is drawn without input (it can't be clicked).
/// `body` is the island at its final size, so nothing reflows while it opens.
pub fn draw_expanded(ui: &mut Ui, st: &mut AppState, body: Rect, page: PageKind, transition: Option<PageTransition>) {
    if crate::pill::expanded::draw(ui, st, body, page, transition) {
        if st.ui.quick_open {
            draw_quick_switcher(ui, st, body);
        }
        draw_toast(ui, st, body);
        return;
    }
    let tr = transition.filter(|tr| tr.t < 1.0 && tr.from != page);
    let header = header_rect(body);
    let (tip, popover) = draw_header(ui, st, header, page, tr);
    if popover.is_some() || st.ui.quick_open {
        // Draw page content inert while a modal popover or Quick Switcher is open
        // so clicks never hit controls behind it.
        let mut ghost_input = Input::default();
        let mut ghost_hits = Vec::new();
        let now = ui.now;
        let mut ghost = Ui::new(&mut *ui.c, &mut ghost_input, &mut ghost_hits, now);
        draw_pages(&mut ghost, st, body, header, page, tr);
        if ghost.repaint_requested() {
            ui.request_anim();
        }
    } else {
        draw_pages(ui, st, body, header, page, tr);
    }
    if st.ui.quick_open {
        draw_quick_switcher(ui, st, body);
    } else if let Some(popover) = popover {
        draw_popover(ui, st, body, popover);
    } else if let Some(tip) = tip {
        // Hover labels sit on top of the page, outside the content clip.
        draw_tip(ui, header, &tip);
    }
    draw_toast(ui, st, body);
}

const TOAST_FADE_S: f64 = 0.30;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QuickAction {
    Page(PageKind),
    NewTask,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct QuickItem {
    pub action: QuickAction,
    pub label: &'static str,
    pub icon: Icon,
    pub tint: Color,
    pub hint: Option<&'static str>,
}

const CTRL_HINTS: [&str; 9] = ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4", "Ctrl+5", "Ctrl+6", "Ctrl+7", "Ctrl+8", "Ctrl+9"];

/// Matching tabs and quick actions for the `Ctrl+K` Quick Switcher.
pub(crate) fn quick_matches(st: &AppState, query: &str) -> Vec<QuickItem> {
    let q = query.trim().to_ascii_lowercase();
    let mut items = Vec::new();
    for (i, p) in tabs(st).into_iter().enumerate() {
        items.push(QuickItem {
            action: QuickAction::Page(p),
            label: p.title(),
            icon: p.icon(),
            tint: p.tint(),
            hint: CTRL_HINTS.get(i).copied(),
        });
    }
    items.push(QuickItem {
        action: QuickAction::Page(PageKind::Settings),
        label: PageKind::Settings.title(),
        icon: PageKind::Settings.icon(),
        tint: PageKind::Settings.tint(),
        hint: None,
    });
    items.push(QuickItem {
        action: QuickAction::NewTask,
        label: "New task",
        icon: Icon::Plus,
        tint: palette::GREEN,
        hint: None,
    });
    if q.is_empty() {
        items
    } else {
        items.into_iter().filter(|it| it.label.to_ascii_lowercase().contains(&q)).collect()
    }
}

fn run_quick_action(ui: &mut Ui, st: &mut AppState, action: QuickAction) {
    st.ui.quick_open = false;
    st.ui.quick_query.clear();
    st.ui.quick_sel = 0;
    match action {
        QuickAction::Page(p) => {
            st.cmd(Command::SetPage(p));
            ui.blur();
        }
        QuickAction::NewTask => {
            st.ui.task_parent = None;
            st.cmd(Command::SetPage(PageKind::Tasks));
            ui.blur();
        }
    }
}

/// Floating `Ctrl+K` Quick Switcher inside the expanded island.
fn draw_quick_switcher(ui: &mut Ui, st: &mut AppState, body: Rect) {
    let scrim = ui.button(wid("quick.scrim"), body);
    ui.c.fill_rrect(body, 20.0, Color::rgba(0.0, 0.0, 0.0, 0.34));

    let items = quick_matches(st, &st.ui.quick_query);
    let shown = items.len().clamp(1, 5);
    let row_h = 28.0;
    let card_w = (body.w - 40.0).min(300.0).max(180.0);
    let card_h = 34.0 + 8.0 + shown as f32 * row_h + 12.0;
    let card = Rect::new(body.center().x - card_w * 0.5, (body.y + 18.0).min(body.bottom() - card_h - 8.0).max(body.y + 8.0), card_w, card_h);

    let card_resp = ui.button(wid("quick.card"), card);
    ui.c.fill_rrect(card, 14.0, palette::ISLAND.with_alpha(0.96));
    ui.c.stroke_rrect(card, 14.0, Color::white(0.14), 1.0);

    // Pull Up/Down before `text_input` consumes `ui.input.keys`.
    let mut step: isize = 0;
    ui.input.keys.retain(|k| match k {
        crate::ui::KeyInput::Up => {
            step -= 1;
            false
        }
        crate::ui::KeyInput::Down => {
            step += 1;
            false
        }
        _ => true,
    });
    if !items.is_empty() {
        let len = items.len() as isize;
        let cur = (st.ui.quick_sel as isize).min(len - 1);
        st.ui.quick_sel = (cur + step).rem_euclid(len) as usize;
    } else {
        st.ui.quick_sel = 0;
    }

    let input_r = Rect::new(card.x + 8.0, card.y + 8.0, card.w - 16.0, 30.0);
    let iresp = ui.text_input(wid("quick.input"), input_r, &mut st.ui.quick_query, "Jump to tab or action…", 40);
    if iresp.changed {
        st.ui.quick_sel = 0;
    }

    let list_top = input_r.bottom() + 6.0;
    let mut picked: Option<QuickAction> = None;
    if items.is_empty() {
        ui.c.text("No matching tab", Rect::new(card.x + 14.0, list_top, card.w - 28.0, row_h), &TextStyle::footnote().color(palette::TEXT_FAINT));
    } else {
        let start = st.ui.quick_sel.saturating_sub(shown - 1);
        let end = (start + shown).min(items.len());
        for (slot, item) in items[start..end].iter().enumerate() {
            let idx = start + slot;
            let rr = Rect::new(card.x + 8.0, list_top + slot as f32 * row_h, card.w - 16.0, row_h);
            let rresp = ui.button(wid_n("quick.row", idx as u64), rr);
            if rresp.hovered {
                st.ui.quick_sel = idx;
            }
            if idx == st.ui.quick_sel {
                ui.c.fill_rrect(rr, 8.0, Color::white(0.10));
            }
            let icon_r = Rect::from_center(Point::new(rr.x + 14.0, rr.center().y), 14.0, 14.0);
            ui.c.icon(item.icon, icon_r, item.tint);
            let mut label_right = rr.right() - 8.0;
            if let Some(hint) = item.hint {
                let hst = TextStyle::footnote().color(palette::TEXT_FAINT).right();
                let hw = ui.c.measure(hint, &hst, 60.0).0;
                ui.c.text(hint, Rect::new(rr.right() - 8.0 - hw, rr.y, hw, rr.h), &hst);
                label_right = rr.right() - 14.0 - hw;
            }
            ui.c.text(item.label, Rect::new(rr.x + 28.0, rr.y, (label_right - (rr.x + 28.0)).max(0.0), rr.h), &TextStyle::body().color(palette::TEXT));
            if rresp.clicked {
                picked = Some(item.action.clone());
            }
        }
    }

    if let Some(action) = picked {
        run_quick_action(ui, st, action);
    } else if iresp.submitted {
        if let Some(item) = items.get(st.ui.quick_sel) {
            run_quick_action(ui, st, item.action.clone());
        }
    } else if iresp.cancelled || (scrim.clicked && !card_resp.hovered) {
        st.ui.quick_open = false;
        st.ui.quick_query.clear();
        st.ui.quick_sel = 0;
        ui.blur();
    }
}

/// Brief floating feedback pill near the bottom of the expanded island.
fn draw_toast(ui: &mut Ui, st: &mut AppState, body: Rect) {
    let Some((msg, until)) = st.ui.toast.clone() else { return };
    let left = until - st.now;
    if left <= 0.0 {
        st.ui.toast = None;
        return;
    }
    let a = if left < TOAST_FADE_S {
        ui.request_anim();
        (left / TOAST_FADE_S).clamp(0.0, 1.0) as f32
    } else {
        1.0
    };
    let style = TextStyle::footnote().color(palette::TEXT);
    let tw = ui.c.measure(&msg, &style, body.w - 48.0).0;
    let pw = (tw + 20.0).ceil();
    let ph = 24.0;
    let pill = Rect::new(body.center().x - pw * 0.5, body.bottom() - ph - 10.0, pw, ph);
    let prev_a = ui.c.alpha();
    ui.c.set_alpha(prev_a * a);
    ui.c.fill_rrect(pill, ph * 0.5, palette::ISLAND.with_alpha(0.94));
    ui.c.stroke_rrect(pill, ph * 0.5, Color::white(0.14), 1.0);
    ui.c.text(&msg, Rect::new(pill.x + (pw - tw) * 0.5, pill.y, tw + 2.0, pill.h), &style);
    ui.c.set_alpha(prev_a);
}

/// The page content's rect inside the expanded island `body`: below the
/// header band, `PAD` in from the sides and the bottom.
pub(crate) fn content_area(body: Rect) -> Rect {
    Rect::new(body.x + PAD, body.y + HEADER_H, body.w - 2.0 * PAD, (body.h - HEADER_H - PAD).max(0.0))
}

/// The active page in [`content_area`], clipped to the island below the
/// `header` row. During the page switch `tr` (one in progress, as
/// [`draw_expanded`] filters it) the old page fades out sliding away while
/// the new one fades in. Every theme's frame draws its pages with this, so
/// a switch moves the same in all of them.
pub(crate) fn draw_pages(ui: &mut Ui, st: &mut AppState, body: Rect, header: Rect, page: PageKind, tr: Option<PageTransition>) {
    caffeine::put_away(ui, st);
    // The shared controls' glides (switches, pickers): none with motion off
    // or in the self-test, whose pictures are taken at rest.
    entrance::set_still(entrance::still(st) || st.in_selftest);
    let area = content_area(body);
    // Content may reach a little above `area` (text boxes whose cap height,
    // not their line box, sits on the content top) but never into the tabs.
    let clip_top = header.bottom() + 2.0;
    ui.c.clip(Rect::new(body.x, clip_top, body.w, (body.bottom() - clip_top).max(0.0)));
    match tr {
        Some(tr) => {
            // Overlapping smoothstep fades: the summed opacity stays at or
            // above about 0.5, so dense pages don't dip into a muddy grey.
            let out_a = 1.0 - smoothstep(tr.t / OUT_END);
            let in_a = smoothstep((tr.t - IN_START) / IN_LEN);
            let base = ui.c.alpha();
            // Restore (never reset) the transform: the app may be scaling the
            // whole island content while it opens.
            let m0 = ui.c.save_transform();
            if out_a > 0.0 {
                // Outgoing page: ghost UI (no input, throwaway hit list).
                let mut ghost_input = Input::default();
                let mut ghost_hits = Vec::new();
                let now = ui.now;
                let mut ghost = Ui::new(&mut *ui.c, &mut ghost_input, &mut ghost_hits, now);
                ghost.c.set_alpha(base * out_a);
                ghost.c.translate(-tr.dir * SLIDE * (1.0 - out_a), 0.0);
                draw_page(&mut ghost, st, area, tr.from);
                ghost.c.restore_transform(m0);
            }
            // The new page always draws (even fully transparent) so its hit
            // regions and animation state stay continuous.
            ui.c.set_alpha(base * in_a);
            ui.c.translate(tr.dir * SLIDE * (1.0 - in_a), 0.0);
            draw_page(ui, st, area, page);
            ui.c.restore_transform(m0);
            ui.c.set_alpha(base);
            ui.request_anim();
        }
        None => draw_page(ui, st, area, page),
    }
    ui.c.unclip();
}

pub(crate) fn draw_page(ui: &mut Ui, st: &mut AppState, area: Rect, page: PageKind) {
    match page {
        PageKind::Media => media::draw(ui, st, area),
        PageKind::Focus => focus::draw(ui, st, area),
        PageKind::Ai => usage::draw(ui, st, area),
        PageKind::Shelf => shelf::draw(ui, st, area),
        PageKind::Tasks => tasks::draw(ui, st, area),
        PageKind::Today => today::draw(ui, st, area),
        PageKind::System => system::draw(ui, st, area),
        PageKind::Games => games::draw(ui, st, area),
        PageKind::Script => script::draw(ui, st, area),
        PageKind::Record => record::draw(ui, st, area),
        PageKind::Prayer => prayer::draw(ui, st, area),
        PageKind::Stocks => stocks::draw(ui, st, area),
        PageKind::Caffeine => caffeine::draw(ui, st, area),
        PageKind::Downloads => downloads::draw(ui, st, area),
        PageKind::Notifications => notifications::draw(ui, st, area),
        PageKind::Addons => addons::draw(ui, st, area),
        PageKind::Settings => settings::draw(ui, st, area),
        PageKind::Widgets => widgets::draw(ui, st, area),
    }
}

pub fn ease_out_cubic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn smoothstep(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Round to the device-pixel grid.
pub(crate) fn snap(v: f32, scale: f32) -> f32 {
    (v * scale).round() / scale
}

// ---------------------------------------------------------------- header

const TAB_H: f32 = 28.0;
/// Inactive tabs are bare icons in a 30 DIP slot.
const TAB_W: f32 = 30.0;
const TAB_GAP: f32 = 2.0;
const TAB_ICON: f32 = 16.0;
/// The active tab's label starts this far in and ends `TAB_LABEL_END` before its edge.
const TAB_LABEL_X: f32 = 28.0;
const TAB_LABEL_END: f32 = 12.0;
/// Clock to date, and date (or clock) to the Settings button, which sits
/// right of the pin.
const DATE_GAP: f32 = 12.0;
const PIN_GAP: f32 = 10.0;
const PIN: f32 = 28.0;
const POWER: f32 = 28.0;
const GEAR: f32 = 28.0;
const GEAR_GAP: f32 = 2.0;
/// His theme: Halo peeks up beside the pin, as he does beside the clock when
/// the island is closed.
const HALO_W: f32 = 24.0;
const HALO_GAP: f32 = 6.0;
/// Hover labels: dwell, fade-in, capsule metrics.
const TIP_DELAY: f64 = 0.4;
const TIP_FADE: f64 = 0.12;
pub(crate) const TIP_H: f32 = 22.0;
pub(crate) const TIP_PAD: f32 = 8.0;
const TIP_RADIUS: f32 = 8.0;
/// White 0.16 over the black island, made opaque so page text can't show through.
pub(crate) const TIP_FILL: Color = Color::rgba(0.16, 0.16, 0.16, 1.0);
/// `tab_hover` slots of the pin, Power, Settings and overflow buttons (tabs use their place in the row).
const PIN_SLOT: usize = PageKind::ALL.len();
const GEAR_SLOT: usize = PIN_SLOT + 1;
const MORE_SLOT: usize = GEAR_SLOT + 1;
const POWER_SLOT: usize = MORE_SLOT + 1;
/// Crowded tabs narrow to this, then the rest go behind "…", as wide.
const TAB_MIN_W: f32 = 24.0;
/// Tabs end at least this far before the pin.
const PIN_CLEAR: f32 = 8.0;

/// What the header row keeps when the tabs crowd it. They give way in the
/// owner's order (0.3.00 Q7.4): Halo's face, the date, the open tab's label,
/// full-width tabs, and last the tabs past the room go behind "…". The pin
/// and Settings always keep their place: nothing pushes them out.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fit {
    face: bool,
    date: bool,
    label: bool,
    tab_w: f32,
    /// Tabs in the row; the rest are behind "…".
    shown: usize,
}

/// `room`: from the row's left to [`PIN_CLEAR`] before the pin with no date.
/// `label_w`: the open tab's label; `date_w`: the date (0: none); `face`:
/// the theme shows Halo's face.
fn fit(room: f32, n: usize, label_w: f32, date_w: f32, face: bool) -> Fit {
    let row = |k: usize, w: f32, label: bool| {
        let label_extra = if label { TAB_LABEL_X + label_w + TAB_LABEL_END + 1.0 - w } else { 0.0 };
        k as f32 * w + k.saturating_sub(1) as f32 * TAB_GAP + label_extra
    };
    let date_cost = if date_w > 0.0 { DATE_GAP + date_w } else { 0.0 };
    // The face sits between the tabs and the pin, 12 DIP clear of the tabs.
    let face_cost = if face { HALO_GAP + HALO_W + 4.0 } else { 0.0 };
    let steps = [(true, true, true, TAB_W), (false, true, true, TAB_W), (false, false, true, TAB_W), (false, false, false, TAB_W), (false, false, false, TAB_MIN_W)];
    for (f, d, label, w) in steps {
        if row(n, w, label) + if d { date_cost } else { 0.0 } + if f { face_cost } else { 0.0 } <= room {
            return Fit { face: f && face, date: d && date_w > 0.0, label, tab_w: w, shown: n };
        }
    }
    let shown = (1..n).rev().find(|&k| row(k, TAB_MIN_W, false) + TAB_GAP + TAB_MIN_W <= room).unwrap_or(1);
    Fit { face: false, date: false, label: false, tab_w: TAB_MIN_W, shown }
}

/// The first `shown` tabs, with the open page among them (in the last
/// place when it falls past them), and the ones behind "…".
fn split_row(row: Vec<PageKind>, shown: usize, page: PageKind) -> (Vec<PageKind>, Vec<PageKind>) {
    if shown >= row.len() {
        return (row, Vec::new());
    }
    let mut keep: Vec<PageKind> = row[..shown].to_vec();
    if !keep.contains(&page) && row.contains(&page) {
        keep[shown - 1] = page;
    }
    let rest = row.into_iter().filter(|p| !keep.contains(p)).collect();
    (keep, rest)
}
/// The Settings button's widget id (the self-test clicks it).
pub const GEAR_ID: &str = "header.gear";
/// The Power menu button's widget id.
pub const POWER_ID: &str = "header.power";

/// An open in-island header popover waiting to be painted above the page.
pub(crate) enum HeaderPopover {
    Tabs(Rect, Vec<PageKind>),
    Power(Rect),
}

/// A hover label waiting to be painted above the page.
pub(crate) struct Tip {
    pub(crate) anchor: Rect,
    pub(crate) text: &'static str,
    pub(crate) alpha: f32,
}

/// Tab slots (x, width) of `tabs`, `tab_w` wide, with `active` showing its label when `label`.
fn tab_layout(ui: &mut Ui, x0: f32, tabs: &[PageKind], active: PageKind, label_st: &TextStyle, scale: f32, tab_w: f32, label: bool) -> Vec<(f32, f32)> {
    let mut x = x0;
    tabs.iter()
        .map(|&p| {
            let w = if p == active && label { snap(TAB_LABEL_X + ui.c.measure(p.title(), label_st, 200.0).0 + TAB_LABEL_END, scale) } else { tab_w };
            let slot = (snap(x, scale), w);
            x += w + TAB_GAP;
            slot
        })
        .collect()
}

/// All tabs that appear in Settings › Tabs for reordering: every hideable tab
/// (including hidden ones, so they can be toggled and ordered) plus any active
/// add-on tabs, ordered by `settings.tab_order` (defaulting to `PageKind::ALL`).
pub fn orderable_tabs(st: &AppState) -> Vec<PageKind> {
    let mut list: Vec<PageKind> = Vec::new();
    let active = tabs(st);
    for &p in PageKind::ALL.iter() {
        if p == PageKind::Addons || !p.tabbed() {
            continue;
        }
        if hideable_tabs(st).contains(&p) || active.contains(&p) {
            list.push(p);
        }
    }
    if !st.settings.tab_order.is_empty() {
        let custom = &st.settings.tab_order;
        list.sort_by_key(|&p| {
            if let Some(pos) = custom.iter().position(|k| k.eq_ignore_ascii_case(p.key())) {
                (0usize, pos)
            } else {
                (1usize, p.index())
            }
        });
    }
    list
}

/// Move `tab` to `target`'s slot in `settings.tab_order`, preserving hidden or disabled keys in place.
pub fn reorder_tab(st: &mut AppState, all: &[PageKind], tab: PageKind, target: PageKind) {
    if tab == target || tab == PageKind::Addons || target == PageKind::Addons {
        return;
    }
    let mut keys: Vec<String> = Vec::new();
    for k in &st.settings.tab_order {
        if !k.eq_ignore_ascii_case(PageKind::Addons.key()) && !keys.iter().any(|x| x.eq_ignore_ascii_case(k)) {
            keys.push(k.clone());
        }
    }
    for &p in all {
        if p != PageKind::Addons && !keys.iter().any(|x| x.eq_ignore_ascii_case(p.key())) {
            keys.push(p.key().to_string());
        }
    }
    let Some(from_i) = keys.iter().position(|k| k.eq_ignore_ascii_case(tab.key())) else { return };
    let Some(to_i) = keys.iter().position(|k| k.eq_ignore_ascii_case(target.key())) else { return };
    let item = keys.remove(from_i);
    keys.insert(to_i, item);
    st.settings.tab_order = keys;
}

/// Tabs on the left; pin, date and clock on the right. During a page switch
/// the tabs re-flow, the selection pill slides and the labels cross-fade.
/// Returns the hover label (if any) and the open overflow popover (if any)
/// to paint once the page is drawn.
pub(crate) fn draw_header(ui: &mut Ui, st: &mut AppState, r: Rect, page: PageKind, tr: Option<PageTransition>) -> (Option<Tip>, Option<HeaderPopover>) {
    let scale = ui.c.scale().max(0.5);
    let cy = r.center().y;
    let label_st = TextStyle::body().weight(Weight::Semibold);
    let from = tr.map_or(page, |t| t.from);
    let e = tr.map_or(1.0, |t| ease_out_cubic(t.t / TABS_END));

    // Wheel over the header switches tabs when no popover is open.
    if !st.ui.tab_more_open && !st.ui.power_menu_open {
        let w = ui.wheel_in(r);
        if w != 0.0 {
            st.cmd(Command::SetPage(step_tab(page, st, w < 0.0)));
        }
    }

    // The right side first: the clock ends on the content column's right
    // edge (where every page's trailing control ends), then the date, the
    // Settings button, the Power button and the pin. Today already shows the
    // date large, so the header leaves it out there. The tabs get what is left.
    let right = r.right() - 6.0;
    let time = fmt_clock(&st.local, st.settings.use_24h);
    let time_st = TextStyle::body().weight(Weight::Semibold).tabular().right();
    let tw = ui.c.measure(&time, &time_st, 200.0).0;
    let date = fmt_date_short(&st.local);
    let date_st = TextStyle::footnote().color(palette::TEXT_DIM).right();
    let dw = ui.c.measure(&date, &date_st, 200.0).0;
    let pin_x = |left: f32| snap(snap(snap(left - PIN_GAP - GEAR, scale) - GEAR_GAP - POWER, scale) - GEAR_GAP - PIN, scale);
    let label_w = ui.c.measure(page.title(), &label_st, 200.0).0.max(ui.c.measure(from.title(), &label_st, 200.0).0);
    let all = self::tabs(st);
    // The date's room is kept while it fades in or out too.
    let date_w = if page == PageKind::Today && from == PageKind::Today { 0.0 } else { dw };
    let fits = fit(pin_x(right - tw) - PIN_CLEAR - r.x, all.len(), label_w, date_w, crate::face::on(st.settings.notch_theme));
    let (row, more) = split_row(all.clone(), fits.shown, page);
    if more.is_empty() {
        st.ui.tab_more_open = false;
    }
    let lay_from = tab_layout(ui, r.x, &row, from, &label_st, scale, fits.tab_w, fits.label);
    let lay_to = if from == page { lay_from.clone() } else { tab_layout(ui, r.x, &row, page, &label_st, scale, fits.tab_w, fits.label) };
    let tabs: Vec<Rect> =
        (0..row.len()).map(|i| Rect::new(lerp(lay_from[i].0, lay_to[i].0, e), cy - TAB_H / 2.0, lerp(lay_from[i].1, lay_to[i].1, e), TAB_H)).collect();
    // A page that isn't in the row counts as the first tab.
    let place = |p: PageKind| row.iter().position(|t| *t == p).unwrap_or(0);
    let (fi, pi) = (place(from), place(page));
    let tabs_end = tabs[tabs.len() - 1].right();
    // How much the selection sits on the gear (Settings) rather than on a tab.
    let on_gear = |p: PageKind| if p.tabbed() { 0.0 } else { 1.0 };
    let gear_k = lerp(on_gear(from), on_gear(page), e);

    // Finish an active tab drag on mouse-up.
    let mut drag_just_ended = false;
    if !ui.input.down {
        if let Some((_, moved)) = st.ui.tab_drag.take() {
            if moved {
                drag_just_ended = true;
                st.cmd(Command::SaveSettings);
            }
        }
    }

    // Selection pill, gliding from the old tab to the new one. To or from
    // Settings it stays on its tab and fades while the gear takes it.
    let (a, b) = (if from.tabbed() { fi } else { pi }, if page.tabbed() { pi } else { fi });
    let pill = Rect::new(lerp(lay_from[a].0, lay_to[b].0, e), cy - TAB_H / 2.0, lerp(lay_from[a].1, lay_to[b].1, e), TAB_H);
    if gear_k < 1.0 {
        ui.c.with_alpha(1.0 - gear_k, |c| c.fill_rrect(pill, TAB_H / 2.0, Color::white(0.14)));
    }

    let mut hover: Option<(usize, Rect)> = None;
    let mut reorder_req: Option<(PageKind, PageKind)> = None;
    for (i, &p) in row.iter().enumerate() {
        let tr = tabs[i];
        let resp = ui.button(wid_n("tab", p as u64), tr);
        let active = p == page;
        let is_dragged = st.ui.tab_drag.is_some_and(|(dp, moved)| dp == p && moved);
        if is_dragged {
            ui.c.fill_rrect(tr, TAB_H / 2.0, Color::white(0.22));
            ui.c.stroke_rrect(tr, TAB_H / 2.0, Color::white(0.32), 1.0);
        } else if resp.hovered && !active {
            ui.c.fill_rrect(tr, TAB_H / 2.0, palette::SURFACE);
            hover = Some((i, tr));
        }
        // Dragging a tab horizontally (> 8 DIPs) reorders it within the row.
        if p != PageKind::Addons && !st.ui.tab_more_open && !st.ui.power_menu_open {
            if let Some((dx, _)) = resp.drag {
                let moved = st.ui.tab_drag.is_some_and(|(dp, m)| dp == p && m) || dx.abs() >= 8.0;
                st.ui.tab_drag = Some((p, moved));
                if moved {
                    if let Some(mx) = ui.input.mouse.map(|pt| pt.x) {
                        if let Some((ti, &target)) = row
                            .iter()
                            .enumerate()
                            .filter(|(_, &cand)| cand != PageKind::Addons)
                            .min_by(|(ia, _), (ib, _)| {
                                (tabs[*ia].center().x - mx).abs().total_cmp(&(tabs[*ib].center().x - mx).abs())
                            })
                        {
                            if ti != i && target != p {
                                reorder_req = Some((p, target));
                            }
                        }
                    }
                }
            }
        }
        let k = if active || is_dragged { e } else if from.tabbed() && i == fi { 1.0 - e } else { 0.0 };
        let ic = if resp.hovered || is_dragged { palette::TEXT } else { Color::lerp(palette::TEXT_FAINT, palette::TEXT, k) };
        let ix = snap(tr.x + (fits.tab_w.min(tr.w) - TAB_ICON) / 2.0, scale);
        ui.c.icon(p.icon(), Rect::new(ix, snap(cy - TAB_ICON / 2.0, scale), TAB_ICON, TAB_ICON), ic);
        if resp.clicked && !active && !drag_just_ended {
            st.ui.tab_more_open = false;
            st.ui.power_menu_open = false;
            st.ui.power_confirm = None;
            st.cmd(Command::SetPage(p));
        }
    }
    if let Some((tab, target)) = reorder_req {
        reorder_tab(st, &all, tab, target);
    }
    if fits.label && from.tabbed() && (fi != pi || !page.tabbed()) {
        draw_tab_label(ui, from, tabs[fi], &label_st, 1.0 - e);
    }
    if fits.label && page.tabbed() {
        draw_tab_label(ui, page, tabs[pi], &label_st, e);
    }
    // The tabs past the room: "…" toggles an in-island popover grid.
    let more_r = Rect::new(snap(tabs_end + TAB_GAP, scale), cy - TAB_H / 2.0, TAB_MIN_W, TAB_H);
    let mut popover = None;
    if !more.is_empty() {
        let resp = ui.button(wid("header.more"), more_r);
        if st.ui.tab_more_open {
            ui.c.fill_rrect(more_r, TAB_H / 2.0, Color::white(0.14));
        } else if resp.hovered {
            ui.c.fill_rrect(more_r, TAB_H / 2.0, palette::SURFACE);
            hover = Some((MORE_SLOT, more_r));
        }
        let dot = if st.ui.tab_more_open || resp.hovered { palette::TEXT } else { palette::TEXT_FAINT };
        for i in -1..=1 {
            ui.c.fill_circle(Point::new(more_r.center().x + i as f32 * 5.0, cy), 1.6, dot);
        }
        if resp.clicked {
            st.ui.tab_more_open = !st.ui.tab_more_open;
            st.ui.power_menu_open = false;
            st.ui.power_confirm = None;
            st.ui.tab_more_since = ui.now;
            ui.request_anim();
        }
        if st.ui.tab_more_open {
            popover = Some(HeaderPopover::Tabs(more_r, more.clone()));
        }
    }

    ui.c.text(&time, Rect::new(right - tw - 4.0, r.y, tw + 4.0, r.h), &time_st);
    let mut left = right - tw;
    let shows_date = |p: PageKind| if p == PageKind::Today { 0.0 } else { 1.0 };
    let date_k = if fits.date { lerp(shows_date(from), shows_date(page), e) } else { 0.0 };
    if date_k > 0.0 {
        let date_left = left - DATE_GAP - dw;
        let date_r = Rect::new(date_left - 2.0, r.y, dw + 2.0, r.h);
        ui.c.with_alpha(date_k, |c| c.text(&date, date_r, &date_st));
        left = lerp(left, date_left, date_k);
    }

    let gear_r = Rect::new(snap(left - PIN_GAP - GEAR, scale), cy - GEAR / 2.0, GEAR, GEAR);
    let power_r = Rect::new(snap(gear_r.x - GEAR_GAP - POWER, scale), cy - POWER / 2.0, POWER, POWER);
    let pin_r = Rect::new(pin_x(left), cy - PIN / 2.0, PIN, PIN);
    let halo_cx = pin_r.x - HALO_GAP - HALO_W / 2.0;
    if fits.face {
        // Eyes on the cursor while it is over the island; asleep at night.
        let look = ui.input.mouse.map(|p| ((p.x - r.center().x) / (r.w / 2.0)).clamp(-1.0, 1.0));
        let (frame, moving) = crate::face::rest(st.now, st.local.hour < 5, look);
        if moving {
            ui.request_anim();
        }
        crate::face::peek(ui.c, &frame, halo_cx, cy + 9.0, HALO_W, r.y);
    }
    // The pin, Power and Settings always show: `fit` left them their room.
    {
        let pinned = st.pinned;
        let (icon, color) = if pinned { (Icon::PinFill, st.accent) } else { (Icon::Pin, palette::TEXT_FAINT) };
        let resp = ui.icon_button(wid("pin"), pin_r, icon, color, 15.0);
        if resp.hovered {
            hover = Some((PIN_SLOT, pin_r));
        }
        if resp.clicked {
            st.ui.tab_more_open = false;
            st.ui.power_menu_open = false;
            st.ui.power_confirm = None;
            st.cmd(Command::SetPinned(!pinned));
        }
    }
    // Power menu: opens an in-island popover with Shut down, Restart, Sleep, Lock.
    {
        if st.ui.power_menu_open {
            ui.c.fill_rrect(power_r, POWER / 2.0, Color::white(0.14));
        }
        let color = if st.ui.power_menu_open { palette::TEXT } else { palette::TEXT_FAINT };
        let resp = ui.icon_button(wid(POWER_ID), power_r, Icon::Power, color, 15.0);
        if resp.hovered && !st.ui.power_menu_open {
            hover = Some((POWER_SLOT, power_r));
        }
        if resp.clicked {
            st.ui.power_menu_open = !st.ui.power_menu_open;
            st.ui.power_confirm = None;
            st.ui.tab_more_open = false;
            st.ui.power_menu_since = ui.now;
            ui.request_anim();
        }
        if st.ui.power_menu_open {
            popover = Some(HeaderPopover::Power(power_r));
        }
    }
    // Settings: the gear is lit like a selected tab while its page shows,
    // and a second press goes back to the page it was pressed on. The page
    // has the everyday settings; its last row opens Advanced settings, the
    // window with every setting.
    {
        if gear_k > 0.0 {
            ui.c.with_alpha(gear_k, |c| c.fill_rrect(gear_r, GEAR / 2.0, Color::white(0.14)));
        }
        let color = Color::lerp(palette::TEXT_FAINT, palette::TEXT, gear_k);
        let resp = ui.icon_button(wid(GEAR_ID), gear_r, Icon::Gear, color, 16.0);
        if resp.hovered {
            hover = Some((GEAR_SLOT, gear_r));
        }
        if resp.clicked {
            st.ui.tab_more_open = false;
            st.ui.power_menu_open = false;
            st.ui.power_confirm = None;
            if page == PageKind::Settings {
                let back = st.ui.before_settings.take().filter(|p| row.contains(p) || more.contains(p)).unwrap_or(row[0]);
                st.cmd(Command::SetPage(back));
            } else {
                st.ui.before_settings = Some(page);
                st.cmd(Command::SetPage(PageKind::Settings));
            }
        }
    }

    // Hover labels: after a short dwell over an inactive tab, the pin, Power or the gear.
    let now = ui.now;
    st.ui.tab_hover = match (hover, st.ui.tab_hover) {
        (Some((i, _)), Some((j, t0))) if i == j => Some((j, t0)),
        (Some((i, _)), _) => Some((i, now)),
        (None, _) => None,
    };
    let tip = (|| -> Option<Tip> {
        let (slot, anchor) = hover?;
        let since = now - st.ui.tab_hover.map_or(now, |h| h.1);
        if since < TIP_DELAY + TIP_FADE {
            ui.request_anim();
        }
        let alpha = ((since - TIP_DELAY) / TIP_FADE).clamp(0.0, 1.0) as f32;
        if alpha <= 0.0 {
            return None;
        }
        let text = match row.get(slot) {
            Some(p) => p.title(),
            None if slot == GEAR_SLOT && page == PageKind::Settings => "Close settings",
            None if slot == GEAR_SLOT => "Settings",
            None if slot == POWER_SLOT => "Power",
            None if slot == MORE_SLOT => "More tabs",
            None if st.pinned => "Unpin",
            None => "Keep open",
        };
        Some(Tip { anchor, text, alpha })
    })();
    (tip, popover)
}

pub(crate) fn draw_popover(ui: &mut Ui, st: &mut AppState, body: Rect, popover: HeaderPopover) {
    match popover {
        HeaderPopover::Tabs(anchor, items) => draw_tab_popover(ui, st, body, anchor, &items),
        HeaderPopover::Power(anchor) => draw_power_popover(ui, st, body, anchor),
    }
}

fn trigger_power_action(st: &mut AppState, now: f64, action: crate::commands::PowerAction) {
    st.ui.power_menu_open = false;
    st.ui.power_confirm = None;
    if now < st.ui.power_busy_until {
        return;
    }
    st.ui.power_busy_until = now + 3.0;
    st.cmd(Command::Power(action));
    st.cmd(Command::Collapse);
}

/// In-island power menu popover: Shut down, Restart, Sleep, Lock.
/// Destructive actions (Shut down, Restart) switch to an inline confirmation
/// step before firing; Sleep and Lock run immediately.
pub(crate) fn draw_power_popover(ui: &mut Ui, st: &mut AppState, body: Rect, anchor: Rect) {
    use crate::commands::PowerAction;

    let scale = ui.c.scale().max(0.5);
    const POP_PAD: f32 = 8.0;
    const TILE_W: f32 = 86.0;
    const TILE_H: f32 = 54.0;
    const TILE_GAP: f32 = 4.0;
    const POP_R: f32 = 16.0;
    const POP_DUR: f64 = 0.22;

    let confirming = st.ui.power_confirm;
    let (pop_w, pop_h) = if confirming.is_some() {
        (208.0, 112.0)
    } else {
        (2.0 * TILE_W + TILE_GAP + 2.0 * POP_PAD, 2.0 * TILE_H + TILE_GAP + 2.0 * POP_PAD)
    };
    let x = (anchor.center().x - pop_w / 2.0).clamp(body.x + 12.0, (body.right() - 12.0 - pop_w).max(body.x + 12.0));
    let y = (anchor.bottom() + 4.0).min((body.bottom() - 8.0 - pop_h).max(body.y + 42.0));
    let pop_r = Rect::new(snap(x, scale), snap(y, scale), snap(pop_w, scale), snap(pop_h, scale));

    // Clicking outside the popover (and outside the Power toggle button) dismisses it.
    if ui.input.just_pressed {
        if let Some(p) = ui.input.press_pos {
            if !pop_r.contains(p) && !anchor.contains(p) {
                st.ui.power_menu_open = false;
                st.ui.power_confirm = None;
                return;
            }
        }
    }

    let anim_t = if entrance::still(st) || st.in_selftest {
        1.0
    } else {
        let elapsed = (ui.now - st.ui.power_menu_since).max(0.0);
        if elapsed < POP_DUR {
            ui.request_anim();
        }
        (elapsed / POP_DUR).clamp(0.0, 1.0) as f32
    };
    let k = ease_out_cubic(anim_t);
    let sc = 0.94 + 0.06 * k;

    let m = ui.c.save_transform();
    let origin = Point::new(pop_r.center().x, pop_r.y);
    ui.c.scale_about(origin, sc);
    ui.c.with_alpha(k, |c| {
        c.fill_rrect(pop_r, POP_R, Color::rgba(0.14, 0.14, 0.16, 0.98));
        c.stroke_rrect(pop_r, POP_R, Color::white(0.12), 1.0);
    });

    if let Some(action) = confirming {
        let (icon, tint) = match action {
            PowerAction::Shutdown => (Icon::Power, palette::RED),
            PowerAction::Restart => (Icon::Reset, palette::ORANGE),
            PowerAction::Sleep => (Icon::Moon, palette::BLUE),
            PowerAction::Lock => (Icon::Lock, palette::PURPLE),
        };
        let pad = 12.0;
        let badge = Rect::new(snap(pop_r.x + pad, scale), snap(pop_r.y + pad, scale), 26.0, 26.0);
        ui.c.fill_rrect(badge, 8.0, tint);
        ui.c.icon(icon, Rect::from_center(badge.center(), 14.0, 14.0), palette::ON_ACCENT);

        let tx = badge.right() + 10.0;
        let tw = (pop_r.right() - pad - tx).max(40.0);
        ui.c.text(action.confirm_prompt(), Rect::new(tx, pop_r.y + pad - 1.0, tw, 18.0), &TextStyle::body().weight(Weight::Semibold));
        ui.c.text("Unsaved work may be lost.", Rect::new(tx, pop_r.y + pad + 17.0, tw, 15.0), &TextStyle::caption().color(palette::TEXT_DIM));

        let btn_h = 28.0;
        let btn_gap = 8.0;
        let btn_w = (pop_r.w - 2.0 * pad - btn_gap) / 2.0;
        let btn_y = snap(pop_r.bottom() - pad - btn_h, scale);
        let cancel_r = Rect::new(snap(pop_r.x + pad, scale), btn_y, snap(btn_w, scale), btn_h);
        let confirm_r = Rect::new(snap(cancel_r.right() + btn_gap, scale), btn_y, snap(btn_w, scale), btn_h);

        let c_resp = ui.button(wid("power.confirm.cancel"), cancel_r);
        let c_fill = if c_resp.pressed { palette::SURFACE_HOVER } else if c_resp.hovered { Color::white(0.14) } else { palette::SURFACE };
        ui.c.fill_rrect(cancel_r, 8.0, c_fill);
        ui.c.text("Cancel", cancel_r, &TextStyle::caption().weight(Weight::Semibold).center());
        if c_resp.clicked {
            st.ui.power_confirm = None;
        }

        let ok_resp = ui.button(wid("power.confirm.ok"), confirm_r);
        let ok_fill = if ok_resp.pressed { tint.with_alpha(0.80) } else if ok_resp.hovered { tint.with_alpha(0.92) } else { tint };
        ui.c.fill_rrect(confirm_r, 8.0, ok_fill);
        ui.c.text(action.label(), confirm_r, &TextStyle::caption().weight(Weight::Semibold).color(palette::ON_ACCENT).center());
        if ok_resp.clicked {
            trigger_power_action(st, ui.now, action);
        }
    } else {
        const ITEMS: [(PowerAction, Icon, Color); 4] = [
            (PowerAction::Shutdown, Icon::Power, palette::RED),
            (PowerAction::Restart, Icon::Reset, palette::ORANGE),
            (PowerAction::Sleep, Icon::Moon, palette::BLUE),
            (PowerAction::Lock, Icon::Lock, palette::PURPLE),
        ];
        for (idx, &(action, icon, tint)) in ITEMS.iter().enumerate() {
            let col = idx % 2;
            let row = idx / 2;
            let tx = pop_r.x + POP_PAD + col as f32 * (TILE_W + TILE_GAP);
            let ty = pop_r.y + POP_PAD + row as f32 * (TILE_H + TILE_GAP);
            let tr = Rect::new(snap(tx, scale), snap(ty, scale), TILE_W, TILE_H);
            let resp = ui.button(wid_n("power.item", idx as u64), tr);
            if resp.hovered {
                let fill = if resp.pressed { palette::SURFACE_HOVER } else { palette::SURFACE };
                ui.c.fill_rrect(tr, 10.0, fill);
            }
            let badge = Rect::from_center(Point::new(tr.center().x, tr.y + 18.0), 26.0, 26.0);
            ui.c.fill_rrect(badge, 8.0, tint);
            ui.c.icon(icon, Rect::from_center(badge.center(), 14.0, 14.0), palette::ON_ACCENT);
            let lbl_col = if resp.hovered { palette::TEXT } else { Color::white(0.84) };
            let lbl_r = Rect::new(tr.x + 2.0, tr.y + 35.0, tr.w - 4.0, 15.0);
            ui.c.text(action.label(), lbl_r, &TextStyle::caption().color(lbl_col).center());
            if resp.clicked {
                if action.needs_confirm() {
                    st.ui.power_confirm = Some(action);
                } else {
                    trigger_power_action(st, ui.now, action);
                }
            }
        }
    }
    ui.c.restore_transform(m);
}

/// In-island overflow popover for crowded tabs behind "…": a dark rounded card
/// with tinted icon tiles and English labels, replacing the classic Win32 popup menu.
pub(crate) fn draw_tab_popover(ui: &mut Ui, st: &mut AppState, body: Rect, anchor: Rect, items: &[PageKind]) {
    if items.is_empty() {
        return;
    }
    let scale = ui.c.scale().max(0.5);
    const POP_PAD: f32 = 8.0;
    const TILE_W: f32 = 80.0;
    const TILE_H: f32 = 52.0;
    const TILE_GAP: f32 = 4.0;
    const POP_R: f32 = 16.0;
    const POP_DUR: f64 = 0.22;

    let cols = items.len().clamp(1, 3);
    let rows = items.len().div_ceil(cols);
    let pop_w = cols as f32 * TILE_W + (cols - 1) as f32 * TILE_GAP + 2.0 * POP_PAD;
    let pop_h = rows as f32 * TILE_H + (rows - 1) as f32 * TILE_GAP + 2.0 * POP_PAD;
    let x = (anchor.center().x - pop_w / 2.0).clamp(body.x + 12.0, (body.right() - 12.0 - pop_w).max(body.x + 12.0));
    let y = (anchor.bottom() + 4.0).min((body.bottom() - 8.0 - pop_h).max(body.y + 42.0));
    let pop_r = Rect::new(snap(x, scale), snap(y, scale), snap(pop_w, scale), snap(pop_h, scale));

    // Clicking outside the popover (and outside the "…" toggle button) dismisses it.
    if ui.input.just_pressed {
        if let Some(p) = ui.input.press_pos {
            if !pop_r.contains(p) && !anchor.contains(p) {
                st.ui.tab_more_open = false;
                return;
            }
        }
    }

    let anim_t = if entrance::still(st) || st.in_selftest {
        1.0
    } else {
        let elapsed = (ui.now - st.ui.tab_more_since).max(0.0);
        if elapsed < POP_DUR {
            ui.request_anim();
        }
        (elapsed / POP_DUR).clamp(0.0, 1.0) as f32
    };
    let k = ease_out_cubic(anim_t);
    let sc = 0.94 + 0.06 * k;

    let m = ui.c.save_transform();
    let origin = Point::new(pop_r.center().x, pop_r.y);
    ui.c.scale_about(origin, sc);
    ui.c.with_alpha(k, |c| {
        c.fill_rrect(pop_r, POP_R, Color::rgba(0.14, 0.14, 0.16, 0.98));
        c.stroke_rrect(pop_r, POP_R, Color::white(0.12), 1.0);
    });

    for (idx, &p) in items.iter().enumerate() {
        let col = idx % cols;
        let row = idx / cols;
        let tx = pop_r.x + POP_PAD + col as f32 * (TILE_W + TILE_GAP);
        let ty = pop_r.y + POP_PAD + row as f32 * (TILE_H + TILE_GAP);
        let tr = Rect::new(snap(tx, scale), snap(ty, scale), TILE_W, TILE_H);
        let resp = ui.button(wid_n("tab.more.item", p as u64), tr);
        if resp.hovered {
            let fill = if resp.pressed { palette::SURFACE_HOVER } else { palette::SURFACE };
            ui.c.fill_rrect(tr, 10.0, fill);
        }
        let badge = Rect::from_center(Point::new(tr.center().x, tr.y + 18.0), 26.0, 26.0);
        ui.c.fill_rrect(badge, 8.0, p.tint());
        ui.c.icon(p.icon(), Rect::from_center(badge.center(), 14.0, 14.0), palette::ON_ACCENT);
        let lbl_col = if resp.hovered { palette::TEXT } else { Color::white(0.84) };
        let lbl_r = Rect::new(tr.x + 2.0, tr.y + 34.0, tr.w - 4.0, 15.0);
        ui.c.text(p.title(), lbl_r, &TextStyle::caption().color(lbl_col).center());
        if resp.clicked {
            st.ui.tab_more_open = false;
            st.cmd(Command::SetPage(p));
        }
    }
    ui.c.restore_transform(m);
}

/// A tab's label, faded by `alpha` and cut (not ellipsized) at the tab's
/// current right edge while the tab is narrower than its settled width.
fn draw_tab_label(ui: &mut Ui, p: PageKind, tab: Rect, st: &TextStyle, alpha: f32) {
    if alpha <= 0.01 {
        return;
    }
    let lx = tab.x + TAB_LABEL_X;
    let avail = tab.right() - TAB_LABEL_END + 2.0 - lx;
    if avail <= 0.5 {
        return;
    }
    let lw = ui.c.measure(p.title(), st, 200.0).0;
    let cut = avail < lw + 1.0;
    if cut {
        ui.c.clip(Rect::new(lx, tab.y, avail, tab.h));
    }
    ui.c.with_alpha(alpha, |c| c.text(p.title(), Rect::new(lx, tab.y, lw + 2.0, tab.h), st));
    if cut {
        ui.c.unclip();
    }
}

/// A small capsule centred under the hovered tab (kept within the header's width).
fn draw_tip(ui: &mut Ui, header: Rect, tip: &Tip) {
    let scale = ui.c.scale().max(0.5);
    let st = TextStyle::footnote().center();
    let tw = ui.c.measure(tip.text, &st, 300.0).0;
    let w = snap((tw + 2.0 * TIP_PAD).ceil(), scale);
    let x = (tip.anchor.center().x - w / 2.0).clamp(header.x, (header.right() - w).max(header.x));
    let r = Rect::new(snap(x, scale), snap(header.bottom() + 4.0, scale), w, TIP_H);
    ui.c.with_alpha(tip.alpha, |c| {
        c.fill_rrect(r, TIP_RADIUS, TIP_FILL);
        c.text(tip.text, r, &st);
    });
}

/// Shared helper: a centred, dim placeholder message with an icon.
pub fn empty_state(ui: &mut Ui, area: Rect, icon: Icon, title: &str, hint: &str, e: entrance::Entrance) {
    let c = area.center();
    let ic = Point::new(c.x, c.y - 22.0);
    entrance::scaled(ui, ic, e.pop(0.05), |ui| ui.c.icon(icon, Rect::from_center(ic, 26.0, 26.0), Color::white(0.35)));
    ui.c.text(title, Rect::new(area.x, c.y - 4.0, area.w, 20.0), &TextStyle::headline().center());
    ui.c.text(hint, Rect::new(area.x, c.y + 16.0, area.w, 18.0), &TextStyle::footnote().color(palette::TEXT_DIM).center());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::HaloLink;
    use halo_core::settings::Settings;
    use halo_core::store::Store;

    fn state() -> AppState {
        AppState::new(Settings::default(), Store::default())
    }

    #[test]
    fn the_row_is_the_island_s_own_pages_then_the_add_ons_then_the_plus() {
        // Music, then Today beside it; Focus where Today was; AI after it,
        // with Studio Pro.
        use PageKind::*;
        let mut st = state();
        // Widgets is an add-on, off until switched on; then it sits beside Music.
        assert_eq!(tabs(&st), [Media, Today, Shelf, Tasks, Focus, Addons]);
        st.addons.insert("widgets", halo_core::addons::Status::Ready);
        assert_eq!(tabs(&st), [Media, Widgets, Today, Shelf, Tasks, Focus, Addons]);
        st.halo_account.link = HaloLink::Connected { account: "a•••@example.com".into() };
        assert_eq!(tabs(&st), [Media, Widgets, Today, Shelf, Tasks, Focus, Addons]);
        st.halo_account.pro = Some(true);
        assert_eq!(tabs(&st), [Media, Widgets, Today, Shelf, Tasks, Focus, Ai, Addons]);
        // The add-ons' pages come next, before the "+".
        for id in ["system", "script"] {
            st.addons.insert(id, halo_core::addons::Status::Ready);
        }
        // Games comes with System, on Windows only.
        if cfg!(windows) {
            assert_eq!(&tabs(&st)[7..], [System, Games, Script, Addons]);
        } else {
            assert_eq!(&tabs(&st)[7..], [System, Script, Addons]);
        }
        // The Halo, IELTS, Notes and Health pages left the app: an island last open there opens on Music.
        for key in ["halo", "ielts", "notes", "health", "water"] {
            assert_eq!(PageKind::from_key(key), Media);
        }
        // Clipboard left the app: an island last open there opens on Music.
        assert_eq!(PageKind::from_key("clipboard"), Media);
    }

    #[test]
    fn hidden_tabs_leave_the_row_but_not_while_their_page_is_open() {
        use PageKind::*;
        let mut st = state();
        st.addons.insert("widgets", halo_core::addons::Status::Ready);
        st.settings.hidden_tabs = vec!["today".into(), "tasks".into(), "halo".into(), "addons".into()];
        st.page = Focus;
        // The "+" can't be hidden.
        assert_eq!(tabs(&st), [Media, Widgets, Shelf, Focus, Addons]);
        assert_eq!(first_tab(&st), Media);
        // Opened from elsewhere (a meeting's notice opens Today): its tab
        // shows while it's open, and stepping on leaves it behind.
        st.page = Today;
        assert_eq!(tabs(&st), [Media, Widgets, Today, Shelf, Focus, Addons]);
        st.page = step_tab(Today, &st, true);
        assert_eq!(st.page, Shelf);
        assert!(!tabs(&st).contains(&Today));
        // Every one of them hidden: the island opens on Widgets.
        st.settings.hidden_tabs = PageKind::HIDEABLE.iter().map(|p| p.key().to_string()).collect();
        st.page = Widgets;
        assert_eq!(tabs(&st), [Widgets, Addons]);
        assert_eq!(first_tab(&st), Widgets);
        // A start on a hidden page opens on the first tab instead: Widgets,
        // which sits beside Music and can't be hidden.
        let mut settings = halo_core::settings::Settings::default();
        settings.start_page = "today".into();
        settings.remember_page = false;
        settings.hidden_tabs = vec!["media".into(), "today".into()];
        settings.set_addon(halo_core::addons::WIDGETS.id, true);
        assert_eq!(AppState::new(settings.clone(), Store::default()).page, Widgets);
        // Widgets off as well: the next of the island's own pages.
        settings.set_addon(halo_core::addons::WIDGETS.id, false);
        assert_eq!(AppState::new(settings, Store::default()).page, Shelf);
    }

    #[test]
    fn settings_has_the_gear_not_a_tab() {
        let mut st = state();
        assert!(!tabs(&st).contains(&PageKind::Settings));
        st.halo_account.link = HaloLink::Connected { account: "a•••@example.com".into() };
        assert!(!tabs(&st).contains(&PageKind::Settings));
        // Stepping through the tabs never lands on Settings.
        let mut p = PageKind::Media;
        for _ in 0..2 * PageKind::ALL.len() {
            p = step_tab(p, &st, true);
            assert!(p.tabbed(), "stepped onto {p:?}");
        }
    }

    #[test]
    fn settings_keeps_its_key_and_slides_in_from_the_right() {
        assert_eq!(PageKind::from_key("settings"), PageKind::Settings);
        assert!(PageKind::ALL.iter().all(|p| p.tab_index() <= PageKind::Settings.tab_index()));
    }

    #[test]
    fn tab_order_reorders_tabs_while_keeping_addons_last() {
        use PageKind::*;
        let mut st = state();
        st.settings.tab_order = vec!["focus".into(), "tasks".into(), "media".into()];
        assert_eq!(tabs(&st), [Focus, Tasks, Media, Today, Shelf, Addons]);
        let all = tabs(&st);
        reorder_tab(&mut st, &all, Shelf, Focus);
        assert_eq!(tabs(&st), [Shelf, Focus, Tasks, Media, Today, Addons]);
    }

    /// The header row as `draw_header` lays it out at an island `width`
    /// wide (Settings › Width when open, 480 to 900), the clock `clock_w`
    /// wide: tabs, "…" and the pin's left edge.
    fn header_at(width: f32, n: usize, label_w: f32, date_w: f32, face: bool, clock_w: f32) -> (Fit, f32, f32) {
        let r = header_rect(Rect::new(0.0, 0.0, width, 300.0));
        let right = r.right() - 6.0 - clock_w;
        let pin_x = right - PIN_GAP - GEAR - GEAR_GAP - POWER - GEAR_GAP - PIN;
        let f = fit(pin_x - PIN_CLEAR - r.x, n, label_w, date_w, face);
        let label = if f.label { TAB_LABEL_X + label_w + TAB_LABEL_END + 1.0 - f.tab_w } else { 0.0 };
        let tabs_end = r.x + f.shown as f32 * (f.tab_w + TAB_GAP) - TAB_GAP + label;
        let end = if f.shown < n { tabs_end + TAB_GAP + TAB_MIN_W } else { tabs_end };
        let pin = if f.date { pin_x - DATE_GAP - date_w } else { pin_x };
        (f, end, pin)
    }

    #[test]
    fn the_pin_and_settings_always_show_however_many_tabs() {
        // Every width the island opens at, every tab count up to all 17
        // (every add-on on), the longest label ("Notifications"), a 24-hour
        // and a 12-hour clock, with and without the date and Halo's face.
        let all = PageKind::ALL.len() - 1;
        for width in (480..=900).step_by(10).map(|w| w as f32) {
            for n in 1..=all {
                for (label_w, date_w, face, clock_w) in [(92.0, 70.0, true, 66.0), (40.0, 0.0, false, 40.0), (92.0, 70.0, false, 40.0)] {
                    let (f, end, pin) = header_at(width, n, label_w, date_w, face, clock_w);
                    // Nothing runs into the pin, so the pin and Settings never hide.
                    assert!(end + PIN_CLEAR <= pin + 0.01, "{width} wide, {n} tabs: tabs end at {end}, pin at {pin} ({f:?})");
                    // Every tab is a click away: in the row, or behind "…".
                    assert!(f.shown >= 1 && f.shown <= n, "{f:?}");
                    assert!(f.tab_w >= TAB_MIN_W, "{f:?}");
                }
            }
        }
    }

    #[test]
    fn crowded_tabs_give_way_in_the_owners_order() {
        // Room for everything: nothing gives.
        let f = fit(1000.0, 7, 60.0, 70.0, true);
        assert_eq!(f, Fit { face: true, date: true, label: true, tab_w: TAB_W, shown: 7 });
        // Less and less room: the face goes first, then the date, the label,
        // full-width tabs, and last tabs go behind "…".
        let order: Vec<Fit> = (100..=600).rev().step_by(5).map(|room| fit(room as f32, 12, 60.0, 70.0, true)).collect();
        let rank = |f: &Fit| [f.face, f.date, f.label, f.tab_w == TAB_W, f.shown == 12].iter().filter(|b| **b).count();
        assert!(order.windows(2).all(|w| rank(&w[1]) <= rank(&w[0])), "{order:?}");
        assert!(order.iter().any(|f| !f.face && f.date), "the face goes before the date");
        assert!(order.iter().any(|f| !f.date && f.label), "the date goes before the label");
        assert!(order.iter().any(|f| !f.label && f.tab_w == TAB_W), "the label goes before the tabs narrow");
        assert!(order.iter().any(|f| f.tab_w == TAB_MIN_W && f.shown == 12), "the tabs narrow before any hides");
        assert!(order.last().is_some_and(|f| f.shown < 12), "{:?}", order.last());
        // The open page stays in the row, last, when it falls past the room.
        use PageKind::*;
        let row = vec![Media, Widgets, Today, Shelf, Tasks, Addons];
        assert_eq!(split_row(row.clone(), 3, Tasks), (vec![Media, Widgets, Tasks], vec![Today, Shelf, Addons]));
        assert_eq!(split_row(row.clone(), 3, Widgets), (vec![Media, Widgets, Today], vec![Shelf, Tasks, Addons]));
        assert_eq!(split_row(row.clone(), 6, Media), (row, vec![]));
    }

    #[test]
    fn addons_is_the_last_tab_and_never_makes_the_window_taller() {
        let mut st = state();
        assert_eq!(tabs(&st).last(), Some(&PageKind::Addons));
        st.halo_account.link = HaloLink::Connected { account: "a•••@example.com".into() };
        assert_eq!(tabs(&st).last(), Some(&PageKind::Addons));
        assert_eq!(PageKind::from_key("addons"), PageKind::Addons);
        // The window is as tall as the tallest page (System, laid out for
        // whether it shows or not); the add-ons' rows and Stocks stay under it.
        assert!(content_height(PageKind::Addons, &st) <= content_height(PageKind::System, &st));
        assert!(content_height(PageKind::Stocks, &st) <= content_height(PageKind::System, &st));
        assert!(content_height(PageKind::Caffeine, &st) <= content_height(PageKind::System, &st));
        // Notifications with as many apps as it lists.
        for i in 0..halo_core::notifications::MAX_APPS {
            st.settings.notifications.remember(&format!("App.{i}"), &format!("App {i}"));
        }
        assert!(content_height(PageKind::Notifications, &st) <= content_height(PageKind::System, &st));
    }

    #[test]
    fn ai_has_a_tab_only_with_studio_pro_and_never_makes_the_window_taller() {
        let mut st = state();
        assert!(!tabs(&st).contains(&PageKind::Ai), "signed out");
        st.halo_account.link = HaloLink::Connected { account: "a\u{2022}\u{2022}\u{2022}@example.com".into() };
        assert!(!tabs(&st).contains(&PageKind::Ai), "before the server's answer");
        st.halo_account.pro = Some(false);
        assert!(!tabs(&st).contains(&PageKind::Ai), "an account without Pro");
        st.halo_account.pro = Some(true);
        assert!(tabs(&st).contains(&PageKind::Ai), "on with Pro, nothing to switch on");
        st.settings.set_addon(halo_core::addons::AI_USAGE.id, false);
        assert!(!tabs(&st).contains(&PageKind::Ai), "switched off under Add-ons");
        assert_eq!(PageKind::from_key("ai"), PageKind::Ai);
        // Pro isn't known yet at start: an island last open on the AI tab opens on the first tab.
        let store = Store { last_page: "ai".into(), ..Store::default() };
        assert_eq!(AppState::new(Settings { remember_page: true, ..Settings::default() }, store).page, PageKind::ALL[0]);
        assert!(content_height(PageKind::Ai, &st) <= content_height(PageKind::System, &st));
    }

    #[test]
    fn system_stocks_caffeine_and_widgets_have_a_tab_only_while_their_add_ons_are_on() {
        for (page, addon) in [
            (PageKind::System, halo_core::addons::SYSTEM),
            (PageKind::Stocks, halo_core::addons::STOCKS),
            (PageKind::Caffeine, halo_core::addons::CAFFEINE),
            (PageKind::Widgets, halo_core::addons::WIDGETS),
        ] {
            let mut st = state();
            assert!(!tabs(&st).contains(&page), "{} is off for everyone until switched on", addon.id);
            st.settings.set_addon(addon.id, true);
            st.addons.insert(addon.id, halo_core::addons::Status::Ready);
            assert!(tabs(&st).contains(&page));
            // An island last open on its page opens on the first tab while it's off.
            let store = Store { last_page: page.key().into(), ..Store::default() };
            let mut settings = Settings { remember_page: true, ..Settings::default() };
            assert_eq!(AppState::new(settings.clone(), store.clone()).page, PageKind::ALL[0]);
            settings.set_addon(addon.id, true);
            assert_eq!(AppState::new(settings, store).page, page);
        }
    }

    #[test]
    fn notifications_is_on_wherever_the_copy_can_read_them_until_switched_off() {
        // The download can't read other apps' notifications: no tab, nothing listens.
        let mut st = state();
        assert!(!st.notifications_on() && !tabs(&st).contains(&PageKind::Notifications));
        assert_eq!(hideable_tabs(&st), &PageKind::HIDEABLE[..5], "Settings doesn't offer it");
        // The Store copy: on for everyone, without an add-on to switch on.
        st.notify_can = true;
        assert!(st.notifications_on() && tabs(&st).contains(&PageKind::Notifications));
        assert_eq!(hideable_tabs(&st).last(), Some(&PageKind::Notifications));
        // Its chip in Settings' tabs row switches it off, tab and all, even
        // while its page is open.
        st.settings.hidden_tabs = vec!["notifications".into()];
        st.page = PageKind::Notifications;
        assert!(!st.notifications_on() && !tabs(&st).contains(&PageKind::Notifications));
        assert_eq!(first_tab(&st), PageKind::Media);
    }

    #[test]
    fn downloads_has_a_tab_only_with_studio_pro() {
        for (page, addon) in [(PageKind::Downloads, halo_core::addons::DOWNLOADER)] {
            let mut st = state();
            st.downloader_offered = true;
            st.settings.set_addon(addon.id, true);
            st.addons.insert(addon.id, halo_core::addons::Status::Ready);
            assert!(!tabs(&st).contains(&page), "{}: signed out", addon.id);
            st.halo_account.link = HaloLink::Connected { account: "a•••@example.com".into() };
            assert!(!tabs(&st).contains(&page), "{}: before the server's answer", addon.id);
            st.halo_account.pro = Some(false);
            assert!(!tabs(&st).contains(&page), "{}: an account without Pro", addon.id);
            st.halo_account.pro = Some(true);
            assert!(tabs(&st).contains(&page), "{}", addon.id);
            // Signed out again: gone, whatever the server said before.
            st.halo_account.link = HaloLink::Off;
            assert!(!tabs(&st).contains(&page), "{}", addon.id);
            // Pro isn't known yet at start: an island last open on its page opens on the first tab.
            let store = Store { last_page: page.key().into(), ..Store::default() };
            let mut settings = Settings { remember_page: true, ..Settings::default() };
            settings.set_addon(addon.id, true);
            assert_eq!(AppState::new(settings, store).page, PageKind::ALL[0], "{}", addon.id);
        }
    }

    #[test]
    fn prayer_has_a_tab_only_while_its_add_on_is_on_and_ready() {
        let mut st = state();
        assert!(!tabs(&st).contains(&PageKind::Prayer));
        st.addons.insert("sholat", halo_core::addons::Status::Ready);
        assert!(!tabs(&st).contains(&PageKind::Prayer), "no places read yet");
        let list = halo_core::prayer::PlaceList::parse(r#"{"places":[]}"#).unwrap();
        st.prayer.places = Some(std::sync::Arc::new(list));
        let row = tabs(&st);
        assert_eq!(&row[row.len() - 2..], &[PageKind::Prayer, PageKind::Addons]);
        assert!(content_height(PageKind::Prayer, &st) <= content_height(PageKind::Tasks, &st));
        st.addons.insert("sholat", halo_core::addons::Status::Downloading { done: 1, total: 2 });
        assert!(!tabs(&st).contains(&PageKind::Prayer));
    }

    #[test]
    fn script_has_a_tab_only_while_its_add_on_is_on() {
        let mut st = state();
        assert!(!tabs(&st).contains(&PageKind::Script), "off for everyone until switched on");
        st.addons.insert("script", halo_core::addons::Status::Ready);
        assert!(tabs(&st).contains(&PageKind::Script));
        assert_eq!(tabs(&st).last(), Some(&PageKind::Addons));
    }

    #[test]
    fn record_has_a_tab_once_the_recorder_is_installed() {
        let mut st = state();
        assert!(!tabs(&st).contains(&PageKind::Record), "no recorder: no tab");
        st.recorder = true;
        assert!(!tabs(&st).contains(&PageKind::Record), "a recorder that never wrote (1.0.0) can't read Record's links");
        st.store.recorder_island = true;
        let row = tabs(&st);
        if cfg!(target_os = "macos") {
            assert!(!row.contains(&PageKind::Record), "Windows only for now");
            return;
        }
        assert!(row.contains(&PageKind::Record), "installing the recorder is the switch");
        assert_eq!(row.last(), Some(&PageKind::Addons));
        st.settings.set_addon("recorder", false);
        assert!(!tabs(&st).contains(&PageKind::Record), "switched off: gone");
        st.settings.set_addon("recorder", true);
        st.addons.insert("script", halo_core::addons::Status::Ready);
        let row = tabs(&st);
        let at = |p| row.iter().position(|t| *t == p).unwrap();
        assert_eq!(at(PageKind::Record), at(PageKind::Script) + 1, "beside the Script page");
    }

    /// Widgets switched on, off and removed: its tab follows, the other tabs
    /// stay, and what it kept goes; its choices wait in settings.
    #[test]
    fn widgets_on_off_and_removed_leaves_the_island_s_own_tabs_alone() {
        use halo_core::addons::{Status, WIDGETS};
        let mut st = state();
        let core = tabs(&st);
        assert!(!core.contains(&PageKind::Widgets) && !st.widgets_on());
        // On: its tab, beside Music, and every other tab as it was.
        st.settings.set_addon(WIDGETS.id, true);
        st.addons.insert(WIDGETS.id, Status::Ready);
        assert_eq!(tabs(&st)[..2], [PageKind::Media, PageKind::Widgets]);
        assert_eq!(tabs(&st).into_iter().filter(|p| *p != PageKind::Widgets).collect::<Vec<_>>(), core);
        st.clips.push("copied", 5);
        st.store.weather_last = Some((halo_core::halo::Weather { temp_c: 30.0, code: 1 }, 1));
        st.weather.last = st.store.weather_last;
        st.settings.water_every_min = 60;
        // Off (the switch) and removed (its state cleared): the row is the core's again.
        st.settings.set_addon(WIDGETS.id, false);
        st.addons.insert(WIDGETS.id, Status::Off);
        st.widgets_off();
        assert_eq!(tabs(&st), core);
        assert!(st.clips.items.is_empty() && st.weather.last.is_none() && st.store.weather_last.is_none());
        assert!(st.cmds.contains(&crate::commands::Command::SaveStore), "the last reading leaves data.json");
        assert_eq!(st.settings.water_every_min, 60, "its choices stay for next time");
        // Its page open as it went: Settings' start pages no longer offer it.
        let store = Store { last_page: "widgets".into(), ..Store::default() };
        let settings = Settings { remember_page: true, ..Settings::default() };
        assert_eq!(AppState::new(settings, store).page, PageKind::ALL[0]);
    }

    #[test]
    fn neither_the_record_page_nor_six_add_ons_make_the_window_taller() {
        let mut st = state();
        st.recorder = true;
        st.store.recorder_island = true;
        let tallest = content_height(PageKind::System, &st);
        assert!(content_height(PageKind::Record, &st) <= tallest);
        assert!(content_height(PageKind::Addons, &st) <= tallest, "{} > {tallest}", content_height(PageKind::Addons, &st));
        assert!(content_height(PageKind::Widgets, &st) <= tallest);
    }

    #[test]
    fn an_island_last_on_the_script_page_opens_on_the_first_tab_once_it_is_off() {
        let store = Store { last_page: "script".into(), ..Store::default() };
        let mut settings = Settings { remember_page: true, ..Settings::default() };
        assert_eq!(AppState::new(settings.clone(), store.clone()).page, PageKind::ALL[0]);
        settings.set_addon("script", true);
        assert_eq!(AppState::new(settings, store).page, PageKind::Script);
    }

    #[test]
    fn settings_never_makes_the_window_taller() {
        // The window fits the tallest page; System is always 256 DIPs (Settings
        // has three rows of switches since Clipboard history left).
        let mut st = state();
        assert!(content_height(PageKind::Settings, &st) <= content_height(PageKind::System, &st));
        // Nor its new version's row, nor the Update sheet in its place.
        let rel = halo_core::update::Release {
            version: "9.9.9".into(),
            date: String::new(),
            headline: "Something new".into(),
            notes: String::new(),
            windows: None,
            mac: None,
        };
        st.update.offer = Some(rel);
        let row = content_height(PageKind::Settings, &st);
        st.update.sheet = true;
        let sheet = content_height(PageKind::Settings, &st);
        assert!(row <= content_height(PageKind::System, &st) && sheet <= row, "{row} {sheet}");
    }

    #[test]
    fn power_menu_confirm_rules_and_double_fire_guard() {
        use crate::commands::PowerAction;
        assert!(PowerAction::Shutdown.needs_confirm());
        assert!(PowerAction::Restart.needs_confirm());
        assert!(!PowerAction::Sleep.needs_confirm());
        assert!(!PowerAction::Lock.needs_confirm());

        let mut st = state();
        st.ui.power_menu_open = true;
        st.ui.power_confirm = Some(PowerAction::Shutdown);
        trigger_power_action(&mut st, 10.0, PowerAction::Shutdown);
        assert!(!st.ui.power_menu_open);
        assert!(st.ui.power_confirm.is_none());
        assert_eq!(st.cmds.iter().filter(|c| matches!(c, Command::Power(_))).count(), 1);
        // A second trigger immediately after must not fire again.
        trigger_power_action(&mut st, 10.1, PowerAction::Shutdown);
        assert_eq!(st.cmds.iter().filter(|c| matches!(c, Command::Power(_))).count(), 1);
    }

    #[test]
    fn quick_switcher_filters_tabs_and_actions() {
        let st = state();
        let all = quick_matches(&st, "");
        assert!(all.iter().any(|i| i.action == QuickAction::Page(PageKind::Tasks)));
        assert!(all.iter().any(|i| i.action == QuickAction::Page(PageKind::Settings)));
        assert!(all.iter().any(|i| i.action == QuickAction::NewTask));
        assert_eq!(all[0].hint, Some("Ctrl+1"));

        let filtered = quick_matches(&st, "task");
        assert!(filtered.iter().any(|i| i.action == QuickAction::NewTask));
        assert!(filtered.iter().all(|i| i.label.to_ascii_lowercase().contains("task")));
    }

    #[test]
    fn toast_schedules_fade_wake_without_polling() {
        let mut st = state();
        st.now = 100.0;
        st.toast("Copied");
        let w = next_wake(PageKind::Media, &st).expect("wake");
        assert!(w <= 102.2 - TOAST_FADE_S + 0.01);
    }
}

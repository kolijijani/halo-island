//! The Settings window's content: every setting Halo Island has, in a
//! sidebar of sections (General, Appearance, Island, Music and lyrics,
//! Pop-ups, Tabs, Add-ons, Focus, Prayer, Caffeine, AI, About), called
//! Advanced settings. The last row of the island's Settings page (behind
//! the gear) and the menu's Advanced settings… open it, in a window of its
//! own: on Windows a borderless one (`settings_host.rs`), on the Mac a real
//! window with the system's title bar (`mac/settings_host.rs`). Both draw
//! this [`View`].
//!
//! It draws with the island's widgets ([`Ui`], the Settings page's switch
//! and picker) on the same [`AppState`], and changes settings the way the
//! island's pages do: `Command::SaveSettings` after a change, which the app
//! applies as a menu change is. Each section is a list of [`Block`]s, most
//! of them groups of rows ([`Item`]: a label, a dim line under it, and a
//! switch, a picker, a stepper or a button), laid out from the top; a click
//! comes back as an [`Act`] on the row's [`Key`] ([`apply`]).
//!
//! It follows the system's light or dark mode: the window's own surfaces in
//! [`Surfaces`], and everything on them through the Glass look of that mode,
//! which turns the island's white ink dark on the light window.

use crate::commands::Command;
use crate::gfx::{Canvas, Image, ImageData, TextStyle, Weight};
use crate::icons::Icon;
use crate::pages::settings::picker_width;
use crate::pages::PageKind;
use crate::pill::status;
use crate::state::AppState;
use crate::ui::{wid, wid_n, CursorKind, Hit, Input, MouseButton, PillStyle, Ui, PILL_GAP, PILL_SMALL};
use crate::version::VERSION;
use halo_core::caffeine;
use halo_core::geom::{palette, Color, Point, Rect};
use halo_core::settings::{DockIconSize, DockThemeMode, IdleStyle, MonitorChoice, NotchTheme, NotchType, PrayerFollow, PrayerSound};
use std::cell::RefCell;
use std::collections::HashMap;

/// The card, in DIPs. Windows' window gives it room for its shadow and is
/// less tall on a short screen (the content scrolls).
pub const CARD_W: f32 = 840.0;
pub const CARD_H: f32 = 580.0;
/// Its corners when the window draws its own (Windows).
const RADIUS: f32 = 14.0;
const SIDEBAR_W: f32 = 220.0;
/// The content column: its left and right padding, and where it starts.
const PAD: f32 = 28.0;
const TITLE_Y: f32 = 22.0;
const TITLE_H: f32 = 30.0;
const CONTENT_Y: f32 = 70.0;
/// Sidebar search box and section rows.
const SEARCH_Y: f32 = 54.0;
const SEARCH_H: f32 = 26.0;
const NAV_Y: f32 = 90.0;
const NAV_H: f32 = 30.0;
const NAV_GAP: f32 = 2.0;
const NAV_TILE: f32 = 21.0;
/// Rows: one line, or a label over a dim line.
const ROW_H: f32 = 40.0;
const ROW_H_SUB: f32 = 52.0;
const ROW_PAD: f32 = 14.0;
const GROUP_TITLE_H: f32 = 24.0;
const GROUP_GAP: f32 = 18.0;
const GROUP_R: f32 = 10.0;
const SWITCH_W: f32 = 30.0;
const SWITCH_H: f32 = 18.0;
const PICKER_H: f32 = 26.0;
/// The picker's and the switch's insides, as the island's Settings page has them.
const SEG_INSET: f32 = 2.0;
const SEG_PAD: f32 = 11.0;
const KNOB_INSET: f32 = 2.0;
const STEP_BTN: f32 = 26.0;
const STEP_VALUE_W: f32 = 84.0;
/// The Appearance section's type & theme cards: two to a row, preview on the
/// left and name + description on the right, fitting without scrolling.
const THEME_COLS: usize = 2;
const THEME_CARD_H: f32 = 84.0;
const THEME_PREVIEW_W: f32 = 136.0;
const THEME_PREVIEW_H: f32 = 64.0;
const THEME_GAP: f32 = 12.0;
/// A card's line of words wraps at this measure, not at the card's width:
/// here the longer lines break into two fuller ones, where the whole width
/// leaves one word alone on the second.
const THEME_LINE_W: f32 = 108.0;
/// On the cards' little screens, Pill's capsule floats this far below the
/// top edge.
const THEME_FLOAT: f32 = 7.0;
/// Diorama's picture floats nearer the edge: it is drawn at its own size,
/// pixel for pixel, and is taller than the little capsules beside it.
const DIORAMA_FLOAT: f32 = 3.0;
/// The card's picture is the place on a shorter pane than the resting
/// pill's (`studio_diorama::LONG`), so it fits the little screen at its own
/// size: the house, the clock, the fence and the tree.
const DIORAMA_CARD_W: f32 = 120.0;
/// The Diorama card shows the theme's own picture as it rests on the real
/// screen: this moment of it, a morning at the 10:08 the other cards' clocks
/// say, the glass ball on the house.
const DIORAMA_MOMENT: studio_diorama::Moment = studio_diorama::Moment {
    minute: 608.0,
    sunrise: 360.0,
    sunset: 1080.0,
    day: 9817,
    lens: 0.0,
    clock: Some(studio_diorama::Clock { hour: 10, minute: 8, h24: true }),
    badge: None,
    dark: None,
};
/// A wheel notch scrolls this far.
pub const WHEEL_STEP: f32 = 48.0;

/// The window's own surfaces, painted as they are; text and widgets on
/// them go through the look of the same mode.
#[derive(Clone, Copy, Debug)]
pub struct Surfaces {
    pub ground: Color,
    sidebar: Color,
    group: Color,
    group_hover: Color,
    group_press: Color,
}

const DARK: Surfaces = Surfaces {
    ground: Color::hex(0x1C1C1E),
    sidebar: Color::hex(0x242426),
    group: Color::hex(0x2C2C2E),
    group_hover: Color::hex(0x333336),
    group_press: Color::hex(0x3A3A3D),
};
const LIGHT: Surfaces = Surfaces {
    ground: Color::hex(0xF2F2F7),
    sidebar: Color::hex(0xE6E6EB),
    group: Color::hex(0xFFFFFF),
    group_hover: Color::hex(0xF7F7F9),
    group_press: Color::hex(0xEDEDF1),
};

pub fn surfaces(light: bool) -> Surfaces {
    if light {
        LIGHT
    } else {
        DARK
    }
}

/// Lines and edges, in the look's ink.
const RULE: Color = Color::white(0.08);
const EDGE: Color = Color::white(0.10);
/// Behind the theme cards' little screens.
const WALLPAPER: Color = Color::hex(0x3B4A7A);

/// The window's sections, in the sidebar's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Section {
    General,
    Appearance,
    Island,
    Dock,
    Music,
    Popups,
    Tabs,
    Addons,
    Focus,
    Prayer,
    Caffeine,
    Ai,
    /// Halo Pro: the key, and the Pro parts once it's on.
    HaloPro,
    About,
}

impl Section {
    pub const ALL: [Section; 14] = [
        Section::General,
        Section::Appearance,
        Section::Island,
        Section::Dock,
        Section::Music,
        Section::Popups,
        Section::Tabs,
        Section::Addons,
        Section::Focus,
        Section::Prayer,
        Section::Caffeine,
        Section::Ai,
        Section::HaloPro,
        Section::About,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Appearance => "Appearance",
            Section::Island => "Island",
            Section::Dock => "Dock",
            Section::Music => "Music and lyrics",
            Section::Popups => "Pop-ups",
            Section::Tabs => "Tabs",
            Section::Addons => "Add-ons",
            Section::Focus => "Focus",
            Section::Prayer => "Prayer times",
            Section::Caffeine => "Caffeine",
            Section::Ai => "AI usage",
            Section::HaloPro => "Halo Pro",
            Section::About => "About",
        }
    }

    /// For the self-test's file names.
    pub fn key(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Appearance => "appearance",
            Section::Island => "island",
            Section::Dock => "dock",
            Section::Music => "music",
            Section::Popups => "popups",
            Section::Tabs => "tabs",
            Section::Addons => "addons",
            Section::Focus => "focus",
            Section::Prayer => "prayer",
            Section::Caffeine => "caffeine",
            Section::Ai => "ai",
            Section::HaloPro => "halo-pro",
            Section::About => "about",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Section::General => Icon::Gear,
            Section::Appearance => Icon::Eye,
            Section::Island => Icon::AppWindow,
            Section::Dock => Icon::Tray,
            Section::Music => Icon::Music,
            Section::Popups => Icon::Volume,
            Section::Tabs => Icon::App,
            Section::Addons => Icon::Addons,
            Section::Focus => Icon::Timer,
            Section::Prayer => Icon::Moon,
            Section::Caffeine => Icon::Coffee,
            Section::Ai => Icon::Ai,
            Section::HaloPro => Icon::Lock,
            Section::About => Icon::Gear,
        }
    }

    /// The colour of its tile in the sidebar.
    fn tint(self) -> Color {
        match self {
            Section::General => Color::hex(0x8E8E93),
            Section::Appearance => Color::hex(0x5E5CE6),
            Section::Island => palette::BLUE,
            Section::Dock => Color::hex(0x38BDF8),
            Section::Music => palette::PINK,
            Section::Popups => palette::CYAN,
            Section::Tabs => palette::GREEN,
            Section::Addons => Color::hex(0x0A84FF),
            Section::Focus => palette::ORANGE,
            Section::Prayer => Color::hex(0x30B0A0),
            Section::Caffeine => Color::hex(0xA2845E),
            Section::Ai => palette::PURPLE,
            Section::HaloPro => palette::YELLOW,
            Section::About => Color::hex(0x8E8E93),
        }
    }

    /// Prayer times and Caffeine have a section only while their add-ons are
    /// on, AI usage only with Studio Pro, and Dock on Windows.
    pub fn shown(self, st: &AppState) -> bool {
        match self {
            Section::Dock => cfg!(windows),
            Section::Prayer => st.prayer_on(),
            Section::Caffeine => st.caffeine_on(),
            Section::Ai => st.ai_usage_on(),
            _ => true,
        }
    }
}

/// The sections the sidebar lists now.
pub fn sections(st: &AppState) -> Vec<Section> {
    Section::ALL.into_iter().filter(|s| s.shown(st)).collect()
}

// ---------------------------------------------------------------- rows

/// What a row changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// Halo: drink water, its goal, the clipboard, widget slots, the theme
    /// builder and cloud sync.
    WaterEvery,
    WaterGoal,
    Clips,
    Widget(u8),
    AddWidget,
    CustomLook,
    LookColours,
    LookRadius,
    LookGlass,
    CloudSync,
    StepAside,
    Autostart,
    Sounds,
    Clock24,
    SystemHistory,
    IdleLook,
    Mascot,
    HoverDelay,
    CloseDelay,
    Width,
    Display,
    Fullscreen,
    GameStats,
    GamePeek,
    Lyrics,
    Pronunciation,
    MediaActivity,
    LyricsOffset,
    HudVolume,
    HudPower,
    HudBluetooth,
    HudCaps,
    HudWifi,
    HudBrightness,
    HudMic,
    RememberPage,
    StartPage,
    ResetTabOrder,
    /// The Today page's news source (`halo_core::news::PRESETS`).
    NewsFeed,
    Tab(PageKind),
    FocusMin,
    ShortBreak,
    LongBreak,
    LongEvery,
    AutoBreaks,
    AutoFocus,
    PrayerSound,
    PrayerCountdown,
    PrayerPause,
    PrayerFollow,
    PrayerCalendar,
    CaffeineBedtime,
    CaffeineHalfLife,
    CaffeineBedLimit,
    CaffeineDayLimit,
    CaffeineRemind,
    AiNotices,
    AiNear,
    DataFolder,
    ReleaseNotes,
    LyricsCredit,
    DockEnabled,
    DockTheme,
    DockIconSize,
    DockMagnification,
    DockHideTaskbar,
    DockBottomMost,
    DockShowApps,
    DockShowDownloads,
    DockShowTray,
}

/// A row's control, at its right end.
#[derive(Clone, Debug, PartialEq)]
pub enum Ctl {
    /// `None` = not known yet (Start with Windows until it's read): dimmed.
    Switch(Option<bool>),
    /// A picker; `None` = none of its choices (a value typed into the file).
    Choice(Vec<&'static str>, Option<usize>),
    /// − and + around the value.
    Stepper(String),
    /// A button with this label.
    Button(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub key: Key,
    pub label: String,
    pub sub: Option<String>,
    pub ctl: Ctl,
}

fn item(key: Key, label: &str, sub: Option<&str>, ctl: Ctl) -> Item {
    Item { key, label: label.to_string(), sub: sub.map(str::to_string), ctl }
}

fn switch(key: Key, label: &str, sub: Option<&str>, on: bool) -> Item {
    item(key, label, sub, Ctl::Switch(Some(on)))
}

/// A section's content, top to bottom.
#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// Rows on one card, with a caption above it.
    Group(Option<&'static str>, Vec<Item>),
    /// This copy's version, Check now, and a new version when there is one.
    Version,
    /// The notch shapes (Notch / Pill) with live preview of the current theme.
    NotchTypes,
    /// The themes as cards with a live preview of the current notch type.
    Themes,
    /// The Add-ons page itself.
    Addons,
    /// A dim line of text.
    Note(&'static str),
    /// Halo Pro's key: the field, Activate, and what it said.
    ProKey,
}

/// What a click on a row's control did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    Toggle,
    Pick(usize),
    Step(i32),
    Press,
    Preview,
}

/// Sample pop-up for the Preview button on each Pop-ups row.
fn preview_hud(key: Key) -> Option<crate::state::HudKind> {
    use crate::state::HudKind;
    Some(match key {
        Key::HudVolume => HudKind::Volume { level: 0.68, muted: false },
        Key::HudPower => HudKind::Charging { percent: 76, plugged: true },
        Key::HudBluetooth => HudKind::Bluetooth { name: "AirPods Pro".into(), connected: true, battery: Some(82), audio: true },
        Key::HudCaps => HudKind::CapsLock { on: true },
        Key::HudWifi => HudKind::Wifi { connected: true, ssid: Some("Home Wi-Fi".into()), internet: true },
        Key::HudBrightness => HudKind::Brightness { level: 0.72 },
        Key::HudMic => HudKind::Mic { muted: true },
        _ => return None,
    })
}

const WATER_EVERY: [(u32, &str); 4] = [(0, "Off"), (45, "45 min"), (60, "Hourly"), (90, "90 min")];

/// Halo's own Free rows, first in General: water, the clipboard, widgets.
fn halo_free(s: &halo_core::settings::Settings) -> Vec<Block> {
    use halo_core::halo::{clip_cap, widget_slots, Widget};
    let clip_sub = format!("Keeps the last {} things you copy, in memory only", if s.pro { "500".to_string() } else { clip_cap(false).to_string() });
    let mut widgets: Vec<Item> = s
        .widgets
        .iter()
        .enumerate()
        .map(|(i, w)| {
            let names: Vec<&'static str> = Widget::ALL.iter().filter(|w| s.pro || !w.pro()).map(|w| w.title()).chain(["Remove"]).collect();
            let at = names.iter().position(|n| *n == w.title());
            item(Key::Widget(i as u8), &format!("Slot {}", i + 1), None, Ctl::Choice(names, at))
        })
        .collect();
    if s.widgets.len() < widget_slots(s.pro) {
        widgets.push(item(Key::AddWidget, "Add a widget", Some(if s.pro { "Up to 5, one of each widget" } else { "2 free; all 5 with Halo Pro" }), Ctl::Button("Add")));
    }
    vec![
        Block::Group(
            Some("Halo"),
            vec![
                item(Key::WaterEvery, "Drink water", Some("A gentle reminder on the island"), Ctl::Choice(names(&WATER_EVERY), nearest(&WATER_EVERY, s.water_every_min))),
                item(Key::WaterGoal, "Glasses a day", None, Ctl::Stepper(s.water_goal.to_string())),
                switch(Key::Clips, "Clipboard", Some(clip_sub.as_str()), s.clips),
            ],
        ),
        Block::Group(Some("Widgets tab"), widgets),
    ]
}

/// How long the pointer rests on the island before it opens (ms).
const HOVER: [(u32, &str); 3] = [(120, "Quick"), (220, "Normal"), (450, "Slow")];
/// How long it stays open after the pointer leaves (ms).
const CLOSE: [(u32, &str); 3] = [(0, "At once"), (400, "Short"), (1000, "Long")];
const DISPLAYS: [(MonitorChoice, &str); 4] = [
    (MonitorChoice::Primary, "Primary"),
    (MonitorChoice::Secondary, "Secondary"),
    (MonitorChoice::All, "All"),
    (MonitorChoice::Cursor, "Under the pointer"),
];
const FULLSCREEN: [(bool, &str); 2] = [(true, "Hide"), (false, "Stay on top")];
/// Prayer times' Sound choices (`halo_core::addons::SOUNDS`, the Prayer page's too).
const SOUNDS: [(PrayerSound, &str); 4] = {
    let s = &halo_core::addons::SOUNDS;
    [(s[0].0, s[0].1), (s[1].0, s[1].1), (s[2].0, s[2].1), (s[3].0, s[3].1)]
};
/// Whose times and Hijri dates (the Prayer page's place picker has the same).
const FOLLOWS: [(PrayerFollow, &str); 1] = [(PrayerFollow::Jakim, "Malaysia")];
const NOTCH_TYPES: [(NotchType, &str, &str); 2] = [
    (NotchType::Notch, "Notch", "Hangs from the top edge"),
    (NotchType::Pill, "Pill", "Floats below the top edge"),
];
const THEMES: [(NotchTheme, &str, &str); 4] = [
    (NotchTheme::Classic, "Classic", "Clean black or system look"),
    (NotchTheme::Halo, "Halo", "Halo lives in the island"),
    (NotchTheme::Glass, "Glass", "Frosted glass over your screen"),
    (NotchTheme::Diorama, "Diorama", "A little place that keeps your day"),
];
/// Start with Windows, or at login on a Mac (its menu bar says so).
const AUTOSTART: (&str, &str) = if cfg!(target_os = "macos") {
    ("Start at login", "Opens Halo Island when you log in")
} else {
    ("Start with Windows", "Opens Halo Island when you sign in")
};
const WIDTH_STEP: f32 = 20.0;
const OFFSET_STEP: i32 = 100;
/// Caffeine's Left at bedtime and Day limit rows: the least, the most and
/// the step between (mg).
const BED_LIMIT_MG: (i32, i32, i32) = (10, 80, 5);
const DAY_LIMIT_MG: (i32, i32, i32) = (100, 1000, 50);
/// Its Bedtime row goes by quarter hours.
const BEDTIME_STEP: i32 = 15;

fn chosen<T: PartialEq>(list: &[(T, &str)], v: T) -> Option<usize> {
    list.iter().position(|(x, _)| *x == v)
}

fn names<T>(list: &[(T, &'static str)]) -> Vec<&'static str> {
    list.iter().map(|(_, n)| *n).collect()
}

/// The preset nearest to `ms` (a hand-edited file's value lands on one).
fn nearest(list: &[(u32, &str)], ms: u32) -> Option<usize> {
    list.iter().enumerate().min_by_key(|(_, (v, _))| v.abs_diff(ms)).map(|(i, _)| i)
}

fn idle_name(s: IdleStyle, theme: NotchTheme) -> &'static str {
    match s {
        IdleStyle::Clock => "Clock",
        IdleStyle::Eyes if theme == NotchTheme::Halo => "Halo",
        IdleStyle::Eyes => "Eyes",
        IdleStyle::Minimal => "Minimal",
        IdleStyle::Hidden => "Hidden",
        IdleStyle::Status => "Status",
    }
}

/// The pages the island may open on: its tabs, but not the Add-ons "+".
fn start_pages(st: &AppState) -> Vec<PageKind> {
    crate::pages::tabs(st).into_iter().filter(|p| *p != PageKind::Addons).collect()
}

fn minutes(m: u32) -> String {
    format!("{m} min")
}

/// A half-life as Caffeine's row shows it: "5 h", "2.5 h".
fn half_life_text(h: f32) -> String {
    if h.fract() == 0.0 {
        format!("{h:.0} h")
    } else {
        format!("{h:.1} h")
    }
}

/// The lyrics' shift as people say it.
fn offset_text(ms: i32) -> String {
    match ms {
        0 => "On time".into(),
        _ => format!("{}{:.1} s", if ms > 0 { "−" } else { "+" }, ms.unsigned_abs() as f32 / 1000.0),
    }
}

/// The Today page's news source as the row names it: a preset, or "Your
/// own feed" for an address typed into settings.json.
fn news_name(setting: &str) -> String {
    halo_core::news::preset_index(setting).map_or_else(|| "Your own feed".to_string(), |i| halo_core::news::PRESETS[i].0.to_string())
}

/// What `section` shows for `st`.
pub fn blocks(section: Section, st: &AppState) -> Vec<Block> {
    let s = &st.settings;
    match section {
        Section::General => {
            let mut rows = vec![
                item(Key::Autostart, AUTOSTART.0, Some(AUTOSTART.1), Ctl::Switch(st.ui.autostart)),
                switch(Key::Sounds, "Sounds", Some("A soft sound for timers, drops and reminders"), s.sounds),
                switch(Key::Clock24, "24-hour clock", None, s.use_24h),
                item(Key::NewsFeed, "News on Today", Some("Top stories, read only while Today is open"), Ctl::Stepper(news_name(&s.news_feed))),
            ];
            if s.system_on() && s.pro {
                rows.push(switch(Key::SystemHistory, "Keep System page history", Some("Halo Pro: the charts go on while the page is closed"), s.system_history));
            }
            let mut blocks = vec![Block::Version, Block::Group(None, rows)];
            // Water, the clipboard and the slots: the Widgets add-on's.
            if st.widgets_on() {
                blocks.extend(halo_free(s));
            }
            blocks
        }
        Section::Appearance => {
            let looks = status::looks(s.notch_type, s.notch_theme);
            let look_names = looks.iter().map(|l| idle_name(*l, s.notch_theme)).collect();
            let shown = status::shown(s.notch_type, s.notch_theme, s.idle_style);
            let mut rest = vec![item(Key::IdleLook, "Idle look", Some("What the closed island shows"), Ctl::Choice(look_names, looks.iter().position(|l| *l == shown)))];
            // Halo's eyes are Windows' so far (`activities::mascot_shows`).
            if cfg!(windows) {
                rest.push(switch(Key::Mascot, "Halo Mascot", Some("Halo's eyes rest in place of the clock"), s.mascot));
            }
            vec![Block::NotchTypes, Block::Themes, Block::Group(None, rest)]
        }
        Section::Island => {
            // The Mac's island goes where the notch is; the game readout is Windows'.
            let mut shows = Vec::new();
            if cfg!(windows) {
                shows.push(item(Key::Display, "Display", Some("All: it follows the active window"), Ctl::Choice(names(&DISPLAYS), chosen(&DISPLAYS, s.monitor))));
            }
            shows.push(item(Key::Fullscreen, "In full-screen apps", Some("Games and videos that fill the screen"), Ctl::Choice(names(&FULLSCREEN), chosen(&FULLSCREEN, s.hide_in_fullscreen))));
            // The tray's old "In full-screen apps" submenu, as it showed them:
            // without Pro the readout says so, and `pro_gate` keeps it off.
            if cfg!(windows) {
                let stats = if s.pro { "Show FPS, GPU and CPU over games" } else { "Show FPS, GPU and CPU over games (Halo Pro)" };
                shows.push(switch(Key::GameStats, stats, Some("Keeps the island on top of full-screen apps"), s.game_stats));
                shows.push(switch(Key::GamePeek, "Only for a few seconds", Some("Then the island hides and adds no delay to the game"), s.game_peek));
            }
            vec![
                Block::Group(
                    Some("Opening and closing"),
                    vec![
                        item(Key::HoverDelay, "Opens after", Some("How long the pointer rests on the island first"), Ctl::Choice(names(&HOVER), nearest(&HOVER, s.hover_delay_ms))),
                        item(Key::CloseDelay, "Closes", Some("Once the pointer leaves the open island"), Ctl::Choice(names(&CLOSE), nearest(&CLOSE, s.collapse_delay_ms))),
                        item(Key::Width, "Width when open", Some("Wider gives the pages more room"), Ctl::Stepper(format!("{:.0}", s.expanded_width))),
                    ],
                ),
                Block::Group(Some("Where it shows"), shows),
                Block::Group(
                    Some("Over maximized windows"),
                    vec![switch(Key::StepAside, "Mini handle", Some("Halo shrinks to a small handle at the top edge"), s.step_aside)],
                ),
            ]
        }
        Section::Dock => {
            const DOCK_THEMES: [(DockThemeMode, &str); 3] = [
                (DockThemeMode::FollowIsland, "Follow island"),
                (DockThemeMode::LiquidGlass, "Liquid Glass"),
                (DockThemeMode::Classic, "Classic"),
            ];
            const DOCK_SIZES: [(DockIconSize, &str); 3] = [
                (DockIconSize::Compact, "Compact"),
                (DockIconSize::Default, "Default"),
                (DockIconSize::Large, "Large"),
            ];
            let dock_sub = if s.pro {
                "Floating macOS-style app shelf at the bottom of the screen"
            } else {
                "Floating macOS-style app shelf at the bottom of the screen (Halo Pro)"
            };
            vec![
                Block::Group(
                    Some("Bottom dock"),
                    vec![
                        switch(Key::DockEnabled, "Enable Bottom Dock", Some(dock_sub), s.dock_enabled && s.pro),
                        item(
                            Key::DockTheme,
                            "Appearance",
                            Some("Match the island's look or force frosted Liquid Glass"),
                            Ctl::Choice(names(&DOCK_THEMES), chosen(&DOCK_THEMES, s.dock_theme)),
                        ),
                        item(
                            Key::DockIconSize,
                            "Icon size",
                            Some("Size of app icons and shelf height"),
                            Ctl::Choice(names(&DOCK_SIZES), chosen(&DOCK_SIZES, s.dock_icon_size)),
                        ),
                        switch(
                            Key::DockMagnification,
                            "Hover magnification",
                            Some("Icons grow smoothly under the pointer"),
                            s.dock_magnification,
                        ),
                    ],
                ),
                Block::Group(
                    Some("Taskbar & placement"),
                    vec![
                        switch(
                            Key::DockHideTaskbar,
                            "Auto-hide Windows taskbar",
                            Some("Hide the system taskbar while the dock is active; restores when off or exiting"),
                            s.dock_hide_taskbar,
                        ),
                        switch(
                            Key::DockBottomMost,
                            "Anchor to bottom edge",
                            Some("Keep the dock flush at the very bottom so open apps stay unobstructed"),
                            s.dock_bottom_most,
                        ),
                    ],
                ),
                Block::Group(
                    Some("Dock items"),
                    vec![
                        switch(
                            Key::DockShowApps,
                            "Running apps",
                            Some("Show open and minimized windows with Genie minimize & restore"),
                            s.dock_show_apps,
                        ),
                        switch(
                            Key::DockShowDownloads,
                            "Downloads stack",
                            Some("Fan-out recent downloads popover next to the tray"),
                            s.dock_show_downloads,
                        ),
                        switch(
                            Key::DockShowTray,
                            "System tray & Control Center",
                            Some("Weather, Wi-Fi, volume slider, battery, Focus Mode and clock"),
                            s.dock_show_tray,
                        ),
                    ],
                ),
            ]
        }
        Section::Music => vec![
            Block::Group(
                Some("Lyrics"),
                vec![
                    switch(Key::Lyrics, "Synced lyrics", Some("Lyrics move with the song, from LRCLIB"), s.lyrics),
                    switch(Key::Pronunciation, "Lyrics pronunciation", Some("Romaji, pinyin and Korean under the lines"), s.lyrics_pronunciation),
                    item(Key::LyricsOffset, "Timing", Some("Shift every line earlier or later"), Ctl::Stepper(offset_text(s.lyrics_offset_ms))),
                ],
            ),
            Block::Group(Some("Closed island"), vec![switch(Key::MediaActivity, "Show what's playing", Some("The song and its cover on the closed island"), s.show_media_activity)]),
        ],
        Section::Popups => vec![Block::Group(
            Some("The island shows a pop-up for"),
            vec![
                switch(Key::HudVolume, "Volume", Some("When you change the volume or mute"), s.hud_volume),
                switch(Key::HudBrightness, "Brightness", Some("When display brightness changes"), s.hud_brightness),
                switch(Key::HudPower, "Charger and battery", Some("When you plug in, unplug or run low"), s.hud_power),
                switch(Key::HudBluetooth, "Bluetooth", Some("When headphones and other devices connect"), s.hud_bluetooth),
                switch(Key::HudCaps, "Caps Lock", Some("When it turns on or off"), s.hud_caps),
                switch(Key::HudWifi, "Wi-Fi", Some("When you join or leave a network"), s.hud_wifi),
                switch(Key::HudMic, "Microphone", Some("When your microphone is muted or unmuted"), s.hud_mic),
            ],
        )],
        Section::Tabs => {
            let pages = start_pages(st);
            let start = PageKind::from_key(&s.start_page);
            let start_name = if pages.contains(&start) { start.title() } else { pages.first().map_or("Music", |p| p.title()) };
            let mut open = vec![switch(Key::RememberPage, "Open where you left off", Some("The page you last had open"), s.remember_page)];
            if !s.remember_page {
                open.push(item(Key::StartPage, "Opens on", None, Ctl::Stepper(start_name.to_string())));
            }
            // All reorderable tabs in the user's order: hideable tabs get switches;
            // enabled add-on tabs can also be dragged into any position.
            let tabs: Vec<Item> = crate::pages::orderable_tabs(st)
                .into_iter()
                .map(|p| {
                    if p.hideable() {
                        let sub = (p == PageKind::Notifications).then_some("Messages from WhatsApp, Telegram and your phone");
                        item(Key::Tab(p), p.title(), sub, Ctl::Switch(Some(!st.tab_hidden(p))))
                    } else {
                        item(Key::Tab(p), p.title(), Some("Add-on tab"), Ctl::Switch(None))
                    }
                })
                .collect();
            let order_sub = if s.tab_order.is_empty() {
                "Using the default tab order"
            } else {
                "Restore the default tab order"
            };
            vec![
                Block::Group(Some("When the island opens"), open),
                Block::Group(Some("Tabs in the island"), tabs),
                Block::Group(Some("Tab order"), vec![item(Key::ResetTabOrder, "Reset to default", Some(order_sub), Ctl::Button("Reset"))]),
                Block::Note("Drag rows above or tabs in the island header to reorder them. Add-ons always stays at the end."),
            ]
        }
        Section::Addons => vec![Block::Addons],
        Section::Focus => {
            let p = &s.pomodoro;
            let every = match p.long_every {
                0 => "Never".to_string(),
                1 => "1 session".to_string(),
                n => format!("{n} sessions"),
            };
            vec![
                Block::Group(
                    Some("Lengths"),
                    vec![
                        item(Key::FocusMin, "Focus", None, Ctl::Stepper(minutes(p.focus_min))),
                        item(Key::ShortBreak, "Short break", None, Ctl::Stepper(minutes(p.short_break_min))),
                        item(Key::LongBreak, "Long break", None, Ctl::Stepper(minutes(p.long_break_min))),
                        item(Key::LongEvery, "Long break after", Some("Focus sessions before a long break"), Ctl::Stepper(every)),
                    ],
                ),
                Block::Group(
                    Some("Starting by itself"),
                    vec![
                        switch(Key::AutoBreaks, "Breaks", Some("A break starts when a focus session ends"), p.auto_start_breaks),
                        switch(Key::AutoFocus, "Focus sessions", Some("The next session starts when a break ends"), p.auto_start_focus),
                    ],
                ),
            ]
        }
        Section::Prayer => {
            let p = &s.prayer;
            let countdown = if p.remind_before_min == 0 { "Off".to_string() } else { minutes(p.remind_before_min) };
            let sound_line = halo_core::addons::sound_line(p.sound, p.sound.voice().map(|v| st.addon(v.id)));
            vec![
                Block::Group(
                    Some("Times and Hijri dates"),
                    vec![item(Key::PrayerCalendar, "Follow", Some("Jabatan Kemajuan Islam Malaysia (JAKIM)"), Ctl::Choice(names(&FOLLOWS), chosen(&FOLLOWS, p.follow)))],
                ),
                Block::Group(
                    Some("When it's time"),
                    vec![
                        item(Key::PrayerSound, "Sound", Some(sound_line.as_str()), Ctl::Choice(names(&SOUNDS), chosen(&SOUNDS, p.sound))),
                        switch(Key::PrayerPause, "Pause music for the adzan", None, p.pause_music),
                        item(Key::PrayerCountdown, "Countdown before each prayer", Some("On the closed island"), Ctl::Stepper(countdown)),
                    ],
                ),
                Block::Group(Some("Place"), vec![switch(Key::PrayerFollow, "Follow my location", Some("Windows' location picks the nearest place"), p.follow_location)]),
                Block::Note("Pick a place by hand on the island's Prayer page."),
            ]
        }
        Section::Ai => {
            let u = &s.usage;
            let mut rows = vec![switch(Key::AiNotices, "Heads-up notices", Some("When a plan's limit gets close, runs out or comes back"), u.notices)];
            if u.notices {
                rows.push(item(Key::AiNear, "Close to the limit at", None, Ctl::Stepper(format!("{} %", st.usage_near_pct()))));
            }
            vec![Block::Group(None, rows), Block::Note("Services and keys (a Mireld key from portal.mireld.my too) are on the AI tab, under Services.")]
        }
        Section::Caffeine => {
            let c = s.caffeine.sane();
            let remind = format!("On the island, {} minutes before", caffeine::REMIND_BEFORE);
            vec![
                Block::Group(
                    Some("Last call"),
                    vec![
                        item(Key::CaffeineBedtime, "Bedtime", Some("When you usually go to sleep"), Ctl::Stepper(caffeine::hhmm(c.bedtime.into()))),
                        item(Key::CaffeineHalfLife, "Half-life", Some("Most adults: 2 to 10 hours"), Ctl::Stepper(half_life_text(c.half_life))),
                        item(Key::CaffeineBedLimit, "Left at bedtime", Some("Last call is when more than this of one drink would still be in you at bedtime"), Ctl::Stepper(format!("{} mg", c.bed_limit_mg))),
                        switch(Key::CaffeineRemind, "Remind me before last call", Some(remind.as_str()), c.remind),
                    ],
                ),
                Block::Group(None, vec![item(Key::CaffeineDayLimit, "Day limit", Some("400 mg a day for healthy adults, 200 mg in pregnancy"), Ctl::Stepper(format!("{} mg", c.day_limit_mg)))]),
                Block::Note("Add what you drink on the island's Caffeine page."),
            ]
        }
        Section::HaloPro => {
            let mut blocks = vec![Block::ProKey];
            if s.pro {
                let c = s.custom_look.unwrap_or_default();
                let mut look = vec![switch(Key::CustomLook, "Theme builder", Some("Classic in your own colours"), s.custom_look.is_some())];
                if s.custom_look.is_some() {
                    look.push(item(Key::LookColours, "Colours", Some("Ground and ink"), Ctl::Stepper(format!("{}", halo_core::halo::SWATCHES.iter().position(|w| *w == (c.ground, c.ink)).map_or(0, |i| i + 1)))));
                    look.push(item(Key::LookRadius, "Corners", None, Ctl::Stepper(format!("{:.0}", c.radius))));
                    look.push(item(Key::LookGlass, "Glass", Some("How much of the screen shows through"), Ctl::Stepper(format!("{:.0} %", c.glass * 100.0))));
                }
                blocks.push(Block::Group(Some("Premium theme"), look));
                blocks.push(Block::Group(
                    Some("Between PCs"),
                    vec![switch(Key::CloudSync, "Cloud sync", Some("Settings and data through OneDrive; off any time"), s.cloud_sync)],
                ));
                blocks.push(Block::Note("Also on: game overlay, System's long history, 500-item clipboard with search, all 5 widgets."));
            } else {
                blocks.push(Block::Note("Pro: game overlay, System history + watts, Phone Drop anywhere, 500-item clipboard,"));
                blocks.push(Block::Note("all 5 widgets (adds world clock, moon, countdown), theme builder, cloud sync."));
            }
            blocks
        }
        Section::About => vec![
            Block::Version,
            Block::Note(crate::version::TAGLINE),
            Block::Note(crate::version::TAGLINE_PRO),
            Block::Group(
                None,
                vec![
                    item(Key::ReleaseNotes, "What's new", Some("What this version brings"), Ctl::Button("Open")),
                    item(Key::LyricsCredit, "Lyrics", Some("Synced lyrics come from LRCLIB"), Ctl::Button("lrclib.net")),
                ],
            ),
            Block::Group(
                Some("Files"),
                vec![
                    item(Key::DataFolder, "Data folder", Some("Tasks, the Shelf and your Focus history"), Ctl::Button("Open")),
                ],
            ),
        ],
    }
}

/// Every item the window can draw, by the section it lives in: its label
/// and its key. The rows are listed in `blocks`' order; one whose row the
/// window draws only sometimes is still here.
/// A test checks this against `blocks` both ways, and `docs/advanced-settings.md`
/// is the same list.
#[cfg(test)]
const INVENTORY: &[(Section, &str, Key)] = &[
    (Section::General, AUTOSTART.0, Key::Autostart),
    (Section::General, "Sounds", Key::Sounds),
    (Section::General, "24-hour clock", Key::Clock24),
    (Section::General, "News on Today", Key::NewsFeed),
    (Section::General, "Keep System page history", Key::SystemHistory),
    (Section::General, "Drink water", Key::WaterEvery),
    (Section::General, "Glasses a day", Key::WaterGoal),
    (Section::General, "Clipboard", Key::Clips),
    (Section::General, "Widget slots", Key::Widget(0)),
    (Section::General, "Add a widget", Key::AddWidget),
    (Section::Appearance, "Idle look", Key::IdleLook),
    (Section::Appearance, "Halo Mascot", Key::Mascot),
    (Section::Island, "Opens after", Key::HoverDelay),
    (Section::Island, "Closes", Key::CloseDelay),
    (Section::Island, "Width when open", Key::Width),
    (Section::Island, "Display", Key::Display),
    (Section::Island, "In full-screen apps", Key::Fullscreen),
    (Section::Island, "Show FPS, GPU and CPU over games", Key::GameStats),
    (Section::Island, "Only for a few seconds", Key::GamePeek),
    (Section::Island, "Mini handle", Key::StepAside),
    (Section::Dock, "Enable Bottom Dock", Key::DockEnabled),
    (Section::Dock, "Appearance", Key::DockTheme),
    (Section::Dock, "Icon size", Key::DockIconSize),
    (Section::Dock, "Hover magnification", Key::DockMagnification),
    (Section::Dock, "Auto-hide Windows taskbar", Key::DockHideTaskbar),
    (Section::Dock, "Anchor to bottom edge", Key::DockBottomMost),
    (Section::Dock, "Running apps", Key::DockShowApps),
    (Section::Dock, "Downloads stack", Key::DockShowDownloads),
    (Section::Dock, "System tray & Control Center", Key::DockShowTray),
    (Section::Music, "Synced lyrics", Key::Lyrics),
    (Section::Music, "Lyrics pronunciation", Key::Pronunciation),
    (Section::Music, "Timing", Key::LyricsOffset),
    (Section::Music, "Show what's playing", Key::MediaActivity),
    (Section::Popups, "Volume", Key::HudVolume),
    (Section::Popups, "Brightness", Key::HudBrightness),
    (Section::Popups, "Charger and battery", Key::HudPower),
    (Section::Popups, "Bluetooth", Key::HudBluetooth),
    (Section::Popups, "Caps Lock", Key::HudCaps),
    (Section::Popups, "Wi-Fi", Key::HudWifi),
    (Section::Popups, "Microphone", Key::HudMic),
    (Section::Tabs, "Open where you left off", Key::RememberPage),
    (Section::Tabs, "Opens on", Key::StartPage),
    (Section::Tabs, "Music", Key::Tab(PageKind::Media)),
    (Section::Tabs, "Today", Key::Tab(PageKind::Today)),
    (Section::Tabs, "Shelf", Key::Tab(PageKind::Shelf)),
    (Section::Tabs, "Tasks", Key::Tab(PageKind::Tasks)),
    (Section::Tabs, "Focus", Key::Tab(PageKind::Focus)),
    (Section::Tabs, "Widgets", Key::Tab(PageKind::Widgets)),
    (Section::Tabs, "AI", Key::Tab(PageKind::Ai)),
    (Section::Tabs, "System", Key::Tab(PageKind::System)),
    (Section::Tabs, "Games", Key::Tab(PageKind::Games)),
    (Section::Tabs, "Script", Key::Tab(PageKind::Script)),
    (Section::Tabs, "Record", Key::Tab(PageKind::Record)),
    (Section::Tabs, "Prayer", Key::Tab(PageKind::Prayer)),
    (Section::Tabs, "Stocks", Key::Tab(PageKind::Stocks)),
    (Section::Tabs, "Caffeine", Key::Tab(PageKind::Caffeine)),
    (Section::Tabs, "Downloads", Key::Tab(PageKind::Downloads)),
    (Section::Tabs, "Notifications", Key::Tab(PageKind::Notifications)),
    (Section::Tabs, "Reset to default", Key::ResetTabOrder),
    (Section::Focus, "Focus", Key::FocusMin),
    (Section::Focus, "Short break", Key::ShortBreak),
    (Section::Focus, "Long break", Key::LongBreak),
    (Section::Focus, "Long break after", Key::LongEvery),
    (Section::Focus, "Breaks", Key::AutoBreaks),
    (Section::Focus, "Focus sessions", Key::AutoFocus),
    (Section::Prayer, "Follow", Key::PrayerCalendar),
    (Section::Prayer, "Sound", Key::PrayerSound),
    (Section::Prayer, "Pause music for the adzan", Key::PrayerPause),
    (Section::Prayer, "Countdown before each prayer", Key::PrayerCountdown),
    (Section::Prayer, "Follow my location", Key::PrayerFollow),
    (Section::Caffeine, "Bedtime", Key::CaffeineBedtime),
    (Section::Caffeine, "Half-life", Key::CaffeineHalfLife),
    (Section::Caffeine, "Left at bedtime", Key::CaffeineBedLimit),
    (Section::Caffeine, "Remind me before last call", Key::CaffeineRemind),
    (Section::Caffeine, "Day limit", Key::CaffeineDayLimit),
    (Section::Ai, "Heads-up notices", Key::AiNotices),
    (Section::Ai, "Close to the limit at", Key::AiNear),
    (Section::HaloPro, "Theme builder", Key::CustomLook),
    (Section::HaloPro, "Colours", Key::LookColours),
    (Section::HaloPro, "Corners", Key::LookRadius),
    (Section::HaloPro, "Glass", Key::LookGlass),
    (Section::HaloPro, "Cloud sync", Key::CloudSync),
    (Section::About, "What's new", Key::ReleaseNotes),
    (Section::About, "Lyrics", Key::LyricsCredit),
    (Section::About, "Data folder", Key::DataFolder),
];

/// The same key with a widget slot's number dropped: the slots are one item.
#[cfg(test)]
fn item_key(k: Key) -> Key {
    match k {
        Key::Widget(_) => Key::Widget(0),
        k => k,
    }
}

/// The widget id of `key`'s row in `section` (the self-test's film).
#[cfg(windows)]
pub fn row_id(section: Section, st: &AppState, key: Key) -> Option<u64> {
    blocks(section, st).iter().enumerate().find_map(|(bi, b)| match b {
        Block::Group(_, items) => items.iter().position(|it| it.key == key).map(|ri| wid_n(&format!("settingswin.row.{bi}"), ri as u64)),
        _ => None,
    })
}

/// The widget id of `section`'s row in the sidebar (the self-test's film).
#[cfg(windows)]
pub fn nav_id(st: &AppState, section: Section) -> Option<u64> {
    sections(st).iter().position(|s| *s == section).map(|i| wid_n("settingswin.nav", i as u64))
}

/// Step a number of minutes: by 5, by 1 below 5.
fn step_min(v: u32, d: i32, lo: u32, hi: u32) -> u32 {
    let by = if (d < 0 && v <= 5) || (d > 0 && v < 5) { 1 } else { 5 };
    (v as i64 + d as i64 * by).clamp(lo as i64, hi as i64) as u32
}

/// `v` moved `d` steps of `by` along their grid (a value between two goes
/// to the next one that way), kept to `lo..=hi`.
fn step_on(v: i32, d: i32, by: i32, lo: i32, hi: i32) -> i32 {
    let below = v.div_euclid(by) * by;
    let from = if d == 0 {
        v
    } else if d < 0 && below != v {
        below + by
    } else {
        below
    };
    (from + d * by).clamp(lo, hi)
}

/// Milligrams `d` steps along one of Caffeine's mg rows (`BED_LIMIT_MG`, `DAY_LIMIT_MG`).
fn step_mg(v: u16, d: i32, (lo, hi, by): (i32, i32, i32)) -> u16 {
    step_on(i32::from(v), d, by, lo, hi) as u16
}

/// Bedtime `d` quarter hours later (earlier for a negative `d`), within
/// the times offered (`caffeine::BEDTIMES`, which counts on from 24:00 after
/// midnight), kept as minutes after midnight.
fn step_bedtime(bedtime: u16, d: i32) -> u16 {
    let day = caffeine::DAY as i32;
    let mut v = i32::from(bedtime) % day;
    if v < caffeine::DAY_START as i32 {
        v += day;
    }
    let (lo, hi) = (i32::from(*caffeine::BEDTIMES.start()), i32::from(*caffeine::BEDTIMES.end()));
    (step_on(v, d, BEDTIME_STEP, lo, hi) % day) as u16
}

/// The half-life `d` places along `caffeine::HALF_LIVES` (one between two
/// goes to the next one that way), within its ends.
fn step_half_life(h: f32, d: i32) -> f32 {
    let list = caffeine::HALF_LIVES;
    (0..d.unsigned_abs()).fold(h, |h, _| {
        if d > 0 {
            list.iter().copied().find(|x| *x > h + 0.01).unwrap_or(list[list.len() - 1])
        } else {
            list.iter().rev().copied().find(|x| *x < h - 0.01).unwrap_or(list[0])
        }
    })
}

/// Carry out `act` on the row `key`. Settings save through
/// `Command::SaveSettings`, as the island's pages save them.
pub fn apply(st: &mut AppState, key: Key, act: Act) {
    if act == Act::Preview {
        if let Some(hud) = preview_hud(key) {
            st.cmd(Command::PreviewHud(hud));
        }
        return;
    }
    let (notch_type, notch_theme) = (st.settings.notch_type, st.settings.notch_theme);
    let s = &mut st.settings;
    let step = if let Act::Step(d) = act { d } else { 0 };
    match key {
        Key::WaterEvery => match act {
            Act::Pick(i) if i < WATER_EVERY.len() => s.water_every_min = WATER_EVERY[i].0,
            _ => return,
        },
        Key::WaterGoal => s.water_goal = (s.water_goal as i32 + step).clamp(1, 20) as u8,
        Key::Clips => {
            s.clips = !s.clips;
            if !s.clips {
                st.clips.items.clear();
            }
        }
        Key::Widget(i) => {
            let i = i as usize;
            let offered: Vec<halo_core::halo::Widget> = halo_core::halo::Widget::ALL.into_iter().filter(|w| s.pro || !w.pro()).collect();
            match act {
                Act::Pick(n) if n < offered.len() && i < s.widgets.len() => s.widgets[i] = offered[n],
                Act::Pick(n) if n == offered.len() && i < s.widgets.len() => {
                    s.widgets.remove(i);
                }
                _ => return,
            }
        }
        Key::AddWidget => {
            let next = halo_core::halo::Widget::ALL.into_iter().find(|w| !s.widgets.contains(w) && (s.pro || !w.pro()));
            match next {
                Some(w) if s.widgets.len() < halo_core::halo::widget_slots(s.pro) => s.widgets.push(w),
                _ => return,
            }
        }
        Key::CustomLook => s.custom_look = if s.custom_look.is_some() { None } else { Some(Default::default()) },
        Key::LookColours | Key::LookRadius | Key::LookGlass => {
            let Some(c) = s.custom_look.as_mut() else { return };
            match key {
                Key::LookColours => {
                    let n = halo_core::halo::SWATCHES.len() as i32;
                    let at = halo_core::halo::SWATCHES.iter().position(|w| *w == (c.ground, c.ink)).map_or(0, |i| i as i32);
                    (c.ground, c.ink) = halo_core::halo::SWATCHES[(at + step).rem_euclid(n) as usize];
                }
                Key::LookRadius => c.radius = (c.radius + 2.0 * step as f32).clamp(2.0, 18.0),
                _ => c.glass = (c.glass + 0.25 * step as f32).clamp(0.0, 1.0),
            }
        }
        Key::CloudSync => s.cloud_sync = !s.cloud_sync,
        Key::StepAside => s.step_aside = !s.step_aside,
        Key::DockEnabled => {
            if !s.pro {
                st.notify(Icon::Lock, palette::YELLOW, "Bottom dock is Halo Pro", "Activate Halo Pro in Settings to unlock the bottom dock", 5.0, None);
                return;
            }
            s.dock_enabled = !s.dock_enabled;
        }
        Key::DockTheme => match act {
            Act::Pick(0) => s.dock_theme = DockThemeMode::FollowIsland,
            Act::Pick(1) => s.dock_theme = DockThemeMode::LiquidGlass,
            Act::Pick(2) => s.dock_theme = DockThemeMode::Classic,
            Act::Toggle => {
                s.dock_theme = match s.dock_theme {
                    DockThemeMode::FollowIsland => DockThemeMode::LiquidGlass,
                    DockThemeMode::LiquidGlass => DockThemeMode::Classic,
                    DockThemeMode::Classic => DockThemeMode::FollowIsland,
                };
            }
            _ => return,
        },
        Key::DockIconSize => match act {
            Act::Pick(0) => s.dock_icon_size = DockIconSize::Compact,
            Act::Pick(1) => s.dock_icon_size = DockIconSize::Default,
            Act::Pick(2) => s.dock_icon_size = DockIconSize::Large,
            _ => return,
        },
        Key::DockMagnification => s.dock_magnification = !s.dock_magnification,
        Key::DockHideTaskbar => s.dock_hide_taskbar = !s.dock_hide_taskbar,
        Key::DockBottomMost => s.dock_bottom_most = !s.dock_bottom_most,
        Key::DockShowApps => s.dock_show_apps = !s.dock_show_apps,
        Key::DockShowDownloads => s.dock_show_downloads = !s.dock_show_downloads,
        Key::DockShowTray => s.dock_show_tray = !s.dock_show_tray,
        Key::Autostart => {
            if let Some(on) = st.ui.autostart {
                st.cmd(Command::SetAutostart(!on));
            }
            return;
        }
        Key::Sounds => s.sounds = !s.sounds,
        Key::Clock24 => s.use_24h = !s.use_24h,
        Key::SystemHistory => s.system_history = !s.system_history,
        Key::Mascot => s.mascot = !s.mascot,
        Key::IdleLook => {
            let looks = status::looks(notch_type, notch_theme);
            match act {
                Act::Pick(i) if i < looks.len() => s.idle_style = looks[i],
                _ => return,
            }
        }
        Key::HoverDelay => match act {
            Act::Pick(i) if i < HOVER.len() => s.hover_delay_ms = HOVER[i].0,
            _ => return,
        },
        Key::CloseDelay => match act {
            Act::Pick(i) if i < CLOSE.len() => s.collapse_delay_ms = CLOSE[i].0,
            _ => return,
        },
        Key::Width => s.expanded_width = (s.expanded_width + step as f32 * WIDTH_STEP).clamp(480.0, 900.0),
        Key::Display => match act {
            Act::Pick(i) if i < DISPLAYS.len() => s.monitor = DISPLAYS[i].0,
            _ => return,
        },
        Key::Fullscreen => match act {
            Act::Pick(i) if i < FULLSCREEN.len() => s.hide_in_fullscreen = FULLSCREEN[i].0,
            _ => return,
        },
        Key::GameStats => {
            if !s.pro {
                st.notify(Icon::Lock, palette::YELLOW, "Game overlay is Halo Pro", "Activate Halo Pro in Settings to show FPS, GPU and CPU over games", 5.0, None);
                return;
            }
            s.game_stats = !s.game_stats;
        }
        Key::GamePeek => s.game_peek = !s.game_peek,
        Key::Lyrics => s.lyrics = !s.lyrics,
        Key::Pronunciation => s.lyrics_pronunciation = !s.lyrics_pronunciation,
        Key::MediaActivity => s.show_media_activity = !s.show_media_activity,
        // + shows the lines later, as people hear a late line.
        Key::LyricsOffset => s.lyrics_offset_ms = (s.lyrics_offset_ms - step * OFFSET_STEP).clamp(-5000, 5000),
        Key::HudVolume => s.hud_volume = !s.hud_volume,
        Key::HudBrightness => s.hud_brightness = !s.hud_brightness,
        Key::HudPower => s.hud_power = !s.hud_power,
        Key::HudBluetooth => s.hud_bluetooth = !s.hud_bluetooth,
        Key::HudCaps => s.hud_caps = !s.hud_caps,
        Key::HudWifi => s.hud_wifi = !s.hud_wifi,
        Key::HudMic => s.hud_mic = !s.hud_mic,
        Key::ResetTabOrder => s.tab_order.clear(),
        Key::RememberPage => s.remember_page = !s.remember_page,
        Key::StartPage => {
            let pages = start_pages(st);
            if pages.is_empty() || step == 0 {
                return;
            }
            let at = pages.iter().position(|p| p.key() == st.settings.start_page).unwrap_or(0) as i32;
            let next = (at + step).rem_euclid(pages.len() as i32) as usize;
            st.settings.start_page = pages[next].key().to_string();
        }
        // The presets in turn; an address of its own steps to the first one.
        Key::NewsFeed => {
            if step == 0 {
                return;
            }
            let n = halo_core::news::PRESETS.len() as i32;
            let next = match halo_core::news::preset_index(&s.news_feed) {
                Some(i) => (i as i32 + step).rem_euclid(n) as usize,
                None => 0,
            };
            s.news_feed = halo_core::news::PRESETS[next].0.to_string();
            // Another source: its stories, not the last one's.
            st.news = Default::default();
        }
        Key::Tab(p) => {
            if !p.hideable() {
                return;
            }
            let hidden = &mut s.hidden_tabs;
            match hidden.iter().position(|k| k == p.key()) {
                Some(i) => {
                    hidden.remove(i);
                }
                None => hidden.push(p.key().to_string()),
            }
        }
        Key::FocusMin => s.pomodoro.focus_min = step_min(s.pomodoro.focus_min, step, 1, 180),
        Key::ShortBreak => s.pomodoro.short_break_min = (s.pomodoro.short_break_min as i32 + step).clamp(1, 60) as u32,
        Key::LongBreak => s.pomodoro.long_break_min = step_min(s.pomodoro.long_break_min, step, 1, 120),
        Key::LongEvery => s.pomodoro.long_every = (s.pomodoro.long_every as i32 + step).clamp(0, 12) as u32,
        Key::AutoBreaks => s.pomodoro.auto_start_breaks = !s.pomodoro.auto_start_breaks,
        Key::AutoFocus => s.pomodoro.auto_start_focus = !s.pomodoro.auto_start_focus,
        Key::PrayerSound => match act {
            Act::Pick(i) if i < SOUNDS.len() => s.prayer.sound = SOUNDS[i].0,
            _ => return,
        },
        Key::PrayerCountdown => s.prayer.remind_before_min = step_min(s.prayer.remind_before_min, step, 0, 60),
        Key::PrayerPause => s.prayer.pause_music = !s.prayer.pause_music,
        Key::PrayerFollow => s.prayer.follow_location = !s.prayer.follow_location,
        Key::PrayerCalendar => match act {
            Act::Pick(i) if i < FOLLOWS.len() => s.prayer.follow = FOLLOWS[i].0,
            _ => return,
        },
        Key::CaffeineBedtime => s.caffeine.bedtime = step_bedtime(s.caffeine.sane().bedtime, step),
        Key::CaffeineHalfLife => s.caffeine.half_life = step_half_life(s.caffeine.sane().half_life, step),
        Key::CaffeineBedLimit => s.caffeine.bed_limit_mg = step_mg(s.caffeine.sane().bed_limit_mg, step, BED_LIMIT_MG),
        Key::CaffeineDayLimit => s.caffeine.day_limit_mg = step_mg(s.caffeine.sane().day_limit_mg, step, DAY_LIMIT_MG),
        Key::CaffeineRemind => s.caffeine.remind = !s.caffeine.remind,
        Key::AiNotices => s.usage.notices = !s.usage.notices,
        Key::AiNear => s.usage.near_pct = (s.usage.near_pct as i32 + step * 5).clamp(50, 95) as u8,
        Key::DataFolder => {
            st.cmd(Command::OpenPath(crate::paths::data_dir().to_string_lossy().into_owned()));
            return;
        }
        Key::ReleaseNotes => {
            // The island opens on its What's New sheet, as the tray's item does.
            st.cmd(Command::ShowWhatsNew);
            return;
        }
        Key::LyricsCredit => {
            st.cmd(Command::OpenUrl("https://lrclib.net".into()));
            return;
        }
    }
    st.cmd(Command::SaveSettings);
}

// ---------------------------------------------------------------- the view

/// Where the content goes in the window that shows it.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// The card: what the window shows of the content (DIPs).
    pub card: Rect,
    /// The window draws its own shadow, edge and close button (Windows' is
    /// borderless); the Mac's window has the system's.
    pub own_chrome: bool,
    /// Room at the top of the sidebar for the Mac's window buttons.
    pub top: f32,
    /// Everything lands where it's going at once: the system asks for less
    /// motion, or a picture is taken.
    pub still: bool,
}

/// What a frame of the window asked for.
#[derive(Clone, Copy, Debug, Default)]
pub struct Painted {
    /// A click changed something: draw it again (after the app ran the
    /// commands it pushed).
    pub again: bool,
    /// The close button was clicked.
    pub close: bool,
    /// Something still glides: draw the next frame at the display's rate.
    pub moving: bool,
}

/// The content and its input, whichever window shows it.
pub struct View {
    pub input: Input,
    hits: Vec<Hit>,
    pub section: Section,
    /// Sidebar quick-search query (empty = show `section`).
    pub query: String,
    scroll: Scroll,
    motion: Motion,
    /// Something it shows changed: draw it again.
    pub dirty: bool,
    /// Something glided in the last frame: the window keeps drawing frames.
    pub moving: bool,
}

/// `a` and `b` together.
fn span(a: Rect, b: Rect) -> Rect {
    let (x0, y0) = (a.x.min(b.x), a.y.min(b.y));
    Rect::new(x0, y0, a.right().max(b.right()) - x0, a.bottom().max(b.bottom()) - y0)
}

/// How far the content is scrolled (DIPs): where it is, where it's going
/// (a wheel glides there; a trackpad goes there at once, under the
/// fingers), and how tall it was last drawn.
#[derive(Default)]
struct Scroll {
    at: f32,
    to: f32,
    content_h: f32,
    /// A trackpad's points since the last frame.
    pending: f32,
}

impl View {
    pub fn new(section: Section) -> Self {
        View { input: Input::default(), hits: Vec::new(), section, query: String::new(), scroll: Scroll::default(), motion: Motion::default(), dirty: true, moving: false }
    }

    /// Focus the sidebar search box (Ctrl+F).
    pub fn focus_search(&mut self) {
        self.input.focused = Some(wid("settingswin.search"));
        self.input.caret = self.query.chars().count();
        self.dirty = true;
    }

    /// Show `section`, from outside the window (it opens on it): no glide,
    /// it's there at once.
    pub fn show_section(&mut self, section: Section) {
        self.query.clear();
        if section != self.section {
            self.section = section;
            self.scroll = Scroll::default();
            self.input.focused = None;
            self.motion.leave_section();
        }
        self.motion.snap_next = true;
        self.dirty = true;
    }

    /// The hit under `p` in the last frame.
    pub fn hit_at(&self, p: Point) -> Option<Hit> {
        self.hits.iter().rev().find(|h| h.rect.contains(p)).copied()
    }

    /// Where the widget `id` was in the last frame (the self-test's film).
    #[cfg(windows)]
    pub fn hit_rect(&self, id: u64) -> Option<Rect> {
        self.hits.iter().find(|h| h.id == id).map(|h| h.rect)
    }

    /// The pointer's cursor over `p`.
    pub fn cursor_at(&self, p: Point) -> CursorKind {
        self.hit_at(p).map_or(CursorKind::Arrow, |h| h.cursor)
    }

    /// The pointer moved to `p`. True when what's under it changed (or a
    /// press is held): the window is drawn again only then.
    pub fn hover(&mut self, p: Point) -> bool {
        let was = self.input.mouse.and_then(|m| self.hit_at(m)).map(|h| h.id);
        let entered = self.input.mouse.is_none();
        self.input.mouse = Some(p);
        entered || self.input.down || was != self.hit_at(p).map(|h| h.id)
    }

    /// The pointer left the window.
    pub fn leave(&mut self) {
        self.input.mouse = None;
    }

    pub fn press(&mut self, p: Point) {
        self.input.mouse = Some(p);
        self.input.down = true;
        self.input.just_pressed = true;
        self.input.press_pos = Some(p);
        self.input.pressed_id = self.hit_at(p).map(|h| h.id);
    }

    /// The button came up at `p`: a click when it's still on what it went
    /// down on (it shows on the next frame).
    pub fn release(&mut self, p: Point) {
        self.input.mouse = Some(p);
        if let (Some(pid), Some(h)) = (self.input.pressed_id, self.hit_at(p).map(|h| h.id)) {
            if pid == h {
                self.input.clicked = Some((h, MouseButton::Left));
            }
        }
        if self.input.down {
            self.input.released = self.input.pressed_id.map(|id| (id, p));
        }
        self.input.down = false;
        self.input.pressed_id = None;
    }

    /// A mouse wheel: whole notches, the content glides.
    pub fn wheel(&mut self, p: Point, notches: f32) {
        self.input.mouse = Some(p);
        self.input.wheel += notches;
    }

    /// What the next frame of a glide must redraw (DIPs): only what still
    /// moves. None = nothing moves, or the whole window must be drawn.
    pub fn next_clip(&self) -> Option<Rect> {
        self.motion.next.filter(|_| self.moving && !self.dirty)
    }

    /// A trackpad: `points` (+ = up), the content follows the fingers.
    pub fn scroll_by(&mut self, p: Point, points: f32) {
        self.input.mouse = Some(p);
        self.scroll.pending += points;
    }

    /// Draw the window on `c` for `st`, light or dark.
    pub fn draw(&mut self, c: &mut Canvas, st: &mut AppState, now: f64, frame: Frame, light: bool) -> Painted {
        self.dirty = false;
        // A section that went away (Prayer times switched off) is General.
        if !self.section.shown(st) {
            self.section = Section::General;
            self.scroll = Scroll::default();
            self.motion.leave_section();
        }
        self.motion.begin(now, frame.still);
        let mut out = Painted::default();
        let mut hits = std::mem::take(&mut self.hits);
        hits.clear();
        {
            let mut ui = Ui::new(c, &mut self.input, &mut hits, now);
            match draw(&mut ui, st, self.section, &mut self.query, frame, light, &mut self.scroll, &mut self.motion) {
                Some(Nav::Section(s)) => {
                    self.query.clear();
                    self.section = s;
                    self.scroll = Scroll::default();
                    self.motion.leave_section();
                    out.again = true;
                }
                Some(Nav::Close) => out.close = true,
                None => {}
            }
            out.again |= ui.repaint_requested();
        }
        self.hits = hits;
        self.input.end_frame();
        out.moving = self.motion.moving;
        self.moving = out.moving;
        out
    }
}

impl Drop for View {
    /// The window closes: the Diorama card's picture goes with it, so
    /// nothing of the window stays at rest.
    fn drop(&mut self) {
        let _ = DIORAMA_KEPT.try_with(|kept| kept.try_borrow_mut().map(|mut k| k.take()));
    }
}

// ---------------------------------------------------------------- motion

/// How fast a value glides: a critically damped spring of this many
/// radians a second, so it never overshoots and has all but arrived after
/// about 4.5 / pace seconds.
/// Hover and press washes: about a tenth of a second.
const WASH: f32 = 46.0;
/// A switch's knob and colour.
const SWITCHING: f32 = 24.0;
/// A highlight going to another row or choice.
const SLIDE: f32 = 24.0;
/// The content after a mouse wheel.
const SCROLLING: f32 = 22.0;
/// A motion's first step out of rest: about one refresh (144 Hz's is 1/144,
/// 60 Hz's 1/60), however long the window was still.
const FIRST_STEP: f32 = 1.0 / 120.0;

/// Values that glide to where they're going, a step a frame at the
/// display's own rate (120 Hz on ProMotion, 144 Hz on a 144 Hz screen),
/// and stop there. Only answers to the person move: a value first asked
/// for starts where it's going, so the window opens complete.
#[derive(Default)]
struct Motion {
    /// Each value, its speed, and whether it is the section's content's,
    /// by key.
    vals: HashMap<u64, (f32, f32, bool)>,
    /// Values asked for from now on (this frame) are the section's content's.
    content: bool,
    /// Seconds since the last frame, at most a thirtieth: a long pause
    /// doesn't jump.
    dt: f32,
    last: f64,
    /// Something hasn't arrived yet.
    moving: bool,
    /// This frame, everything lands at once.
    snap: bool,
    /// So does the next one (a section opened from outside).
    snap_next: bool,
    /// Where what still moves will be drawn on the next frame (from where
    /// it is to where it's going): a frame of a glide redraws only that.
    next: Option<Rect>,
}

impl Motion {
    fn begin(&mut self, now: f64, still: bool) {
        // Out of rest, a motion starts with one refresh's step, not the whole
        // time since the last frame (a jump on its first frame).
        let dt = if self.moving { (now - self.last) as f32 } else { FIRST_STEP };
        self.dt = dt.clamp(0.0, 1.0 / 30.0);
        self.last = now;
        self.moving = false;
        self.snap = still || std::mem::take(&mut self.snap_next);
        self.next = None;
        self.content = false;
    }

    /// Another section opens: its rows start where they're going, not where
    /// the last section's rows in their place were (a switch would slide).
    fn leave_section(&mut self) {
        self.vals.retain(|_, v| !v.2);
    }

    /// Something in `r` still moves: the next frame redraws it.
    fn cover(&mut self, r: Rect) {
        self.next = Some(match self.next {
            Some(n) => {
                let (x0, y0) = (n.x.min(r.x), n.y.min(r.y));
                Rect::new(x0, y0, n.right().max(r.right()) - x0, n.bottom().max(r.bottom()) - y0)
            }
            None => r,
        });
    }

    /// `key`'s value this frame on its way to `target` at `pace`; within
    /// `near` it has arrived.
    fn glide(&mut self, key: u64, target: f32, pace: f32, near: f32) -> f32 {
        let content = self.content;
        let e = self.vals.entry(key).or_insert((target, 0.0, content));
        let (x, v, _) = *e;
        if self.snap || ((x - target).abs() < near && v.abs() < near * pace) {
            *e = (target, 0.0, content);
            return target;
        }
        self.moving = true;
        if self.dt <= 0.0 {
            return x;
        }
        let (t, d) = (self.dt, x - target);
        let k = (-pace * t).exp();
        let nd = (d + (v + pace * d) * t) * k;
        let nv = (v - pace * (v + pace * d) * t) * k;
        *e = (target + nd, nv, content);
        target + nd
    }

    /// A 0..1 value (an opacity, a switch's way across): within 0.4 % it
    /// has arrived.
    fn fade(&mut self, key: u64, target: f32, pace: f32) -> f32 {
        self.glide(key, target, pace, 0.004)
    }

    /// A position or a size, in DIPs: within a third of one it has arrived
    /// (under a pixel at any scale), so no frames go to a tail nobody sees.
    fn slide(&mut self, key: u64, target: f32, pace: f32) -> f32 {
        self.glide(key, target, pace, 0.3)
    }

    /// Put `key` at `value` now (a trackpad's scroll).
    fn jump(&mut self, key: u64, value: f32) {
        self.vals.insert(key, (value, 0.0, self.content));
    }
}

/// A key for one of a widget's moving values.
fn mkey(id: u64, what: u64) -> u64 {
    id.wrapping_mul(0x9E3779B97F4A7C15) ^ what
}

const K_WASH: u64 = 1;
const K_ON: u64 = 2;
const K_X: u64 = 3;
const K_W: u64 = 4;
const K_Y: u64 = 5;
const K_LIT: u64 = 6;

// ---------------------------------------------------------------- drawing

/// Where a click in the window's frame goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Nav {
    Section(Section),
    Close,
}

/// Matching settings across every visible section for `query` (case-insensitive word match).
fn search_blocks(st: &AppState, query: &str) -> (Vec<Block>, Vec<Section>) {
    let words: Vec<String> = query.split_whitespace().map(|w| w.to_lowercase()).collect();
    if words.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let mut out = Vec::new();
    let mut matched_secs = Vec::new();
    for sec in sections(st) {
        let sec_low = sec.title().to_lowercase();
        let mut items = Vec::new();
        for b in blocks(sec, st) {
            if let Block::Group(gtitle, rows) = b {
                let g_low = gtitle.unwrap_or("").to_lowercase();
                for it in rows {
                    let lbl = it.label.to_lowercase();
                    let sub = it.sub.as_deref().unwrap_or("").to_lowercase();
                    if words.iter().all(|w| lbl.contains(w) || sub.contains(w) || sec_low.contains(w) || g_low.contains(w)) {
                        items.push(it);
                    }
                }
            }
        }
        if !items.is_empty() {
            matched_secs.push(sec);
            out.push(Block::Group(Some(sec.title()), items));
        }
    }
    (out, matched_secs)
}

/// The whole window: the card, the sidebar and the section (or search results).
/// Returns the section picked in the sidebar, or the close button.
#[allow(clippy::too_many_arguments)]
fn draw(ui: &mut Ui, st: &mut AppState, section: Section, query: &mut String, frame: Frame, light: bool, sc: &mut Scroll, m: &mut Motion) -> Option<Nav> {
    let card = frame.card;
    let sf = surfaces(light);
    // Windows' borderless window: its own shadow and rounded corners.
    let radius = if frame.own_chrome { RADIUS } else { 0.0 };
    ui.c.unthemed(|c| {
        if frame.own_chrome {
            let shadow = Color::black(if light { 0.30 } else { 0.55 });
            c.rrect_shadow(card.offset(0.0, 4.0), RADIUS, shadow, &crate::pill::shape::SHADOW_RAMP);
        }
        c.fill_rrect(card, radius, sf.ground);
    });
    // The sidebar: the card's left part in its own shade, rounded only where
    // the card is. Nothing of it reaches past its edge, so no clip is needed
    // (a glide's frame elsewhere doesn't draw it).
    let side = Rect::new(card.x, card.y, SIDEBAR_W, card.h);
    ui.c.unthemed(|c| {
        c.fill_rrect(side, radius, sf.sidebar);
        if radius > 0.0 {
            c.fill_rect(Rect::new(side.right() - radius, side.y, radius, side.h), sf.sidebar);
        }
    });
    let px = 1.0 / ui.c.scale().max(0.5);
    ui.c.fill_rect(Rect::new(side.right() - px, side.y, px, side.h), RULE);
    if frame.own_chrome {
        ui.c.stroke_rrect(card.inset(px / 2.0, px / 2.0), RADIUS, EDGE, px);
    }

    let q_before = query.clone();
    let searching_now = !query.trim().is_empty();
    let (search_res, matched_secs) = if searching_now { search_blocks(st, query) } else { (Vec::new(), Vec::new()) };
    let picked = draw_sidebar(ui, st, side, section, query, &matched_secs, frame.top, m);
    if *query != q_before {
        sc.at = 0.0;
        sc.to = 0.0;
    }
    let searching = !query.trim().is_empty();
    let content = Rect::new(side.right() + PAD, card.y, card.right() - side.right() - 2.0 * PAD, card.h);
    let heading = if searching { format!("Results for \"{}\"", query.trim()) } else { section.title().to_string() };
    ui.c.text(&heading, Rect::new(content.x, card.y + TITLE_Y, content.w - 40.0, TITLE_H), &TextStyle::new(22.0).weight(Weight::Bold));
    let close = frame.own_chrome && draw_close(ui, card, m);

    let view = Rect::new(content.x - PAD / 2.0, card.y + CONTENT_Y, content.w + PAD, card.bottom() - CONTENT_Y - card.y - 1.0);
    // From here on, the section's own: its scroll and rows, whose values go
    // when another section opens.
    m.content = true;
    // Scrolling: a wheel glides to where it points, a trackpad follows the fingers.
    let max_scroll = (sc.content_h - view.h + 16.0).max(0.0);
    let wheel = ui.wheel_in(view);
    if wheel != 0.0 {
        sc.to -= wheel * WHEEL_STEP;
    }
    let skey = mkey(wid("settingswin.scroll"), K_Y);
    if sc.pending != 0.0 {
        sc.to = (sc.to - sc.pending).clamp(0.0, max_scroll);
        sc.pending = 0.0;
        m.jump(skey, sc.to);
    }
    sc.to = sc.to.clamp(0.0, max_scroll);
    sc.at = m.slide(skey, sc.to, SCROLLING);
    if sc.at != sc.to {
        // The whole content moves, and the edge it scrolls under.
        m.cover(view.inset(0.0, -2.0));
    }
    ui.c.clip(view);
    let mut y = view.y - sc.at;
    let x = content.x;
    let shown_blocks = if searching {
        if *query == q_before { search_res } else { search_blocks(st, query).0 }
    } else {
        blocks(section, st)
    };
    if searching && shown_blocks.is_empty() {
        let msg = format!("No settings match \"{}\"", query.trim());
        ui.c.text(&msg, Rect::new(x, y + 32.0, content.w, 22.0), &TextStyle::body().color(palette::TEXT_DIM).center());
        y += 64.0;
    } else {
        for (bi, b) in shown_blocks.into_iter().enumerate() {
            y = draw_block(ui, st, &b, bi, Rect::new(x, y, content.w, 0.0), view, &sf, m);
        }
    }
    ui.c.unclip();
    sc.content_h = y + sc.at - view.y;
    // The edge the content scrolls under.
    if sc.at > 0.5 {
        ui.c.fill_rect(Rect::new(view.x, view.y, view.w, px), RULE);
    }
    if close {
        return Some(Nav::Close);
    }
    picked.filter(|p| searching || *p != section).map(Nav::Section)
}

/// The close button at the card's top right (Windows). True when clicked.
fn draw_close(ui: &mut Ui, card: Rect, m: &mut Motion) -> bool {
    let r = Rect::new(card.right() - 14.0 - 28.0, card.y + 14.0, 28.0, 28.0);
    let id = wid("settingswin.close");
    let resp = ui.button(id, r);
    let to = if resp.pressed { 0.8 } else if resp.hovered { 0.6 } else { 0.0 };
    let a = m.fade(mkey(id, K_WASH), to, WASH);
    if a != to {
        m.cover(r);
    }
    if a > 0.0 {
        ui.c.fill_rrect(r, 8.0, palette::RED.with_alpha(a));
    }
    let t = (a / 0.6).min(1.0);
    let x = Rect::from_center(r.center(), 14.0, 14.0);
    ui.c.icon(Icon::Close, x, palette::TEXT_DIM.mul_alpha(1.0 - t));
    if t > 0.0 {
        ui.c.icon(Icon::Close, x, palette::ON_ACCENT.mul_alpha(t));
    }
    resp.clicked
}

/// The sidebar: the halo mascot's tile, the window's name, a quick search box,
/// then a row per section (About at the bottom). The highlight glides to the section picked.
/// `top`: room for the Mac's window buttons.
#[allow(clippy::too_many_arguments)]
fn draw_sidebar(ui: &mut Ui, st: &mut AppState, side: Rect, section: Section, query: &mut String, matched: &[Section], top: f32, m: &mut Motion) -> Option<Section> {
    let icon_c = Point::new(side.x + 20.0 + 12.0, side.y + top + 28.0);
    halo_mascot(ui.c, icon_c, 22.0);
    ui.c.text("Advanced settings", Rect::new(icon_c.x + 19.0, side.y + top + 16.0, side.w - 60.0, 24.0), &TextStyle::headline());

    // Quick search box under the title.
    let search_r = Rect::new(side.x + 12.0, side.y + top + SEARCH_Y, side.w - 24.0, SEARCH_H);
    let clear_w = if query.is_empty() { 0.0 } else { 22.0 };
    let field_r = Rect::new(search_r.x, search_r.y, (search_r.w - clear_w).max(40.0), search_r.h);
    let sresp = ui.text_input_row(wid("settingswin.search"), field_r, query, "Search settings", 48, st.accent, Some(Icon::Search));
    if sresp.cancelled {
        query.clear();
        ui.blur();
    }
    if !query.is_empty() {
        let cr = Rect::new(search_r.right() - 20.0, search_r.center().y - 10.0, 20.0, 20.0);
        if ui.icon_button(wid("settingswin.search.clear"), cr, Icon::Close, palette::TEXT_DIM, 11.0).clicked {
            query.clear();
            ui.blur();
        }
    }

    let searching = !query.trim().is_empty();
    let list = sections(st);
    let about_at = side.bottom() - 12.0 - NAV_H;
    let row_y = |i: usize, s: Section| if s == Section::About { about_at } else { side.y + top + NAV_Y + i as f32 * (NAV_H + NAV_GAP) };
    // The highlight, under the rows (hidden while viewing search results).
    let sel_key = mkey(wid("settingswin.nav.sel"), K_Y);
    if !searching {
        if let Some(i) = list.iter().position(|s| *s == section) {
            let to = row_y(i, section);
            let y = m.slide(sel_key, to, SLIDE);
            let r = Rect::new(side.x + 10.0, y, side.w - 20.0, NAV_H);
            if y != to {
                m.cover(span(r, Rect::new(r.x, to, r.w, r.h)));
            }
            ui.c.fill_rrect(r, 8.0, Color::white(0.12));
        }
    }
    let mut picked = None;
    for (i, s) in list.iter().enumerate() {
        let r = Rect::new(side.x + 10.0, row_y(i, *s), side.w - 20.0, NAV_H);
        let id = wid_n("settingswin.nav", i as u64);
        let resp = ui.button(id, r);
        if resp.clicked {
            picked = Some(*s);
        }
        let on = !searching && *s == section;
        let dim = searching && !matched.contains(s);
        let to = if on { 0.0 } else if resp.pressed { 0.09 } else if resp.hovered { 0.05 } else { 0.0 };
        let wash = m.fade(mkey(id, K_WASH), to, WASH);
        if wash != to {
            m.cover(r);
        }
        if wash > 0.0 {
            ui.c.fill_rrect(r, 8.0, Color::white(wash));
        }
        let tile = Rect::new(r.x + 8.0, r.center().y - NAV_TILE / 2.0, NAV_TILE, NAV_TILE);
        let row_alpha = if dim { 0.38 } else { 1.0 };
        ui.c.with_alpha(row_alpha, |c| {
            if *s == Section::About {
                halo_mascot(c, tile.center(), NAV_TILE);
            } else {
                c.fill_rrect(tile, 6.0, s.tint());
                c.icon(s.icon(), Rect::from_center(tile.center(), 13.0, 13.0), palette::ON_ACCENT);
            }
            let tx = tile.right() + 10.0;
            let style = TextStyle::body().weight(if on { Weight::Semibold } else { Weight::Regular }).color(if on { palette::TEXT } else { Color::white(0.82) });
            c.text(s.title(), Rect::new(tx, r.y, (r.right() - tx - 6.0).max(0.0), r.h), &style);
        });
    }
    picked
}

/// Where `r` meets the scrolled view (None = not on screen): widgets take
/// the pointer only there.
fn visible(r: Rect, view: Rect) -> Option<Rect> {
    let x0 = r.x.max(view.x);
    let y0 = r.y.max(view.y);
    let x1 = r.right().min(view.right());
    let y1 = r.bottom().min(view.bottom());
    (x1 > x0 && y1 > y0).then(|| Rect::new(x0, y0, x1 - x0, y1 - y0))
}

fn row_h(it: &Item) -> f32 {
    if it.sub.is_some() {
        ROW_H_SUB
    } else {
        ROW_H
    }
}

/// How tall a block is drawn.
fn block_h(st: &AppState, b: &Block) -> f32 {
    match b {
        Block::Group(title, items) => title.map_or(0.0, |_| GROUP_TITLE_H) + items.iter().map(row_h).sum::<f32>(),
        Block::Version => {
            if st.update.offer.is_some() {
                112.0
            } else {
                72.0
            }
        }
        Block::NotchTypes => GROUP_TITLE_H + cards_h(NOTCH_TYPES.len()),
        Block::Themes => GROUP_TITLE_H + cards_h(THEMES.len()),
        Block::Addons => crate::pages::addons::height(st),
        Block::Note(_) => 18.0,
        Block::ProKey => pro_key_h(st),
    }
}

/// A group's card.
fn fill_group(ui: &mut Ui, r: Rect, color: Color) {
    ui.c.unthemed(|c| c.fill_rrect(r, GROUP_R, color));
}

/// Draw block `b` (the `bi`-th) from `at`'s top left, `at.w` wide; returns
/// where the next one starts.
#[allow(clippy::too_many_arguments)]
fn draw_block(ui: &mut Ui, st: &mut AppState, b: &Block, bi: usize, at: Rect, view: Rect, sf: &Surfaces, m: &mut Motion) -> f32 {
    let h = block_h(st, b);
    let r = Rect::new(at.x, at.y, at.w, h);
    let next = r.bottom() + GROUP_GAP;
    if visible(r.inset(-4.0, -4.0), view).is_none() {
        return next;
    }
    match b {
        Block::Group(title, items) => {
            let mut y = r.y;
            if let Some(t) = title {
                ui.c.text(t, Rect::new(r.x + 2.0, y, r.w, GROUP_TITLE_H - 6.0), &TextStyle::footnote().weight(Weight::Semibold).color(palette::TEXT_DIM));
                y += GROUP_TITLE_H;
            }
            let card = Rect::new(r.x, y, r.w, r.bottom() - y);
            fill_group(ui, card, sf.group);
            if !items.is_empty() && items.iter().all(|it| matches!(it.key, Key::Tab(_))) {
                draw_tab_order_group(ui, st, card, items, view, m);
            } else {
                let accent = st.accent;
                let px = 1.0 / ui.c.scale().max(0.5);
                for (i, it) in items.iter().enumerate() {
                    let rh = row_h(it);
                    let row = Rect::new(card.x, y, card.w, rh);
                    if i > 0 {
                        ui.c.fill_rect(Rect::new(row.x + ROW_PAD, row.y, row.w - ROW_PAD, px), RULE);
                    }
                    if let Some(act) = draw_row(ui, it, (bi, i), row, view, accent, (i == 0, i + 1 == items.len()), m) {
                        apply(st, it.key, act);
                    }
                    y += rh;
                }
            }
        }
        Block::Version => draw_version(ui, st, r, view, sf),
        Block::NotchTypes => draw_notch_types(ui, st, r, view, sf, m),
        Block::Themes => draw_themes(ui, st, r, view, sf, m),
        Block::Addons => crate::pages::addons::draw(ui, st, r),
        Block::Note(t) => ui.c.text(t, Rect::new(r.x + 2.0, r.y, r.w, r.h), &TextStyle::footnote().color(palette::TEXT_FAINT)),
        Block::ProKey => {
            fill_group(ui, r, sf.group);
            if visible(r, view).is_some_and(|v| v.h >= r.h - 0.5) {
                draw_pro_key(ui, st, r.inset(ROW_PAD, 14.0));
            }
        }
    }
    next
}

/// Settings › Halo Pro, like an account: a badge that tells a trial from
/// permanent Pro at a glance, what the license is and when it ends (a
/// countdown, drawn from the island's own clock while the page shows), this
/// PC's bind, and the actions the app really has. The key field: the
/// built-in key is checked here (`halo_core::license`); a code from the
/// license server (HALO- and four groups of five) is redeemed there for
/// this install, and its token checked here too (`services::license`).
/// Nothing is asked of a server to draw it.
fn draw_pro_key(ui: &mut Ui, st: &mut AppState, r: Rect) {
    use halo_core::license::{self, State};
    let grant = st.ui.license.as_ref().and_then(|l| l.as_ref().ok()).cloned();
    let refused = st.ui.license.as_ref().and_then(|l| l.as_ref().err().copied().flatten());
    let key = !st.settings.pro_key.is_empty() && license::valid(&st.settings.pro_key);
    let state = State::of(grant.as_ref(), refused, key, crate::services::license::server().is_some());
    let pro = st.settings.pro;
    // The badge: its own colour per kind, so a trial never reads as permanent.
    let (fill, ink) = match state {
        State::Trial => (palette::ORANGE, Color::black(0.85)),
        State::Key | State::Licensed => (palette::YELLOW, Color::black(0.85)),
        State::Expired | State::Revoked => (palette::RED, palette::ON_ACCENT),
        State::Free | State::NoServer => (Color::white(0.14), palette::TEXT),
    };
    let badge = state.badge();
    let badge_st = TextStyle::caption().weight(Weight::Bold);
    let bw = ui.c.measure(badge, &badge_st, 300.0).0.ceil() + 16.0;
    ui.c.text("Halo Pro", Rect::new(r.x, r.y, r.w, 22.0), &TextStyle::headline());
    let hx = r.x + ui.c.measure("Halo Pro", &TextStyle::headline(), 300.0).0.ceil() + 10.0;
    let br = Rect::new(hx, r.y + 2.0, bw, 18.0);
    ui.c.fill_rrect(br, 9.0, fill);
    ui.c.text(badge, br, &badge_st.color(ink).center());
    ui.c.text(state.line(), Rect::new(r.x, r.y + 22.0, r.w, 17.0), &TextStyle::footnote().color(palette::TEXT_DIM));

    // What the license says: a line each, label on the left.
    let offset = halo_core::halo_account::offset_minutes(&st.local, st.unix) as i64 * 60;
    let mut rows: Vec<(&str, String)> = Vec::new();
    match (&grant, state) {
        (Some(g), _) => {
            rows.push(("Plan", if g.kind == "trial" { "Trial Pro".into() } else { "Halo Pro".into() }));
            rows.push(("Ends", match g.exp {
                Some(e) => format!("{} \u{b7} {}", license::countdown(Some(e), st.unix), license::end_date(e, offset)),
                None => "No end (permanent)".into(),
            }));
            rows.push(("Started", license::end_date(g.iat, offset)));
            rows.push(("License", format!("{}\u{2026}", g.lic.chars().take(8).collect::<String>())));
        }
        (None, State::Key) => {
            rows.push(("Plan", "Halo Pro, built-in key".into()));
            rows.push(("Ends", "No end (permanent)".into()));
            rows.push(("Key", "HALO-\u{2022}\u{2022}\u{2022}\u{2022}-\u{2022}\u{2022}\u{2022}\u{2022}".into()));
        }
        _ => {}
    }
    let device = st.ui.license_device.as_deref().map(license::short_device).unwrap_or_else(|| "not made yet".into());
    rows.push(("This PC", format!("{device} \u{b7} {}", if grant.is_some() { "this license's seat is here" } else { "a code takes this PC's seat when activated" })));
    rows.push(("Seats", if grant.is_some() { "1 PC at a time: this one".into() } else { "1 PC at a time per code".into() }));
    let label_st = TextStyle::footnote().color(palette::TEXT_DIM);
    let value_st = TextStyle::footnote().weight(Weight::Semibold).tabular();
    let mut y = r.y + 46.0;
    for (label, value) in &rows {
        let mut vw = (r.w - 90.0).max(0.0);
        // This PC's id, whole, for whoever looks it up in the license portal.
        if let Some(hash) = st.ui.license_device.clone().filter(|_| *label == "This PC") {
            let cw = ui.pill_width("Copy ID", false, true);
            let cr = Rect::new(r.right() - cw, y - 2.0, cw, PILL_SMALL);
            if ui.pill(wid("settingswin.pro.copy"), cr, "Copy ID", None, PillStyle::Utility).clicked {
                st.cmd(Command::ClipBack(hash));
            }
            vw = (cr.x - 8.0 - r.x - 90.0).max(0.0);
        }
        ui.c.text(label, Rect::new(r.x, y, 90.0, 17.0), &label_st);
        ui.c.text(value, Rect::new(r.x + 90.0, y, vw, 17.0), &value_st);
        y += PRO_ROW_H;
    }
    y += 6.0;

    // The key field and Activate, or Deactivate.
    let label = if pro { "Deactivate" } else { "Activate" };
    let bw = ui.pill_width(label, false, true);
    let field = Rect::new(r.x, y, (r.w - bw - PILL_GAP).max(0.0), PILL_SMALL + 4.0);
    let mut buf = std::mem::take(&mut st.ui.pro_key_input);
    let resp = if pro {
        let what = if grant.is_some() { "Deactivate removes the license from this PC. Its code activates it again, here or on another PC." } else { "Deactivate removes the key from this PC; type it again to come back." };
        ui.c.text(what, field.inset(2.0, 0.0), &TextStyle::footnote().color(palette::TEXT_DIM));
        Default::default()
    } else {
        ui.text_input(wid("settingswin.pro.key"), field, &mut buf, "HALO-XXXXX-XXXXX-XXXXX-XXXXX", 64)
    };
    if resp.changed {
        st.ui.pro_said = None;
        st.ui.pro_trouble = None;
    }
    let b = Rect::new(field.right() + PILL_GAP, y + 2.0, bw, PILL_SMALL);
    let pressed = ui.pill(wid("settingswin.pro.activate"), b, label, None, if pro { PillStyle::Utility } else { PillStyle::Primary(palette::YELLOW) }).clicked;
    if pro && pressed {
        st.settings.pro_key.clear();
        st.settings.pro = false;
        st.settings.licensed = false;
        st.ui.pro_said = None;
        st.cmd(Command::ForgetLicense);
        st.cmd(Command::SaveSettings);
    } else if !pro && (pressed || resp.submitted) && !st.ui.pro_redeeming && is_license_code(&buf) {
        // A license server's code: sent there once; the answer turns Pro on.
        st.ui.pro_redeeming = true;
        st.ui.pro_said = None;
        st.ui.pro_trouble = None;
        st.cmd(Command::RedeemLicense(halo_core::license::normalize(&buf)));
        ui.blur();
    } else if !pro && (pressed || resp.submitted) && !st.ui.pro_redeeming {
        let ok = halo_core::license::valid(&buf);
        st.ui.pro_said = Some(ok);
        if ok {
            st.settings.pro_key = halo_core::license::normalize(&buf);
            st.settings.pro = true;
            buf.clear();
            ui.blur();
            st.cmd(Command::SaveSettings);
        }
    }
    st.ui.pro_key_input = buf;
    let line = Rect::new(r.x, y + PILL_SMALL + 8.0, r.w, 16.0);
    if st.ui.pro_redeeming {
        ui.c.text("Checking the code with the license server\u{2026}", line, &TextStyle::footnote().color(palette::TEXT_DIM));
    } else if let Some(t) = &st.ui.pro_trouble {
        ui.c.text(t, line, &TextStyle::footnote().color(palette::RED));
    } else if st.ui.pro_said == Some(false) {
        ui.c.text("Key not valid. Check it and try again.", line, &TextStyle::footnote().color(palette::RED));
    }
}

/// One line of the Halo Pro page's license details (room for Copy ID's pill).
const PRO_ROW_H: f32 = 24.0;

/// How tall the Halo Pro page's top block is: its head, a line per detail,
/// the field and its message.
fn pro_key_h(st: &AppState) -> f32 {
    let grant = st.ui.license.as_ref().is_some_and(|l| l.is_ok());
    let key = !st.settings.pro_key.is_empty() && halo_core::license::valid(&st.settings.pro_key);
    let details = if grant { 4 } else if key { 3 } else { 0 } + 2;
    28.0 + 46.0 + details as f32 * PRO_ROW_H + 6.0 + PILL_SMALL + 30.0
}

/// A license server's code: HALO- and four groups of five (no 0, 1, I, L, O).
fn is_license_code(s: &str) -> bool {
    let k = halo_core::license::normalize(s);
    let groups: Vec<&str> = k.split('-').collect();
    groups.len() == 5 && groups[0] == "HALO" && groups[1..].iter().all(|g| g.len() == 5 && g.bytes().all(|b| b"ABCDEFGHJKMNPQRSTUVWXYZ23456789".contains(&b)))
}

/// Draggable list of tabs in Settings › Tabs: drag any row vertically to
/// reorder `settings.tab_order`, or click a hideable row to toggle visibility.
fn draw_tab_order_group(ui: &mut Ui, st: &mut AppState, card: Rect, items: &[Item], view: Rect, m: &mut Motion) {
    let mut drag_just_ended = false;
    if !ui.input.down {
        if let Some((_, moved)) = st.ui.settings_tab_drag.take() {
            if moved {
                drag_just_ended = true;
                st.cmd(Command::SaveSettings);
            }
        }
    }

    let pages: Vec<PageKind> = items
        .iter()
        .filter_map(|it| match it.key {
            Key::Tab(p) => Some(p),
            _ => None,
        })
        .collect();
    let mut rects: Vec<Rect> = Vec::with_capacity(items.len());
    let mut y = card.y;
    for it in items {
        let rh = row_h(it);
        rects.push(Rect::new(card.x, y, card.w, rh));
        y += rh;
    }

    let accent = st.accent;
    let px = 1.0 / ui.c.scale().max(0.5);
    let mut reorder_req: Option<(PageKind, PageKind)> = None;
    let mut toggle_req: Option<PageKind> = None;

    for (i, it) in items.iter().enumerate() {
        let Key::Tab(p) = it.key else { continue };
        let row = rects[i];
        let ends = (i == 0, i + 1 == items.len());
        if i > 0 {
            ui.c.fill_rect(Rect::new(row.x + ROW_PAD, row.y, row.w - ROW_PAD, px), RULE);
        }
        // Key the widget by PageKind so dragging follows the tab across live swaps.
        let rid = wid_n("settingswin.tabrow", p as u64);
        let resp = match visible(row, view) {
            Some(hit) => ui.button(rid, hit),
            None => Default::default(),
        };
        if let Some((_, dy)) = resp.drag {
            let moved = st.ui.settings_tab_drag.is_some_and(|(dp, mv)| dp == p && mv) || dy.abs() >= 6.0;
            st.ui.settings_tab_drag = Some((p, moved));
            if moved {
                if let Some(my) = ui.input.mouse.map(|pt| pt.y) {
                    if let Some((ti, &target)) = pages
                        .iter()
                        .enumerate()
                        .min_by(|(ia, _), (ib, _)| (rects[*ia].center().y - my).abs().total_cmp(&(rects[*ib].center().y - my).abs()))
                    {
                        if ti != i && target != p {
                            reorder_req = Some((p, target));
                        }
                    }
                }
            }
        }
        let is_dragged = st.ui.settings_tab_drag.is_some_and(|(dp, mv)| dp == p && mv);
        if is_dragged {
            fill_row(ui, row, Color::white(0.12), ends);
            ui.c.stroke_rrect(row.inset(1.0, 1.0), 8.0, Color::white(0.28), 1.0);
        } else {
            let to = if resp.pressed { 0.07 } else if resp.hovered { 0.04 } else { 0.0 };
            let wash = m.fade(mkey(rid, K_WASH), to, WASH);
            if wash != to {
                m.cover(row);
            }
            if wash > 0.0 {
                fill_row(ui, row, Color::white(wash), ends);
            }
        }

        let inner = row.inset(ROW_PAD, 0.0);
        let cy = row.center().y;
        // 6-dot drag grip on the left.
        let grip_col = if is_dragged || resp.hovered { palette::TEXT_DIM } else { palette::TEXT_FAINT };
        for col in 0..2 {
            for r_dot in -1..=1 {
                let gx = inner.x + 2.0 + col as f32 * 4.5;
                let gy = cy + r_dot as f32 * 4.5;
                ui.c.fill_circle(Point::new(gx, gy), 1.15, grip_col);
            }
        }
        // Tinted mini icon badge for the tab.
        let badge = Rect::new(inner.x + 16.0, cy - 10.0, 20.0, 20.0);
        ui.c.fill_rrect(badge, 6.0, p.tint());
        ui.c.icon(p.icon(), Rect::from_center(badge.center(), 11.0, 11.0), palette::ON_ACCENT);

        // Visibility switch on the right.
        let ctl_x = inner.right() - SWITCH_W;
        if let Ctl::Switch(on) = &it.ctl {
            let to = if *on == Some(true) || on.is_none() { 1.0 } else { 0.0 };
            let k = m.fade(mkey(rid, K_ON), to, SWITCHING);
            let sw = Rect::new(ctl_x, cy - SWITCH_H / 2.0, SWITCH_W, SWITCH_H);
            if k != to {
                m.cover(sw.inset(-1.0, -1.0));
            }
            let alpha = if on.is_some() { 1.0 } else { 0.45 };
            ui.c.with_alpha(alpha, |c| draw_switch_at(c, sw, k, accent));
        }

        let tx = badge.right() + 10.0;
        let tw = (ctl_x - 16.0 - tx).max(0.0);
        match &it.sub {
            Some(sub) => {
                ui.c.text(&it.label, Rect::new(tx, cy - 19.0, tw, 19.0), &TextStyle::body());
                ui.c.text(sub, Rect::new(tx, cy + 1.0, tw, 17.0), &TextStyle::footnote().color(palette::TEXT_DIM));
            }
            None => ui.c.text(&it.label, Rect::new(tx, row.y, tw, row.h), &TextStyle::body()),
        }

        if resp.clicked && p.hideable() && !drag_just_ended && !is_dragged {
            toggle_req = Some(p);
        }
    }

    if let Some((tab, target)) = reorder_req {
        crate::pages::reorder_tab(st, &pages, tab, target);
    } else if let Some(p) = toggle_req {
        apply(st, Key::Tab(p), Act::Toggle);
    }
}

/// One row: its label (and dim line), and its control. Returns what a
/// click did. `ends`: the row is the card's first, its last.
#[allow(clippy::too_many_arguments)]
fn draw_row(ui: &mut Ui, it: &Item, at: (usize, usize), row: Rect, view: Rect, accent: Color, ends: (bool, bool), m: &mut Motion) -> Option<Act> {
    let id = |what: &str| wid_n(&format!("settingswin.{what}.{}", at.0), at.1 as u64);
    let shown = visible(row, view);
    let whole = shown.is_some_and(|v| v.h >= row.h - 0.5);
    let inner = row.inset(ROW_PAD, 0.0);
    let mut act = None;
    // The control's room at the right end.
    let ctl_w = match &it.ctl {
        Ctl::Switch(_) => SWITCH_W,
        Ctl::Choice(names, _) => picker_width(ui, names),
        Ctl::Stepper(_) => 2.0 * STEP_BTN + STEP_VALUE_W,
        Ctl::Button(label) => ui.pill_width(label, true, true),
    };
    let ctl_x = inner.right() - ctl_w;
    let mut label_right = ctl_x - 16.0;
    match &it.ctl {
        Ctl::Switch(on) => {
            // The whole row toggles, as on the island's Settings page.
            let rid = id("row");
            let resp = match (on, shown) {
                (Some(_), Some(hit)) => ui.button(rid, hit),
                _ => Default::default(),
            };
            let to = if resp.pressed { 0.07 } else if resp.hovered { 0.04 } else { 0.0 };
            let wash = m.fade(mkey(rid, K_WASH), to, WASH);
            if wash != to {
                m.cover(row);
            }
            if wash > 0.0 {
                fill_row(ui, row, Color::white(wash), ends);
            }
            if resp.clicked {
                act = Some(Act::Toggle);
            }
            // Pop-ups rows carry a compact live Preview pill beside the switch.
            if preview_hud(it.key).is_some() {
                let pv_h = 24.0;
                let pv_w = ui.pill_width("Preview", true, true).min(86.0);
                let pv_r = Rect::new(ctl_x - 10.0 - pv_w, row.center().y - pv_h / 2.0, pv_w, pv_h);
                label_right = pv_r.x - 12.0;
                if visible(pv_r, view).is_some_and(|v| v.h >= pv_r.h - 0.5) {
                    if ui.pill(id("preview"), pv_r, "Preview", Some(Icon::Play), PillStyle::Utility).clicked {
                        act = Some(Act::Preview);
                    }
                }
            }
            // The knob slides and the track takes the colour as it goes.
            let to = if *on == Some(true) { 1.0 } else { 0.0 };
            let k = m.fade(mkey(rid, K_ON), to, SWITCHING);
            let sw = Rect::new(ctl_x, row.center().y - SWITCH_H / 2.0, SWITCH_W, SWITCH_H);
            if k != to {
                m.cover(sw.inset(-1.0, -1.0));
            }
            let alpha = if on.is_some() { 1.0 } else { 0.5 };
            ui.c.with_alpha(alpha, |c| draw_switch_at(c, sw, k, accent));
        }
        Ctl::Choice(names, sel) => {
            let y = row.center().y - PICKER_H / 2.0;
            if let Some(i) = picker_glide(ui, m, &format!("settingswin.pick.{}.{}", at.0, at.1), ctl_x, y, names, *sel, whole) {
                act = Some(Act::Pick(i));
            }
        }
        Ctl::Stepper(value) => {
            let cy = row.center().y;
            let minus = Rect::new(ctl_x, cy - STEP_BTN / 2.0, STEP_BTN, STEP_BTN);
            let plus = Rect::new(inner.right() - STEP_BTN, cy - STEP_BTN / 2.0, STEP_BTN, STEP_BTN);
            for (r, icon, d, what) in [(minus, Icon::Minus, -1, "minus"), (plus, Icon::Plus, 1, "plus")] {
                let bid = id(what);
                let resp = match visible(r, view) {
                    Some(hit) => ui.button(bid, hit),
                    None => Default::default(),
                };
                let to = if resp.pressed { 1.0 } else if resp.hovered { 0.5 } else { 0.0 };
                let lit = m.fade(mkey(bid, K_WASH), to, WASH);
                if lit != to {
                    m.cover(r.inset(-1.0, -1.0));
                }
                let fill = if lit <= 0.5 { Color::lerp(palette::SURFACE, palette::SURFACE_HOVER, lit * 2.0) } else { Color::lerp(palette::SURFACE_HOVER, palette::SURFACE_PRESSED, lit * 2.0 - 1.0) };
                ui.c.fill_circle(r.center(), STEP_BTN / 2.0, fill);
                ui.c.icon(icon, Rect::from_center(r.center(), 12.0, 12.0), palette::TEXT);
                if resp.clicked {
                    act = Some(Act::Step(d));
                }
            }
            let vr = Rect::new(minus.right(), row.y, plus.x - minus.right(), row.h);
            ui.c.text(value, vr, &TextStyle::body().weight(Weight::Semibold).tabular().center());
        }
        Ctl::Button(label) => {
            let icon = if it.key == Key::ResetTabOrder { Icon::Reset } else { Icon::Open };
            let r = Rect::new(ctl_x, row.center().y - PILL_SMALL / 2.0, ctl_w, PILL_SMALL);
            let clicked = match visible(r, view).filter(|v| v.h >= r.h - 0.5) {
                Some(_) => ui.pill(id("button"), r, label, Some(icon), PillStyle::Utility).clicked,
                None => {
                    let mut none = Input::default();
                    let mut no_hits = Vec::new();
                    let mut ghost = Ui::new(&mut *ui.c, &mut none, &mut no_hits, 0.0);
                    ghost.pill(0, r, label, Some(icon), PillStyle::Utility);
                    false
                }
            };
            if clicked {
                act = Some(Act::Press);
            }
        }
    }
    // The label, and the dim line under it.
    let tw = (label_right - inner.x).max(0.0);
    match &it.sub {
        Some(sub) => {
            let cy = row.center().y;
            ui.c.text(&it.label, Rect::new(inner.x, cy - 19.0, tw, 19.0), &TextStyle::body());
            ui.c.text(sub, Rect::new(inner.x, cy + 1.0, tw, 17.0), &TextStyle::footnote().color(palette::TEXT_DIM));
        }
        None => ui.c.text(&it.label, Rect::new(inner.x, row.y, tw, row.h), &TextStyle::body()),
    }
    act
}

/// A switch `k` of the way on (0 off, 1 on): the knob slides across and
/// the track takes the accent as it goes (the island's switch, in motion).
fn draw_switch_at(c: &mut Canvas, r: Rect, k: f32, accent: Color) {
    let k = k.clamp(0.0, 1.0);
    c.fill_rrect(r, r.h / 2.0, palette::TRACK.mul_alpha(1.0 - k));
    if k > 0.0 {
        c.fill_rrect(r, r.h / 2.0, accent.mul_alpha(k));
    }
    let d = r.h - 2.0 * KNOB_INSET;
    let x0 = r.x + KNOB_INSET + d / 2.0;
    let x1 = r.right() - KNOB_INSET - d / 2.0;
    c.fill_circle(Point::new(x0 + (x1 - x0) * k, r.center().y), d / 2.0, palette::ON_ACCENT);
}

/// The island's picker (choices side by side in a track, the chosen one
/// lifted) with the lift gliding to the choice picked. `live`: it takes
/// clicks (not while partly scrolled away). Returns the choice clicked.
#[allow(clippy::too_many_arguments)]
fn picker_glide(ui: &mut Ui, m: &mut Motion, id: &str, x: f32, y: f32, names: &[&str], sel: Option<usize>, live: bool) -> Option<usize> {
    let st = TextStyle::footnote().weight(Weight::Semibold);
    let h = PICKER_H;
    let inner_h = h - 2.0 * SEG_INSET;
    let mut cx = x + SEG_INSET;
    let cells: Vec<Rect> = names
        .iter()
        .map(|n| {
            let w = (2.0 * SEG_PAD + ui.c.measure(n, &st, 300.0).0).ceil();
            let cell = Rect::new(cx, y + SEG_INSET, w, inner_h);
            cx += w;
            cell
        })
        .collect();
    ui.c.fill_rrect(Rect::new(x, y, cx + SEG_INSET - x, h), h / 2.0, palette::SURFACE);
    let base = wid(id);
    // The lift, under the names.
    let track = Rect::new(x, y, cx + SEG_INSET - x, h);
    if let Some(cell) = sel.and_then(|s| cells.get(s)) {
        let lx = m.slide(mkey(base, K_X), cell.x - x, SLIDE);
        let lw = m.slide(mkey(base, K_W), cell.w, SLIDE);
        if lx != cell.x - x || lw != cell.w {
            m.cover(track);
        }
        ui.c.fill_rrect(Rect::new(x + lx, cell.y, lw, inner_h), inner_h / 2.0, Color::white(0.16));
    }
    let mut picked = None;
    for (i, (name, cell)) in names.iter().zip(&cells).enumerate() {
        let cid = wid_n(id, i as u64);
        let chosen = sel == Some(i);
        let resp = if live && !chosen { ui.button(cid, Rect::new(cell.x, y, cell.w, h)) } else { Default::default() };
        if resp.clicked {
            picked = Some(i);
        }
        let to = if resp.pressed { 0.10 } else if resp.hovered { 0.06 } else { 0.0 };
        let wash = m.fade(mkey(cid, K_WASH), to, WASH);
        if wash > 0.0 {
            ui.c.fill_rrect(*cell, inner_h / 2.0, Color::white(wash));
        }
        let lit_to = if chosen || resp.hovered || resp.pressed { 1.0 } else { 0.0 };
        let lit = m.fade(mkey(cid, K_LIT), lit_to, WASH);
        if wash != to || lit != lit_to {
            m.cover(*cell);
        }
        ui.c.text(name, *cell, &st.color(Color::lerp(palette::TEXT_DIM, palette::TEXT, lit)).center());
    }
    picked
}

/// A row's hover wash, rounded where the row meets the card's corners.
fn fill_row(ui: &mut Ui, row: Rect, c: Color, (first, last): (bool, bool)) {
    if !(first || last) {
        ui.c.fill_rect(row, c);
        return;
    }
    ui.c.clip(row);
    let top = if first { row.y } else { row.y - GROUP_R };
    let bottom = if last { row.bottom() } else { row.bottom() + GROUP_R };
    ui.c.fill_rrect(Rect::new(row.x, top, row.w, bottom - top), GROUP_R, c);
    ui.c.unclip();
}

/// The halo mascot as the Settings logo (`res/halo.ico`): two capsule eyes
/// (8 x 16, 10 apart, Halo's #F2F5FA) on a near-black pill (#0B0D12) with
/// the icon's rounded corners, `size` DIPs square and centred on `center`.
/// Static: it draws once wherever it shows, nothing runs for it.
pub(crate) fn halo_mascot(c: &mut Canvas, center: Point, size: f32) {
    const PILL: Color = Color::hex(0x0B0D12);
    const EYES: Color = Color::hex(0xF2F5FA);
    let tile = Rect::new(center.x - size / 2.0, center.y - size / 2.0, size, size);
    c.unthemed(|c| {
        c.fill_rrect(tile, size * 0.2237, PILL);
        let eye = size * 8.0 / 48.0;
        let half = size * 5.0 / 48.0;
        for dx in [-half, half] {
            let e = Rect::from_center(Point::new(center.x + dx, center.y), eye, size * 16.0 / 48.0);
            c.fill_rrect(e, eye / 2.0, EYES);
        }
    });
}

/// The version card: the halo mascot's tile, the app's name and this
/// version, and Check now; with a later version out, what it brings and how
/// to get it (the island's Update sheet does the same).
fn draw_version(ui: &mut Ui, st: &mut AppState, r: Rect, view: Rect, sf: &Surfaces) {
    fill_group(ui, r, sf.group);
    let head = Rect::new(r.x + ROW_PAD, r.y, r.w - 2.0 * ROW_PAD, 72.0);
    let cy = head.center().y;
    halo_mascot(ui.c, Point::new(head.x + 20.0, cy), 40.0);
    let tx = head.x + 54.0;
    let clickable = visible(r, view).is_some_and(|v| v.h >= r.h - 0.5);
    let check = clickable.then(|| crate::pages::settings::check_slot(ui, st, head)).flatten();
    let right = check.map_or(head.right(), |b| b.x - 12.0);
    ui.c.text(crate::version::APP_NAME, Rect::new(tx, cy - 28.0, (right - tx).max(0.0), 22.0), &TextStyle::headline());
    ui.c.text(&format!("Version {VERSION}"), Rect::new(tx, cy - 6.0, (right - tx).max(0.0), 17.0), &TextStyle::footnote().color(palette::TEXT_DIM));
    let by = format!("by {}", crate::version::AUTHOR);
    ui.c.text(&by, Rect::new(tx, cy + 11.0, (right - tx).max(0.0), 16.0), &TextStyle::caption().color(palette::TEXT_DIM));
    if let Some(b) = check {
        crate::pages::settings::draw_check(ui, st, b);
    }
    let Some(rel) = st.update.offer.clone() else { return };
    let px = 1.0 / ui.c.scale().max(0.5);
    let row = Rect::new(head.x, head.bottom(), head.w, r.bottom() - head.bottom());
    ui.c.fill_rect(Rect::new(row.x, row.y, row.w, px), RULE);
    let accent = st.accent;
    // Windows with the Setup in: now or next start; otherwise its page (or the Mac's DMG).
    let mut buttons: Vec<(&str, Command, PillStyle)> = Vec::new();
    if st.update.ready {
        buttons.push(("Install now", Command::Update, PillStyle::Primary(accent)));
        let later = if st.update.next_start { "Set for next start" } else { "On next start" };
        buttons.push((later, Command::InstallOnNextStart, PillStyle::Utility));
    } else {
        buttons.push(("Download", Command::Update, PillStyle::Primary(accent)));
    }
    buttons.push(("What's new", Command::OpenUrl(rel.notes.clone()), PillStyle::Utility));
    let mut x = row.right();
    let by = row.center().y - PILL_SMALL / 2.0;
    let mut picked = None;
    for (i, (label, cmd, style)) in buttons.into_iter().enumerate().rev() {
        let w = ui.pill_width(label, false, true);
        x -= w;
        let b = Rect::new(x, by, w, PILL_SMALL);
        x -= PILL_GAP;
        let clicked = if clickable {
            ui.pill(wid_n("settingswin.update", i as u64), b, label, None, style).clicked
        } else {
            let mut none = Input::default();
            let mut no_hits = Vec::new();
            Ui::new(&mut *ui.c, &mut none, &mut no_hits, 0.0).pill(0, b, label, None, style);
            false
        };
        if clicked {
            picked = Some(cmd);
        }
    }
    let tw = (x - 12.0 - row.x).max(0.0);
    let cy = row.center().y;
    crate::activities::text_with_dim(ui, "Update available", &rel.version, Rect::new(row.x, cy - 19.0, tw, 19.0), TextStyle::body().weight(Weight::Semibold));
    ui.c.text(&rel.headline, Rect::new(row.x, cy + 1.0, tw, 17.0), &TextStyle::footnote().color(palette::TEXT_DIM));
    if let Some(c) = picked {
        st.cmd(c);
    }
}

/// How tall `n` cards are together: as many rows of [`THEME_COLS`] as they fill.
fn cards_h(n: usize) -> f32 {
    let rows = n.div_ceil(THEME_COLS) as f32;
    rows * THEME_CARD_H + (rows - 1.0).max(0.0) * THEME_GAP
}

/// The `i`-th card in a picker block `r` (under its caption).
fn theme_card(r: Rect, i: usize) -> Rect {
    let cols = THEME_COLS as f32;
    let w = ((r.w - (cols - 1.0) * THEME_GAP) / cols).floor();
    let (row, col) = ((i / THEME_COLS) as f32, (i % THEME_COLS) as f32);
    Rect::new(r.x + col * (w + THEME_GAP), r.y + GROUP_TITLE_H + row * (THEME_CARD_H + THEME_GAP), w, THEME_CARD_H)
}

#[allow(clippy::too_many_arguments)]
fn draw_picker_card(
    ui: &mut Ui,
    c: Rect,
    view: Rect,
    id: u64,
    on: bool,
    locked: bool,
    name: &str,
    line: &str,
    ty: NotchType,
    th: NotchTheme,
    accent: Color,
    sf: &Surfaces,
    m: &mut Motion,
) -> bool {
    let resp = match visible(c, view) {
        Some(hit) => ui.button(id, hit),
        None => Default::default(),
    };
    let label = if locked { format!("{name} (Pro)") } else { name.to_string() };
    fill_group(ui, c, sf.group);
    let lit_to = if resp.pressed { 1.0 } else if resp.hovered { 0.6 } else { 0.0 };
    let lit = m.fade(mkey(id, K_WASH), lit_to, WASH);
    if lit > 0.0 {
        let over = if lit > 0.6 { sf.group_press } else { sf.group_hover };
        ui.c.unthemed(|cv| cv.fill_rrect(c, GROUP_R, over.mul_alpha((lit / 0.6).min(1.0))));
    }
    let ring_to = if on { 1.0 } else { 0.0 };
    let ring = m.fade(mkey(id, K_ON), ring_to, SWITCHING);
    if lit != lit_to || ring != ring_to {
        m.cover(c.inset(-2.0, -2.0));
    }
    if ring > 0.0 {
        ui.c.stroke_rrect(c.inset(1.0, 1.0), GROUP_R - 1.0, accent.mul_alpha(ring), 2.0);
    }
    let screen = Rect::new(c.x + 10.0, c.y + 10.0, THEME_PREVIEW_W, THEME_PREVIEW_H);
    ui.c.unthemed(|cv| draw_combo_preview(cv, ty, th, screen));
    let tx = screen.right() + 12.0;
    let ty_text = c.y + 12.0;
    let tw = (c.right() - 10.0 - tx).max(0.0);
    let title_w = (tw - if on { 20.0 } else { 0.0 }).max(0.0);
    ui.c.text(&label, Rect::new(tx, ty_text, title_w, 19.0), &TextStyle::body().weight(Weight::Semibold));
    ui.c.text(line, Rect::new(tx, ty_text + 22.0, tw.min(THEME_LINE_W), 34.0), &TextStyle::footnote().color(palette::TEXT_DIM).wrap().top());
    if ring > 0.0 {
        let dot = Point::new(c.right() - 18.0, ty_text + 9.5);
        ui.c.with_alpha(ring, |cv| {
            cv.fill_circle(dot, 8.0, accent);
            cv.icon(Icon::Check, Rect::from_center(dot, 10.0, 10.0), palette::ON_ACCENT);
        });
    }
    resp.clicked
}

/// Independent notch type picker (Notch / Pill) with live preview of the current theme.
fn draw_notch_types(ui: &mut Ui, st: &mut AppState, r: Rect, view: Rect, sf: &Surfaces, m: &mut Motion) {
    ui.c.text("Notch type", Rect::new(r.x + 2.0, r.y, r.w, GROUP_TITLE_H - 6.0), &TextStyle::footnote().weight(Weight::Semibold).color(palette::TEXT_DIM));
    let accent = st.accent;
    let cur_theme = st.settings.notch_theme;
    let mut picked = None;
    for (i, (ty, name, line)) in NOTCH_TYPES.iter().enumerate() {
        let c = theme_card(r, i);
        let id = wid_n("settingswin.notch_type", i as u64);
        let locked = !ty.free() && !st.settings.pro;
        if draw_picker_card(ui, c, view, id, st.settings.notch_type == *ty, locked, name, line, *ty, cur_theme, accent, sf, m) {
            picked = Some(*ty);
        }
    }
    if let Some(ty) = picked.filter(|t| *t != st.settings.notch_type) {
        if !ty.free() && !st.settings.pro {
            st.notify(crate::icons::Icon::Lock, palette::YELLOW, "That notch type is Halo Pro", "Activate Halo Pro in Settings to use the floating Pill", 5.0, None);
        } else {
            st.settings.notch_type = ty;
            st.cmd(Command::SaveSettings);
        }
    }
}

/// Independent theme picker with live preview of the current notch type.
fn draw_themes(ui: &mut Ui, st: &mut AppState, r: Rect, view: Rect, sf: &Surfaces, m: &mut Motion) {
    ui.c.text("Theme", Rect::new(r.x + 2.0, r.y, r.w, GROUP_TITLE_H - 6.0), &TextStyle::footnote().weight(Weight::Semibold).color(palette::TEXT_DIM));
    let accent = st.accent;
    let cur_type = st.settings.notch_type;
    let mut picked = None;
    for (i, (theme, name, line)) in THEMES.iter().enumerate() {
        let c = theme_card(r, i);
        let id = wid_n("settingswin.theme", i as u64);
        let locked = !theme.free() && !st.settings.pro;
        if draw_picker_card(ui, c, view, id, st.settings.notch_theme == *theme, locked, name, line, cur_type, *theme, accent, sf, m) {
            picked = Some(*theme);
        }
    }
    if let Some(t) = picked.filter(|t| *t != st.settings.notch_theme) {
        if !t.free() && !st.settings.pro {
            st.notify(crate::icons::Icon::Lock, palette::YELLOW, "That theme is Halo Pro", "Activate Halo Pro in Settings to use Glass and Diorama", 5.0, None);
        } else {
            st.settings.notch_theme = t;
            st.cmd(Command::SaveSettings);
        }
    }
}

thread_local! {
    /// The Diorama card's picture and the scale (pixels per DIP) it was
    /// painted for: painted when the card first shows, kept while the
    /// window is open ([`View`]'s drop lets it go).
    static DIORAMA_KEPT: RefCell<Option<(f32, Image)>> = const { RefCell::new(None) };
}

/// The Diorama card's picture for a canvas of `scale` pixels per DIP: the
/// theme's resting pill at [`DIORAMA_MOMENT`], at its own size (a smaller
/// copy would smear its pixels and the time in its sky).
fn diorama_picture(scale: f32) -> studio_diorama::Picture {
    let (w, h) = (DIORAMA_CARD_W, studio_diorama::H);
    studio_diorama::render(&DIORAMA_MOMENT, &studio_diorama::View { w, h, rest_w: w, rest_h: h, scale })
}

/// That picture as an image, painted once for a scale and kept. With it
/// comes the painting it took the place of (the window moved to a screen of
/// another scale), for the canvas to let go of.
fn diorama_image(scale: f32) -> (Image, Option<Image>) {
    DIORAMA_KEPT.with(|kept| {
        let mut kept = kept.borrow_mut();
        if let Some((_, image)) = kept.as_ref().filter(|(s, _)| *s == scale) {
            return (image.clone(), None);
        }
        let p = diorama_picture(scale);
        let image = ImageData::new(p.w, p.h, p.bgra);
        let old = kept.replace((scale, image.clone())).map(|(_, old)| old);
        (image, old)
    })
}

/// Where the Diorama card's picture goes on its little `screen`: centred,
/// floating [`DIORAMA_FLOAT`] below the top edge, on whole pixels and
/// exactly as large as its `px` pixels are at `scale`. So it is drawn pixel
/// for pixel: `Canvas::image` fills its rect, and a rect of another shape
/// would crop the picture.
fn diorama_spot(screen: Rect, px: (u32, u32), scale: f32) -> Rect {
    let s = scale.max(0.25);
    let snap = |v: f32| (v * s).round() / s;
    let (w, h) = (px.0 as f32 / s, px.1 as f32 / s);
    Rect::new(snap(screen.center().x - w / 2.0), snap(screen.y + DIORAMA_FLOAT), w, h)
}

/// Live preview of any `(NotchType, NotchTheme)` combination on a small screen.
fn draw_combo_preview(c: &mut Canvas, ty: NotchType, theme: NotchTheme, screen: Rect) {
    c.clip(screen);
    c.fill_rrect(screen, 7.0, WALLPAPER);
    c.fill_rrect(Rect::new(screen.x, screen.y + screen.h * 0.55, screen.w, screen.h), 7.0, Color::hex(0x56679C));
    let cx = screen.center().x;
    let clock = TextStyle::caption().weight(Weight::Semibold).tabular().center();
    if ty == NotchType::Pill && theme == NotchTheme::Diorama {
        let scale = c.scale();
        let (image, old) = diorama_image(scale);
        if let Some(old) = old {
            c.forget_image(&old);
        }
        let body = diorama_spot(screen, (image.width, image.height), scale);
        c.fill_rrect(body.offset(0.0, 2.0), body.h / 2.0, Color::black(0.25));
        c.image_exact(&image, body, body.h / 2.0);
        c.unclip();
        return;
    }
    let (fill, border, ink) = match theme {
        NotchTheme::Classic => (palette::ISLAND, (ty == NotchType::Pill).then_some(Color::white(0.14)), palette::TEXT),
        NotchTheme::Halo => (Color::hex(0xFFF1DE), Some(Color::rgba(0.55, 0.30, 0.10, 0.25)), Color::hex(0x2B1B10)),
        NotchTheme::Glass => (Color::white(0.30), Some(Color::white(0.45)), palette::TEXT),
        NotchTheme::Diorama => (Color::hex(0xF8F3E7), Some(Color::hex(0xBFB49C)), Color::hex(0x14173A)),
    };
    let body = match ty {
        NotchType::Notch => {
            let b = Rect::new(cx - 34.0, screen.y, 68.0, 20.0);
            c.notch(b, 9.0, 5.0, fill, border);
            b
        }
        NotchType::Pill => {
            let b = Rect::new(cx - 36.0, screen.y + THEME_FLOAT, 72.0, 20.0);
            c.fill_rrect(b.offset(0.0, 2.0), 10.0, Color::black(0.25));
            c.fill_rrect(b, 10.0, fill);
            if let Some(bc) = border {
                c.stroke_rrect(b, 10.0, bc, 1.0);
            }
            b
        }
    };
    if theme == NotchTheme::Halo {
        for dx in [-6.0, 6.0] {
            c.fill_rrect(Rect::from_center(Point::new(cx + dx, body.center().y), 5.0, 8.0), 2.5, palette::ORANGE);
        }
    } else {
        c.text("10:08", body, &clock.color(ink));
    }
    c.unclip();
}

#[cfg(test)]
mod tests {
    use super::*;
    use halo_core::settings::Settings;
    use halo_core::store::Store;

    fn state() -> AppState {
        AppState::new(Settings::default(), Store::default())
    }

    fn rows(section: Section, st: &AppState) -> Vec<Item> {
        blocks(section, st)
            .into_iter()
            .flat_map(|b| match b {
                Block::Group(_, items) => items,
                _ => Vec::new(),
            })
            .collect()
    }

    fn row(section: Section, st: &AppState, key: Key) -> Option<Item> {
        rows(section, st).into_iter().find(|i| i.key == key)
    }

    #[test]
    fn every_setting_the_window_has_saves_through_the_app() {
        let mut st = state();
        for s in Section::ALL {
            for it in rows(s, &st) {
                st.cmds.clear();
                let act = match it.ctl {
                    Ctl::Switch(_) => Act::Toggle,
                    Ctl::Choice(..) => Act::Pick(0),
                    Ctl::Stepper(_) => Act::Step(1),
                    Ctl::Button(_) => Act::Press,
                };
                apply(&mut st, it.key, act);
                let saved = st.cmds.iter().any(|c| matches!(c, Command::SaveSettings));
                let opens = st.cmds.iter().any(|c| matches!(c, Command::OpenPath(_) | Command::OpenUrl(_) | Command::ShowWhatsNew));
                match it.key {
                    // Start with Windows waits for its reading; the buttons open things;
                    // GameStats without Pro notifies instead of toggling and getting reverted.
                    Key::Autostart => assert!(st.cmds.is_empty()),
                    Key::GameStats | Key::DockEnabled if !st.settings.pro => assert!(st.notice.is_some()),
                    Key::DataFolder | Key::ReleaseNotes | Key::LyricsCredit => assert!(opens, "{:?}", it.key),
                    _ => assert!(saved, "{:?} didn't save", it.key),
                }
            }
        }
    }

    /// The inventory and the rows the window draws are the same list: every
    /// row drawn in any section, in a state with everything shown (Pro,
    /// every add-on, the theme builder, notices, notifications), is in
    /// [`INVENTORY`] under its section, and every inventory item is drawn in
    /// that state or one of its simpler ones.
    #[test]
    fn the_inventory_is_every_row_the_window_draws() {
        let full = || {
            let mut s = Settings { pro: true, licensed: true, ..Settings::default() };
            for a in halo_core::addons::ALL {
                s.set_addon(a.id, true);
            }
            s.custom_look = Some(Default::default());
            s.usage.notices = true;
            let mut st = AppState::new(s.sanitized(), Store::default());
            st.settings.pro = true;
            st.notify_can = true;
            st.recorder = true;
            st.store.recorder_island = true;
            st.downloader_offered = true;
            for a in halo_core::addons::ALL {
                st.addons.insert(a.id, halo_core::addons::Status::Ready);
            }
            st.prayer.places = Some(std::sync::Arc::new(halo_core::prayer::PlaceList::parse(r#"{"places":[]}"#).unwrap()));
            st
        };
        let mut start_page = full();
        start_page.settings.remember_page = false;
        start_page.settings.tab_order = vec!["focus".into(), "media".into()];
        let mut no_widgets = full();
        no_widgets.settings.widgets.clear();
        let states = [full(), start_page, no_widgets, state()];
        let mut drawn: Vec<(Section, Key)> = Vec::new();
        for st in &states {
            for s in Section::ALL {
                for it in rows(s, st) {
                    let k = (s, item_key(it.key));
                    if !drawn.contains(&k) {
                        drawn.push(k);
                    }
                }
            }
        }
        let listed: Vec<(Section, Key)> = INVENTORY.iter().map(|i| (i.0, i.2)).collect();
        for d in &drawn {
            // Windows draws the game and display rows; another OS may not.
            assert!(listed.contains(d), "{d:?} is drawn but not in the inventory");
        }
        for (s, label, k) in INVENTORY {
            let here = cfg!(windows) || !matches!(k, Key::Display | Key::Mascot | Key::GameStats | Key::GamePeek | Key::DockEnabled | Key::Tab(PageKind::Games));
            assert!(!here || drawn.contains(&(*s, *k)), "{label} is in the inventory but never drawn");
            // Its words are the row's own label (the slots are one item).
            match states.iter().find_map(|st| rows(*s, st).into_iter().find(|i| i.key == *k)) {
                Some(it) if *k == Key::Widget(0) => assert_eq!(it.label, "Slot 1"),
                Some(it) => assert_eq!(it.label, *label, "{k:?}"),
                None => {}
            }
        }
    }

    #[test]
    fn a_license_servers_code_is_told_from_the_built_in_key() {
        assert!(is_license_code("HALO-ABCDE-FGHJK-MNPQR-STUVW"));
        assert!(is_license_code(" halo-abcde-fghjk-mnpqr-stuvw "));
        // The built-in key's shape, a code with a 0 or an O, a group short: not a server code.
        for s in ["HALO-2026-KOLIJI-9F4C-77B1", "HALO-ABCD0-FGHJK-MNPQR-STUVW", "HALO-ABCDO-FGHJK-MNPQR-STUVW", "HALO-ABCD-FGHJK-MNPQR-STUVW", ""] {
            assert!(!is_license_code(s), "{s}");
        }
        // Settings can't turn a license on by themselves.
        let s: Settings = serde_json::from_str(r#"{"licensed":true,"pro":true}"#).unwrap();
        assert!(!s.sanitized().pro);
        assert!(Settings { licensed: true, ..Settings::default() }.sanitized().pro);
    }

    #[test]
    fn the_news_source_steps_through_the_presets_and_starts_over() {
        let mut st = state();
        assert_eq!(row(Section::General, &st, Key::NewsFeed).map(|i| i.ctl), Some(Ctl::Stepper("BBC World".into())));
        st.news.asked();
        apply(&mut st, Key::NewsFeed, Act::Step(1));
        assert_eq!(st.settings.news_feed, "CNA");
        assert!(st.cmds.contains(&Command::SaveSettings));
        assert!(!st.news.asking, "another source starts over");
        apply(&mut st, Key::NewsFeed, Act::Step(-1));
        apply(&mut st, Key::NewsFeed, Act::Step(-1));
        assert_eq!(st.settings.news_feed, "Bernama", "wraps around");
        // An address of its own (typed into settings.json) shows as such, and a step goes to the first preset.
        st.settings.news_feed = "https://example.org/feed.xml".into();
        assert_eq!(row(Section::General, &st, Key::NewsFeed).map(|i| i.ctl), Some(Ctl::Stepper("Your own feed".into())));
        apply(&mut st, Key::NewsFeed, Act::Step(-1));
        assert_eq!(st.settings.news_feed, "BBC World");
    }

    /// Add fills every slot the copy promises, each one saved and shown on
    /// the Widgets tab, then goes; never a slot nothing can fill.
    #[test]
    fn add_fills_every_promised_widget_slot_and_then_goes() {
        use halo_core::halo::{widget_slots, Widget};
        // Without the Widgets add-on there are no slots, water or clipboard rows.
        let mut st = state();
        assert!(row(Section::General, &st, Key::AddWidget).is_none() && row(Section::General, &st, Key::Clips).is_none());
        st.addons.insert(halo_core::addons::WIDGETS.id, halo_core::addons::Status::Ready);
        assert!(row(Section::General, &st, Key::Clips).is_some());
        for pro in [false, true] {
            let mut st = state();
            st.addons.insert(halo_core::addons::WIDGETS.id, halo_core::addons::Status::Ready);
            st.settings.pro = pro;
            st.settings.widgets.clear();
            while row(Section::General, &st, Key::AddWidget).is_some() {
                let before = st.settings.widgets.len();
                apply(&mut st, Key::AddWidget, Act::Press);
                assert_eq!(st.settings.widgets.len(), before + 1, "Add did nothing at {before} (pro {pro})");
            }
            assert_eq!(st.settings.widgets.len(), widget_slots(pro), "pro {pro}");
            let slots = rows(Section::General, &st).into_iter().filter(|i| matches!(i.key, Key::Widget(_))).count();
            assert_eq!(slots, widget_slots(pro));
            // Saved and read back as is.
            let back: Settings = serde_json::from_str(&serde_json::to_string(&st.settings).unwrap()).unwrap();
            assert_eq!(back.widgets, st.settings.widgets);
        }
        let mut st = state();
        st.addons.insert(halo_core::addons::WIDGETS.id, halo_core::addons::Status::Ready);
        st.settings.pro = true;
        st.settings.widgets = Widget::ALL[..4].to_vec();
        assert_eq!(row(Section::General, &st, Key::AddWidget).and_then(|i| i.sub), Some("Up to 5, one of each widget".into()));
    }

    #[test]
    fn prayer_times_follow_jakim() {
        let mut st = state();
        let row = |st: &AppState| blocks(Section::Prayer, st).into_iter().find_map(|b| match b {
            Block::Group(_, items) => items.into_iter().find(|i| i.key == Key::PrayerCalendar),
            _ => None,
        });
        let r = row(&st).unwrap();
        assert_eq!((r.ctl, r.sub.as_deref()), (Ctl::Choice(vec!["Malaysia"], Some(0)), Some("Jabatan Kemajuan Islam Malaysia (JAKIM)")));
        apply(&mut st, Key::PrayerCalendar, Act::Pick(1));
        assert_eq!(st.settings.prayer.follow, PrayerFollow::Jakim);
        apply(&mut st, Key::PrayerCalendar, Act::Pick(0));
        assert_eq!(st.settings.prayer.follow, PrayerFollow::Jakim);
    }

    #[test]
    fn steppers_stay_in_range() {
        let mut st = state();
        for _ in 0..100 {
            apply(&mut st, Key::Width, Act::Step(1));
            apply(&mut st, Key::FocusMin, Act::Step(1));
            apply(&mut st, Key::LongEvery, Act::Step(1));
            apply(&mut st, Key::AiNear, Act::Step(1));
            apply(&mut st, Key::LyricsOffset, Act::Step(1));
        }
        let s = &st.settings;
        assert_eq!((s.expanded_width, s.pomodoro.focus_min, s.pomodoro.long_every, s.usage.near_pct, s.lyrics_offset_ms), (900.0, 180, 12, 95, -5000));
        for _ in 0..100 {
            apply(&mut st, Key::Width, Act::Step(-1));
            apply(&mut st, Key::FocusMin, Act::Step(-1));
            apply(&mut st, Key::PrayerCountdown, Act::Step(-1));
        }
        let s = &st.settings;
        assert_eq!((s.expanded_width, s.pomodoro.focus_min, s.prayer.remind_before_min), (480.0, 1, 0));
        // Minutes go by five, and by one below five.
        assert_eq!((step_min(25, -1, 1, 180), step_min(5, -1, 1, 180), step_min(3, 1, 1, 180), step_min(5, 1, 1, 180)), (20, 4, 4, 10));
    }

    #[test]
    fn delays_show_the_nearest_choice() {
        let mut st = state();
        let pick = |st: &AppState, k| match row(Section::Island, st, k).map(|i| i.ctl) {
            Some(Ctl::Choice(_, sel)) => sel,
            _ => None,
        };
        assert_eq!((pick(&st, Key::HoverDelay), pick(&st, Key::CloseDelay)), (Some(1), Some(0)));
        st.settings.hover_delay_ms = 2000;
        assert_eq!(pick(&st, Key::HoverDelay), Some(2));
        apply(&mut st, Key::HoverDelay, Act::Pick(0));
        assert_eq!(st.settings.hover_delay_ms, 120);
        // A pick past the list changes nothing.
        st.cmds.clear();
        apply(&mut st, Key::CloseDelay, Act::Pick(9));
        assert!(st.cmds.is_empty());
    }

    #[test]
    fn tabs_flip_and_the_start_page_cycles_through_the_tabs() {
        let mut st = state();
        apply(&mut st, Key::Tab(PageKind::Shelf), Act::Toggle);
        assert!(st.tab_hidden(PageKind::Shelf));
        apply(&mut st, Key::Tab(PageKind::Shelf), Act::Toggle);
        assert!(!st.tab_hidden(PageKind::Shelf));
        // Opens on shows only while the island doesn't open where it was left.
        assert!(row(Section::Tabs, &st, Key::StartPage).is_none());
        apply(&mut st, Key::RememberPage, Act::Toggle);
        assert!(row(Section::Tabs, &st, Key::StartPage).is_some());
        let pages = start_pages(&st);
        apply(&mut st, Key::StartPage, Act::Step(1));
        assert_eq!(st.settings.start_page, pages[1].key());
        apply(&mut st, Key::StartPage, Act::Step(-1));
        apply(&mut st, Key::StartPage, Act::Step(-1));
        assert_eq!(st.settings.start_page, pages[pages.len() - 1].key());
        assert!(!pages.contains(&PageKind::Addons) && !pages.contains(&PageKind::Settings));
    }

    #[test]
    fn notifications_switches_off_under_tabs_only_where_the_copy_can_read_them() {
        let mut st = state();
        let listed = |st: &AppState| row(Section::Tabs, st, Key::Tab(PageKind::Notifications)).is_some();
        assert!(!listed(&st), "the download has no Notifications");
        st.notify_can = true;
        assert!(listed(&st) && st.notifications_on(), "on from the start in the Store copy");
        apply(&mut st, Key::Tab(PageKind::Notifications), Act::Toggle);
        assert!(!st.notifications_on() && st.cmds.iter().any(|c| matches!(c, Command::SaveSettings)));
        apply(&mut st, Key::Tab(PageKind::Notifications), Act::Toggle);
        assert!(st.notifications_on());
    }

    #[test]
    fn tab_order_persists_preserves_order_when_hidden_and_resets_to_default() {
        let mut st = state();
        let default_order = crate::pages::tabs(&st);
        let orderable = crate::pages::orderable_tabs(&st);
        // Reorder Focus to the front.
        crate::pages::reorder_tab(&mut st, &orderable, PageKind::Focus, PageKind::Media);
        assert_eq!(st.settings.tab_order.first().map(String::as_str), Some("focus"));
        // Persists across settings JSON round-trip (app restart).
        let json = serde_json::to_string(&st.settings).unwrap();
        let loaded: Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.tab_order, st.settings.tab_order);
        let mut restarted = AppState::new(loaded, Store::default());
        assert_eq!(crate::pages::tabs(&restarted)[0], PageKind::Focus);
        // Hiding a tab keeps all remaining tabs in the user's order.
        apply(&mut restarted, Key::Tab(PageKind::Today), Act::Toggle);
        let after_hide = crate::pages::tabs(&restarted);
        assert!(!after_hide.contains(&PageKind::Today));
        assert_eq!(after_hide[..4], [PageKind::Focus, PageKind::Media, PageKind::Shelf, PageKind::Tasks]);
        // Reset-to-default option exists and restores default order.
        assert!(row(Section::Tabs, &restarted, Key::ResetTabOrder).is_some());
        apply(&mut restarted, Key::Tab(PageKind::Today), Act::Toggle);
        apply(&mut restarted, Key::ResetTabOrder, Act::Press);
        assert!(restarted.settings.tab_order.is_empty());
        assert_eq!(crate::pages::tabs(&restarted), default_order);
    }

    #[test]
    fn prayer_has_a_section_only_while_its_add_on_is_on() {
        let st = state();
        assert!(!sections(&st).contains(&Section::Prayer));
        assert_eq!(sections(&st).first(), Some(&Section::General));
        assert_eq!(sections(&st).last(), Some(&Section::About));
    }

    /// Caffeine switched on, as its switch on the Add-ons page leaves it.
    fn with_caffeine() -> AppState {
        let mut s = Settings::default();
        s.set_addon(halo_core::addons::CAFFEINE.id, true);
        AppState::new(s, Store::default())
    }

    #[test]
    fn caffeine_has_a_section_only_while_its_add_on_is_on() {
        assert!(!sections(&state()).contains(&Section::Caffeine));
        let mut st = with_caffeine();
        let list = sections(&st);
        // After Focus (and Prayer times while that is on), with its cup.
        let at = list.iter().position(|s| *s == Section::Caffeine).expect("its section");
        assert_eq!((list[at - 1], list.last()), (Section::Focus, Some(&Section::About)));
        assert_eq!((Section::Caffeine.title(), Section::Caffeine.key(), Section::Caffeine.icon()), ("Caffeine", "caffeine", Icon::Coffee));
        // Switched off, it goes from the sidebar (an open window shows General).
        st.addons.insert(halo_core::addons::CAFFEINE.id, halo_core::addons::Status::Off);
        assert!(!sections(&st).contains(&Section::Caffeine));
    }

    #[test]
    fn caffeine_rows_apply_and_stay_in_range() {
        let mut st = with_caffeine();
        let steppers = [Key::CaffeineBedtime, Key::CaffeineHalfLife, Key::CaffeineBedLimit, Key::CaffeineDayLimit];
        let shown = |st: &AppState| -> Vec<String> {
            steppers
                .into_iter()
                .map(|k| match row(Section::Caffeine, st, k).map(|i| i.ctl) {
                    Some(Ctl::Stepper(v)) => v,
                    other => panic!("{k:?}: {other:?}"),
                })
                .collect()
        };
        assert_eq!(shown(&st), ["23:00", "5 h", "35 mg", "400 mg"]);
        // A step up: a quarter hour, the next half-life, 5 mg and 50 mg. Each saves.
        for k in steppers {
            st.cmds.clear();
            apply(&mut st, k, Act::Step(1));
            assert!(st.cmds.iter().any(|c| matches!(c, Command::SaveSettings)), "{k:?}");
        }
        assert_eq!(shown(&st), ["23:15", "5.5 h", "40 mg", "450 mg"]);
        // To their ends and no further.
        for _ in 0..100 {
            steppers.into_iter().for_each(|k| apply(&mut st, k, Act::Step(1)));
        }
        assert_eq!(shown(&st), ["03:00", "15 h", "80 mg", "1000 mg"]);
        let c = &st.settings.caffeine;
        assert_eq!((c.bedtime, c.half_life, c.bed_limit_mg, c.day_limit_mg), (3 * 60, 15.0, 80, 1000));
        for _ in 0..100 {
            steppers.into_iter().for_each(|k| apply(&mut st, k, Act::Step(-1)));
        }
        assert_eq!(shown(&st), ["20:00", "2 h", "10 mg", "100 mg"]);
        // Bedtime goes past midnight and back, kept as minutes after midnight.
        st.settings.caffeine.bedtime = 23 * 60 + 45;
        apply(&mut st, Key::CaffeineBedtime, Act::Step(1));
        assert_eq!((st.settings.caffeine.bedtime, shown(&st)[0].as_str()), (0, "00:00"));
        apply(&mut st, Key::CaffeineBedtime, Act::Step(1));
        assert_eq!(st.settings.caffeine.bedtime, 15);
        apply(&mut st, Key::CaffeineBedtime, Act::Step(-1));
        apply(&mut st, Key::CaffeineBedtime, Act::Step(-1));
        assert_eq!(st.settings.caffeine.bedtime, 23 * 60 + 45);
        // A value typed into the file goes to the next step that way, or
        // back into the rows' range.
        for (d, to) in [(1, 23 * 60 + 15), (-1, 23 * 60)] {
            st.settings.caffeine.bedtime = 23 * 60 + 7;
            apply(&mut st, Key::CaffeineBedtime, Act::Step(d));
            assert_eq!(st.settings.caffeine.bedtime, to);
        }
        st.settings.caffeine.bedtime = 12 * 60;
        apply(&mut st, Key::CaffeineBedtime, Act::Step(-1));
        assert_eq!(st.settings.caffeine.bedtime, 20 * 60);
        st.settings.caffeine.half_life = 5.2;
        apply(&mut st, Key::CaffeineHalfLife, Act::Step(1));
        assert_eq!(st.settings.caffeine.half_life, 5.5);
        st.settings.caffeine.half_life = f32::NAN;
        apply(&mut st, Key::CaffeineHalfLife, Act::Step(-1));
        assert_eq!(st.settings.caffeine.half_life, 4.5);
        st.settings.caffeine.bed_limit_mg = 37;
        apply(&mut st, Key::CaffeineBedLimit, Act::Step(-1));
        assert_eq!(st.settings.caffeine.bed_limit_mg, 35);
        st.settings.caffeine.day_limit_mg = 5000;
        apply(&mut st, Key::CaffeineDayLimit, Act::Step(-1));
        assert_eq!(st.settings.caffeine.day_limit_mg, 1000);
        // What the rows set is what the add-on uses as it is.
        assert_eq!(st.settings.caffeine.sane(), st.settings.caffeine);
        // The reminder's switch.
        let remind = |st: &AppState| row(Section::Caffeine, st, Key::CaffeineRemind).map(|i| i.ctl);
        assert_eq!(remind(&st), Some(Ctl::Switch(Some(true))));
        apply(&mut st, Key::CaffeineRemind, Act::Toggle);
        assert!(!st.settings.caffeine.remind && st.cmds.iter().any(|c| matches!(c, Command::SaveSettings)));
        assert_eq!(remind(&st), Some(Ctl::Switch(Some(false))));
        apply(&mut st, Key::CaffeineRemind, Act::Toggle);
        assert!(st.settings.caffeine.remind);
    }

    #[test]
    fn the_sidebar_has_room_for_every_section_at_full_height() {
        // Every section but About is a row from the top, About sits 12 DIPs
        // above the card's bottom (`draw_sidebar`), with Prayer times,
        // Caffeine and AI usage all shown. The Mac's window buttons take
        // `TITLEBAR` (28) of its 600.
        let rows = (Section::ALL.len() - 1) as f32;
        let last = NAV_Y + rows * (NAV_H + NAV_GAP) - NAV_GAP;
        assert!(last + NAV_GAP <= CARD_H - 12.0 - NAV_H, "Windows: {last}");
        assert!(28.0 + last + NAV_GAP <= 600.0 - 12.0 - NAV_H, "Mac: {last}");
    }

    #[test]
    fn caffeine_fits_the_window_at_its_full_height_without_scrolling() {
        let st = with_caffeine();
        let content: f32 = blocks(Section::Caffeine, &st).iter().map(|b| block_h(&st, b) + GROUP_GAP).sum();
        assert!(content + 16.0 <= CARD_H - CONTENT_Y - 1.0, "{content}");
        // Its help lines say the half-life's usual range and the day's limits.
        let sub = |k| row(Section::Caffeine, &st, k).and_then(|i| i.sub).unwrap_or_default();
        assert_eq!(sub(Key::CaffeineHalfLife), "Most adults: 2 to 10 hours");
        assert_eq!(sub(Key::CaffeineDayLimit), "400 mg a day for healthy adults, 200 mg in pregnancy");
        assert_eq!(sub(Key::CaffeineRemind), format!("On the island, {} minutes before", caffeine::REMIND_BEFORE));
    }

    #[test]
    fn halo_mascot_switch_flips_and_saves() {
        let mut st = state();
        let on = |st: &AppState| row(Section::Appearance, st, Key::Mascot).map(|i| i.ctl);
        assert_eq!(on(&st), Some(Ctl::Switch(Some(true))));
        apply(&mut st, Key::Mascot, Act::Toggle);
        assert!(!st.settings.mascot && st.cmds.iter().any(|c| matches!(c, Command::SaveSettings)));
        assert_eq!(on(&st), Some(Ctl::Switch(Some(false))));
    }

    #[test]
    fn idle_looks_follow_the_theme() {
        let mut st = state();
        let looks = |st: &AppState| match row(Section::Appearance, st, Key::IdleLook).map(|i| i.ctl) {
            Some(Ctl::Choice(names, _)) => names,
            _ => Vec::new(),
        };
        assert!(!looks(&st).contains(&"Status"));
        st.settings.notch_type = NotchType::Pill;
        assert!(looks(&st).contains(&"Status"));
        st.settings.notch_type = NotchType::Notch;
        st.settings.notch_theme = NotchTheme::Halo;
        assert!(looks(&st).contains(&"Halo"));
        // Pill + Diorama rests as its picture: with the time, alone, or hidden.
        st.settings.notch_type = NotchType::Pill;
        st.settings.notch_theme = NotchTheme::Diorama;
        st.settings.idle_style = IdleStyle::Status;
        assert_eq!(looks(&st), ["Clock", "Minimal", "Hidden"]);
        assert_eq!(row(Section::Appearance, &st, Key::IdleLook).map(|i| i.ctl), Some(Ctl::Choice(looks(&st), Some(0))));
        st.settings.idle_style = IdleStyle::Eyes;
        assert_eq!(row(Section::Appearance, &st, Key::IdleLook).map(|i| i.ctl), Some(Ctl::Choice(looks(&st), Some(1))));
        // A pick there is one of its three.
        apply(&mut st, Key::IdleLook, Act::Pick(2));
        assert_eq!(st.settings.idle_style, IdleStyle::Hidden);
        apply(&mut st, Key::IdleLook, Act::Pick(1));
        assert_eq!(st.settings.idle_style, IdleStyle::Minimal);
        st.cmds.clear();
        apply(&mut st, Key::IdleLook, Act::Pick(3));
        assert!(st.cmds.is_empty() && st.settings.idle_style == IdleStyle::Minimal);
    }

    fn place(t: NotchTheme) -> usize {
        match t {
            NotchTheme::Classic => 0,
            NotchTheme::Halo => 1,
            NotchTheme::Glass => 2,
            NotchTheme::Diorama => 3,
        }
    }

    #[test]
    fn every_theme_has_a_card_in_the_pickers_order() {
        assert_eq!(NOTCH_TYPES.map(|t| t.1), ["Notch", "Pill"]);
        assert_eq!(THEMES.map(|t| t.1), ["Classic", "Halo", "Glass", "Diorama"]);
        for (i, (theme, name, line)) in THEMES.iter().enumerate() {
            assert_eq!(place(*theme), i, "{name}");
            assert!(!line.is_empty(), "{name} says what it is");
        }
    }

    #[test]
    fn the_theme_cards_sit_two_to_a_row_as_wide_as_the_content() {
        let r = Rect::new(SIDEBAR_W + PAD, CONTENT_Y, CARD_W - SIDEBAR_W - 2.0 * PAD, block_h(&state(), &Block::Themes));
        let cards: Vec<Rect> = (0..THEMES.len()).map(|i| theme_card(r, i)).collect();
        assert_eq!(cards.iter().filter(|c| c.y == cards[0].y).count(), 2);
        assert_eq!(cards.iter().filter(|c| c.y == cards[2].y).count(), 2);
        assert_eq!(r.h, GROUP_TITLE_H + 2.0 * THEME_CARD_H + THEME_GAP);
        for (i, c) in cards.iter().enumerate() {
            assert_eq!((c.w, c.h), (276.0, THEME_CARD_H), "card {i}");
            assert!(c.x >= r.x && c.right() <= r.right() && c.y >= r.y + GROUP_TITLE_H && c.bottom() <= r.bottom(), "card {i} leaves the block: {c:?}");
            for (j, d) in cards[..i].iter().enumerate() {
                let apart = c.x >= d.right() + THEME_GAP || d.x >= c.right() + THEME_GAP || c.y >= d.bottom() + THEME_GAP || d.y >= c.bottom() + THEME_GAP;
                assert!(apart, "cards {j} and {i} touch");
            }
        }
        assert_eq!((cards[0].x, cards[1].right(), cards[2].x), (r.x, r.right(), r.x));
        assert!(THEME_PREVIEW_W >= DIORAMA_CARD_W + 16.0);
    }

    #[test]
    fn appearance_fits_the_window_at_its_full_height_without_scrolling() {
        let mut st = state();
        for (ty, _, _) in NOTCH_TYPES {
            for (theme, _, _) in THEMES {
                st.settings.notch_type = ty;
                st.settings.notch_theme = theme;
                let content: f32 = blocks(Section::Appearance, &st).iter().map(|b| block_h(&st, b) + GROUP_GAP).sum();
                assert!(content + 16.0 <= CARD_H - CONTENT_Y - 1.0, "{ty:?} x {theme:?}: {content}");
            }
        }
    }

    #[test]
    fn the_diorama_card_shows_the_themes_picture_whole_at_its_own_size() {
        let screen = Rect::new(282.0, 104.0, 160.0, THEME_PREVIEW_H);
        for scale in [1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0] {
            let p = diorama_picture(scale);
            // The card's pane, 120 x 38 DIP, to the pixel.
            assert_eq!(p.w as f32, DIORAMA_CARD_W * scale, "{scale}");
            assert!((p.h as f32 - studio_diorama::H * scale).abs() <= 0.5, "{scale}: {} px tall", p.h);
            assert_eq!(p.bgra.len(), (p.w * p.h * 4) as usize, "{scale}");
            // Its place has the picture's own shape (so nothing of it is
            // cropped or stretched) on whole pixels, inside the screen.
            let at = diorama_spot(screen, (p.w, p.h), scale);
            assert_eq!(((at.w * scale).round(), (at.h * scale).round()), (p.w as f32, p.h as f32), "{scale}: {at:?}");
            assert!((at.w / at.h - p.w as f32 / p.h as f32).abs() < 1e-4, "{scale}: {at:?}");
            for v in [at.x, at.y] {
                assert!((v * scale - (v * scale).round()).abs() < 1e-3, "{scale}: {at:?}");
            }
            assert!((at.center().x - screen.center().x).abs() <= 0.5 / scale + 1e-3, "{scale}: {at:?}");
            assert!((at.y - (screen.y + DIORAMA_FLOAT)).abs() <= 0.5 / scale + 1e-3 && at.bottom() + 2.0 < screen.bottom(), "{scale}: {at:?}");
            // Room beside it on the card's screen, as beside Pill's capsule.
            assert!(at.x - screen.x >= 16.0 && screen.right() - at.right() >= 16.0, "{scale}: {at:?}");
        }
        // A morning: the sun is up and the glass ball rests on the house.
        assert!(!DIORAMA_MOMENT.night() && DIORAMA_MOMENT.lens == DIORAMA_MOMENT.rest_lens());
        assert_eq!(DIORAMA_MOMENT.clock.map(|c| (c.hour, c.minute)), Some((10, 8)));
        assert_eq!(DIORAMA_MOMENT.minute, 10.0 * 60.0 + 8.0);
    }

    #[test]
    fn the_diorama_cards_picture_is_painted_once_and_goes_with_the_window() {
        let view = View::new(Section::Appearance);
        let (first, replaced) = diorama_image(2.0);
        assert!(replaced.is_none());
        assert_eq!((first.width, first.height), (240, 76));
        // Drawn again, it is the same painting.
        let (again, replaced) = diorama_image(2.0);
        assert!(std::sync::Arc::ptr_eq(&first, &again) && replaced.is_none());
        // On a screen of another scale it is painted for that one, and the
        // canvas gets the old painting to let go of.
        let (small, replaced) = diorama_image(1.0);
        assert_eq!((small.width, small.height), (120, 38));
        assert!(replaced.is_some_and(|old| std::sync::Arc::ptr_eq(&old, &first)));
        // The window closes: nothing of it is kept.
        drop(view);
        assert!(DIORAMA_KEPT.with(|kept| kept.borrow().is_none()));
    }

    #[test]
    fn motion_glides_without_overshoot_and_stops() {
        let mut m = Motion::default();
        // Asked for the first time, a value starts where it's going: the
        // window opens complete.
        m.begin(1.0, false);
        assert_eq!(m.slide(7, 100.0, SLIDE), 100.0);
        assert!(!m.moving);
        // Sent elsewhere, it moves a little every frame at 144 Hz, never
        // past where it's going, and stops there.
        let (mut t, mut last, mut frames) = (1.0, 100.0f32, 0);
        loop {
            t += 1.0 / 144.0;
            m.begin(t, false);
            let x = m.slide(7, 0.0, SLIDE);
            assert!(x <= last + 1e-4 && x >= 0.0, "frame {frames}: {x} after {last}");
            last = x;
            frames += 1;
            if !m.moving {
                break;
            }
            assert!(frames < 144, "still moving after a second");
        }
        assert_eq!(last, 0.0);
        // About a third of a second, then no more frames.
        assert!((30..60).contains(&frames), "{frames} frames");
        m.begin(t + 1.0 / 144.0, false);
        assert_eq!(m.slide(7, 0.0, SLIDE), 0.0);
        assert!(!m.moving);
        // A switch's knob arrives sooner; a hover wash sooner still.
        let count = |pace: f32| {
            let mut m = Motion::default();
            m.begin(1.0, false);
            m.fade(1, 0.0, pace);
            let (mut t, mut n) = (1.0, 0);
            loop {
                t += 1.0 / 144.0;
                m.begin(t, false);
                m.fade(1, 1.0, pace);
                n += 1;
                if !m.moving || n > 300 {
                    return n;
                }
            }
        };
        assert!(count(WASH) < count(SWITCHING) && count(SWITCHING) <= 48, "wash {} switch {}", count(WASH), count(SWITCHING));
        // Less motion (or a picture): everything lands at once.
        m.begin(t + 1.0, true);
        assert_eq!(m.slide(7, 50.0, SLIDE), 50.0);
        assert!(!m.moving);
        // Out of rest, the first step is about one refresh, however long the
        // window was still: no jump as it starts.
        let mut m = Motion::default();
        m.begin(1.0, false);
        m.slide(9, 0.0, SLIDE);
        m.begin(10.0, false);
        let first = m.slide(9, 100.0, SLIDE);
        assert!(first > 0.0 && first < 3.0, "first step {first}");
        // A long pause on the way doesn't jump either: a step is at most a 30th of a second.
        m.begin(20.0, false);
        let next = m.slide(9, 100.0, SLIDE);
        assert!(next < 40.0, "{first} then {next}");
        // Another section opens: the switch in a row's place starts as it is
        // (no slide from the last section's), while the sidebar's highlight
        // glides on.
        let mut m = Motion::default();
        m.begin(1.0, false);
        m.slide(1, 0.0, SLIDE);
        m.content = true;
        m.fade(2, 1.0, SWITCHING);
        m.begin(1.0 + 1.0 / 144.0, false);
        m.leave_section();
        let highlight = m.slide(1, 100.0, SLIDE);
        m.content = true;
        assert_eq!(m.fade(2, 0.0, SWITCHING), 0.0);
        assert!(highlight > 0.0 && highlight < 100.0 && m.moving, "{highlight}");
    }

    #[test]
    fn lyrics_timing_reads_as_earlier_or_later() {
        assert_eq!(offset_text(0), "On time");
        assert_eq!(offset_text(300), "−0.3 s");
        assert_eq!(offset_text(-1200), "+1.2 s");
    }

    #[test]
    fn search_finds_settings_across_sections_and_popups_preview() {
        let mut st = state();
        let (found, secs) = search_blocks(&st, "brightness");
        assert_eq!(secs, vec![Section::Popups]);
        assert_eq!(found.len(), 1);
        let (_, secs_mic) = search_blocks(&st, "microphone");
        assert_eq!(secs_mic, vec![Section::Popups]);
        apply(&mut st, Key::HudBrightness, Act::Preview);
        assert!(matches!(st.cmds.last(), Some(Command::PreviewHud(crate::state::HudKind::Brightness { .. }))));
        apply(&mut st, Key::HudMic, Act::Preview);
        assert!(matches!(st.cmds.last(), Some(Command::PreviewHud(crate::state::HudKind::Mic { muted: true }))));
    }
}

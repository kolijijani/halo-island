//! Halo Island's own parts, the pure half (the Windows half is in the app):
//! what is Free and what is Pro, the Widgets tab's slots, the clipboard's
//! list, the day's glasses of water, the moon's phase, world clocks, the
//! theme builder's colours, and which copy of a synced file wins.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------- Free / Pro

/// Clipboard items kept: 5 free, many more with Pro (in memory only).
/// ponytail: Pro's "unlimited" is 500 in memory, gone at quit; a file in the
/// data folder when people want it across restarts.
pub const CLIPS_FREE: usize = 5;
pub const CLIPS_PRO: usize = 500;

pub fn clip_cap(pro: bool) -> usize {
    if pro {
        CLIPS_PRO
    } else {
        CLIPS_FREE
    }
}

/// Widgets tab slots: 2 free, 6 with Pro.
pub fn widget_slots(pro: bool) -> usize {
    if pro {
        6
    } else {
        2
    }
}

// ---------------------------------------------------------------- widgets

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Widget {
    Weather,
    Calendar,
    /// Pro: London, New York, Tokyo beside the local time.
    WorldClock,
    /// Pro: tonight's moon.
    Moon,
    /// Pro: days to `Settings::countdown_day`.
    Countdown,
}

impl Widget {
    pub const ALL: [Widget; 5] = [Widget::Weather, Widget::Calendar, Widget::WorldClock, Widget::Moon, Widget::Countdown];

    pub fn pro(self) -> bool {
        matches!(self, Widget::WorldClock | Widget::Moon | Widget::Countdown)
    }

    pub fn title(self) -> &'static str {
        match self {
            Widget::Weather => "Weather",
            Widget::Calendar => "Calendar",
            Widget::WorldClock => "World clock",
            Widget::Moon => "Moon",
            Widget::Countdown => "Countdown",
        }
    }
}

// ---------------------------------------------------------------- weather

/// The Weather widget's reading, from Open-Meteo (no key, no account).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Weather {
    pub temp_c: f32,
    /// WMO weather code.
    pub code: u16,
}

/// Readings older than this are read again when the Widgets tab opens.
pub const WEATHER_FRESH_S: i64 = 30 * 60;

pub fn weather_url(lat: f64, lng: f64) -> String {
    format!("https://api.open-meteo.com/v1/forecast?latitude={lat:.2}&longitude={lng:.2}&current=temperature_2m,weather_code")
}

pub fn parse_weather(body: &str) -> Option<Weather> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let c = v.get("current")?;
    Some(Weather { temp_c: c.get("temperature_2m")?.as_f64()? as f32, code: c.get("weather_code")?.as_u64()? as u16 })
}

/// WMO code in a word or two.
pub fn weather_words(code: u16) -> &'static str {
    match code {
        0 => "Clear",
        1..=3 => "Cloudy",
        45 | 48 => "Fog",
        51..=67 | 80..=82 => "Rain",
        71..=77 | 85 | 86 => "Snow",
        95..=99 => "Thunder",
        _ => "—",
    }
}

// ---------------------------------------------------------------- clipboard

/// What was copied, newest first, without repeats.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clips {
    pub items: Vec<String>,
}

impl Clips {
    /// Largest item kept, in chars: a copied book stays out.
    pub const MAX_CHARS: usize = 4000;

    /// Keep `text` first; returns whether the list changed.
    pub fn push(&mut self, text: &str, cap: usize) -> bool {
        let t = text.trim();
        if t.is_empty() || self.items.first().is_some_and(|f| f == t) {
            return false;
        }
        let t: String = t.chars().take(Self::MAX_CHARS).collect();
        self.items.retain(|i| *i != t);
        self.items.insert(0, t);
        self.items.truncate(cap.max(1));
        true
    }

    /// Items containing `q` (any case); everything for an empty `q`.
    pub fn find(&self, q: &str) -> Vec<&str> {
        let q = q.trim().to_lowercase();
        self.items.iter().filter(|i| q.is_empty() || i.to_lowercase().contains(&q)).map(String::as_str).collect()
    }
}

// ---------------------------------------------------------------- water

/// Glasses drunk, by day key ("2026-11-11"); the last 14 days are kept.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Water {
    pub days: BTreeMap<String, u8>,
}

impl Water {
    pub fn today(&self, day: &str) -> u8 {
        self.days.get(day).copied().unwrap_or(0)
    }

    pub fn add(&mut self, day: &str, delta: i8) {
        let n = self.days.entry(day.to_string()).or_insert(0);
        *n = (*n as i16 + delta as i16).clamp(0, 99) as u8;
        while self.days.len() > 14 {
            let first = self.days.keys().next().cloned();
            first.map(|k| self.days.remove(&k));
        }
    }
}

/// "5/8 glasses".
pub fn fmt_glasses(n: u8, goal: u8) -> String {
    format!("{n}/{goal} glasses")
}

// ---------------------------------------------------------------- moon

/// Days in a synodic month.
const SYNODIC: f64 = 29.530588861;
/// A new moon: 6 Jan 2000 18:14 UTC (Meeus).
const NEW_MOON_UNIX: f64 = 947_182_440.0;

/// 0..1 through the lunation at `unix` (0 new, 0.5 full). Mean phase: within
/// about a day of the true one, enough for a widget.
pub fn moon_age(unix: i64) -> f64 {
    ((unix as f64 - NEW_MOON_UNIX) / 86_400.0 / SYNODIC).rem_euclid(1.0)
}

/// How much of the moon is lit, 0..1.
pub fn moon_lit(age: f64) -> f64 {
    (1.0 - (age * std::f64::consts::TAU).cos()) / 2.0
}

pub fn moon_name(age: f64) -> &'static str {
    const NAMES: [&str; 8] = ["New moon", "Waxing crescent", "First quarter", "Waxing gibbous", "Full moon", "Waning gibbous", "Last quarter", "Waning crescent"];
    NAMES[((age * 8.0 + 0.5).floor() as usize) % 8]
}

// ---------------------------------------------------------------- world clock

/// The world clock's cities and their standard UTC offsets in minutes.
/// ponytail: fixed offsets, no daylight saving; read Windows' time zones
/// (`GetDynamicTimeZoneInformation`) when the cities become a choice.
pub const CITIES: [(&str, i32); 3] = [("London", 0), ("New York", -300), ("Tokyo", 540)];

/// "HH:MM" at `offset_min` from UTC.
pub fn clock_at(unix: i64, offset_min: i32) -> String {
    let m = (unix.div_euclid(60) + offset_min as i64).rem_euclid(24 * 60);
    format!("{:02}:{:02}", m / 60, m % 60)
}

// ---------------------------------------------------------------- countdown

/// Days from `today` to `day` (both "YYYY-MM-DD"); negative once past.
pub fn days_until(today: &str, day: &str) -> Option<i64> {
    let parse = |s: &str| {
        let mut p = s.trim().splitn(3, '-');
        let y = p.next()?.parse().ok()?;
        let m: u8 = p.next()?.parse().ok()?;
        let d: u8 = p.next()?.parse().ok()?;
        ((1..=12).contains(&m) && (1..=31).contains(&d)).then(|| crate::datetime::days_from_civil(y, m, d))
    };
    Some(parse(day)? - parse(today)?)
}

// ---------------------------------------------------------------- theme builder

/// Halo Pro's theme builder: the island's ground and ink, its corner
/// radius at rest, and how see-through its ground is (0 solid, 1 the
/// most glass the builder allows).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomLook {
    pub ground: u32,
    pub ink: u32,
    pub radius: f32,
    pub glass: f32,
}

impl Default for CustomLook {
    fn default() -> Self {
        CustomLook { ground: 0x0B0D12, ink: 0xF2F5FA, radius: 14.0, glass: 0.0 }
    }
}

/// The builder's swatches, ground and ink each.
pub const SWATCHES: [(u32, u32); 6] = [(0x0B0D12, 0xF2F5FA), (0xF3F5F9, 0x141821), (0x10213A, 0xE6F0FF), (0x1E1430, 0xF4E9FF), (0x0F2A22, 0xE3FFF4), (0x2A1410, 0xFFEDE3)];

/// Halo's own palette: the pill and the eyes, dark and light
pub const PILL_DARK: u32 = 0x0B0D12;
pub const EYES_DARK: u32 = 0xF2F5FA;
pub const PILL_LIGHT: u32 = 0xF3F5F9;
pub const EYES_LIGHT: u32 = 0x141821;

// ---------------------------------------------------------------- cloud sync

/// Which of two copies of a synced file to keep: the newer by its own
/// stamp, the local one on a tie.
pub fn newer_wins(local_ms: u64, synced_ms: u64) -> bool {
    synced_ms > local_ms
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_keep_five_free_newest_first_and_find_any_case() {
        let mut c = Clips::default();
        for t in ["a", "b", "c", "d", "e", "f"] {
            assert!(c.push(t, clip_cap(false)));
        }
        assert_eq!(c.items, ["f", "e", "d", "c", "b"]);
        assert!(!c.push(" f ", 5), "the same again changes nothing");
        assert!(c.push("c", 5));
        assert_eq!(c.items, ["c", "f", "e", "d", "b"], "a repeat moves up");
        assert!(!c.push("  ", 5));
        let mut p = Clips::default();
        p.push("Invoice 42", clip_cap(true));
        p.push("hello", clip_cap(true));
        assert_eq!(p.find("INVOICE"), ["Invoice 42"]);
        assert_eq!(p.find("").len(), 2);
    }

    #[test]
    fn water_counts_the_day_and_keeps_two_weeks() {
        let mut w = Water::default();
        w.add("2026-11-11", 1);
        w.add("2026-11-11", 4);
        assert_eq!(fmt_glasses(w.today("2026-11-11"), 8), "5/8 glasses");
        w.add("2026-11-11", -9);
        assert_eq!(w.today("2026-11-11"), 0);
        for d in 1..=20 {
            w.add(&format!("2026-10-{d:02}"), 1);
        }
        assert_eq!(w.days.len(), 14);
    }

    #[test]
    fn moon_phase_follows_the_lunation() {
        // 2026-01-03 10:03 UTC was a full moon.
        let full = moon_age(1_767_434_580);
        assert!((full - 0.5).abs() < 0.04, "{full}");
        assert_eq!(moon_name(full), "Full moon");
        assert!(moon_lit(full) > 0.98);
        assert_eq!(moon_name(moon_age(NEW_MOON_UNIX as i64)), "New moon");
    }

    #[test]
    fn world_clock_and_countdown() {
        // 2026-11-11 12:30 UTC.
        let t = 1_794_400_200;
        assert_eq!(clock_at(t, 0), "12:30");
        assert_eq!(clock_at(t, 540), "21:30");
        assert_eq!(clock_at(t, -300), "07:30");
        assert_eq!(days_until("2026-11-11", "2027-01-01"), Some(51));
        assert_eq!(days_until("2026-11-11", "2026-11-10"), Some(-1));
        assert_eq!(days_until("2026-11-11", "soon"), None);
    }

    #[test]
    fn weather_reads_open_meteo() {
        let w = parse_weather(r#"{"current":{"time":"2026-11-11T12:00","temperature_2m":31.4,"weather_code":95}}"#).unwrap();
        assert_eq!((w.temp_c, weather_words(w.code)), (31.4, "Thunder"));
        assert!(parse_weather("{}").is_none());
        assert!(weather_url(3.139, 101.687).contains("latitude=3.14&longitude=101.69"));
    }

    #[test]
    fn free_and_pro_limits() {
        assert_eq!((widget_slots(false), widget_slots(true)), (2, 6));
        assert!(Widget::ALL.iter().filter(|w| !w.pro()).count() == 2);
        assert!(newer_wins(1, 2) && !newer_wins(2, 2));
    }
}

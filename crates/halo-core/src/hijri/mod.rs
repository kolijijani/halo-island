//! The Hijri date, by JAKIM's calendar (MABIMS' criteria): a month starts
//! the evening the young moon, at sunset, stands at least 3° high and 6.4°
//! from the sun. The reckoning still runs over Indonesia's grid, and the
//! announced starts kept in [`KEMENAG_DECIDED`] (Indonesia's sidang isbat)
//! win over it, until Malaysia's own are in.
//!
//! The reckoning runs on [`astro`] (Meeus) over a grid of places. One month
//! takes a few milliseconds and is kept once worked out; nothing runs unless
//! a date is asked for.

pub mod astro;

use std::collections::HashMap;
use std::sync::Mutex;

use astro::{crescent, day_to_jd, new_moon, sunset};

/// Which calendar's dates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Calendar {
    /// JAKIM (MABIMS criterion).
    #[default]
    Jakim,
}

/// A Hijri date.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HijriDate {
    pub year: i32,
    /// 1 = Muharram ... 12 = Dzulhijjah.
    pub month: u8,
    pub day: u8,
}

/// The months as the app writes them.
pub const MONTHS: [&str; 12] = [
    "Muharram",
    "Safar",
    "Rabiul Awal",
    "Rabiul Akhir",
    "Jumadil Awal",
    "Jumadil Akhir",
    "Rajab",
    "Syakban",
    "Ramadan",
    "Syawal",
    "Zulkaidah",
    "Zulhijah",
];

impl HijriDate {
    /// "21 Jumadil Awal 1448 H".
    pub fn label(&self) -> String {
        format!("{} {} {} H", self.day, MONTHS[(self.month - 1) as usize], self.year)
    }
}

/// Lunation 315 (Meeus' count, new moon 25 June 2025) is Muharram 1447.
const MUHARRAM_1447_LUNATION: i64 = 315;

/// The Hijri month (years * 12 + month - 1, from 1 AH) a lunation begins.
fn month_index(k: i64) -> i64 {
    (1447 - 1) * 12 + (k - MUHARRAM_1447_LUNATION)
}

/// Month starts the government announced (sidang isbat, or Kemenag's
/// calendar for Muharram), which win over the reckoning: Hijri year, month,
/// the 1st. Add each new isbat's here.
pub const KEMENAG_DECIDED: &[(i32, u8, (i32, u8, u8))] = &[
    (1445, 9, (2024, 3, 12)),
    (1445, 10, (2024, 4, 10)),
    (1445, 12, (2024, 6, 8)),
    (1446, 9, (2025, 3, 1)),
    (1446, 10, (2025, 3, 31)),
    (1446, 12, (2025, 5, 28)),
    (1447, 1, (2025, 6, 27)),
    (1447, 9, (2026, 2, 19)),
    (1447, 10, (2026, 3, 21)),
    (1447, 12, (2026, 5, 18)),
    (1448, 1, (2026, 6, 16)),
];

/// What a calendar's months need of the young moon at sunset.
#[derive(Clone, Copy, Debug)]
struct Criterion {
    alt: f64,
    elong: f64,
    /// Altitude and elongation seen from the place (parallax taken off)
    /// rather than from the earth's centre.
    alt_topo: bool,
    elong_topo: bool,
    /// Grid step (degrees) over the places looked at.
    step: f64,
}

/// Kemenag reckons the altitude from the place and the elongation from the
/// earth's centre (its 28 Feb 2025 report for Aceh: 4°40' high, 6°24' apart).
const MABIMS: Criterion = Criterion { alt: 3.0, elong: 6.4, alt_topo: true, elong_topo: false, step: 0.5 };

impl Criterion {
    fn met(&self, jd: f64, lat: f64, lng: f64) -> bool {
        let c = crescent(jd, lat, lng);
        let alt = if self.alt_topo { c.alt_topo } else { c.alt_geo };
        let el = if self.elong_topo { c.elong_topo } else { c.elong_geo };
        alt >= self.alt && el >= self.elong
    }
}

fn grid(step: f64, lat0: f64, lat1: f64, lng0: f64, lng1: f64) -> impl Iterator<Item = (f64, f64)> {
    let nlat = ((lat1 - lat0) / step).round() as i64;
    let nlng = ((lng1 - lng0) / step).round() as i64;
    (0..=nlat).flat_map(move |i| (0..=nlng).map(move |j| (lat0 + i as f64 * step, lng0 + j as f64 * step)))
}


/// The earliest month start a criterion gives after the new moon `conj`:
/// for every sunset after it, in `area` (on land where `land` says), that
/// meets it, the day after that sunset's UTC date. `exception` maps a sunset
/// (its JD) to the start it gives instead, when it should count earlier.
fn earliest_start(cr: &Criterion, conj: f64, area: (f64, f64, f64, f64), land: Option<fn(f64, f64) -> bool>, exception: Option<&dyn Fn(f64) -> i64>) -> Option<i64> {
    let conj_day = (conj - astro::UNIX_EPOCH_JD).floor() as i64;
    let mut best: Option<i64> = None;
    for d in conj_day - 1..=conj_day + 2 {
        let day_jd = day_to_jd(d);
        for (lat, lng) in grid(cr.step, area.0, area.1, area.2, area.3) {
            if land.is_some_and(|f| !f(lat, lng)) {
                continue;
            }
            let Some(t) = sunset(day_jd, lat, lng) else { continue };
            if t <= conj || !cr.met(t, lat, lng) {
                continue;
            }
            let start = exception.map_or_else(|| (t - astro::UNIX_EPOCH_JD).floor() as i64 + 1, |f| f(t));
            best = Some(best.map_or(start, |b| b.min(start)));
        }
        // Every later evening's sunset is on day d or after.
        if best.is_some_and(|b| b <= d + 1) {
            break;
        }
    }
    best
}

/// Indonesia, Sabang to Merauke, Miangas to Rote.
const INDONESIA: (f64, f64, f64, f64) = (-11.0, 6.0, 95.0, 141.0);

/// MABIMS' reckoning: the first Indonesian evening after the new moon that
/// meets MABIMS; the month starts the next day. Istikmal (the month before
/// runs to 30 days) caps it two days after the new moon's.
fn mabims_start(conj: f64) -> i64 {
    let conj_day = (conj - astro::UNIX_EPOCH_JD).floor() as i64;
    earliest_start(&MABIMS, conj, INDONESIA, None, None).unwrap_or(conj_day + 2).min(conj_day + 2)
}

/// The month start by the reckoning alone.
fn reckon(cal: Calendar, k: i64) -> i64 {
    let conj = new_moon(k);
    match cal {
        Calendar::Jakim => mabims_start(conj),
    }
}

static STARTS: Mutex<Option<HashMap<(Calendar, i64), i64>>> = Mutex::new(None);

/// The first day (days since 1970, as a civil date) of the month lunation
/// `k` begins, by `cal`.
pub fn month_start(cal: Calendar, k: i64) -> i64 {
    if let Some(&d) = STARTS.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(&(cal, k)).copied())).as_ref() {
        return d;
    }
    let idx = month_index(k);
    let (year, month) = ((idx / 12 + 1) as i32, (idx % 12 + 1) as u8);
    let table = match cal {
        Calendar::Jakim => KEMENAG_DECIDED,
    };
    let decided = table.iter().find(|&&(y, m, _)| y == year && m == month).map(|&(_, _, (gy, gm, gd))| crate::datetime::days_from_civil(gy, gm, gd));
    let d = decided.unwrap_or_else(|| reckon(cal, k));
    if let Ok(mut g) = STARTS.lock() {
        g.get_or_insert_with(HashMap::new).insert((cal, k), d);
    }
    d
}

/// The Hijri date of the civil date `day` (days since 1970). The new date is
/// reckoned from the day, not from the evening before.
pub fn date(cal: Calendar, day: i64) -> HijriDate {
    // The lunation whose month holds `day`: the new moon before it, or the
    // one before that when this month hasn't started yet.
    let mut k = astro::lunation_at(day_to_jd(day) + 1.0);
    while month_start(cal, k) > day {
        k -= 1;
    }
    let idx = month_index(k);
    HijriDate { year: (idx / 12 + 1) as i32, month: (idx % 12 + 1) as u8, day: (day - month_start(cal, k) + 1) as u8 }
}

/// Is `day` in Ramadan by `cal`?
pub fn is_ramadan(cal: Calendar, day: i64) -> bool {
    date(cal, day).month == 9
}

#[cfg(test)]
mod tests;

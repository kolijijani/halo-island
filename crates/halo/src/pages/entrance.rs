//! A page's entrance: one start as the page opens
//! (`PageUiState::entrance`, set by the app), each part a little after the
//! one before, all landed by [`END`]. Parts pop on one spring
//! (`halo_core::spring::bouncy_at`, `cubic-bezier(0.34, 1.32, 0.5, 1)`),
//! rise in or draw along (an arc, a bar).
//!
//! Only shapes move: text and numbers show where and as they are from the
//! first frame. The app starts none under reduced motion or battery saver,
//! so the page shows at rest at once, and the page asks for frames only
//! while a part still moves.

use super::PageKind;
use crate::state::AppState;
use crate::ui::{Ui, WidgetId};
use halo_core::geom::{Color, Point, Rect};
use halo_core::spring::bouncy_at;

/// Every part has landed by then (seconds after the start).
pub const END: f64 = 1.1;
/// How far a part rises in from (DIPs).
const RISE: f32 = 8.0;

/// Seconds into `page`'s entrance, or None once it's over (or there was none).
#[derive(Clone, Copy)]
pub struct Entrance(Option<f32>);

impl Entrance {
    /// `page`'s entrance now; asks for the next frame while it moves.
    pub fn of(ui: &mut Ui, st: &AppState, page: PageKind) -> Self {
        let t = st.ui.entrance.filter(|e| e.0 == page).map(|e| ui.now - e.1).filter(|&t| t < END);
        if t.is_some() {
            ui.request_anim();
        }
        Self(t.map(|t| t as f32))
    }

    /// A part on the spring, starting `delay` s in: 0, then 1 overshooting a little.
    pub fn pop(self, delay: f32) -> f32 {
        self.0.map_or(1.0, |t| bouncy_at(t - delay))
    }

    /// A part drawn along (an arc, a bar), `delay` s in for `dur` s: 0 to 1, easing out.
    pub fn draw(self, delay: f32, dur: f32) -> f32 {
        self.0.map_or(1.0, |t| super::ease_out_cubic((t - delay) / dur))
    }
}

/// The entrance for the page opening (`page`) at `now`: none while motion
/// is off (`still`). Every page has one; a page without moving parts just
/// never asks for its frames.
pub fn start(page: PageKind, now: f64, still: bool) -> Option<(PageKind, f64)> {
    (!still).then_some((page, now))
}

/// Whether some page's entrance still moves (the self-test waits it out).
pub fn moving(st: &AppState) -> bool {
    st.ui.entrance.is_some_and(|e| st.now - e.1 < END)
}

/// How long after a page opens its ambient touches (a water surface, a
/// weather sky, a live dot) keep moving: long enough to be seen, then the
/// page rests and asks for no frames (AGENTS.md: motion must stop).
pub const ALIVE_END: f64 = 10.0;
/// The last seconds of [`ALIVE_END`], over which the motion eases to rest.
const ALIVE_FADE: f32 = 2.0;
/// How long a hover takes to fade in.
const HOVER_S: f64 = 0.16;

/// `page`'s ambient motion now: seconds since it opened and its strength
/// (1, easing to 0 by [`ALIVE_END`]), or None at rest. No entrance (reduced
/// motion, battery saver) means none at all. Asks for ambient frames (~30
/// fps, paused while locked or the display is off) while it moves.
pub fn alive(ui: &mut Ui, st: &AppState, page: PageKind) -> Option<(f32, f32)> {
    // The self-test's pictures are taken once the entrance settles: no motion after it.
    if st.in_selftest {
        return None;
    }
    let t = st.ui.entrance.filter(|e| e.0 == page).map(|e| ui.now - e.1).filter(|t| (0.0..ALIVE_END).contains(t))? as f32;
    ui.request_live();
    let k = ((ALIVE_END as f32 - t) / ALIVE_FADE).clamp(0.0, 1.0);
    Some((t, k * k * (3.0 - 2.0 * k)))
}

/// Whether motion is off (reduced motion or battery saver): hovers and taps
/// show at once.
pub fn still(st: &AppState) -> bool {
    st.ui.hud.still || st.power.saver
}

/// How far the pointer's hover over `id` (`on`: it is over it now) has
/// faded in, 0 to 1. One hover at a time (`PageUiState::hover`); asks for
/// frames while it fades.
pub fn hover(ui: &mut Ui, st: &mut AppState, id: WidgetId, on: bool) -> f32 {
    let mine = st.ui.hover.filter(|h| h.0 == id);
    if !on {
        if mine.is_some() {
            st.ui.hover = None;
        }
        return 0.0;
    }
    if still(st) {
        return 1.0;
    }
    let t0 = mine.map_or(ui.now, |h| h.1);
    st.ui.hover = Some((id, t0));
    let k = ((ui.now - t0) / HOVER_S) as f32;
    if k < 1.0 {
        ui.request_anim();
    }
    super::ease_out_cubic(k)
}

/// A soft band of light crossing the card `r` (corner `radius`) once,
/// `delay` s into the entrance, for 0.55 s. Clipped short of the round
/// corners so it never shows outside the card.
pub fn shine(ui: &mut Ui, e: Entrance, r: Rect, radius: f32, delay: f32) {
    let k = e.draw(delay, 0.55);
    if k <= 0.0 || k >= 1.0 {
        return;
    }
    let x = r.x - 16.0 + (r.w + 32.0) * k;
    let fade = (k * (1.0 - k) * 4.0).min(1.0);
    let inset = 0.1 * radius + 0.5;
    ui.c.clip(Rect::new(r.x + 0.6 * radius, r.y, (r.w - 1.2 * radius).max(0.0), r.h));
    let band = |w: f32| Rect::new(x - w / 2.0, r.y + inset, w, r.h - 2.0 * inset);
    ui.c.fill_rect(band(28.0), Color::white(0.035 * fade));
    ui.c.fill_rect(band(10.0), Color::white(0.05 * fade));
    ui.c.unclip();
}

/// Draw `f` faded and [`RISE`] low at `k` 0, in place at 1.
pub fn rise<R>(ui: &mut Ui, k: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
    if k >= 1.0 {
        return f(ui);
    }
    let (a, m) = (ui.c.alpha(), ui.c.save_transform());
    ui.c.set_alpha(a * k.clamp(0.0, 1.0));
    ui.c.translate(0.0, RISE * (1.0 - k));
    let r = f(ui);
    ui.c.restore_transform(m);
    ui.c.set_alpha(a);
    r
}

/// Draw `f` scaled by `k` about `c`, fading in as it grows. `f` always
/// runs, so a button keeps its place to click while it pops.
pub fn scaled<R>(ui: &mut Ui, c: Point, k: f32, f: impl FnOnce(&mut Ui) -> R) -> R {
    if k == 1.0 {
        return f(ui);
    }
    let (a, m) = (ui.c.alpha(), ui.c.save_transform());
    ui.c.set_alpha(a * k.clamp(0.0, 1.0));
    ui.c.scale_about(c, k.max(0.01));
    let r = f(ui);
    ui.c.restore_transform(m);
    ui.c.set_alpha(a);
    r
}

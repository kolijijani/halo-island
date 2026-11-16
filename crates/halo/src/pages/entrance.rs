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
#[derive(Clone, Copy, Default)]
pub struct Entrance(Option<f32>);

impl Entrance {
    /// `page`'s entrance now; asks for the next frame while it moves.
    pub fn of(ui: &mut Ui, st: &AppState, page: PageKind) -> Self {
        Self::at(ui, st.ui.entrance.filter(|e| e.0 == page).map(|e| e.1))
    }

    /// An entrance that began at `t0` (mono seconds): a page's, or a part
    /// of one replaying it (System's module picked). Asks for the next frame
    /// while it moves. Callers pass no `t0` while motion is off ([`still`]).
    pub fn at(ui: &mut Ui, t0: Option<f64>) -> Self {
        let t = t0.map(|t0| ui.now - t0).filter(|&t| t < END);
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

/// How long a shared control's glide takes (a switch's knob, a picker's lift, a row's wash).
pub const GLIDE_S: f64 = 0.2;
/// A control not drawn for this long starts again where it's going.
const GLIDE_STALE: f64 = 1.0;

/// One control's glide: where from, where to, since when, last drawn.
#[derive(Clone, Copy)]
struct Glide {
    from: Rect,
    to: Rect,
    t0: f64,
    drawn: f64,
}

thread_local! {
    /// Motion is off this frame ([`still`]), for the shared controls that
    /// get no `AppState` (set by `pages::draw_pages`).
    static STILL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static GLIDES: std::cell::RefCell<std::collections::HashMap<WidgetId, Glide>> = std::cell::RefCell::default();
}

/// Whether motion is off for this frame's shared controls ([`glide`]).
pub fn set_still(still: bool) {
    STILL.with(|s| s.set(still));
}

/// `id`'s rect this frame on its way to `to` over [`GLIDE_S`], easing out;
/// asks for frames while it moves. A control first drawn (or not drawn for
/// a second) starts where it's going, so a page never opens mid-glide; with
/// motion off it's there at once. Only an answer to the person moves it.
pub fn glide(ui: &mut Ui, id: WidgetId, to: Rect) -> Rect {
    let still = STILL.with(std::cell::Cell::get);
    let (shown, moving) = GLIDES.with(|g| glide_step(&mut g.borrow_mut(), id, to, ui.now, still));
    if moving {
        ui.request_anim();
    }
    shown
}

/// [`glide`]'s step: `id`'s rect at `now` and whether it still moves.
fn glide_step(g: &mut std::collections::HashMap<WidgetId, Glide>, id: WidgetId, to: Rect, now: f64, still: bool) -> (Rect, bool) {
    // Controls of pages long closed go (a few hundred at most stay).
    if g.len() > 256 {
        g.retain(|_, v| now - v.drawn < GLIDE_STALE);
    }
    let rest = Glide { from: to, to, t0: f64::NEG_INFINITY, drawn: now };
    let e = g.entry(id).or_insert(rest);
    if still || now - e.drawn > GLIDE_STALE {
        *e = rest;
        return (to, false);
    }
    let k = super::ease_out_cubic(((now - e.t0) / GLIDE_S) as f32);
    let shown = Rect::lerp(e.from, e.to, k);
    if e.to != to {
        *e = Glide { from: shown, to, t0: now, drawn: now };
        return (shown, true);
    }
    e.drawn = now;
    (shown, k < 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_control_glides_only_after_a_change_and_never_with_motion_off() {
        let mut g = std::collections::HashMap::new();
        let (a, b) = (Rect::new(0.0, 0.0, 10.0, 10.0), Rect::new(30.0, 0.0, 10.0, 10.0));
        // First drawn: where it's going, at rest.
        assert_eq!(glide_step(&mut g, 1, a, 10.0, false), (a, false));
        // Picked elsewhere: it leaves from where it was, gets there by GLIDE_S, then rests.
        assert_eq!(glide_step(&mut g, 1, b, 10.02, false), (a, true));
        let (mid, moving) = glide_step(&mut g, 1, b, 10.02 + GLIDE_S / 2.0, false);
        assert!(moving && mid.x > 15.0 && mid.x < 30.0, "{mid:?}");
        assert_eq!(glide_step(&mut g, 1, b, 10.02 + GLIDE_S, false), (b, false));
        // Motion off, or not drawn for a while: there at once.
        assert_eq!(glide_step(&mut g, 1, a, 10.5, true), (a, false));
        assert_eq!(glide_step(&mut g, 1, b, 20.0, false), (b, false));
    }
}

/// [`glide`] for one number (a switch 0 off to 1 on, a hover's wash).
pub fn glide_k(ui: &mut Ui, id: WidgetId, to: f32) -> f32 {
    glide(ui, id, Rect::new(to, 0.0, 0.0, 0.0)).x
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

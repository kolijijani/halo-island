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
use crate::ui::Ui;
use halo_core::geom::Point;
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

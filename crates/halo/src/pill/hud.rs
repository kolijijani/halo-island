//! The pop-ups (`Mode::Hud`): volume, charging, low battery, Bluetooth,
//! Caps Lock and Wi-Fi.
//!
//! Each is a capsule of the resting pill's height: a glyph in the left end,
//! a label (or the volume's level bar) after it and the value or the battery
//! on the right, clear of the round ends. What they show and how the level
//! bar glides are Classic's (`activities::draw_hud`); the layout is the Pill's.

use crate::activities::HudTag;
use crate::gfx::{TextStyle, Weight};
use crate::icons::Icon;
use crate::island::Geom;
use crate::state::{AppState, Hud, HudKind};
use crate::ui::Ui;
use halo_core::geom::{palette, Color, Point, Rect};
use halo_core::spring::Spring;
use halo_core::svgpath::Seg;

use super::media;

/// One height for every pop-up, with round ends (spec 6.1).
const H: f32 = 32.0;
/// The spec's 260 plus room for "Muted" and "100%" (its value was a bare "64").
const VOLUME_W: f32 = 272.0;
/// Charging, on battery and low battery: the longest label and the battery.
const BATTERY_W: f32 = 216.0;
/// Bluetooth fits its line between these.
const BT_MIN_W: f32 = 160.0;
const BT_MAX_W: f32 = 360.0;
/// The left glyph: a 16 DIP box 12 in, in the round end (spec 6.8).
const GLYPH: f32 = 16.0;
const GLYPH_X: f32 = 12.0;
const BATTERY_ICON: f32 = 20.0;
/// Labels start past the glyph; the level bar a little further (spec 6.8).
const TEXT_X: f32 = 36.0;
const TRACK_X: f32 = 40.0;
/// The value ends 14 from the right end (spec 6.8), the battery 16 as on the resting pill (6.3).
const VALUE_INSET: f32 = 14.0;
const BATTERY_INSET: f32 = 16.0;
/// The level bar: 5 thick, a faint track under a white fill (spec 6.8).
const TRACK_H: f32 = 5.0;
const TRACK: Color = Color::white(0.22);
const FILL: Color = Color::white(1.0);
const VALUE_GAP: f32 = 10.0;
/// The battery as the resting pill draws it (spec 6.3): the percentage's
/// slot (room for "100%"), a gap, a 20 x 10 body and its nub.
const PCT_SLOT: f32 = 35.0;
const PCT_GAP: f32 = 6.0;
const BODY_W: f32 = 20.0;
const BODY_H: f32 = 10.0;
const NUB_GAP: f32 = 0.75;
const NUB_W: f32 = 1.75;
const BATTERY_GLYPH_W: f32 = BODY_W + NUB_GAP + NUB_W;
const CLUSTER_W: f32 = PCT_SLOT + PCT_GAP + BATTERY_GLYPH_W;
/// Bluetooth's disc sits in the left end, 6 from the edge like the music pill's art (spec 6.4).
const DISC: f32 = 20.0;
const DISC_GLYPH: f32 = 12.0;
/// "Sound Buds  Connected": two spaces' worth between the name and the state, as the music peek's title and artist.
const SEP: f32 = 7.0;
/// The least room kept between the left group and the battery.
const MID_GAP: f32 = 16.0;

/// The pop-ups' one motion language (plan 0.3.00 §2.1): a pop when
/// something comes on or arrives, a settle when it goes, nothing for a
/// warning or a repeat. Text and numbers never move.
const POP: (f32, f32) = (520.0, 30.0);
const POP_FROM: f32 = 0.6;
/// How far the glyph rises in on a pop (from below) or drops in on a settle.
const LIFT: f32 = 3.0;
/// The settle's dip: `Spring::smooth` kicked from rest at this speed
/// bottoms out at 0.92 about 50 ms in, and comes back (measured on `step`).
const SETTLE_KICK: f32 = 5.0;
/// The gesture waits for the content's fade (plan §4's timing).
const DELAY: f64 = 0.08;
/// The disc while off, Classic's and the Pill's.
const DISC_OFF: Color = Color::white(0.16);

#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    Pop,
    Settle,
    Still,
}

/// What this pop-up does as it shows, and whether its disc is lit.
fn gesture(k: &HudKind) -> (Gesture, bool) {
    match k {
        HudKind::Volume { .. } | HudKind::Brightness { .. } | HudKind::LowBattery { .. } => (Gesture::Still, false),
        HudKind::Charging { plugged, .. } => (if *plugged { Gesture::Pop } else { Gesture::Settle }, false),
        HudKind::Bluetooth { connected, .. } => (if *connected { Gesture::Pop } else { Gesture::Settle }, *connected),
        HudKind::Mic { .. } => (Gesture::Pop, true),
        k => {
            let on = crate::activities::toggle_glyph(k).1;
            (if on { Gesture::Pop } else { Gesture::Settle }, on)
        }
    }
}

/// The pop-up's moving parts (`PageUiState::hud`), Classic's and the Pill's.
/// It starts over only when the pop-up says something else, or a new one
/// shows after the last one's time ran out.
#[derive(Clone, Debug)]
pub struct Motion {
    /// Windows' Animation effects or the Mac's Reduce motion are off: every
    /// pop-up (and a notice) shows at rest at once (the app sets it as the
    /// island changes mode).
    pub still: bool,
    kind: Option<HudKind>,
    until: f64,
    gesture: Gesture,
    /// When the gesture began, when its parts start to move, and the last frame.
    since: f64,
    hold: f64,
    last: f64,
    scale: Spring,
    lift: Spring,
    lit: Spring,
    level: Spring,
}

impl Default for Motion {
    fn default() -> Self {
        Self {
            still: false,
            kind: None,
            until: 0.0,
            gesture: Gesture::Still,
            since: f64::NEG_INFINITY,
            hold: f64::NEG_INFINITY,
            last: 0.0,
            scale: Spring::smooth(1.0),
            lift: Spring::smooth(0.0),
            lit: Spring::smooth(0.0),
            level: Spring::smooth(0.0),
        }
    }
}

/// Where the moving parts are this frame.
pub(crate) struct Pose {
    pub scale: f32,
    pub lift: f32,
    pub lit: f32,
    /// The volume bar.
    pub level: f32,
    /// Seconds into the glow, while there is one.
    pub glow: Option<f32>,
}

impl Motion {
    /// Step to `ui.now` for `hud`, asking for frames only while something moves.
    pub(crate) fn frame(&mut self, ui: &mut Ui, hud: &Hud, saver: bool) -> Pose {
        let p = self.step(ui.now, hud, saver);
        if self.moving(ui.now) {
            ui.request_anim();
        }
        p
    }

    fn step(&mut self, now: f64, hud: &Hud, saver: bool) -> Pose {
        // At rest no frames came: the time since the last one isn't motion.
        let dt = if self.moving(self.last) { (now - self.last) as f32 } else { 0.0 };
        let (g, on) = gesture(&hud.kind);
        // A new pop-up, or a word about the same thing in place of one showing.
        let fresh = now >= self.until || self.kind.as_ref().is_none_or(|k| !k.same_slot(&hud.kind));
        let turned = self.kind.as_ref().map(gesture) != Some((g, on));
        if turned || (fresh && self.until != hud.until) {
            self.start(g, on, fresh, now);
        }
        self.kind = Some(hud.kind.clone());
        self.until = hud.until;
        match hud.kind {
            HudKind::Volume { level, muted } => self.level.set_target(if muted { 0.0 } else { level }),
            HudKind::Brightness { level } => self.level.set_target(level),
            _ => {}
        }
        if self.still || saver {
            for s in [&mut self.scale, &mut self.lift, &mut self.lit, &mut self.level] {
                s.snap(s.target);
            }
            self.gesture = Gesture::Still;
        }
        self.last = now;
        self.level.step_eps(dt, 0.002);
        if now >= self.hold {
            self.scale.step_eps(dt, 0.005);
            self.lift.step_eps(dt, 0.01);
            self.lit.step_eps(dt, 0.005);
        }
        let glow = (self.gesture == Gesture::Pop).then(|| (now - self.since) as f32);
        Pose { scale: self.scale.value, lift: self.lift.value, lit: self.lit.value.clamp(0.0, 1.0), level: self.level.value.clamp(0.0, 1.0), glow }
    }

    fn start(&mut self, g: Gesture, on: bool, fresh: bool, now: f64) {
        self.gesture = g;
        self.since = now;
        // A quick second press carries on from where the first one is.
        self.hold = if fresh { now + DELAY } else { now };
        let lit = if on { 1.0 } else { 0.0 };
        match g {
            Gesture::Still => {
                self.scale.snap(1.0);
                self.lift.snap(0.0);
                self.lit.snap(lit);
                return;
            }
            // In place of one still showing, the parts keep where they are
            // and how they move: no jump, just the kick.
            _ if !fresh => self.scale.velocity -= SETTLE_KICK,
            Gesture::Pop => {
                self.scale.snap(POP_FROM);
                self.lift.snap(LIFT);
                self.lit.snap(1.0 - lit);
            }
            Gesture::Settle => {
                self.scale.snap(1.0);
                self.scale.velocity = -SETTLE_KICK;
                self.lift.snap(-LIFT);
                self.lit.snap(1.0 - lit);
            }
        }
        let s = Spring::smooth(0.0);
        let (k, d) = if g == Gesture::Pop { POP } else { (s.stiffness, s.damping) };
        self.scale.retune(k, d);
        self.scale.set_target(1.0);
        self.lift.set_target(0.0);
        self.lit.set_target(lit);
    }

    /// Something still moves at `now` (the self-test waits it out).
    pub fn moving(&self, now: f64) -> bool {
        let t = (now - self.since) as f32;
        !self.level.is_settled_eps(0.002)
            || (self.gesture != Gesture::Still
                && (now < self.hold
                    || !self.scale.is_settled_eps(0.005)
                    || !self.lift.is_settled_eps(0.01)
                    || !self.lit.is_settled_eps(0.005)
                    || (self.gesture == Gesture::Pop && t < media::PULSE_FROM + media::PULSE_S)))
    }
}

/// A pop-up's disc (radius `r`) and its glyph (`g` square) at `c`, as `p`
/// has them: lit with `on` and `ink`, popped or settled.
pub(crate) fn disc(ui: &mut Ui, c: Point, r: f32, icon: Icon, g: f32, on: Color, ink: Color, p: &Pose) {
    let m = ui.c.save_transform();
    ui.c.scale_about(c, p.scale.max(0.01));
    ui.c.fill_circle(c, r, Color::lerp(DISC_OFF, on, p.lit));
    ui.c.translate(0.0, p.lift);
    ui.c.icon(icon, Rect::from_center(c, g, g), Color::lerp(palette::TEXT, ink, p.lit));
    ui.c.restore_transform(m);
}

/// A glyph without a disc (the power pop-ups'), popped or settled as `p` has it.
pub(crate) fn glyph(ui: &mut Ui, r: Rect, icon: Icon, color: Color, p: &Pose) {
    let m = ui.c.save_transform();
    ui.c.scale_about(r.center(), p.scale.max(0.01));
    ui.c.translate(0.0, p.lift);
    ui.c.icon(icon, r, color);
    ui.c.restore_transform(m);
}

pub fn geom(t: HudTag, st: &AppState) -> Option<Geom> {
    let w = match t {
        HudTag::Volume | HudTag::Brightness => VOLUME_W,
        HudTag::Charging | HudTag::LowBattery => BATTERY_W,
        HudTag::Bluetooth => bluetooth_w(st),
        HudTag::Toggle => toggle_w(st),
    };
    Some(Geom::new(w, H, H / 2.0))
}

pub fn draw(ui: &mut Ui, st: &mut AppState, _t: HudTag, body: Rect) -> bool {
    // As in Classic, a pop-up whose time is up draws nothing while its pill morphs away.
    let Some(hud) = st.hud.clone() else { return true };
    let p = st.ui.hud.frame(ui, &hud, st.power.saver);
    // The glow lies under everything else: the bolt's green, mic red/green, else the accent.
    if let Some(t) = p.glow {
        let color = match hud.kind {
            HudKind::Charging { .. } | HudKind::Mic { muted: false } => palette::GREEN,
            HudKind::Mic { muted: true } => palette::RED,
            _ => st.accent,
        };
        media::pulse(ui, body, color, t);
    }
    match hud.kind {
        HudKind::Volume { level, muted } => volume(ui, body, level, muted, p.level),
        HudKind::Brightness { level } => brightness(ui, body, level, p.level),
        HudKind::Charging { percent, plugged } => {
            let (icon, color, label) = if plugged { (Icon::Bolt, palette::GREEN, "Charging") } else { (Icon::Battery, palette::TEXT, "On battery") };
            power(ui, body, icon, color, label, percent, plugged, &p);
        }
        HudKind::LowBattery { percent } => power(ui, body, Icon::Battery, palette::RED, "Low battery", percent, false, &p),
        HudKind::Bluetooth { name, connected, battery, audio } => bluetooth(ui, st, body, &name, connected, battery, audio, &p),
        k @ (HudKind::CapsLock { .. } | HudKind::Wifi { .. } | HudKind::Mic { .. }) => toggle(ui, st, body, &k, &p),
    }
    true
}

fn label_style() -> TextStyle {
    TextStyle::body().weight(Weight::Semibold)
}

fn value_style() -> TextStyle {
    TextStyle::body().weight(Weight::Medium).tabular().color(palette::TEXT_DIM)
}

/// Snap a DIP coordinate to the device-pixel grid (crisp thin outlines at 100 % and 150 %).
fn snap(ui: &Ui, v: f32) -> f32 {
    let s = ui.c.scale().max(0.1);
    (v * s).round() / s
}

/// The speaker by level, the bar and the percentage (or "Muted").
fn volume(ui: &mut Ui, body: Rect, level: f32, muted: bool, shown: f32) {
    let cy = body.center().y;
    let waves = if muted || level <= 0.001 {
        None
    } else if level < 0.5 {
        Some(1)
    } else {
        Some(2)
    };
    speaker(ui, Rect::new(body.x + GLYPH_X, cy - GLYPH / 2.0, GLYPH, GLYPH), waves, palette::TEXT);

    let vs = value_style();
    // The bar keeps one length for "100%" and "Muted".
    let slot = ui.c.measure("100%", &vs, 200.0).0.max(ui.c.measure("Muted", &vs, 200.0).0);
    let right = body.right() - VALUE_INSET;
    let (x0, x1) = (body.x + TRACK_X, right - slot - VALUE_GAP);
    // The bar glides to new levels (`Motion`); the number is exact immediately.
    // While the pill is still growing there may be no room for it yet.
    if x1 - x0 >= TRACK_H {
        let bar = Rect::new(x0, snap(ui, cy - TRACK_H / 2.0), x1 - x0, TRACK_H);
        ui.c.fill_rrect(bar, TRACK_H / 2.0, TRACK);
        if shown > 0.0 {
            ui.c.fill_rrect(Rect::new(bar.x, bar.y, (bar.w * shown).max(TRACK_H), bar.h), TRACK_H / 2.0, FILL);
        }
    }
    let value = if muted { "Muted".to_string() } else { format!("{}%", (level * 100.0).round() as i32) };
    ui.c.text(&value, Rect::new(right - slot - 4.0, body.y, slot + 4.0, body.h), &vs.right());
}

/// Brightness: the sun glyph, the gliding bar and the percentage.
fn brightness(ui: &mut Ui, body: Rect, level: f32, shown: f32) {
    let cy = body.center().y;
    ui.c.icon(Icon::Sun, Rect::new(body.x + GLYPH_X, cy - GLYPH / 2.0, GLYPH, GLYPH), palette::TEXT);
    let vs = value_style();
    let slot = ui.c.measure("100%", &vs, 200.0).0;
    let right = body.right() - VALUE_INSET;
    let (x0, x1) = (body.x + TRACK_X, right - slot - VALUE_GAP);
    if x1 - x0 >= TRACK_H {
        let bar = Rect::new(x0, snap(ui, cy - TRACK_H / 2.0), x1 - x0, TRACK_H);
        ui.c.fill_rrect(bar, TRACK_H / 2.0, TRACK);
        if shown > 0.0 {
            ui.c.fill_rrect(Rect::new(bar.x, bar.y, (bar.w * shown).max(TRACK_H), bar.h), TRACK_H / 2.0, FILL);
        }
    }
    let value = format!("{}%", (level * 100.0).round() as i32);
    ui.c.text(&value, Rect::new(right - slot - 4.0, body.y, slot + 4.0, body.h), &vs.right());
}

/// A filled speaker in a 16-unit box with `waves` sound waves, or a cross
/// for muted (`None`), as in the spec's mockup.
fn speaker(ui: &mut Ui, r: Rect, waves: Option<u8>, color: Color) {
    let s = r.w / 16.0;
    let (x, cy) = (r.x, r.center().y);
    let pts = [(1.0, -3.0), (4.0, -3.0), (8.0, -6.5), (8.0, 6.5), (4.0, 3.0), (1.0, 3.0)];
    let mut cone: Vec<Seg> = pts.iter().map(|&(dx, dy)| Seg::LineTo(x + dx * s, cy + dy * s)).collect();
    cone[0] = Seg::MoveTo(x + pts[0].0 * s, cy + pts[0].1 * s);
    cone.push(Seg::Close);
    // A hairline in the same colour rounds the cone's corners a little.
    ui.c.path(&cone, Some(color), Some((color, 0.75 * s)));
    let w = 1.5 * s;
    match waves {
        Some(n) => {
            let c = Point::new(x + 8.5 * s, cy);
            for i in 0..n {
                // 50 degrees either side of 3 o'clock (`arc` turns clockwise from 12).
                ui.c.arc(c, (4.0 + 3.2 * i as f32) * s, 40.0 / 360.0, 100.0 / 360.0, color, w);
            }
        }
        None => {
            let (cx, d) = (x + 12.5 * s, 2.5 * s);
            ui.c.line(Point::new(cx - d, cy - d), Point::new(cx + d, cy + d), color, w);
            ui.c.line(Point::new(cx - d, cy + d), Point::new(cx + d, cy - d), color, w);
        }
    }
}

/// Charging, on battery or low: the glyph, the label, and the battery as
/// the resting pill shows it.
#[allow(clippy::too_many_arguments)]
fn power(ui: &mut Ui, body: Rect, icon: Icon, color: Color, label: &str, percent: u8, charging: bool, p: &Pose) {
    let cy = body.center().y;
    // The battery outline is a flat glyph: a bigger box gives it the bolt's weight.
    let g = if icon == Icon::Battery { BATTERY_ICON } else { GLYPH };
    glyph(ui, Rect::from_center(Point::new(body.x + GLYPH_X + GLYPH / 2.0, cy), g, g), icon, color, p);
    battery_cluster(ui, body.right() - BATTERY_INSET, cy, percent, charging);
    // The label's room at the pill's own width, so it doesn't re-ellipsize while the pill grows.
    let room = body.w.max(BATTERY_W) - TEXT_X - MID_GAP - CLUSTER_W - BATTERY_INSET;
    ui.c.text(label, Rect::new(body.x + TEXT_X, body.y, room, body.h), &label_style());
}

/// The device's glyph in a disc (the accent while connected), its name and
/// whether it came or went on one line, and its battery when it reports one.
#[allow(clippy::too_many_arguments)]
fn bluetooth(ui: &mut Ui, st: &AppState, body: Rect, name: &str, connected: bool, battery: Option<u8>, audio: bool, p: &Pose) {
    let cy = body.center().y;
    let c = Point::new(body.x + body.h / 2.0, cy);
    let icon = if audio { Icon::Headphones } else { Icon::Bluetooth };
    disc(ui, c, DISC / 2.0, icon, DISC_GLYPH, st.accent, ink_on(st.accent), p);

    let (name_st, state_st) = (label_style(), TextStyle::body().color(palette::TEXT_DIM));
    let state = if connected { "Connected" } else { "Disconnected" };
    let sw = ui.c.measure(state, &state_st, 400.0).0;
    let nw = ui.c.measure(name, &name_st, 10_000.0).0;
    let x = body.x + TEXT_X;
    // Rooms are taken at the pill's own width (see `power`): a long name
    // gives way, the state stays whole.
    let full = body.w.max(bluetooth_w(st));
    match battery {
        // The battery holds the right end; the state follows the name.
        Some(b) => {
            battery_cluster(ui, body.right() - BATTERY_INSET, cy, b, false);
            let pw = ui.c.measure(&format!("{b}%"), &value_style(), 100.0).0;
            let sep = if nw > 0.0 { SEP } else { 0.0 };
            let room = full - TEXT_X - (BATTERY_INSET + BATTERY_GLYPH_W + PCT_GAP + pw + MID_GAP) - sep - sw;
            let name_w = text_upto(ui, name, nw, x, body, room, &name_st);
            ui.c.text(state, Rect::new(x + name_w + sep, body.y, sw + 2.0, body.h), &state_st);
        }
        // Without one the state holds the right end, so a wrong guess of the
        // width only moves the room in the middle, as on the resting pill.
        None => {
            let right = body.right() - BATTERY_INSET;
            ui.c.text(state, Rect::new(right - sw - 2.0, body.y, sw + 2.0, body.h), &state_st.right());
            text_upto(ui, name, nw, x, body, full - TEXT_X - BATTERY_INSET - sw - SEP, &name_st);
        }
    }
}

/// Caps Lock or Wi-Fi: the glyph in a disc (the accent while on or
/// joined), what it is and its state on one line, as Bluetooth's.
fn toggle(ui: &mut Ui, st: &AppState, body: Rect, k: &HudKind, p: &Pose) {
    let cy = body.center().y;
    let c = Point::new(body.x + body.h / 2.0, cy);
    let icon = crate::activities::toggle_glyph(k).0;
    let fill = match k {
        HudKind::Mic { muted: true } => palette::RED,
        HudKind::Mic { muted: false } => palette::GREEN,
        _ => st.accent,
    };
    disc(ui, c, DISC / 2.0, icon, DISC_GLYPH, fill, ink_on(fill), p);
    let (title, state) = k.words();
    let (name_st, state_st) = (label_style(), TextStyle::body().color(palette::TEXT_DIM));
    let sw = ui.c.measure(&state, &state_st, 400.0).0;
    let right = body.right() - BATTERY_INSET;
    ui.c.text(&state, Rect::new(right - sw - 2.0, body.y, sw + 2.0, body.h), &state_st.right());
    let nw = ui.c.measure(&title, &name_st, 10_000.0).0;
    text_upto(ui, &title, nw, body.x + TEXT_X, body, body.w.max(toggle_w(st)) - TEXT_X - BATTERY_INSET - sw - SEP, &name_st);
}

/// Caps Lock's or Wi-Fi's pop-up: its words, guessed a little wide as
/// Bluetooth's are.
fn toggle_w(st: &AppState) -> f32 {
    let Some(k) = st.hud.as_ref().map(|h| &h.kind) else { return BT_MIN_W };
    let (title, state) = k.words();
    (TEXT_X + approx_w(&title) + SEP + approx_w(&state) + BATTERY_INSET).clamp(BT_MIN_W, BT_MAX_W).round()
}

/// `s`, `w` wide as measured, at `x` in at most `room`: whole when it fits,
/// else ellipsized. Returns the width it takes.
fn text_upto(ui: &mut Ui, s: &str, w: f32, x: f32, body: Rect, room: f32, ts: &TextStyle) -> f32 {
    let w = w.min(room).max(0.0);
    if w > 0.0 {
        // Slack for text that fits, so rounding doesn't ellipsize it.
        let slack = if w < room { 2.0 } else { 0.0 };
        ui.c.text(s, Rect::new(x, body.y, w + slack, body.h), ts);
    }
    w
}

/// White on the accent, dark on a very light one (as the Today page's date).
fn ink_on(fill: Color) -> Color {
    if fill.luminance() > 0.6 {
        palette::ON_ACCENT_DARK
    } else {
        palette::ON_ACCENT
    }
}

/// The Bluetooth pop-up's width: the disc, the name and the state, and the
/// battery when known. `geom` can't measure text, so the name is guessed a
/// little wide; the drawing measures, a name that doesn't fit gives way and
/// a wrong guess only moves the room in the middle.
fn bluetooth_w(st: &AppState) -> f32 {
    let Some(HudKind::Bluetooth { name, connected, battery, .. }) = st.hud.as_ref().map(|h| &h.kind) else { return BT_MIN_W };
    let state = approx_w(if *connected { "Connected" } else { "Disconnected" });
    let end = if battery.is_some() { MID_GAP + CLUSTER_W + BATTERY_INSET } else { BATTERY_INSET };
    (TEXT_X + approx_w(name) + SEP + state + end).clamp(BT_MIN_W, BT_MAX_W).round()
}

/// A generous guess of `s`'s width in 13 DIP semibold: Inter's widths, a
/// little wider than SF Pro's and about 8 % wider than Segoe UI's.
fn approx_w(s: &str) -> f32 {
    s.chars()
        .map(|ch| match ch {
            ' ' => 3.4,
            'i' | 'j' | 'l' | 'I' | '.' | ',' | ':' | ';' | '\'' | '|' | '!' => 3.6,
            '(' | ')' | 'f' | 'r' | 't' => 5.2,
            'm' | 'M' => 12.0,
            'w' => 11.0,
            'W' => 13.4,
            c if c.is_ascii_uppercase() => 9.6,
            c if (c as u32) < 0x1100 => 8.0,
            // Hangul, kana, CJK and the like are about an em wide.
            _ => 13.0,
        })
        .sum()
}

/// The battery as the resting pill shows it: the percentage, then the drawn
/// battery, its right end at `right`, centred on `cy`.
fn battery_cluster(ui: &mut Ui, right: f32, cy: f32, percent: u8, charging: bool) {
    let gx = right - BATTERY_GLYPH_W;
    battery(ui, Rect::new(gx, cy - BODY_H / 2.0, BATTERY_GLYPH_W, BODY_H), percent, charging);
    // Right-aligned in a box wider than its slot, so a wider font never ellipsizes "100%".
    let (tr, bw) = (gx - PCT_GAP, PCT_SLOT + 12.0);
    ui.c.text(&format!("{percent}%"), Rect::new(tr - bw, cy - 10.0, bw, 20.0), &value_style().right());
}

/// The resting pill's battery glyph (spec 6.3) in `r` (body and nub).
fn battery(ui: &mut Ui, r: Rect, percent: u8, charging: bool) {
    super::idle::battery(ui, r, percent, charging);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::Hud;
    use halo_core::settings::Settings;
    use halo_core::store::Store;

    fn with_device(name: &str, battery: Option<u8>) -> AppState {
        let mut st = AppState::new(Settings::default(), Store::default());
        st.hud = Some(Hud { kind: HudKind::Bluetooth { name: name.into(), connected: true, battery, audio: true }, until: 10.0 });
        st
    }

    #[test]
    fn every_pop_up_is_a_capsule_as_tall_as_the_resting_pill() {
        let st = with_device("Sound Buds", Some(80));
        for t in [HudTag::Volume, HudTag::Charging, HudTag::LowBattery, HudTag::Bluetooth] {
            let g = geom(t, &st).unwrap();
            assert_eq!((g.h, g.r), (32.0, 16.0), "{t:?}");
        }
    }

    /// Steps `m` through `secs` at 60 Hz; the least scale and lift seen, and
    /// whether it still moved at the end.
    fn run(m: &mut Motion, hud: &Hud, from: f64, secs: f64) -> (f32, f32, bool) {
        let (mut lo, mut lift) = (f32::MAX, 0.0f32);
        let n = (secs * 60.0) as usize;
        for i in 0..=n {
            let p = m.step(from + i as f64 / 60.0, hud, false);
            lo = lo.min(p.scale);
            lift = if lift.abs() > p.lift.abs() { lift } else { p.lift };
        }
        (lo, lift, m.moving(from + secs))
    }

    #[test]
    fn pop_ups_pop_settle_or_hold_still_and_then_rest() {
        let caps = |on| Hud { kind: HudKind::CapsLock { on }, until: 10.0 };
        // On: from 0.6 up, rising in from below, glowing; at rest by 0.75 s.
        let mut m = Motion::default();
        let first = m.step(1.0, &caps(true), false);
        assert_eq!((first.scale, first.lift, first.glow), (POP_FROM, LIFT, Some(0.0)));
        let (lo, _, moving) = run(&mut m, &caps(true), 1.0, 0.75);
        assert!(lo >= POP_FROM && !moving);
        assert_eq!(m.step(1.75, &caps(true), false).scale, 1.0);
        // Off, in place: dips under 1 without a jump, no glow, rests again.
        let (lo, _, moving) = run(&mut m, &caps(false), 2.0, 0.75);
        assert!(lo < 0.97 && lo > 0.85 && !moving, "{lo}");
        assert_eq!(m.step(2.75, &caps(false), false).glow, None);
        // A fresh Off dips to about 0.92 and drops in from above.
        let mut m = Motion::default();
        let (lo, lift, moving) = run(&mut m, &caps(false), 1.0, 0.75);
        assert!((lo - 0.92).abs() < 0.01 && lift == -LIFT && !moving, "{lo} {lift}");
        // Low battery and volume never pop; the bar glides on the clock.
        let mut m = Motion::default();
        let low = Hud { kind: HudKind::LowBattery { percent: 9 }, until: 10.0 };
        assert!(!m.moving(1.0) && m.step(1.0, &low, false).scale == 1.0 && !m.moving(1.0));
        let vol = |level| Hud { kind: HudKind::Volume { level, muted: false }, until: 10.0 };
        m.step(2.0, &vol(0.5), false);
        let half = (0..9).map(|i| m.step(2.0 + i as f64 / 60.0, &vol(0.5), false).level).last().unwrap();
        let mut fast = Motion::default();
        fast.step(2.0, &vol(0.5), false);
        let half_144 = (0..20).map(|i| fast.step(2.0 + i as f64 / 144.0, &vol(0.5), false).level).last().unwrap();
        assert!((half - half_144).abs() < 0.03, "60 Hz {half} vs 144 Hz {half_144}");
        // Battery saver: at rest at once.
        let mut m = Motion::default();
        let p = m.step(1.0, &caps(true), true);
        assert!(p.scale == 1.0 && p.lift == 0.0 && p.lit == 1.0 && p.glow.is_none() && !m.moving(1.0));
    }

    #[test]
    fn bluetooth_grows_with_the_name_within_bounds() {
        let w = |name: &str, battery| geom(HudTag::Bluetooth, &with_device(name, battery)).unwrap().w;
        let usual = w("Sound Buds", Some(80));
        assert!(w("Buds", Some(80)) < usual);
        assert!(usual < w("Surface Headphones 2+", Some(80)));
        assert_eq!(w("A headset whose name goes on and on and on", Some(80)), BT_MAX_W);
        assert!(w("JBL", None) >= BT_MIN_W);
        // No battery reading: no room kept for one.
        assert!(w("Sound Buds", None) < usual);
    }
}

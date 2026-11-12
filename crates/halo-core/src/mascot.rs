//! Halo, the mascot that rests in the idle island in place of the clock
//! (`docs/A2Z-07-mascot-ideas.md`, "Keputusan"): awake and blinking every
//! 4-6 s, dozing after 8 s without input, asleep after 20 s, and a 1.2 s
//! wave on the first input after a doze or sleep.
//!
//! Pure timing: the app feeds it input times and asks for the pose and the
//! next moment it changes by itself (one `SetTimer` one-shot, no poll).

/// No input for this long: the eyes doze.
pub const DOZE_AFTER: f64 = 8.0;
/// The awake-to-doze transition, then a static frame.
pub const DOZE_LEN: f64 = 0.6;
/// No input for this long: asleep, a static frame until input.
pub const SLEEP_AFTER: f64 = 20.0;
/// The wave on coming back.
pub const WAVE_LEN: f64 = 1.2;
pub const BLINK_LEN: f64 = 0.12;
/// Gaps between blinks while awake, cycled: all within 4-6 s, uneven so it
/// doesn't read as a metronome.
const BLINK_GAPS: [f64; 4] = [4.6, 5.8, 4.0, 5.2];
const BLINK_CYCLE: f64 = 4.6 + 5.8 + 4.0 + 5.2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pose {
    Open { blink: bool },
    /// 0..1 through the doze transition; 1 = settled.
    Doze(f32),
    Sleep,
    /// 0..1 through the wave.
    Wave(f32),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Mascot {
    /// Nobody watching or battery saver (`App::locked`, `App::display_off`,
    /// `st.power.saver`): no blink, transition or wave, and no wakes.
    pub still: bool,
    /// Blinks count from here (the last time it woke up).
    awake_at: f64,
    last_input: f64,
    wave_at: Option<f64>,
}

impl Mascot {
    /// Fresh and awake, no wave (the island just came back to rest).
    pub fn reset(&mut self, now: f64) {
        self.awake_at = now;
        self.last_input = now;
        self.wave_at = None;
    }

    pub fn last_input(&self) -> f64 {
        self.last_input
    }

    /// The person did something: a doze or sleep ends in a wave.
    pub fn input(&mut self, now: f64) {
        if matches!(self.pose(now), Pose::Doze(_) | Pose::Sleep) {
            let still = self.still;
            self.wave_at = (!still).then_some(now);
            self.awake_at = if still { now } else { now + WAVE_LEN };
        }
        self.last_input = now;
    }

    pub fn pose(&self, now: f64) -> Pose {
        if let Some(w) = self.wave_at.filter(|_| !self.still) {
            if now >= w && now < w + WAVE_LEN {
                return Pose::Wave(((now - w) / WAVE_LEN) as f32);
            }
        }
        let idle = now - self.last_input;
        if idle >= SLEEP_AFTER {
            Pose::Sleep
        } else if idle >= DOZE_AFTER {
            Pose::Doze(if self.still { 1.0 } else { ((idle - DOZE_AFTER) / DOZE_LEN).min(1.0) as f32 })
        } else {
            Pose::Open { blink: !self.still && self.blink_at(now).is_some_and(|b| now < b + BLINK_LEN) }
        }
    }

    /// Frames needed every refresh (the doze transition, the wave).
    pub fn moving(&self, now: f64) -> bool {
        matches!(self.pose(now), Pose::Wave(_)) || matches!(self.pose(now), Pose::Doze(t) if t < 1.0)
    }

    /// The next time the pose changes by itself. `None` while still or
    /// asleep: the frame stays until input.
    pub fn next_wake(&self, now: f64) -> Option<f64> {
        if self.still {
            return None;
        }
        match self.pose(now) {
            Pose::Sleep => None,
            Pose::Doze(_) => Some(self.last_input + SLEEP_AFTER),
            Pose::Wave(_) => Some(self.wave_at? + WAVE_LEN),
            Pose::Open { blink } => {
                let b = self.blink_at(now).unwrap_or(f64::INFINITY);
                let edge = if blink { b + BLINK_LEN } else { self.next_blink(now) };
                Some(edge.min(self.last_input + DOZE_AFTER))
            }
        }
    }

    /// The start of the blink under way or last begun at `now`.
    fn blink_at(&self, now: f64) -> Option<f64> {
        let since = now - self.awake_at;
        if since < BLINK_GAPS[0] {
            return None;
        }
        let base = self.awake_at + (since / BLINK_CYCLE).floor() * BLINK_CYCLE;
        let mut t = base;
        let mut last = base; // the cycle's end blink is the next cycle's start
        for g in BLINK_GAPS {
            t += g;
            if t > now {
                break;
            }
            last = t;
        }
        Some(last)
    }

    fn next_blink(&self, now: f64) -> f64 {
        let since = (now - self.awake_at).max(0.0);
        let mut t = self.awake_at + (since / BLINK_CYCLE).floor() * BLINK_CYCLE;
        for g in BLINK_GAPS.iter().cycle() {
            t += g;
            if t > now {
                break;
            }
        }
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn awake(now: f64) -> Mascot {
        let mut m = Mascot::default();
        m.reset(now);
        m
    }

    #[test]
    fn awake_blinks_dozes_then_sleeps_and_stays_still() {
        let m = awake(0.0);
        assert_eq!(m.pose(1.0), Pose::Open { blink: false });
        // First blink 4.6 s in, 120 ms long; the wakes land on its edges.
        assert_eq!(m.next_wake(1.0), Some(4.6));
        assert_eq!(m.pose(4.65), Pose::Open { blink: true });
        assert_eq!(m.next_wake(4.65), Some(4.6 + BLINK_LEN));
        assert_eq!(m.pose(4.8), Pose::Open { blink: false });
        // The next blink would be at 10.4, after the doze at 8.
        assert_eq!(m.next_wake(4.8), Some(8.0));
        assert_eq!(m.pose(8.3), Pose::Doze(0.5));
        assert!(m.moving(8.3) && !m.moving(8.7));
        assert_eq!(m.pose(15.0), Pose::Doze(1.0));
        assert_eq!(m.next_wake(15.0), Some(20.0));
        assert_eq!(m.pose(20.0), Pose::Sleep);
        assert_eq!(m.next_wake(30.0), None, "asleep: a static frame, no wake");
        assert!(!m.moving(500.0));
    }

    #[test]
    fn blink_gaps_stay_between_four_and_six_seconds() {
        let m = awake(0.0);
        let mut t = 0.0;
        for _ in 0..12 {
            let next = m.next_blink(t);
            assert!((4.0..=6.0).contains(&(next - t)), "{t} -> {next}");
            t = next;
        }
    }

    #[test]
    fn input_after_doze_or_sleep_waves_then_wakes() {
        for at in [9.0, 30.0] {
            let mut m = awake(0.0);
            m.input(at);
            assert_eq!(m.pose(at + 0.6), Pose::Wave(0.5));
            assert!(m.moving(at + 0.6));
            assert_eq!(m.next_wake(at + 0.6), Some(at + WAVE_LEN));
            // Another input mid-wave doesn't restart it.
            m.input(at + 0.3);
            assert_eq!(m.pose(at + 0.6), Pose::Wave(0.5));
            assert_eq!(m.pose(at + WAVE_LEN), Pose::Open { blink: false });
            // Blinks count from the end of the wave, the doze from the last input.
            assert_eq!(m.next_wake(at + 2.0), Some(at + WAVE_LEN + 4.6));
            assert_eq!(m.pose(at + 0.2 + DOZE_AFTER), Pose::Open { blink: false });
            assert!(matches!(m.pose(at + 0.4 + DOZE_AFTER), Pose::Doze(_)));
        }
    }

    #[test]
    fn input_while_awake_only_holds_off_the_doze() {
        let mut m = awake(0.0);
        m.input(5.0);
        assert_eq!(m.pose(5.1), Pose::Open { blink: false });
        assert_eq!(m.pose(12.0), Pose::Open { blink: false });
        assert!(matches!(m.pose(13.0), Pose::Doze(_)));
    }

    #[test]
    fn still_means_no_motion_and_no_wakes() {
        let mut m = awake(0.0);
        m.still = true;
        assert_eq!(m.pose(4.65), Pose::Open { blink: false });
        assert_eq!(m.pose(8.1), Pose::Doze(1.0), "the doze lands without its transition");
        assert!(!m.moving(8.1));
        assert_eq!(m.next_wake(1.0), None);
        // Coming back while still: awake at once, no wave.
        m.input(30.0);
        assert_eq!(m.pose(30.1), Pose::Open { blink: false });
        m.still = false;
        assert!(!m.moving(30.1));
    }
}

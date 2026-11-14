//! The system pop-ups' order: one at a time, never two for the same thing
//! in a row, at most a few waiting.
//!
//! Volume and the charger show at once, as they always did (a volume key
//! held down is one bar that moves). The others (Caps Lock, Wi-Fi, a
//! Bluetooth device) go through a [`Queue`]: shown at once on a free
//! island; while another pop-up shows they wait their turn, and a newer
//! word about the same thing (Caps Lock off after Caps Lock on, the same
//! headphones again) takes the older one's place instead of queueing behind
//! it. One that says what is already on screen is dropped.
//!
//! Also Wi-Fi's own rule ([`wifi_change`]): a pop-up when the PC joins or
//! leaves a Wi-Fi network, or moves to another, never for the internet
//! coming and going on the same network (that isn't Wi-Fi's news, and a
//! flaky line would spam it).

use std::collections::VecDeque;

/// What a newly offered pop-up should do.
#[derive(Clone, Debug, PartialEq)]
pub enum Offer<T> {
    /// Show it now.
    Show(T),
    /// It waits its turn.
    Wait,
    /// It says what is already on screen: nothing to do (keep that one up).
    Same,
}

/// Pop-ups waiting their turn.
#[derive(Clone, Debug)]
pub struct Queue<T> {
    waiting: VecDeque<T>,
}

impl<T> Default for Queue<T> {
    fn default() -> Self {
        Queue { waiting: VecDeque::new() }
    }
}

impl<T: Clone + PartialEq> Queue<T> {
    /// More than this waiting and the oldest goes: it would be old news.
    pub const MAX: usize = 3;

    /// Offer `item`. `showing`: the pop-up on screen now, if any. `slot`:
    /// whether two pop-ups are about the same thing.
    pub fn offer(&mut self, item: T, showing: Option<&T>, slot: impl Fn(&T, &T) -> bool) -> Offer<T> {
        if showing == Some(&item) {
            // A newer word about it that would have waited goes too.
            self.waiting.retain(|w| !slot(w, &item));
            return Offer::Same;
        }
        // The same thing as on screen, newer: it replaces it at once.
        if showing.is_some_and(|s| slot(s, &item)) {
            self.waiting.retain(|w| !slot(w, &item));
            return Offer::Show(item);
        }
        if let Some(i) = self.waiting.iter().position(|w| slot(w, &item)) {
            self.waiting[i] = item;
            return Offer::Wait;
        }
        if showing.is_none() {
            return Offer::Show(item);
        }
        self.waiting.push_back(item);
        while self.waiting.len() > Self::MAX {
            self.waiting.pop_front();
        }
        Offer::Wait
    }

    /// The next to show, once the island is free.
    pub fn next(&mut self) -> Option<T> {
        self.waiting.pop_front()
    }

    pub fn len(&self) -> usize {
        self.waiting.len()
    }

    pub fn is_empty(&self) -> bool {
        self.waiting.is_empty()
    }

    /// Nobody can see the island (locked, display off): what waited is old news.
    pub fn clear(&mut self) {
        self.waiting.clear();
    }
}

/// Where Wi-Fi stands, as Windows says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wifi {
    /// Joined to a Wi-Fi network.
    pub connected: bool,
    /// Its name, when Windows lets apps read it (Windows 11 asks for the
    /// location permission first): None otherwise, never a guess.
    pub ssid: Option<String>,
    /// That network reaches the internet (Windows' own check). Connected
    /// to Wi-Fi is not the same thing.
    pub internet: bool,
}

/// What a Wi-Fi change is worth saying: joined, left, or another network.
pub fn wifi_change(before: &Wifi, now: &Wifi) -> Option<Wifi> {
    match (before.connected, now.connected) {
        (false, true) => Some(now.clone()),
        (true, false) => Some(Wifi { connected: false, ssid: before.ssid.clone(), internet: false }),
        (true, true) if before.ssid.is_some() && now.ssid.is_some() && before.ssid != now.ssid => Some(now.clone()),
        _ => None,
    }
}

/// Caps Lock as the island follows it, from the keyboard's raw input: the
/// state it started from (Windows' word, read once as listening starts) and
/// a flip for each press let go. Deterministic: no read of another thread's
/// key state at the moment of the press, which Windows doesn't keep current
/// for an app in the background. Only the one key is looked at; nothing is
/// kept of the rest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CapsLock {
    /// Its state, once known.
    pub on: Option<bool>,
    /// The key is held down (repeats don't count).
    down: bool,
}

impl CapsLock {
    pub const VK_CAPITAL: u16 = 0x14;

    /// Start from `on` (Windows' word as listening starts).
    pub fn seeded(on: bool) -> Self {
        CapsLock { on: Some(on), down: false }
    }

    /// One raw key event. `Some(state)` when Caps Lock was let go after a
    /// press: its new state.
    pub fn key(&mut self, vkey: u16, released: bool) -> Option<bool> {
        if vkey != Self::VK_CAPITAL {
            return None;
        }
        let was_down = std::mem::replace(&mut self.down, !released);
        if !(released && was_down) {
            return None;
        }
        let on = !self.on?;
        self.on = Some(on);
        Some(on)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (thing, state): the same thing when the names match.
    fn slot(a: &(&str, u8), b: &(&str, u8)) -> bool {
        a.0 == b.0
    }

    #[test]
    fn one_at_a_time_never_twice_and_never_more_than_a_few() {
        let mut q: Queue<(&str, u8)> = Queue::default();
        // A free island: at once.
        assert_eq!(q.offer(("caps", 1), None, slot), Offer::Show(("caps", 1)));
        // Showing Caps Lock on: Wi-Fi waits, Caps Lock on again is the same.
        assert_eq!(q.offer(("wifi", 1), Some(&("caps", 1)), slot), Offer::Wait);
        assert_eq!(q.offer(("caps", 1), Some(&("caps", 1)), slot), Offer::Same);
        // Caps Lock off replaces what's on screen at once.
        assert_eq!(q.offer(("caps", 0), Some(&("caps", 1)), slot), Offer::Show(("caps", 0)));
        // Wi-Fi again while it waits: the newer word takes its place.
        assert_eq!(q.offer(("wifi", 0), Some(&("caps", 0)), slot), Offer::Wait);
        assert_eq!(q.len(), 1);
        assert_eq!(q.next(), Some(("wifi", 0)));
        assert_eq!(q.next(), None);
        // A burst of different things: the oldest beyond the limit goes.
        for (i, name) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            q.offer((name, i as u8), Some(&("volume", 0)), slot);
        }
        assert_eq!(q.len(), Queue::<(&str, u8)>::MAX);
        assert_eq!(q.next(), Some(("c", 2)));
        q.clear();
        assert!(q.is_empty());
    }

    #[test]
    fn a_waiting_word_that_matches_the_screen_goes() {
        let mut q: Queue<(&str, u8)> = Queue::default();
        // Caps Lock on shows; Wi-Fi shows over it later (a volume bar in between)...
        q.offer(("caps", 0), Some(&("volume", 0)), slot);
        // ...then Caps Lock back to off while "off" is on screen: nothing to add.
        assert_eq!(q.offer(("caps", 0), Some(&("caps", 0)), slot), Offer::Same);
        assert!(q.is_empty(), "the stale waiting one went too");
    }

    #[test]
    fn wifi_says_joins_leaves_and_moves_never_internet_flaps() {
        let off = Wifi::default();
        let home = Wifi { connected: true, ssid: Some("Home".into()), internet: true };
        let home_no_net = Wifi { internet: false, ..home.clone() };
        let cafe = Wifi { connected: true, ssid: Some("Cafe".into()), internet: false };
        let hidden = Wifi { connected: true, ssid: None, internet: true };
        assert_eq!(wifi_change(&off, &home), Some(home.clone()));
        assert_eq!(wifi_change(&home, &off), Some(Wifi { connected: false, ssid: Some("Home".into()), internet: false }));
        assert_eq!(wifi_change(&home, &cafe), Some(cafe.clone()));
        // The internet going on the same network is not Wi-Fi's news.
        assert_eq!(wifi_change(&home, &home_no_net), None);
        assert_eq!(wifi_change(&home_no_net, &home), None);
        // A name Windows stopped telling is no move.
        assert_eq!(wifi_change(&home, &hidden), None);
        assert_eq!(wifi_change(&off, &off), None);
    }

    #[test]
    fn caps_lock_flips_once_per_press_and_ignores_every_other_key() {
        let mut c = CapsLock::seeded(false);
        assert_eq!(c.key(0x41, false), None);
        assert_eq!(c.key(0x41, true), None);
        // Pressed and held (repeats): nothing yet; let go: on.
        for _ in 0..3 {
            assert_eq!(c.key(CapsLock::VK_CAPITAL, false), None);
        }
        assert_eq!(c.key(CapsLock::VK_CAPITAL, true), Some(true));
        // A stray release (it was down before listening began): nothing.
        assert_eq!(c.key(CapsLock::VK_CAPITAL, true), None);
        // Again: off.
        c.key(CapsLock::VK_CAPITAL, false);
        assert_eq!(c.key(CapsLock::VK_CAPITAL, true), Some(false));
        // Not seeded: no state to say, never a guess.
        let mut unknown = CapsLock::default();
        unknown.key(CapsLock::VK_CAPITAL, false);
        assert_eq!(unknown.key(CapsLock::VK_CAPITAL, true), None);
    }
}

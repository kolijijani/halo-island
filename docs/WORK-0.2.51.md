# Work Order — Halo Island 0.2.51

Encik Koliji's order, 16 Nov 2026. Split into CC CLI sessions so no single
session runs unbounded. Each session reads this file first, does its own
scope, marks its items DONE, and never re-does another session's work.

## Version

Target `0.2.51` (owner wrote "0.2.5.1"). Current is `0.2.50`.

## Items

| # | Item | Session | Status |
|---|------|---------|--------|
| 1 | Version bump 0.2.50 → 0.2.51 | S1 | DONE |
| 2 | Move "In full-screen apps" menu into Advanced settings | S1 | DONE |
| 3 | Remove 8 tray-menu entries (hidden-icon menu) | S1 | DONE |
| 4 | Remove "Every setting" menu in Advanced settings | S1 | DONE |
| 5 | License rework — L1, L2, R1 + seat model | S2 | DONE |
| 6 | Redesign Today + Widgets sections (wow + animation) | S3 | DONE |
| 7 | Delete local branch `koliji-requests-2026-11` | cleanup | done |

### S1 notes
- Advanced › Island already had the full-screen rows. The game rows now show
  without Pro too (labelled "(Halo Pro)", as the tray had them); `pro_gate`
  still keeps the readout off without Pro.
- Windows no longer has any way to set the Apple Music token (the tray
  submenu was its only one). A token saved before keeps working. Owner's
  Q6.2 wanted it in Advanced › Music; that is not done.
- `INVENTORY` + its test stay (`#[cfg(test)]`) to keep
  `docs/advanced-settings.md` correct (owner's Q4.2). README menu paths updated (Q6.4).
- Checked with `cargo wcheck` (Windows target): no errors, same 10 warnings
  as before. `cargo mcheck` can't build on this Linux box (aws-lc-sys needs
  Apple's cc). Tests not run (Windows-only).

### S2 notes
- Server: redeeming from another install moves the seat (`seat` + 1,
  audited `seat_move`); no more `409 already_used`. `/api/status` says
  `revoked` to an install that lost the seat, and to an older `seat`. Trial
  days count from the first redemption, not reset by a move.
- R1: codes kept sealed (AES-256-GCM, key derived from the signing key);
  admin "View key" (POST + CSRF, audited `view_key` by license id). Keys
  made before this have only the hash and can't be shown.
- L2: app asks `/api/status` once a day while a server license is on, on
  the existing minute timer (no new timer); skips while locked / display
  off / battery saver; an hour retry on no answer. `revoked` deletes the
  token, Pro off, badge REVOKED, one notice.
- L1: `license.bin` + `license-device.bin` moved to `%APPDATA%\Halo`
  (moved once at start from the old cache path). Uninstall keeps them, so a
  reinstall is Pro on the same seat; the token is kept because the server
  decides the seat.
- `cargo test -p halo-core license` 5/5, `node --test` 8/8, `cargo wcheck`
  no errors, same warnings as before. Windows-only tests not run here.

## Details

### 1 — Version bump
`Cargo.toml` (`version = "0.2.50"`), `installer/halo.nsi`
(`!define VERSION`, `VIProductVersion`), `installer/halo.iss`
(`#define AppVersion`), plus any other literal `0.2.50`.

### 2 — "In full-screen apps" into Advanced settings
The tray submenu in `crates/halo/src/tray.rs` (MenuCmd::SetHideFullscreen,
ToggleGameStats, ToggleGamePeek) moves into the Advanced settings window
(`crates/halo/src/settings_window.rs`). Keep behaviour identical.

### 3 — Remove tray-menu entries (hidden-icon menu, `tray.rs::menu_items`)
Remove exactly these 8, keep everything else (Advanced settings, Keep island
open, Sounds, What's New, About, Halo Pro, Quit, etc.):
- `Theme` submenu
- `Idle look` submenu (owner wrote "idle lock")
- `Halo Mascot`
- `Synced lyrics`
- `Apple Music lyrics` submenu
- `Start with Windows`
- `Open data folder`
- `Lyrics by LRCLIB`

### 4 — Remove "Every setting" menu
`crates/halo/src/settings_window.rs` `Section::Every` ("Every setting") entry
must go.

### 5 — License rework (session S2, see below)

### 6 — Redesign Today + Widgets (session S3, see below)

### 7 — Delete branch
Local branch `koliji-requests-2026-11` (contains Oyenza references). Local
only, never pushed. Deleted.

## License rework (S2) — owner's requirement, verbatim intent

Owner: "license tak bind ikut limit pc, tapi hanya boleh guna untuk 1 pc dalam
satu masa. jika pc kedua guna, license akan auto kick license yang pertama.
license tak bind pada pc id, license boleh digunakan mengikut masa, trial /
permenant. dan jika user uninstall halo island."

Translated spec:
- License is NOT bound to a fixed set of PCs / not bound to a device id.
- One license = ONE active seat at a time.
- A second PC activating the same license auto-kicks the first PC (first PC
  loses Pro on its next check; server records the new active seat).
- License lifetime is time-based: `trial` (expiry date) or `permanent`
  (no expiry).
- Uninstalling must NOT destroy the license — the user can re-activate the
  same license on a fresh install without a new code.

Combined with backlog items:
- **L1** — uninstall currently `RMDir /r "$LOCALAPPDATA\Halo"` which deletes
  `license.bin` (DPAPI-sealed token) + `license-device.bin` (install id). Fix:
  persist the license across uninstall (e.g. `%APPDATA%\Halo`), remove the
  token on uninstall only if that is what "permanent / trial by time" implies —
  the seat model means the server, not the token file, is the source of truth.
- **L2** — revoke does not reach the app (`revoked` hardcoded false, no
  `/api/status` poll). With a seat model this is mandatory: a light
  `/api/status` check while Pro is active (e.g. once a day) so a kicked/revoked
  seat drops promptly.
- **R1** — admin portal (`server/license-server.mjs`, tailnet :8790) must let
  the owner view a license key after creation. Currently only SHA-256 is stored
  (`codeHash`) and the code is shown once. Store the code (or owner-decryptable
  form) so keys are viewable.

## Redesign (S3) — Today + Widgets

Owner: redesign the "today" section and the "widgets" section in the notch.
Wants a "wow" effect, beautiful, with animation on every card and section.

Files: `crates/halo/src/pages/today.rs`, `crates/halo/src/pages/widgets.rs`
(and whatever pill/render helpers they use). Match the existing Halo Island
theme; do not break existing behaviour (news fetch, water/weather widgets,
entrance animation). Each card and each section gets its own animation.

### S3 notes
- Only drawing changed in `pages/today.rs` and `pages/widgets.rs`. Data, fetches, clicks and layout heights (200 / 196) are the same.
- New shared pieces in `pages/entrance.rs`: `alive` (ambient motion for 10 s after a page opens, easing to rest over the last 2 s, at ~30 fps via `request_live`, so it pauses while locked or the display is off), `hover` (a 0.16 s hover fade-in), `shine` (one light sweep across a card) and `still`. New fields in `PageUiState`: `hover`, `water_tap` and `today_scroll_shown`.
- Today: an accent rule under the date, the weekend columns and this week's band, today's disc with a halo that breathes, a feed dot that pings, stories as cards that rise in with a staggered accent bar, a hover outline and arrow, a press dip, a wheel scroll that glides, a scroll thumb, and a placeholder skeleton while the news loads.
- Widgets: tiles get a colour glow, a top highlight and a shine sweep. Each tile gets a drawing, shown only where the words leave room: a sun or cloud with rain or snow for the weather, clock hands that sweep round, a calendar leaf, a timer icon, and the moon on a halo. The Weather tile has a hover outline and pin, and a press dip. The water card gets water inside the ring, a cyan wash, a ripple and spring settle on −/+, a glow behind +, and an outline once the goal is reached. Clipboard rows rise in staggered and fade in on hover with a copy icon.
- Cost: nothing new at rest. Opening Today or Widgets now adds ambient frames (~30 fps) for 10 s, and none under reduced motion or battery saver. A hover or tap asks for frames for under 0.8 s.
- Not done: self-test pictures of Today and Widgets are taken once the entrance ends (1.1 s), while the ambient motion is still on. If a scene compares pixels exactly, it needs a wait or the motion turned off during the self-test. Fixed in S3b.

### S3b note
- The self-test now turns off the ambient motion: `AppState::in_selftest` (set in `App::new` from the self-test flag) makes `entrance::alive` return `None`, so no halo breath, feed ping, water sway or weather sky runs after the entrance. (`shine` and the clock sweep run on the entrance clock, so they are done by the time the entrance settles.) The entrance still runs, and `selftest_settled` still waits for it (`entrance::moving`), so Today and Widgets pictures are taken at rest. Hover, `still`, `shine` and `Entrance::of` are unchanged.

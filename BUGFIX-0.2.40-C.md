# Work order C: items 8-9 of 0.2.40, then the release path

Same rules as `BUGFIX-0.2.40.md` and `BUGFIX-0.2.40-B.md`: grill-me rounds into `docs/GRILL-0.2.40.md` first, smallest diff, `AGENTS.md`, English prose, `cargo test` + `cargo check` before you finish.

## Item 9 — a choice of adzan voices

Today `PrayerSound` is `Adzan | Tone | Silent` (`crates/halo-core/src/settings.rs`), one recording: `adzan.m4a`, "The Adhan — Aaqib Azeez" by Atcovi, Wikimedia Commons, CC BY-SA 4.0, prepared as `addons/sholat/NOTICE.txt` says. `services/adzan.rs` plays it with WinRT, one at a time, and Subuh falls back to the tone because that recording isn't Subuh's adhan.

What the owner wants: several voices to choose from, the way a phone's prayer app does.

Design constraints that are not negotiable:

- **Licence first.** Only recordings we may redistribute: CC BY-SA, CC BY, CC0 or public domain, each with its source URL and licence in `addons/sholat/NOTICE.txt` and the add-on's README. CC BY-SA recordings stay CC BY-SA (share-alike). Search Wikimedia Commons (it already gave us the first one) and pick 2-3 that are clean, complete and clearly a different muezzin — Haram/Makkah and Masjid an-Nabawi/Madinah if they exist under a free licence, plus a Subuh recording (with the "as-salatu khairum min an-naum") so Subuh stops falling back to a tone. Convert with ffmpeg the same way (`tools/build-ffmpeg-min.sh`, mono, rumble below 70 Hz out, loudness levelled, AAC ~80 kbit/s) and write down what each became.
- **Big files are add-ons.** Each voice is its own file, downloaded only when someone picks it (`addons/`, `services/addons.rs`), never `include_bytes!`. A new tag (`addon-sholat-4`) carries them; never delete or change `addon-sholat-1/2/3` — older app versions download those.
- **Settings that save by name keep working.** `PrayerSound` is serialised (`"adzan"`, `"tone"`, `"silent"`); a new variant must load old settings unchanged, and a voice deleted from the add-on must fall back to the tone instead of failing silently.
- The picker lives where the sound is chosen today (`pages/prayer.rs` and the Settings window's Prayer tab, plus the Mac path that shares the file): each voice with its name and who recites it, a play/preview action that uses the existing one-at-a-time player, and a line saying what it costs to download.
- Nothing at rest: no streaming, no prefetch of a voice nobody chose, no polling.

If a candidate recording's licence is even slightly unclear, drop it and say so under "Needs the owner" — do not ship a maybe.

## Item 8 — version 0.2.40

After items 1-7 and 9 are in: the workspace `Cargo.toml` version to `0.2.40`, every entry in `Cargo.lock` for `halo`, `halo-core`, `studio-usage`, `studio-diorama` (CI's `cargo fetch --locked` fails if one is left), `docs/release-notes/v0.2.40.md` written in the repo's own voice from what this release actually changes (all nine items), and the website's fallback version `<span data-release="version">` + the "New in 0.2.40" band's five cards in `site/index.html` following `AGENTS.md`. Leave `?v=` alone unless the share card really changes.

Do not commit a "Release 0.2.40" commit with a tag, do not push, do not publish anything: the owner says when a release is cut. Prepare the tree so a tag is the only step left, and list what a release would publish in your report.

## Finish

- `cargo test --workspace` (the parts that run on Linux) and `cargo check`.
- A short report: per item, what changed and which file; the tests that passed; what a Windows build still needs to prove (screenshots of the Weather widget, the AI usage balance, Stocks with live prices, Advanced settings, What's New, the Halo Pro page, the adzan picker); and everything under "Needs the owner".

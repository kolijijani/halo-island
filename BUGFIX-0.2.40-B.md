# Work order B: items 4-7 of 0.2.40

Continue from `BUGFIX-0.2.40.md` (read it first, and `docs/GRILL-0.2.40.md` if it exists). Same rules: grill-me rounds written down first, then the smallest diff that fixes it, `AGENTS.md` in force, repo prose in English, no release tag and no `site/` release fallback changes, and `cargo test` + `cargo check` before you finish.

## Item 4 — the download part 404s

What I measured on 14 Nov 2026:

```
gh release list -R kolijijani/halo-island      -> empty
GET api.github.com/repos/kolijijani/halo-island/releases -> []
```

The repository has **no GitHub release at all**, so every URL the app and the site build from `releases/...` 404s:

```
https://github.com/kolijijani/halo-island/releases/download/addon-sholat-3/adzan.m4a -> 404
https://github.com/kolijijani/halo-island/releases/download/addon-ja-1/ja-readings.dic.zst -> 404
https://github.com/kolijijani/halo-island/releases/download/addon-downloader-1/yt-dlp.exe -> 404
https://github.com/kolijijani/halo-island/releases/latest/download/Halo-Island-Setup.exe -> 404
```

`crates/halo-core/src/addons.rs` pins `addon-sholat-3` (places.json, adzan.m4a, NOTICE.txt), `addon-ja-1` (ja-readings.dic.zst, NOTICE.txt) and `addon-downloader-1` (yt-dlp, QuickJS, ffmpeg). The Sholat files are already in the repo (`addons/sholat/`), the Japanese dictionary is built by `tools/make_ja_dict.py`, the downloader tools by the pins in `tools/pin_ytdlp.py` / `docs/downloader-pins.md`.

So: the fix is to **publish the add-on pre-releases with exactly those assets and the sizes and SHA-256 the code pins**, and to say plainly in the UI when an add-on isn't published yet instead of showing a bare 404 as if the network failed. Write the publish path down as a tool (one script under `tools/`, `gh release create --prerelease`, assets + `sha256sum` check against `addons.rs`), create the three add-on pre-releases, and verify each URL answers 200 with a byte-identical file. Never change a pinned SHA-256 to match a file you just made: if a file doesn't match its pin, stop and say so.

The app's own release (`releases/latest/download/Halo-Island-Setup.exe`) is a tag away but publishing it is the owner's call: prepare it, don't push it.

## Item 5 — Advanced settings popup: list everything

The Settings window (`crates/halo/src/settings_window.rs`) is the "Advanced settings" window the island's last row opens. The owner wants the popup to **list every item it needs and that this system can actually show** — an inventory, not a new screen of toggles.

Do it as: read every tab and row the window really has today (and every row that depends on something else: Pro, an add-on, a Windows feature, a Mac-only path), and put an itemised list in the popup/page: each item's name, which tab it lives on, and its state on this machine — on, off, needs Pro, needs an add-on, not available on this build. Facts from the code only; invent nothing. Keep it one text-first list in the existing sheet/popup style, no new window, no new thread, no polling (read the states that are already in `AppState`).

Also write the same inventory into `docs/advanced-settings.md` so the list and the code can be checked against each other, and add a test that every row drawn in the window appears in the inventory (and the reverse).

## Item 6 — What's New: the wow list

`pages::settings::whats_new()` is seven lines today (Today's top stories, Halo the mascot, offline states, synced lyrics, Caps Lock/Wi-Fi pop-ups, the Mireld key, Halo Pro) and a test caps it at seven lines.

The owner wants every top feature in, **including the FPS game and the rest**, with the most impressive ones up front so the first three lines alone feel like a wow. So: inventory what this build actually ships (read `docs/`, the pages, `ARCHITECTURE.md`, the Pro features, the FPS/game stats page, Diorama, Glass/Pill themes, the Downloader, AI usage, Stocks, Today, Script, paired phone drop, add-ons), then write the sheet: the strongest 5-7 lines first, then the rest in a compact second block. Order by what a new user sees and feels first, not by module. If the seven-line cap gets in the way, raise the cap deliberately in the test and say why — the cap exists so the sheet stays readable, so whatever replaces it must still fit the sheet (no scrolling text wall).

Keep the emoji lead on every line (there is a test for it) and keep out the words that test forbids.

## Item 7 — Halo Pro page: a real license page

There is a whole license path already: `crates/halo/src/services/license.rs`, `crates/halo-core/src/license.rs` (`Grant` with `kind` `pro`/`trial`, expiry, the device hash, revocation), `server/license-server.mjs`, `docs/licenses.md`. The owner wants the Halo Pro page to **show the license like an account**: not a badge.

Design it from the facts the token actually carries:

- state: free / licensed / trial / expired / revoked / this build has no license server;
- plan kind, and the expiry as a live countdown ("ends in 23 days, 4 hours") plus the date;
- this PC's bind: the device id's short hash and its label, since the license is bound to it (`license-device.bin`, only the SHA-256 leaves the PC) — say in plain words that the code works on this PC and what Deactivate does;
- seats/limits if the grant carries any; the key's prefix if it is known to the app;
- the actions the app really has: redeem a code, copy the device id, deactivate here, open the portal.

Nothing at rest: the countdown is drawn while the page is open, from the clock the island already ticks (`App::unix`), and stops when the page closes. No new thread, no polling a server. If a field the owner asked for isn't in the token or the server's answer, don't invent it: say what is missing under "Needs the owner" in `docs/GRILL-0.2.40.md` and show what you do have.

Update `docs/licenses.md` with the page's fields, and add tests in `halo-core::license` for the countdown wording and the states (expired, trial, revoked).

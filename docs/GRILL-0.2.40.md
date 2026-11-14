# GRILL — 0.2.40 (owner's decisions)

The three work orders for 0.2.40 — `BUGFIX-0.2.40.md` (items 1–3),
`BUGFIX-0.2.40-B.md` (items 4–7) and `BUGFIX-0.2.40-C.md` (items 8–9) — each
ask the same set of questions. The owner answered them all in one pass; this
page records those answers so the work is a matter of record.

## Decisions

1. **Weather — IP fallback:** use `ipwho.is` (HTTPS, no key), 10 s timeout.
   Stop asking when the weather widget is off, and label the result as an
   IP-based estimate.
2. **Weather — city picker:** reuse the existing `places.json` prayer
   locations for the manual city choice; no new data set.
3. **Weather — offline:** show the last reading and its age when offline.
4. **AI usage / Mireld balance:** show everything the API returns that can be
   displayed — AI usage, remaining token balance, total tokens used, and every
   other available field. Show the full balance; never hide or truncate it.
   The key's prefix and its state stay visible.
5. **Stocks provider:** Yahoo Finance chart API, keyless. When rate-limited,
   show "market closed/limited", not a spinner.
6. **Stocks search:** focus on Malaysia (Bursa MY) for the bundled/browsed
   list, and allow searching any stock worldwide. Provider and search must
   support MY-focused browsing plus free search for any ticker.
7. **Add-on publishing:** prepare everything — `tools/publish_addons.py`
   (`gh release create --prerelease`, asset + sha256sum check against
   `crates/halo-core/src/addons.rs`), assemble every asset for
   `addon-sholat-3`, `addon-sholat-4`, `addon-ja-1`, `addon-downloader-1` and
   `addon-downloader-mac-1`, and verify each asset's size and SHA-256 against
   the code pins locally. Do not run `gh` from the render node; the owner's
   side publishes from the VPS. Never change a pinned SHA-256 to match a file
   just made.
8. **Advanced settings:** itemised inventory of every row/tab the Settings
   window really draws, each item's name, tab and state on this machine, in
   the existing sheet style. Write `docs/advanced-settings.md` and add a test
   that every drawn row appears in the inventory and the reverse.
9. **What's New:** top lines — FPS game stats, Diorama, paired phone drop,
   Glass/Pill themes, Downloader; the rest in a compact second block. Keep the
   emoji lead on every line and keep the sheet readable.
10. **Halo Pro page:** make the key type obvious at a glance (trial vs pro
    permanent as a visually distinct badge/label), a live countdown for trial
    keys ("ends in X days, X hours") with the date, plus the rest of the item
    (state, device bind hash and label, what Deactivate does, the real
    actions). No new thread and no server polling: draw from `App::unix` while
    the page is open. No daily `/api/status` poll.
11. **Adzan voices:** three new voices — Makkah, Madinah, and a Subuh recording
    with "as-salatu khairum min an-naum" — each its own add-on file downloaded
    only when picked, licensed clearly in `addons/sholat/NOTICE.txt` and the
    add-on README, on the `addon-sholat-4` tag. Drop any recording whose
    licence is unclear and say so under "Needs the owner".
12. **Version 0.2.40:** prepare only — workspace `Cargo.toml` to 0.2.40, every
    matching `Cargo.lock` entry, `docs/release-notes/v0.2.40.md` in the repo's
    voice, site fallback version span and the 5 "New in 0.2.40" cards. Do not
    tag, push or publish anything.

## Needs the owner

- **Mac Downloader (`addon-downloader-mac-1`):** `qjs` and `ffmpeg` come from
  the `downloader-tools-mac` CI artifact; downloading it needs a token, so the
  tag waits.
- **Adzan voices:** no Masjid an-Nabawi (Madinah) recording exists on
  Wikimedia Commons, so Madinah is dropped. The 2018 Makkah recording was also
  left out because it starts mid-takbir.
- **Stocks:** Yahoo was checked from the render node; the home PC's IP could
  not be reached over SSH, so the owner should confirm from home.
- **Real Windows screenshots** are still wanted for Weather, the Mireld
  balance, Stocks, Every setting, What's New, Halo Pro and the adzan picker.

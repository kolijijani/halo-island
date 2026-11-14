# Work order: 9 owner-reported bugs for 0.2.40

From Encik Koliji (owner), 14 Nov 2026, verbatim list:

1. Weather widget still shows nothing — an error, "can't reach it". Can it take weather from Google and follow the location?
2. Mireld AI key: the key goes in and it detects the model count, but it never detects tokens used / unused. Check.
3. Stocks tab still doesn't work: no search, no live display.
4. The download part gives a 404. Fix.
5. Advanced settings popup: list every item it needs and can show on our system.
6. What's New: put every top feature in, including the FPS game and the rest. Pick the most impressive ones to highlight, so the user feels a wow.
7. The Halo Pro menu: create a license timer/status and detail the page. If a bind on the user's PC is needed, build it too, so the user knows everything in detail.
8. Bump the software version to 0.2.40.
9. Add a choice of adzan voices.

## How to work (do this in order)

1. **Read first**: `AGENTS.md`, `CLAUDE.md`, `ARCHITECTURE.md`, `docs/ai-usage.md`, `docs/downloader.md`, `docs/licenses.md`, `docs/updates.md`, and the files each item touches.
2. **Grill-me rounds before code** (skill `grill-me`, pasted at the end of this file). Model each item as a design tree, ask the whole frontier in rounds, answer each with a recommendation **and** the evidence from the code you read. You cannot talk to the owner mid-run, so: record every question, your recommended answer, and the one-line why in `docs/GRILL-0.2.40.md`, then treat the recommendation as the decision. Flag in that file, under "Needs the owner", only decisions that spend money, publish a release, or change what people see on a paid feature — those you do **not** decide and do **not** start.
3. **Then implement**, following `AGENTS.md` exactly: nothing runs at rest unless switched on, no new crate without a reason, every new network call has a timeout/backoff and stops when nobody is there, and the diff stays as small as the fix allows.
4. **Verify**: `cargo test -p halo-core -p studio-usage` and `cargo check` on the render node must pass. Native Windows pixels come later on the PC — say so if you couldn't look.

Do not commit a release tag, do not push a release, do not touch `site/` release fallbacks for a version that isn't released yet.

---

## Item 1 — Weather widget: no reading, "can't reach it"

Facts I checked myself (14 Nov 2026):

- The reading is Open-Meteo, no key: `halo_core::halo::weather_url` → `https://api.open-meteo.com/v1/forecast?latitude=..&longitude=..&current=temperature_2m,weather_code`. That URL answers **HTTP 200** from a Linux host, so the provider is not down.
- `halo/src/services/weather.rs` maps *every* HTTP/DNS/TLS failure to `Failure::Unreachable` and every unparsable body to `Malformed`; a thread-spawn failure does the same. The widget therefore says "can't reach it" for a missing location too.
- Two askers: `app_halo.rs:159-166` (Widgets tab open, freshness gate) and `app.rs:4385-4391` ("The Weather widget asked where the PC is (no Prayer place): read it there").

**Google is not an option**: Google has no public weather API (the old `google.com/ig/api` is long dead, and the Weather on Google surfaces has no API). Say that in `GRILL-0.2.40.md` and take the location route instead.

What to fix:

- Find the real failure before changing providers: does `lat,lng` ever arrive (Windows `Geolocator` / whatever `app.rs:4385` uses)? If location fails or is refused, the widget must say *that* ("location off — pick a city"), not "can't reach it".
- Always get a reading: location chain = Windows location → last known place (prayer place / `follow_location`) → IP-based fallback (a no-key JSON geolocation service) → a low-cost city picker in the widget/settings. Cache the last good reading and show it (with its age) while offline, like `halo_core::net::Fetched` already does for other add-ons.
- Keep Open-Meteo as the source, keep it keyless, one ask with a 10 s timeout, retry with backoff, nothing at rest when the widget is off (`AGENTS.md`).

## Item 2 — Mireld: models detected, balance never shown

`crates/studio-usage/src/mireld.rs` reads `/v1/models` only, and says in its own header that a balance endpoint would be needed. **Mireld does publish one** — I called it with a real prepaid key on 14 Nov 2026:

```
GET https://api.mireld.my/v1/usage          Authorization: Bearer <key>
HTTP 200 {"object":"mireld.balance","unit":"credits","plan":"200M Token Credits / 30 Days",
 "credits_total":7375000000,"credits_used":6474910074,"credits_reserved":0,
 "credits_remaining":900089926,"credits_available":900089926,
 "expires_at":"2026-12-13T04:57:12.000Z","generated_at":"2026-11-13T18:04:34.097Z",
 "key":{"prefix":"sk-mD6O_Aj_G…","status":"active"},"usage_percent":87.8,
 "request_count":76399,"limits":{"rpm":40,"concurrency":10},
 "history":{"available":false,"reason":"Historical credit grants are incomplete for this key."}}
```

Two more that answer on the same key, useful as fallbacks:

```
GET https://api.mireld.my/v1/credits                     -> {"data":{"total_credits":7375,"total_usage":6474.910074}}
GET https://api.mireld.my/v1/dashboard/billing/credit_grants -> {"object":"credit_summary","total_granted":7375,"total_used":6474.910074,"total_available":900.089926,"grants":{"object":"list","data":[]}}
```

These do **not** exist for token plans (404 `unsupported_endpoint`): `/v1/me`, `/v1/key`, `/v1/balance`.

What to fix: read `/v1/usage` in `mireld.rs` after a successful `/v1/models`, add the fields to the `Snapshot` with a documented unit story (the `/v1/usage` numbers are 1/1_000_000 of the `/v1/credits` numbers — show tokens used, tokens left, the percent, the plan name, the expiry as a countdown, the key's status and prefix, `request_count`, and the rpm limit). Fall back to `/v1/credits` if `/v1/usage` fails; keep `Malformed` vs `Unreachable` honest. The AI usage page (`docs/ai-usage.md`) then shows a balance card. Key goes to `api.mireld.my` and nowhere else. Fixtures: add real-shaped JSON to `fixtures/mireld/`, no key material.

## Item 3 — Stocks: no search, no live prices

`crates/halo-core/src/stocks.rs` bases every URL on

```
pub const BASE: &str = "https://halo.local.invalid/stocks/live/";
```

A placeholder that was never deployed: `halo.trade` does not resolve at all (DNS gives nothing), so `names` (search) and `watch` (prices) both fail for every user. That is the whole bug — the page, the parsing and the arithmetic are fine.

What to fix: point `watch`, `names` and `chart` at a source that exists, without breaking the `HALO_STOCKS_URL` override and without inventing a backend we have to operate. Check these from **the Windows PC** (a home IP) before choosing — a datacenter IP gets rate-limited, which is not the same as "broken":

- `https://query1.finance.yahoo.com/v8/finance/chart/BBCA.JK?range=1d&interval=5m` — keyless, gives `meta.regularMarketPrice`, `meta.chartPreviousClose`, the day's points and (range=1y) the year of closes. I measured it from the home IP (render node, 14 Nov 2026): **HTTP 200** with a browser UA. From this VPS's datacenter IP the same call is **429** — so the source is fine, the IP was the problem. `range=1d&interval=5m` gives 5-minute points, which is exactly the day line the page wants; `range=1y&interval=1d` gives the year.
- `https://query1.finance.yahoo.com/v7/finance/quote?symbols=BBCA.JK` — **401**, that endpoint now needs a crumb; don't use it, take `chart`'s `meta` instead.
- `https://www.idx.co.id/primary/TradingSummary/GetStockSummary` — **403** (Cloudflare), so the exchange's own JSON is out.

Search needs the code/name list: bundle it (a few KB, an add-on file at most — never `include_bytes!` of something big) or fetch it from the same provider. Show the page's offline/limited state instead of a spinner that never ends, and keep the pacing rules already documented in `services/stocks.rs` (page open only, 15 s while IDX trades, back off on failures, sleep at zero CPU when the page closes).

If no keyless source survives the check from the PC, stop and write that under "Needs the owner" in `GRILL-0.2.40.md` with the options and their costs — do not ship a page that pretends to work.

## Items 4-9

Later rounds: item 4 (download 404), 5 (advanced settings list), 6 (What's New), 7 (Halo Pro license page), 8 (version 0.2.40), 9 (adzan voices). No code for those in this round.

---

## grill-me (the skill, verbatim from the library)

Stress-tests a plan through structured adversarial questioning before any code is written. Models the plan as a design tree — every decision branches into the decisions that hang off it — and interviews in rounds until every branch is resolved and nothing is silently assumed.

- The **frontier** is every decision whose prerequisites are already settled. Ask the whole current frontier in one round, numbered, each question carrying a recommended answer, then wait. A question whose answer depends on another open question belongs to a later round.
- Format per question: `Q — <title>: <body>` then `Recommendation: <answer + one-line why>`.
- Facts are your job, decisions are not: read the codebase for every fact instead of asking.
- Do not write code during the interrogation.
- Be adversarial: find what breaks the fix, what the worst case is, how it rolls back. If everything looks fine, look harder.
- When the frontier is empty, summarise every decision, list what is out of scope, and only then implement.

In this run: same rounds, written down in `docs/GRILL-0.2.40.md`, then implement the resolved branches.

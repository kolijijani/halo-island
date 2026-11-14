# Halo Pro licenses

Halo Pro unlocks in two ways:

- **The built-in key** (`halo_core::license::KEY_SHA256`): one key, checked on the PC against its SHA-256, no server. Unchanged.
- **A license code** from the license server: made by an admin for one person, redeemed once for one install, as Halo Pro (no end, or a set number of days) or trial Pro (always a set number of days). This page is about these.

## Pieces

| Piece | Where | What it holds |
|---|---|---|
| Admin portal and redeem API | `server/license-server.mjs` (Node 20+, no dependencies) | Licenses (`licenses.json`: code hashes, never codes), the audit log (`audit.jsonl`) |
| Signing key | a PEM file outside the repository, mode 0600 (`--key`) | The P-256 private key that signs license tokens. Never in git, the app, CI or a log |
| Trusted public keys | `crates/halo-core/data/license-keys.txt` | The public half, one line per key, built into the app |
| The app's license | `%LOCALAPPDATA%\Halo\license.bin` (DPAPI), on a Mac `~/Library/Application Support/Halo/license.bin` (owner-only) | The token. Also `license-device.bin`: the install's own random id, of which only a hash leaves the PC |

## Codes and tokens

- **Code:** `HALO-XXXXX-XXXXX-XXXXX-XXXXX`, 20 symbols from 31 (no 0, 1, I, L, O), about 99 random bits, made with `crypto.randomBytes`. Shown once in the portal; the server keeps only `SHA-256("halo-license/1 code\n" + code)`. A code not redeemed within 90 days lapses.
- **Install id:** 32 random bytes made once per install, sealed; the app sends `SHA-256("halo-license/1 device\n" + id)`.
- **Token:** `HL1.<payload>.<signature>`, both base64url. Payload: `{"v":1,"kid","lic","kind":"pro"|"trial","iat","exp":<unix s>|null,"dev"}`. Signature: ECDSA P-256 over SHA-256 of `HL1.<payload>`, r‖s (64 bytes). The app checks it offline with the platform's own crypto (BCrypt on Windows, the `p256` crate on a Mac, the same code as the update file): a trusted `kid`, the signature, `dev` equal to this install, not past `exp`. Anything else is Free. A test signs with Node and verifies with BCrypt so both sides stay in step.

## API (redeem listener)

`POST /api/redeem` `{"code","device"}` (JSON only):

| Status | Body | Meaning |
|---|---|---|
| 200 | `{"token","kind","exp"}` | Redeemed now, or the same install asking again (reinstall) |
| 400 | `{"error":"malformed"}` | Not a code or not a device hash |
| 404 | `unknown_code` | No such code |
| 403 | `revoked` | Revoked by an admin |
| 409 | `already_used` | Redeemed by another install |
| 410 | `lapsed` / `expired` | Not redeemed in time / its days are over |
| 429 | `busy` | More than 10 a minute from one address, or 300 in all |

`POST /api/status` `{"lic","device"}` → `{"state":"active"|"expired"|"revoked"|"unused"|"lapsed"}`. An unknown license, or another install's, answers `revoked`, so nothing is learned by guessing.

## Admin portal

- **Reachable only on the tailnet.** `--admin-host` must be a Tailscale address (100.64.0.0/10 or fd7a:115c:a1e0::/48) or loopback; the server refuses any other, and answers 403 to any peer outside those ranges. Behind `tailscale serve` (which gives it HTTPS on the tailnet), bind it to 127.0.0.1 and pass `--secure-cookie`.
- **Sign-in:** `LICENSE_ADMIN_TOKEN` (32+ characters, from the environment, never a flag), compared in constant time. 5 failed tries from one address block it for 15 minutes. Sessions last 8 hours, in an HttpOnly, SameSite=Strict cookie; every form carries a CSRF token.
- **Make keys:** Pro or trial Pro, days from redemption (empty: no end; a trial needs days), 1 to 50 at a time, a note. **Revoke** any license. **Audit:** every sign-in, failed sign-in, block, key made, revoke, redemption and reissue, with license ids and never a code, token or secret.
- Every page sends a strict Content-Security-Policy (no scripts), `no-store` and `no-referrer`.

## Running it

```
node server/license-server.mjs init-key --key /srv/halo/license-signing.pem
#   prints "<kid> <public point>": paste it as a line in crates/halo-core/data/license-keys.txt
LICENSE_ADMIN_TOKEN=$(openssl rand -base64 36) \
node server/license-server.mjs serve --key /srv/halo/license-signing.pem --data /srv/halo/license-data \
  --admin-host 100.x.y.z [--redeem-host 127.0.0.1 --redeem-port 8791 --trust-proxy]
```

Tests: `node --test server/test/license.test.mjs` (keys, redemption once per install, expiry, revoke, lapse, malformed input, the tailnet check, sign-in limits, CSRF, the audit, the redeem API's limits, init-key's file mode) and `cargo test -p halo-core license`, plus the Windows unit test `a_license_servers_token_verifies_here`.

## Settings › Halo Pro

The page shows the license like an account, from what this PC already holds: the token kept in `license.bin` (checked offline at start, after a redemption and after Deactivate) and the install id's hash. It asks no server to draw it.

| Field | Where it comes from |
|---|---|
| Badge | `halo_core::license::State`: **TRIAL** (orange), **PRO** from a license code or **PRO · PERMANENT** with the built-in key (yellow), **EXPIRED** or **REVOKED** (red), **FREE** (grey). A trial never looks like permanent Pro |
| The line under it | What the state means, in a sentence: Free, the built-in key, a license or trial bound to this PC, ended, revoked, or "this version has no license server" |
| Plan | The token's `kind`: Halo Pro or Trial Pro; with the key, "Halo Pro, built-in key" |
| Ends | The token's `exp` as a countdown, "ends in 23 days, 4 hours", then the date and time on this PC's clock; "No end (permanent)" without one. Drawn again on the island's minute timer while the page is open, nothing otherwise (`license::countdown`, `license::end_date`) |
| Started | The token's `iat` |
| License | The first 8 characters of the token's `lic` (the license id the server knows it by) |
| Key | With the built-in key: the key masked |
| This PC | The first 12 characters of the install id's hash (`SHA-256("halo-license/1 device\n" + id)`, the only part of the id that leaves the PC), in groups of four, and that the license works on this PC only |
| Seats | One PC per code: the token carries no seat count, and a code redeems for one install |

The actions are the ones the app has: type a code or the key and **Activate**, or **Deactivate**, which deletes the token (or the key) from this PC; the code stays used on the server, so the same install can redeem it again, another can't.

## What is not decided here (needs the maintainer)

- **Where the redeem API is public.** The app asks `HALO_LICENSE_URL` (https only); no address is built in, so this build redeems nothing and says "This version has no license server set up." Choosing a host, its TLS and putting the address into the build (or a release-time setting) is a deployment decision.
- **The signing key.** `license-keys.txt` is empty: until a key is made on the server and its public line committed, every token fails closed. Keep a second key offline for rotation, as `update-keys.txt` does.
- **Revocation reaching the app.** `/api/status` exists and the app's check takes a `revoked` flag, but the app doesn't ask it on a timer yet (it would be one request a day while Pro is on: a light-cost decision for AGENTS.md's rules). Until then a revoked license stops at its `exp`; give revocable people trials with days.
- **Backups** of `license-data/` (losing it loses who redeemed what, not the tokens already out).

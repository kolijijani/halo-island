#!/usr/bin/env node
// Halo Island license server (MIT): an internal admin portal that makes
// Halo Pro and trial Pro keys, and the redemption API the app asks.
// The contract is docs/licenses.md. Node 20+, no dependencies.
//
// Two listeners:
// - admin (always): the portal. It binds only to a Tailscale address
//   (100.64.0.0/10, fd7a:115c:a1e0::/48) or loopback (for `tailscale serve`),
//   refuses to start on any other, and answers only peers from those ranges.
//   An admin token (LICENSE_ADMIN_TOKEN) signs in; sessions are cookies with
//   a CSRF token; failed sign-ins are rate-limited; every action is audited.
// - redeem (off unless --redeem-host is given): POST /api/redeem and
//   /api/status for the app. Where it listens is a deployment decision.
//
// Keys: a code (HALO-XXXXX-XXXXX-XXXXX-XXXXX, 100 random bits), found by its
// SHA-256 and kept sealed with a key derived from the signing key, so the
// portal can show it again and licenses.json alone gives nothing away. A
// license is one seat: one install at a time. Redeeming it on another
// install moves the seat there (the `seat` number goes up) and the install
// it left reads `revoked` from /api/status. Each redemption gives a license
// token signed with the server's P-256 key, which the app verifies offline
// against the public keys built into it. The private key lives in a file
// outside the repository (--key, mode 0600), never in git or the app.
// The server never logs a code, a token, the admin token or a key.

import http from 'node:http';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import net from 'node:net';
import { fileURLToPath } from 'node:url';

export const LIMITS = Object.freeze({
  bodyMax: 8 * 1024,
  sessionMs: 8 * 3600_000,
  loginFailures: 5, // per address per window
  loginWindowMs: 15 * 60_000,
  redeemPerMinute: 10, // per address
  redeemGlobalPerMinute: 300,
  keysPerBatch: 50,
  maxDays: 3650,
  redeemWindowDays: 90, // a code not redeemed by then lapses
  noteMax: 120,
});

const CODE_ALPHABET = 'ABCDEFGHJKMNPQRSTUVWXYZ23456789'; // no 0/O, 1/I/L
const CODE_RE = /^HALO(-[A-Z2-9]{5}){4}$/;
const DEVICE_RE = /^[0-9a-f]{64}$/;
const LIC_RE = /^[0-9a-f]{32}$/;

const SECURITY_HEADERS = [
  ['Content-Security-Policy', "default-src 'none'; style-src 'self'; font-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'"],
  ['Referrer-Policy', 'no-referrer'],
  ['X-Content-Type-Options', 'nosniff'],
  ['Cache-Control', 'no-store'],
];

// ---------- addresses ----------

/** Whether `addr` is loopback or in Tailscale's ranges (IPv4-mapped too). */
export function isTailnetOrLoopback(addr) {
  if (typeof addr !== 'string' || !addr) return false;
  let a = addr.toLowerCase();
  if (a.startsWith('::ffff:') && net.isIPv4(a.slice(7))) a = a.slice(7);
  if (net.isIPv4(a)) {
    const [p, q] = a.split('.').map(Number);
    return p === 127 || (p === 100 && q >= 64 && q <= 127);
  }
  if (net.isIPv6(a)) {
    if (a === '::1') return true;
    const full = expandV6(a);
    return full.startsWith('fd7a:115c:a1e0:');
  }
  return false;
}

function expandV6(a) {
  const [head, tail = ''] = a.split('::');
  const h = head ? head.split(':') : [];
  const t = tail ? tail.split(':') : [];
  const mid = Array(8 - h.length - t.length).fill('0');
  return [...h, ...(a.includes('::') ? mid : []), ...t].map((x) => x.padStart(4, '0')).join(':');
}

// ---------- keys and tokens ----------

const b64u = (buf) => Buffer.from(buf).toString('base64url');
const sha256hex = (s) => crypto.createHash('sha256').update(s).digest('hex');

/** A new redeem code: 20 symbols of 31, about 99 bits. */
export function newCode() {
  const out = [];
  while (out.length < 20) {
    const b = crypto.randomBytes(32);
    for (const x of b) {
      // Rejection sampling keeps every symbol equally likely.
      if (x < 248 && out.length < 20) out.push(CODE_ALPHABET[x % 31]);
    }
  }
  return 'HALO-' + [0, 5, 10, 15].map((i) => out.slice(i, i + 5).join('')).join('-');
}

/** A typed code as stored: spaces gone, upper case. */
export function normalizeCode(s) {
  return String(s ?? '').replace(/\s+/g, '').toUpperCase();
}

export const codeHash = (code) => sha256hex('halo-license/1 code\n' + normalizeCode(code));

/** The key that seals codes at rest: derived from the signing key, so a copy of licenses.json alone opens nothing. */
export function codeKey(privateKey) {
  return crypto.createHash('sha256').update('halo-license/1 codes\n' + privateKey.export({ format: 'jwk' }).d).digest();
}

/** `code` sealed with AES-256-GCM: `v1.<iv|tag|ciphertext>` (base64url). */
export function sealCode(key, code) {
  const iv = crypto.randomBytes(12);
  const c = crypto.createCipheriv('aes-256-gcm', key, iv);
  const ct = Buffer.concat([c.update(code, 'utf8'), c.final()]);
  return 'v1.' + b64u(Buffer.concat([iv, c.getAuthTag(), ct]));
}

/** A sealed code opened, or null (another signing key, or tampered with). */
export function openCode(key, sealed) {
  try {
    const b = Buffer.from(String(sealed).slice(3), 'base64url');
    const d = crypto.createDecipheriv('aes-256-gcm', key, b.subarray(0, 12));
    d.setAuthTag(b.subarray(12, 28));
    return Buffer.concat([d.update(b.subarray(28)), d.final()]).toString('utf8');
  } catch {
    return null;
  }
}

/** The key's id: the first 16 hex digits of its public point's SHA-256 (as the update keys). */
export function keyId(publicKey) {
  return sha256hex(publicPoint(publicKey)).slice(0, 16);
}

/** The uncompressed P-256 point (04 X Y). */
export function publicPoint(publicKey) {
  const jwk = publicKey.export({ format: 'jwk' });
  return Buffer.concat([Buffer.from([4]), Buffer.from(jwk.x, 'base64url'), Buffer.from(jwk.y, 'base64url')]);
}

/**
 * A license token: `HL1.<payload>.<signature>`, both base64url; the
 * signature is ECDSA P-256 over SHA-256 of `HL1.<payload>` (r || s, 64 bytes).
 */
export function signToken(privateKey, grant) {
  const payload = b64u(JSON.stringify(grant));
  const msg = 'HL1.' + payload;
  const sig = crypto.sign('sha256', Buffer.from(msg), { key: privateKey, dsaEncoding: 'ieee-p1363' });
  return msg + '.' + b64u(sig);
}

// ---------- the store ----------

/** Licenses on disk: one JSON file, written whole and renamed into place. */
export class Store {
  constructor(dir) {
    this.dir = dir;
    this.file = path.join(dir, 'licenses.json');
    this.auditFile = path.join(dir, 'audit.jsonl');
    fs.mkdirSync(dir, { recursive: true, mode: 0o700 });
    this.data = fs.existsSync(this.file) ? JSON.parse(fs.readFileSync(this.file, 'utf8')) : { licenses: [] };
  }

  save() {
    const tmp = this.file + '.tmp';
    fs.writeFileSync(tmp, JSON.stringify(this.data, null, 1), { mode: 0o600 });
    fs.renameSync(tmp, this.file);
  }

  /** One audit line. Never a code, a token or a secret: ids and hash prefixes only. */
  audit(event) {
    const line = JSON.stringify({ at: new Date().toISOString(), ...event }) + '\n';
    fs.appendFileSync(this.auditFile, line, { mode: 0o600 });
  }

  auditTail(n = 200) {
    if (!fs.existsSync(this.auditFile)) return [];
    const lines = fs.readFileSync(this.auditFile, 'utf8').trim().split('\n').filter(Boolean);
    return lines.slice(-n).map((l) => JSON.parse(l)).reverse();
  }

  byHash(h) {
    return this.data.licenses.find((l) => l.code_sha256 === h);
  }

  byId(id) {
    return this.data.licenses.find((l) => l.id === id);
  }
}

/** A license's state at `now` (ms). */
export function stateOf(l, now) {
  if (l.revoked_at) return 'revoked';
  if (!l.redeemed_at) return now > l.redeem_by ? 'lapsed' : 'unused';
  if (l.expires_at && now >= l.expires_at) return 'expired';
  return 'active';
}

/** Make `count` keys. Returns the codes; with `sealKey`, each is kept sealed so the portal can show it again. */
export function makeKeys(store, { kind, days, count, note, actor, sealKey }, now = Date.now()) {
  if (!['pro', 'trial'].includes(kind)) throw new UserError('Pick Pro or trial Pro.');
  const d = days === '' || days == null ? null : Number(days);
  if (kind === 'trial' && d == null) throw new UserError('A trial needs a length in days.');
  if (d != null && (!Number.isInteger(d) || d < 1 || d > LIMITS.maxDays)) throw new UserError(`Days: a whole number from 1 to ${LIMITS.maxDays}.`);
  const n = Number(count);
  if (!Number.isInteger(n) || n < 1 || n > LIMITS.keysPerBatch) throw new UserError(`How many: 1 to ${LIMITS.keysPerBatch}.`);
  const cleanNote = String(note ?? '').replace(/[\u0000-\u001f\u007f]/g, ' ').slice(0, LIMITS.noteMax);
  const codes = [];
  for (let i = 0; i < n; i++) {
    const code = newCode();
    const l = {
      id: crypto.randomBytes(16).toString('hex'),
      code_sha256: codeHash(code),
      code_sealed: sealKey ? sealCode(sealKey, code) : null,
      kind,
      days: d,
      note: cleanNote,
      created_at: now,
      created_by: actor,
      redeem_by: now + LIMITS.redeemWindowDays * 86_400_000,
      redeemed_at: null,
      device_sha256: null, // the install holding the seat now
      seat: 0, // goes up each time the seat moves to another install
      expires_at: null,
      revoked_at: null,
    };
    store.data.licenses.push(l);
    codes.push({ id: l.id, code });
  }
  store.save();
  store.audit({ action: 'create', actor, kind, days: d, count: n, ids: codes.map((c) => c.id) });
  return codes;
}

export function revoke(store, id, actor, now = Date.now()) {
  const l = store.byId(id);
  if (!l) throw new UserError('No such license.');
  if (!l.revoked_at) {
    l.revoked_at = now;
    store.save();
  }
  store.audit({ action: 'revoke', actor, id });
}

/**
 * Redeem `code` for the install `device` (the SHA-256 of its own random id).
 * One seat: the install holding it asking again gets its license again (a
 * reinstall); another install takes the seat, and the one it left reads
 * `revoked` at its next /api/status. Fails closed.
 */
export function redeem(store, privateKey, kid, { code, device }, now = Date.now()) {
  const c = normalizeCode(code);
  if (!CODE_RE.test(c) || !DEVICE_RE.test(String(device ?? ''))) return { status: 400, body: { error: 'malformed' } };
  const l = store.byHash(codeHash(c));
  if (!l) return { status: 404, body: { error: 'unknown_code' } };
  const state = stateOf(l, now);
  if (state === 'revoked') return { status: 403, body: { error: 'revoked' } };
  if (state === 'lapsed') return { status: 410, body: { error: 'lapsed' } };
  if (state === 'expired') return { status: 410, body: { error: 'expired' } };
  if (!l.redeemed_at) {
    l.redeemed_at = now;
    l.device_sha256 = device;
    l.seat = 1;
    l.expires_at = l.days == null ? null : now + l.days * 86_400_000;
    store.save();
    store.audit({ action: 'redeem', id: l.id, kind: l.kind });
  } else if (l.device_sha256 !== device) {
    // The seat moves; the days keep counting from the first redemption.
    l.device_sha256 = device;
    l.seat = (l.seat ?? 0) + 1;
    store.save();
    store.audit({ action: 'seat_move', id: l.id, kind: l.kind });
  } else {
    store.audit({ action: 'reissue', id: l.id });
  }
  const grant = { v: 1, kid, lic: l.id, kind: l.kind, iat: Math.floor(now / 1000), exp: l.expires_at ? Math.floor(l.expires_at / 1000) : null, dev: device, seat: l.seat ?? 0 };
  return { status: 200, body: { token: signToken(privateKey, grant), kind: l.kind, exp: grant.exp } };
}

/** Whether a license still stands for this install: the app asks once a day. `seat`: its token's (optional). */
export function status(store, { lic, device, seat }, now = Date.now()) {
  if (!LIC_RE.test(String(lic ?? '')) || !DEVICE_RE.test(String(device ?? ''))) return { status: 400, body: { error: 'malformed' } };
  if (seat !== undefined && !(Number.isInteger(seat) && seat >= 0)) return { status: 400, body: { error: 'malformed' } };
  const l = store.byId(lic);
  // Unknown, another install's, or a seat since moved on: said as revoked, so nothing is learned.
  if (!l || l.device_sha256 !== device || (seat !== undefined && seat !== (l.seat ?? 0))) return { status: 200, body: { state: 'revoked' } };
  return { status: 200, body: { state: stateOf(l, now) } };
}

export class UserError extends Error {}

// ---------- rate limits ----------

export class Limiter {
  constructor(max, windowMs) {
    this.max = max;
    this.windowMs = windowMs;
    this.hits = new Map();
  }

  /** Count one at `now`; whether it is still within the limit. */
  take(key, now = Date.now()) {
    const list = (this.hits.get(key) ?? []).filter((t) => now - t < this.windowMs);
    list.push(now);
    this.hits.set(key, list);
    if (this.hits.size > 50_000) this.hits.delete(this.hits.keys().next().value);
    return list.length <= this.max;
  }

  count(key, now = Date.now()) {
    return (this.hits.get(key) ?? []).filter((t) => now - t < this.windowMs).length;
  }

  clear(key) {
    this.hits.delete(key);
  }
}

// ---------- HTTP ----------

function send(res, status, body, headers = {}) {
  for (const [k, v] of SECURITY_HEADERS) res.setHeader(k, v);
  for (const [k, v] of Object.entries(headers)) res.setHeader(k, v);
  if (typeof body === 'object' && !Buffer.isBuffer(body)) {
    res.setHeader('Content-Type', 'application/json; charset=utf-8');
    body = JSON.stringify(body);
  }
  res.writeHead(status);
  res.end(body);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let size = 0;
    const parts = [];
    req.on('data', (c) => {
      size += c.length;
      if (size > LIMITS.bodyMax) {
        reject(new UserError('too large'));
        req.destroy();
      } else parts.push(c);
    });
    req.on('end', () => resolve(Buffer.concat(parts).toString('utf8')));
    req.on('error', reject);
  });
}

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

function cookies(req) {
  const out = {};
  for (const part of String(req.headers.cookie ?? '').split(';')) {
    const i = part.indexOf('=');
    if (i > 0) out[part.slice(0, i).trim()] = part.slice(i + 1).trim();
  }
  return out;
}

// Admin assets are versioned in their URLs and cached for a year: bump
// ASSET_V whenever CSS or the fonts change. Pages stay no-store (codes).
const ASSET_V = 4;
const CACHE_LONG = 'public, max-age=31536000, immutable';
const FONTS = ['outfit-400.woff2', 'outfit-600.woff2'];

// Black and white, following the OS: black on white by day, white on black
// at night, greys between. Only badges and the two action buttons keep
// colour: the Diorama looks' (crates/halo-core/src/look.rs), day and night.
// Outfit, as Halo writes.
const CSS = `@font-face{font-family:Outfit;font-weight:400;font-display:swap;src:url(/fonts/outfit-400.woff2?v=${ASSET_V}) format("woff2")}
@font-face{font-family:Outfit;font-weight:600;font-display:swap;src:url(/fonts/outfit-600.woff2?v=${ASSET_V}) format("woff2")}
:root{color-scheme:light dark;--page:#FFFFFF;--pane:#FFFFFF;--field:#FFFFFF;--hover:#F4F4F4;--ink:#000000;--muted:#5C5C5C;--line:#D9D9D9;
--accent:#000000;--accent-ink:#FFFFFF;--tile:#0B0D12;--eyes:#F2F5FA;
--primary:#4C58A8;--primary-ink:#FFFFFF;--unused:#3B4274;--green:#4A6A30;--red:#B0392D;--yellow:#86600E;--orange:#9E5418;--mono:ui-monospace,"Cascadia Mono",Menlo,Consolas,monospace}
@media (prefers-color-scheme:dark){:root{--page:#000000;--pane:#000000;--field:#0D0D0D;--hover:#161616;--ink:#FFFFFF;--muted:#A3A3A3;--line:#333333;
--accent:#FFFFFF;--accent-ink:#000000;--tile:#F2F5FA;--eyes:#0B0D12;
--primary:#E9AE3A;--primary-ink:#1B1D40;--unused:#7AB0FF;--green:#5FD068;--red:#FF6A5E;--yellow:#FFD76A;--orange:#F2B550}}
*{box-sizing:border-box}
body{margin:0;font:400 15px/22px Outfit,system-ui,sans-serif;color:var(--ink);background:var(--page)}
h1{font-size:28px;line-height:34px;font-weight:600;margin:0}
h2{font-size:18px;line-height:24px;font-weight:600;margin:0 0 16px}
p{margin:0}a{color:var(--ink)}
code,.mono,textarea{font-family:var(--mono)}
:focus-visible{outline:2px solid var(--accent);outline-offset:2px}
.muted{color:var(--muted)}
.bar{background:var(--pane);border-bottom:1px solid var(--line)}
.bar-in{max-width:1200px;margin:0 auto;padding:0 24px;height:56px;display:flex;align-items:center;gap:24px}
.brand{display:flex;align-items:center;gap:12px;font-weight:600;color:var(--ink);text-decoration:none}
.mark{width:28px;height:28px;display:block}.mark rect{fill:var(--eyes)}.mark .tile{fill:var(--tile)}
.tabs{display:flex;gap:24px;align-self:stretch;flex:1}
.tabs a{display:flex;align-items:center;color:var(--muted);text-decoration:none;font-weight:600;border-bottom:2px solid transparent;margin-bottom:-1px}
.tabs a:hover{color:var(--ink)}.tabs a[aria-current]{color:var(--ink);border-bottom-color:var(--accent)}
.link-btn{background:none;border:0;padding:8px 0;font:600 15px/22px Outfit,system-ui,sans-serif;color:var(--muted);cursor:pointer}.link-btn:hover{color:var(--ink)}
main{max-width:1200px;margin:0 auto;padding:32px 24px 48px;display:grid;grid-template-columns:minmax(0,1fr);gap:24px}
.head{display:grid;gap:4px}
.card{background:var(--pane);border:1px solid var(--line);border-radius:20px;padding:24px}
.flush{padding:0;overflow-x:auto}
.error{background:var(--hover);color:var(--ink);border:1px solid var(--ink);border-radius:10px;padding:12px 16px;font-weight:600}
.card>.error{margin-bottom:16px}
label{display:grid;gap:4px;font-size:12px;line-height:16px;font-weight:600;letter-spacing:.02em;color:var(--muted)}
input,select{font:400 15px/22px Outfit,system-ui,sans-serif;color:var(--ink);background:var(--field);border:1px solid var(--line);border-radius:10px;height:40px;padding:0 12px;width:100%}
button{font:600 15px/22px Outfit,system-ui,sans-serif;border-radius:10px;cursor:pointer}
.primary{height:40px;padding:0 20px;border:0;background:var(--primary);color:var(--primary-ink)}
.danger{height:32px;padding:0 12px;border:0;background:var(--red);color:var(--pane);font-size:13px}
.make{display:grid;grid-template-columns:1fr 1fr 1fr 2fr auto;gap:12px;align-items:end}
.make+p{margin-top:12px;font-size:14px}
.stats{display:grid;grid-template-columns:repeat(5,1fr);gap:12px}
.stat{background:var(--pane);border:1px solid var(--line);border-radius:20px;padding:16px;display:grid;gap:8px;justify-items:start}
.stat b{font-size:28px;line-height:34px;font-weight:600;font-variant-numeric:tabular-nums}
.badge{display:inline-block;border-radius:999px;padding:2px 10px;font-size:12px;line-height:16px;font-weight:600;letter-spacing:.02em;color:var(--c,var(--muted));background:color-mix(in srgb,var(--c,var(--muted)) 14%,transparent)}
.s-active,.a-create,.a-redeem,.a-reissue{--c:var(--green)}.s-unused{--c:var(--unused)}.s-expired{--c:var(--yellow)}
.s-revoked,.a-login_failed,.a-login_blocked{--c:var(--red)}.a-revoke{--c:var(--orange)}.a-view_key{--c:var(--yellow)}.a-seat_move{--c:var(--unused)}
table{border-collapse:separate;border-spacing:0;width:100%;font-size:14px;line-height:20px;font-variant-numeric:tabular-nums}
th{position:sticky;top:0;background:var(--pane);text-align:left;font-size:12px;line-height:16px;font-weight:600;letter-spacing:.02em;color:var(--muted);padding:12px 10px;border-bottom:1px solid var(--line)}
th:first-child{border-top-left-radius:20px}th:last-child{border-top-right-radius:20px}
td{padding:10px;border-bottom:1px solid var(--line);vertical-align:middle;white-space:nowrap}
.wrap{white-space:normal;overflow-wrap:anywhere}
tr:last-child td{border-bottom:0}
tbody tr:hover td,tr:has(details[open]) td{background:var(--hover)}
td code{font-size:13px}
.note{max-width:150px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}
.empty{padding:32px 24px;text-align:center}
details summary{list-style:none;display:inline-flex;align-items:center;height:32px;padding:0 12px;border:1px solid var(--line);border-radius:10px;font-size:13px;font-weight:600;cursor:pointer;white-space:nowrap}
details summary::-webkit-details-marker{display:none}
.acts{display:flex;flex-wrap:wrap;justify-content:flex-end;gap:8px;align-items:flex-start}.acts details form{justify-items:end;text-align:right}.danger{white-space:nowrap}.acts form{margin:0}
.ghost{height:32px;padding:0 12px;border:1px solid var(--line);background:none;color:var(--ink);font-size:13px;white-space:nowrap}.ghost:hover{border-color:var(--ink)}
details[open] summary{border-color:var(--red);color:var(--red)}
details form{display:grid;gap:4px;justify-items:start;margin-top:8px;font-size:12px}
.once{border-left:4px solid var(--ink)}
.once h2{display:flex;gap:8px;align-items:center;margin-bottom:4px}.once h2 svg{width:20px;height:20px;color:var(--ink);flex:none}
.once ul{list-style:none;margin:16px 0 0;padding:0;display:grid;gap:8px}
.once li{display:flex;gap:12px;align-items:baseline;flex-wrap:wrap}
.once code{font-size:22px;line-height:30px;user-select:all;overflow-wrap:anywhere}
.once label{margin-top:16px}
textarea{font-size:14px;line-height:20px;color:var(--ink);background:var(--field);border:1px solid var(--line);border-radius:10px;padding:12px;width:100%;resize:vertical}
.login{min-height:100vh;display:grid;place-items:center;padding:24px}
.login .card{width:min(360px,100%);display:grid;gap:16px;justify-items:center;text-align:center}
.login .mark{width:56px;height:56px}.login form{display:grid;gap:12px;width:100%;text-align:left}.login .primary{width:100%}.login .error{width:100%;text-align:left}
@media (max-width:640px){
.bar-in{padding:0 16px;gap:16px}.brand span{display:none}
main{padding:24px 16px 40px}
.make{grid-template-columns:1fr}.stats{grid-template-columns:1fr 1fr}
table,tbody,tr,td{display:block}thead{position:absolute;width:1px;height:1px;overflow:hidden;clip:rect(0 0 0 0)}
tr{padding:12px 16px;border-bottom:1px solid var(--line)}tr:last-child{border-bottom:0}
td{display:flex;justify-content:space-between;gap:16px;padding:4px 0;border:0;background:none!important}
td[data-label]::before{content:attr(data-label);flex:none;color:var(--muted);font-size:12px;font-weight:600;letter-spacing:.02em}
.note,.wrap{max-width:none;white-space:normal;text-align:right}.once code{font-size:18px;line-height:26px}td:empty{display:none}
}`;

// Halo the mascot, as the app icon draws him (draw_mascot in tools/make_brand.py).
const MARK = '<svg class="mark" aria-hidden="true" focusable="false" viewBox="0 0 100 100"><rect class="tile" x="4.7" y="4.7" width="90.6" height="90.6" rx="20.27"/><rect x="33.01" y="34.9" width="15.1" height="30.2" rx="7.55"/><rect x="51.89" y="34.9" width="15.1" height="30.2" rx="7.55"/></svg>';
const WARN_ICON = '<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12 2 1 21h22L12 2Zm-1 7h2v6h-2V9Zm0 8h2v2h-2v-2Z"/></svg>';

function page(title, inner) {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${esc(title)} · Halo Pro keys</title><link rel="stylesheet" href="/admin.css?v=${ASSET_V}"></head><body>${inner}</body></html>`;
}

/**
 * The server. `opts`: { dataDir, privateKey, adminToken, secureCookie, now }.
 * Returns { admin, redeem } request handlers (tested without listening).
 */
export function createLicenseServer(opts) {
  const { dataDir, privateKey, adminToken } = opts;
  if (!adminToken || adminToken.length < 32) throw new Error('LICENSE_ADMIN_TOKEN must be set, 32 characters or more');
  const now = opts.now ?? (() => Date.now());
  const store = new Store(dataDir);
  const publicKey = crypto.createPublicKey(privateKey);
  const kid = keyId(publicKey);
  const sealKey = codeKey(privateKey);
  const tokenHash = crypto.createHash('sha256').update(adminToken).digest();
  const sessions = new Map(); // id -> { csrf, until }
  const loginFails = new Limiter(LIMITS.loginFailures, LIMITS.loginWindowMs);
  const redeemByAddr = new Limiter(LIMITS.redeemPerMinute, 60_000);
  const redeemAll = new Limiter(LIMITS.redeemGlobalPerMinute, 60_000);
  // The portal's two Outfit weights, read once (about 33 KB).
  const fontDir = path.join(path.dirname(fileURLToPath(import.meta.url)), 'public');
  const fonts = new Map(FONTS.map((f) => [`/fonts/${f}`, fs.readFileSync(path.join(fontDir, f))]));
  const cookieFlags = `HttpOnly; SameSite=Strict; Path=/${opts.secureCookie ? '; Secure' : ''}`;

  const session = (req) => {
    const id = cookies(req).hl_admin;
    const s = id && sessions.get(id);
    if (!s || s.until < now()) {
      if (id) sessions.delete(id);
      return null;
    }
    return { id, ...s };
  };

  const formFields = (text) => Object.fromEntries(new URLSearchParams(text));

  async function admin(req, res) {
    const peer = req.socket.remoteAddress;
    if (!isTailnetOrLoopback(peer)) return send(res, 403, 'Forbidden', { 'Content-Type': 'text/plain' });
    const url = new URL(req.url, 'http://admin');
    try {
      if (req.method === 'GET' && url.pathname === '/admin.css') return send(res, 200, CSS, { 'Content-Type': 'text/css; charset=utf-8', 'Cache-Control': CACHE_LONG });
      if (req.method === 'GET' && fonts.has(url.pathname)) return send(res, 200, fonts.get(url.pathname), { 'Content-Type': 'font/woff2', 'Cache-Control': CACHE_LONG });
      if (url.pathname === '/login') {
        if (req.method === 'GET') return send(res, 200, page('Sign in', loginForm()), { 'Content-Type': 'text/html; charset=utf-8' });
        if (req.method !== 'POST') return send(res, 405, 'Method not allowed', { 'Content-Type': 'text/plain' });
        if (loginFails.count(peer, now()) >= LIMITS.loginFailures) {
          store.audit({ action: 'login_blocked', peer });
          return send(res, 429, page('Sign in', loginForm('Too many tries. Wait 15 minutes.')), { 'Content-Type': 'text/html; charset=utf-8' });
        }
        const f = formFields(await readBody(req));
        const given = crypto.createHash('sha256').update(String(f.token ?? '')).digest();
        if (!crypto.timingSafeEqual(given, tokenHash)) {
          loginFails.take(peer, now());
          store.audit({ action: 'login_failed', peer });
          return send(res, 401, page('Sign in', loginForm('That token is not right.')), { 'Content-Type': 'text/html; charset=utf-8' });
        }
        loginFails.clear(peer);
        const id = crypto.randomBytes(32).toString('base64url');
        sessions.set(id, { csrf: crypto.randomBytes(32).toString('base64url'), until: now() + LIMITS.sessionMs, peer });
        store.audit({ action: 'login', peer });
        return send(res, 303, '', { Location: '/', 'Set-Cookie': `hl_admin=${id}; ${cookieFlags}; Max-Age=${LIMITS.sessionMs / 1000}` });
      }
      const s = session(req);
      if (!s) return send(res, 303, '', { Location: '/login' });
      const actor = `admin@${s.peer}`;
      if (req.method === 'POST') {
        const f = formFields(await readBody(req));
        const a = Buffer.from(String(f.csrf ?? ''));
        const b = Buffer.from(s.csrf);
        if (a.length !== b.length || !crypto.timingSafeEqual(a, b)) return send(res, 403, 'Forbidden', { 'Content-Type': 'text/plain' });
        if (url.pathname === '/keys') {
          let codes, error;
          try {
            codes = makeKeys(store, { kind: f.kind, days: f.days, count: f.count, note: f.note, actor, sealKey }, now());
          } catch (e) {
            if (!(e instanceof UserError)) throw e;
            error = e.message;
          }
          return send(res, error ? 400 : 200, page('Keys', home(s, store, codes, error)), { 'Content-Type': 'text/html; charset=utf-8' });
        }
        if (url.pathname === '/view') {
          // A key shown again, to an admin only; the audit names the license, never the code.
          const l = store.byId(String(f.id ?? ''));
          const code = l?.code_sealed ? openCode(sealKey, l.code_sealed) : null;
          if (!code) return send(res, 404, page('Keys', home(s, store, null, l ? 'This key was made before keys were kept, or with another signing key.' : 'No such license.')), { 'Content-Type': 'text/html; charset=utf-8' });
          store.audit({ action: 'view_key', actor, id: l.id });
          return send(res, 200, page('Keys', home(s, store, [{ id: l.id, code }], null, true)), { 'Content-Type': 'text/html; charset=utf-8' });
        }
        if (url.pathname === '/revoke') {
          try {
            revoke(store, String(f.id ?? ''), actor, now());
          } catch (e) {
            if (!(e instanceof UserError)) throw e;
            return send(res, 404, page('Keys', home(s, store, null, e.message)), { 'Content-Type': 'text/html; charset=utf-8' });
          }
          return send(res, 303, '', { Location: '/' });
        }
        if (url.pathname === '/logout') {
          sessions.delete(s.id);
          store.audit({ action: 'logout', peer: s.peer });
          return send(res, 303, '', { Location: '/login', 'Set-Cookie': `hl_admin=; ${cookieFlags}; Max-Age=0` });
        }
        return send(res, 404, 'Not found', { 'Content-Type': 'text/plain' });
      }
      if (req.method === 'GET' && url.pathname === '/') return send(res, 200, page('Keys', home(s, store)), { 'Content-Type': 'text/html; charset=utf-8' });
      if (req.method === 'GET' && url.pathname === '/audit') return send(res, 200, page('Audit', auditPage(s, store)), { 'Content-Type': 'text/html; charset=utf-8' });
      return send(res, 404, 'Not found', { 'Content-Type': 'text/plain' });
    } catch (e) {
      if (e instanceof UserError) return send(res, 400, 'Bad request', { 'Content-Type': 'text/plain' });
      console.error('admin: error', e?.code ?? e?.name ?? 'unknown');
      return send(res, 500, 'Server error', { 'Content-Type': 'text/plain' });
    }
  }

  function homeTimes(t) {
    return t ? new Date(t).toISOString().slice(0, 16).replace('T', ' ') : '';
  }

  const csrfField = (s) => `<input type="hidden" name="csrf" value="${esc(s.csrf)}">`;

  function bar(s, current) {
    const tab = (href, name) => `<a href="${href}"${current === name ? ' aria-current="page"' : ''}>${name}</a>`;
    return `<header class="bar"><div class="bar-in"><a class="brand" href="/">${MARK}<span>Halo Pro keys</span></a>
<nav class="tabs" aria-label="Pages">${tab('/', 'Keys')}${tab('/audit', 'Audit')}</nav>
<form method="post" action="/logout">${csrfField(s)}<button class="link-btn">Sign out</button></form></div></header>`;
  }

  function home(s, st, codes, error, viewed = false) {
    const t = now();
    const STATES = ['active', 'unused', 'expired', 'lapsed', 'revoked'];
    const counts = Object.fromEntries(STATES.map((k) => [k, 0]));
    for (const l of st.data.licenses) counts[stateOf(l, t)]++;
    const rows = [...st.data.licenses]
      .sort((a, b) => b.created_at - a.created_at)
      .slice(0, 500)
      .map((l) => {
        const state = stateOf(l, t);
        const hidden = `${csrfField(s)}<input type="hidden" name="id" value="${esc(l.id)}">`;
        const view = l.code_sealed ? `<form method="post" action="/view">${hidden}<button class="ghost">View key</button></form>` : '';
        const revoke = state === 'revoked' ? '' : `<details><summary>Revoke…</summary><form method="post" action="/revoke">${hidden}<button class="danger">Revoke for good</button><span class="muted">Can't be undone.</span></form></details>`;
        const seat = l.device_sha256 ? `<code>${esc(l.device_sha256.slice(0, 12))}</code> #${l.seat ?? 0}` : '';
        return `<tr><td data-label="Id"><code>${esc(l.id.slice(0, 8))}</code></td><td data-label="Kind">${l.kind === 'pro' ? 'Pro' : 'Trial'}</td><td data-label="Days">${l.days ?? '∞'}</td><td data-label="State"><span class="badge s-${state}">${state}</span></td><td data-label="Made">${homeTimes(l.created_at)}</td><td data-label="Redeemed">${homeTimes(l.redeemed_at)}</td><td data-label="Ends">${homeTimes(l.expires_at)}</td><td data-label="Seat">${seat}</td><td data-label="Note" class="note" title="${esc(l.note)}">${esc(l.note)}</td><td>${view || revoke ? `<div class="acts">${view}${revoke}</div>` : ''}</td></tr>`;
      })
      .join('');
    const made = codes?.length
      ? `<section class="card once" role="alert"><h2>${WARN_ICON}${viewed ? 'This key is a secret.' : 'New keys.'}</h2><p class="muted">${viewed ? 'Viewing it is in the audit log.' : 'View key shows them again later.'} Click a code to select it.</p>
<ul>${codes.map((c) => `<li><code>${esc(c.code)}</code><span class="muted mono">${esc(c.id.slice(0, 8))}</span></li>`).join('')}</ul>${
          codes.length > 1 ? `<label>All ${codes.length} codes, one per line<textarea readonly rows="${Math.min(codes.length, 10)}">${codes.map((c) => esc(c.code)).join('\n')}</textarea></label>` : ''
        }</section>`
      : '';
    const table = rows
      ? `<table><thead><tr><th>Id</th><th>Kind</th><th>Days</th><th>State</th><th>Made</th><th>Redeemed</th><th>Ends</th><th>Seat (PC #)</th><th>Note</th><th></th></tr></thead><tbody>${rows}</tbody></table>`
      : '<p class="empty muted">No keys yet. Make the first one above.</p>';
    return `${bar(s, 'Keys')}<main>${made}
<div class="head"><h1>Keys</h1><p class="muted">Internal. Reachable on the tailnet only.</p></div>
<div class="stats">${STATES.map((k) => `<div class="stat"><span class="badge s-${k}">${k}</span><b>${counts[k]}</b></div>`).join('')}</div>
<section class="card"><h2>Make keys</h2>${error ? `<p class="error" role="alert">${esc(error)}</p>` : ''}<form method="post" action="/keys" class="make">${csrfField(s)}
<label>Kind<select name="kind"><option value="pro">Halo Pro</option><option value="trial">Trial Pro</option></select></label>
<label>Days<input name="days" type="number" min="1" max="${LIMITS.maxDays}" placeholder="No end"></label>
<label>How many<input name="count" type="number" min="1" max="${LIMITS.keysPerBatch}" value="1"></label>
<label>Note<input name="note" maxlength="${LIMITS.noteMax}"></label>
<button class="primary">Make keys</button></form>
<p class="muted">Days count from redemption; leave empty for no end. Each key is one seat: one PC at a time. Activating it on another PC moves the seat there and the first PC drops to Free at its next daily check. Unredeemed keys lapse after ${LIMITS.redeemWindowDays} days.</p></section>
<section class="card flush">${table}</section></main>`;
  }

  function auditPage(s, st) {
    const rows = st
      .auditTail()
      .map((e) => `<tr><td data-label="When">${esc(e.at)}</td><td data-label="What"><span class="badge a-${esc(String(e.action).replace(/[^a-z_]/g, ''))}">${esc(e.action)}</span></td><td data-label="Who">${esc(e.actor ?? e.peer ?? '')}</td><td data-label="License" class="wrap"><code>${esc((e.id ?? (e.ids ?? []).join(', ')).toString().slice(0, 200))}</code></td><td data-label="Kind">${esc(e.kind ?? '')}</td></tr>`)
      .join('');
    const table = rows
      ? `<table><thead><tr><th>When</th><th>What</th><th>Who</th><th>License</th><th>Kind</th></tr></thead><tbody>${rows}</tbody></table>`
      : '<p class="empty muted">No entries yet.</p>';
    return `${bar(s, 'Audit')}<main><div class="head"><h1>Audit</h1><p class="muted">The newest 200 entries, newest first. Never a code, a token or a secret.</p></div>
<section class="card flush">${table}</section></main>`;
  }

  function loginForm(error) {
    return `<main class="login"><div class="card">${MARK}<h1>Halo Pro keys</h1>${error ? `<p class="error" role="alert">${esc(error)}</p>` : ''}<form method="post" action="/login"><label>Admin token<input type="password" name="token" autocomplete="current-password" autofocus></label><button class="primary">Sign in</button></form><p class="muted">Internal. Reachable on the tailnet only.</p></div></main>`;
  }

  async function redeemApi(req, res) {
    const peer = opts.trustProxy ? String(req.headers['x-forwarded-for'] ?? '').split(',')[0].trim() || req.socket.remoteAddress : req.socket.remoteAddress;
    const url = new URL(req.url, 'http://api');
    try {
      if (req.method !== 'POST' || !['/api/redeem', '/api/status'].includes(url.pathname)) return send(res, 404, { error: 'not_found' });
      if (!String(req.headers['content-type'] ?? '').startsWith('application/json')) return send(res, 415, { error: 'json_only' });
      if (!redeemAll.take('all', now()) || !redeemByAddr.take(peer, now())) return send(res, 429, { error: 'busy' }, { 'Retry-After': '60' });
      let body;
      try {
        body = JSON.parse(await readBody(req));
      } catch {
        return send(res, 400, { error: 'malformed' });
      }
      if (typeof body !== 'object' || body === null) return send(res, 400, { error: 'malformed' });
      const r = url.pathname === '/api/redeem' ? redeem(store, privateKey, kid, body, now()) : status(store, body, now());
      return send(res, r.status, r.body);
    } catch (e) {
      if (e instanceof UserError) return send(res, 413, { error: 'too_large' });
      console.error('redeem: error', e?.code ?? e?.name ?? 'unknown');
      return send(res, 500, { error: 'server' });
    }
  }

  return { admin, redeem: redeemApi, store, kid };
}

// ---------- the command line ----------

function arg(argv, name, fallback) {
  const i = argv.indexOf(name);
  return i >= 0 ? argv[i + 1] : fallback;
}

function readPrivateKey(file) {
  const st = fs.statSync(file);
  if (process.platform !== 'win32' && (st.mode & 0o077) !== 0) throw new Error(`${file} must be readable by its owner only (chmod 600)`);
  const key = crypto.createPrivateKey(fs.readFileSync(file));
  if (key.asymmetricKeyDetails?.namedCurve !== 'prime256v1') throw new Error('the signing key must be P-256');
  return key;
}

async function main(argv) {
  const cmd = argv[0];
  if (cmd === 'init-key') {
    const file = arg(argv, '--key');
    if (!file) throw new Error('init-key --key <path outside the repository>');
    if (fs.existsSync(file)) throw new Error(`${file} exists; refusing to overwrite a signing key`);
    const { privateKey, publicKey } = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' });
    fs.writeFileSync(file, privateKey.export({ format: 'pem', type: 'pkcs8' }), { mode: 0o600 });
    // The public half only: this line goes into crates/halo-core/data/license-keys.txt.
    console.log(`${keyId(publicKey)} ${publicPoint(publicKey).toString('hex')}`);
    return;
  }
  if (cmd !== 'serve') throw new Error('usage: license-server.mjs init-key --key <file> | serve --key <file> --data <dir> --admin-host <tailscale ip> [--admin-port 8790] [--redeem-host <ip> --redeem-port 8791 --trust-proxy] [--secure-cookie]');
  const adminHost = arg(argv, '--admin-host', '127.0.0.1');
  if (!isTailnetOrLoopback(adminHost)) throw new Error(`--admin-host ${adminHost} is not a Tailscale or loopback address; the portal is never public`);
  const srv = createLicenseServer({
    dataDir: arg(argv, '--data', path.join(path.dirname(fileURLToPath(import.meta.url)), 'license-data')),
    privateKey: readPrivateKey(arg(argv, '--key')),
    adminToken: process.env.LICENSE_ADMIN_TOKEN,
    secureCookie: argv.includes('--secure-cookie'),
    trustProxy: argv.includes('--trust-proxy'),
  });
  const adminPort = Number(arg(argv, '--admin-port', 8790));
  const a = http.createServer(srv.admin).listen(adminPort, adminHost, () => console.log(`license admin on ${adminHost}:${a.address().port} (key ${srv.kid})`));
  const redeemHost = arg(argv, '--redeem-host');
  if (redeemHost) {
    const r = http.createServer(srv.redeem).listen(Number(arg(argv, '--redeem-port', 8791)), redeemHost, () => console.log(`license redeem API on ${redeemHost}:${r.address().port}`));
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  main(process.argv.slice(2)).catch((e) => {
    console.error(String(e.message ?? e));
    process.exit(1);
  });
}

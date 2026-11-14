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
// Keys: a code shown once (HALO-XXXXX-XXXXX-XXXXX-XXXXX, 100 random bits),
// stored only as its SHA-256. Redeemed once, for one install, it gives a
// license token signed with the server's P-256 key, which the app verifies
// offline against the public keys built into it. The private key lives in a
// file outside the repository (--key, mode 0600), never in git or the app.
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
  ['Content-Security-Policy', "default-src 'none'; style-src 'self'; form-action 'self'; base-uri 'none'; frame-ancestors 'none'"],
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

/** Make `count` keys. Returns the codes, shown once. */
export function makeKeys(store, { kind, days, count, note, actor }, now = Date.now()) {
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
      kind,
      days: d,
      note: cleanNote,
      created_at: now,
      created_by: actor,
      redeem_by: now + LIMITS.redeemWindowDays * 86_400_000,
      redeemed_at: null,
      device_sha256: null,
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
 * One code, one install: the same install asking again gets its license
 * again (a reinstall); another install is refused. Fails closed.
 */
export function redeem(store, privateKey, kid, { code, device }, now = Date.now()) {
  const c = normalizeCode(code);
  if (!CODE_RE.test(c) || !DEVICE_RE.test(String(device ?? ''))) return { status: 400, body: { error: 'malformed' } };
  const l = store.byHash(codeHash(c));
  if (!l) return { status: 404, body: { error: 'unknown_code' } };
  const state = stateOf(l, now);
  if (state === 'revoked') return { status: 403, body: { error: 'revoked' } };
  if (state === 'lapsed') return { status: 410, body: { error: 'lapsed' } };
  if (l.redeemed_at && l.device_sha256 !== device) return { status: 409, body: { error: 'already_used' } };
  if (state === 'expired') return { status: 410, body: { error: 'expired' } };
  if (!l.redeemed_at) {
    l.redeemed_at = now;
    l.device_sha256 = device;
    l.expires_at = l.days == null ? null : now + l.days * 86_400_000;
    store.save();
    store.audit({ action: 'redeem', id: l.id, kind: l.kind });
  } else {
    store.audit({ action: 'reissue', id: l.id });
  }
  const grant = { v: 1, kid, lic: l.id, kind: l.kind, iat: Math.floor(now / 1000), exp: l.expires_at ? Math.floor(l.expires_at / 1000) : null, dev: device };
  return { status: 200, body: { token: signToken(privateKey, grant), kind: l.kind, exp: grant.exp } };
}

/** Whether a license still stands: the app asks now and then. */
export function status(store, { lic, device }, now = Date.now()) {
  if (!LIC_RE.test(String(lic ?? '')) || !DEVICE_RE.test(String(device ?? ''))) return { status: 400, body: { error: 'malformed' } };
  const l = store.byId(lic);
  // Unknown or another install's: said as revoked, so nothing is learned.
  if (!l || l.device_sha256 !== device) return { status: 200, body: { state: 'revoked' } };
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

const CSS = `body{font:15px/1.45 system-ui,sans-serif;max-width:980px;margin:32px auto;padding:0 16px;color:#141821;background:#F3F5F9}
h1{font-size:22px}table{border-collapse:collapse;width:100%;font-size:13px}td,th{padding:6px 8px;border-bottom:1px solid #D5DAE3;text-align:left}
input,select,button{font:inherit;padding:6px 8px}code{font-size:15px;background:#fff;padding:2px 6px;border-radius:4px}
.box{background:#fff;border:1px solid #D5DAE3;border-radius:12px;padding:16px;margin:16px 0}.warn{color:#9A3412}.muted{color:#5B6270}`;

function page(title, inner) {
  return `<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${esc(title)}</title><link rel="stylesheet" href="/admin.css"></head><body>${inner}</body></html>`;
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
  const tokenHash = crypto.createHash('sha256').update(adminToken).digest();
  const sessions = new Map(); // id -> { csrf, until }
  const loginFails = new Limiter(LIMITS.loginFailures, LIMITS.loginWindowMs);
  const redeemByAddr = new Limiter(LIMITS.redeemPerMinute, 60_000);
  const redeemAll = new Limiter(LIMITS.redeemGlobalPerMinute, 60_000);
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
      if (req.method === 'GET' && url.pathname === '/admin.css') return send(res, 200, CSS, { 'Content-Type': 'text/css; charset=utf-8' });
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
            codes = makeKeys(store, { kind: f.kind, days: f.days, count: f.count, note: f.note, actor }, now());
          } catch (e) {
            if (!(e instanceof UserError)) throw e;
            error = e.message;
          }
          return send(res, error ? 400 : 200, page('Keys', home(s, store, codes, error)), { 'Content-Type': 'text/html; charset=utf-8' });
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
      if (req.method === 'GET' && url.pathname === '/audit') return send(res, 200, page('Audit', auditPage(store)), { 'Content-Type': 'text/html; charset=utf-8' });
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

  function home(s, st, codes, error) {
    const t = now();
    const rows = [...st.data.licenses]
      .sort((a, b) => b.created_at - a.created_at)
      .slice(0, 500)
      .map((l) => {
        const state = stateOf(l, t);
        const revoke = state === 'revoked' ? '' : `<form method="post" action="/revoke"><input type="hidden" name="csrf" value="${esc(s.csrf)}"><input type="hidden" name="id" value="${esc(l.id)}"><button>Revoke</button></form>`;
        return `<tr><td><code>${esc(l.id.slice(0, 8))}</code></td><td>${l.kind === 'pro' ? 'Pro' : 'Trial'}</td><td>${l.days ?? '∞'}</td><td>${esc(state)}</td><td>${homeTimes(l.created_at)}</td><td>${homeTimes(l.redeemed_at)}</td><td>${homeTimes(l.expires_at)}</td><td>${esc(l.note)}</td><td>${revoke}</td></tr>`;
      })
      .join('');
    const made = codes?.length
      ? `<div class="box"><p class="warn"><b>Copy these now.</b> They are shown once; the server keeps only their hash.</p>${codes.map((c) => `<p><code>${esc(c.code)}</code> <span class="muted">${esc(c.id.slice(0, 8))}</span></p>`).join('')}</div>`
      : '';
    return `<h1>Halo Pro keys</h1>
<p class="muted">Internal. Reachable on the tailnet only. <a href="/audit">Audit log</a></p>
${error ? `<p class="warn">${esc(error)}</p>` : ''}${made}
<div class="box"><form method="post" action="/keys"><input type="hidden" name="csrf" value="${esc(s.csrf)}">
<label>Kind <select name="kind"><option value="pro">Halo Pro</option><option value="trial">Trial Pro</option></select></label>
<label>Lasts (days from redemption; empty = no end) <input name="days" inputmode="numeric" size="6"></label>
<label>How many <input name="count" value="1" inputmode="numeric" size="4"></label>
<label>Note <input name="note" maxlength="${LIMITS.noteMax}"></label>
<button>Make keys</button></form>
<p class="muted">Each key works once, on one install. Unredeemed keys lapse after ${LIMITS.redeemWindowDays} days.</p></div>
<table><tr><th>Id</th><th>Kind</th><th>Days</th><th>State</th><th>Made</th><th>Redeemed</th><th>Ends</th><th>Note</th><th></th></tr>${rows}</table>
<form method="post" action="/logout"><input type="hidden" name="csrf" value="${esc(s.csrf)}"><button>Sign out</button></form>`;
  }

  function auditPage(st) {
    const rows = st
      .auditTail()
      .map((e) => `<tr><td>${esc(e.at)}</td><td>${esc(e.action)}</td><td>${esc(e.actor ?? e.peer ?? '')}</td><td>${esc((e.id ?? (e.ids ?? []).join(', ')).toString().slice(0, 200))}</td><td>${esc(e.kind ?? '')}</td></tr>`)
      .join('');
    return `<h1>Audit</h1><p><a href="/">Keys</a></p><table><tr><th>When</th><th>What</th><th>Who</th><th>License</th><th>Kind</th></tr>${rows}</table>`;
  }

  function loginForm(error) {
    return `<h1>Halo Pro keys</h1>${error ? `<p class="warn">${esc(error)}</p>` : ''}<form method="post" action="/login"><label>Admin token <input type="password" name="token" autocomplete="current-password" autofocus></label> <button>Sign in</button></form>`;
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

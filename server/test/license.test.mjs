// The license server: keys, redemption, the admin portal's guards.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import { tempDir } from './helpers.mjs';
import {
  createLicenseServer, isTailnetOrLoopback, newCode, normalizeCode, codeHash, codeKey, openCode, makeKeys, redeem, status, revoke, stateOf, Store, Limiter, keyId, publicPoint, LIMITS,
} from '../license-server.mjs';

const TOKEN = 'a'.repeat(40);
const key = () => crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' }).privateKey;
const device = () => crypto.randomBytes(32).toString('hex');

test('only the tailnet and loopback reach the portal', () => {
  for (const a of ['127.0.0.1', '::1', '100.64.0.1', '100.127.255.254', '100.100.1.2', '::ffff:100.101.1.1', 'fd7a:115c:a1e0::1', 'fd7a:115c:a1e0:ab12::42']) {
    assert.ok(isTailnetOrLoopback(a), a);
  }
  for (const a of ['0.0.0.0', '10.0.0.1', '192.168.1.5', '100.63.255.255', '100.128.0.1', '8.8.8.8', '::', 'fd7a:115c:a1e1::1', '2001:db8::1', '', 'localhost', undefined]) {
    assert.ok(!isTailnetOrLoopback(a), String(a));
  }
});

test('codes are random, well formed and kept only hashed and sealed', () => {
  const seen = new Set();
  for (let i = 0; i < 2000; i++) {
    const c = newCode();
    assert.match(c, /^HALO(-[A-HJ-KM-NP-Z2-9]{5}){4}$/);
    assert.ok(!/[01OIL]/.test(c.slice(5)), c);
    seen.add(c);
  }
  assert.equal(seen.size, 2000);
  assert.equal(normalizeCode(' halo-abcde-fghjk-mnpqr-stuvw\n'), 'HALO-ABCDE-FGHJK-MNPQR-STUVW');
  const store = new Store(tempDir());
  const pk = key();
  const [made] = makeKeys(store, { kind: 'pro', days: '', count: 1, note: 'x', actor: 't', sealKey: codeKey(pk) });
  const disk = fs.readFileSync(store.file, 'utf8') + fs.readFileSync(store.auditFile, 'utf8');
  assert.ok(!disk.includes(made.code), 'the code itself never reaches the disk');
  assert.ok(disk.includes(codeHash(made.code)));
  // Sealed: the signing key opens it, another key doesn't.
  assert.equal(openCode(codeKey(pk), store.byId(made.id).code_sealed), made.code);
  assert.equal(openCode(codeKey(key()), store.byId(made.id).code_sealed), null);
});

test('a code is one seat: another install takes it, the first is kicked, the token verifies', () => {
  const store = new Store(tempDir());
  const pk = key();
  const kid = keyId(crypto.createPublicKey(pk));
  const t0 = Date.UTC(2026, 10, 13);
  const [trial] = makeKeys(store, { kind: 'trial', days: 14, count: 1, actor: 't' }, t0);
  const dev = device();
  const r = redeem(store, pk, kid, { code: trial.code.toLowerCase(), device: dev }, t0 + 1000);
  assert.equal(r.status, 200);
  const [, payload, sig] = r.body.token.split('.');
  const grant = JSON.parse(Buffer.from(payload, 'base64url'));
  assert.deepEqual([grant.kind, grant.dev, grant.kid, grant.exp, grant.seat], ['trial', dev, kid, Math.floor((t0 + 1000) / 1000) + 14 * 86400, 1]);
  const ok = crypto.verify('sha256', Buffer.from('HL1.' + payload), { key: crypto.createPublicKey(pk), dsaEncoding: 'ieee-p1363' }, Buffer.from(sig, 'base64url'));
  assert.ok(ok);
  const seatOf = (res) => JSON.parse(Buffer.from(res.body.token.split('.')[1], 'base64url')).seat;
  // The same install again (a reinstall): the same seat, idempotent.
  const again = redeem(store, pk, kid, { code: trial.code, device: dev }, t0 + 5000);
  assert.equal(again.status, 200);
  assert.equal(seatOf(again), 1);
  assert.equal(store.byId(trial.id).seat, 1);
  assert.equal(status(store, { lic: trial.id, device: dev, seat: 1 }, t0 + 5500).body.state, 'active');
  // Another install: takes the seat; the days keep counting from the first redemption.
  const dev2 = device();
  const moved = redeem(store, pk, kid, { code: trial.code, device: dev2 }, t0 + 6000);
  assert.equal(moved.status, 200);
  assert.equal(seatOf(moved), 2);
  assert.equal(JSON.parse(Buffer.from(moved.body.token.split('.')[1], 'base64url')).exp, grant.exp);
  // The first install is kicked, with or without its seat number; the second stands.
  assert.equal(status(store, { lic: trial.id, device: dev, seat: 1 }, t0 + 7000).body.state, 'revoked');
  assert.equal(status(store, { lic: trial.id, device: dev }, t0 + 7000).body.state, 'revoked');
  assert.equal(status(store, { lic: trial.id, device: dev2, seat: 2 }, t0 + 7000).body.state, 'active');
  // The first takes it back (reinstall, same code): the second is kicked, and an old token's seat no longer stands.
  const back = redeem(store, pk, kid, { code: trial.code, device: dev }, t0 + 8000);
  assert.equal(seatOf(back), 3);
  assert.equal(status(store, { lic: trial.id, device: dev2, seat: 2 }, t0 + 9000).body.state, 'revoked');
  assert.equal(status(store, { lic: trial.id, device: dev, seat: 1 }, t0 + 9000).body.state, 'revoked', 'a token from an earlier seat');
  assert.equal(status(store, { lic: trial.id, device: dev, seat: 3 }, t0 + 9000).body.state, 'active');
  assert.equal(status(store, { lic: trial.id, device: dev, seat: -1 }, t0).status, 400);
  // After its days: expired, for every install.
  assert.equal(redeem(store, pk, kid, { code: trial.code, device: dev }, t0 + 15 * 86_400_000).body.error, 'expired');
  assert.equal(redeem(store, pk, kid, { code: trial.code, device: device() }, t0 + 15 * 86_400_000).body.error, 'expired');
  assert.equal(status(store, { lic: trial.id, device: dev }, t0 + 15 * 86_400_000).body.state, 'expired');
  // Another install asking about it learns nothing.
  assert.equal(status(store, { lic: trial.id, device: device() }, t0).body.state, 'revoked');
  assert.match(fs.readFileSync(store.auditFile, 'utf8'), /"action":"seat_move"/);
});

test('revoked, lapsed, unknown and malformed all fail closed', () => {
  const store = new Store(tempDir());
  const pk = key();
  const t0 = Date.UTC(2026, 10, 13);
  const [a, b] = makeKeys(store, { kind: 'pro', days: '', count: 2, actor: 't' }, t0);
  const dev = device();
  assert.equal(redeem(store, pk, 'k', { code: a.code, device: dev }, t0).status, 200);
  revoke(store, a.id, 't', t0 + 1);
  assert.equal(status(store, { lic: a.id, device: dev }, t0 + 2).body.state, 'revoked');
  assert.equal(redeem(store, pk, 'k', { code: a.code, device: dev }, t0 + 2).status, 403);
  assert.equal(redeem(store, pk, 'k', { code: b.code, device: dev }, t0 + (LIMITS.redeemWindowDays + 1) * 86_400_000).status, 410, 'lapsed');
  assert.equal(redeem(store, pk, 'k', { code: newCode(), device: dev }, t0).status, 404);
  for (const bad of [{}, { code: 'HALO-1', device: dev }, { code: b.code, device: 'x' }, { code: b.code }, { code: 42, device: dev }]) {
    assert.equal(redeem(store, pk, 'k', bad, t0).status, 400, JSON.stringify(bad));
  }
  assert.equal(stateOf(store.byId(b.id), t0), 'unused');
  assert.throws(() => makeKeys(store, { kind: 'trial', days: '', count: 1, actor: 't' }), /length in days/);
  assert.throws(() => makeKeys(store, { kind: 'pro', days: 0, count: 1, actor: 't' }), /Days/);
  assert.throws(() => makeKeys(store, { kind: 'pro', days: 5, count: 51, actor: 't' }), /How many/);
  assert.throws(() => makeKeys(store, { kind: 'gold', days: 5, count: 1, actor: 't' }), /Pick/);
});

test('the limiter counts within its window', () => {
  const l = new Limiter(2, 1000);
  assert.ok(l.take('a', 0) && l.take('a', 10));
  assert.ok(!l.take('a', 20));
  assert.ok(l.take('a', 1500), 'the window moved on');
  assert.ok(l.take('b', 20));
});

// ---------- over HTTP ----------

function listen(handler) {
  return new Promise((resolve) => {
    const s = http.createServer(handler).listen(0, '127.0.0.1', () => resolve(s));
  });
}

async function call(server, method, path, { body, headers = {} } = {}) {
  const { port } = server.address();
  const res = await fetch(`http://127.0.0.1:${port}${path}`, { method, body, headers, redirect: 'manual' });
  return { status: res.status, headers: res.headers, text: await res.text() };
}

test('the portal needs the token, rate-limits sign-in, checks CSRF and audits', async () => {
  assert.throws(() => createLicenseServer({ dataDir: tempDir(), privateKey: key(), adminToken: 'short' }), /32 characters/);
  const srv = createLicenseServer({ dataDir: tempDir(), privateKey: key(), adminToken: TOKEN });
  const s = await listen(srv.admin);
  try {
    assert.equal((await call(s, 'GET', '/')).headers.get('location'), '/login');
    const form = { 'Content-Type': 'application/x-www-form-urlencoded' };
    for (let i = 0; i < LIMITS.loginFailures; i++) {
      assert.equal((await call(s, 'POST', '/login', { body: 'token=wrong', headers: form })).status, 401);
    }
    assert.equal((await call(s, 'POST', '/login', { body: `token=${TOKEN}`, headers: form })).status, 429, 'blocked after the failures, even with the right token');
    srv.store.audit({ action: 'test' });
    // A fresh server (no failures yet) signs in.
    const srv2 = createLicenseServer({ dataDir: tempDir(), privateKey: key(), adminToken: TOKEN });
    const s2 = await listen(srv2.admin);
    try {
      const login = await call(s2, 'POST', '/login', { body: `token=${TOKEN}`, headers: form });
      assert.equal(login.status, 303);
      const cookie = login.headers.get('set-cookie');
      assert.match(cookie, /HttpOnly/);
      assert.match(cookie, /SameSite=Strict/);
      const sid = cookie.split(';')[0];
      const home = await call(s2, 'GET', '/', { headers: { cookie: sid } });
      assert.equal(home.status, 200);
      assert.match(home.headers.get('content-security-policy'), /default-src 'none'/);
      const csrf = home.text.match(/name="csrf" value="([^"]+)"/)[1];
      // Without the CSRF token: refused.
      assert.equal((await call(s2, 'POST', '/keys', { body: 'kind=pro&count=1', headers: { ...form, cookie: sid } })).status, 403);
      const made = await call(s2, 'POST', '/keys', { body: `csrf=${csrf}&kind=trial&days=7&count=2&note=demo`, headers: { ...form, cookie: sid } });
      assert.equal(made.status, 200);
      const codes = [...made.text.matchAll(/<code>(HALO-[^<]+)<\/code>/g)].map((m) => m[1]);
      assert.equal(codes.length, 2);
      // The list afterwards has ids, never the codes.
      const again = await call(s2, 'GET', '/', { headers: { cookie: sid } });
      assert.ok(codes.every((c) => !again.text.includes(c)));
      // View key: shows the whole code again, needs the CSRF token, and is audited by license id.
      const id = srv2.store.byHash(codeHash(codes[0])).id;
      assert.match(again.text, /action="\/view"/);
      assert.equal((await call(s2, 'POST', '/view', { body: `id=${id}`, headers: { ...form, cookie: sid } })).status, 403);
      const viewed = await call(s2, 'POST', '/view', { body: `csrf=${csrf}&id=${id}`, headers: { ...form, cookie: sid } });
      assert.equal(viewed.status, 200);
      assert.ok(viewed.text.includes(codes[0]) && !viewed.text.includes(codes[1]));
      assert.equal(viewed.headers.get('cache-control'), 'no-store');
      assert.equal((await call(s2, 'POST', '/view', { body: `csrf=${csrf}&id=${'0'.repeat(32)}`, headers: { ...form, cookie: sid } })).status, 404);
      const audit = fs.readFileSync(srv2.store.auditFile, 'utf8');
      assert.match(audit, /"action":"login"/);
      assert.match(audit, /"action":"create"/);
      assert.ok(audit.includes(`"action":"view_key","actor":"admin@127.0.0.1","id":"${id}"`));
      assert.ok(!fs.readFileSync(srv2.store.file, 'utf8').includes(codes[0]), 'kept sealed, never plain');
      assert.ok(codes.every((c) => !audit.includes(c)) && !audit.includes(TOKEN), 'no code or token in the audit');
    } finally {
      s2.close();
    }
  } finally {
    s.close();
  }
});

test('the portal serves its two fonts and its CSS, cached, and nothing else from disk', async () => {
  const srv = createLicenseServer({ dataDir: tempDir(), privateKey: key(), adminToken: TOKEN });
  const s = await listen(srv.admin);
  try {
    for (const p of ['/fonts/outfit-400.woff2', '/fonts/outfit-600.woff2?v=1', '/admin.css?v=1']) {
      const r = await call(s, 'GET', p);
      assert.equal(r.status, 200, p);
      assert.match(r.headers.get('cache-control'), /max-age=31536000, immutable/, p);
    }
    assert.equal((await call(s, 'GET', '/fonts/outfit-400.woff2')).headers.get('content-type'), 'font/woff2');
    // Anything else is never a file: signed out, the sign-in page; signed in, 404.
    const login = await call(s, 'POST', '/login', { body: `token=${TOKEN}`, headers: { 'Content-Type': 'application/x-www-form-urlencoded' } });
    const cookie = login.headers.get('set-cookie').split(';')[0];
    for (const p of ['/fonts/outfit-700.woff2', '/fonts/../license-server.mjs', '/fonts/%2e%2e/license-server.mjs', '/fonts/']) {
      assert.equal((await call(s, 'GET', p)).status, 303, p);
      assert.equal((await call(s, 'GET', p, { headers: { cookie } })).status, 404, p);
    }
    const csp = (await call(s, 'GET', '/login')).headers.get('content-security-policy');
    assert.match(csp, /font-src 'self'/);
    assert.match(csp, /default-src 'none'/);
    assert.doesNotMatch(csp, /script-src/);
    assert.equal((await call(s, 'GET', '/login')).headers.get('cache-control'), 'no-store');
  } finally {
    s.close();
  }
});

test('the redeem API takes JSON only and rate-limits', async () => {
  const srv = createLicenseServer({ dataDir: tempDir(), privateKey: key(), adminToken: TOKEN });
  const [k] = makeKeys(srv.store, { kind: 'pro', days: '', count: 1, actor: 't' });
  const s = await listen(srv.redeem);
  try {
    const json = { 'Content-Type': 'application/json' };
    assert.equal((await call(s, 'POST', '/api/redeem', { body: 'code=x', headers: { 'Content-Type': 'text/plain' } })).status, 415);
    assert.equal((await call(s, 'POST', '/api/redeem', { body: '{', headers: json })).status, 400);
    const ok = await call(s, 'POST', '/api/redeem', { body: JSON.stringify({ code: k.code, device: device() }), headers: json });
    assert.equal(ok.status, 200);
    assert.match(JSON.parse(ok.text).token, /^HL1\./);
    let last;
    for (let i = 0; i < LIMITS.redeemPerMinute + 1; i++) {
      last = await call(s, 'POST', '/api/redeem', { body: JSON.stringify({ code: newCode(), device: device() }), headers: json });
    }
    assert.equal(last.status, 429);
    assert.equal((await call(s, 'GET', '/api/redeem')).status, 404);
  } finally {
    s.close();
  }
});

test('init-key writes the private key owner-only and prints the public line', async () => {
  const { execFileSync } = await import('node:child_process');
  const file = `${tempDir()}/signing.pem`;
  const out = execFileSync(process.execPath, [new URL('../license-server.mjs', import.meta.url).pathname, 'init-key', '--key', file]).toString().trim();
  const [id, point] = out.split(' ');
  const pub = crypto.createPublicKey(crypto.createPrivateKey(fs.readFileSync(file)));
  assert.equal(id, keyId(pub));
  assert.equal(point, publicPoint(pub).toString('hex'));
  if (process.platform !== 'win32') assert.equal(fs.statSync(file).mode & 0o777, 0o600);
  assert.throws(() => execFileSync(process.execPath, [new URL('../license-server.mjs', import.meta.url).pathname, 'init-key', '--key', file], { stdio: 'pipe' }));
});

// The license server: keys, redemption, the admin portal's guards.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import { tempDir } from './helpers.mjs';
import {
  createLicenseServer, isTailnetOrLoopback, newCode, normalizeCode, codeHash, makeKeys, redeem, status, revoke, stateOf, Store, Limiter, keyId, publicPoint, LIMITS,
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

test('codes are random, well formed and stored only as a hash', () => {
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
  const [made] = makeKeys(store, { kind: 'pro', days: '', count: 1, note: 'x', actor: 't' });
  const disk = fs.readFileSync(store.file, 'utf8') + fs.readFileSync(store.auditFile, 'utf8');
  assert.ok(!disk.includes(made.code), 'the code itself never reaches the disk');
  assert.ok(disk.includes(codeHash(made.code)));
});

test('a code is redeemed once, for one install, and the token verifies', () => {
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
  assert.deepEqual([grant.kind, grant.dev, grant.kid, grant.exp], ['trial', dev, kid, Math.floor((t0 + 1000) / 1000) + 14 * 86400]);
  const ok = crypto.verify('sha256', Buffer.from('HL1.' + payload), { key: crypto.createPublicKey(pk), dsaEncoding: 'ieee-p1363' }, Buffer.from(sig, 'base64url'));
  assert.ok(ok);
  // The same install again (a reinstall): the same license.
  assert.equal(redeem(store, pk, kid, { code: trial.code, device: dev }, t0 + 5000).status, 200);
  // Another install: refused.
  assert.deepEqual(redeem(store, pk, kid, { code: trial.code, device: device() }, t0 + 6000), { status: 409, body: { error: 'already_used' } });
  // After its days: expired, for its own install too.
  assert.equal(redeem(store, pk, kid, { code: trial.code, device: dev }, t0 + 15 * 86_400_000).body.error, 'expired');
  // Status: active until the end, then expired.
  assert.equal(status(store, { lic: trial.id, device: dev }, t0 + 2000).body.state, 'active');
  assert.equal(status(store, { lic: trial.id, device: dev }, t0 + 15 * 86_400_000).body.state, 'expired');
  // Another install asking about it learns nothing.
  assert.equal(status(store, { lic: trial.id, device: device() }, t0).body.state, 'revoked');
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
      // Shown once: the list afterwards has ids, never the codes.
      const again = await call(s2, 'GET', '/', { headers: { cookie: sid } });
      assert.ok(codes.every((c) => !again.text.includes(c)));
      const audit = fs.readFileSync(srv2.store.auditFile, 'utf8');
      assert.match(audit, /"action":"login"/);
      assert.match(audit, /"action":"create"/);
      assert.ok(codes.every((c) => !audit.includes(c)) && !audit.includes(TOKEN), 'no code or token in the audit');
    } finally {
      s2.close();
    }
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

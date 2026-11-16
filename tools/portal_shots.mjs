#!/usr/bin/env node
// Screenshots of the license admin portal (server/license-server.mjs) for
// review: every page state, light and dark, desktop and phone.
//
//     node tools/portal_shots.mjs [out-dir]     (default smoke-out/portal)
//
// Runs the portal in this process on a throwaway data dir, key and token
// (never the real ones), seeds licenses in every state, signs in over HTTP,
// keeps each page's HTML with its real headers (CSP too), and serves them to
// headless Chrome through a loopback proxy that adds the session cookie.
// Fails if Chrome reports a Content Security Policy violation. Writes
// <n>-<state>.<light|dark>.<desktop|phone>.png and index.html, a grid of them.
import crypto from 'node:crypto';
import fs from 'node:fs';
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import { execFile } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { codeKey, createLicenseServer, makeKeys, redeem, revoke } from '../server/license-server.mjs';

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), '..');
const OUT = path.resolve(process.argv[2] ?? path.join(ROOT, 'smoke-out', 'portal'));
const CHROME = process.env.CHROME ?? 'google-chrome';
const DAY = 86_400_000;
const TOKEN = crypto.randomBytes(24).toString('hex');
const FORM = { 'Content-Type': 'application/x-www-form-urlencoded' };

const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'portal-shots-'));
const privateKey = crypto.generateKeyPairSync('ec', { namedCurve: 'P-256' }).privateKey;
const srv = createLicenseServer({ dataDir: path.join(tmp, 'data'), privateKey, adminToken: TOKEN });
const listen = (h) => new Promise((r) => { const s = http.createServer(h).listen(0, '127.0.0.1', () => r(s)); });
const portal = await listen(srv.admin);
const base = `http://127.0.0.1:${portal.address().port}`;
const go = async (p, init = {}) => {
  const r = await fetch(base + p, { redirect: 'manual', ...init });
  return { status: r.status, headers: Object.fromEntries(r.headers), body: await r.text() };
};

// ---- the states, as served ----
const snaps = new Map(); // name -> { headers, body }
const keep = (name, r) => snaps.set(name, r);
keep('01-login', await go('/login'));
keep('02-login-error', await go('/login', { method: 'POST', headers: FORM, body: 'token=wrong' }));
const sid = (await go('/login', { method: 'POST', headers: FORM, body: `token=${TOKEN}` })).headers['set-cookie'].split(';')[0];
const get = (p) => go(p, { headers: { cookie: sid } });
keep('03-keys-empty', await get('/'));

const st = srv.store;
const now = Date.now();
const dev = () => crypto.randomBytes(32).toString('hex');
const mk = (kind, days, note, ago) => makeKeys(st, { kind, days, count: 1, note, actor: 'admin@100.99.255.105', sealKey: codeKey(privateKey) }, now - ago * DAY)[0];
const use = (k, ago) => redeem(st, privateKey, srv.kid, { code: k.code, device: dev() }, now - ago * DAY);
const moved = mk('pro', '', 'Reviewer copy, tech press', 20);
use(moved, 19);
use(moved, 4); // the seat moves to a second PC
use(mk('trial', 30, 'Beta tester: Kuala Lumpur meetup', 6), 5);
use(mk('pro', 365, 'Annual, paid by bank transfer', 2), 1);
use(mk('trial', 7, 'Trial from the October newsletter', 40), 30);
use(mk('trial', 14, '', 25), 24);
mk('pro', '', 'Giveaway winner, not redeemed yet', 3);
mk('trial', 14, 'A very long note that keeps going so the table has to cut it short with an ellipsis', 1);
mk('pro', '', '', 0);
mk('trial', 7, 'Expired before anyone used it', 120);
mk('pro', '', 'Left over from the launch batch', 95);
revoke(st, mk('pro', '', 'Leaked in a public chat', 10).id, 'admin@100.99.255.105', now - 9 * DAY);
const r2 = mk('trial', 30, 'Refunded', 8);
use(r2, 8);
revoke(st, r2.id, 'admin@100.99.255.105', now - 7 * DAY);

const full = await get('/');
keep('04-keys-full', full);
keep('05-keys-revoke-open', { ...full, body: full.body.replace('<details>', '<details open>') });
const csrf = full.body.match(/name="csrf" value="([^"]+)"/)[1];
const post = (p, b) => go(p, { method: 'POST', headers: { ...FORM, cookie: sid }, body: `csrf=${encodeURIComponent(csrf)}&${b}` });
keep('06-keys-made', await post('/keys', 'kind=trial&days=14&count=3&note=Shots'));
keep('07-keys-error', await post('/keys', 'kind=pro&days=&count=0&note='));
keep('08-audit', await get('/audit'));
keep('09-key-viewed', await post('/view', `id=${st.data.licenses[0].id}`));

// ---- the proxy: snapshots by name, everything else from the portal with the cookie ----
const proxy = await listen(async (req, res) => {
  const snap = snaps.get(req.url.slice(1));
  const r = snap ? null : await fetch(base + req.url, { headers: { cookie: sid } });
  const headers = snap ? snap.headers : Object.fromEntries(r.headers);
  const body = snap ? snap.body : Buffer.from(await r.arrayBuffer());
  res.statusCode = snap ? 200 : r.status;
  for (const [k, v] of Object.entries(headers)) if (!['content-length', 'transfer-encoding', 'connection', 'keep-alive', 'date'].includes(k)) res.setHeader(k, v);
  res.end(body);
});
const pbase = `http://127.0.0.1:${proxy.address().port}`;

// ---- Chrome ----
// Chrome's --screenshot takes the window, not the page: windows tall enough for each page.
const SIZES = { desktop: [1440, 1], phone: [390, 2] }; // width, scale
const HEIGHT = { desktop: { login: 900, empty: 900, keys: 1500, audit: 1400 }, phone: { login: 844, empty: 1300, keys: 5000, audit: 3600 } };
const kindOf = (n) => (n.includes('login') ? 'login' : n.includes('empty') ? 'empty' : n.includes('audit') ? 'audit' : 'keys');
const run = (args) => new Promise((resolve) => execFile(CHROME, args, { timeout: 60_000 }, (e, out, err) => resolve({ e, err })));
fs.mkdirSync(OUT, { recursive: true });
const csp = [];
const jobs = [];
for (const name of snaps.keys()) for (const theme of ['light', 'dark']) for (const [size, [w, scale]] of Object.entries(SIZES)) jobs.push({ name, theme, size, w, scale, h: HEIGHT[size][kindOf(name)] });
await Promise.all(Array.from({ length: 4 }, async () => {
  for (let j; (j = jobs.shift()); ) {
    const file = `${j.name}.${j.theme}.${j.size}.png`;
    const { e, err } = await run([
      '--headless', '--disable-gpu', '--hide-scrollbars', '--no-first-run', '--no-default-browser-check',
      `--user-data-dir=${fs.mkdtempSync(path.join(tmp, 'chrome-'))}`, '--enable-logging=stderr', '--v=0',
      `--blink-settings=preferredColorScheme=${j.theme === 'dark' ? 0 : 1}`,
      `--window-size=${j.w},${j.h}`, `--force-device-scale-factor=${j.scale}`, '--virtual-time-budget=3000',
      `--screenshot=${path.join(OUT, file)}`, `${pbase}/${j.name}`,
    ]);
    if (e && !fs.existsSync(path.join(OUT, file))) throw new Error(`chrome failed on ${file}: ${err.slice(-400)}`);
    if (/Content Security Policy/i.test(err)) csp.push(`${file}: ${err.split('\n').find((l) => /Content Security Policy/i.test(l))}`);
    process.stdout.write('.');
  }
}));
console.log();

// ---- the grid ----
const cols = ['light.desktop', 'light.phone', 'dark.desktop', 'dark.phone'];
fs.writeFileSync(path.join(OUT, 'index.html'), `<!doctype html><meta charset="utf-8"><title>Portal shots</title>
<style>body{font:14px system-ui;margin:24px;background:#ddd}table{border-spacing:12px}td{vertical-align:top}th{text-align:left}img{display:block;border:1px solid #999;background:#fff}</style>
<table><tr><th></th>${cols.map((c) => `<th>${c}</th>`).join('')}</tr>
${[...snaps.keys()].map((n) => `<tr><th>${n}</th>${cols.map((c) => `<td><a href="${n}.${c}.png"><img src="${n}.${c}.png" width="${c.endsWith('phone') ? 195 : 360}"></a></td>`).join('')}</tr>`).join('\n')}
</table>`);

// ---- sizes ----
const sizes = [...snaps].map(([n, r]) => `${n}.html ${Buffer.byteLength(r.body)}`);
for (const p of ['/admin.css', '/fonts/outfit-400.woff2', '/fonts/outfit-600.woff2']) sizes.push(`${p} ${(await (await fetch(base + p)).arrayBuffer()).byteLength}`);
console.log(sizes.join('\n'));
console.log(`${snaps.size * 4} shots in ${OUT}`);
portal.close();
proxy.close();
fs.rmSync(tmp, { recursive: true, force: true });
if (csp.length) {
  console.error('CSP violations:\n' + csp.join('\n'));
  process.exit(1);
}

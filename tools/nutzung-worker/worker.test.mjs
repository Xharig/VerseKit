// Prüfung des Nutzungs-Workers mit Node (node --test). Keine echte Datenbank,
// kein echtes Access: `DB` wird untergeschoben, und für Access erzeugt die
// Prüfung einen eigenen RSA-Schlüssel und unterschreibt echte Zeichen. Jede
// Gruppe beginnt mit einer Vorbedingung, die durchgehen MUSS — sonst wäre
// jedes „abgelehnt" geschenkt.
import test from 'node:test';
import assert from 'node:assert/strict';
import worker, { verifyAccess } from './worker.js';

const TEAM = 'probe.cloudflareaccess.com';
const AUD = 'aud-probe-123';
const MAIL = 'probe@example.org';  // privacy-ok: erfundene Adresse, nur fuer den Test

function fakeDb(rows = { tage: [], merkmale: [] }) {
  const calls = [];
  const stmt = (sql) => ({
    bind(...args) {
      return {
        sql, args,
        run: async () => { calls.push({ sql, args }); return {}; },
        all: async () => ({ results: /FROM merkmale/.test(sql) ? rows.merkmale : rows.tage }),
      };
    },
  });
  return { calls, prepare: stmt, batch: async (list) => { for (const s of list) calls.push({ sql: s.sql, args: s.args }); return []; } };
}

function env(extra = {}) {
  return { DB: fakeDb(), LIMIT: { limit: async () => ({ success: true }) },
           TEAM_DOMAIN: TEAM, POLICY_AUD: AUD, ERLAUBTE_MAIL: MAIL, ...extra };
}

const FULL = { v: '3.65.0', os: 'windows', ui: 'de', game: 'en', rc: false,
               mods: ['handel', 'schiffe'], overlay: 'popup', autostart: true };

function ping(body, { cf = { country: 'DE', city: 'Berlin', latitude: '52.5' }, host = 'nutzung-versekit.xharig.com', method = 'POST' } = {}) {
  const req = new Request(`https://${host}/ping`, {
    method,
    body: method === 'POST' ? (typeof body === 'string' ? body : JSON.stringify(body)) : undefined,
    headers: { 'cf-connecting-ip': '203.0.113.7' },
  });
  Object.defineProperty(req, 'cf', { value: cf });
  return req;
}

// ---------------------------------------------------------------- Meldung

test('Vorbedingung: eine vollständige Meldung wird gezählt', async () => {
  const e = env();
  const r = await worker.fetch(ping(FULL), e);
  assert.equal(r.status, 204);
  const tage = e.DB.calls.filter((c) => /INTO tage/.test(c.sql));
  const merk = e.DB.calls.filter((c) => /INTO merkmale/.test(c.sql)).map((c) => c.args.slice(1).join('='));
  assert.equal(tage.length, 1);
  assert.deepEqual(tage[0].args.slice(1), ['3.65.0', 'windows', 'DE']);
  assert.deepEqual(merk.sort(), ['autostart=ja', 'game=en', 'mod=handel', 'mod=schiffe',
                                 'overlay=popup', 'rc=nein', 'ui=de']);
});

test('nur Version und System genügen auch', async () => {
  const e = env();
  assert.equal((await worker.fetch(ping({ v: '3.65.0', os: 'linux' }), e)).status, 204);
});

test('weder IP noch Stadt landen in der Datenbank', async () => {
  const e = env();
  await worker.fetch(ping(FULL), e);
  const stored = JSON.stringify(e.DB.calls);
  for (const secret of ['203.0.113.7', 'Berlin', '52.5']) assert.ok(!stored.includes(secret), secret);
});

test('unbekanntes oder seltsames Land wird XX', async () => {
  for (const cf of [{}, { country: '' }, { country: 'DROP' }, { country: 'd' }]) {
    const e = env();
    await worker.fetch(ping(FULL, { cf }), e);
    assert.equal(e.DB.calls[0].args[3], 'XX', JSON.stringify(cf));
  }
});

test('zusätzliche Felder (Kennung, Land) werden abgelehnt', async () => {
  for (const extra of [{ id: 'abc' }, { land: 'US' }, { name: 'x' }]) {
    const e = env();
    const r = await worker.fetch(ping({ ...FULL, ...extra }), e);
    assert.equal(r.status, 400, JSON.stringify(extra));
    assert.equal(e.DB.calls.length, 0);
  }
});

test('falsche Werte werden abgelehnt', async () => {
  const bad = [{ v: 'DROP TABLE' }, { os: 'mac' }, { ui: 'deutsch' }, { game: 'EN' },
               { rc: 'ja' }, { autostart: 1 }, { overlay: 'aus' },
               { mods: 'handel' }, { mods: ['C:/Users/x'] }, { mods: ['a', 'a'] },  // privacy-ok: erfundener Pfad, muss abgewiesen werden
               { mods: Array.from({ length: 13 }, (_, i) => 'm' + 'x'.repeat(i)) }];
  for (const b of bad) {
    const e = env();
    const r = await worker.fetch(ping({ ...FULL, ...b }), e);
    assert.equal(r.status, 400, JSON.stringify(b));
    assert.equal(e.DB.calls.length, 0);
  }
});

test('kein JSON und zu groß werden abgelehnt', async () => {
  const e = env();
  assert.equal((await worker.fetch(ping('hallo'), e)).status, 400);
  assert.equal((await worker.fetch(ping('x'.repeat(900)), e)).status, 413);
  assert.equal(e.DB.calls.length, 0);
});

test('Mengenbremse und fehlende Einrichtung', async () => {
  const e = env({ LIMIT: { limit: async () => ({ success: false }) } });
  assert.equal((await worker.fetch(ping(FULL), e)).status, 429);
  assert.equal((await worker.fetch(ping(FULL), { ...env(), DB: undefined })).status, 503);
});

test('im Browser aufgerufen zeigt /ping nichts', async () => {
  const r = await worker.fetch(ping(null, { method: 'GET' }), env());
  assert.equal(r.status, 405);
});

// ---------------------------------------------------------------- Access

const keys = await crypto.subtle.generateKey(
  { name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' },
  true, ['sign', 'verify']);
const otherKeys = await crypto.subtle.generateKey(
  { name: 'RSASSA-PKCS1-v1_5', modulusLength: 2048, publicExponent: new Uint8Array([1, 0, 1]), hash: 'SHA-256' },
  true, ['sign', 'verify']);
const jwk = { ...(await crypto.subtle.exportKey('jwk', keys.publicKey)), kid: 'k1', alg: 'RS256' };
const b64 = (obj) => Buffer.from(typeof obj === 'string' ? obj : JSON.stringify(obj)).toString('base64url');

async function token(claims = {}, { key = keys.privateKey, kid = 'k1' } = {}) {
  const now = Math.floor(Date.now() / 1000);
  const body = { aud: [AUD], iss: `https://${TEAM}`, email: MAIL, exp: now + 600, iat: now, ...claims };
  const head = b64({ alg: 'RS256', kid, typ: 'JWT' });
  const data = `${head}.${b64(body)}`;
  const sig = await crypto.subtle.sign('RSASSA-PKCS1-v1_5', key, new TextEncoder().encode(data));
  return `${data}.${Buffer.from(sig).toString('base64url')}`;
}

const certs = async () => new Response(JSON.stringify({ keys: [jwk] }));

function statsReq(path, jwt, host = 'statistik-versekit.xharig.com') {
  return new Request(`https://${host}${path}`, { headers: jwt ? { 'cf-access-jwt-assertion': jwt } : {} });
}

const verify = (req, e) => verifyAccess(req, e, certs);

test('Vorbedingung: ein echtes Access-Zeichen öffnet die Übersicht', async () => {
  globalThis.fetch = async () => new Response('[]');
  const r = await worker.fetch(statsReq('/', await token()), env(), undefined, verify);
  assert.equal(r.status, 200);
  const csp = r.headers.get('content-security-policy');
  assert.match(csp, /script-src 'nonce-[0-9a-f]{32}'/);
  assert.equal(r.headers.get('x-frame-options'), 'DENY');
  const html = await r.text();
  const nonce = /nonce-([0-9a-f]{32})/.exec(csp)[1];
  assert.ok(html.includes(`<script nonce="${nonce}">`));
});

test('Vorbedingung: die Daten kommen mit Zeichen als JSON', async () => {
  globalThis.fetch = async () => new Response('[]');
  const r = await worker.fetch(statsReq('/daten?tage=30', await token()), env(), undefined, verify);
  assert.equal(r.status, 200);
  const j = await r.json();
  assert.ok(Array.isArray(j.tage) && Array.isArray(j.merkmale) && Array.isArray(j.downloads));
});

test('ohne, mit gefälschtem oder falschem Zeichen bleibt die Tür zu', async () => {
  const now = Math.floor(Date.now() / 1000);
  const cases = {
    'kein Zeichen': null,
    'Unsinn': 'a.b.c',
    'fremder Schlüssel': await token({}, { key: otherKeys.privateKey }),
    'unbekannte kid': await token({}, { kid: 'k2' }),
    'falsche Zielgruppe': await token({ aud: ['fremd'] }),
    'falscher Aussteller': await token({ iss: 'https://boese.cloudflareaccess.com' }),
    'abgelaufen': await token({ exp: now - 10 }),
    'andere Mail': await token({ email: 'jemand@example.org' }),
  };
  for (const [name, jwt] of Object.entries(cases)) {
    for (const path of ['/', '/daten']) {
      const r = await worker.fetch(statsReq(path, jwt), env(), undefined, verify);
      assert.equal(r.status, 403, `${name} ${path}`);
    }
  }
});

test('veränderte Angaben im Zeichen brechen die Unterschrift', async () => {
  const [h, , sig] = (await token()).split('.');
  const forged = `${h}.${b64({ aud: [AUD], iss: `https://${TEAM}`, email: MAIL, exp: 9999999999 })}.${sig}`;
  const r = await worker.fetch(statsReq('/', forged), env(), undefined, verify);
  assert.equal(r.status, 403);
});

test('fehlt eine Einstellung, bleibt die Übersicht zu', async () => {
  for (const missing of ['TEAM_DOMAIN', 'POLICY_AUD', 'ERLAUBTE_MAIL']) {
    const r = await worker.fetch(statsReq('/', await token()), env({ [missing]: '' }), undefined, verify);
    assert.equal(r.status, 403, missing);
  }
});

test('die Übersicht gibt es NUR unter statistik-versekit.xharig.com', async () => {
  const jwt = await token();
  for (const host of ['nutzung-versekit.xharig.com', 'versekit-nutzung.xharig.workers.dev']) {
    for (const path of ['/', '/daten']) {
      const r = await worker.fetch(statsReq(path, jwt, host), env(), undefined, verify);
      assert.equal(r.status, 404, `${host}${path}`);
    }
  }
});

test('die Seite lädt nichts von außen', async () => {
  const { dashboardHtml } = await import('./dashboard.js');
  const html = dashboardHtml('abc');
  assert.ok(!/(src|href)=["']https?:/i.test(html), 'fremde Quelle in der Seite');
});

// ---------------------------------------------------------------- Kurzlinks

function short(path, { agent = 'Mozilla/5.0 (Windows NT 10.0) Firefox/130.0', method = 'GET', cf = { country: 'RU', city: 'Moskau' } } = {}) {
  const req = new Request(`https://xharig.com${path}`, { method, headers: { 'user-agent': agent, 'cf-connecting-ip': '203.0.113.9' } });
  Object.defineProperty(req, 'cf', { value: cf });
  return req;
}
function ctxCollect() { const p = []; return { p, waitUntil: (x) => p.push(x) }; }

test('Vorbedingung: ein Download über den Kurzlink wird gezählt und weitergeleitet', async () => {
  const e = env(); const c = ctxCollect();
  const r = await worker.fetch(short('/windows'), e, c);
  await Promise.all(c.p);
  assert.equal(r.status, 302);
  assert.equal(r.headers.get('location'), 'https://github.com/Xharig/VerseKit/releases/latest/download/VerseKit-Setup.exe');
  const dl = e.DB.calls.filter((x) => /INTO downloads/.test(x.sql));
  assert.equal(dl.length, 1);
  assert.deepEqual(dl[0].args.slice(1), ['windows', 'RU']);
});

test('Linux führt zum AppImage', async () => {
  const r = await worker.fetch(short('/linux'), env(), ctxCollect());
  assert.equal(r.headers.get('location'), 'https://github.com/Xharig/VerseKit/releases/latest/download/VerseKit-x86_64.AppImage');
});

test('Vorschau-Roboter und Werkzeuge zählen nicht — werden aber weitergeleitet', async () => {
  for (const agent of ['Mozilla/5.0 (compatible; Discordbot/2.0)', 'TelegramBot (like TwitterBot)',
                       'WhatsApp/2.23', 'curl/8.4.0', 'Googlebot/2.1', '']) {
    const e = env(); const c = ctxCollect();
    const r = await worker.fetch(short('/windows', { agent }), e, c);
    await Promise.all(c.p);
    assert.equal(r.status, 302, agent);
    assert.equal(e.DB.calls.length, 0, agent);
  }
});

test('HEAD zählt nicht, schnelle Wiederholung zählt nicht', async () => {
  let e = env(); let c = ctxCollect();
  await worker.fetch(short('/windows', { method: 'HEAD' }), e, c); await Promise.all(c.p);
  assert.equal(e.DB.calls.length, 0);
  e = env({ LIMIT: { limit: async () => ({ success: false }) } }); c = ctxCollect();
  const r = await worker.fetch(short('/windows'), e, c); await Promise.all(c.p);
  assert.equal(r.status, 302);
  assert.equal(e.DB.calls.length, 0);
});

test('klemmt die Datenbank, kommt der Download trotzdem', async () => {
  const e = env({ DB: { prepare: () => ({ bind: () => ({ run: async () => { throw new Error('weg'); } }) }) } });
  const c = ctxCollect();
  const r = await worker.fetch(short('/windows'), e, c);
  await Promise.all(c.p);
  assert.equal(r.status, 302);
});

test('vom Download wird weder IP noch Stadt gespeichert', async () => {
  const e = env(); const c = ctxCollect();
  await worker.fetch(short('/windows'), e, c); await Promise.all(c.p);
  const stored = JSON.stringify(e.DB.calls);
  assert.ok(!stored.includes('203.0.113.9') && !stored.includes('Moskau'), stored);
});

test('andere Pfade auf xharig.com gehören nicht dem Worker', async () => {
  const r = await worker.fetch(short('/discord'), env(), ctxCollect());
  assert.equal(r.status, 404);
});

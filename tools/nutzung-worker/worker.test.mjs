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
    all: async () => ({ results: [] }),
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

function env0() { return { LIMIT: { limit: async () => ({ success: true }) }, TEAM_DOMAIN: TEAM, POLICY_AUD: AUD, ERLAUBTE_MAIL: MAIL }; }
function env(extra = {}) {
  return { DB: fakeDb(), LIMIT: { limit: async () => ({ success: true }) },
           TEAM_DOMAIN: TEAM, POLICY_AUD: AUD, ERLAUBTE_MAIL: MAIL, ...extra };
}

const FULL = { v: '3.65.0', os: 'windows', ui: 'de', game: 'en', rc: false,
               mods: ['handel', 'schiffe'], overlay: 'popup', autostart: true,
               update: false };

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
                                 'overlay=popup', 'rc=nein', 'ui=de', 'update=nein']);
});

test('ältere Fassungen ohne Feld update werden weiter gezählt', async () => {
  const e = env();
  const { update, ...old } = FULL;
  assert.equal(update, false);
  assert.equal((await worker.fetch(ping(old), e)).status, 204);
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
               { rc: 'ja' }, { autostart: 1 }, { update: 'an' }, { overlay: 'aus' },
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
  assert.equal((await worker.fetch(ping('x'.repeat(MAX_BYTES + 1)), e)).status, 413);
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

test('alle vier Schreibweisen der Kurzlinks führen zur Datei', async () => {
  for (const url of ['https://xharig.com/windows', 'https://xharig.com/windows/',
                     'https://www.xharig.com/windows', 'https://www.xharig.com/linux/']) {
    const req = new Request(url, { headers: { 'user-agent': 'Mozilla/5.0 Firefox/130.0' } });
    Object.defineProperty(req, 'cf', { value: { country: 'DE' } });
    const r = await worker.fetch(req, env(), ctxCollect());
    assert.equal(r.status, 302, url);
    assert.match(r.headers.get('location'), /releases\/latest\/download\//, url);
  }
});

// ---------------------------------------------------------------- GitHub-Zahlen

import { downloads } from './worker.js';

function ablageDb() {
  const map = new Map();
  return {
    map,
    prepare(sql) {
      return { bind(...args) { return {
        all: async () => {
          const row = map.get(args[0]);
          return { results: row ? [row] : [] };
        },
        run: async () => { map.set(args[0], { inhalt: args[1], zeit: args[2] }); return {}; },
      }; } };
    },
  };
}
const rel = (tag, exe, app) => ({ tag_name: tag, published_at: '2026-10-01T00:00:00Z', prerelease: false,
  assets: [{ name: 'VerseKit-Setup.exe', download_count: exe }, { name: 'VerseKit-x86_64.AppImage', download_count: app }] });

test('Vorbedingung: frische Zahlen werden geholt und abgelegt', async () => {
  const env = { DB: ablageDb() };
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.65.0', 4, 1)]), { headers: { etag: '"a1"' } });
  const r = await downloads(env, 1_000_000);
  assert.equal(r.alt, false);
  assert.deepEqual(r.liste.map((x) => [x.tag, x.windows, x.linux]), [['v3.65.0', 4, 1]]);
  assert.ok(env.DB.map.has('downloads') && env.DB.map.has('seite:1'));
});

test('innerhalb einer Minute wird GitHub gar nicht gefragt', async () => {
  const env = { DB: ablageDb() };
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.65.0', 4, 1)]));
  await downloads(env, 1_000_000);
  let asked = 0;
  globalThis.fetch = async () => { asked++; return new Response('[]'); };
  const r = await downloads(env, 1_000_000 + 30_000);
  assert.equal(asked, 0);
  assert.equal(r.liste.length, 1);
});

test('GitHub sperrt: die gespeicherten Zahlen bleiben, mit Uhrzeit und Vermerk', async () => {
  const env = { DB: ablageDb() };
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.65.0', 4, 1)]), { headers: { etag: '"a1"' } });
  await downloads(env);
  const zeit = env.DB.map.get('downloads').zeit;
  globalThis.fetch = async () => new Response('rate limit', { status: 403 });
  const r = await downloads(env, zeit + 11 * 60 * 1000);
  assert.equal(r.alt, true);
  assert.equal(r.stand, zeit);
  assert.equal(r.liste.length, 1, 'die Seite darf nicht leer werden');
});

test('304 („nichts geändert") nutzt die abgelegte Seite und schickt das ETag mit', async () => {
  const env = { DB: ablageDb() };
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.65.0', 4, 1)]), { headers: { etag: '"a1"' } });
  await downloads(env);
  const zeit = env.DB.map.get('downloads').zeit;
  let sent = null;
  globalThis.fetch = async (url, init) => { sent = init.headers['if-none-match']; return new Response(null, { status: 304 }); };
  const r = await downloads(env, zeit + 11 * 60 * 1000);
  assert.equal(sent, '"a1"');
  assert.equal(r.alt, false);
  assert.equal(r.liste[0].windows, 4);
});

// ---------------------------------------------------------------- Mitschreiben (echtes SQLite)

import { DatabaseSync } from 'node:sqlite';
import { readFileSync } from 'node:fs';
import { snapshot, allowed } from './worker.js';

function realD1() {
  const db = new DatabaseSync(':memory:');
  db.exec(readFileSync(new URL('./schema.sql', import.meta.url), 'utf-8'));
  const stmt = (sql, args = []) => ({
    sql, args,
    bind: (...a) => stmt(sql, a),
    run: async () => { db.prepare(sql).run(...args); return {}; },
    all: async () => ({ results: db.prepare(sql).all(...args) }),
  });
  return { db, prepare: (sql) => stmt(sql), batch: async (list) => { for (const s of list) await s.run(); return []; } };
}

test('Vorbedingung: der Schnappschuss schreibt Bestand und Tageszeile', async () => {
  const env = { DB: realD1() };
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.65.0', 4, 1), rel('v3.64.2', 7, 3)]));
  const r = await snapshot(env, Date.parse('2026-10-01T12:00:00Z'));
  assert.equal(r.aktiv, 15);
  const b = env.DB.db.prepare('SELECT version, gesamt FROM download_bestand ORDER BY version').all();
  assert.deepEqual(b.map((x) => [x.version, x.gesamt]), [['v3.64.2', 10], ['v3.65.0', 5]]);
  const v = env.DB.db.prepare('SELECT * FROM download_verlauf').all();
  assert.equal(v.length, 1);
  assert.equal(v[0].je_gesehen, 15);
});

test('ein gelöschtes Release bleibt im Bestand und zählt weiter', async () => {
  const env = { DB: realD1() };
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.65.0-rc1', 9, 2), rel('v3.64.2', 7, 3)]));
  await snapshot(env, Date.parse('2026-10-01T12:00:00Z'));
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.64.2', 8, 3)]));     // rc gelöscht
  const r = await snapshot(env, Date.parse('2026-10-02T12:00:00Z'));
  assert.equal(r.aktiv, 11, 'GitHub kennt nur noch 11');
  assert.equal(r.je_gesehen, 22, 'je gesehen: 11 (rc1) + 11 (v3.64.2)');
  const rc = env.DB.db.prepare("SELECT gesamt, zuletzt FROM download_bestand WHERE version = 'v3.65.0-rc1'").get();
  assert.equal(rc.gesamt, 11);
  assert.equal(rc.zuletzt, '2026-10-01');
});

test('ein Zähler sinkt nie, auch wenn GitHub weniger meldet', async () => {
  const env = { DB: realD1() };
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.64.2', 7, 3)]));
  await snapshot(env, Date.parse('2026-10-01T12:00:00Z'));
  globalThis.fetch = async () => new Response(JSON.stringify([rel('v3.64.2', 5, 1)]));
  await snapshot(env, Date.parse('2026-10-02T12:00:00Z'));
  const b = env.DB.db.prepare("SELECT windows, linux, gesamt FROM download_bestand WHERE version = 'v3.64.2'").get();
  assert.deepEqual([b.windows, b.linux, b.gesamt], [7, 3, 10]);
});

test('sperrt GitHub, wird nichts geschrieben — und laut abgebrochen', async () => {
  const env = { DB: realD1() };
  globalThis.fetch = async () => new Response('rate limit', { status: 403 });
  await assert.rejects(() => snapshot(env, Date.parse('2026-10-01T12:00:00Z')));
  assert.equal(env.DB.db.prepare('SELECT COUNT(*) AS n FROM download_verlauf').get().n, 0);
});

test('Rechte: Eigentümer überall, das Dienst-Zeichen NUR beim Export', () => {
  const env = { ERLAUBTE_MAIL: MAIL, SERVICE_ID: 'abc.access' };
  assert.equal(allowed({ email: MAIL }, env, '/'), true);
  assert.equal(allowed({ email: MAIL }, env, '/export'), true);
  assert.equal(allowed({ common_name: 'abc.access' }, env, '/export'), true);
  assert.equal(allowed({ common_name: 'abc.access' }, env, '/'), false);
  assert.equal(allowed({ common_name: 'abc.access' }, env, '/daten'), false);
  assert.equal(allowed({ common_name: 'fremd.access' }, env, '/export'), false);
  assert.equal(allowed({ common_name: '' }, { ERLAUBTE_MAIL: MAIL, SERVICE_ID: '' }, '/export'), false,
    'ohne hinterlegte Kennung gibt es den Weg nicht');
});

// ---------------------------------------------------------------- Seitenzähler

import { PAGES, ROUTES, ACTIONS, MAX_CLICKS, MAX_COUNT, MAX_MISS_PAIRS } from './pages.js';
import { MAX_BYTES } from './worker.js';

const NAV = {
  pages: { liste: 3, verkauf: 1, laeden: 1 },
  entry: { liste: { start: 1, seitenleiste: 2 }, verkauf: { seitenleiste: 1 }, laeden: { seitenleiste: 1 } },
  clicks: { liste: { 0: 1 }, laeden: { 4: 1 } },
  misses: { 'verkauf>laeden': 1 },
  actions: { lager_scan: 2, lager_hand: 1 },
};
const NAV_TABLE_NAMES = ['seiten', 'seiten_wege', 'seiten_klicks', 'seiten_fehlgriffe', 'seiten_handlungen'];

function navPing(extra = {}) { return ping({ ...FULL, ...NAV, ...extra }); }

function rows(db, table) {
  return db.prepare('SELECT * FROM ' + table + ' ORDER BY 1, 2, 3').all().map((r) => ({ ...r }));
}

test('Vorbedingung: Seitenzähler landen in den vier Tabellen (echtes SQLite)', async () => {
  const e = { ...env(), DB: realD1() };
  const r = await worker.fetch(navPing(), e);
  assert.equal(r.status, 204);
  const day = new Date().toISOString().slice(0, 10);
  assert.deepEqual(rows(e.DB.db, 'seiten').map((x) => [x.tag, x.seite, x.n]),
    [[day, 'laeden', 1], [day, 'liste', 3], [day, 'verkauf', 1]]);
  assert.deepEqual(rows(e.DB.db, 'seiten_wege').map((x) => [x.seite, x.weg, x.n]),
    [['laeden', 'seitenleiste', 1], ['liste', 'seitenleiste', 2], ['liste', 'start', 1], ['verkauf', 'seitenleiste', 1]]);
  assert.deepEqual(rows(e.DB.db, 'seiten_klicks').map((x) => [x.seite, x.klicks, x.n]),
    [['laeden', 4, 1], ['liste', 0, 1]]);
  assert.deepEqual(rows(e.DB.db, 'seiten_fehlgriffe').map((x) => [x.von, x.nach, x.n]), [['verkauf', 'laeden', 1]]);
  assert.deepEqual(rows(e.DB.db, 'seiten_handlungen').map((x) => [x.handlung, x.n]),
    [['lager_hand', 1], ['lager_scan', 2]]);
  // Eine zweite Meldung am selben Tag zählt dazu, statt zu überschreiben.
  await worker.fetch(navPing(), e);
  assert.equal(e.DB.db.prepare("SELECT n FROM seiten WHERE seite = 'liste'").get().n, 6);
  assert.equal(e.DB.db.prepare("SELECT n FROM seiten_handlungen WHERE handlung = 'lager_scan'").get().n, 4);
  assert.equal(e.DB.db.prepare("SELECT n FROM seiten_fehlgriffe WHERE von = 'verkauf'").get().n, 2);
  // Die Installation zählt weiterhin genau einmal je Meldung.
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM tage').get().n, 2);
});

test('Seitenzähler sind nicht mit Version, Land oder System verknüpft', async () => {
  const e = { ...env(), DB: realD1() };
  await worker.fetch(navPing(), e);
  for (const t of NAV_TABLE_NAMES) {
    const cols = e.DB.db.prepare(`PRAGMA table_info(${t})`).all().map((c) => c.name);
    for (const c of cols) assert.ok(!['version', 'land', 'system'].includes(c), `${t}.${c}`);
  }
});

test('falsche Seitenzähler werden abgelehnt — die ganze Meldung', async () => {
  const pairs = Object.fromEntries(Array.from({ length: MAX_MISS_PAIRS + 1 }, (_, i) =>
    [Object.keys(PAGES)[i] + '>' + Object.keys(PAGES)[i + 1], 1]));
  const bad = [
    { pages: { unbekannt: 1 } }, { pages: { liste: 0 } }, { pages: { liste: 1.5 } }, { pages: { liste: '3' } },
    { pages: { liste: MAX_COUNT + 1 } }, { pages: [['liste', 1]] }, { pages: JSON.parse('{"__proto__": 1}') },
    { pages: { 'Suche: Gladius': 1 } },
    { entry: { liste: { browser: 1 } } }, { entry: { liste: 1 } }, { entry: { fremd: { start: 1 } } },
    { clicks: { liste: { [MAX_CLICKS + 1]: 1 } } }, { clicks: { liste: { '-1': 1 } } }, { clicks: { liste: { '01': 1 } } },
    { clicks: { liste: { a: 1 } } },
    { misses: { 'liste>liste': 1 } }, { misses: { 'liste>fremd': 1 } }, { misses: { 'a>b>c': 1 } },
    { misses: { liste: 1 } }, { misses: pairs },
    { actions: { unbekannt: 1 } }, { actions: { lager_scan: 0 } }, { actions: { lager_scan: '2' } },
    { actions: { lager_scan: MAX_COUNT + 1 } }, { actions: [['lager_scan', 1]] },
    { actions: { 'Titanium 295': 1 } }, { actions: JSON.parse('{"__proto__": 1}') },
  ];
  for (const b of bad) {
    const e = env();
    const r = await worker.fetch(navPing(b), e);
    assert.equal(r.status, 400, JSON.stringify(b));
    assert.equal(e.DB.calls.length, 0, JSON.stringify(b));
  }
});

test('die größte mögliche Meldung passt unter MAX_BYTES und wird angenommen', async () => {
  const ids = Object.keys(PAGES);
  const longest = ids.slice().sort((a, b) => b.length - a.length);
  const big = {
    ...FULL,
    mods: ['schiffe', 'werkstatt', 'bergung', 'handel', 'statistiken'],
    pages: Object.fromEntries(ids.map((p) => [p, MAX_COUNT])),
    entry: Object.fromEntries(ids.map((p) => [p, Object.fromEntries(Object.keys(ROUTES).map((w) => [w, MAX_COUNT]))])),
    clicks: Object.fromEntries(ids.map((p) => [p, Object.fromEntries(
      Array.from({ length: MAX_CLICKS + 1 }, (_, k) => [String(k), MAX_COUNT]))])),
    misses: Object.fromEntries(Array.from({ length: MAX_MISS_PAIRS }, (_, i) =>
      [longest[i % 4] + '>' + longest[4 + (i % 16)], MAX_COUNT])),
    actions: Object.fromEntries(Object.keys(ACTIONS).map((k) => [k, MAX_COUNT])),
  };
  const size = new TextEncoder().encode(JSON.stringify(big)).length;
  assert.ok(size <= MAX_BYTES, `${size} Byte > ${MAX_BYTES}`);
  const e = { ...env(), DB: realD1() };
  assert.equal((await worker.fetch(ping(big), e)).status, 204, `${size} Byte`);
  assert.equal(e.DB.db.prepare('SELECT COUNT(*) AS n FROM seiten_klicks').get().n, ids.length * (MAX_CLICKS + 1));
});

test('fehlen die Seiten-Tabellen noch, zählt die Installation trotzdem', async () => {
  const e = { ...env(), DB: realD1() };
  for (const t of NAV_TABLE_NAMES) e.DB.db.exec('DROP TABLE ' + t);
  const r = await worker.fetch(navPing(), e);
  assert.equal(r.status, 204);
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM tage').get().n, 1);
  globalThis.fetch = async () => new Response('[]');
  const d = await (await worker.fetch(statsReq('/daten?tage=30', await token()), e, undefined, verify)).json();
  assert.deepEqual([d.seiten, d.seiten_klicks], [[], []], 'die Übersicht bleibt erreichbar');
});

test('schema.sql erneut einspielen lässt vorhandene Daten unberührt', async () => {
  const sql = readFileSync(new URL('./schema.sql', import.meta.url), 'utf-8');
  const code = sql.replace(/--[^\n]*/g, '');
  const statements = code.split(';').map((s) => s.trim()).filter(Boolean);
  for (const s of statements) assert.match(s, /^CREATE TABLE IF NOT EXISTS \w+ \(/, s.slice(0, 60));
  const e = { ...env(), DB: realD1() };
  await worker.fetch(navPing(), e);
  const before = ['tage', 'merkmale', ...NAV_TABLE_NAMES]
    .map((t) => JSON.stringify(rows(e.DB.db, t)));
  e.DB.db.exec(sql);
  const after = ['tage', 'merkmale', ...NAV_TABLE_NAMES]
    .map((t) => JSON.stringify(rows(e.DB.db, t)));
  assert.deepEqual(after, before);
  assert.ok(before.every((x) => x !== '[]'), 'Vorbedingung: in jeder Tabelle stand etwas');
});

test('die Übersicht bekommt die Seitenzähler', async () => {
  const e = { ...env(), DB: realD1() };
  await worker.fetch(navPing(), e);
  globalThis.fetch = async () => new Response('[]');
  const d = await (await worker.fetch(statsReq('/daten?tage=30', await token()), e, undefined, verify)).json();
  assert.equal(d.seiten.length, 3);
  assert.equal(d.seiten_wege.length, 4);
  assert.equal(d.seiten_klicks.length, 2);
  assert.deepEqual(d.seiten_fehlgriffe.map((r) => [r.von, r.nach, r.n]), [['verkauf', 'laeden', 1]]);
  assert.equal(d.seiten_handlungen.length, 2);
});

test('Export über den echten Weg: Dienst-Zeichen bekommt alle Tabellen', async () => {
  const env = { ...env0(), DB: realD1(), SERVICE_ID: 'abc.access' };
  const jwt = await token({ email: undefined, common_name: 'abc.access' });
  const r = await worker.fetch(statsReq('/export', jwt), env, undefined, verify);
  assert.equal(r.status, 200);
  const j = await r.json();
  for (const t of ['tage', 'merkmale', 'downloads', 'download_bestand', 'download_verlauf',
                   ...NAV_TABLE_NAMES]) assert.ok(Array.isArray(j[t]), t);
  const r2 = await worker.fetch(statsReq('/daten', jwt), env, undefined, verify);
  assert.equal(r2.status, 403, 'das Dienst-Zeichen sieht die Übersicht nicht');
});

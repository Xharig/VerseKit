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

test('Ansicht und ausgeblendete Kacheln werden als Merkmale gezählt', async () => {
  const e = env();
  const r = await worker.fetch(ping({ ...FULL, view: 'karten', tiles_off: ['spielzeit', 'rechner'] }), e);
  assert.equal(r.status, 204);
  const merk = e.DB.calls.filter((c) => /INTO merkmale/.test(c.sql)).map((c) => c.args.slice(1).join('='));
  for (const m of ['ansicht=karten', 'kachel_aus=spielzeit', 'kachel_aus=rechner']) assert.ok(merk.includes(m), m);
});

test('falsche Ansicht oder unbekannte Kachel wird abgelehnt', async () => {
  for (const b of [{ view: 'liste' }, { view: true }, { tiles_off: 'spielzeit' },
                   { tiles_off: ['C:/x'] }, { tiles_off: ['spielzeit', 'spielzeit'] },
                   { tiles_off: ['gibtsnicht'] }]) {
    const e = env();
    const r = await worker.fetch(ping({ ...FULL, ...b }), e);
    assert.equal(r.status, 400, JSON.stringify(b));
    assert.equal(e.DB.calls.length, 0);
  }
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

test('die Übersicht hält das Design ein: eigene Schrift, kein Inline-Stil, keine Großbuchstaben-Überschriften, keine Gedankenstriche', async () => {
  const { dashboardHtml } = await import('./dashboard.js');
  const html = dashboardHtml('abc');
  assert.ok(html.includes('Xharig Sans') && html.includes('url(data:font/woff2;base64,'), 'Schrift fehlt');
  assert.ok(!/url\(schrift\//.test(html), 'Schrift-Adresse neben der Seite statt Daten-Adresse');
  assert.ok(!/\sstyle=/i.test(html.replace(/<script[\s\S]*<\/script>/, '')), 'style-Attribut in der Seite');
  assert.ok(!/text-transform\s*:\s*uppercase/.test(html), 'Großbuchstaben-Überschriften');
  assert.ok(!/<(script|style)(?![^>]*nonce="abc")[^>]*>/.test(html.replace(/<script type="application\/ld\+json">/g, '')), 'Skript oder Stil ohne Einmal-Wert');
  const text = html.split('<script nonce="abc">')[1];
  assert.ok(!/[—–](?!['"])/.test(text.replace(/const NONE = '–';/, '')), 'Gedankenstrich im Text der Seite');
  assert.ok(html.includes('bester abgeschlossener Tag'), 'Beschriftung des besten Tags');
});

test('die Übersicht hat den Kompakt-Schalter mit gemerkter Wahl und bedienbaren Falt-Knöpfen', async () => {
  const { dashboardHtml } = await import('./dashboard.js');
  const html = dashboardHtml('abc');
  assert.ok(/<input type="checkbox" id="kompakt" role="switch">/.test(html), 'Schalter fehlt');
  assert.ok(html.includes("localStorage.getItem('ansicht')") && html.includes("localStorage.setItem('ansicht'"), 'Wahl wird nicht gemerkt');
  assert.ok(html.includes("class: 'faltet'") && html.includes("'aria-expanded'"), 'Falt-Knopf ohne aria-expanded');
  const eigenerStil = html.split('/* ---------- Diese Seite')[1].split('</style>')[0];
  assert.ok(!/transition|animation/.test(eigenerStil), 'Animation im Stil dieser Seite');
  for (const t of ['Downloads je Version', 'Länder: Nutzer und Downloads']) {
    assert.ok(html.includes(`'${t}`) , t + ' fehlt');
  }
  assert.ok(html.includes("'Downloads je Version (neueste zuerst)', 'halb'") && html.includes("'Länder: Nutzer und Downloads', 'halb'"),
    'Versionen und Länder nicht nebeneinander');
});

test('die Übersicht erlaubt Schrift nur als Daten-Adresse', async () => {
  globalThis.fetch = async () => new Response('[]');
  const r = await worker.fetch(statsReq('/', await token()), env(), undefined, verify);
  const csp = r.headers.get('content-security-policy');
  assert.match(csp, /font-src data:;/);
  assert.match(csp, /default-src 'none'/);
  assert.ok(!/https?:/.test(csp), 'Fremdquelle in der Richtlinie');
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

test('andere Pfade auf xharig.com gehen an die Webseite weiter', async () => {
  const before = globalThis.fetch;
  const seen = [];
  globalThis.fetch = async (req) => { seen.push(new URL(req.url).pathname); return new Response('seite', { status: 200 }); };
  try {
    const r = await worker.fetch(short('/windowsxyz'), env(), ctxCollect());
    assert.equal(r.status, 200);
    assert.deepEqual(seen, ['/windowsxyz']);
  } finally {
    globalThis.fetch = before;
  }
});

test('die Routen fangen die Kurzlinks auch mit ?ref=', () => {
  const toml = readFileSync(new URL('./wrangler.toml', import.meta.url), 'utf-8');
  const patterns = [...toml.matchAll(/pattern = "([^"]+)"/g)].map((m) => m[1]);
  const covers = (url) => patterns.some((p) => p.endsWith('*') ? url.startsWith(p.slice(0, -1)) : url === p);
  for (const url of ['xharig.com/windows?ref=discord', 'www.xharig.com/linux?ref=reddit',
                     'xharig.com/windows/', 'xharig.com/linux']) {
    assert.ok(covers(url), url + ' fängt keine Route: ' + patterns.join(', '));
  }
});

test('alle Schreibweisen der Kurzlinks führen zur Datei', async () => {
  for (const url of ['https://xharig.com/windows', 'https://xharig.com/windows/',
                     'https://www.xharig.com/windows', 'https://www.xharig.com/linux/',
                     'https://xharig.com/windows?ref=discord', 'https://xharig.com/linux/?ref=discord']) {
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

test('mit GITHUB_TOKEN geht das Zeichen mit, ohne nicht', async () => {
  const seen = [];
  globalThis.fetch = async (url, init) => { seen.push(init.headers.authorization); return new Response(JSON.stringify([rel('v3.65.0', 4, 1)])); };
  await downloads({ DB: ablageDb(), GITHUB_TOKEN: 'ghp_test' }, 1_000_000);
  await downloads({ DB: ablageDb() }, 1_000_000);
  assert.deepEqual(seen, ['Bearer ghp_test', undefined]);
});

test('der Zeitplan läuft stündlich', () => {
  const toml = readFileSync(new URL('./wrangler.toml', import.meta.url), 'utf-8');
  const m = toml.match(/^crons\s*=\s*\["([^"]+)"\]/m);
  assert.ok(m, 'kein crons-Eintrag in wrangler.toml');
  assert.equal(m[1].split(/\s+/)[1], '*', 'Stundenfeld muss * sein, ist: ' + m[1]);
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

// ------------------------------------------------------------------ Suche (VerseKit 5)

import { ROUTES_V5, SEARCH } from './pages.js';

test('Vorbedingung: die Suche wird als Weg und als zwei Zähler angenommen und gezählt (echtes SQLite)', async () => {
  const e = { ...env(), DB: realD1() };
  const r = await worker.fetch(ping({ ...FULL, pages: { liste: 2 },
    entry: { liste: { suche: 2 } }, actions: { suche_benutzt: 5, suche_ohne_treffer: 2 } }), e);
  assert.equal(r.status, 204);
  assert.deepEqual(rows(e.DB.db, 'seiten_wege').map((x) => [x.seite, x.weg, x.n]), [['liste', 'suche', 2]]);
  assert.deepEqual(rows(e.DB.db, 'seiten_handlungen').map((x) => [x.handlung, x.n]),
    [['suche_benutzt', 5], ['suche_ohne_treffer', 2]]);
  // Eine zweite Meldung zählt dazu; die Installation zählt genau einmal je Meldung.
  await worker.fetch(ping({ ...FULL, actions: { suche_benutzt: 1 } }), e);
  assert.equal(e.DB.db.prepare("SELECT n FROM seiten_handlungen WHERE handlung = 'suche_benutzt'").get().n, 6);
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM tage').get().n, 2);
});

test('ältere Fassungen ohne die Suche-Felder werden weiter angenommen', async () => {
  for (const body of [FULL, { ...FULL, ...NAV }, { ...FULL, actions: { lager_scan: 1 }, entry: { liste: { start: 1 } } }]) {
    const e = { ...env(), DB: realD1() };
    assert.equal((await worker.fetch(ping(body), e)).status, 204);
    assert.equal(e.DB.db.prepare("SELECT COUNT(*) AS n FROM seiten_handlungen WHERE handlung LIKE 'suche%'").get().n, 0);
  }
});

test('unbekannte Wege und Zähler und jeder Freitext bleiben abgelehnt, auch neben der Suche', async () => {
  const bad = [
    { entry: { liste: { suchfeld: 1 } } }, { entry: { liste: { Suche: 1 } } }, { entry: { liste: { 'suche ': 1 } } },
    { entry: { liste: { 'suche: Gladius': 1 } } }, { entry: { liste: { suche: '2' } } }, { entry: { liste: { suche: 0 } } },
    { entry: { liste: { suche: MAX_COUNT + 1 } } },
    { actions: { suche: 1 } }, { actions: { suche_benutzt: 0 } }, { actions: { suche_benutzt: '3' } },
    { actions: { suche_benutzt: MAX_COUNT + 1 } }, { actions: { suche_ohne_treffer: 1.5 } },
    { actions: { suche_ergebnis: 1 } }, { actions: { 'suche_benutzt: Gladius': 1 } },
    { actions: { 'Gladius': 1 } }, { actions: { Gladius: 'Gladius' } },
    { pages: { suche: 1 } },                              // Suche ist ein Weg, keine Seite
    { search: 'Gladius' }, { search: { benutzt: 1 } }, { suche: 3 }, { query: 'Titanium' }, { suchwort: 'x' },
    { actions: { suche_benutzt: 'Gladius' } },
  ];
  for (const b of bad) {
    const e = env();
    const r = await worker.fetch(navPing(b), e);
    assert.equal(r.status, 400, JSON.stringify(b));
    assert.equal(e.DB.calls.length, 0, JSON.stringify(b));
  }
  // Gegenprobe: dieselbe Meldung ohne den Fehler geht durch.
  const ok = await worker.fetch(navPing({ actions: { suche_benutzt: 1 } }), env());
  assert.equal(ok.status, 204);
});

test('Suchwörter können nirgends gespeichert werden: Schlüssel nur aus festen Listen, Werte nur ganze Zahlen', async () => {
  const e = { ...env(), DB: realD1() };
  await worker.fetch(ping({ ...FULL, actions: { suche_benutzt: 3, suche_ohne_treffer: 1 }, entry: { liste: { suche: 3 } } }), e);
  const gespeichert = [];
  for (const t of ['seiten', 'seiten_wege', 'seiten_klicks', 'seiten_fehlgriffe', 'seiten_handlungen', 'merkmale', 'tage']) {
    gespeichert.push(...rows(e.DB.db, t).flatMap((x) => Object.values(x).filter((v) => typeof v === 'string')));
  }
  const erlaubt = new Set([...Object.keys(ACTIONS), ...Object.keys(SEARCH), ...Object.keys(ROUTES), ...Object.keys(ROUTES_V5),
    ...Object.keys(PAGES), '3.65.0', 'windows', 'XX', 'DE', 'de', 'en', 'ja', 'nein', 'popup', 'handel', 'schiffe',
    'ui', 'game', 'rc', 'autostart', 'update', 'overlay', 'mod', new Date().toISOString().slice(0, 10)]);
  for (const v of gespeichert) assert.ok(erlaubt.has(v), 'unerwarteter Text in der Datenbank: ' + v);
});

test('ROUTES und ACTIONS bleiben, wie sie die Python-Fassung im Selbsttest erwartet; die Suche steht daneben', () => {
  assert.deepEqual(Object.keys(ROUTES), ['seitenleiste', 'sprung', 'overlay', 'tray', 'start', 'karte', 'pfad']);
  assert.ok(Object.keys(ACTIONS).every((k) => !k.startsWith('suche')));
  assert.deepEqual(Object.keys(ROUTES_V5), ['suche']);
  assert.deepEqual(Object.keys(SEARCH), ['suche_benutzt', 'suche_ohne_treffer']);
});

test('die größte Meldung mit der Suche passt weiter unter MAX_BYTES', async () => {
  const ids = Object.keys(PAGES);
  const wege = { ...ROUTES, ...ROUTES_V5 };
  const big = {
    ...FULL,
    pages: Object.fromEntries(ids.map((p) => [p, MAX_COUNT])),
    entry: Object.fromEntries(ids.map((p) => [p, Object.fromEntries(Object.keys(wege).map((w) => [w, MAX_COUNT]))])),
    clicks: Object.fromEntries(ids.map((p) => [p, Object.fromEntries(
      Array.from({ length: MAX_CLICKS + 1 }, (_, k) => [String(k), MAX_COUNT]))])),
    actions: Object.fromEntries([...Object.keys(ACTIONS), ...Object.keys(SEARCH)].map((k) => [k, MAX_COUNT])),
  };
  const size = new TextEncoder().encode(JSON.stringify(big)).length;
  assert.ok(size <= MAX_BYTES, `${size} Byte > ${MAX_BYTES}`);
  assert.equal((await worker.fetch(ping(big), { ...env(), DB: realD1() })).status, 204);
});

test('die größte mögliche Meldung passt unter MAX_BYTES und wird angenommen', async () => {
  const ids = Object.keys(PAGES);
  const longest = ids.slice().sort((a, b) => b.length - a.length);
  const big = {
    ...FULL,
    mods: ['basetool', 'schiffe', 'werkstatt', 'bergbau', 'bergung', 'handel', 'beschaffung', 'statistiken'],
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

// ---------------------------------------------------------------- Erfolg: Alter, Kanal, Vorwoche

import { channel, MAX_AGE } from './worker.js';
import { trend, activeWeek, newWeek, stayWeek, downloadsWeek, topChannel, weeklyActive,
         releaseMarks, weekDays, dayBefore, ERFOLG } from './erfolg.js';

test('Vorbedingung: das Alter der Installation wird je Tag und Alter gezählt (echtes SQLite)', async () => {
  const e = { ...env(), DB: realD1() };
  for (const alter of [0, 0, 7, MAX_AGE]) {
    assert.equal((await worker.fetch(ping({ ...FULL, alter }), e)).status, 204, String(alter));
  }
  const day = new Date().toISOString().slice(0, 10);
  assert.deepEqual(rows(e.DB.db, 'installationen_alter'),
    [{ tag: day, tage: 0, n: 2 }, { tag: day, tage: 7, n: 1 }, { tag: day, tage: 31, n: 1 }]);
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM tage').get().n, 4);
});

test('das Alter steht ohne Kennung, Version, Land oder System', async () => {
  const e = { ...env(), DB: realD1() };
  await worker.fetch(ping({ ...FULL, alter: 3 }), e);
  const cols = e.DB.db.prepare('PRAGMA table_info(installationen_alter)').all().map((c) => c.name);
  assert.deepEqual(cols, ['tag', 'tage', 'n']);
  const merk = e.DB.db.prepare('SELECT merkmal FROM merkmale').all().map((r) => r.merkmal);
  assert.ok(!merk.includes('alter'), 'Alter auch unter den Merkmalen');
});

test('Meldungen ohne Alter bleiben gültig und schreiben kein Alter', async () => {
  const e = { ...env(), DB: realD1() };
  assert.equal((await worker.fetch(ping(FULL), e)).status, 204);
  assert.equal(e.DB.db.prepare('SELECT COUNT(*) AS n FROM installationen_alter').get().n, 0);
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM tage').get().n, 1);
});

test('falsches Alter wird abgelehnt, die ganze Meldung', async () => {
  for (const alter of [-1, MAX_AGE + 1, 3.5, '7', true, null, [1], 1e9]) {
    const e = { ...env(), DB: realD1() };
    const r = await worker.fetch(ping({ ...FULL, alter }), e);
    assert.equal(r.status, 400, JSON.stringify(alter));
    assert.equal(e.DB.db.prepare('SELECT COUNT(*) AS n FROM tage').get().n, 0, JSON.stringify(alter));
  }
});

test('fehlt die Alters-Tabelle noch, zählt die Installation trotzdem', async () => {
  const e = { ...env(), DB: realD1() };
  e.DB.db.exec('DROP TABLE installationen_alter');
  assert.equal((await worker.fetch(ping({ ...FULL, alter: 0 }), e)).status, 204);
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM tage').get().n, 1);
});

test('Vorbedingung: ein Kurzlink mit ?ref= zählt den Kanal mit (echtes SQLite)', async () => {
  const e = { ...env(), DB: realD1() }; const c = ctxCollect();
  const r = await worker.fetch(short('/windows?ref=discord'), e, c); await Promise.all(c.p);
  assert.equal(r.status, 302);
  assert.equal(r.headers.get('location'), 'https://github.com/Xharig/VerseKit/releases/latest/download/VerseKit-Setup.exe');
  const day = new Date().toISOString().slice(0, 10);
  assert.deepEqual(rows(e.DB.db, 'downloads_kanal'), [{ tag: day, kanal: 'discord', n: 1 }]);
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM downloads').get().n, 1);
  const cols = e.DB.db.prepare('PRAGMA table_info(downloads_kanal)').all().map((x) => x.name);
  assert.deepEqual(cols, ['tag', 'kanal', 'n'], 'Kanal ohne System und Land');
});

test('falsches Kanal-Kennzeichen wird verworfen, der Download zählt trotzdem', async () => {
  for (const ref of ['Discord', 'dis-cord', 'a'.repeat(21), '', 'x1', 'ü', '%3Cscript%3E', 'discord&ref=x']) {
    const e = { ...env(), DB: realD1() }; const c = ctxCollect();
    const r = await worker.fetch(short('/linux?ref=' + ref), e, c); await Promise.all(c.p);
    assert.equal(r.status, 302, ref);
    assert.match(r.headers.get('location'), /VerseKit-x86_64\.AppImage$/, ref);
    assert.equal(e.DB.db.prepare('SELECT COUNT(*) AS n FROM downloads_kanal').get().n,
      ref === 'discord&ref=x' ? 1 : 0, ref);
    assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM downloads').get().n, 1, ref);
  }
  assert.equal(channel(new Request('https://xharig.com/windows?ref=' + 'a'.repeat(20))), 'a'.repeat(20));
  assert.equal(channel(new Request('https://xharig.com/windows')), null);
});

test('Roboter mit ?ref= zählen auch keinen Kanal; fehlt die Kanal-Tabelle, zählt der Download', async () => {
  let e = { ...env(), DB: realD1() }; let c = ctxCollect();
  await worker.fetch(short('/windows?ref=discord', { agent: 'Discordbot/2.0' }), e, c); await Promise.all(c.p);
  assert.equal(e.DB.db.prepare('SELECT COUNT(*) AS n FROM downloads_kanal').get().n, 0);
  e = { ...env(), DB: realD1() }; c = ctxCollect();
  e.DB.db.exec('DROP TABLE downloads_kanal');
  const r = await worker.fetch(short('/windows?ref=discord'), e, c); await Promise.all(c.p);
  assert.equal(r.status, 302);
  assert.equal(e.DB.db.prepare('SELECT SUM(n) AS n FROM downloads').get().n, 1);
});

test('die Übersicht bekommt Tagessummen über die ganze Zeit, Alter und Kanäle', async () => {
  const e = { ...env(), DB: realD1() };
  e.DB.db.exec("INSERT INTO tage VALUES ('2020-01-01', '1.0.0', 'windows', 'DE', 3), ('2020-01-01', '1.0.0', 'linux', 'DE', 2)");
  await worker.fetch(ping({ ...FULL, alter: 0 }), e);
  const c = ctxCollect(); await worker.fetch(short('/windows?ref=forum'), e, c); await Promise.all(c.p);
  globalThis.fetch = async () => new Response('[]');
  const d = await (await worker.fetch(statsReq('/daten?tage=30', await token()), e, undefined, verify)).json();
  assert.deepEqual(d.tage_summe[0], { tag: '2020-01-01', n: 5 }, 'alte Tage trotz Zeitraum 30');
  assert.equal(d.tage.some((r) => r.tag === '2020-01-01'), false, 'die Tageszeilen bleiben im Zeitraum');
  assert.deepEqual(d.alter.map((r) => [r.tage, r.n]), [[0, 1]]);
  assert.deepEqual(d.kanaele.map((r) => [r.kanal, r.n]), [['forum', 1]]);
  const env2 = { ...env0(), DB: e.DB, SERVICE_ID: 'abc.access' };
  const j = await (await worker.fetch(statsReq('/export', await token({ email: undefined, common_name: 'abc.access' })), env2, undefined, verify)).json();
  assert.equal(j.installationen_alter.length, 1);
  assert.equal(j.downloads_kanal.length, 1);
});

// Rechnungen gegen feste Tage. Heute = Donnerstag 05.11.2026; diese Woche =
// 29.10. bis 04.11., Vorwoche = 22.10. bis 28.10.
const HEUTE = '2026-11-05';

test('Wochen: die letzten 7 vollen Tage, ohne heute, und die 7 davor', () => {
  assert.deepEqual(weekDays(HEUTE, 0), ['2026-10-29', '2026-10-30', '2026-10-31', '2026-11-01', '2026-11-02', '2026-11-03', '2026-11-04']);
  assert.equal(weekDays(HEUTE, 1)[0], '2026-10-22');
  assert.equal(weekDays(HEUTE, 1)[6], '2026-10-28');
  assert.equal(dayBefore('2026-03-01', 1), '2026-02-28');
});

test('Vorwoche: aktive Installationen, Schnitt gegen Schnitt, der laufende Tag zählt nicht', () => {
  const sums = [];
  for (const t of weekDays(HEUTE, 1)) sums.push({ tag: t, n: 40 });
  for (const t of weekDays(HEUTE, 0)) sums.push({ tag: t, n: 50 });
  sums.push({ tag: HEUTE, n: 1000 });
  const a = activeWeek(sums, HEUTE);
  assert.deepEqual(a, { jetzt: 50, vorher: 40 });
  assert.deepEqual(trend(a.jetzt, a.vorher, 'prozent'), { art: 'auf', pfeil: '▲', text: '+25 % zur Vorwoche' });
  assert.deepEqual(trend(40, 50, 'prozent'), { art: 'ab', pfeil: '▼', text: '−20 % zur Vorwoche' });
  assert.equal(trend(50, 50, 'prozent').art, 'gleich');
  assert.equal(trend(50, 50, 'prozent').pfeil, '=');
  assert.equal(trend(3, 0, 'prozent').art, 'neu');
  assert.deepEqual(trend(0.4, 0.3, 'punkte'), { art: 'auf', pfeil: '▲', text: '+10 Punkte zur Vorwoche' });
  // Ein fehlender Tag zählt als 0 (niemand aktiv), nicht als Lücke.
  const gap = sums.filter((r) => r.tag !== '2026-11-01');
  assert.equal(activeWeek(gap, HEUTE).jetzt, 300 / 7);
});

test('Vorwoche: begann die Zählung erst in dieser Woche, gibt es keinen Vergleich', () => {
  const a = activeWeek([{ tag: '2026-11-01', n: 10 }], HEUTE);
  assert.equal(a.vorher, null);
  assert.equal(trend(a.jetzt, a.vorher, 'prozent').art, 'keiner');
  assert.deepEqual(activeWeek([], HEUTE), { jetzt: null, vorher: null });
});

test('Neue und Bleiben: ohne Alters-Meldungen null, mit Meldungen aus Alter 0 und 7', () => {
  assert.deepEqual(newWeek([], HEUTE), { jetzt: null, vorher: null });
  assert.deepEqual(stayWeek([], HEUTE), { jetzt: null, vorher: null, neu: 0 });
  const ages = [];
  // Vorvorwoche: 10 Neue am 15.10., davon am 22.10. noch 5 da.
  ages.push({ tag: '2026-10-15', tage: 0, n: 10 }, { tag: '2026-10-22', tage: 7, n: 5 });
  // Vorwoche: 4 Neue am 22.10. und 4 am 28.10.; am 29.10. und 04.11. noch je 3 da.
  ages.push({ tag: '2026-10-22', tage: 0, n: 4 }, { tag: '2026-10-28', tage: 0, n: 4 },
            { tag: '2026-10-29', tage: 7, n: 3 }, { tag: '2026-11-04', tage: 7, n: 3 });
  // Diese Woche: 6 Neue, heute zählt nicht mit; andere Alter zählen nicht als neu.
  ages.push({ tag: '2026-10-30', tage: 0, n: 6 }, { tag: HEUTE, tage: 0, n: 99 }, { tag: '2026-10-30', tage: 1, n: 50 });
  assert.deepEqual(newWeek(ages, HEUTE), { jetzt: 6, vorher: 8 });
  const st = stayWeek(ages, HEUTE);
  assert.equal(st.jetzt, 6 / 8);
  assert.equal(st.vorher, 0.5);
  assert.equal(st.neu, 8);
  assert.equal(trend(st.jetzt, st.vorher, 'punkte').text, '+25 Punkte zur Vorwoche');
});

test('Neue: begann die Alters-Meldung mitten in der Woche, gibt es für sie keine Zahl', () => {
  assert.deepEqual(newWeek([{ tag: '2026-11-02', tage: 0, n: 5 }], HEUTE), { jetzt: null, vorher: null });
  assert.deepEqual(newWeek([{ tag: '2026-10-29', tage: 0, n: 5 }], HEUTE), { jetzt: 5, vorher: null });
});

test('Downloads der Woche: Zuwachs des nie sinkenden Stands, Lücken schieben ihn weiter', () => {
  const v = [
    { tag: '2026-10-21', je_gesehen: 100 },   // Ende der Vorvorwoche
    { tag: '2026-10-28', je_gesehen: 130 },   // Ende der Vorwoche
    { tag: '2026-11-02', je_gesehen: 170 },
    { tag: '2026-11-04', je_gesehen: 190 },   // Ende dieser Woche
    { tag: HEUTE, je_gesehen: 500 },          // heute zählt nicht
  ];
  assert.deepEqual(downloadsWeek(v, HEUTE), { jetzt: 60, vorher: 30 });
  // Messung fehlt am letzten Tag: der letzte Stand davor gilt.
  assert.deepEqual(downloadsWeek(v.filter((r) => r.tag !== '2026-11-04'), HEUTE), { jetzt: 40, vorher: 30 });
  // Mitschreiben begann erst am Ende der Vorwoche: kein Wert für die Vorwoche.
  assert.deepEqual(downloadsWeek(v.slice(1), HEUTE), { jetzt: 60, vorher: null });
  assert.deepEqual(downloadsWeek([], HEUTE), { jetzt: null, vorher: null });
});

test('stärkster Kanal der Woche, sonst null', () => {
  assert.equal(topChannel([], HEUTE), null);
  const k = [{ tag: '2026-11-01', kanal: 'discord', n: 3 }, { tag: '2026-11-02', kanal: 'forum', n: 2 },
             { tag: '2026-11-03', kanal: 'forum', n: 2 }, { tag: '2026-10-20', kanal: 'reddit', n: 50 },
             { tag: HEUTE, kanal: 'reddit', n: 50 }];
  assert.deepEqual(topChannel(k, HEUTE), { kanal: 'forum', n: 4, gesamt: 7 });
});

test('Wochenkurve: Montag bis Sonntag, Schnitt der vollen Tage ab der ersten Meldung', () => {
  const sums = [{ tag: '2026-10-22', n: 10 }, { tag: '2026-10-25', n: 20 },  // Do und So
                { tag: '2026-10-27', n: 7 }, { tag: HEUTE, n: 999 }];
  const w = weeklyActive(sums, HEUTE);
  assert.deepEqual(w.map((x) => x.ab), ['2026-10-19', '2026-10-26', '2026-11-02']);
  assert.deepEqual(w.map((x) => x.tage), [4, 7, 3]);
  assert.equal(w[0].schnitt, 30 / 4);
  assert.equal(w[1].schnitt, 1);
  assert.equal(w[2].schnitt, 0);
  assert.deepEqual(weeklyActive([], HEUTE), []);
});

test('Markierungen: jede fertige Version einmal, gelöschte dabei, Vorabversionen nicht', () => {
  const m = releaseMarks(
    [{ tag: 'v5.1.0', am: '2026-10-20', vorab: false }, { tag: 'v5.1.1-rc1', am: '2026-10-25', vorab: true }],
    [{ version: 'v5.1.0', veroeffentlicht: '2026-10-20', vorab: 0 }, { version: 'v5.0.0', veroeffentlicht: '2026-10-01', vorab: 0 },
     { version: 'v4.9.0-rc2', veroeffentlicht: '2026-09-01', vorab: 1 }, { version: 'v3.0.0', veroeffentlicht: null, vorab: 0 }]);
  assert.deepEqual(m, [{ tag: 'v5.0.0', am: '2026-10-01' }, { tag: 'v5.1.0', am: '2026-10-20' }]);
});

test('die Rechnungen laufen auch, wenn der Bündler beim Hochladen __name-Aufrufe einsetzt', async () => {
  const { dashboardHtml } = await import('./dashboard.js');
  const code = dashboardHtml('abc').split('<script nonce="abc">')[1].split('</script>')[0];
  // So sieht der Text nach dem Bündeln von wrangler aus (keepNames): ein Aufruf in jeder Funktion.
  const bundled = code.replace(/function dayBefore\(([^)]*)\) \{/, (m) => m + ' __name(dayBefore, "dayBefore");');
  assert.ok(bundled.includes('__name(dayBefore'), 'Nachbildung des Bündlers greift nicht');
  const shim = bundled.indexOf('const __name'), first = bundled.indexOf('__name(');
  assert.ok(shim >= 0 && shim < first, 'der Helfer __name fehlt vor seinem ersten Aufruf');
});

test('das Skript der Seite lässt sich übersetzen und hält die Zeichen-Regel ein', async () => {
  for (const f of ERFOLG) {
    const src = f.toString();
    assert.ok(!src.includes('`') && !src.includes('$' + '{') && !src.includes(String.fromCharCode(92)), f.name);
  }
  const { dashboardHtml } = await import('./dashboard.js');
  const html = dashboardHtml('abc');
  const code = html.split('<script nonce="abc">')[1].split('</script>')[0];
  assert.doesNotThrow(() => new Function(code), 'Syntaxfehler im Skript der Seite');
  for (const t of ["'Wie läuft VerseKit?'", "'Aktive Installationen je Tag', 'paar'", "'Aktive Nutzer je Woche', 'paar'",
                   "'Für die Entwicklung'", "localStorage.getItem('entwicklung')", "'ab VerseKit 5.2'",
                   "'Kanal: noch keine Kennzeichen'", 'function activeWeek(']) {
    assert.ok(code.includes(t), t + ' fehlt');
  }
  // Reihenfolge: Erfolg, Tageskurve, Wochenkurve, dann erst die Klappe.
  const at = (t) => code.indexOf(t);
  assert.ok(at("card('Wie läuft VerseKit?')") < at("card('Aktive Installationen je Tag'")
    && at("card('Aktive Installationen je Tag'") < at("card('Aktive Nutzer je Woche'")
    && at("card('Aktive Nutzer je Woche'") < at('target = dev;')
    && at('target = dev;') < at("card('Downloads je Tag')"), 'Reihenfolge der Seite');
});

import { siteWeek, firstData } from './erfolg.js';

test('Reichweite zählt nur Downloads über die Webseite, 7 volle Tage gegen die 7 davor', () => {
  const kurz = [{ tag: '2026-11-04', n: 5 }, { tag: '2026-10-29', n: 2 }, { tag: '2026-10-28', n: 3 },
    { tag: '2026-10-22', n: 4 }, { tag: '2026-10-21', n: 8 }, { tag: '2026-11-05', n: 9 }];
  // Heute (05.11.) zählt nicht mit; diese Woche 29.10.–04.11., Vorwoche 22.10.–28.10., der 21.10. gehört zu keiner.
  assert.deepEqual(siteWeek(kurz, '2026-11-05'), { jetzt: 7, vorher: 7 });
});

test('die Tageskurve beginnt am ersten Tag mit Meldungen', () => {
  assert.equal(firstData([0, 0, 0, 4, 0, 6]), 3);
  assert.equal(firstData([0, 0]), 0);
  assert.equal(firstData([2, 0]), 0);
});

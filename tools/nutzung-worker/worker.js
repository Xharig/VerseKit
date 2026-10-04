// SPDX-License-Identifier: GPL-3.0-only
//
// Zähler für täglich aktive Verse-Kit-Installationen — und die private
// Übersicht dazu. Läuft als Cloudflare Worker, NICHT im Programm und NICHT
// auf einem Heimserver.
//
// Zwei Adressen, streng getrennt:
//
//   nutzung-versekit.xharig.com/ping   öffentlich. Das Programm meldet sich höchstens
//                             einmal am Tag. Angenommen wird nur, was
//                             `check()` durchlässt — feste Felder, feste
//                             Formen, keine Kennung.
//   xharig.com/windows, /linux   die Download-Kurzlinks der Webseite. Zählen
//                             Tag, System und Land und leiten IMMER auf die
//                             neueste Datei bei GitHub weiter — auch wenn das
//                             Zählen scheitert. Vorschau-Roboter zählen nicht.
//   statistik-versekit.xharig.com      privat. Davor sitzt Cloudflare Access (Anmeldung
//                             über GitHub); der Worker prüft dessen Zeichen
//                             ZUSÄTZLICH selbst (`verifyAccess`). Fehlt die
//                             Einrichtung, bleibt die Tür zu.
//
// Gespeichert werden nur Zähler je Tag. Die Absender-Adresse wird nirgends
// abgelegt (die Mengenbremse sieht sie nur im Vorbeigehen, wie jeder
// Webserver). Das Land schickt das Programm NICHT mit — es ist das Kürzel,
// das Cloudflare selbst ermittelt (`request.cf.country`); keine Stadt, keine
// Region. Eine Installation lässt sich damit über Tage nicht verfolgen:
// gezählt wird die Menge, nicht die Person.

import { dashboardHtml } from './dashboard.js';
import { PAGES, ROUTES, ACTIONS, MAX_CLICKS, MAX_COUNT, MAX_MISS_PAIRS } from './pages.js';

// Reicht für die größte Meldung, die das Programm bauen kann: jede Seite mit
// jedem Weg und jeder Klickstufe, dazu alle Fehlgriff-Paare (Prüfung im Test).
export const MAX_BYTES = 16384;
const VERSION_RE = /^\d{1,3}\.\d{1,3}\.\d{1,3}(?:-rc\d{1,4})?$/;
const SYSTEMS = ['windows', 'linux'];
const LANG_RE = /^[a-z]{2}$/;
const MODULE_RE = /^[a-z]{1,20}$/;
const MAX_MODULES = 12;
const OVERLAY = ['immer', 'popup'];
const FIELDS = ['actions', 'autostart', 'clicks', 'entry', 'game', 'misses', 'mods', 'os',
                'overlay', 'pages', 'rc', 'ui', 'update', 'v'];
const COUNTRY_RE = /^[A-Z][A-Z0-9]$/;   // ISO-Kürzel; Cloudflare nutzt auch T1 (Tor), XX (unbekannt)
const STATS_HOST = 'statistik-versekit.xharig.com';
const MAX_DAYS = 400;
const REPO = 'Xharig/VerseKit';

function answer(status, text) {
  return new Response(text, {
    status,
    headers: { 'content-type': 'text/plain; charset=utf-8', 'cache-control': 'no-store' },
  });
}

export function country(request) {
  const c = String((request.cf && request.cf.country) || '').toUpperCase();
  return COUNTRY_RE.test(c) ? c : 'XX';
}

export async function check(request) {
  // Gibt die geprüften Angaben zurück oder wirft { status, text }.
  const length = Number(request.headers.get('content-length') || '0');
  if (length > MAX_BYTES) throw { status: 413, text: 'zu gross' };
  let text;
  try {
    text = await request.text();
  } catch (e) {
    throw { status: 400, text: 'unlesbar' };
  }
  if (text.length > MAX_BYTES) throw { status: 413, text: 'zu gross' };
  let d;
  try {
    d = JSON.parse(text);
  } catch (e) {
    throw { status: 400, text: 'kein JSON' };
  }
  if (!d || typeof d !== 'object' || Array.isArray(d)) throw { status: 400, text: 'Form' };
  // ⚠ Nur bekannte Felder — was mehr schickt (etwa eine Kennung oder ein
  // Land), wird abgelehnt, statt still gespeichert zu werden.
  for (const k of Object.keys(d)) {
    if (!FIELDS.includes(k)) throw { status: 400, text: 'Felder' };
  }
  if (!VERSION_RE.test(String(d.v))) throw { status: 400, text: 'Version' };
  if (!SYSTEMS.includes(d.os)) throw { status: 400, text: 'System' };
  const out = { version: d.v, system: d.os, traits: [] };
  const lang = (key) => {
    if (d[key] === undefined) return;
    if (!LANG_RE.test(String(d[key]))) throw { status: 400, text: key };
    out.traits.push([key, d[key]]);
  };
  const yesNo = (key) => {
    if (d[key] === undefined) return;
    if (typeof d[key] !== 'boolean') throw { status: 400, text: key };
    out.traits.push([key, d[key] ? 'ja' : 'nein']);
  };
  lang('ui');
  lang('game');
  yesNo('rc');
  yesNo('autostart');
  yesNo('update');
  if (d.overlay !== undefined) {
    if (!OVERLAY.includes(d.overlay)) throw { status: 400, text: 'overlay' };
    out.traits.push(['overlay', d.overlay]);
  }
  if (d.mods !== undefined) {
    if (!Array.isArray(d.mods) || d.mods.length > MAX_MODULES) throw { status: 400, text: 'mods' };
    const seen = new Set();
    for (const m of d.mods) {
      if (typeof m !== 'string' || !MODULE_RE.test(m) || seen.has(m)) throw { status: 400, text: 'mods' };
      seen.add(m);
      out.traits.push(['mod', m]);
    }
  }
  out.nav = navigation(d);
  return out;
}

const own = (o, k) => Object.prototype.hasOwnProperty.call(o, k);
const isMap = (o) => !!o && typeof o === 'object' && !Array.isArray(o);
const isCount = (n) => Number.isInteger(n) && n >= 1 && n <= MAX_COUNT;
const isStep = (k) => /^\d{1,2}$/.test(k) && String(Number(k)) === k && Number(k) <= MAX_CLICKS;

// Die Seitenzähler (`pages`, `entry`, `clicks`, `misses`) und die
// Handlungszähler (`actions`). Jede Seite muss in PAGES stehen, jeder Weg in
// ROUTES, jede Handlung in ACTIONS, jede Zahl zwischen 1 und MAX_COUNT —
// sonst wird die ganze Meldung abgelehnt. Ältere Fassungen schicken die
// Felder nicht; dann bleiben die Listen leer.
export function navigation(d) {
  const nav = { pages: [], entry: [], clicks: [], misses: [], actions: [] };
  const bad = (key) => { throw { status: 400, text: key }; };
  if (d.pages !== undefined) {
    if (!isMap(d.pages)) bad('pages');
    for (const [page, n] of Object.entries(d.pages)) {
      if (!own(PAGES, page) || !isCount(n)) bad('pages');
      nav.pages.push([page, n]);
    }
  }
  for (const [field, keyOk] of [['entry', (k) => own(ROUTES, k)], ['clicks', isStep]]) {
    if (d[field] === undefined) continue;
    if (!isMap(d[field])) bad(field);
    for (const [page, inner] of Object.entries(d[field])) {
      if (!own(PAGES, page) || !isMap(inner)) bad(field);
      for (const [key, n] of Object.entries(inner)) {
        if (!keyOk(key) || !isCount(n)) bad(field);
        nav[field].push(field === 'clicks' ? [page, Number(key), n] : [page, key, n]);
      }
    }
  }
  if (d.misses !== undefined) {
    if (!isMap(d.misses) || Object.keys(d.misses).length > MAX_MISS_PAIRS) bad('misses');
    for (const [pair, n] of Object.entries(d.misses)) {
      const parts = pair.split('>');
      if (parts.length !== 2 || !own(PAGES, parts[0]) || !own(PAGES, parts[1])
          || parts[0] === parts[1] || !isCount(n)) bad('misses');
      nav.misses.push([parts[0], parts[1], n]);
    }
  }
  if (d.actions !== undefined) {
    if (!isMap(d.actions)) bad('actions');
    for (const [key, n] of Object.entries(d.actions)) {
      if (!own(ACTIONS, key) || !isCount(n)) bad('actions');
      nav.actions.push([key, n]);
    }
  }
  return nav;
}

// Je Tabelle EINE Anweisung, die Zeilen als JSON-Liste (`json_each`) — so
// bleibt es bei fünf Abfragen, egal wie viele Seiten in der Meldung stehen.
const NAV_SQL = {
  pages: 'INSERT INTO seiten (tag, seite, n) SELECT ?1, json_extract(value, \'$[0]\'), ' +
    'json_extract(value, \'$[1]\') FROM json_each(?2) WHERE true ' +
    'ON CONFLICT (tag, seite) DO UPDATE SET n = n + excluded.n',
  entry: 'INSERT INTO seiten_wege (tag, seite, weg, n) SELECT ?1, json_extract(value, \'$[0]\'), ' +
    'json_extract(value, \'$[1]\'), json_extract(value, \'$[2]\') FROM json_each(?2) WHERE true ' +
    'ON CONFLICT (tag, seite, weg) DO UPDATE SET n = n + excluded.n',
  clicks: 'INSERT INTO seiten_klicks (tag, seite, klicks, n) SELECT ?1, json_extract(value, \'$[0]\'), ' +
    'json_extract(value, \'$[1]\'), json_extract(value, \'$[2]\') FROM json_each(?2) WHERE true ' +
    'ON CONFLICT (tag, seite, klicks) DO UPDATE SET n = n + excluded.n',
  misses: 'INSERT INTO seiten_fehlgriffe (tag, von, nach, n) SELECT ?1, json_extract(value, \'$[0]\'), ' +
    'json_extract(value, \'$[1]\'), json_extract(value, \'$[2]\') FROM json_each(?2) WHERE true ' +
    'ON CONFLICT (tag, von, nach) DO UPDATE SET n = n + excluded.n',
  actions: 'INSERT INTO seiten_handlungen (tag, handlung, n) SELECT ?1, json_extract(value, \'$[0]\'), ' +
    'json_extract(value, \'$[1]\') FROM json_each(?2) WHERE true ' +
    'ON CONFLICT (tag, handlung) DO UPDATE SET n = n + excluded.n',
};

export function navStatements(db, day, nav) {
  return Object.keys(NAV_SQL).filter((k) => nav[k].length)
    .map((k) => db.prepare(NAV_SQL[k]).bind(day, JSON.stringify(nav[k])));
}

async function ping(request, env) {
  if (request.method !== 'POST') return answer(405, 'nur POST');
  if (!env.DB || !env.LIMIT) return answer(503, 'nicht eingerichtet');
  const ip = request.headers.get('cf-connecting-ip') || 'unbekannt';
  const { success } = await env.LIMIT.limit({ key: ip });
  if (!success) return answer(429, 'zu viele');
  let c;
  try {
    c = await check(request);
  } catch (e) {
    if (e && e.status) return answer(e.status, e.text);
    return answer(400, 'unlesbar');
  }
  // Der Tag kommt vom Worker (UTC), nicht vom Absender — eine falsch
  // gestellte Uhr soll nicht in der Vergangenheit zählen.
  const day = new Date().toISOString().slice(0, 10);
  const statements = [
    env.DB.prepare(
      'INSERT INTO tage (tag, version, system, land, n) VALUES (?1, ?2, ?3, ?4, 1) ' +
      'ON CONFLICT (tag, version, system, land) DO UPDATE SET n = n + 1'
    ).bind(day, c.version, c.system, country(request)),
  ];
  for (const [trait, value] of c.traits) {
    statements.push(env.DB.prepare(
      'INSERT INTO merkmale (tag, merkmal, wert, n) VALUES (?1, ?2, ?3, 1) ' +
      'ON CONFLICT (tag, merkmal, wert) DO UPDATE SET n = n + 1'
    ).bind(day, trait, value));
  }
  await env.DB.batch(statements);
  // ⚠ Getrennt und abgefangen: Fehlen die Seiten-Tabellen noch (schema.sql
  // nicht eingespielt), zählt die Installation trotzdem.
  const nav = navStatements(env.DB, day, c.nav);
  if (nav.length) {
    try {
      await env.DB.batch(nav);
    } catch (e) {
      await store(env, 'fehler_seiten', { ausnahme: String(e && e.message).slice(0, 200) }).catch(() => {});
    }
  }
  return new Response(null, { status: 204 });
}

// ------------------------------------------------------------ Access prüfen

function b64urlBytes(s) {
  const b = atob(s.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((s.length + 3) % 4));
  return Uint8Array.from(b, (ch) => ch.charCodeAt(0));
}

function b64urlJson(s) {
  return JSON.parse(new TextDecoder().decode(b64urlBytes(s)));
}

// Gibt die geprüften Angaben des Zeichens zurück oder null. Jede Prüfung
// muss bestehen; fehlt eine Einstellung, ist die Antwort null (Tür zu).
export async function verifyAccess(request, env, fetchCerts = fetch) {
  const team = env.TEAM_DOMAIN || '';
  const aud = env.POLICY_AUD || '';
  if (!/^[a-z0-9-]+\.cloudflareaccess\.com$/.test(team) || !aud) return null;
  const token = request.headers.get('cf-access-jwt-assertion') || '';
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  let head, claims;
  try {
    head = b64urlJson(parts[0]);
    claims = b64urlJson(parts[1]);
  } catch (e) {
    return null;
  }
  if (head.alg !== 'RS256' || !head.kid) return null;
  let keys;
  try {
    const r = await fetchCerts(`https://${team}/cdn-cgi/access/certs`);
    keys = (await r.json()).keys || [];
  } catch (e) {
    return null;
  }
  const jwk = keys.find((k) => k.kid === head.kid);
  if (!jwk) return null;
  let ok = false;
  try {
    const key = await crypto.subtle.importKey(
      'jwk', jwk, { name: 'RSASSA-PKCS1-v1_5', hash: 'SHA-256' }, false, ['verify']);
    ok = await crypto.subtle.verify(
      'RSASSA-PKCS1-v1_5', key, b64urlBytes(parts[2]),
      new TextEncoder().encode(`${parts[0]}.${parts[1]}`));
  } catch (e) {
    return null;
  }
  if (!ok) return null;
  const now = Math.floor(Date.now() / 1000);
  const auds = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (!auds.includes(aud)) return null;
  if (claims.iss !== `https://${team}`) return null;
  if (typeof claims.exp !== 'number' || claims.exp < now) return null;
  if (typeof claims.nbf === 'number' && claims.nbf > now + 60) return null;
  return claims;
}

// Übersicht und Daten: nur die Mail aus ERLAUBTE_MAIL. `/export` zusätzlich
// das Access-Dienst-Zeichen mit der Client-ID aus SERVICE_ID.
export function allowed(claims, env, path) {
  const mail = (env.ERLAUBTE_MAIL || '').trim().toLowerCase();
  if (mail && String(claims.email || '').toLowerCase() === mail) return true;
  const service = (env.SERVICE_ID || '').trim();
  return path === '/export' && !!service && claims.common_name === service;
}

// ------------------------------------------------------------ Übersicht

// GitHub-Downloads je Version. ⚠⚠ Ohne Anmeldung erlaubt GitHub 60 Abrufe je
// Stunde und Absender-Adresse — und Cloudflare-Worker teilen sich ihre
// Adressen mit vielen anderen. Ohne Vorsorge steht dann plötzlich 0 Downloads
// da, die Seite ist leer. Deshalb:
//   1. Höchstens einmal je Minute wird GitHub überhaupt gefragt.
//   2. Mit ETag (Abfrage auf Änderung) — eine 304-Antwort zählt bei
//      GitHub NICHT gegen die Grenze.
//   3. Scheitert der Abruf, gelten die zuletzt gespeicherten Zahlen weiter —
//      mit ihrer Uhrzeit, damit die Seite nicht lügt.
// Abgelegt in der Tabelle `ablage` (schluessel, inhalt, zeit).
const DL_MAX_AGE = 60 * 1000;

async function stored(env, key) {
  try {
    const { results } = await env.DB.prepare('SELECT inhalt, zeit FROM ablage WHERE schluessel = ?1').bind(key).all();
    const row = results && results[0];
    return row && row.inhalt ? { data: JSON.parse(row.inhalt), zeit: row.zeit } : null;
  } catch (e) {
    return null;
  }
}

async function store(env, key, data, zeit = Date.now()) {
  await env.DB.prepare(
    'INSERT INTO ablage (schluessel, inhalt, zeit) VALUES (?1, ?2, ?3) ' +
    'ON CONFLICT (schluessel) DO UPDATE SET inhalt = ?2, zeit = ?3'
  ).bind(key, JSON.stringify(data), zeit).run();
}

function compact(rel) {
  return {
    tag: rel.tag_name,
    am: String(rel.published_at || '').slice(0, 10),
    vorab: !!rel.prerelease,
    windows: (rel.assets || []).filter((a) => a.name.endsWith('.exe')).reduce((s, a) => s + a.download_count, 0),
    linux: (rel.assets || []).filter((a) => a.name.endsWith('.AppImage')).reduce((s, a) => s + a.download_count, 0),
  };
}

export async function downloads(env, now = Date.now()) {
  const last = await stored(env, 'downloads');
  if (last && now - last.zeit < DL_MAX_AGE) return { liste: last.data, stand: last.zeit, alt: false };
  try {
    const all = [];
    for (let page = 1; page <= 10; page++) {
      const before = await stored(env, 'seite:' + page);
      const headers = { 'user-agent': 'versekit-statistik', accept: 'application/vnd.github+json' };
      if (before && before.data.etag) headers['if-none-match'] = before.data.etag;
      const r = await fetch(`https://api.github.com/repos/${REPO}/releases?per_page=100&page=${page}`, { headers });
      let rows, count;
      if (r.status === 304 && before) {
        rows = before.data.rows; count = before.data.count;
      } else if (r.ok) {
        const batch = await r.json();
        rows = batch.map(compact); count = batch.length;
        await store(env, 'seite:' + page, { etag: r.headers.get('etag') || '', rows, count });
      } else {
        // Den Grund festhalten — sonst bleibt eine fehlende GitHub-Antwort ein Rätsel.
        await store(env, 'fehler', { status: r.status,
          rest: r.headers.get('x-ratelimit-remaining'), grenze: r.headers.get('x-ratelimit-limit'),
          text: (await r.text()).slice(0, 200) }).catch(() => {});
        throw new Error('GitHub ' + r.status);
      }
      all.push(...rows);
      if (count < 100) break;
    }
    // ⚠ Mit `now`, nicht der Uhr: Die Minutensperre oben vergleicht gegen
    // genau diesen Wert.
    await store(env, 'downloads', all, now);
    return { liste: all, stand: now, alt: false };
  } catch (e) {
    if (!String(e && e.message).startsWith('GitHub ')) {
      await store(env, 'fehler', { ausnahme: String(e && e.message).slice(0, 200) }).catch(() => {});
    }
    return last ? { liste: last.data, stand: last.zeit, alt: true } : { liste: [], stand: null, alt: true };
  }
}

async function data(env, ctx, days) {
  const since = new Date(Date.now() - days * 86400000).toISOString().slice(0, 10);
  const [t, m, dl, links] = await Promise.all([
    env.DB.prepare('SELECT tag, version, system, land, n FROM tage WHERE tag >= ?1').bind(since).all(),
    env.DB.prepare('SELECT tag, merkmal, wert, n FROM merkmale WHERE tag >= ?1').bind(since).all(),
    downloads(env),
    env.DB.prepare('SELECT tag, system, land, n FROM downloads WHERE tag >= ?1').bind(since).all(),
  ]);
  const [bestand, verlauf] = await Promise.all([
    env.DB.prepare('SELECT version, windows, linux, gesamt, veroeffentlicht, vorab, zuletzt FROM download_bestand').all(),
    env.DB.prepare('SELECT * FROM download_verlauf ORDER BY tag').all(),
  ]);
  // Fehlen die Seiten-Tabellen noch, bleiben die Blöcke leer statt der Seite.
  const nav = await Promise.all(NAV_TABLES.map((table) =>
    env.DB.prepare(`SELECT * FROM ${table} WHERE tag >= ?1`).bind(since).all()
      .then((r) => r.results || []).catch(() => [])));
  return { tage: t.results || [], merkmale: m.results || [], downloads: dl.liste,
           downloads_stand: dl.stand, downloads_alt: dl.alt, kurzlinks: links.results || [],
           bestand: bestand.results || [], verlauf: verlauf.results || [],
           seiten: nav[0], seiten_wege: nav[1], seiten_klicks: nav[2], seiten_fehlgriffe: nav[3],
           seiten_handlungen: nav[4] };
}

const NAV_TABLES = ['seiten', 'seiten_wege', 'seiten_klicks', 'seiten_fehlgriffe', 'seiten_handlungen'];

// ------------------------------------------------------------ Kurzlinks

const SHORT_HOSTS = ['xharig.com', 'www.xharig.com'];
const SHORT_LINKS = {
  '/windows': { system: 'windows',
    target: `https://github.com/${REPO}/releases/latest/download/VerseKit-Setup.exe` },
  '/linux': { system: 'linux',
    target: `https://github.com/${REPO}/releases/latest/download/VerseKit-x86_64.AppImage` },
};
// Wer einen Link nur ansieht, lädt nichts herunter: Vorschau-Roboter (Discord
// ruft jeden geposteten Link selbst ab), Suchmaschinen, Kommandozeilen-Werkzeuge.
const NOT_A_PERSON = /bot|crawl|spider|slurp|preview|facebookexternalhit|embed|discord|telegram|whatsapp|skype|slack|curl|wget|python|go-http|java|okhttp|headless/i;

export function countsAsDownload(request) {
  if (request.method !== 'GET') return false;
  const agent = request.headers.get('user-agent') || '';
  return agent.length > 0 && !NOT_A_PERSON.test(agent);
}

async function countDownload(request, env, link) {
  if (!env.DB || !env.LIMIT || !countsAsDownload(request)) return;
  // Dieselbe Adresse mehrfach in kurzer Zeit (Doppelklick, Abbruch und neu)
  // zählt einmal — die Bremse ist dieselbe wie bei der Meldung.
  const ip = request.headers.get('cf-connecting-ip') || 'unbekannt';
  const { success } = await env.LIMIT.limit({ key: 'dl:' + ip });
  if (!success) return;
  const day = new Date().toISOString().slice(0, 10);
  await env.DB.prepare(
    'INSERT INTO downloads (tag, system, land, n) VALUES (?1, ?2, ?3, 1) ' +
    'ON CONFLICT (tag, system, land) DO UPDATE SET n = n + 1'
  ).bind(day, link.system, country(request)).run();
}

function shortLink(request, env, ctx, link) {
  // ⚠ Die Weiterleitung kommt IMMER und sofort — gezählt wird nebenher. Ein
  // klemmender Zähler darf keinen Download verhindern.
  const counting = countDownload(request, env, link).catch(() => {});
  if (ctx && ctx.waitUntil) ctx.waitUntil(counting);
  return new Response(null, {
    status: 302,
    headers: { location: link.target, 'cache-control': 'no-store', 'referrer-policy': 'no-referrer' },
  });
}

// ------------------------------------------------------------ Mitschreiben

// Alle 6 Stunden (Zeitplan in wrangler.toml): je Version den höchsten je
// gesehenen Stand festhalten und eine Tageszeile schreiben. Gelöschte Releases
// bleiben so erhalten. ⚠ Kommen keine FRISCHEN Zahlen (GitHub sperrt), wird
// nichts geschrieben und laut abgebrochen — ein alter Stand als Tageszeile
// sähe aus, als sei heute nichts heruntergeladen worden.
export async function snapshot(env, now = Date.now()) {
  const dl = await downloads(env, now);
  if (dl.alt || !dl.liste.length) throw new Error('keine frischen GitHub-Zahlen');
  const day = new Date(now).toISOString().slice(0, 10);
  await env.DB.batch(dl.liste.map((r) => env.DB.prepare(
    'INSERT INTO download_bestand (version, windows, linux, gesamt, veroeffentlicht, vorab, erstmals, zuletzt) ' +
    'VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7) ON CONFLICT (version) DO UPDATE SET ' +
    'windows = MAX(windows, ?2), linux = MAX(linux, ?3), gesamt = MAX(gesamt, ?4), ' +
    'veroeffentlicht = COALESCE(veroeffentlicht, ?5), vorab = ?6, zuletzt = ?7'
  ).bind(r.tag, r.windows, r.linux, r.windows + r.linux, r.am, r.vorab ? 1 : 0, day)));
  const { results } = await env.DB.prepare(
    'SELECT COUNT(*) AS n, SUM(gesamt) AS je FROM download_bestand').all();
  const row = (results && results[0]) || { n: 0, je: 0 };
  const win = dl.liste.reduce((a, r) => a + r.windows, 0);
  const lin = dl.liste.reduce((a, r) => a + r.linux, 0);
  await env.DB.prepare(
    'INSERT INTO download_verlauf (tag, aktiv, je_gesehen, windows, linux, releases_aktiv, releases_je) ' +
    'VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT (tag) DO UPDATE SET aktiv = ?2, ' +
    'je_gesehen = ?3, windows = ?4, linux = ?5, releases_aktiv = ?6, releases_je = ?7'
  ).bind(day, win + lin, row.je || 0, win, lin, dl.liste.length, row.n || 0).run();
  return { tag: day, aktiv: win + lin, je_gesehen: row.je || 0 };
}

// Alles, was die Datenbank weiß — für die Sicherung beim Feierabend.
async function exportAll(env) {
  const tables = ['tage', 'merkmale', 'downloads', 'download_bestand', 'download_verlauf'];
  const out = { erstellt: new Date().toISOString() };
  for (const t of tables) {
    out[t] = (await env.DB.prepare('SELECT * FROM ' + t).all()).results || [];
  }
  for (const t of NAV_TABLES) {
    out[t] = await env.DB.prepare('SELECT * FROM ' + t).all()
      .then((r) => r.results || []).catch(() => []);
  }
  return out;
}

const SECURITY = {
  'cache-control': 'no-store',
  'x-frame-options': 'DENY',
  'x-content-type-options': 'nosniff',
  'referrer-policy': 'no-referrer',
  'cross-origin-opener-policy': 'same-origin',
  'permissions-policy': 'camera=(), microphone=(), geolocation=()',
};

async function stats(request, env, ctx, verify) {
  if (request.method !== 'GET') return answer(405, 'nur GET');
  if (!env.DB) return answer(503, 'nicht eingerichtet');
  const who = await verify(request, env);
  const url = new URL(request.url);
  if (!who || !allowed(who, env, url.pathname)) return answer(403, 'kein Zugang');
  if (url.pathname === '/export') {
    return new Response(JSON.stringify(await exportAll(env)), {
      headers: { 'content-type': 'application/json; charset=utf-8', ...SECURITY },
    });
  }
  if (url.pathname === '/daten') {
    const days = Math.min(MAX_DAYS, Math.max(1, Number(url.searchParams.get('tage')) || 120));
    return new Response(JSON.stringify(await data(env, ctx, days)), {
      headers: { 'content-type': 'application/json; charset=utf-8', ...SECURITY },
    });
  }
  if (url.pathname !== '/') return answer(404, 'nicht hier');
  const nonce = crypto.randomUUID().replace(/-/g, '');
  return new Response(dashboardHtml(nonce), {
    headers: {
      'content-type': 'text/html; charset=utf-8',
      'content-security-policy':
        `default-src 'none'; script-src 'nonce-${nonce}'; style-src 'nonce-${nonce}'; ` +
        "connect-src 'self'; img-src 'self' data:; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
      ...SECURITY,
    },
  });
}

export default {
  async scheduled(event, env, ctx) {
    ctx.waitUntil(snapshot(env));
  },
  async fetch(request, env, ctx, verify = verifyAccess) {
    const url = new URL(request.url);
    // ⚠ Die Übersicht NUR unter ihrer eigenen Adresse — dort sitzt Access
    // davor. Über workers.dev oder nutzung-versekit.xharig.com gibt es sie nicht.
    if (url.hostname === STATS_HOST) return stats(request, env, ctx, verify);
    // ⚠ Wie die frühere Weiterleitungsregel: mit und ohne `www.`, mit und ohne
    // Schrägstrich am Ende. Sonst liefe `www.xharig.com/windows/` ins Leere.
    const shortPath = url.pathname.replace(/\/+$/, '');
    if (SHORT_HOSTS.includes(url.hostname) && SHORT_LINKS[shortPath]) {
      return shortLink(request, env, ctx, SHORT_LINKS[shortPath]);
    }
    if (url.pathname === '/ping') return ping(request, env);
    return answer(404, 'nicht hier');
  },
};

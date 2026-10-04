// SPDX-License-Identifier: GPL-3.0-only
//
// Die private Übersicht auf statistik-versekit.xharig.com. Eine Seite, keine fremden
// Skripte, keine Schriften von außen: Alles steht hier, gezeichnet wird als
// SVG im Browser. Die Content-Security-Policy des Workers lässt nur Skript
// und Stil mit dem Einmal-Wert (`nonce`) dieser Antwort zu.

import { ICON } from './icon.js';
import { PAGES, ROUTES, ACTIONS, MAX_CLICKS } from './pages.js';

// Für das Skript der Seite: als JSON, `<` maskiert, damit kein Name das
// Skript-Element beenden kann.
const asScript = (value) => JSON.stringify(value).replace(/</g, '\\u003c');

export function dashboardHtml(nonce) {
  return `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<title>VerseKit Statistik</title>
<link rel="icon" type="image/png" href="${ICON}">
<style nonce="${nonce}">
:root { --bg:#0f1417; --card:#171e22; --line:#26313a; --fg:#e6edf0; --sub:#8a9aa3;
        --accent:#9ce430; --b:#4fb3ff; --c:#ffb547; --d:#c792ea; }
* { box-sizing:border-box; }
/* ⚠ Ohne diese Zeile schlägt display:grid der Balkenzeilen das hidden —
   die eingeklappten Zeilen blieben sichtbar. */
[hidden] { display:none !important; }
body { margin:0; background:var(--bg); color:var(--fg);
       font:14px/1.45 system-ui, -apple-system, "Segoe UI", sans-serif; }
header { padding:18px 16px 6px; max-width:1200px; margin:0 auto; display:flex;
         gap:12px; align-items:baseline; flex-wrap:wrap; }
h1 { font-size:20px; margin:0; } h1 b { color:var(--accent); }
h1 .logo { width:28px; height:28px; vertical-align:-6px; margin-right:8px; }
header .sub { color:var(--sub); }
header select, header button { background:var(--card); color:var(--fg);
         border:1px solid var(--line); border-radius:8px; padding:5px 10px; font:inherit; }
header select { margin-left:auto; }
header button { cursor:pointer; }
header button:hover { border-color:var(--accent); color:var(--accent); }
header button:disabled { opacity:.5; cursor:default; }
main { max-width:1200px; margin:0 auto; padding:8px 16px 32px; display:grid;
       grid-template-columns:repeat(3, minmax(0, 1fr)); gap:14px; }
@media (max-width: 900px) { main { grid-template-columns:1fr; } }
.card { background:var(--card); border:1px solid var(--line); border-radius:12px;
        padding:14px 16px; min-width:0; }
.wide { grid-column:1 / -1; }
.card h2 { font-size:13px; font-weight:600; color:var(--sub); margin:0 0 10px;
           text-transform:uppercase; letter-spacing:.04em; }
.kpis { display:grid; grid-template-columns:repeat(auto-fit, minmax(140px, 1fr)); gap:10px; }
.kpi .v { font-size:28px; font-weight:700; color:var(--accent); }
.kpi .l { color:var(--sub); font-size:12px; }
.legend { display:flex; gap:14px; flex-wrap:wrap; color:var(--sub); font-size:12px; margin-top:6px; }
.legend i { display:inline-block; width:10px; height:10px; border-radius:2px; margin-right:5px; }
.bar { display:grid; grid-template-columns:minmax(90px, 38%) 1fr auto; gap:8px;
       align-items:center; margin:5px 0; font-size:13px; }
.bar .track { background:var(--line); border-radius:4px; height:10px; overflow:hidden; }
.bar .fill { background:var(--accent); height:100%; }
.bar .track.stack { display:flex; }
.mehr { margin-top:8px; background:none; border:1px solid var(--line); color:var(--sub);
        border-radius:8px; padding:4px 10px; font:inherit; font-size:12px; cursor:pointer; }
.mehr:hover { border-color:var(--accent); color:var(--accent); }
.bar .n { color:var(--sub); font-variant-numeric:tabular-nums; min-width:52px; text-align:right; }
.note { color:var(--sub); font-size:12px; margin-top:8px; }
svg text { fill:var(--sub); font-size:11px; }
.empty { color:var(--sub); padding:20px 0; }
.scroll { overflow-x:auto; }
.tab { width:100%; border-collapse:collapse; font-size:13px; }
.tab th { text-align:left; color:var(--sub); font-weight:600; padding:6px 8px; border-bottom:1px solid var(--line); white-space:nowrap; }
.tab td { padding:6px 8px; border-bottom:1px solid var(--line); }
.tab .num { text-align:right; font-variant-numeric:tabular-nums; }
.tab th:not(:first-child) { text-align:right; }
.cols { column-count:2; column-gap:28px; }
@media (max-width: 900px) { .cols { column-count:1; } }
.cols .bar { break-inside:avoid; }
.bar .grp { color:var(--sub); font-size:11px; margin-left:6px; }
.dim { opacity:.5; }
.hist { display:inline-grid; grid-template-columns:repeat(${MAX_CLICKS + 1}, 16px); gap:2px;
        align-items:end; height:26px; vertical-align:middle; }
.hist div { background:var(--accent); border-radius:2px 2px 0 0; min-height:1px; }
.hist.axis { height:auto; align-items:start; }
.hist.axis span { color:var(--sub); font-size:10px; text-align:center; }
</style>
</head>
<body>
<header>
  <h1><img class="logo" src="${ICON}" alt=""><b>VerseKit</b> Statistik</h1>
  <span class="sub" id="stand">lädt …</span>
  <select id="zeitraum" aria-label="Zeitraum">
    <option value="30">30 Tage</option>
    <option value="90" selected>90 Tage</option>
    <option value="365">1 Jahr</option>
  </select>
  <button id="neu" type="button" title="Lädt die Seite neu — die Zahlen kommen sonst alle 10 Minuten von selbst">Aktualisieren</button>
</header>
<main id="inhalt"></main>
<script nonce="${nonce}">
(() => {
'use strict';
const $ = (t, a = {}, kids = []) => {
  const e = document.createElement(t);
  // ⚠ \`style\` NICHT als Attribut: Die Content-Security-Policy erlaubt Stil nur
  // mit Einmal-Wert und blockiert style="…" — die Legenden-Kästchen blieben
  // farblos. Über das Stil-Objekt gesetzt, ist es erlaubt.
  for (const [k, v] of Object.entries(a)) {
    if (k === 'text') e.textContent = v;
    else if (k === 'style') e.style.cssText = v;
    else e.setAttribute(k, v);
  }
  for (const c of kids) e.appendChild(c);
  return e;
};
const NS = 'http://www.w3.org/2000/svg';
const s = (t, a = {}) => { const e = document.createElementNS(NS, t);
  for (const [k, v] of Object.entries(a)) e.setAttribute(k, v); return e; };
const today = new Date().toISOString().slice(0, 10);
const dayList = (n) => { const out = []; const d = new Date(today + 'T00:00:00Z');
  for (let i = n - 1; i >= 0; i--) out.push(new Date(d - i * 86400000).toISOString().slice(0, 10)); return out; };
const short = (t) => t.slice(8, 10) + '.' + t.slice(5, 7) + '.';
let land;
try { land = new Intl.DisplayNames(['de'], { type: 'region' }); } catch (e) { land = null; }
const landName = (c) => c === 'XX' ? 'unbekannt' : c === 'T1' ? 'Tor' : (land && land.of(c)) || c;
const NAMES = { ui: { de: 'Deutsch', en: 'Englisch' }, overlay: { immer: 'immer sichtbar', popup: 'nur bei neuen Bauplänen' },
  mod: { schiffe: 'Schiffe', werkstatt: 'Werkstatt', bergung: 'Bergung', handel: 'Handel', statistiken: 'Statistiken' } };
const langName = (c) => ({ de: 'Deutsch', en: 'Englisch', fr: 'Französisch', es: 'Spanisch', it: 'Italienisch',
  pt: 'Portugiesisch', pl: 'Polnisch', zh: 'Chinesisch', ja: 'Japanisch', ko: 'Koreanisch', ru: 'Russisch', xx: 'unbekannt' }[c] || c);
const PAGES = ${asScript(PAGES)};
const ROUTES = ${asScript(ROUTES)};
const ACTIONS = ${asScript(ACTIONS)};
const MAX_CLICKS = ${MAX_CLICKS};
const ROUTE_COLORS = { seitenleiste: 'var(--accent)', sprung: 'var(--b)', overlay: 'var(--c)',
  tray: 'var(--d)', start: 'var(--sub)' };
const pageName = (p) => (PAGES[p] || ['', p])[1];
const stepName = (k) => (Number(k) >= MAX_CLICKS ? MAX_CLICKS + '+' : String(k));

function card(title, wide) {
  const c = $('div', { class: 'card' + (wide ? ' wide' : '') }, [$('h2', { text: title })]);
  document.getElementById('inhalt').appendChild(c); return c;
}

function lineChart(parent, days, series) {
  const W = 1000, H = 260, L = 40, R = 10, T = 10, B = 26;
  // Obergrenze auf ein Vielfaches von 4 — sonst ergeben die fünf
  // Teilstriche krumme Werte, und gerundet fehlt einer (0, 1, 3, 4, 5).
  const max = Math.max(1, ...series.flatMap((x) => x.values));
  const top = Math.max(4, Math.ceil((max * 1.15) / 4) * 4);
  const X = (i) => L + (days.length < 2 ? 0 : (i * (W - L - R)) / (days.length - 1));
  const Y = (v) => T + (H - T - B) * (1 - v / top);
  const svg = s('svg', { viewBox: \`0 0 \${W} \${H}\`, width: '100%', role: 'img' });
  for (let k = 0; k <= 4; k++) {
    const v = Math.round((top * k) / 4), y = Y(v);
    svg.appendChild(s('line', { x1: L, x2: W - R, y1: y, y2: y, stroke: 'var(--line)' }));
    const t = s('text', { x: L - 6, y: y + 4, 'text-anchor': 'end' }); t.textContent = v; svg.appendChild(t);
  }
  const step = Math.max(1, Math.ceil(days.length / 10));
  days.forEach((d, i) => { if (i % step === 0 || i === days.length - 1) {
    const t = s('text', { x: X(i), y: H - 8, 'text-anchor': 'middle' }); t.textContent = short(d); svg.appendChild(t); } });
  for (const sr of series) {
    const pts = sr.values.map((v, i) => \`\${X(i).toFixed(1)},\${Y(v).toFixed(1)}\`).join(' ');
    if (sr.fill) svg.appendChild(s('polygon', { points: \`\${X(0)},\${Y(0)} \${pts} \${X(days.length - 1)},\${Y(0)}\`,
      fill: sr.color, opacity: '0.12' }));
    svg.appendChild(s('polyline', { points: pts, fill: 'none', stroke: sr.color, 'stroke-width': sr.width || 2,
      'stroke-linejoin': 'round', 'stroke-linecap': 'round' }));
  }
  // Wert beim Überfahren; ohne Maus der letzte Tag
  const tip = s('text', { x: W - R, y: T + 10, 'text-anchor': 'end' }); svg.appendChild(tip);
  const showDay = (i) => { tip.textContent = short(days[i]) + '  ' + series.map((sr) => sr.name + ' ' + sr.values[i]).join(' · '); };
  showDay(days.length - 1);
  svg.addEventListener('mousemove', (ev) => {
    const r = svg.getBoundingClientRect(); const x = ((ev.clientX - r.left) / r.width) * W;
    showDay(Math.max(0, Math.min(days.length - 1, Math.round(((x - L) / (W - L - R)) * (days.length - 1)))));
  });
  svg.addEventListener('mouseleave', () => showDay(days.length - 1));
  parent.appendChild(svg);
  parent.appendChild($('div', { class: 'legend' }, series.map((sr) =>
    $('span', {}, [$('i', { style: 'background:' + sr.color }), document.createTextNode(sr.name)]))));
}

function bars(parent, rows, fmt) {
  if (!rows.length) { parent.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' })); return; }
  const max = Math.max(...rows.map((r) => r[1]), 1e-9);
  for (const [label, v, extra] of rows) {
    const fill = $('div', { class: 'fill' }); fill.style.width = ((v / max) * 100).toFixed(1) + '%';
    parent.appendChild($('div', { class: 'bar' }, [
      $('span', { text: label }), $('div', { class: 'track' }, [fill]),
      $('span', { class: 'n', text: fmt(v, extra) })]));
  }
}

function render(d, span) {
  const main = document.getElementById('inhalt'); main.textContent = '';
  const days = dayList(span);
  const perDay = {}, perSys = { windows: {}, linux: {} };
  for (const r of d.tage) {
    perDay[r.tag] = (perDay[r.tag] || 0) + r.n;
    perSys[r.system][r.tag] = (perSys[r.system][r.tag] || 0) + r.n;
  }
  const total = days.map((t) => perDay[t] || 0);
  const yesterday = days[days.length - 2], full = days.slice(0, -1);
  const avg = (n) => { const w = full.slice(-n); return w.length ? w.reduce((a, t) => a + (perDay[t] || 0), 0) / w.length : 0; };
  const peak = Math.max(0, ...full.map((t) => perDay[t] || 0));
  // ⭐ Gesamt aus dem Mitschreiben (höchster je gesehener Stand, gelöschte
  // Releases eingeschlossen) — sinkt nie. Ohne Verlauf: was GitHub gerade hat.
  const dlNow = d.downloads.reduce((a, r) => a + r.windows + r.linux, 0);
  const lastV = (d.verlauf || [])[(d.verlauf || []).length - 1];
  const dlTotal = lastV ? Math.max(lastV.je_gesehen, dlNow) : dlNow;

  const k = card('Überblick', true);
  const kpi = (v, l) => $('div', { class: 'kpi' }, [$('div', { class: 'v', text: v }), $('div', { class: 'l', text: l })]);
  k.appendChild($('div', { class: 'kpis' }, [
    kpi(perDay[today] || 0, 'heute bisher'), kpi(perDay[yesterday] || 0, 'gestern'),
    kpi(avg(7).toFixed(1), 'Ø letzte 7 Tage'), kpi(avg(30).toFixed(1), 'Ø letzte 30 Tage'),
    kpi(peak, 'bester Tag im Zeitraum'), kpi(dlTotal, 'Downloads gesamt (mit gelöschten)'),
    kpi((d.kurzlinks || []).filter((r) => days.includes(r.tag)).reduce((a, r) => a + r.n, 0),
        'über die Webseite (Zeitraum)')]));
  k.appendChild($('div', { class: 'note', text: 'Aktiv = hat sich an dem Tag gemeldet (UTC). Jede Installation höchstens einmal am Tag, ohne Kennung. Wer die Meldung abschaltet, fehlt. Erst ab v3.65.0.' }));

  lineChart(card('Aktive Installationen je Tag', true), days, [
    { name: 'gesamt', color: 'var(--accent)', values: total, fill: true, width: 2.5 },
    { name: 'Windows', color: 'var(--b)', values: days.map((t) => perSys.windows[t] || 0) },
    { name: 'Linux', color: 'var(--c)', values: days.map((t) => perSys.linux[t] || 0) }]);

  // Downloads je Tag — aus dem Mitschreiben (Zuwachs des nie sinkenden
  // Gesamtstands). Fehlt ein Tag, steht sein Zuwachs am nächsten gemessenen.
  const perDayDl = {}, perDayW = {}, perDayL = {};
  const vl = d.verlauf || [];
  for (let i = 1; i < vl.length; i++) {
    perDayDl[vl[i].tag] = Math.max(0, vl[i].je_gesehen - vl[i - 1].je_gesehen);
    perDayW[vl[i].tag] = Math.max(0, vl[i].windows - vl[i - 1].windows);
    perDayL[vl[i].tag] = Math.max(0, vl[i].linux - vl[i - 1].linux);
  }
  const dayCard = card('Downloads je Tag', true);
  if (vl.length < 2) {
    dayCard.appendChild($('div', { class: 'empty', text: 'Noch zu wenig Messungen — der Worker schreibt alle 6 Stunden mit.' }));
  } else {
    lineChart(dayCard, days, [
      { name: 'gesamt', color: 'var(--accent)', values: days.map((t) => perDayDl[t] || 0), fill: true, width: 2.5 },
      { name: 'Windows', color: 'var(--b)', values: days.map((t) => perDayW[t] || 0) },
      { name: 'Linux', color: 'var(--c)', values: days.map((t) => perDayL[t] || 0) }]);
    dayCard.appendChild($('div', { class: 'note', text: 'Gemessen alle 6 Stunden. Fehlt ein Tag, steht sein Zuwachs am nächsten gemessenen Tag. Gelöschte Releases zählen mit.' }));
  }

  // Anteile über die letzten 7 Tage INKLUSIVE heute, aber nur Tage mit
  // Meldungen: Summe der Meldungen mit dem Wert geteilt durch alle Meldungen —
  // ein Durchschnitt, keine Personenzahl. (Nur volle Tage hieße: am ersten Tag
  // stünde überall der Leer-Hinweis, obwohl schon Meldungen da sind.)
  const win = new Set(days.slice(-7).filter((t) => perDay[t]));
  const base = [...win].reduce((a, t) => a + (perDay[t] || 0), 0);
  const share = (trait) => { const m = {};
    for (const r of d.merkmale) if (r.merkmal === trait && win.has(r.tag)) m[r.wert] = (m[r.wert] || 0) + r.n;
    return Object.entries(m).sort((a, b) => b[1] - a[1]); };
  const pct = (v) => base ? Math.round((v / base) * 100) + ' %' : '—';

  // ⭐ Feste Reihenfolge (Symmetrie): erst die vier breiten Karten, dann genau
  // ZWEI Reihen zu je DREI kleinen. Wer eine kleine Karte dazubaut oder
  // wegnimmt, hält die Zahl durch drei teilbar — sonst steht eine allein.

  // 3. Downloads je Version — Windows und Linux je in eigener Farbe, dieselben
  // wie in der Kurve oben. Direkt unter der Kurve, damit man nicht scrollt.
  // Eingeklappt die letzten 5, ausgeklappt alle fertigen Versionen. Die Wahl
  // merkt sich der Browser (nur bequem — fehlt der Speicher, gilt eingeklappt).
  // Versionen ohne einen einzigen Download (frühe Fassungen) blähen nur auf —
  // außer der neuesten fertigen, die steht immer da, auch mit 0.
  const current = new Set(d.downloads.map((r) => r.tag));
  const newest = d.downloads.filter((r) => !r.vorab)
    .reduce((a, r) => (!a || String(r.am).localeCompare(String(a.am)) > 0 ? r : a), null);
  // Gelöschte fertige Versionen aus dem Mitschreiben dazu — sonst wären ihre
  // Downloads aus der Liste verschwunden.
  const gone = (d.bestand || []).filter((b) => !b.vorab && b.gesamt > 0 && !current.has(b.version))
    .map((b) => ({ tag: b.version, am: b.veroeffentlicht || b.zuletzt, windows: b.windows, linux: b.linux,
                   gesamt: b.gesamt, weg: true }));
  const rel = d.downloads.filter((r) => !r.vorab && (r.windows + r.linux > 0 || r === newest))
    .map((r) => ({ ...r, gesamt: r.windows + r.linux })).concat(gone)
    .sort((a, b) => String(b.am).localeCompare(String(a.am)));
  const SHOWN = 5;
  let open = false;
  try { open = localStorage.getItem('dl-alle') === '1'; } catch (e) { open = false; }
  const dlCard = card('Downloads je Version (neueste zuerst)', true);
  const dlMax = Math.max(1, ...rel.map((r) => r.gesamt));
  const extra = [];
  for (const [i, r] of rel.entries()) {
    const w = $('div', { class: 'fill' }); w.style.width = ((r.windows / dlMax) * 100).toFixed(1) + '%';
    w.style.background = 'var(--b)';
    const l = $('div', { class: 'fill' }); l.style.width = ((r.linux / dlMax) * 100).toFixed(1) + '%';
    l.style.background = 'var(--c)';
    // Bei übernommenen Altdaten ist die Aufteilung unbekannt: dann ein grauer
    // Balken für den Rest, statt eine Aufteilung zu erfinden.
    const rest = Math.max(0, r.gesamt - r.windows - r.linux);
    const u = $('div', { class: 'fill' }); u.style.width = ((rest / dlMax) * 100).toFixed(1) + '%';
    u.style.background = 'var(--sub)';
    const split = r.windows + r.linux > 0 ? '  (' + r.windows + ' Windows · ' + r.linux + ' Linux)' : '';
    const line = $('div', { class: 'bar' }, [
      $('span', { text: r.tag + ' · ' + short(String(r.am)) + (r.weg ? ' · gelöscht' : '') }),
      $('div', { class: 'track stack' }, [w, l, u]),
      $('span', { class: 'n', text: r.gesamt + split })]);
    if (i >= SHOWN) { extra.push(line); line.hidden = !open; }
    dlCard.appendChild(line);
  }
  if (!rel.length) dlCard.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' }));
  if (extra.length) {
    const toggle = $('button', { type: 'button', class: 'mehr' });
    const label = () => { toggle.textContent = open ? 'Nur die letzten ' + SHOWN : 'Alle Versionen zeigen (' + rel.length + ')'; };
    label();
    toggle.addEventListener('click', () => {
      open = !open;
      for (const x of extra) x.hidden = !open;
      label();
      try { localStorage.setItem('dl-alle', open ? '1' : '0'); } catch (e) { /* nur bequem */ }
    });
    dlCard.appendChild(toggle);
  }
  if (d.downloads_stand) {
    const at = new Date(d.downloads_stand).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' });
    dlCard.appendChild($('div', { class: 'note', text: d.downloads_alt
      ? 'GitHub antwortet gerade nicht — das sind die Zahlen von ' + at + ' Uhr.'
      : 'Zahlen von GitHub, Stand ' + at + ' Uhr.' }));
  } else if (d.downloads_alt) {
    dlCard.appendChild($('div', { class: 'note', text: 'GitHub antwortet gerade nicht — noch keine gespeicherten Zahlen.' }));
  }
  dlCard.appendChild($('div', { class: 'legend' }, [
    $('span', {}, [$('i', { style: 'background:var(--b)' }), document.createTextNode('Windows')]),
    $('span', {}, [$('i', { style: 'background:var(--c)' }), document.createTextNode('Linux')])]));

  // 4. Länder: aktive Nutzer (Ø 7 Tage) und Downloads über die Kurzlinks im
  // gewählten Zeitraum nebeneinander. Downloads direkt bei GitHub und
  // Auto-Updates kennen kein Land und fehlen hier.
  const lands = {};
  const row = (c) => (lands[c] = lands[c] || { users: 0, dl: 0 });
  for (const r of d.tage) if (win.has(r.tag)) row(r.land).users += r.n;
  const inSpan = new Set(days);
  for (const r of (d.kurzlinks || [])) if (inSpan.has(r.tag)) row(r.land).dl += r.n;
  const userSum = Object.values(lands).reduce((a, x) => a + x.users, 0);
  const dlSum = Object.values(lands).reduce((a, x) => a + x.dl, 0);
  const lc = card('Länder — Nutzer und Downloads', true);
  const entries = Object.entries(lands).sort((a, b) => (b[1].users - a[1].users) || (b[1].dl - a[1].dl));
  if (!entries.length) {
    lc.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' }));
  } else {
    const head = $('tr', {}, ['Land', 'aktive Nutzer je Tag (Ø 7 Tage)', 'Anteil', 'Downloads (Zeitraum)', 'Anteil']
      .map((h) => $('th', { text: h })));
    // Wie bei den Versionen: eingeklappt die ersten 5, ausgeklappt alle.
    // Eigener Merker, damit beide Listen unabhängig auf- und zuklappen.
    const LAND_SHOWN = 5;
    let landOpen = false;
    try { landOpen = localStorage.getItem('land-alle') === '1'; } catch (e) { landOpen = false; }
    const landExtra = [];
    const body = entries.map(([c, x], i) => {
      const tr = $('tr', {}, [
        $('td', { text: landName(c) }),
        $('td', { class: 'num', text: win.size ? (x.users / win.size).toFixed(1) : '—' }),
        $('td', { class: 'num', text: userSum ? Math.round((x.users / userSum) * 100) + ' %' : '—' }),
        $('td', { class: 'num', text: String(x.dl) }),
        $('td', { class: 'num', text: dlSum ? Math.round((x.dl / dlSum) * 100) + ' %' : '—' })]);
      if (i >= LAND_SHOWN) { landExtra.push(tr); tr.hidden = !landOpen; }
      return tr;
    });
    lc.appendChild($('div', { class: 'scroll' }, [$('table', { class: 'tab' }, [$('thead', {}, [head]), $('tbody', {}, body)])]));
    if (landExtra.length) {
      const toggle = $('button', { type: 'button', class: 'mehr' });
      const label = () => { toggle.textContent = landOpen ? 'Nur die ersten ' + LAND_SHOWN : 'Alle Länder zeigen (' + entries.length + ')'; };
      label();
      toggle.addEventListener('click', () => {
        landOpen = !landOpen;
        for (const x of landExtra) x.hidden = !landOpen;
        label();
        try { localStorage.setItem('land-alle', landOpen ? '1' : '0'); } catch (e) { /* nur bequem */ }
      });
      lc.appendChild(toggle);
    }
  }
  lc.appendChild($('div', { class: 'note', text: 'Downloads hier = über xharig.com/windows und /linux (Knöpfe der Webseite). Direkt bei GitHub und per Auto-Update verrät GitHub kein Land.' }));

  // 5./6. Zwei Reihen zu je drei kleinen Karten.
  const lastDay = [...days].reverse().find((t) => perDay[t]) || today;
  const vers = {}; for (const r of d.tage) if (r.tag === lastDay) vers[r.version] = (vers[r.version] || 0) + r.n;
  bars(card('Versionen ' + (lastDay === today ? 'heute' : lastDay === yesterday ? 'gestern' : 'am ' + short(lastDay))),
    Object.entries(vers).sort((a, b) => b[1] - a[1]).slice(0, 10), (v) => v);
  const sys = { Windows: 0, Linux: 0 };
  for (const r of d.tage) if (win.has(r.tag)) sys[r.system === 'linux' ? 'Linux' : 'Windows'] += r.n;
  bars(card('System (Ø 7 Tage)'), Object.entries(sys), pct);
  bars(card('Sprache der Oberfläche (Ø 7 Tage)'), share('ui').map(([c, v]) => [langName(c), v]), pct);

  bars(card('Sprache des Spiels (Ø 7 Tage)'), share('game').map(([c, v]) => [langName(c), v]), pct);
  bars(card('Eingeschaltete Bereiche (Ø 7 Tage)'), share('mod').map(([c, v]) => [NAMES.mod[c] || c, v]), pct);
  bars(card('Einstellungen (Ø 7 Tage)'), [
    ['Testversionen an', (share('rc').find((x) => x[0] === 'ja') || [0, 0])[1]],
    ['Autostart an', (share('autostart').find((x) => x[0] === 'ja') || [0, 0])[1]],
    ['Auto-Update an', (share('update').find((x) => x[0] === 'ja') || [0, 0])[1]],
    ...share('overlay').map(([c, v]) => ['Overlay ' + (NAMES.overlay[c] || c), v])], pct);

  // 7.–10. Seiten des Hauptfensters und Handlungen über den gewählten
  // Zeitraum — vier breite Karten unter den kleinen, damit die zwei
  // Dreierreihen geschlossen bleiben.
  pageBlocks(d, inSpan);

  document.getElementById('stand').textContent = 'Stand ' + new Date().toLocaleString('de-DE')
    + ' · lädt alle 10 Minuten neu';
}

function pageBlocks(d, inSpan) {
  const sum = (rows, key) => { const m = {};
    for (const r of rows || []) if (inSpan.has(r.tag)) { const k = key(r); m[k] = (m[k] || 0) + r.n; }
    return m; };

  // Rangliste: jede Seite aus PAGES, auch mit 0 — der Balken nach Wegen
  // aufgeteilt. Bei gleicher Zahl gilt die Reihenfolge der Seitenleiste.
  const views = sum(d.seiten, (r) => r.seite);
  const ways = sum(d.seiten_wege, (r) => r.seite + '|' + r.weg);
  const order = Object.keys(PAGES);
  const ranked = order.slice().sort((a, b) => ((views[b] || 0) - (views[a] || 0)) || (order.indexOf(a) - order.indexOf(b)));
  const rc = card('Seiten nach Aufrufen (Zeitraum)', true);
  const top = Math.max(1, ...ranked.map((p) => views[p] || 0));
  const total = ranked.reduce((a, p) => a + (views[p] || 0), 0);
  const list = $('div', { class: 'cols' });
  for (const p of ranked) {
    const v = views[p] || 0;
    const parts = Object.keys(ROUTES).map((w) => {
      const seg = $('div', { class: 'fill' });
      seg.style.width = (((ways[p + '|' + w] || 0) / top) * 100).toFixed(1) + '%';
      seg.style.background = ROUTE_COLORS[w];
      seg.title = ROUTES[w] + ': ' + (ways[p + '|' + w] || 0);
      return seg;
    });
    list.appendChild($('div', { class: 'bar' + (v ? '' : ' dim') }, [
      $('span', {}, [document.createTextNode(pageName(p)), $('span', { class: 'grp', text: PAGES[p][0] })]),
      $('div', { class: 'track stack' }, parts),
      $('span', { class: 'n', text: String(v) })]));
  }
  rc.appendChild(list);
  rc.appendChild($('div', { class: 'legend' }, Object.keys(ROUTES).map((w) =>
    $('span', {}, [$('i', { style: 'background:' + ROUTE_COLORS[w] }), document.createTextNode(ROUTES[w])]))));
  rc.appendChild($('div', { class: 'note', text: total
    ? total + ' Aufrufe im Zeitraum. Wer die Meldung abschaltet, fehlt; ältere Fassungen zählen keine Seiten.'
    : 'Noch keine Seitenaufrufe im Zeitraum.' }));

  // Klicks bis zum Ziel: je Seite die Verteilung 0 … MAX_CLICKS+ und ihr Median.
  const dist = {};
  for (const r of d.seiten_klicks || []) {
    if (!inSpan.has(r.tag)) continue;
    const k = Math.min(Number(r.klicks), MAX_CLICKS);
    const m = dist[r.seite] = dist[r.seite] || {};
    m[k] = (m[k] || 0) + r.n;
  }
  const median = (m) => { const n = Object.values(m).reduce((a, x) => a + x, 0); let run = 0;
    for (let k = 0; k <= MAX_CLICKS; k++) { run += m[k] || 0; if (run * 2 >= n) return k; } return MAX_CLICKS; };
  const kc = card('Klicks bis zum Ziel', true);
  const targets = Object.keys(dist).filter((p) => PAGES[p])
    .map((p) => [p, Object.values(dist[p]).reduce((a, x) => a + x, 0)])
    .sort((a, b) => (b[1] - a[1]) || (order.indexOf(a[0]) - order.indexOf(b[0])));
  if (!targets.length) {
    kc.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' }));
  } else {
    const axis = $('div', { class: 'hist axis' });
    for (let k = 0; k <= MAX_CLICKS; k++) axis.appendChild($('span', { text: stepName(k) }));
    const head = $('tr', {}, [$('th', { text: 'Seite' }), $('th', { text: 'erreicht' }), $('th', { text: 'Median' }),
      $('th', {}, [document.createTextNode('Verteilung (Klicks) '), axis])]);
    const body = targets.map(([p, n]) => {
      const m = dist[p]; const hi = Math.max(1, ...Object.values(m));
      const hist = $('div', { class: 'hist' });
      for (let k = 0; k <= MAX_CLICKS; k++) {
        const b = $('div'); b.style.height = (((m[k] || 0) / hi) * 100).toFixed(0) + '%';
        if (!m[k]) b.style.background = 'var(--line)';
        b.title = stepName(k) + ' Klicks: ' + (m[k] || 0); hist.appendChild(b);
      }
      return $('tr', {}, [$('td', { text: pageName(p) }), $('td', { class: 'num', text: String(n) }),
        $('td', { class: 'num', text: stepName(median(m)) }), $('td', { class: 'num' }, [hist])]);
    });
    kc.appendChild($('div', { class: 'scroll' }, [$('table', { class: 'tab' }, [$('thead', {}, [head]), $('tbody', {}, body)])]));
  }
  kc.appendChild($('div', { class: 'note', text: 'Ziel = eine Seite, auf der man mindestens 3 Sekunden bleibt. Klicks = Seitenwechsel und Auf- oder Zuklappen in der Seitenleiste, ab dem Öffnen des Fensters bzw. seit dem letzten Ziel. 0 = gleich beim Öffnen da, ' + MAX_CLICKS + '+ = ' + MAX_CLICKS + ' oder mehr.' }));

  // Häufigste Fehlgriffe als „A → B".
  const miss = sum(d.seiten_fehlgriffe, (r) => r.von + '>' + r.nach);
  const pairs = Object.entries(miss).filter(([k]) => k.split('>').every((p) => PAGES[p]))
    .sort((a, b) => b[1] - a[1]).slice(0, 15)
    .map(([k, n]) => { const [a, b] = k.split('>'); return [pageName(a) + ' → ' + pageName(b), n]; });
  const mc = card('Häufigste Fehlgriffe', true);
  bars(mc, pairs, (v) => v);
  mc.appendChild($('div', { class: 'note', text: 'Fehlgriff = Seite nach weniger als 3 Sekunden wieder verlassen. Rechts die Seite, auf der man danach geblieben ist.' }));

  // Handlungen: jede aus ACTIONS, auch mit 0, in der Reihenfolge der Liste.
  const done = sum(d.seiten_handlungen, (r) => r.handlung);
  const ac = card('Handlungen (Zeitraum)', true);
  bars(ac, Object.keys(ACTIONS).map((k) => [ACTIONS[k], done[k] || 0]), (v) => v);
  ac.appendChild($('div', { class: 'note', text: 'Wie oft eine Handlung genutzt wurde — nur die Anzahl, keine Mengen oder Namen.' }));
}

async function load() {
  const span = Number(document.getElementById('zeitraum').value);
  const button = document.getElementById('neu');
  button.disabled = true; button.textContent = 'Lädt …';
  try {
    const r = await fetch('/daten?tage=' + span, { credentials: 'same-origin', redirect: 'manual' });
    // Nach 24 Stunden läuft die Anmeldung ab — dann leitet Access um, und
    // nur ein Neuladen der ganzen Seite führt durch die Anmeldung.
    if (r.type === 'opaqueredirect' || r.status === 0 || r.status === 403) {
      document.getElementById('stand').textContent = 'Anmeldung abgelaufen — Seite neu laden (F5)';
      return;
    }
    if (!r.ok) throw new Error('HTTP ' + r.status);
    render(await r.json(), span);
  } catch (e) {
    document.getElementById('stand').textContent = 'Laden fehlgeschlagen (' + e.message + ')';
  } finally {
    button.disabled = false; button.textContent = 'Aktualisieren';
  }
}
document.getElementById('zeitraum').addEventListener('change', load);
// Der Knopf lädt die GANZE Seite neu, nicht nur die Zahlen — sonst blieb nach
// einer neuen Fassung der Übersicht die alte stehen, bis jemand F5 drückt.
document.getElementById('neu').addEventListener('click', () => location.reload());
load();
setInterval(load, 10 * 60 * 1000);
})();
</script>
</body>
</html>`;
}

// SPDX-License-Identifier: GPL-3.0-only
//
// Das Skript der Übersicht (läuft im Browser). Es steht als Text hier, weil der
// Worker es in die Seite einsetzt; dashboard.js liefert die Hülle und den Stil.
// ⚠ Im Text darf weder ein Gravis noch ein Dollar mit geschweifter Klammer noch
// ein Rückwärtsschrägstrich stehen (Vorlage-Zeichenkette): Texte also mit "+"
// zusammensetzen.

import { PAGES, ROUTES, ACTIONS, MERGED, TILES, MAX_CLICKS } from './pages.js';

// Für das Skript der Seite: als JSON, `<` maskiert, damit kein Name das
// Skript-Element beenden kann.
const asScript = (value) => JSON.stringify(value).replace(/</g, '\\u003c');

export function clientScript() {
  return `(() => {
'use strict';
const $ = (t, a = {}, kids = []) => {
  const e = document.createElement(t);
  // ⚠ style NICHT als Attribut: Die Content-Security-Policy erlaubt Stil nur
  // mit Einmal-Wert und blockiert style="…". Über das Stil-Objekt gesetzt, ist es erlaubt.
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
  mod: { schiffe: 'Schiffe', werkstatt: 'Werkstatt', bergung: 'Bergung', handel: 'Handel', statistiken: 'Statistiken',
    basetool: 'Basetool' } };
const langName = (c) => ({ de: 'Deutsch', en: 'Englisch', fr: 'Französisch', es: 'Spanisch', it: 'Italienisch',
  pt: 'Portugiesisch', pl: 'Polnisch', zh: 'Chinesisch', ja: 'Japanisch', ko: 'Koreanisch', ru: 'Russisch', xx: 'unbekannt' }[c] || c);
const PAGES = ${asScript(PAGES)};
const ROUTES = ${asScript(ROUTES)};
const ACTIONS = ${asScript(ACTIONS)};
const MERGED = ${asScript(MERGED)};
const TILES = ${asScript(TILES)};
const MAX_CLICKS = ${MAX_CLICKS};
// Seite, unter der eine Kennung gezählt wird (MERGED, sonst sie selbst).
const alias = (p) => MERGED[p] || p;
// Farben: gesamt Neon; Windows blau und Linux orange (auch für Farbschwache
// trennbar, Linux zusätzlich gestrichelt); die Wege der Seitenleiste nach Okabe und Ito.
const C_ALL = 'var(--neon)', C_WIN = 'var(--win)', C_LIN = 'var(--lin)';
const ROUTE_COLORS = { seitenleiste: 'var(--neon)', sprung: 'var(--weg-blau)', overlay: 'var(--lin)',
  tray: 'var(--weg-rosa)', start: 'var(--weg-grau)', karte: 'var(--weg-rot)', pfad: 'var(--weg-eis)' };
const pageName = (p) => (PAGES[p] || ['', p])[1];
const stepName = (k) => (Number(k) >= MAX_CLICKS ? MAX_CLICKS + '+' : String(k));
const NONE = '–';

// Ansicht: ausführlich (wie bisher) oder kompakt. Die Wahl merkt sich der
// Browser (nur bequem, fehlt der Speicher, gilt ausführlich).
const readMode = () => { try { return localStorage.getItem('ansicht') === 'kompakt'; } catch (e) { return false; } };
let compact = readMode();
// In der kompakten Ansicht aufgeklappte Abschnitte (nach Titel). Bleibt beim
// Neuladen der Zahlen erhalten, beim Neuladen der Seite ist wieder alles zu.
const openSections = new Set();
let lastData = null;

// Ein Abschnitt: Überschrift plus Inhalt. kind: 'klein' (drei je Reihe),
// 'halb' (zwei nebeneinander), 'paar' (nur kompakt nebeneinander). Gibt den Inhalt zurück.
function card(title, kind) {
  const sec = $('section', { class: 'block' + (kind ? ' ' + kind : '') });
  const h2 = $('h2', { text: title });
  const body = $('div', { class: 'inhalt' });
  sec.appendChild(h2); sec.appendChild(body);
  document.getElementById('daten').appendChild(sec);
  body.sec = sec; body.h2 = h2; body.title = title;
  return body;
}

function chevron() {
  const e = s('svg', { class: 'ic chev', viewBox: '0 0 24 24', fill: 'none', stroke: 'currentColor', 'stroke-width': 2,
    'stroke-linecap': 'round', 'stroke-linejoin': 'round', 'aria-hidden': 'true' });
  e.appendChild(s('path', { d: 'm6 9 6 6 6-6' }));
  return e;
}

// Kompakt: der Abschnitt zeigt nur Überschrift und die wichtigste Zahl und
// klappt auf Klick auf (Knopf mit aria-expanded, per Tastatur bedienbar).
// Ausführlich: bleibt wie er ist.
function zusammen(body, text) {
  if (!compact) return;
  const open = openSections.has(body.title);
  const btn = $('button', { type: 'button', class: 'faltet', 'aria-expanded': String(open) }, [
    $('span', { class: 't', text: body.title }), $('span', { class: 'z', text: text || 'Noch keine Daten' }), chevron()]);
  body.h2.textContent = ''; body.h2.appendChild(btn); body.h2.classList.add('mitfalt');
  const apply = (o) => { body.hidden = !o; body.sec.classList.toggle('offen', o); btn.setAttribute('aria-expanded', String(o)); };
  apply(open);
  btn.addEventListener('click', () => {
    const o = !openSections.has(body.title);
    if (o) openSections.add(body.title); else openSections.delete(body.title);
    apply(o);
  });
}

// Farbmuster der Legende; gestrichelt wie die Linie im Diagramm.
function swatch(color, dash) {
  const i = $('i', { class: 'sw' });
  i.style.cssText = dash
    ? 'background:repeating-linear-gradient(90deg,' + color + ' 0 6px,transparent 6px 10px)'
    : 'background:' + color;
  return i;
}
function legend(items) {
  return $('div', { class: 'legende' }, items.map((it) =>
    $('span', {}, [swatch(it.color, it.dash), document.createTextNode(it.name)])));
}

// Die Diagramme werden bei jeder Breite neu gezeichnet, damit die Schrift am
// Handy lesbar bleibt (sonst schrumpft eine feste Zeichenfläche mit).
const charts = [];

function lineChart(parent, days, series, label) {
  const tip = $('p', { class: 'tipp' });
  const holder = $('div', { class: 'diagramm' });
  parent.appendChild($('div', { class: 'winkel bild' }, [tip, holder]));
  parent.appendChild(legend(series.map((sr) => ({ name: sr.name, color: sr.color, dash: sr.dash }))));
  const showDay = (i) => { tip.textContent = short(days[i]) + ': ' + series.map((sr) => sr.name + ' ' + sr.values[i]).join(' · '); };

  // Obergrenze auf ein Vielfaches von 4, sonst ergeben die fünf Teilstriche
  // krumme Werte, und gerundet fehlt einer (0, 1, 3, 4, 5).
  const max = Math.max(1, ...series.flatMap((x) => x.values));
  const top = Math.max(4, Math.ceil((max * 1.15) / 4) * 4);

  function draw() {
    holder.textContent = '';
    // Kompakt etwa ein Drittel der Höhe, mit drei statt fünf Teilstrichen.
    const W = Math.max(260, Math.round(holder.clientWidth) || 600), H = compact ? 100 : W < 520 ? 210 : 280;
    const L = 34, R = 8, T = 8, B = 26;
    const X = (i) => L + (days.length < 2 ? 0 : (i * (W - L - R)) / (days.length - 1));
    const Y = (v) => T + (H - T - B) * (1 - v / top);
    const svg = s('svg', { viewBox: '0 0 ' + W + ' ' + H, width: W, height: H, role: 'img', 'aria-label': label });
    const ticks = compact ? 2 : 4;
    for (let k = 0; k <= ticks; k++) {
      const v = Math.round((top * k) / ticks), y = Y(v);
      svg.appendChild(s('line', { x1: L, x2: W - R, y1: y, y2: y, stroke: 'var(--linie)' }));
      const t = s('text', { x: L - 6, y: y + 4, 'text-anchor': 'end' }); t.textContent = v; svg.appendChild(t);
    }
    // So viele Tagesmarken, wie nebeneinander passen.
    const fit = Math.max(2, Math.floor((W - L - R) / 62));
    const step = Math.max(1, Math.ceil(days.length / fit));
    days.forEach((d, i) => { if ((days.length - 1 - i) % step === 0) {
      const t = s('text', { x: X(i), y: H - 8, 'text-anchor': i === days.length - 1 ? 'end' : i === 0 ? 'start' : 'middle' }); t.textContent = short(d); svg.appendChild(t); } });
    const guide = s('line', { y1: T, y2: H - B, stroke: 'var(--linie-hell)', visibility: 'hidden' });
    svg.appendChild(guide);
    for (const sr of series) {
      const pts = sr.values.map((v, i) => X(i).toFixed(1) + ',' + Y(v).toFixed(1)).join(' ');
      if (sr.fill) svg.appendChild(s('polygon', { points: X(0) + ',' + Y(0) + ' ' + pts + ' ' + X(days.length - 1) + ',' + Y(0),
        fill: sr.color, opacity: '0.12' }));
      const line = { points: pts, fill: 'none', stroke: sr.color, 'stroke-width': sr.width || 2,
        'stroke-linejoin': 'round', 'stroke-linecap': 'round' };
      if (sr.dash) line['stroke-dasharray'] = '7 5';
      svg.appendChild(s('polyline', line));
    }
    // Wert beim Überfahren oder Antippen; sonst der letzte Tag
    const at = (ev) => {
      const r = svg.getBoundingClientRect(); const x = ((ev.clientX - r.left) / r.width) * W;
      const i = Math.max(0, Math.min(days.length - 1, Math.round(((x - L) / (W - L - R)) * (days.length - 1))));
      guide.setAttribute('x1', X(i)); guide.setAttribute('x2', X(i)); guide.setAttribute('visibility', 'visible');
      showDay(i);
    };
    svg.addEventListener('pointermove', at);
    svg.addEventListener('pointerdown', at);
    svg.addEventListener('pointerleave', () => { guide.setAttribute('visibility', 'hidden'); showDay(days.length - 1); });
    holder.appendChild(svg);
    holder.dataset.breite = String(holder.clientWidth);
  }
  showDay(days.length - 1);
  charts.push({ holder, draw });
  draw();
}

let resizeTimer = 0;
window.addEventListener('resize', () => {
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    for (const c of charts) if (c.holder.dataset.breite !== String(c.holder.clientWidth)) c.draw();
  }, 150);
});

// Knopf „Alle zeigen": in der Liste die Zeilen ab dem Grenzwert ein- und ausklappen.
function foldButton(parent, label, onToggle) {
  const b = $('button', { type: 'button', class: 'knopf klein leise' });
  b.addEventListener('click', () => { const open = onToggle(); b.textContent = label(open); b.setAttribute('aria-expanded', String(open)); });
  parent.appendChild($('div', { class: 'mehrzeile' }, [b]));
  return b;
}

// Balkenlisten mit mehr als BARS_LIMIT Zeilen zeigen eingeklappt die ersten
// BARS_SHOWN, bei jedem Laden, ohne gemerkten Zustand.
const BARS_SHOWN = 3, BARS_LIMIT = 4;

// Gibt die Kurzfassung für die kompakte Ansicht zurück: die Zeile mit dem
// größten Wert (bei Gleichstand die erste).
function bars(parent, rows, fmt) {
  if (!rows.length) { parent.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' })); return ''; }
  const best = rows.reduce((a, r) => (r[1] > a[1] ? r : a), rows[0]);
  const summary = best[0] + ': ' + fmt(best[1], best[2]);
  const max = Math.max(...rows.map((r) => r[1]), 1e-9);
  const extra = [];
  const fold = rows.length > BARS_LIMIT;
  const list = $('div', { class: 'liste' });
  rows.forEach(([label, v, more], i) => {
    const fill = $('div', { class: 'fill' }); fill.style.width = ((v / max) * 100).toFixed(1) + '%';
    const line = $('div', { class: 'bar' }, [
      $('span', { text: label }), $('div', { class: 'track' }, [fill]),
      $('span', { class: 'n', text: fmt(v, more) })]);
    if (fold && i >= BARS_SHOWN) { extra.push(line); line.hidden = true; }
    list.appendChild(line);
  });
  parent.appendChild(list);
  if (!extra.length) return summary;
  let open = false;
  const label = (o) => (o ? 'Nur die ersten ' + BARS_SHOWN : 'Alle zeigen (' + rows.length + ')');
  const b = foldButton(parent, label, () => { open = !open; for (const x of extra) x.hidden = !open; return open; });
  b.textContent = label(false); b.setAttribute('aria-expanded', 'false');
  return summary;
}

// Versionsnummern absteigend: 4.10.0 vor 4.9.0, eine fertige vor ihren -rc.
function versionDesc(a, b) {
  const part = (v) => String(v).replace(/^v/, '').split('-');
  const [ka, pa] = part(a), [kb, pb] = part(b);
  const na = ka.split('.').map(Number), nb = kb.split('.').map(Number);
  for (let i = 0; i < Math.max(na.length, nb.length); i++) {
    const d = (nb[i] || 0) - (na[i] || 0);
    if (d) return d;
  }
  if (!pa !== !pb) return pa ? 1 : -1;
  return String(pb || '').localeCompare(String(pa || ''), undefined, { numeric: true });
}

function render(d, span) {
  lastData = d;
  const main = document.getElementById('daten'); main.textContent = '';
  charts.length = 0;
  const days = dayList(span);
  const perDay = {}, perSys = { windows: {}, linux: {} };
  for (const r of d.tage) {
    perDay[r.tag] = (perDay[r.tag] || 0) + r.n;
    perSys[r.system][r.tag] = (perSys[r.system][r.tag] || 0) + r.n;
  }
  const total = days.map((t) => perDay[t] || 0);
  const yesterday = days[days.length - 2], full = days.slice(0, -1);
  const avg = (n) => { const w = full.slice(-n); return w.length ? w.reduce((a, t) => a + (perDay[t] || 0), 0) / w.length : 0; };
  // Der laufende Tag ist noch nicht zu Ende und zählt bei Durchschnitt und
  // bestem Tag bewusst nicht mit (Absicht, nicht Fehler).
  const peak = Math.max(0, ...full.map((t) => perDay[t] || 0));
  const peakDay = full.find((t) => (perDay[t] || 0) === peak);
  // ⭐ Gesamt aus dem Mitschreiben (höchster je gesehener Stand, gelöschte
  // Releases eingeschlossen), sinkt nie. Ohne Verlauf: was GitHub gerade hat.
  const dlNow = d.downloads.reduce((a, r) => a + r.windows + r.linux, 0);
  const lastV = (d.verlauf || [])[(d.verlauf || []).length - 1];
  const dlTotal = lastV ? Math.max(lastV.je_gesehen, dlNow) : dlNow;

  const k = card('Überblick');
  const kpi = (v, l) => $('div', { class: 'kpi' }, [$('div', { class: 'v', text: String(v) }), $('div', { class: 'l', text: l })]);
  k.appendChild($('div', { class: 'winkel datenblatt' }, [$('div', { class: 'zellen' }, [
    $('div', { class: 'kpis fuenf' }, [
      kpi(perDay[today] || 0, 'heute bisher'), kpi(perDay[yesterday] || 0, 'gestern'),
      kpi(avg(7).toFixed(1), 'Durchschnitt letzte 7 Tage'), kpi(avg(30).toFixed(1), 'Durchschnitt letzte 30 Tage'),
      kpi(peak, 'bester abgeschlossener Tag im Zeitraum' + (peak && peakDay ? ' (' + short(peakDay) + ')' : ''))]),
    $('div', { class: 'kpis zwei' }, [
      kpi(dlTotal, 'Downloads gesamt (mit gelöschten)'),
      kpi((d.kurzlinks || []).filter((r) => days.includes(r.tag)).reduce((a, r) => a + r.n, 0),
          'über die Webseite (Zeitraum)')])])]));
  k.appendChild($('p', { class: 'hinweis', text: 'Aktiv = hat sich an dem Tag gemeldet (UTC). Jede Installation höchstens einmal am Tag, ohne Kennung. Wer die Meldung abschaltet, fehlt. Erst ab v3.65.0. Der laufende Tag zählt bei den Durchschnitten und beim besten Tag nicht mit, weil er noch nicht zu Ende ist.' }));

  lineChart(card('Aktive Installationen je Tag', 'paar'), days, [
    { name: 'gesamt', color: C_ALL, values: total, fill: true, width: 2.5 },
    { name: 'Windows', color: C_WIN, values: days.map((t) => perSys.windows[t] || 0) },
    { name: 'Linux', color: C_LIN, dash: true, values: days.map((t) => perSys.linux[t] || 0) }],
    'Aktive Installationen je Tag');

  // Downloads je Tag: aus dem Mitschreiben (Zuwachs des nie sinkenden
  // Gesamtstands). Fehlt ein Tag, steht sein Zuwachs am nächsten gemessenen.
  const perDayDl = {}, perDayW = {}, perDayL = {};
  const vl = d.verlauf || [];
  for (let i = 1; i < vl.length; i++) {
    perDayDl[vl[i].tag] = Math.max(0, vl[i].je_gesehen - vl[i - 1].je_gesehen);
    perDayW[vl[i].tag] = Math.max(0, vl[i].windows - vl[i - 1].windows);
    perDayL[vl[i].tag] = Math.max(0, vl[i].linux - vl[i - 1].linux);
  }
  const dayCard = card('Downloads je Tag', 'paar');
  if (vl.length < 2) {
    dayCard.appendChild($('div', { class: 'empty', text: 'Noch zu wenig Messungen. Der Worker schreibt stündlich mit.' }));
  } else {
    lineChart(dayCard, days, [
      { name: 'gesamt', color: C_ALL, values: days.map((t) => perDayDl[t] || 0), fill: true, width: 2.5 },
      { name: 'Windows', color: C_WIN, values: days.map((t) => perDayW[t] || 0) },
      { name: 'Linux', color: C_LIN, dash: true, values: days.map((t) => perDayL[t] || 0) }],
      'Downloads je Tag');
    dayCard.appendChild($('p', { class: 'hinweis', text: 'Gemessen stündlich. Fehlt ein Tag, steht sein Zuwachs am nächsten gemessenen Tag. Gelöschte Releases zählen mit.' }));
  }

  // Anteile über die letzten 7 Tage INKLUSIVE heute, aber nur Tage mit
  // Meldungen: Summe der Meldungen mit dem Wert geteilt durch alle Meldungen,
  // ein Durchschnitt, keine Personenzahl. (Nur volle Tage hieße: am ersten Tag
  // stünde überall der Leer-Hinweis, obwohl schon Meldungen da sind.)
  const win = new Set(days.slice(-7).filter((t) => perDay[t]));
  const base = [...win].reduce((a, t) => a + (perDay[t] || 0), 0);
  const share = (trait) => { const m = {};
    for (const r of d.merkmale) if (r.merkmal === trait && win.has(r.tag)) m[r.wert] = (m[r.wert] || 0) + r.n;
    return Object.entries(m).sort((a, b) => b[1] - a[1]); };
  const pct = (v) => base ? Math.round((v / base) * 100) + ' %' : NONE;

  // ⭐ Feste Reihenfolge (Symmetrie): erst die breiten Blöcke, dann genau ZWEI
  // Reihen zu je DREI kleinen. Wer einen kleinen Block dazubaut oder
  // wegnimmt, hält die Zahl durch drei teilbar, sonst steht einer allein.

  // Downloads je Version: Windows und Linux je in eigener Farbe, dieselben
  // wie in der Kurve oben. Eingeklappt die letzten 5, ausgeklappt alle fertigen
  // Versionen. Die Wahl merkt sich der Browser (nur bequem, fehlt der Speicher,
  // gilt eingeklappt). Versionen ohne einen einzigen Download (frühe Fassungen)
  // blähen nur auf, außer der neuesten fertigen, die steht immer da, auch mit 0.
  const current = new Set(d.downloads.map((r) => r.tag));
  const newest = d.downloads.filter((r) => !r.vorab)
    .reduce((a, r) => (!a || String(r.am).localeCompare(String(a.am)) > 0 ? r : a), null);
  // Gelöschte fertige Versionen aus dem Mitschreiben dazu, sonst wären ihre
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
  const dlCard = card('Downloads je Version (neueste zuerst)', 'halb');
  zusammen(dlCard, rel.length ? rel[0].tag + ': ' + rel[0].gesamt + ' Downloads' : '');
  const dlMax = Math.max(1, ...rel.map((r) => r.gesamt));
  const extra = [];
  const dlList = $('div', { class: 'liste versionen' });
  for (const [i, r] of rel.entries()) {
    const seg = (n, color) => { const f = $('div', { class: 'fill' }); f.style.width = ((n / dlMax) * 100).toFixed(1) + '%'; f.style.background = color; return f; };
    // Bei übernommenen Altdaten ist die Aufteilung unbekannt: dann ein grauer
    // Balken für den Rest, statt eine Aufteilung zu erfinden.
    const rest = Math.max(0, r.gesamt - r.windows - r.linux);
    const split = r.windows + r.linux > 0 ? '  (' + r.windows + ' Windows · ' + r.linux + ' Linux)' : '';
    const line = $('div', { class: 'bar' }, [
      $('span', { text: r.tag + ' · ' + short(String(r.am)) + (r.weg ? ' · gelöscht' : '') }),
      $('div', { class: 'track stack' }, [seg(r.windows, C_WIN), seg(r.linux, C_LIN), seg(rest, 'var(--leise)')]),
      $('span', { class: 'n', text: r.gesamt + split })]);
    if (i >= SHOWN) { extra.push(line); line.hidden = !open; }
    dlList.appendChild(line);
  }
  dlCard.appendChild(dlList);
  if (!rel.length) dlCard.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' }));
  if (extra.length) {
    const label = (o) => (o ? 'Nur die letzten ' + SHOWN : 'Alle Versionen zeigen (' + rel.length + ')');
    const b = foldButton(dlCard, label, () => {
      open = !open;
      for (const x of extra) x.hidden = !open;
      try { localStorage.setItem('dl-alle', open ? '1' : '0'); } catch (e) { /* nur bequem */ }
      return open;
    });
    b.textContent = label(open); b.setAttribute('aria-expanded', String(open));
  }
  dlCard.appendChild(legend([{ name: 'Windows', color: C_WIN }, { name: 'Linux', color: C_LIN }]));
  if (d.downloads_stand) {
    const at = new Date(d.downloads_stand).toLocaleTimeString('de-DE', { hour: '2-digit', minute: '2-digit' });
    dlCard.appendChild($('p', { class: 'hinweis', text: d.downloads_alt
      ? 'GitHub antwortet gerade nicht. Das sind die Zahlen von ' + at + ' Uhr.'
      : 'Zahlen von GitHub, Stand ' + at + ' Uhr.' }));
  } else if (d.downloads_alt) {
    dlCard.appendChild($('p', { class: 'hinweis', text: 'GitHub antwortet gerade nicht. Es gibt noch keine gespeicherten Zahlen.' }));
  }

  // Länder: aktive Nutzer (Durchschnitt 7 Tage) und Downloads über die Kurzlinks
  // im gewählten Zeitraum nebeneinander. Downloads direkt bei GitHub und
  // Auto-Updates kennen kein Land und fehlen hier.
  const lands = {};
  const row = (c) => (lands[c] = lands[c] || { users: 0, dl: 0 });
  for (const r of d.tage) if (win.has(r.tag)) row(r.land).users += r.n;
  const inSpan = new Set(days);
  for (const r of (d.kurzlinks || [])) if (inSpan.has(r.tag)) row(r.land).dl += r.n;
  const userSum = Object.values(lands).reduce((a, x) => a + x.users, 0);
  const dlSum = Object.values(lands).reduce((a, x) => a + x.dl, 0);
  const lc = card('Länder: Nutzer und Downloads', 'halb');
  const entries = Object.entries(lands).sort((a, b) => (b[1].users - a[1].users) || (b[1].dl - a[1].dl));
  const userPct = (x) => (userSum ? Math.round((x.users / userSum) * 100) + ' %' : NONE);
  const dlPct = (x) => (dlSum ? Math.round((x.dl / dlSum) * 100) + ' %' : NONE);
  zusammen(lc, entries.length ? landName(entries[0][0]) + ': ' + userPct(entries[0][1]) + ' der Nutzer' : '');
  if (!entries.length) {
    lc.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' }));
  } else {
    // Drei Spalten, damit die Tabelle auch halbbreit ohne Rollen passt; der
    // Anteil steht klein unter der Zahl.
    const th = (t, sub) => $('th', {}, [document.createTextNode(t), $('small', { text: sub })]);
    const head = $('tr', {}, [th('Land', ''), th('Nutzer/Tag', 'Durchschnitt 7 Tage'), th('Downloads', 'Zeitraum')]);
    const zelle = (zahl, anteil) => $('td', { class: 'num' }, [$('span', { class: 'zahl', text: zahl }), $('span', { class: 'anteil', text: anteil })]);
    // Wie bei den Versionen: eingeklappt die ersten 5, ausgeklappt alle.
    // Eigener Merker, damit beide Listen unabhängig auf- und zuklappen.
    const LAND_SHOWN = 5;
    let landOpen = false;
    try { landOpen = localStorage.getItem('land-alle') === '1'; } catch (e) { landOpen = false; }
    const landExtra = [];
    const body = entries.map(([c, x], i) => {
      const tr = $('tr', {}, [
        $('td', { text: landName(c) }),
        zelle(win.size ? (x.users / win.size).toFixed(1) : NONE, userPct(x)),
        zelle(String(x.dl), dlPct(x))]);
      if (i >= LAND_SHOWN) { landExtra.push(tr); tr.hidden = !landOpen; }
      return tr;
    });
    lc.appendChild($('div', { class: 'scroll' }, [$('table', { class: 'tab' }, [$('thead', {}, [head]), $('tbody', {}, body)])]));
    if (landExtra.length) {
      const label = (o) => (o ? 'Nur die ersten ' + LAND_SHOWN : 'Alle Länder zeigen (' + entries.length + ')');
      const b = foldButton(lc, label, () => {
        landOpen = !landOpen;
        for (const x of landExtra) x.hidden = !landOpen;
        try { localStorage.setItem('land-alle', landOpen ? '1' : '0'); } catch (e) { /* nur bequem */ }
        return landOpen;
      });
      b.textContent = label(landOpen); b.setAttribute('aria-expanded', String(landOpen));
    }
  }
  lc.appendChild($('p', { class: 'hinweis', text: 'Nutzer/Tag = aktive Nutzer je Tag im Durchschnitt der letzten 7 Tage, Downloads = im gewählten Zeitraum, klein darunter der Anteil. Downloads hier = über xharig.com/windows und /linux (Knöpfe der Webseite). Direkt bei GitHub und per Auto-Update verrät GitHub kein Land.' }));

  // Zwei Reihen zu je drei kleinen Blöcken.
  const lastDay = [...days].reverse().find((t) => perDay[t]) || today;
  const vers = {}; for (const r of d.tage) if (r.tag === lastDay) vers[r.version] = (vers[r.version] || 0) + r.n;
  const klein = (title, rows, fmt) => { const b = card(title, 'klein'); zusammen(b, bars(b, rows, fmt)); };
  klein('Versionen ' + (lastDay === today ? 'heute' : lastDay === yesterday ? 'gestern' : 'am ' + short(lastDay)),
    Object.entries(vers).sort((a, b) => versionDesc(a[0], b[0])).slice(0, 10), (v) => v);
  const sys = { Windows: 0, Linux: 0 };
  for (const r of d.tage) if (win.has(r.tag)) sys[r.system === 'linux' ? 'Linux' : 'Windows'] += r.n;
  klein('System (Durchschnitt 7 Tage)', Object.entries(sys), pct);
  klein('Sprache der Oberfläche (Durchschnitt 7 Tage)', share('ui').map(([c, v]) => [langName(c), v]), pct);

  klein('Sprache des Spiels (Durchschnitt 7 Tage)', share('game').map(([c, v]) => [langName(c), v]), pct);
  klein('Eingeschaltete Bereiche (Durchschnitt 7 Tage)', share('mod').map(([c, v]) => [NAMES.mod[c] || c, v]), pct);
  klein('Einstellungen (Durchschnitt 7 Tage)', [
    ['Testversionen an', (share('rc').find((x) => x[0] === 'ja') || [0, 0])[1]],
    ['Autostart an', (share('autostart').find((x) => x[0] === 'ja') || [0, 0])[1]],
    ['Auto-Update an', (share('update').find((x) => x[0] === 'ja') || [0, 0])[1]],
    ...share('overlay').map(([c, v]) => ['Overlay ' + (NAMES.overlay[c] || c), v]),
    ...share('ansicht').map(([c, v]) => [c === 'karten' ? 'Kartenansicht' : 'Reiteransicht', v])], pct);

  // Seiten des Hauptfensters und Handlungen über den gewählten Zeitraum:
  // breite Blöcke unter den kleinen, damit die zwei Dreierreihen geschlossen bleiben.
  pageBlocks(d, inSpan);

  // Ausgeblendete Kacheln der Startseite: jede aus TILES, auch mit 0.
  // Anteil an den Meldungen der letzten 7 Tage (ältere Fassungen melden
  // nichts und ziehen den Anteil nach unten, bis sie ausgetauscht sind).
  const off = Object.fromEntries(share('kachel_aus'));
  const tc = card('Ausgeblendete Kacheln der Startseite (Durchschnitt 7 Tage)');
  zusammen(tc, bars(tc, Object.keys(TILES).map((k) => [TILES[k], off[k] || 0]), pct));
  tc.appendChild($('p', { class: 'hinweis', text: 'Erst ab v4.1.2 gemeldet. Ab Werk ist nur die Spielzeit ausgeblendet.' }));

  document.getElementById('stand').textContent = 'Stand ' + new Date().toLocaleString('de-DE')
    + ' · lädt alle 10 Minuten neu';
}

function pageBlocks(d, inSpan) {
  const sum = (rows, key) => { const m = {};
    for (const r of rows || []) if (inSpan.has(r.tag)) { const k = key(r); m[k] = (m[k] || 0) + r.n; }
    return m; };

  // Rangliste: jede Seite aus PAGES, auch mit 0, der Balken nach Wegen
  // aufgeteilt. Bei gleicher Zahl gilt die Reihenfolge der Seitenleiste.
  const views = sum(d.seiten, (r) => alias(r.seite));
  const ways = sum(d.seiten_wege, (r) => alias(r.seite) + '|' + r.weg);
  const order = Object.keys(PAGES).filter((p) => !MERGED[p]);
  const ranked = order.slice().sort((a, b) => ((views[b] || 0) - (views[a] || 0)) || (order.indexOf(a) - order.indexOf(b)));
  const rc = card('Seiten nach Aufrufen (Zeitraum)');
  const top = Math.max(1, ...ranked.map((p) => views[p] || 0));
  const total = ranked.reduce((a, p) => a + (views[p] || 0), 0);
  zusammen(rc, total ? pageName(ranked[0]) + ': ' + (views[ranked[0]] || 0) + ' von ' + total + ' Aufrufen' : '');
  const list = $('div', { class: 'liste cols' });
  // Eingeklappt die ersten SEITEN_SHOWN, gerade Zahl, damit beide Spalten
  // gleich lang sind; bei jedem Laden, ohne gemerkten Zustand.
  const SEITEN_SHOWN = 10;
  const seitenExtra = [];
  ranked.forEach((p, i) => {
    const v = views[p] || 0;
    const parts = Object.keys(ROUTES).map((w) => {
      const seg = $('div', { class: 'fill' });
      seg.style.width = (((ways[p + '|' + w] || 0) / top) * 100).toFixed(1) + '%';
      seg.style.background = ROUTE_COLORS[w];
      seg.title = ROUTES[w] + ': ' + (ways[p + '|' + w] || 0);
      return seg;
    });
    const line = $('div', { class: 'bar' + (v ? '' : ' dim') }, [
      $('span', {}, [document.createTextNode(pageName(p)), $('span', { class: 'grp', text: PAGES[p][0] })]),
      $('div', { class: 'track stack' }, parts),
      $('span', { class: 'n', text: String(v) })]);
    if (i >= SEITEN_SHOWN) { seitenExtra.push(line); line.hidden = true; }
    list.appendChild(line);
  });
  rc.appendChild(list);
  if (seitenExtra.length) {
    let seitenOpen = false;
    const label = (o) => (o ? 'Nur die ersten ' + SEITEN_SHOWN : 'Alle Seiten zeigen (' + ranked.length + ')');
    const b = foldButton(rc, label, () => { seitenOpen = !seitenOpen; for (const x of seitenExtra) x.hidden = !seitenOpen; return seitenOpen; });
    b.textContent = label(false); b.setAttribute('aria-expanded', 'false');
  }
  rc.appendChild(legend(Object.keys(ROUTES).map((w) => ({ name: ROUTES[w], color: ROUTE_COLORS[w] }))));
  rc.appendChild($('p', { class: 'hinweis', text: total
    ? total + ' Aufrufe im Zeitraum. Wer die Meldung abschaltet, fehlt; ältere Fassungen zählen keine Seiten.'
    : 'Noch keine Seitenaufrufe im Zeitraum.' }));

  // Klicks bis zum Ziel: je Seite die Verteilung 0 … MAX_CLICKS+ und ihr Median.
  const dist = {};
  for (const r of d.seiten_klicks || []) {
    if (!inSpan.has(r.tag)) continue;
    const k = Math.min(Number(r.klicks), MAX_CLICKS);
    const m = dist[alias(r.seite)] = dist[alias(r.seite)] || {};
    m[k] = (m[k] || 0) + r.n;
  }
  const median = (m) => { const n = Object.values(m).reduce((a, x) => a + x, 0); let run = 0;
    for (let k = 0; k <= MAX_CLICKS; k++) { run += m[k] || 0; if (run * 2 >= n) return k; } return MAX_CLICKS; };
  const kc = card('Klicks bis zum Ziel');
  const targets = Object.keys(dist).filter((p) => PAGES[p])
    .map((p) => [p, Object.values(dist[p]).reduce((a, x) => a + x, 0)])
    .sort((a, b) => (b[1] - a[1]) || (order.indexOf(a[0]) - order.indexOf(b[0])));
  zusammen(kc, targets.length ? pageName(targets[0][0]) + ': Median ' + stepName(median(dist[targets[0][0]])) + ' Klicks' : '');
  if (!targets.length) {
    kc.appendChild($('div', { class: 'empty', text: 'Noch keine Daten.' }));
  } else {
    const axis = $('div', { class: 'hist axis' });
    for (let k = 0; k <= MAX_CLICKS; k++) axis.appendChild($('span', { text: stepName(k) }));
    const head = $('tr', {}, [$('th', { text: 'Seite' }), $('th', { text: 'erreicht' }), $('th', { text: 'Median' }),
      $('th', {}, [document.createTextNode('Verteilung (Klicks) '), axis])]);
    // Eingeklappt auf die ersten KLICK_SHOWN Seiten, bei jedem Laden, ohne
    // gemerkten Zustand.
    const KLICK_SHOWN = 5;
    const klickExtra = [];
    const body = targets.map(([p, n], i) => {
      const m = dist[p]; const hi = Math.max(1, ...Object.values(m));
      const hist = $('div', { class: 'hist' });
      for (let k = 0; k <= MAX_CLICKS; k++) {
        const b = $('div'); b.style.height = (((m[k] || 0) / hi) * 100).toFixed(0) + '%';
        if (!m[k]) b.style.background = 'var(--linie)';
        b.title = stepName(k) + ' Klicks: ' + (m[k] || 0); hist.appendChild(b);
      }
      const tr = $('tr', {}, [$('td', { text: pageName(p) }), $('td', { class: 'num', text: String(n) }),
        $('td', { class: 'num', text: stepName(median(m)) }), $('td', { class: 'num' }, [hist])]);
      if (i >= KLICK_SHOWN) { klickExtra.push(tr); tr.hidden = true; }
      return tr;
    });
    kc.appendChild($('div', { class: 'scroll' }, [$('table', { class: 'tab' }, [$('thead', {}, [head]), $('tbody', {}, body)])]));
    if (klickExtra.length) {
      let klickOpen = false;
      const label = (o) => (o ? 'Nur die ersten ' + KLICK_SHOWN : 'Alle Seiten zeigen (' + targets.length + ')');
      const b = foldButton(kc, label, () => { klickOpen = !klickOpen; for (const x of klickExtra) x.hidden = !klickOpen; return klickOpen; });
      b.textContent = label(false); b.setAttribute('aria-expanded', 'false');
    }
  }
  kc.appendChild($('p', { class: 'hinweis', text: 'Ziel = eine Seite, auf der man mindestens 3 Sekunden bleibt. Klicks = Seitenwechsel und Auf- oder Zuklappen in der Seitenleiste, ab dem Öffnen des Fensters bzw. seit dem letzten Ziel. 0 = gleich beim Öffnen da, ' + MAX_CLICKS + '+ = ' + MAX_CLICKS + ' oder mehr.' }));

  // Häufigste Fehlgriffe als „A → B".
  const miss = sum(d.seiten_fehlgriffe, (r) => alias(r.von) + '>' + alias(r.nach));
  const pairs = Object.entries(miss).filter(([k]) => k.split('>').every((p) => PAGES[p]))
    .sort((a, b) => b[1] - a[1]).slice(0, 15)
    .map(([k, n]) => { const [a, b] = k.split('>'); return [pageName(a) + ' → ' + pageName(b), n]; });
  const mc = card('Häufigste Fehlgriffe');
  zusammen(mc, bars(mc, pairs, (v) => v));
  mc.appendChild($('p', { class: 'hinweis', text: 'Fehlgriff = Seite nach weniger als 3 Sekunden wieder verlassen. Rechts die Seite, auf der man danach geblieben ist.' }));

  // Handlungen: jede aus ACTIONS, auch mit 0, in der Reihenfolge der Liste.
  const done = sum(d.seiten_handlungen, (r) => r.handlung);
  const ac = card('Handlungen (Zeitraum)');
  zusammen(ac, bars(ac, Object.keys(ACTIONS).map((k) => [ACTIONS[k], done[k] || 0]), (v) => v));
  ac.appendChild($('p', { class: 'hinweis', text: 'Wie oft eine Handlung genutzt wurde. Es zählt nur die Anzahl, keine Mengen oder Namen.' }));
}

let span = 90;
const rangeButtons = Array.from(document.querySelectorAll('#zeitraum button'));
const button = document.getElementById('neu');
const buttonText = document.getElementById('neu-text');
async function load() {
  button.disabled = true; buttonText.textContent = 'Lädt …';
  try {
    const r = await fetch('/daten?tage=' + span, { credentials: 'same-origin', redirect: 'manual' });
    // Nach 24 Stunden läuft die Anmeldung ab, dann leitet Access um, und
    // nur ein Neuladen der ganzen Seite führt durch die Anmeldung.
    if (r.type === 'opaqueredirect' || r.status === 0 || r.status === 403) {
      document.getElementById('stand').textContent = 'Anmeldung abgelaufen. Lade die Seite neu (F5).';
      return;
    }
    if (!r.ok) throw new Error('HTTP ' + r.status);
    render(await r.json(), span);
  } catch (e) {
    document.getElementById('stand').textContent = 'Laden fehlgeschlagen (' + e.message + ')';
  } finally {
    button.disabled = false; buttonText.textContent = 'Aktualisieren';
  }
}
for (const b of rangeButtons) b.addEventListener('click', () => {
  span = Number(b.dataset.tage);
  for (const o of rangeButtons) o.setAttribute('aria-pressed', String(o === b));
  load();
});
// Kompakt / ausführlich: Schalter oben. Die Wahl merkt sich der Browser; die
// Zahlen werden nicht neu geholt, nur anders aufgebaut.
const modeSwitch = document.getElementById('kompakt');
const showMode = () => { document.getElementById('inhalt').classList.toggle('kompakt', compact); };
modeSwitch.checked = compact; showMode();
modeSwitch.addEventListener('change', () => {
  compact = modeSwitch.checked;
  try { localStorage.setItem('ansicht', compact ? 'kompakt' : 'ausfuehrlich'); } catch (e) { /* nur bequem */ }
  showMode();
  if (lastData) render(lastData, span);
});
// Der Knopf lädt die GANZE Seite neu, nicht nur die Zahlen, sonst blieb nach
// einer neuen Fassung der Übersicht die alte stehen, bis jemand F5 drückt.
button.addEventListener('click', () => location.reload());
load();
setInterval(load, 10 * 60 * 1000);
})();`;
}

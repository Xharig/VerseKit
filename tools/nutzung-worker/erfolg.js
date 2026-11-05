// SPDX-License-Identifier: GPL-3.0-only
//
// Die Rechnungen hinter dem Block „Wie läuft VerseKit?": je Kennzahl diese
// Woche gegen die Vorwoche. Reine Funktionen ohne Seite und ohne Datenbank,
// damit worker.test.mjs sie direkt prüfen kann. client.js setzt denselben
// Quelltext (`Function.prototype.toString`) ins Skript der Seite.
// ⚠ Darum gilt hier dieselbe Regel wie in client.js: kein Gravis, kein Dollar
// mit geschweifter Klammer, kein Rückwärtsschrägstrich (Prüfung im Test).
//
// „Woche" heißt immer: die letzten 7 VOLLEN Tage (gestern und sechs davor),
// „Vorwoche" die 7 Tage davor. Der laufende Tag zählt nie mit, weil er noch
// nicht zu Ende ist. Tage sind UTC-Daten als Text, JJJJ-MM-TT.

// Der Tag `n` Tage vor `day`.
export function dayBefore(day, n) {
  return new Date(Date.parse(day + 'T00:00:00Z') - n * 86400000).toISOString().slice(0, 10);
}

// Die 7 vollen Tage der Woche `back` (0 = diese, 1 = Vorwoche, …), älteste zuerst.
export function weekDays(today, back) {
  const out = [];
  for (let i = 7; i >= 1; i--) out.push(dayBefore(today, i + 7 * back));
  return out;
}

// Veränderung von `before` zu `now`. mode 'prozent': relativ in Prozent,
// mode 'punkte': Unterschied zweier Anteile in Prozentpunkten.
// Gibt { art: 'auf' | 'ab' | 'gleich' | 'neu' | 'keiner', pfeil, text } zurück;
// der Pfeil steht immer auch als Zeichen da, nie nur als Farbe.
export function trend(now, before, mode) {
  if (now === null || now === undefined) return { art: 'keiner', pfeil: '', text: 'noch kein Vergleich' };
  if (before === null || before === undefined) return { art: 'keiner', pfeil: '', text: 'noch keine Vorwoche' };
  if (mode === 'punkte') {
    const d = Math.round((now - before) * 100);
    if (d === 0) return { art: 'gleich', pfeil: '=', text: 'gleich wie Vorwoche' };
    return { art: d > 0 ? 'auf' : 'ab', pfeil: d > 0 ? '▲' : '▼',
             text: (d > 0 ? '+' : '−') + Math.abs(d) + ' Punkte zur Vorwoche' };
  }
  if (before === 0) {
    if (now === 0) return { art: 'gleich', pfeil: '=', text: 'gleich wie Vorwoche' };
    return { art: 'neu', pfeil: '▲', text: 'neu, Vorwoche 0' };
  }
  const p = Math.round(((now - before) / before) * 100);
  if (p === 0) return { art: 'gleich', pfeil: '=', text: 'gleich wie Vorwoche' };
  return { art: p > 0 ? 'auf' : 'ab', pfeil: p > 0 ? '▲' : '▼',
           text: (p > 0 ? '+' : '−') + Math.abs(p) + ' % zur Vorwoche' };
}

// a) Aktive Installationen je Tag: Schnitt der Woche gegen die Vorwoche.
// `sums` = [{ tag, n }] (Summe je Tag). Vor der ersten Meldung überhaupt gibt
// es keine Vorwoche (null), sonst würde der Start als Wachstum aus 0 gelten.
export function activeWeek(sums, today) {
  const per = {};
  let first = null;
  for (const r of sums) {
    per[r.tag] = (per[r.tag] || 0) + r.n;
    if (r.n > 0 && (first === null || r.tag < first)) first = r.tag;
  }
  const avg = (days) => days.reduce((a, t) => a + (per[t] || 0), 0) / days.length;
  const now = weekDays(today, 0), prev = weekDays(today, 1);
  if (first === null || first > now[6]) return { jetzt: null, vorher: null };
  return { jetzt: avg(now), vorher: first <= prev[0] ? avg(prev) : null };
}

// Erster Tag, an dem überhaupt ein Alter gemeldet wurde (ab VerseKit 5.2), oder null.
function firstAge(ages) {
  let first = null;
  for (const r of ages) if (first === null || r.tag < first) first = r.tag;
  return first;
}

// b) Neue Installationen (Alter 0) in der Woche gegen die Vorwoche.
// `ages` = [{ tag, tage, n }]. Ohne jede Alters-Meldung: null (Kachel zeigt
// „ab VerseKit 5.2"). Eine Woche zählt erst, wenn sie ganz in der Zeit mit
// Alters-Meldungen liegt, sonst wäre der erste Teil der Woche fälschlich 0.
export function newWeek(ages, today) {
  const first = firstAge(ages);
  if (first === null) return { jetzt: null, vorher: null };
  const sum = (days) => { const set = new Set(days);
    return ages.reduce((a, r) => a + (r.tage === 0 && set.has(r.tag) ? r.n : 0), 0); };
  const now = weekDays(today, 0), prev = weekDays(today, 1);
  return { jetzt: first <= now[0] ? sum(now) : null, vorher: first <= prev[0] ? sum(prev) : null };
}

// c) Bleiben sie: von den Neuen eines Tages t (Alter 0) wie viele am Tag t+7
// noch aktiv sind (Alter 7). Für „diese Woche" die Neuen, deren t+7 in der
// Woche liegt, also t in der Vorwoche; für die Vorwoche eine Woche früher.
// Gibt Anteile 0 … 1 zurück, null ohne Daten oder ohne Neue.
export function stayWeek(ages, today) {
  const first = firstAge(ages);
  if (first === null) return { jetzt: null, vorher: null, neu: 0 };
  const at = {};
  for (const r of ages) at[r.tag + '|' + r.tage] = (at[r.tag + '|' + r.tage] || 0) + r.n;
  const rate = (back) => {
    const starts = weekDays(today, back + 1);
    if (first > starts[0]) return { anteil: null, neu: 0 };
    let fresh = 0, kept = 0;
    for (const t of starts) { fresh += at[t + '|0'] || 0; kept += at[dayBefore(t, -7) + '|7'] || 0; }
    return { anteil: fresh ? Math.min(1, kept / fresh) : null, neu: fresh };
  };
  const now = rate(0), prev = rate(1);
  return { jetzt: now.anteil, vorher: prev.anteil, neu: now.neu };
}

// d) Downloads der Woche gegen die Vorwoche aus dem Mitschreiben
// (`verlauf` = [{ tag, je_gesehen }], je_gesehen sinkt nie). Ein Tag ohne
// Messung schiebt seinen Zuwachs auf den nächsten gemessenen Tag. Gerechnet
// wird der Stand am Ende des letzten Tags jeder Woche gegen den am Ende der
// Woche davor; fehlt eine Messung vor einer Woche, gibt es dafür keine Zahl.
export function downloadsWeek(verlauf, today) {
  const rows = verlauf.slice().sort((a, b) => (a.tag < b.tag ? -1 : a.tag > b.tag ? 1 : 0));
  // Stand am Ende des Tags day: die letzte Messung an oder vor ihm.
  const standAt = (day) => { let v = null; for (const r of rows) if (r.tag <= day) v = r.je_gesehen; return v; };
  const now = weekDays(today, 0), prev = weekDays(today, 1);
  const end0 = standAt(now[6]), end1 = standAt(prev[6]), end2 = standAt(dayBefore(prev[0], 1));
  return {
    jetzt: end0 !== null && end1 !== null ? Math.max(0, end0 - end1) : null,
    vorher: end1 !== null && end2 !== null ? Math.max(0, end1 - end2) : null,
  };
}

// Neue Leute: Downloads über die Kurzlinks der Webseite (`kurzlinks` = [{ tag, n }])
// in den 7 vollen Tagen gegen die 7 davor. Auto-Updates laufen nicht über die
// Kurzlinks, darum zählt das neue Nutzer, nicht Updates bestehender.
export function siteWeek(kurzlinks, today) {
  const sum = (days) => { const set = new Set(days); let n = 0; for (const r of kurzlinks) if (set.has(r.tag)) n += r.n; return n; };
  return { jetzt: sum(weekDays(today, 0)), vorher: sum(weekDays(today, 1)) };
}

// Ab welcher Stelle eine Tagesreihe Daten hat: Index des ersten Tags mit einem
// Wert über 0 (0, wenn es keinen gibt). Davor liegt nur die Zeit vor dem Zählen.
export function firstData(values) {
  const i = values.findIndex((v) => v > 0);
  return i < 0 ? 0 : i;
}

// Stärkster Kanal der Woche (`kanaele` = [{ tag, kanal, n }]) als
// { kanal, n, gesamt } oder null, wenn kein Download ein Kennzeichen trug.
// Bei Gleichstand der alphabetisch erste, damit die Anzeige nicht springt.
export function topChannel(kanaele, today) {
  const set = new Set(weekDays(today, 0));
  const m = {};
  let total = 0;
  for (const r of kanaele) if (set.has(r.tag)) { m[r.kanal] = (m[r.kanal] || 0) + r.n; total += r.n; }
  const best = Object.keys(m).sort().reduce((a, k) => (a === null || m[k] > m[a] ? k : a), null);
  return best === null ? null : { kanal: best, n: m[best], gesamt: total };
}

// Aktive Nutzer je Woche (Montag bis Sonntag, UTC): Schnitt der vollen Tage
// jeder Woche, ab dem ersten Tag mit Meldungen und ohne den laufenden Tag.
// Gibt [{ ab, schnitt, tage }] zurück, `ab` = Montag der Woche.
export function weeklyActive(sums, today) {
  const per = {};
  let first = null;
  for (const r of sums) {
    per[r.tag] = (per[r.tag] || 0) + r.n;
    if (r.n > 0 && (first === null || r.tag < first)) first = r.tag;
  }
  const last = dayBefore(today, 1);
  if (first === null || first > last) return [];
  const monday = (day) => dayBefore(day, (new Date(day + 'T00:00:00Z').getUTCDay() + 6) % 7);
  const out = [];
  for (let start = monday(first); start <= last; start = dayBefore(start, -7)) {
    let sum = 0, n = 0;
    for (let i = 0; i < 7; i++) {
      const t = dayBefore(start, -i);
      if (t < first || t > last) continue;
      sum += per[t] || 0; n += 1;
    }
    out.push({ ab: start, schnitt: n ? sum / n : 0, tage: n });
  }
  return out;
}

// Fertige Versionen mit Datum für die Markierungen in der Wochenkurve, aus
// den GitHub-Zahlen und dem Mitschreiben (gelöschte eingeschlossen), jede
// Version einmal, älteste zuerst. Vorabversionen (-rc) zählen nicht.
export function releaseMarks(downloads, bestand) {
  const seen = {};
  for (const r of downloads) if (!r.vorab && r.am) seen[r.tag] = String(r.am).slice(0, 10);
  for (const b of bestand) {
    if (b.vorab || seen[b.version]) continue;
    const am = b.veroeffentlicht || '';
    if (am) seen[b.version] = String(am).slice(0, 10);
  }
  return Object.keys(seen).map((tag) => ({ tag, am: seen[tag] }))
    .sort((a, b) => (a.am < b.am ? -1 : a.am > b.am ? 1 : 0));
}

// Alle Funktionen, in der Reihenfolge, in der client.js sie einsetzt.
export const ERFOLG = [dayBefore, weekDays, trend, activeWeek, firstAge, newWeek, stayWeek,
  downloadsWeek, siteWeek, firstData, topChannel, weeklyActive, releaseMarks];

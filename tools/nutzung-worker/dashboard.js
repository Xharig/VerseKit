// SPDX-License-Identifier: GPL-3.0-only
//
// Die private Übersicht auf statistik-versekit.xharig.com. Eine Seite, keine fremden
// Skripte, keine Schriften von außen: Alles steht hier, gezeichnet wird als
// SVG im Browser. Die Content-Security-Policy des Workers lässt nur Skript
// und Stil mit dem Einmal-Wert (`nonce`) dieser Antwort zu, dazu Schrift und
// Bilder als Daten-Adressen.
//
// Aussehen: das gemeinsame Design aller Xharig-Seiten (basis.js, unverändert
// aus der Mutterkopie) plus der Stil dieser einen Seite unten. Das Skript
// steht in client.js.

import { ICON } from './icon.js';
import { BASIS_CSS } from './basis.js';
import { FONT_400, FONT_700 } from './schrift.js';
import { MAX_CLICKS } from './pages.js';
import { clientScript } from './client.js';

// basis.css verweist auf Dateien neben der Seite; hier gibt es nur Daten-Adressen.
const BASIS = BASIS_CSS
  .replace('url(schrift/xharig-sans-400.woff2)', `url(${FONT_400})`)
  .replace('url(schrift/xharig-sans-700.woff2)', `url(${FONT_700})`);

const SEITE_CSS = `
/* ---------- Diese Seite ----------
   Nur, was basis.css nicht kennt. Farben für Windows und Linux: blau und
   orange bleiben auch bei Rot-Grün-Schwäche gut trennbar (Okabe und Ito). */
:root{--win:#56b4e9;--lin:#f08a4b;
  --weg-blau:#4a8de0;--weg-rosa:#e08fd0;--weg-grau:#6b7685;--weg-rot:#c8382c;--weg-eis:#8ee5f0}
[hidden]{display:none !important}
.kopf nav{gap:14px}
.marke em{margin-left:-5px;font-style:normal;font-weight:400;color:var(--leise)}
.sprache button{
  font:inherit;font-size:14px;font-weight:700;color:var(--leise);background:none;border:0;
  padding:5px 12px;min-height:34px;cursor:pointer;
}
.sprache button:hover{background:var(--hover);color:var(--neon)}
.sprache button[aria-pressed=true]{background:var(--neon);color:var(--grund)}
.knopf.klein{padding:5px 14px;font-size:15px;border-width:1px;border-color:var(--neon);color:var(--neon);gap:8px}
.knopf.klein.leise{border-color:var(--linie-hell);color:var(--text);font-weight:400;font-size:14px}
.knopf.klein:hover{border-color:var(--neon);color:var(--neon);background:var(--neon-dunkel)}
.knopf:disabled{opacity:.5;cursor:default}
.knopf:disabled:hover{border-color:var(--neon);color:var(--neon);background:none}

.seitenkopf{padding:44px 0 6px}
.seitenkopf h1{font-size:clamp(30px,4.5vw,42px);margin:0 0 10px}
.seitenkopf .einleitung{margin:0 0 6px;font-size:17px}
.stand{margin:0 0 22px;color:var(--leise);font-size:15px}

/* Sechs Spalten: breit = 6, klein = 2 (drei je Reihe), halb = 3 (ab 1000 px zwei
   nebeneinander: Versionen und Länder immer, die zwei Diagramme nur kompakt) */
.raster{display:grid;grid-template-columns:repeat(6,minmax(0,1fr));column-gap:48px;padding-bottom:64px}
section.block{grid-column:span 6;min-width:0;padding:28px 0 32px;border-top:1px solid var(--linie)}
section.block.klein{grid-column:span 2}
@media (min-width:1000px){
  section.block.halb,.kompakt section.block.paar{grid-column:span 3}
}
.block h2{font-size:24px;line-height:1.25;letter-spacing:0;margin:0 0 18px;max-width:none}
.block .hinweis{font-size:14px}
.empty{color:var(--leise);padding:6px 0 10px}

/* Datenblatt der Kennzahlen: Eckwinkel um alle, Linien dazwischen */
.datenblatt{padding:0}
.zellen{display:grid;gap:1px;background:var(--linie);border-radius:calc(var(--rund) - 1px);overflow:hidden}
.kpis{display:grid;gap:1px;background:var(--linie)}
.kpis.fuenf{grid-template-columns:repeat(5,minmax(0,1fr))}
.kpis.zwei{grid-template-columns:repeat(2,minmax(0,1fr))}
.kpi{background:var(--feld);padding:18px 20px 16px}
.kpi .v{font-size:36px;line-height:1.1;font-weight:700;font-variant-numeric:tabular-nums}
.kpi .l{margin-top:6px;color:var(--leise);font-size:14px;line-height:1.4}

/* Diagramme */
.bild{padding:14px 16px 8px}
.tipp{margin:0 0 6px;min-height:1.6em;font-size:15px;font-variant-numeric:tabular-nums}
.diagramm svg{display:block;max-width:100%;height:auto;touch-action:pan-y}
.diagramm text{fill:var(--leise);font-size:12px;font-family:inherit}
.legende{display:flex;flex-wrap:wrap;gap:6px 22px;margin-top:14px;color:var(--leise);font-size:14px}
.sw{display:inline-block;width:20px;height:4px;border-radius:2px;margin-right:8px;vertical-align:middle}

/* Balkenlisten */
.bar{display:grid;grid-template-columns:minmax(96px,38%) minmax(0,1fr) auto;gap:14px;align-items:center;
  padding:8px 0;border-bottom:1px solid var(--linie);font-size:15px}
.bar .track{height:8px;background:var(--linie);border-radius:2px;overflow:hidden}
.bar .track.stack{display:flex;gap:2px}
.bar .fill{height:100%;background:var(--neon)}
.bar .n{min-width:52px;text-align:right;color:var(--leise);font-variant-numeric:tabular-nums}
.bar .grp{margin-left:8px;color:var(--leise);font-size:12px}
/* Versionen: Name und Zahl oben, der Balken darunter (passt auch halbbreit);
   nur auf mittlerer Breite, wo die Liste die ganze Zeile hat, alles in einer Zeile */
.versionen .bar{grid-template-columns:minmax(0,1fr) auto;gap:6px 12px}
.versionen .bar .track{grid-column:1 / -1;grid-row:2}
.versionen .bar .n{grid-row:1;grid-column:2}
@media (min-width:700px) and (max-width:999px){
  .versionen .bar{grid-template-columns:minmax(96px,200px) minmax(0,1fr) 250px;gap:14px}
  .versionen .bar .track{grid-column:auto;grid-row:auto}
  .versionen .bar .n{grid-row:auto;grid-column:auto}
}
.dim{opacity:.5}
.cols{column-count:2;column-gap:48px}
.cols .bar{break-inside:avoid}
.mehrzeile{margin-top:14px}

/* Tabellen */
.scroll{overflow-x:auto}
.tab{width:100%;border-collapse:collapse;font-size:15px}
.tab th{padding:6px 10px 10px;text-align:left;vertical-align:bottom;color:var(--leise);font-size:14px;
  font-weight:700;border-bottom:1px solid var(--linie-hell)}
.tab td{padding:9px 10px;border-bottom:1px solid var(--linie)}
.tab th:first-child,.tab td:first-child{padding-left:0}
.tab th:not(:first-child){text-align:right}
.tab .num{text-align:right;font-variant-numeric:tabular-nums}
.tab th small{display:block;font-size:12px;font-weight:400;line-height:1.3}
.tab .zahl,.tab .anteil{display:block}
.tab .anteil{font-size:13px;color:var(--leise)}
.hist{display:inline-grid;grid-template-columns:repeat(${MAX_CLICKS + 1},14px);gap:2px;align-items:end;
  height:26px;vertical-align:middle}
.hist div{background:var(--neon);border-radius:1px 1px 0 0;min-height:1px}
.hist.axis{height:auto;align-items:start}
.hist.axis span{color:var(--leise);font-size:11px;font-weight:400;text-align:center}

/* Kompakte Ansicht: engere Abstände, flache Diagramme (in client.js),
   Abschnitte als Falt-Knopf mit der wichtigsten Zahl. Keine Animation. */
.faltet{display:flex;align-items:baseline;flex-wrap:wrap;gap:2px 16px;width:100%;min-height:44px;padding:0;margin:0;
  background:none;border:0;color:inherit;font:inherit;text-align:left;cursor:pointer}
.faltet .t{font-weight:700}
.faltet .z{font-size:15px;font-weight:400;color:var(--leise)}
.faltet .chev{margin-left:auto;align-self:center;width:22px;height:22px;color:var(--neon)}
.klein .faltet .t{flex:1 1 0;min-width:0}
.klein .faltet .chev{order:2;margin-left:0}
.klein .faltet .z{order:3;flex:1 0 100%}
.faltet:hover .t{color:var(--neon)}
.faltet[aria-expanded=true] .chev{transform:rotate(180deg)}
.kompakt section.block{padding:12px 0 14px}
.kompakt section.block.offen{padding-bottom:24px}
.kompakt .block h2{margin-bottom:10px}
.kompakt .block h2.mitfalt{margin:0}
.kompakt .block.offen h2.mitfalt{margin-bottom:12px}
.kompakt .bild{padding:8px 12px 4px}
.kompakt .tipp{margin-bottom:2px;font-size:14px}
.kompakt .legende{margin-top:8px}
.kompakt .kpi{padding:12px 16px}
.kompakt .kpi .v{font-size:30px}
.kompakt .hinweis{margin-top:10px}

footer{padding:28px 0 36px}
.fussnote{margin:0;color:var(--leise);font-size:14px}

@media (max-width:999px){
  .raster{column-gap:32px}
  section.block.klein{grid-column:span 3}
  .kpis.fuenf{grid-template-columns:repeat(2,minmax(0,1fr))}
  .kpis.fuenf .kpi:last-child{grid-column:1 / -1}
}
@media (max-width:860px){
  .sprache button{min-height:44px;padding:0 14px}
  .knopf.klein,.schalter{min-height:44px}
  .cols{column-count:1}
  /* Kopf ohne feste Höhe, nicht mehr mitlaufend; die Elemente des Menüs liegen direkt in der Kopfzeile */
  .kopfleiste{position:static}
  .kopf{height:auto;flex-wrap:wrap;padding-top:8px;padding-bottom:12px;gap:10px}
  .kopf nav{display:contents}
  .marke{order:1}
  #zeitraum{order:2}
  #ansicht{order:3}
  #neu{order:4;margin-left:auto}
}
@media (max-width:640px){
  /* Handy: Name und Aktualisieren oben, darunter Zeitraum und Kompakt */
  #neu{order:2}
  #zeitraum{order:3}
  #ansicht{order:4;margin-left:auto}
  #zeitraum button{padding:0 10px}
  .schalter{gap:8px}
  .seitenkopf{padding-top:28px}
  section.block,.kompakt section.block{padding:24px 0 26px}
  .kompakt section.block{padding:10px 0 12px}
  section.block.klein{grid-column:span 6}
  .block h2{font-size:21px;margin-bottom:14px}
  .kpi{padding:14px 14px 12px}
  .kpi .v{font-size:30px}
  .bild{padding:10px 10px 6px}
  .bar,.versionen .bar{grid-template-columns:minmax(0,1fr) auto;gap:6px 12px}
  .bar .track{grid-column:1 / -1;grid-row:2}
  .bar .n{grid-row:1;grid-column:2}
  .hist{grid-template-columns:repeat(${MAX_CLICKS + 1},9px)}
  .tab{font-size:14px}
  .tab th{font-size:13px}
  .tab th,.tab td{padding-left:5px;padding-right:5px}
}
@media (max-width:420px){
  .neu-text{display:none}
  #neu{min-width:44px;padding:5px 10px;justify-content:center}
}
`;

export function dashboardHtml(nonce) {
  return `<!doctype html>
<html lang="de">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="robots" content="noindex, nofollow">
<meta name="theme-color" content="#10141c">
<meta name="color-scheme" content="dark">
<title>VerseKit Statistik</title>
<link rel="icon" type="image/png" href="${ICON}">
<style nonce="${nonce}">
${BASIS}
${SEITE_CSS}
</style>
</head>
<body>
<a class="springen" href="#inhalt">Zum Inhalt springen</a>

<header class="kopfleiste">
  <div class="mitte kopf">
    <a class="marke" href="/"><img src="${ICON}" alt="" width="36" height="36"><strong>VerseKit</strong> <em>Statistik</em></a>
    <nav aria-label="Zeitraum und Aktualisieren">
      <div class="sprache" id="zeitraum" role="group" aria-label="Zeitraum">
        <button type="button" data-tage="30" aria-pressed="false">30 Tage</button>
        <button type="button" data-tage="90" aria-pressed="true">90 Tage</button>
        <button type="button" data-tage="365" aria-pressed="false">1 Jahr</button>
      </div>
      <label class="schalter" id="ansicht" title="Kompakt: flachere Diagramme nebeneinander, Abschnitte eingeklappt">
        <span>Kompakt</span>
        <input type="checkbox" id="kompakt" role="switch">
      </label>
      <button class="knopf klein" id="neu" type="button" title="Lädt die Seite neu. Die Zahlen kommen sonst alle 10 Minuten von selbst.">
        <svg class="ic" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8"/><path d="M21 3v5h-5"/><path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16"/><path d="M8 16H3v5"/></svg>
        <span class="neu-text" id="neu-text">Aktualisieren</span>
      </button>
    </nav>
  </div>
</header>

<main id="inhalt" class="mitte">
  <div class="seitenkopf">
    <h1>Nutzung und Downloads</h1>
    <p class="einleitung">Private Übersicht, nur für dich sichtbar.</p>
    <p class="stand" id="stand" role="status">lädt …</p>
  </div>
  <div class="raster" id="daten"></div>
</main>

<footer>
  <div class="mitte"><p class="fussnote">Gezählt wird ohne Kennung. Es wird keine IP-Adresse gespeichert.</p></div>
</footer>
<script nonce="${nonce}">
${clientScript()}
</script>
</body>
</html>`;
}

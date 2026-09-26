// SPDX-License-Identifier: GPL-3.0-only
//
// Weiterleitung der Verse-Kit-Fehlerberichte an Discord — läuft als
// Cloudflare Worker, NICHT im Programm und NICHT auf einem Heimserver.
//
// Warum es das gibt: Bis v3.57 stand die Discord-Webhook-Adresse in der
// fertigen Programmdatei. Wer sie ausliest, kann in den Kanal schreiben —
// auch @everyone. Jetzt kennt das Programm nur noch diese öffentliche
// Adresse; der Webhook liegt ausschließlich als Geheimnis beim Worker
// (`DISCORD_WEBHOOK`) und lässt sich jederzeit tauschen, ohne neue Version.
//
// Der Worker nimmt NUR an, was wie ein Verse-Kit-Bericht aussieht, begrenzt
// die Menge je Absender und schaltet alle Erwähnungen ab. Die Nachricht in
// Discord baut er selbst zusammen — vom Absender kommt nur die geprüfte
// Versionsnummer und die Dateien, kein freier Text.

const MAX_BYTES = 8 * 1024 * 1024;   // Discord nimmt je Nachricht mehr, wir brauchen nicht mehr
const MAX_FILES = 10;                // Discord: höchstens zehn Dateien je Nachricht
const HEAD_RE = /^\*\*Fehlerbericht\*\* · (\d{1,3}\.\d{1,3}\.\d{1,3}(?:-rc\d{1,4})?)$/;
const REPORT_NAME_RE = /^bericht-\d{4}-\d{2}-\d{2}-\d{4}\.txt$/;
const REPORT_START = 'Verse-Kit ';
const ALLOWED_TYPES = ['text/plain', 'image/png', 'image/jpeg', 'application/zip'];

function answer(status, text) {
  return new Response(text, {
    status,
    headers: { 'content-type': 'text/plain; charset=utf-8' },
  });
}

function baseType(type) {
  return (type || '').split(';')[0].trim().toLowerCase();
}

export async function check(request) {
  // Gibt { version, files } zurück oder wirft { status, text }.
  const length = Number(request.headers.get('content-length') || '0');
  if (length > MAX_BYTES) throw { status: 413, text: 'zu gross' };
  if (!baseType(request.headers.get('content-type')).startsWith('multipart/form-data')) {
    throw { status: 415, text: 'multipart erwartet' };
  }
  let form;
  try {
    form = await request.formData();
  } catch (e) {
    throw { status: 400, text: 'unlesbar' };
  }
  const head = String(form.get('content') || '');
  const m = HEAD_RE.exec(head);
  if (!m) throw { status: 400, text: 'kein Verse-Kit-Bericht' };

  const files = [];
  let total = 0;
  for (let i = 0; i < MAX_FILES + 1; i++) {
    const f = form.get(`files[${i}]`);
    if (f === null) break;
    if (i >= MAX_FILES || typeof f === 'string') throw { status: 400, text: 'Dateien' };
    if (!ALLOWED_TYPES.includes(baseType(f.type))) throw { status: 415, text: 'Dateityp' };
    total += f.size;
    if (total > MAX_BYTES) throw { status: 413, text: 'zu gross' };
    files.push(f);
  }
  // Die erste Datei ist der Bericht selbst — Name und Anfang müssen passen.
  const report = files[0];
  if (!report || !REPORT_NAME_RE.test(report.name)) throw { status: 400, text: 'Berichtsdatei' };
  const start = await report.slice(0, 64).text();
  if (!start.startsWith(REPORT_START)) throw { status: 400, text: 'Berichtsinhalt' };
  return { version: m[1], files };
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);
    if (url.pathname !== '/bericht') return answer(404, 'nicht hier');
    if (request.method !== 'POST') return answer(405, 'nur POST');
    if (!env.DISCORD_WEBHOOK) return answer(503, 'nicht eingerichtet');

    // Menge begrenzen — je Absender-Adresse. Die Grenze selbst steht in
    // wrangler.toml (Rate-Limiting-Bindung). Fehlt sie, wird abgelehnt:
    // lieber kein Bericht als ein offenes Tor.
    const ip = request.headers.get('cf-connecting-ip') || 'unbekannt';
    if (!env.LIMIT) return answer(503, 'nicht eingerichtet');
    const { success } = await env.LIMIT.limit({ key: ip });
    if (!success) return answer(429, 'zu viele Berichte, bitte später');

    let checked;
    try {
      checked = await check(request);
    } catch (e) {
      if (e && e.status) return answer(e.status, e.text);
      return answer(400, 'unlesbar');
    }

    const out = new FormData();
    out.append('payload_json', JSON.stringify({
      content: `**Fehlerbericht** · ${checked.version}`,
      // ⚠ Keine Erwähnung darf jemanden anpingen — auch kein @everyone,
      // das jemand in einen Dateinamen oder Text schmuggelt.
      allowed_mentions: { parse: [] },
    }));
    checked.files.forEach((f, i) => out.append(`files[${i}]`, f, f.name));

    const sent = await fetch(env.DISCORD_WEBHOOK, { method: 'POST', body: out });
    // ⚠ 204 heißt „kein Inhalt" — ein Rumpf, auch ein leerer Text, ist dann
    // verboten und lässt den Worker selbst scheitern.
    if (sent.ok) return new Response(null, { status: 204 });
    // Die Webhook-Adresse steht NIE in einer Antwort.
    return answer(502, 'Discord nahm nicht an');
  },
};

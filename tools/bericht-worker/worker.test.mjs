// Prüfung des Workers mit Node (node --test). Kein Netz, keine echte
// Discord-Adresse: `fetch` wird untergeschoben und muss zuerst selbst
// zuschnappen (Vorbedingung), sonst wäre jedes Nicht-Weiterleiten geschenkt.
import test from 'node:test';
import assert from 'node:assert/strict';
import worker from './worker.js';

const HOOK = 'https://discord.com/api/webhooks/123456/geheimer-teil_ABC';  // privacy-ok: erfundene Adresse, nur fuer den Test

function report({ head = '**Fehlerbericht** · 3.57.1',
                  name = 'bericht-2026-09-26-1500.txt',
                  body = 'Verse-Kit 3.57.1 · Bericht vom 26.09.2026\n...',
                  extra = [] } = {}) {
  const form = new FormData();
  form.append('content', head);
  form.append('files[0]', new Blob([body], { type: 'text/plain; charset=utf-8' }), name);
  extra.forEach((f, i) => form.append(`files[${i + 1}]`, f.blob, f.name));
  return new Request('https://w.example/bericht', {
    method: 'POST', body: form, headers: { 'cf-connecting-ip': '203.0.113.7' },
  });
}

function env({ allow = true } = {}) {
  return { DISCORD_WEBHOOK: HOOK, LIMIT: { limit: async () => ({ success: allow }) } };
}

function trap() {
  const calls = [];
  globalThis.fetch = async (url, init) => { calls.push({ url, init }); return new Response(null, { status: 204 }); };
  return calls;
}

test('Vorbedingung: ein echter Bericht wird weitergeleitet', async () => {
  const calls = trap();
  const r = await worker.fetch(report(), env());
  assert.equal(r.status, 204);
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, HOOK);
});

test('Erwähnungen sind abgeschaltet, der Text kommt vom Worker', async () => {
  const calls = trap();
  await worker.fetch(report(), env());
  const payload = JSON.parse(calls[0].init.body.get('payload_json'));
  assert.deepEqual(payload.allowed_mentions, { parse: [] });
  assert.equal(payload.content, undefined);
});

test('Berichte mit „Verse-Kit " und mit „VerseKit " am Anfang werden angenommen', async () => {
  for (const body of ['Verse-Kit 3.57.1 · Bericht', 'VerseKit 5.0.0 · Bericht']) {
    const calls = trap();
    const r = await worker.fetch(report({ body }), env());
    assert.equal(r.status, 204, body);
    assert.equal(calls.length, 1, body);
  }
  // Andere Schreibweisen bleiben draußen.
  for (const body of ['Verse Kit 3.57.1', 'VerseKit5.0.0', 'verseKit 5.0.0', 'Verse-Kit']) {
    const calls = trap();
    const r = await worker.fetch(report({ body }), env());
    assert.equal(r.status, 400, body);
    assert.equal(calls.length, 0, body);
  }
});

test('die Discord-Nachricht ist ein Embed im Corporate Design', async () => {
  const calls = trap();
  await worker.fetch(report({ head: '**Fehlerbericht** · 5.0.0-rc2' }), env());
  const payload = JSON.parse(calls[0].init.body.get('payload_json'));
  assert.equal(payload.embeds.length, 1);
  const embed = payload.embeds[0];
  assert.equal(embed.color, 0x9ce430);
  assert.equal(embed.title, 'Fehlerbericht');
  assert.deepEqual(embed.fields, [{ name: 'Version', value: '5.0.0-rc2', inline: true }]);
  assert.equal(embed.footer.text, 'VerseKit');
  // Der Bericht bleibt eine Datei, Erwähnungen bleiben gesperrt.
  assert.equal(calls[0].init.body.get('files[0]').name, 'bericht-2026-09-26-1500.txt');
  assert.deepEqual(payload.allowed_mentions, { parse: [] });
});

test('@everyone im Kopf wird abgelehnt, nicht weitergeleitet', async () => {
  const calls = trap();
  const r = await worker.fetch(report({ head: '**Fehlerbericht** · 3.57.1 @everyone' }), env());
  assert.equal(r.status, 400);
  assert.equal(calls.length, 0);
});

test('fremder Inhalt wird abgelehnt', async () => {
  const calls = trap();
  const r = await worker.fetch(report({ body: 'Kauft billige Credits!' }), env());
  assert.equal(r.status, 400);
  assert.equal(calls.length, 0);
});

test('falscher Dateiname wird abgelehnt', async () => {
  const calls = trap();
  const r = await worker.fetch(report({ name: 'spam.txt' }), env());
  assert.equal(r.status, 400);
  assert.equal(calls.length, 0);
});

const PNG_HEAD = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13];
const JPEG_HEAD = [0xff, 0xd8, 0xff, 0xe0, 0, 16];
const png = (name = 'scan-1.png') => ({ name, blob: new Blob([new Uint8Array(PNG_HEAD)], { type: 'image/png' }) });
const jpeg = (name = 'foto.jpg') => ({ name, blob: new Blob([new Uint8Array(JPEG_HEAD)], { type: 'image/jpeg' }) });

test('Bilder als Anhang gehen mit', async () => {
  const calls = trap();
  const r = await worker.fetch(report({ extra: [png(), jpeg()] }), env());
  assert.equal(r.status, 204);
  assert.ok(calls[0].init.body.get('files[1]'));
  assert.ok(calls[0].init.body.get('files[2]'));
});

test('Bildnamen vergibt der Worker, nicht der Absender', async () => {
  const calls = trap();
  await worker.fetch(report({ extra: [png('@everyone C_Users_Name.png'), jpeg('x.exe')] }), env());
  assert.equal(calls[0].init.body.get('files[1]').name, 'bild-1.png');
  assert.equal(calls[0].init.body.get('files[2]').name, 'bild-2.jpg');
});

test('ein als PNG ausgegebenes Programm wird abgelehnt', async () => {
  const calls = trap();
  const fake = { name: 'bild-1.png', blob: new Blob(['MZ\x90\x00 kein Bild'], { type: 'image/png' }) };
  const r = await worker.fetch(report({ extra: [fake] }), env());
  assert.equal(r.status, 415);
  assert.equal(calls.length, 0);
});

test('ein als JPEG ausgegebener Text wird abgelehnt', async () => {
  const calls = trap();
  const fake = { name: 'bild-1.jpg', blob: new Blob(['hallo'], { type: 'image/jpeg' }) };
  const r = await worker.fetch(report({ extra: [fake] }), env());
  assert.equal(r.status, 415);
  assert.equal(calls.length, 0);
});

test('mehr als vier Bilder werden abgelehnt', async () => {
  const calls = trap();
  const four = await worker.fetch(report({ extra: [png(), png(), jpeg(), jpeg()] }), env());
  assert.equal(four.status, 204);
  const five = await worker.fetch(report({ extra: [png(), png(), jpeg(), jpeg(), png()] }), env());
  assert.equal(five.status, 400);
  assert.equal(calls.length, 1);
});

test('Scan-ZIP älterer Fassungen geht weiter mit, falsche ZIP nicht', async () => {
  const calls = trap();
  const zip = { name: 'scan-bilder.zip', blob: new Blob([new Uint8Array([0x50, 0x4b, 0x03, 0x04, 20, 0])], { type: 'application/zip' }) };
  const ok = await worker.fetch(report({ extra: [zip] }), env());
  assert.equal(ok.status, 204);
  assert.equal(calls[0].init.body.get('files[1]').name, 'scan-bilder.zip');
  const bad = { name: 'scan-bilder.zip', blob: new Blob(['MZ'], { type: 'application/zip' }) };
  const no = await worker.fetch(report({ extra: [bad] }), env());
  assert.equal(no.status, 415);
  assert.equal(calls.length, 1);
});

test('Namen weiterer Anhänge vergibt der Worker, die Scan-ZIP behält ihren Melder', async () => {
  const calls = trap();
  const zipBytes = () => new Blob([new Uint8Array([0x50, 0x4b, 0x03, 0x04, 20, 0])], { type: 'application/zip' });
  const named = { name: 'scan-bilder-Jörg_42.zip', blob: zipBytes() };
  const odd = { name: '@everyone Gratis-Nitro.zip', blob: zipBytes() };
  const text = { name: 'C_Users_Name passwoerter.txt', blob: new Blob(['x'], { type: 'text/plain' }) };
  const r = await worker.fetch(report({ extra: [named, odd, text] }), env());
  assert.equal(r.status, 204);
  const body = calls[0].init.body;
  assert.equal(body.get('files[1]').name, 'scan-bilder-Jörg_42.zip');
  assert.equal(body.get('files[2]').name, 'scan-bilder.zip');
  assert.equal(body.get('files[3]').name, 'anhang-3.txt');
});

test('ausführbare Anhänge werden abgelehnt', async () => {
  const calls = trap();
  const exe = { name: 'x.exe', blob: new Blob(['MZ'], { type: 'application/x-msdownload' }) };
  const r = await worker.fetch(report({ extra: [exe] }), env());
  assert.equal(r.status, 415);
  assert.equal(calls.length, 0);
});

test('zu viele Anfragen: 429, nichts weitergeleitet', async () => {
  const calls = trap();
  const r = await worker.fetch(report(), env({ allow: false }));
  assert.equal(r.status, 429);
  assert.equal(calls.length, 0);
});

test('ohne Mengenbremse bleibt das Tor zu', async () => {
  const calls = trap();
  const r = await worker.fetch(report(), { DISCORD_WEBHOOK: HOOK });
  assert.equal(r.status, 503);
  assert.equal(calls.length, 0);
});

test('Antworten nennen die Webhook-Adresse nie', async () => {
  globalThis.fetch = async () => new Response('kaputt', { status: 500 });
  const r = await worker.fetch(report(), env());
  const text = await r.text();
  assert.equal(r.status, 502);
  assert.ok(!text.includes('geheimer-teil'));
});

test('andere Pfade und GET: nichts', async () => {
  const calls = trap();
  const a = await worker.fetch(new Request('https://w.example/'), env());
  const b = await worker.fetch(new Request('https://w.example/bericht'), env());
  assert.equal(a.status, 404);
  assert.equal(b.status, 405);
  assert.equal(calls.length, 0);
});

test('kaputtes Geheimnis (Steuerzeichen) stürzt nicht ab, sondern meldet 503', async () => {
  const calls = trap();
  const r = await worker.fetch(report(), { DISCORD_WEBHOOK: '',
    LIMIT: { limit: async () => ({ success: true }) } });
  assert.equal(r.status, 503);
  assert.equal(calls.length, 0);
});

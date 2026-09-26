// Prüfung des Workers mit Node (node --test). Kein Netz, keine echte
// Discord-Adresse: `fetch` wird untergeschoben und muss zuerst selbst
// zuschnappen (Vorbedingung), sonst wäre jedes „nicht weitergeleitet" geschenkt.
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
  assert.equal(payload.content, '**Fehlerbericht** · 3.57.1');
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

test('Scan-Bilder als Anhang gehen mit', async () => {
  const calls = trap();
  const png = { name: 'scan-1.png', blob: new Blob([new Uint8Array([137, 80, 78, 71])], { type: 'image/png' }) };
  const r = await worker.fetch(report({ extra: [png] }), env());
  assert.equal(r.status, 204);
  assert.ok(calls[0].init.body.get('files[1]'));
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

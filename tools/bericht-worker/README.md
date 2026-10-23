# Bericht-Weiterleitung (Cloudflare Worker)

VerseKit schickt Fehlerberichte hierhin, nicht mehr direkt an Discord. Der
Worker prüft, ob es ein echter VerseKit-Bericht ist, bremst die Menge je
Absender und leitet ihn an den Discord-Kanal weiter — **ohne dass irgendjemand
angepingt werden kann**. Die Discord-Adresse liegt nur hier als Geheimnis, nie
im Programm.

## Was durchgeht

| Prüfung | Grenze |
|---|---|
| Pfad und Methode | nur `POST /bericht` |
| Größe | höchstens 8 MB, höchstens 10 Dateien |
| Kopfzeile | genau `**Fehlerbericht** · <Version>` |
| erste Datei | `bericht-JJJJ-MM-TT-HHMM.txt`, beginnt mit `VerseKit ` (v5) **oder** `Verse-Kit ` (ältere Programme) |
| Anhänge | nur Text, PNG, JPEG, ZIP — PNG, JPEG und ZIP auch am Inhalt (erste Bytes) geprüft |
| Bilder | höchstens 4; in Discord heißen sie `bild-1.png`, `bild-2.jpg` …, nie wie beim Absender |
| Menge | 3 je Absender und Minute |
| Erwähnungen | alle abgeschaltet (`allowed_mentions: {parse: []}`) |

Die Nachricht in Discord baut der Worker selbst — vom Absender kommen nur die
geprüfte Versionsnummer und die Dateien. Sie ist ein Embed im Corporate Design:

```json
{
  "embeds": [{
    "title": "Fehlerbericht",
    "color": 10282032,
    "fields": [{ "name": "Version", "value": "5.0.0-rc2", "inline": true }],
    "footer": { "text": "VerseKit" }
  }],
  "allowed_mentions": { "parse": [] }
}
```

Farbe `0x9ce430` (Neon), Titel `Fehlerbericht`, Fußzeile `VerseKit`. Ein
System-Feld gibt es nicht: Der Worker kennt nur die Versionsnummer aus der
Kopfzeile, das System steht frei formuliert im Berichtstext (Datei). Die
Berichtsdatei bleibt ein Anhang.

**Reihenfolge beim Umstellen:** erst diesen Worker einspielen, dann das
Programm mit dem neuen Namen veröffentlichen — der Worker nimmt beide
Schreibweisen an.

## Einrichten (einmalig)

1. Cloudflare-Konto anlegen, unter *Workers & Pages* eine Subdomain wählen.
2. In Discord im Berichtskanal einen **neuen** Webhook anlegen und die Adresse
   kopieren.
3. Hier im Ordner:

   ```
   npx wrangler login
   npx wrangler deploy
   npx wrangler secret put DISCORD_WEBHOOK
   ```

   Beim dritten Befehl die Webhook-Adresse einfügen. Sie landet nur bei
   Cloudflare, in keiner Datei.
4. Die Adresse des Workers (`https://versekit-bericht.<subdomain>.workers.dev/bericht`)
   in `scbp/report_target.py` eintragen.

## Webhook tauschen (bei Missbrauch)

Webhook in Discord löschen, neuen anlegen, dann nur:

```
npx wrangler secret put DISCORD_WEBHOOK
```

Keine neue VerseKit-Version nötig.

## Prüfen

```
node --test worker.test.mjs
```

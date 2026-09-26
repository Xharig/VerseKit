# Bericht-Weiterleitung (Cloudflare Worker)

Verse-Kit schickt Fehlerberichte hierhin, nicht mehr direkt an Discord. Der
Worker prüft, ob es ein echter Verse-Kit-Bericht ist, bremst die Menge je
Absender und leitet ihn an den Discord-Kanal weiter — **ohne dass irgendjemand
angepingt werden kann**. Die Discord-Adresse liegt nur hier als Geheimnis, nie
im Programm.

## Was durchgeht

| Prüfung | Grenze |
|---|---|
| Pfad und Methode | nur `POST /bericht` |
| Größe | höchstens 8 MB, höchstens 10 Dateien |
| Kopfzeile | genau `**Fehlerbericht** · <Version>` |
| erste Datei | `bericht-JJJJ-MM-TT-HHMM.txt`, beginnt mit `Verse-Kit ` |
| Anhänge | nur Text, PNG, JPEG, ZIP |
| Menge | 3 je Absender und Minute |
| Erwähnungen | alle abgeschaltet (`allowed_mentions: {parse: []}`) |

Die Nachricht in Discord baut der Worker selbst — vom Absender kommen nur die
geprüfte Versionsnummer und die Dateien.

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

Keine neue Verse-Kit-Version nötig.

## Prüfen

```
node --test worker.test.mjs
```

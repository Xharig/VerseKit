# Nutzungszähler und Statistik (Cloudflare Worker)

Verse-Kit meldet sich höchstens einmal am Tag hier — ohne Kennung. Der Worker
zählt je Tag, wie viele Installationen sich gemeldet haben, und zeigt das in
einer privaten Übersicht. Was genau gemeldet wird, steht in `PRIVACY.md`
(Abschnitt *Nutzung zählen*) und in `scbp/usage_ping.py`.

| Adresse | Wofür | Wer kommt ran |
|---|---|---|
| `nutzung-versekit.xharig.com/ping` | nimmt die tägliche Meldung an | jeder Verse-Kit (nur `POST`, nur geprüfte Felder) |
| `statistik-versekit.xharig.com` | die Übersicht mit den Charts | nur du — Cloudflare Access davor, der Worker prüft zusätzlich selbst |

## Was durchgeht

| Prüfung | Grenze |
|---|---|
| Pfad und Methode | nur `POST /ping` |
| Größe | höchstens 600 Byte |
| Felder | nur `v`, `os`, `ui`, `game`, `rc`, `mods`, `overlay`, `autostart` — jedes andere Feld: abgelehnt |
| Werte | Version `1.2.3(-rcN)`, System `windows`/`linux`, Sprachen zwei Kleinbuchstaben, Ja/Nein als echte Wahrheitswerte, Bereiche nur Kleinbuchstaben (höchstens 12) |
| Menge | 5 je Absender und Minute |
| gespeichert | nur Zähler je Tag — keine IP, keine Stadt; das Land ist Cloudflares Kürzel |

## Übersicht absichern — zwei Schlösser

1. **Cloudflare Access** vor `statistik-versekit.xharig.com`: Ohne Anmeldung über
   GitHub erreicht keine Anfrage den Worker.
2. **Der Worker prüft das Access-Zeichen selbst** (`verifyAccess`):
   Unterschrift, Zielgruppe (AUD), Aussteller, Ablauf und die erlaubte
   Mail-Adresse. Fehlt eine Einstellung, bleibt die Tür zu. Die Übersicht gibt
   es **nur** unter `statistik-versekit.xharig.com` — nicht über `workers.dev`, nicht
   über `nutzung-versekit.xharig.com`.

Die Seite lädt nichts von außen; eine strenge Content-Security-Policy lässt
nur das eigene Skript dieser Antwort zu.

## Einrichten (einmalig)

Alle Befehle in diesem Ordner (`tools/nutzung-worker`) in einem Terminal.

1. **Anmelden:** `npx wrangler login` — der Browser öffnet sich, bei
   Cloudflare bestätigen.
2. **Datenbank anlegen:** `npx wrangler d1 create versekit-nutzung` — die
   ausgegebene `database_id` in `wrangler.toml` bei `database_id` eintragen.
3. **Tabellen anlegen:**
   `npx wrangler d1 execute versekit-nutzung --remote --file=schema.sql`
4. **Worker hochladen:** `npx wrangler deploy` — legt auch die beiden
   Adressen `nutzung-versekit.xharig.com` und `statistik-versekit.xharig.com` an.
5. **GitHub als Anmeldung vorbereiten** (github.com):
   Profilbild oben rechts → *Settings* → ganz unten links *Developer settings*
   → *OAuth Apps* → *New OAuth App*.
   - *Application name*: `VerseKit-Statistik`
   - *Homepage URL*: `https://statistik-versekit.xharig.com`
   - *Authorization callback URL*: `https://<team>.cloudflareaccess.com/cdn-cgi/access/callback`
     (`<team>` legst du in Schritt 6 fest — erst Schritt 6 bis zum Teamnamen
     machen, dann hier eintragen)
   - *Register application* → *Client ID* kopieren → *Generate a new client
     secret* → kopieren und sicher ablegen (GitHub zeigt es nur einmal).
6. **Cloudflare Zero Trust** (dash.cloudflare.com → links *Zero Trust*):
   - Beim ersten Mal einen **Teamnamen** wählen, z. B. `xharig` → ergibt
     `xharig.cloudflareaccess.com`. Tarif **Free** (bis 50 Nutzer).
   - *Settings* → *Authentication* → *Login methods* → *Add new* → **GitHub**
     → Client ID und Client secret aus Schritt 5 einfügen → *Save* → *Test*.
   - *Access* → *Applications* → *Add an application* → **Self-hosted**:
     Name `VerseKit-Statistik`, Domain `statistik-versekit.xharig.com`, Sitzungsdauer
     z. B. 24 Stunden. Unter *Identity providers* nur **GitHub** anhaken.
   - Richtlinie: Name `VerseKit-Statistik-nur-ich`, Action **Allow**,
     Include → **Emails** → deine GitHub-Mail-Adresse.
   - Speichern, dann in der Anwendung unter *Overview* die
     **Application Audience (AUD) Tag** kopieren.
7. **Die drei Angaben als Geheimnisse setzen** (landen nur bei Cloudflare):

   ```
   npx wrangler secret put TEAM_DOMAIN
   npx wrangler secret put POLICY_AUD
   npx wrangler secret put ERLAUBTE_MAIL
   ```

   Eingaben: `<team>.cloudflareaccess.com`, die AUD-Kennung, die Mail-Adresse
   aus Schritt 6.
8. **Abmelden:** `npx wrangler logout`.
9. **Prüfen:** `https://statistik-versekit.xharig.com` öffnen → GitHub-Anmeldung →
   Übersicht. In einem privaten Fenster ohne Anmeldung muss die
   Access-Anmeldeseite kommen, nie die Zahlen.

## Prüfen (Code)

```
node --test worker.test.mjs
```

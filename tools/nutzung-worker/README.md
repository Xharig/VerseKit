# Nutzungszähler und Statistik (Cloudflare Worker)

Verse-Kit meldet sich höchstens einmal am Tag hier — ohne Kennung. Der Worker
zählt je Tag, wie viele Installationen sich gemeldet haben, und zeigt das in
einer privaten Übersicht. Was genau gemeldet wird, steht in `PRIVACY.md`
(Abschnitt *Nutzung zählen*) und in `scbp/usage_ping.py`.

| Adresse | Wofür | Wer kommt ran |
|---|---|---|
| `nutzung-versekit.xharig.com/ping` | nimmt die tägliche Meldung an | jeder Verse-Kit (nur `POST`, nur geprüfte Felder) |
| `statistik-versekit.xharig.com` | die Übersicht mit den Charts | nur du — Cloudflare Access davor, der Worker prüft zusätzlich selbst |
| `xharig.com/windows`, `xharig.com/linux` | Download-Knöpfe der Webseite: zählen Tag, System, Land, dann Weiterleitung zu GitHub | jeder; Vorschau-Roboter, `curl` und schnelle Wiederholungen zählen nicht |

⚠ Für die Kurzlinks dürfen in Cloudflare **keine** Weiterleitungsregeln mit
denselben Pfaden mehr stehen — Weiterleitungsregeln laufen vor Workern.

## Was durchgeht

| Prüfung | Grenze |
|---|---|
| Pfad und Methode | nur `POST /ping` |
| Größe | höchstens 16 KB (`MAX_BYTES`) — die größte Meldung, die das Programm bauen kann, braucht rund 12 KB |
| Felder | nur `v`, `os`, `ui`, `game`, `rc`, `mods`, `overlay`, `autostart`, `update`, `pages`, `entry`, `clicks`, `misses` — jedes andere Feld: abgelehnt, ebenso die ganze Meldung |
| Werte | Version `1.2.3(-rcN)`, System `windows`/`linux`, Sprachen zwei Kleinbuchstaben, Ja/Nein als echte Wahrheitswerte (`rc`, `autostart`, `update`), Bereiche nur Kleinbuchstaben (höchstens 12) |
| Seitenzähler | nur Seiten-Kennungen aus `pages.js`, Wege nur aus `ROUTES`, Klickstufen 0–10, Fehlgriff-Paare `a>b` mit zwei verschiedenen bekannten Seiten (höchstens 20), jede Zahl eine ganze Zahl von 1 bis 9999 |
| Menge | 5 je Absender und Minute |
| gespeichert | nur Zähler je Tag — keine IP, keine Stadt; das Land ist Cloudflares Kürzel. Die Seitenzähler stehen in eigenen Tabellen, **nicht** verknüpft mit Version, Land oder System |

### Die Felder der Meldung

| Feld | Inhalt | Tabelle |
|---|---|---|
| `v`, `os` | Programmversion, System | `tage` (mit Land) |
| `ui`, `game` | Sprache der Oberfläche und des Spiels | `merkmale` |
| `rc`, `autostart`, `update` | Testversionen, Autostart, Auto-Update (ja/nein) | `merkmale` |
| `mods`, `overlay` | eingeschaltete Bereiche, Overlay-Betrieb | `merkmale` |
| `pages` | Aufrufe je Seite, `{"liste": 12}` | `seiten` |
| `entry` | Weg je Seite, `{"laeden": {"seitenleiste": 3}}` | `seiten_wege` |
| `clicks` | Klicks bis zur Zielseite als Verteilung, `{"laeden": {"2": 5}}` (10 = 10 oder mehr) | `seiten_klicks` |
| `misses` | Fehlgriff → nächstes Ziel, `{"verkauf>laeden": 7}` | `seiten_fehlgriffe` |

Was „Klick", „Ziel" und „Fehlgriff" heißen, steht in `scbp/page_usage.py`.
Kommt eine neue Seite ins Programm, gehört sie in `pages.js` — der Selbsttest
(Prüfung 315) meldet jede Abweichung.

## Seiten-Tabellen einspielen (einmalig, vor dem Hochladen)

Die vier Seiten-Tabellen kommen über `schema.sql` dazu. Die Datei legt nur an,
was fehlt (`CREATE TABLE IF NOT EXISTS`), vorhandene Tabellen und Zahlen bleiben
unberührt. **Erst einspielen, dann den Worker hochladen:**

```
npx wrangler d1 execute versekit-nutzung --remote --file=schema.sql
npx wrangler deploy
```

Fehlen die Tabellen doch, zählt der Worker die Installation trotzdem; nur die
Seitenzähler dieser Meldung gehen verloren (Grund in `ablage`, Schlüssel
`fehler_seiten`).

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

## Mitschreiben und Sicherung

- **Alle 6 Stunden** (Zeitplan in `wrangler.toml`) hält der Worker je Version
  den **höchsten je gesehenen** Download-Stand fest (`download_bestand`) und
  schreibt eine Tageszeile (`download_verlauf`). Gelöschte Releases bleiben so
  erhalten; daraus entstehen „Downloads gesamt (mit gelöschten)" und
  „Downloads je Tag".
- **Sicherung außerhalb von Cloudflare:** `GET statistik-versekit.xharig.com/export`
  liefert alle Tabellen, auch die vier Seiten-Tabellen. Erlaubt ist das dem Eigentümer **und** einem
  Cloudflare-Access-Dienst-Zeichen, dessen Client-ID als Geheimnis
  `SERVICE_ID` hinterlegt ist — dieses Zeichen darf **nur** `/export`, nicht
  die Übersicht. Abgeholt wird es von einem Skript außerhalb dieses Repos.

Einrichten des Dienst-Zeichens (Cloudflare Zero Trust):
1. *Zugriffssteuerungen* → *Dienstanmeldeinformationen* → *Diensttoken
   erstellen*, Name z. B. `VerseKit-Statistik-Sicherung`, Dauer nach Wahl.
   Client-ID und Client-Secret sicher ablegen — das Secret erscheint nur einmal.
2. In der Access-Anwendung der Übersicht eine zweite Richtlinie: Aktion
   **Service Auth**, Einschließen → **Service Token** → das eben erstellte.
3. `npx wrangler secret put SERVICE_ID` → die **Client-ID** (nicht das Secret).

## Prüfen (Code)

```
node --test worker.test.mjs
```

**Deutsch** · [English](/tools/orga-standard/README.en.md)

# VerseKit-Orga-Standard (versekit-org), Version 1

Mit diesem offenen Standard bindet jede Star-Citizen-Orga ihr eigenes Webpanel an VerseKit an, ohne dass VerseKit dafür geändert werden muss. Ein Mitglied trägt in VerseKit unter **Einstellungen › Orga verbinden** die Adresse der Orga ein, bestätigt einen Code im Panel der Orga, fertig.

Die genaue Beschreibung steht in [`v1/openapi.yaml`](/tools/orga-standard/v1/openapi.yaml). Ein Vorlage-Server zum Selbsthosten liegt in [`crates/versekit-orgserver`](/crates/versekit-orgserver).

## Was die Orga bereitstellt

1. **Discovery** unter `https://<eure-adresse>/.well-known/versekit-org.json`: Name, Kürzel und die Adressen der Endpunkte, je Hauptversion ein Eintrag unter `versions`.
2. **Anmeldung per Code** nach RFC 8628 (Device Authorization Grant): VerseKit holt einen Code, zeigt ihn an und öffnet eure Bestätigungsseite. Das Mitglied meldet sich dort an (Discord, euer Login oder ein Schlüssel, ganz nach eurer Wahl), sieht die angefragten Rechte und bestätigt.
3. **Tokens**: Zugangstoken (höchstens eine Stunde), Erneuerungstoken mit Wechsel bei jeder Erneuerung, Widerruf nach RFC 7009.
4. **API** mit `Authorization: Bearer …` unter `api_base`.

## Welche Daten fließen

| Recht | VerseKit darf |
|---|---|
| `stock:read` | den Bestand lesen, den die Mitglieder teilen (`GET /stock`) |
| `stock:write` | den eigenen geteilten Bestand ersetzen (`PUT /stock/mine`) |
| `blueprints:read` | lesen, wer welche Baupläne hat (`GET /blueprints`) |
| `blueprints:write` | die eigenen Baupläne ersetzen (`PUT /blueprints/mine`) |
| `orders:read` | die Aufträge der Orga lesen (`GET /orders`) |
| `orders:write` | Lieferungen zu offenen Aufträgen buchen (`POST /orders/{id}/deliveries`) |

Das Mitglied entscheidet auf eurer Bestätigungsseite, welche Rechte es gibt. `GET /me` braucht kein Recht. Alles andere bleibt auf dem Rechner des Mitglieds.

## Regeln für eure Seite

- Nur HTTPS. Alle Adressen in der Discovery liegen auf dem Host, den das Mitglied eintippt, oder darunter (`api.orga.example.org` zu `orga.example.org`). Andere Hosts lehnt VerseKit ab.
- Tokens, Gerätecodes und Schlüssel nur als Hash speichern. Codes laufen nach höchstens zehn Minuten ab.
- Die Bestätigungsseite zeigt Orga, Mitglied und Rechte und lässt Rechte abwählen. VerseKit öffnet nur die nackte Adresse, nie einen Link mit eingebautem Code.
- Bei zu vielen Anfragen `429` mit `Retry-After`; Wartung `503` mit `Retry-After`.
- Fehler der API als `application/problem+json` mit `code` (siehe OpenAPI).
- Listen dürfen seitenweise kommen: `next_cursor` in der Antwort, `?cursor=` in der nächsten Anfrage.
- `PUT …/mine` ersetzt jeweils die ganze Liste; Lieferungen tragen einen `Idempotency-Key`, eine Wiederholung bucht nichts doppelt.

## Versionen

- Version 1 wächst nur verträglich: neue optionale Felder, neue Endpunkte, neue Rechte. Unbekannte Felder werden auf beiden Seiten übergangen.
- Was ältere VerseKit-Fassungen brechen würde, wird Version 2 mit eigener `api_base`. Während des Wechsels nennt die Discovery beide Versionen; VerseKit nimmt die neueste, die es kennt.

## Eigenes Panel anbinden

Zwei Wege, ein Standard: Orgas mit eigener Datenbank und eigenem Panel bauen die Schnittstelle oben in ihr Panel ein, Orgas ohne eigene Lösung nehmen den mitgelieferten Vorlage-Server. VerseKit nutzt nur, was in [`v1/openapi.yaml`](v1/openapi.yaml) und im Discovery-Dokument steht, nie etwas, das nur der Vorlage-Server kann. Seine Verwaltung unter `/admin` gehört nicht zum Standard.

Ihr müsst nicht alles anbieten. Ein Panel, das nur den Bestand führt (je Mitglied und Ort, mit Menge und Qualität), nennt in `scopes_supported` nur `stock:read` und, wenn Mitglieder ihren Bestand aus VerseKit teilen sollen, `stock:write`. VerseKit fragt nur nach angebotenen Rechten; Baupläne und Aufträge kommen später einfach als weitere Rechte dazu. Rechte, die VerseKit nicht kennt, übergeht es.

So prüft ihr euer Panel:

1. Eine Testumgebung mit einem Testmitglied aufsetzen; nie gegen echte Orga-Daten.
2. Das Prüfwerkzeug aus dem Repo starten: `cargo run --release -p versekit-orgserver --bin versekit-orgcheck -- https://orga.example.org` (Hilfe mit `--help`).
3. Zweimal erscheint ein Code: auf eurer Bestätigungsseite mit dem Testmitglied freigeben, beim ersten Mal das eine angefragte Recht, beim zweiten Mal alle.
4. Das Werkzeug prüft Discovery, die Anmeldung per Code, jede Operation mit ihren Fehlercodes, die Grenzen der Rechte, `429` mit `Retry-After`, das Erneuern mit Wechsel und den Widerruf. Es endet mit einer Liste „grün / ROT — Grund“ und dem Code 0, wenn nichts rot ist.

Die schreibenden Prüfungen teilen kurz einen Testeintrag als eigenen Bestand und eigene Baupläne des Testmitglieds und nehmen ihn wieder zurück; `--no-write` lässt sie aus. `--burst N` legt fest, wie viele schnelle Anfragen die Bremse auslösen sollen (Standard 300, `0` lässt die Prüfung aus). `503` lässt sich von außen nicht auslösen und bleibt „übersprungen“.

## Vorlage-Server

`crates/versekit-orgserver` erfüllt den Standard und ist klein genug zum Lesen: Mitglieder mit persönlichem Schlüssel in `config.json`, Zustand in `state.json`, Bestätigungsseite unter `/device`. Er spricht nur HTTP; HTTPS übernimmt ein Reverse-Proxy (Caddy, Traefik, nginx) davor.

Der Server hat eine eigene Versionsreihe ab 1.0.0, getrennt von der App und vom Standard; Änderungen stehen in [`CHANGELOG-orgserver.md`](/crates/versekit-orgserver/CHANGELOG-orgserver.md). Das fertige Image liegt ab Orga-Server 1.1.0 unter `ghcr.io/xharig/versekit-orgserver` (amd64 und arm64, Tags `:1.1.0`, `:1`, `:latest`), siehe nächster Abschnitt. Selbst bauen: `docker build -f crates/versekit-orgserver/Dockerfile -t versekit-orgserver .` im Wurzelordner des Repos, dann in der Compose-Datei `image: versekit-orgserver` eintragen. Ohne Docker: `cargo run --release -p versekit-orgserver -- --config config.json --state state.json --listen 127.0.0.1:8080`.

Umgebungsvariablen gelten vor `config.json`: `VERSEKIT_ORG_PUBLIC_URL`, `VERSEKIT_ORG_NAME`, `VERSEKIT_ORG_TAG`, `VERSEKIT_ORG_RATE_LIMIT` (Anfragen je Minute und Anmeldung eines Mitglieds vor `429`, Standard 120), `VERSEKIT_ORG_POLL_INTERVAL` (Sekunden zwischen zwei Nachfragen während der Bestätigung, Standard 5). Dazu `VERSEKIT_ORG_CONFIG`, `VERSEKIT_ORG_STATE` und `VERSEKIT_ORG_LISTEN` für die Orte.

## In 5 Minuten selbst hosten

Ihr braucht einen Rechner mit Docker (Server, NAS, Raspberry Pi; amd64 oder arm64) und eine Domain, die auf ihn zeigt.

1. **Dateien holen:** [`docker-compose.yml`](/tools/orga-standard/docker-compose.yml) und [`config.example.json`](/crates/versekit-orgserver/config.example.json) in einen leeren Ordner legen, die zweite in `config.json` umbenennen.
2. **Eintragen:** In `config.json` je Mitglied eine `id`, einen Namen und einen eigenen Schlüssel aus mindestens 16 zufälligen Zeichen. In `docker-compose.yml` eure Adresse, Name und Kürzel.
3. **Starten:** `docker compose up -d`. Der Server lauscht nur auf `127.0.0.1:8080`. Der Zustand liegt im Volume `orga-daten` und bleibt bei Neustart und Update erhalten.
4. **HTTPS davor**, am einfachsten mit Caddy, das sich das Zertifikat selbst holt. Die ganze `Caddyfile`:
   ```
   orga.example.org {
       reverse_proxy 127.0.0.1:8080
   }
   ```
   Mit Traefik oder nginx genauso: alles an `127.0.0.1:8080` weiterreichen.
5. **Prüfen:** `https://orga.example.org/.well-known/versekit-org.json` zeigt Name und Kürzel. Dann in VerseKit unter **Einstellungen › Orga verbinden** die Adresse eintragen und den Code auf `https://orga.example.org/device` mit dem eigenen Schlüssel bestätigen. VerseKit zeigt „Verbunden mit … als …“.

Update: `docker compose pull && docker compose up -d`. Die Vorlagen nutzen `:1` (jedes Update der Reihe 1, nie ein brechendes 2); eine feste Version wie `:1.1.0` wechselt nur, wenn ihr sie in der Compose-Datei ändert. Ohne eigenen Reverse-Proxy nehmt ihr [`docker-compose.caddy.yml`](docker-compose.caddy.yml), die Caddy mitbringt. Die ausführliche Anleitung für Unerfahrene, mit NAS, Unraid, Sicherung, Aktuell halten und Fehlerhilfe: [selbst-hosten.md](selbst-hosten.md).

## Verwaltung

Unter `/admin` verwaltet die Orga-Leitung im Browser ihren Server: verbundene Mitglieder mit ihren Rechten und dem letzten Kontakt (Verbindung trennen per Knopf), Bestand, Baupläne und Lieferungen zum Durchsuchen, dazu Aufträge anlegen, bearbeiten und schließen. Am Auftrag stehen die gebuchten Lieferungen mit dem Fortschritt.

1. **Passwort:** Beim ersten Start erzeugt der Server ein starkes Passwort und legt es nur für den Besitzer lesbar in den Datenordner (`admin-password.txt`). Das Log nennt einmal, wo es liegt, nie das Passwort selbst. Anzeigen: `docker compose exec orgserver versekit-orgserver --admin-password`. Wer ein eigenes setzen will, legt neben die Compose-Datei eine Datei `.env` mit `VERSEKIT_ORG_ADMIN_PASSWORD=` und mindestens 12 Zeichen, oder nennt mit `VERSEKIT_ORG_ADMIN_PASSWORD_FILE` eine Datei (etwa ein Docker-Secret). Im Image steht es nie.
2. **Ausschalten (nur mit Absicht):** `VERSEKIT_ORG_ADMIN=off`. Dann sagt `/admin`, dass die Verwaltung aus ist, und das Log warnt beim Start.
3. **Anmelden:** `https://orga.example.org/admin` öffnen und das Passwort eingeben. Die Anmeldung gilt acht Stunden oder bis „Abmelden“.

Die Karte „Einrichtung“ der Verwaltung zeigt die Selbstprüfung beim Start: Passwort, HTTPS, Rechte auf den Datenordner, Uhrzeit und Version. Ist dort etwas rot, sagt die öffentliche Statusseite nur „Einrichtung unvollständig“, ohne Einzelheiten.

Schutz: Das Passwort liegt im Server nur als PBKDF2-Hash vor. Die Sitzung steckt in einem Cookie mit `HttpOnly`, `SameSite=Strict` und unter HTTPS `Secure`. Jedes Formular trägt ein CSRF-Token und muss von der eigenen Adresse kommen. Nach fünf falschen Passwörtern wartet die Anmeldung eine Minute, danach jedes Mal doppelt so lange, höchstens 15 Minuten. Die Statusseite unter `/` zeigt nichts aus der Verwaltung.

Mitglieder und ihre Schlüssel stehen weiter in `config.json`. Wer dort fehlt, kann sich nicht anmelden; „Verbindung trennen“ beendet nur die laufende Anmeldung.

## Mitglieder-Portal (ab 1.1.0)

Unter `/portal` melden sich Mitglieder des Discord-Servers der Orga mit Discord an und sehen dieselben Seiten wie die Verwaltung, so weit ihre Rolle reicht: Mitglied (ansehen), Logistik (dazu Aufträge und Lieferungen bestätigen), Leitung (alles, dazu Zugang und Einstellungen). Die Rollen kommen aus den Discord-Rollen oder von Hand. Ohne die drei Discord-Einstellungen ist das Portal aus; das Admin-Passwort unter `/admin` bleibt der Notzugang. Das Portal gehört nicht zum Standard. Einrichtung Schritt für Schritt: [selbst-hosten.md](selbst-hosten.md#mitglieder-portal-mit-discord-ab-110).

## Testen

Richtet eine Testumgebung mit Testkonten ein und tragt deren Adresse in VerseKit ein. Gegen echte Orga-Daten wird nicht entwickelt. Die Tests von VerseKit laufen gegen den Vorlage-Server (`crates/versekit-core/tests/orga_standard.rs`) und zeigen den ganzen Ablauf.

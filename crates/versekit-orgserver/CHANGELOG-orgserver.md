# Änderungen am VerseKit-Orga-Server

Der Orga-Server hat eine eigene Versionsreihe, getrennt von der App (5.x) und vom Standard (`versekit-org v1`). Git-Tags heißen `orgserver-v<version>`, das Image liegt unter `ghcr.io/xharig/versekit-orgserver`. English: [CHANGELOG-orgserver.en.md](CHANGELOG-orgserver.en.md).

## 1.1.0

Mitglieder-Portal mit Discord-Anmeldung. Erste Version, die als fertiges Image erscheint; sie enthält alles aus 1.0.0.

- Mitglieder-Portal unter `/portal`: Mitglieder des Discord-Servers der Orga melden sich mit Discord an und sehen Bestand, Baupläne, Aufträge und Lieferungen im Browser.
- Rollen Mitglied, Logistik und Leitung: aus den Discord-Rollen, die die Leitung unter **Einstellungen** zuordnet, oder von Hand unter **Zugang**. „Gesperrt“ hält jemanden draußen und wirkt sofort. Ohne Rolle wartet eine Anmeldung auf die Leitung.
- Logistik legt Aufträge an, bearbeitet und schließt sie und bestätigt Lieferungen; die Lieferungen zeigen, wer sie bestätigt hat.
- Das Admin-Passwort unter `/admin` bleibt der Notzugang für die Einrichtung und wenn Discord klemmt.
- Anmeldung der Verwaltung: Das Formular sagt, dass es 10 Minuten gilt; ein abgelaufenes zeigt einfach ein frisches statt einer Fehlermeldung. Ein verstecktes Benutzerfeld hilft Passwort-Managern, das Passwort zuzuordnen. Hält ein Browser das Formular-Cookie zurück, gilt das Formular trotzdem, wenn der Browser selbst bestätigt, dass es von dieser Seite kommt (einmalig, 10 Minuten). Kommt es von woanders, nennt die Seite Herkunft und Adresse. Getippter Text bleibt in allen Feldern sichtbar, auch bei Vorbelegung durch einen Passwort-Manager.
- Anmeldung mit Firefox und anderen Browsern über eine http-Adresse im Heimnetz: Die Seiten senden jetzt `Referrer-Policy: same-origin` statt `no-referrer`, sodass die Browser beim Absenden die eigene Herkunft nennen statt `null`. Schickt ein Browser trotzdem `Origin: null`, gilt das Formular, wenn es das Token trägt, das der Server ausgegeben hat, oder das dazu gehörende Cookie; wer sich als fremde Seite ausgibt (`Sec-Fetch-Site: cross-site` oder `same-site`) oder ein fremder Ursprung bleiben abgelehnt. Gilt auch für die Formulare nach der Anmeldung. Das Passwortfeld nimmt für die Punkte eine Systemschrift.
- Sicher ab Werk: ohne die drei Discord-Einstellungen ist das Portal aus; Anmeldung mit PKCE und einmaligem, an den Browser gebundenem `state`; Sitzungen nur im Speicher, höchstens 8 Stunden, nach 2 Stunden ohne Klick vorbei; jedes Formular mit CSRF-Schutz. Von Discord bleiben nur ID, Namen und Rollen-IDs.
- VerseKit verbindet sich unverändert per Code und Schlüssel; der offene Standard `versekit-org` v1 bleibt gleich.
- Neues Datenformat 3; ältere Daten ziehen beim Start nach einer Sicherung um. Version 1.0.0 startet danach nicht mehr auf diesen Daten (Rückweg: Sicherung vom Update).
- Anleitung: Discord-App anlegen und Rollen zuordnen, Schritt für Schritt.
- Das Image baut auf festen Basis-Images (Rust und distroless, je auf ihren Digest festgelegt): derselbe Tag ergibt immer dieselbe Grundlage.

## 1.0.0

Erste Version zum Selbsthosten.

- Erfüllt den offenen Standard `versekit-org` v1: Discovery, Anmeldung per Gerätecode, Erneuern mit Wechsel, Widerruf, Bestand, Baupläne, Aufträge mit Lieferungen.
- Statusseite unter `/` und Bestätigungsseite unter `/device` im Aussehen von VerseKit, Deutsch und Englisch nach Browser.
- Verwaltung unter `/admin`: Mitglieder mit Rechten und letztem Kontakt, Bestand, Baupläne, Lieferungen, Aufträge anlegen, bearbeiten und schließen.
- Sicher ab Werk: Admin-Passwort wird beim ersten Start erzeugt, ohne HTTPS nur im ausdrücklichen Testbetrieb, Selbstprüfung beim Start.
- Fertiges Image für amd64 und arm64 mit Herkunftsnachweis, gehärteter Container, Vorlage mit Caddy für HTTPS.
- Daten ziehen bei einem Update selbst um, vorher entsteht eine Sicherung.
- Prüfwerkzeug `versekit-orgcheck` für eigene Panels nach dem Standard.

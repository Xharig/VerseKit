# Datenschutz

**Deutsch** · [English](PRIVACY.en.md)

Verse-Kit läuft auf deinem Rechner. Es gibt kein Konto, keine Werbung und kein
Tracking. Einmal am Tag meldet das Programm, dass es läuft — ohne Kennung,
abschaltbar (siehe *Nutzung zählen*). Diese Seite sagt vollständig, was
das Programm liest, was es aus dem Netz holt und was es verschickt.

## Was auf deinem Rechner gelesen wird

Die Protokolldateien von Star Citizen (`Game.log` und `logbackups/`), deine
Steuerungsdateien und die Textdatei des Spiels. Daraus entstehen deine
Bauplan-Liste, dein Auftragsprotokoll und deine Statistik. In den Protokollen
steht auch dein RSI-Handle; Verse-Kit nutzt ihn, um nur die Baupläne deines
eigenen Accounts zu zählen. Alles bleibt in deinem Verse-Kit-Datenordner.

## Was aus dem Netz geholt wird

Nur Abrufe öffentlicher Daten — dabei wird nichts über dich mitgeschickt. Wie
bei jedem Abruf sieht die Gegenseite deine IP-Adresse.

| Wofür | Von |
|---|---|
| Neue Versionen, Katalog- und Vertragsdaten | GitHub |
| Herstellungsdaten | scmdb.net |
| Preise, Läden, Orte | UEX (uexcorp.space) |
| Schiffswerte | erkul.games |
| Serverstatus | status.robertsspaceindustries.com |
| Übersetzungen, die du auswählst | die jeweilige Quelle (etwa StarStrings, Luftwerft, rjcncpt) |

Mit der Umgebungsvariable `SC_BP_NO_NET=1` holt Verse-Kit gar nichts.

## Nutzung zählen

Einmal am Tag schickt Verse-Kit an `nutzung-versekit.xharig.com` genau diese Angaben:

| Angabe | Beispiel |
|---|---|
| Programmversion | `3.65.0` |
| System | Windows oder Linux |
| Sprache der Oberfläche und des Spiels | `de`, `en` |
| ob du Testversionen angeboten bekommst | ja / nein |
| welche Bereiche eingeschaltet sind | Schiffe, Werkstatt, Handel … |
| wie das Overlay läuft | immer sichtbar / nur bei neuen Bauplänen |
| ob Verse-Kit mit dem Rechner startet | ja / nein |

Dazu das **Land**, das Cloudflare bei jeder Anfrage selbst erkennt — nur das
Länderkürzel, keine Stadt, keine Region. Keine Kennung, kein Name, kein
RSI-Handle, keine Pfade. Gespeichert wird dort nur ein Zähler je Tag und Wert —
deine IP-Adresse wird nicht abgelegt. Damit lässt sich zählen, wie viele
Verse-Kit an einem Tag nutzen und was davon, aber nicht, wer, und auch keine
einzelne Installation über mehrere Tage verfolgen. Die Zahlen sieht nur der
Entwickler.

Abschalten: *Einstellungen → Allgemein → Nutzung zählen*. Mit
`SC_BP_NO_NET=1` entfällt die Meldung ebenfalls.

## Fehlerbericht

Der Bericht wird **nur verschickt, wenn du auf „Senden" drückst** — nachdem du
seinen vollen Wortlaut gesehen hast. Er geht an `bericht.xharig.com`, eine
Weiterleitung in den Meldekanal des Projekts auf Discord. Benutzername, Pfade
und alles, was nach Zugangsdaten aussieht, sind vorher ersetzt.

## KRT Profit Basetool

Nur, wenn du Verse-Kit ausdrücklich verbindest **und** den Abgleich
einschaltest. Ab Werk ist beides aus.

**Was hingeht** — je nach Bereich, den du einschaltest: deine Baupläne (Name,
Kennung des Basetools, Zeitpunkt und woher Verse-Kit sie kennt), deine
Lagerposten aus Rohstoff- und Handelslager (Material, Ort, Qualität, Menge,
„gestohlen") und deine Schiffe (Typ und Versicherung). Dazu der Name, den du
dieser Installation gibst — nie der Name deines Rechners — und, nur für die
Account-Prüfung, dein RSI-Handle. Nie Kaufdaten (Preis, Kaufdatum, Paket), nie
Daten anderer Mitglieder.

**Was zurückkommt:** deine eigenen Baupläne, Lagerposten und Schiffe aus dem
Basetool.

**Was gespeichert wird:** der abgeglichene Stand in deinem Datenordner
(`basetool-<Kennung>.json`), die Zugangsdaten nur im Schlüsselspeicher deines
Systems (siehe [Sicherheit](SECURITY.md)). Daten aus dem Basetool bleiben auf
deinem Rechner und gehen an niemanden weiter.

**Trennen** widerruft die Verbindung beim Basetool und löscht die Zugangsdaten
und den abgeglichenen Stand. Was das Basetool selbst mit deinen Daten tut,
regelt seine eigene Datenschutzerklärung auf
[profit-base.online](https://profit-base.online).

## Kontakt

Fragen zum Datenschutz über die [Issues](https://github.com/Xharig/VerseKit/issues),
Sicherheitsprobleme vertraulich über das
[Formular für Sicherheitshinweise](https://github.com/Xharig/VerseKit/security/advisories/new).

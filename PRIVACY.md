# Datenschutz

**Deutsch** · [English](PRIVACY.en.md)

Verse-Kit läuft auf deinem Rechner. Es gibt kein Konto, keine Werbung und kein
Tracking. Einmal am Tag meldet das Programm, dass es läuft und welche Seiten
genutzt wurden — ohne Kennung, abschaltbar (siehe *Nutzung zählen*). Diese Seite sagt vollständig, was
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
| ob neue Versionen von selbst eingespielt werden | ja / nein |
| welche Seiten im Hauptfenster du geöffnet hast, wie oft | Bauplan-Liste 12×, Shops 3× |
| auf welchem Weg eine Seite geöffnet wurde | Seitenleiste, Sprung, Overlay, Tray-Menü, Programmstart |
| wie viele Klicks es bis zu einer Seite brauchte | Shops: 5× nach 2 Klicks |
| welche Seite du nach wenigen Sekunden wieder verlassen hast, und wo du danach geblieben bist | Verkauf → Shops: 7× |
| wie oft du im Rohstofflager von Hand eingetragen, eine Raffinerie-Ausbeute eingetragen, den Bildschirm gelesen oder „Jetzt übertragen" gedrückt hast | Bildschirm gelesen: 2× |

Die Seiten- und Handlungsangaben sind nur Zähler seit der letzten Meldung, mit
festen Kennungen — keine Inhalte, keine Mengen, keine Rohstoffe, keine
Suchbegriffe, keine Namen, keine Uhrzeiten. Sie liegen bis zur nächsten Meldung in deinem Datenordner
(`seitennutzung.json`) und werden danach geleert. Ist die Meldung
ausgeschaltet, wird gar nicht erst mitgezählt.

Dazu das **Land**, das Cloudflare bei jeder Anfrage selbst erkennt — nur das
Länderkürzel, keine Stadt, keine Region. Keine Kennung, kein Name, kein
RSI-Handle, keine Pfade. Gespeichert wird dort nur ein Zähler je Tag und Wert —
deine IP-Adresse wird nicht abgelegt. Damit lässt sich zählen, wie viele
Verse-Kit an einem Tag nutzen und was davon, aber nicht, wer, und auch keine
einzelne Installation über mehrere Tage verfolgen. Die Zahlen sieht nur der
Entwickler.

Abschalten: *Einstellungen → Allgemein → Nutzung zählen lassen*. Mit
`SC_BP_NO_NET=1` entfällt die Meldung ebenfalls — und mit ihr das Mitzählen
der Seiten.

## Downloads über die Webseite

Die Download-Knöpfe auf versekit.xharig.com führen über `xharig.com/windows`
und `xharig.com/linux` zur Datei bei GitHub. Auf diesem Weg wird gezählt: Tag,
System und das **Land**, das Cloudflare selbst erkennt. Keine IP-Adresse,
keine Stadt, keine Kennung, kein Cookie. Wer direkt bei GitHub lädt, wird hier
nicht gezählt.

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
(`basetool-<Kennung>.json`, daneben die Vorgängerfassung
`basetool-<Kennung>.bak.json`), die Zugangsdaten nur im Schlüsselspeicher
deines Systems (siehe [Sicherheit](SECURITY.md)). Daten aus dem Basetool
bleiben auf deinem Rechner und gehen an niemanden weiter.

In dieser Stand-Datei stehen **dein RSI-Handle** (aus der Account-Prüfung) und
die **Liste deiner abgeglichenen Baupläne**, Lagerposten und Schiffe. Sie liegt
im Datenordner und kommt deshalb mit in die **Sicherungs-ZIP**
(*Einstellungen → Sichern & Zurücksetzen*). Liegt dein Datenordner in einer
Cloud (OneDrive, iCloud …) oder auf einer geteilten Platte, liegt sie dort mit.
Die Zugangsdaten selbst sind nie im Datenordner und nie in der Sicherung.

**Trennen** widerruft die Verbindung beim Basetool und löscht die Zugangsdaten
(Erneuerungs-Token und Schlüssel dieser Installation) und den abgeglichenen
Stand — `basetool-<Kennung>.json` **und** `basetool-<Kennung>.bak.json`. Was
schon in einer Sicherungs-ZIP steckt, bleibt dort, bis du sie löschst.

**Fehlerbericht:** Er kann die Namen von Bauplänen enthalten, die Verse-Kit
nicht zuordnen kann — darunter auch solche, die aus dem Basetool kamen. Dein
RSI-Handle und die Zugangsdaten stehen nicht darin, und er geht nur hinaus,
wenn du ihn selbst abschickst. Was das Basetool selbst mit deinen Daten tut,
regelt seine eigene Datenschutzerklärung auf
[profit-base.online](https://profit-base.online).

## scmdb.net

Nur, wenn du *Mit scmdb.net verbinden* einschaltest — ab Werk aus. Dann
öffnet Verse-Kit einen kleinen Anschluss **nur auf deinem eigenen Rechner**
(`127.0.0.1:23456`). Verse-Kit selbst schickt dabei nichts ins Netz: Die
scmdb-Seite in **deinem** Browser holt sich die Angaben dort ab und trägt sie
in das scmdb-Konto ein, mit dem du angemeldet bist.

**Was die Seite bekommt:** die Namen neu erhaltener Baupläne (englisch, wie
im Katalog) und zu laufenden Aufträgen die Kennungen aus dem Spielprotokoll,
den internen Vertragsnamen, Start, Ende und wie er endete — nur aus der
laufenden Spielsitzung und mit dem Spielkanal (LIVE, PTU …). Kein Handle,
keine Pfade, keine Lagerdaten. Andere Webseiten bekommen keinen Zugriff. Was
scmdb damit tut, regelt seine eigene Datenschutzerklärung auf
[scmdb.net](https://scmdb.net).

## Kontakt

Fragen zum Datenschutz über die [Issues](https://github.com/Xharig/VerseKit/issues),
Sicherheitsprobleme vertraulich über das
[Formular für Sicherheitshinweise](https://github.com/Xharig/VerseKit/security/advisories/new).

# Datenschutz

**Deutsch** · [English](PRIVACY.en.md)

VerseKit läuft auf deinem Rechner. Es gibt kein Konto, keine Werbung und kein
Tracking. Einmal am Tag meldet das Programm, dass es läuft und welche Seiten
genutzt wurden, ohne Kennung, immer an und ohne Schalter (siehe *Nutzung zählen*). Diese Seite sagt vollständig, was
das Programm liest, was es aus dem Netz holt und was es verschickt.

## Was auf deinem Rechner gelesen wird

Die Protokolldateien von Star Citizen (`Game.log` und `logbackups/`), deine
Steuerungsdateien und die Textdatei des Spiels. Daraus entstehen deine
Bauplan-Liste, dein Auftragsprotokoll und deine Statistik. In den Protokollen
steht auch dein RSI-Handle; VerseKit nutzt ihn, um nur die Baupläne deines
eigenen Accounts zu zählen. Alles bleibt in deinem VerseKit-Datenordner.

Nur solange du Anti-AFK einschaltest (ab Werk aus, nur unter Windows), sieht das
Programm außerdem, welches Programm im Vordergrund steht, wann Tastatur und Maus
zuletzt benutzt wurden und ob sich an Joystick, HOTAS oder Gamepad etwas bewegt
hat (nur *ob*, nicht was). Nichts davon wird gespeichert oder verschickt.

## Was aus dem Netz geholt wird

Nur Abrufe öffentlicher Daten, dabei wird nichts über dich mitgeschickt. Wie
bei jedem Abruf sieht die Gegenseite deine IP-Adresse.

| Wofür | Von |
|---|---|
| Neue Versionen, Katalog- und Vertragsdaten | GitHub |
| Herstellungsdaten | scmdb.net |
| Preise, Läden, Orte, Gegenstands- und Warenliste | UEX (uexcorp.space) |
| Schiffswerte | erkul.games |
| Serverstatus | status.robertsspaceindustries.com |
| Übersetzungen, die du auswählst | die jeweilige Quelle (etwa StarStrings, Luftwerft, Dymerz) |

Mit der Umgebungsvariable `SC_BP_NO_NET=1` holt VerseKit gar nichts.

## Nutzung zählen

Einmal am Tag schickt VerseKit an `nutzung-versekit.xharig.com` genau diese Angaben:

| Angabe | Beispiel |
|---|---|
| Programmversion | `3.65.0` |
| System | Windows oder Linux |
| Sprache der Oberfläche und des Spiels | `de`, `en` |
| ob du Testversionen angeboten bekommst | ja / nein |
| welche Bereiche eingeschaltet sind | Schiffe, Werkstatt, Handel, Basetool … |
| wie das Overlay läuft | immer sichtbar / nur bei neuen Bauplänen |
| ob VerseKit mit dem Rechner startet | ja / nein |
| ob neue Versionen von selbst eingespielt werden | ja / nein |
| welche Ansicht das Hauptfenster hat (in VerseKit 5 immer dieselbe) | Reiter |
| welche Kacheln der Startseite du ausgeblendet hast | Spielzeit, Rechner … |
| welche Seiten im Hauptfenster du geöffnet hast, wie oft | Bauplan-Liste 12×, Shops 3× |
| auf welchem Weg eine Seite geöffnet wurde | Seitenleiste, Sprung, Overlay, Programmstart |
| wie viele Klicks es bis zu einer Seite brauchte | Shops: 5× nach 2 Klicks |
| welche Seite du nach wenigen Sekunden wieder verlassen hast, und wo du danach geblieben bist | Verkauf → Shops: 7× |
| wie oft du in Mein Lager einen Rohstoff von Hand oder eine Raffinerie-Ausbeute eingetragen hast | von Hand eingetragen: 2× |
| wie oft du die Galerie „Was ist neu" geöffnet und daraus eine Seite geöffnet hast | Galerie geöffnet: 1× |
| ab VerseKit 5.2: das Alter der Installation in Tagen, ohne Kennung (ab 31 Tagen nur „älter") | 3 Tage |

Das Alter wird für sich gezählt, nicht zusammen mit Version, System oder
Land. Damit lässt sich sehen, wie viele neu dazukommen und wie viele nach
einer Woche noch dabei sind, aber nicht, wer.

Die Seiten- und Handlungsangaben sind nur Zähler seit der letzten Meldung, mit
festen Kennungen: keine Inhalte, keine Mengen, keine Rohstoffe, keine
Suchbegriffe, keine Namen, keine Uhrzeiten. Sie liegen bis zur nächsten Meldung in deinem Datenordner
(`seitennutzung.json`) und werden danach geleert. Wo die Meldung nicht
gesendet wird (siehe unten), wird auch nicht mitgezählt.

Dazu das **Land**, das Cloudflare bei jeder Anfrage selbst erkennt, nur das
Länderkürzel, keine Stadt, keine Region. Keine Kennung, kein Name, kein
RSI-Handle, keine Pfade. Gespeichert wird dort nur ein Zähler je Tag und Wert,
deine IP-Adresse wird nicht abgelegt. Damit lässt sich zählen, wie viele
VerseKit an einem Tag nutzen und was davon, aber nicht, wer, und auch keine
einzelne Installation über mehrere Tage verfolgen. Die Zahlen sieht nur der
Entwickler.

Die Meldung ist immer an, einen Schalter dafür gibt es nicht. Nur mit
`SC_BP_NO_NET=1` entfällt sie, und mit ihr das Mitzählen der Seiten.

## Downloads über die Webseite

Die Download-Knöpfe auf versekit.xharig.com führen über `xharig.com/windows`
und `xharig.com/linux` zur Datei bei GitHub. Auf diesem Weg wird gezählt: Tag,
System und das **Land**, das Cloudflare selbst erkennt. Trägt der Link ein
Kanal-Kennzeichen (etwa `xharig.com/windows?ref=discord`, damit sich zeigt,
welcher Beitrag Downloads gebracht hat), wird auch das je Tag gezählt, für sich
und nicht zusammen mit System oder Land. Keine IP-Adresse, keine Stadt, keine
Kennung, kein Cookie. Wer direkt bei GitHub lädt, wird hier nicht gezählt.

## Fehlerbericht

Der Bericht wird **nur verschickt, wenn du auf „Senden" drückst**, nachdem du
seinen vollen Wortlaut gesehen hast. Er geht an `bericht.xharig.com`, eine
Weiterleitung in den Meldekanal des Projekts auf Discord. Dein Benutzername,
dein Heimatordner in jedem Pfad und alles, was nach Zugangsdaten aussieht,
sind vorher ersetzt. Pfade außerhalb deines Heimatordners (etwa ein
Spielordner auf `D:\`) bleiben stehen; du siehst sie vor dem Senden.

Oben im Bericht steht, **wer meldet**, damit Rückfragen ankommen. Solange du
dort nichts Eigenes einträgst, ist das Feld mit deinem Spiel-Account
vorausgefüllt, sofern auf der Seite *Erkennung* einer gewählt ist. Du siehst
es vor dem Senden, kannst es ändern oder leeren; leer geht kein Name mit.
Derselbe Name benennt auch die ZIP mit den Bildern und ihren Ordner. Sonst
steht er nirgends im Bericht.

Statt zu senden kannst du den Bericht auch kopieren oder mit dem Knopf
**GitHub Issue …** in ein Formular legen lassen. Ein Issue ist **öffentlich**,
mitsamt dem Namen oben, falls du ihn stehen lässt.

Hast du den Haken für die Bilder gesetzt, gehen die Scan-Bilder mit: bei
Signaturen der Ausschnitt um die Zahl, beim Raffinerie-Scanner die Tabelle des
Terminals aus einer Lesung, die nicht sicher war (die letzten drei). Nur wenn
die Tabelle gar nicht gefunden wurde, ist es das Spielbild, verkleinert; darauf
können dein Handle und dein aUEC-Stand zu sehen sein. Die Bilder liegen bis
dahin in deinem Datenordner (`raffinerie-bilder`, `signatur-bilder`).

## KRT Profit Basetool

Nur, wenn du VerseKit ausdrücklich verbindest **und** den Abgleich
einschaltest. Ab Werk ist beides aus.

**Was hingeht**, je nach Bereich, den du einschaltest: deine Baupläne (Name,
Kennung des Basetools, Zeitpunkt und woher VerseKit sie kennt), deine
Lagerposten aus Mein Lager (Rohstoffe, Verkaufsware und Items; Material, Ort, Qualität, Menge,
„gestohlen"; gibst du einen Posten ins Lager der Einheit oder nimmst ihn
zurück, dazu die gewählte Einheit und die Menge) und deine Schiffe (Typ, Versicherung, eigener Name,
„Einsatzbereit" und die Einheit, in der ein Schiff steht). Dazu der Name, den du
dieser Installation gibst, nie der Name deines Rechners, und, nur für die
Account-Prüfung, dein RSI-Handle. Nie Kaufdaten (Preis, Kaufdatum, Paket).
Daten anderer Mitglieder nur, wenn du als Logistiker im **Lager der Einheit**
buchst: dann die Buchung selbst, also welcher Eintrag, Menge, Ort, Notiz,
Reservierung, Verkaufserlös und der Besitzer als Pseudonym, das das Basetool
nur für VerseKit vergibt. Vor dem Einbuchen schlägt VerseKit die Ware beim
Basetool nach: Dafür geht der Warenname bzw. die UEX-Nummer der Ware hin, auch
wenn danach nichts gebucht wird.

**Was zurückkommt:** deine eigenen Baupläne, Lagerposten und Schiffe aus dem
Basetool, je Lagerposten, wie viel davon persönlich ist und wie viel im
Lager welcher Einheit liegt, bei Hangar und Lager dazu die Liste der
Einheiten, in denen du Mitglied bist (nur im Speicher, für die Auswahl auf
der Schiffskarte und beim Teilen). Mit dem Bereich
*Lager der Einheit* das gemeinsame Lager deiner Einheiten (als Logistiker samt
Besitzern und Notizen), mit dem Bedarf als Logistiker die Aufträge deiner
Einheit mit Auftraggeber und Kommentar. Beides nur im Speicher, höchstens
7 Tage, nie in einer Datei oder Sicherung.

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
Stand: `basetool-<Kennung>.json` **und** `basetool-<Kennung>.bak.json`. Was
schon in einer Sicherungs-ZIP steckt, bleibt dort, bis du sie löschst.

**Fehlerbericht:** Er kann die Namen von Bauplänen enthalten, die VerseKit
nicht zuordnen kann, darunter auch solche, die aus dem Basetool kamen. Die
Zugangsdaten stehen nie darin, dein RSI-Handle nur oben als Melder, solange du
das Feld nicht änderst oder leerst (siehe [Fehlerbericht](#fehlerbericht)), und er geht nur hinaus,
wenn du ihn selbst abschickst. Was das Basetool selbst mit deinen Daten tut,
regelt seine eigene Datenschutzerklärung auf
[profit-base.online](https://profit-base.online).

## Hangar-Exporter

Damit der **Hangar-Exporter** (eine Browser-Erweiterung von Drachennarr) deinen
Hangar direkt an VerseKit schicken kann, nimmt VerseKit Lieferungen auf einem
kleinen Anschluss **nur auf deinem eigenen Rechner** an (`127.0.0.1`, den Port
nennt die Hangar-Seite neben dem Kopplungscode). Das gibt es nur mit eigenen
Daten, nicht in der Vorschau. Ab Werk ist es an; ausschalten lässt es sich unter
*Mein Hangar → Kopplung verwalten*. VerseKit selbst schickt dabei nichts ins Netz.

**Wer liefern darf:** nur, wer den **Kopplungscode** kennt. Du fügst ihn einmal
in die Erweiterung ein. Er bleibt gleich, bis du ihn erneuerst; danach gilt der
alte nicht mehr. Der Code liegt in deiner Einstellungsdatei im Datenordner und
steht nie in einer Meldung, einem Protokoll oder einer Antwort von VerseKit.

**Was ankommt:** die Schiffsliste deines Hangars, so wie die Erweiterung sie auf
der RSI-Seite liest. VerseKit übernimmt sie wie eine eingelesene Datei in deinen
Hangar (`hangar.json`) im Datenordner, sie bleibt auf deinem Rechner. Trägt eine
Lieferung ein Schiff aus, legt VerseKit vorher `hangar.bak.json` an. Was die
Erweiterung selbst tut, regelt ihre eigene Datenschutzerklärung im
[Repository des Hangar-Exporters](https://github.com/Drachennarr/Hangar-Exporter-Releases).

## scmdb.net

Nur, wenn du *Mit scmdb.net verbinden* einschaltest, ab Werk aus. Dann
öffnet VerseKit einen kleinen Anschluss **nur auf deinem eigenen Rechner**
(`127.0.0.1:23456`). VerseKit selbst schickt dabei nichts ins Netz: Die
scmdb-Seite in **deinem** Browser holt sich die Angaben dort ab und trägt sie
in das scmdb-Konto ein, mit dem du angemeldet bist.

**Was die Seite bekommt:** die Namen neu erhaltener Baupläne (englisch, wie
im Katalog) und zu laufenden Aufträgen die Kennungen aus dem Spielprotokoll,
den internen Vertragsnamen, Start, Ende und wie er endete, nur aus der
laufenden Spielsitzung und mit dem Spielkanal (LIVE, PTU …). Kein Handle,
keine Pfade, keine Lagerdaten. Andere Webseiten bekommen keinen Zugriff, auch
nicht über einen auf deinen Rechner umgelenkten Namen. Was
scmdb damit tut, regelt seine eigene Datenschutzerklärung auf
[scmdb.net](https://scmdb.net).

## Kontakt

Fragen zum Datenschutz über die [Issues](https://github.com/Xharig/VerseKit/issues),
Sicherheitsprobleme vertraulich über das
[Formular für Sicherheitshinweise](https://github.com/Xharig/VerseKit/security/advisories/new).

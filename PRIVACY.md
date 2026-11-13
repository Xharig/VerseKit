# Datenschutz

**Deutsch** · [English](PRIVACY.en.md)

VerseKit läuft auf deinem Rechner. Es gibt kein Konto, keine Werbung und kein
Tracking. Einmal am Tag meldet das Programm, dass es läuft und welche Seiten
genutzt wurden, ohne Kennung, ab Werk an und unter Einstellungen › Allgemein › Datenschutz abschaltbar (siehe *Nutzung zählen*). Diese Seite sagt vollständig, was
das Programm liest, was es schreibt, was es aus dem Netz holt und was es verschickt.
Dieselben Angaben, mit Links auf den Quellcode, stehen auf
[versekit.xharig.com/deine-daten](https://versekit.xharig.com/deine-daten/).

## Was auf deinem Rechner gelesen und geschrieben wird

### Spielordner

VerseKit liest die Protokolldateien von Star Citizen (`Game.log` und
`logbackups/`), deine Steuerungsdateien und die Textdatei des Spiels. Daraus
entstehen deine Bauplan-Liste, dein Auftragsprotokoll und deine Statistik.

In den Protokollen stehen auch dein RSI-Handle und deine RSI-Kontonummer.
VerseKit merkt sich beide in deinen Einstellungen (`eigener_account`,
`eigene_account_kennung`) und je Protokolldatei (`konten-je-log.json`). Es
nutzt sie, um nur die Baupläne deines eigenen Accounts zu zählen und die
Statistik deinem Account zuzuordnen. Der Handle füllt außerdem das Melder-Feld
im Fehlerbericht vor, und mit dem Basetool dient er der Account-Prüfung und
deinem eigenen Eintrag im Lager der Einheit.

VerseKit **schreibt** auch in den Spielordner, aber nur Dateien, die du auch
von Hand bearbeiten könntest: die Textdatei des Spiels (`global.ini`, für
Übersetzung und Bauplan-Angaben), die Steuerung (`actionmaps.xml`), die
Einstellungen (`attributes.xml`), die Grafik-Einstellungen
(`GraphicsSettings.json`) und `user.cfg`. Vorher legt es eine Kopie daneben
(`<Datei>.versekit-<Zeit>`), und solange das Spiel läuft, schreibt es nichts.
Hast du die Bauplan-Angaben eingerichtet, setzt VerseKit sie nach einem Patch
von selbst wieder ein. Den Shader-Zwischenspeicher des Spiels leert es nur auf
deinen Klick. Auf den Speicher des laufenden Spiels greift VerseKit nicht zu.

### Dein Datenordner

Ab Werk der Ordner `Verse-Kit` in deinen Dokumenten; den Ort änderst du unter
Einstellungen › Installation & Pfade. Liegen deine Dokumente in OneDrive,
iCloud oder einer anderen Cloud, liegt der Ordner dort mit. Darin liegt alles,
was VerseKit über dich weiß: Bauplan-Bestand, Merkliste, Einstellungen,
Auftragsprotokoll, Statistik, Spielzeit, Lager, Hangar, Beziehungen,
Fehlerprotokoll und die Bilder der Scanner. Die Sicherungs-ZIP
(*Einstellungen › Sichern & Zurücksetzen*) nimmt all das mit.

Außerhalb des Datenordners legt VerseKit nur ab: einen kleinen Verweis auf den
Ort des Datenordners und Sperrdateien in deinem Benutzerprofil, die
Update-Dateien, die Zugangsdaten des Basetools (siehe
[Sicherheit](SECURITY.md)) und, nur wenn du es einschaltest, den
Autostart-Eintrag. Unter Linux auf Knopfdruck einen Eintrag im Startmenü.

### Tastatur

Unter Windows meldet Windows VerseKit jeden Tastendruck (Raw Input). Anders
gehen Tastenkürzel im Spiel nicht, weil Star Citizen gewöhnliche Kürzel
anderer Programme abschaltet. Das läuft, solange das Overlay läuft, auch ohne
Anti-AFK. VerseKit vergleicht jeden Druck nur mit seinen eigenen Kürzeln (ab
Werk Alt+Umschalt mit V, R, B, H oder F) und behält im Arbeitsspeicher nur,
welche Tasten gerade gedrückt sind und wann zuletzt eine Eingabe kam. Was du
tippst, wird weder gespeichert noch verschickt, und nichts wird abgefangen: Das
Spiel bekommt dieselben Tasten. Unter Linux reserviert VerseKit nur seine
eigenen Kürzel (V, H und F) und sieht andere Tasten nicht.

### Bildschirm

Nur unter Windows und nur, solange Star Citizen das Fenster im Vordergrund ist;
steht etwas anderes vorn, nimmt VerseKit kein Bild auf. Liegt ein anderes
Fenster über dem Spiel, ist es mit im Bild. Unter Linux liest VerseKit den
Bildschirm nicht.

| Was | Wann | Was liest es | Was bleibt liegen |
|---|---|---|---|
| Signatur-Scanner | nur eingeschaltet, ab Werk aus | zweimal je Sekunde den Bereich in der Mitte des Spielbilds, nach einem Fund nur um die Zahl | nur der Ausschnitt um die Zahl: angelernte bis 200 (`signatur-bilder`), unsichere bis 12 (`signatur-gemerkt`) |
| Bildrate aus dem Spiel | nur eingeschaltet, ab Werk aus | alle drei Sekunden die rechte obere Ecke | nichts |
| Raffinerie (Alt+Umschalt+R) | auf deinen Tastendruck | einmal das ganze Spielfenster | nur bei einer unsicheren Lesung die Tabelle, sonst das Spielbild verkleinert (höchstens 2560 Pixel breit), die letzten drei (`raffinerie-bilder`); dazu die Zahlenfelder, bis Mein Lager sie übernimmt (`raffinerie-lesung.json`) |
| Spielbild für das Signatur-Fenster (Alt+Umschalt+B) | auf deinen Tastendruck | einmal das ganze Spielfenster | eine Datei, höchstens 1280 Pixel, das nächste Bild ersetzt sie (`Intern/signatur-spielbild.bin`) |

Auf ganzen Spielbildern können dein Handle und dein aUEC-Stand zu sehen sein.
Alle Bilder liegen im Datenordner und kommen mit in die Sicherungs-ZIP.
Verschickt werden sie nur im Fehlerbericht, und nur mit dem Haken für die
Scan-Bilder (siehe *Fehlerbericht*).

### Anti-AFK

Nur solange du Anti-AFK einschaltest (ab Werk aus, nur unter Windows), sieht das
Programm außerdem, welches Programm im Vordergrund steht, wann Tastatur und Maus
zuletzt benutzt wurden und ob sich an Joystick, HOTAS oder Gamepad etwas bewegt
hat (nur *ob*, nicht was). Der Schalter wird nie gespeichert: Nach jedem Start
ist Anti-AFK aus.

Für das Hauptfenster hält eine Arbeitsdatei (`Intern/anti-afk.json`) den
Zustand fest, dazu Zeitpunkt und Art der letzten Eingabe (Tastatur, Maus oder
Joystick), nie welche Taste. Sie wird bei jedem Start auf „aus“ zurückgesetzt
und kommt nicht in die Sicherung. Schickst du einen Fehlerbericht, während
Anti-AFK an ist, stehen darin die Sekunden seit der letzten Eingabe und ihre
Art, die Taste für „Visier abwischen“ und die Zahl der Sticks. Sonst wird
nichts davon verschickt.

## Was aus dem Netz geholt wird

Nur Abrufe öffentlicher Daten. Von dir geht nur mit, was eine Suche braucht:
bei der Handle-Suche der gesuchte Handle oder Org-Name, bei Preisen die Nummer
der Ware oder des Terminals. Wie bei jedem Abruf sieht die Gegenseite deine
IP-Adresse. Alle Abrufe laufen über https.

| Wofür | Von | Wann |
|---|---|---|
| Neue Versionen und Testversionen | GitHub (api.github.com, github.com) | beim Start und alle zehn Minuten |
| Herkunftsnachweis einer heruntergeladenen Version (siehe [Sicherheit](SECURITY.md)) | api.github.com, das Nachweis-Paket aus GitHubs Speicher bei Microsoft Azure (blob.core.windows.net) | nach jedem Update-Download |
| Herstellungs-, Katalog-, Vertrags- und Bergbaudaten | zuerst der öffentliche Spiegel der scmdb-Daten auf GitHub (raw.githubusercontent.com/KrovaxCode/SCMDB_DATA), sonst scmdb.net | von selbst |
| Preise, Läden, Orte, Gegenstands- und Warenliste | UEX (api.uexcorp.uk) | von selbst und auf den Seiten |
| Serverstatus | status.robertsspaceindustries.com | auf der Seite und für die Kachel der Startseite |
| Schiffswerte | cdn.erkul.games | beim Öffnen von Hangar oder einem Bauplan ohne Daten |
| Handle-Suche: Profil, Organisationen und Bilder des Handles oder der Org, die du suchst | robertsspaceindustries.com | nur, wenn du suchst |
| Takt des Executive Hangars (Contested Zones) | contestedzonetimers.com | auf der Seite, höchstens alle sechs Stunden |
| Übersetzungen, die du auswählst | die jeweilige Quelle (etwa StarStrings, Luftwerft, Dymerz) oder die Adresse, die du selbst einträgst | bei der Auswahl, danach von selbst erneuert |
| Geteilte Beziehungsliste („Liste laden“ in der Handle-Suche) | die Adresse, die du selbst einträgst | nur auf deinen Klick |

Jeder Abruf trägt die Kennung `VerseKit/<Version> (+https://versekit.xharig.com)`;
die Nutzungszählung schickt `VerseKit/<Version>`, der Fehlerbericht `Verse-Kit`.

Eine geteilte Beziehungsliste lädt VerseKit nur, wenn du auf „Liste laden“
klickst, und nur über https. Dorthin geht nur die übliche Kennung. Die Liste
kommt zu deinen Beziehungen in deinem Datenordner (`beziehungen.json`), deine
eigenen Markierungen bleiben dabei stehen. Die Adresse merkt sich VerseKit in
deinen Einstellungen (`beziehungen_liste_adresse`).

Neue Versionen holt und spielt VerseKit von selbst ein, frühestens zwei Minuten
nach ihrem Erscheinen und nie, solange Star Citizen läuft. Einen Schalter dafür
gibt es nicht.

Mit der Umgebungsvariable `SC_BP_NO_NET=1` (jeder Wert außer leer und `0`)
holt und sendet VerseKit gar nichts.

## Nutzung zählen

Einmal am Tag schickt VerseKit an `nutzung-versekit.xharig.com` genau diese Angaben:

| Angabe | Beispiel |
|---|---|
| Programmversion | `5.2.10` |
| System | Windows oder Linux |
| Sprache der Oberfläche und des Spiels | `de`, `en` |
| ob du Testversionen angeboten bekommst | ja / nein |
| welche Bereiche eingeschaltet sind | Schiffe, Werkstatt, Handel, Basetool … |
| wie das Overlay läuft | immer sichtbar / nur bei neuen Bauplänen |
| ob VerseKit mit dem Rechner startet | ja / nein |
| der Update-Schalter aus Einstellungen älterer Versionen (in VerseKit 5 ohne Wirkung) | ja / nein |
| welche Ansicht das Hauptfenster hat (in VerseKit 5 immer dieselbe) | Reiter |
| welche Kacheln der Startseite du ausgeblendet hast | Spielzeit, Rechner … |
| welche Seiten im Hauptfenster du geöffnet hast, wie oft | Bauplan-Liste 12×, Shops 3× |
| auf welchem Weg eine Seite geöffnet wurde | Seitenleiste, Sprung, Overlay, Programmstart, Menüfilter |
| wie viele Klicks es bis zu einer Seite brauchte | Shops: 5× nach 2 Klicks |
| welche Seite du nach wenigen Sekunden wieder verlassen hast, und wo du danach geblieben bist | Verkauf → Shops: 7× |
| wie oft du in Mein Lager von Hand, aus einer Raffinerie-Ausbeute oder mit dem Raffinerie-Scanner eingetragen hast | von Hand eingetragen: 2× |
| wie oft du die Galerie „Was ist neu" geöffnet und daraus eine Seite geöffnet hast | Galerie geöffnet: 1× |
| wie oft du den Menüfilter benutzt hast und wie oft er nichts fand, nie das Suchwort | benutzt: 4×, ohne Treffer: 1× |
| das Alter der Installation in ganzen Tagen, ohne Kennung (ab 31 Tagen immer 31) | 3 Tage |

Das Alter wird für sich gezählt, nicht zusammen mit Version, System oder
Land. Damit lässt sich sehen, wie viele neu dazukommen und wie viele nach
einer Woche noch dabei sind, aber nicht, wer. Gesendet wird nur die Zahl der
Tage, nie das Datum. Das Datum der Installation merkt sich VerseKit einmal als
Tag ohne Uhrzeit in deinen Einstellungen (`installiert_am`). Bei einer
Installation, die es schon gab, ist es der älteste Tag, den dein Datenordner
belegt, sonst der Tag des ersten Starts.

Die Seiten- und Handlungsangaben sind nur Zähler seit der letzten Meldung, mit
festen Kennungen: keine Inhalte, keine Mengen, keine Rohstoffe, keine
Suchbegriffe, keine Namen, keine Uhrzeiten. Sie liegen bis zur nächsten Meldung in deinem Datenordner
(`seitennutzung.json`) und werden danach geleert. Wo die Meldung nicht
gesendet wird (siehe unten), wird auch nicht mitgezählt.

Gesendet wird beim Start; danach sieht VerseKit jede Stunde nach, ob die
Meldung des Tages schon angekommen ist. Den Tag merkt es sich erst, wenn der
Empfänger sie angenommen hat (`nutzung_zuletzt`).

Dazu das **Land**, das Cloudflare bei jeder Anfrage selbst erkennt, nur das
Länderkürzel, keine Stadt, keine Region. Keine Kennung, kein Name, kein
RSI-Handle, keine Pfade. Gespeichert werden dort nur Zähler je Tag: Version,
System und Land zusammen, jede weitere Angabe für sich, das Alter getrennt von
allem anderen. Angaben, die nicht auf der Liste stehen, lehnt der Empfänger ab.
Deine IP-Adresse dient nur kurz der Mengenbremse und wird nicht abgelegt.
Damit lässt sich zählen, wie viele
VerseKit an einem Tag nutzen und was davon, aber nicht, wer, und auch keine
einzelne Installation über mehrere Tage verfolgen. Die Zahlen sieht nur der
Entwickler.

Die Meldung ist ab Werk an. Unter Einstellungen › Allgemein › Datenschutz
schaltest du „Nutzung zählen lassen“ aus (Schlüssel `nutzung_melden`); dann
wird nichts gesendet und nichts mehr gezählt, und die bisherigen Zähler löscht
VerseKit beim nächsten Zählversuch. Ebenso entfällt sie mit `SC_BP_NO_NET=1`.

## Downloads über die Webseite

Die Download-Knöpfe auf versekit.xharig.com führen über `xharig.com/windows`
und `xharig.com/linux` zur Datei bei GitHub. Auf diesem Weg wird gezählt: Tag,
System und das **Land**, das Cloudflare selbst erkennt. Trägt der Link ein
Kanal-Kennzeichen (etwa `xharig.com/windows?ref=discord`, damit sich zeigt,
welcher Beitrag Downloads gebracht hat), wird auch das je Tag gezählt, für sich
und nicht zusammen mit System oder Land. Keine IP-Adresse, keine Stadt, keine
Kennung, kein Cookie. Wer direkt bei GitHub lädt, wird hier nicht gezählt.

Dasselbe Kennzeichen gilt für die Webseite selbst: Rufst du sie mit einem
Kennzeichen auf (etwa `versekit.xharig.com/?ref=discord`), meldet die Seite das
einmal an `nutzung-versekit.xharig.com`, und gezählt wird nur das Kennzeichen je
Tag. Auch hier wird nichts über dich gespeichert: keine IP-Adresse (sie dient
nur kurz der Mengenbremse und wird nicht abgelegt), kein Land, keine Kennung,
kein Cookie. Ohne Kennzeichen am Link sendet die Seite nichts.

## Fehlerbericht

Der Bericht wird **nur verschickt, wenn du den Haken „Ich möchte den Bericht
an den Entwickler senden“ setzt und auf „Senden" drückst**, nachdem du
seinen vollen Wortlaut gesehen hast. Gesendet wird genau der angezeigte Text;
ist die Anzeige nicht mehr aktuell, wird nicht gesendet. Nach einem Absturz
sagt VerseKit das beim nächsten Start einmal und öffnet auf deinen Wunsch die
Seite *Fehler melden*; von selbst geht nichts hinaus.

Er geht an `bericht.xharig.com`, eine Weiterleitung in den Meldekanal des
Projekts auf Discord, die selbst nichts speichert. Vorher sind ersetzt: dein
Benutzername, dein Heimat- und Dokumente-Ordner in jedem Pfad, der
Rechnername, IP-Adressen, Mailadressen, Handles und Spielernummern aus den
Protokollen und alles, was nach Zugangsdaten aussieht. Pfade außerhalb deines
Heimatordners (etwa ein Spielordner auf `D:\`) bleiben stehen; du siehst sie
vor dem Senden.

Oben im Bericht steht, **wer meldet**, damit Rückfragen ankommen. Solange du
dort nichts Eigenes einträgst, ist das Feld mit deinem Spiel-Account
vorausgefüllt, sofern auf der Seite *Erkennung* einer gewählt ist. Du siehst
es vor dem Senden, kannst es ändern oder leeren; leer geht kein Name mit.
Derselbe Name benennt auch die ZIP mit den Bildern und ihren Ordner. Sonst
steht er nirgends im Bericht.

Statt zu senden kannst du den Bericht auch kopieren oder mit dem Knopf
**GitHub Issue …** in ein Formular legen lassen. Ein Issue ist **öffentlich**,
mitsamt dem Namen oben, falls du ihn stehen lässt.

Bilder, die du selbst anhängst (bis zu vier), gehen mit; ihre Metadaten werden
vorher entfernt. Die Scan-Bilder gehen nur mit, wenn du dafür einen eigenen
Haken setzt, und der gilt nur für diesen einen Bericht: bei Signaturen die
angelernten Ausschnitte um die Zahl (bis zu 200) und deine eigenen
Ziffern-Dateien, beim Raffinerie-Scanner die Tabelle des Terminals aus einer
Lesung, die nicht sicher war (die letzten drei). Nur wenn die Tabelle gar nicht
gefunden wurde, ist es das Spielbild, verkleinert; darauf können dein Handle
und dein aUEC-Stand zu sehen sein. Das Spielbild für das Signatur-Fenster und
die unsicheren Signatur-Ausschnitte (`signatur-gemerkt`) gehen nie mit.

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
Mit dem Bereich **Raffinerieaufträge abgleichen** gehen deine in VerseKit erfassten
Raffinerie-Aufträge hin: die Raffinerie, die Endzeit, je Gut der Rohstoff,
die Ausbeute in cSCU und die Qualität, dazu die gewählte Einheit und ein
zufälliger Schlüssel, den VerseKit dem Auftrag gibt. Die eingesetzte Menge
zeigt das Spiel nicht, sie geht als „unbekannt" hin. Ändert sich die Endzeit,
geht die neue hin; buchst du die Ausbeute in Mein Lager, nur die Meldung
„abgeholt", ohne Mengen (das Lager selbst geht über den Lager-Abgleich). Zum
Zuordnen schlägt VerseKit die Namen der Rohstoffe beim Basetool nach.

**Was zurückkommt:** deine eigenen Baupläne, Lagerposten und Schiffe aus dem
Basetool, je Lagerposten, wie viel davon persönlich ist und wie viel im
Lager welcher Einheit liegt, bei Hangar und Lager dazu die Liste der
Einheiten, in denen du Mitglied bist (nur im Speicher, für die Auswahl auf
der Schiffskarte und beim Teilen). Mit dem Bereich
*Lager der Einheit* das gemeinsame Lager deiner Einheiten (als Logistiker samt
Besitzern und Notizen), mit dem Bedarf als Logistiker die Aufträge deiner
Einheit mit Auftraggeber und Kommentar. Beides nur im Speicher, höchstens
7 Tage, nie in einer Datei oder Sicherung. Mit den Raffinerie-Aufträgen deine
offenen Raffinerie-Aufträge, auch die, die du im Basetool angelegt hast, mit
allem, was das Basetool dazu liefert (Ort, Methode, Zeiten, Status, Güter
und, falls dort eingetragen, Kosten und Erlös). Im Basetool angelegte
Aufträge übernimmt VerseKit, damit das Overlay sie meldet.

**Kartellbank:** Nur mit dem eigenen Schalter *Kartellbank anzeigen*. VerseKit liest
die Konten, die du im Basetool sehen darfst (Name, Nummer, Art, Einheit,
Stand, Zielstand, Veränderung und Verlauf der letzten 30 Tage, ob und bis zu
welcher Grenze du dort Anträge stellen darfst), und deine eigenen Anträge mit
Status, Prüfer und Ablehnungsgrund. Öffnest du ein Konto, kommen dessen
letzte 100 Buchungen (mit Gegenkonto und, falls erfasst und nicht verborgen,
dem Namen des Gegenübers) und der Kontostand der letzten 90 Tage dazu. Nur mit
dem zweiten Schalter *Warteschlange als Bankmitarbeiter* und nur, wenn du im Basetool
Bankmitarbeiter bist, kommt die Warteschlange: offene und freizugebende
Anträge anderer Mitglieder mit Betrag, Konten, Begründung, Notiz und dem
Namen des Antragstellers, sofern er ihn nicht verbirgt. Hin geht nur, was du
auf der Seite Kartellbank selbst abschickst: ein neuer Antrag (Art, Konto,
Zielkonto, Betrag, Notiz, Begründung), eine Korrektur deines offenen Antrags
(Betrag, Notiz, Begründung, Zielkonto) oder das Zurückziehen. Bestätigen,
Ablehnen und Freigeben gehen nie über VerseKit, nur im Basetool. **Alles aus
der Kartellbank bleibt nur im Speicher**, höchstens 7 Tage, nie in einer
Datei, einer Sicherung oder einem Fehlerbericht; im Fehlerprotokoll steht
höchstens der Fehlercode, ohne Konto oder Antrag. Das Overlay bekommt die
Kontostände und die Zahl offener Anträge direkt vom Hauptfenster, ohne Datei.
Gespeichert werden nur die beiden Schalter in deinen Einstellungen.

**Was gespeichert wird:** der abgeglichene Stand in deinem Datenordner
(`basetool-<Kennung>.json`, daneben die Vorgängerfassung
`basetool-<Kennung>.bak.json`), die Zugangsdaten nur im Schlüsselspeicher
deines Systems (siehe [Sicherheit](SECURITY.md)). Daten aus dem Basetool
bleiben auf deinem Rechner und gehen an niemanden weiter.

In dieser Stand-Datei stehen **dein RSI-Handle** (aus der Account-Prüfung) und
die **Liste deiner abgeglichenen Baupläne**, Lagerposten und Schiffe, mit den
Raffinerie-Aufträgen dazu deine offenen Raffinerie-Aufträge, wie das Basetool
sie liefert. In deinen Raffinerie-Aufträgen (`raffinerie-auftraege.json`)
merkt sich VerseKit je Auftrag den Schlüssel bzw. die Kennung des
Basetool-Auftrags; übernommene Aufträge stehen dort mit Material, Ausbeute,
Ort und Endzeit wie deine eigenen. Die Stand-Datei liegt
im Datenordner und kommt deshalb mit in die **Sicherungs-ZIP**
(*Einstellungen → Sichern & Zurücksetzen*). Liegt dein Datenordner in einer
Cloud (OneDrive, iCloud …) oder auf einer geteilten Platte, liegt sie dort mit.
Die Zugangsdaten selbst sind nie im Datenordner und nie in der Sicherung.

**Trennen** widerruft die Verbindung beim Basetool und löscht die Zugangsdaten
(Erneuerungs-Token und Schlüssel dieser Installation) und den abgeglichenen
Stand: `basetool-<Kennung>.json` **und** `basetool-<Kennung>.bak.json`, und
leert die Kartellbank im Speicher. Deine Raffinerie-Aufträge bleiben mit
ihren Schlüsseln, bis sie wie jeder Auftrag aus der Liste fallen. Was
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
Daten, nicht in der Vorschau. Ab Werk ist es an. Unter *Mein Hangar → Kopplung
verwalten* schaltest du es aus; dann bleibt der Anschluss offen, lehnt aber
jede Lieferung ab. VerseKit selbst schickt dabei nichts ins Netz.

**Wer liefern darf:** nur, wer den **Kopplungscode** kennt. Du fügst ihn einmal
in die Erweiterung ein. Er bleibt gleich, bis du ihn erneuerst; danach gilt der
alte nicht mehr. Der Code liegt in deiner Einstellungsdatei im Datenordner und
steht nie in einer Meldung, einem Protokoll oder einer Antwort von VerseKit,
im Fehlerbericht nur als Punkte. Mit der Einstellungsdatei kommt er in die
Sicherungs-ZIP; beim Zurückspielen einer Sicherung übernimmt VerseKit ihn
nicht, es gilt der Code, den diese Installation schon hat.

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
keine Pfade, keine Lagerdaten.

**Wer sonst herankommt:** Von anderen Rechnern ist der Anschluss nicht
erreichbar. Er antwortet nur auf Anfragen an `127.0.0.1` oder `localhost`,
damit keine Webseite über einen auf deinen Rechner umgelenkten Namen
herankommt, und Browser lassen nur scmdb.net die Antwort lesen. Andere
Programme auf deinem eigenen Rechner können dieselben Angaben abrufen. Was
scmdb damit tut, regelt seine eigene Datenschutzerklärung auf
[scmdb.net](https://scmdb.net).

## Kontakt

Fragen zum Datenschutz über die [Issues](https://github.com/Xharig/VerseKit/issues),
Sicherheitsprobleme vertraulich über das
[Formular für Sicherheitshinweise](https://github.com/Xharig/VerseKit/security/advisories/new).

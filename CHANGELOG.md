# Changelog

**Deutsch** · [English](CHANGELOG.en.md)

Alle wichtigen Änderungen an diesem Projekt werden hier dokumentiert.

Das Projekt nutzt SemVer: `MAJOR.MINOR.PATCH`.

## v3.19.0 - 2026-09-06

> **Dein Hangar kommt ins Werkzeug.** Trag deine Schiffe ein, und der Watcher
> beantwortet die Frage, die auf jeden neuen Bauplan folgt: Passt das Teil
> überhaupt in eines meiner Schiffe? Dazu planst du je Steckplatz, was
> hineinsoll, siehst was es kostet — gekauft oder selbst gebaut —, was du dafür
> noch farmen musst, und ob sich das Zerlegen eines Wracks überhaupt lohnt.

### Neu

- **Mein Hangar** — eigene Gruppe „Schiffe". Import aus dem Pledge-Store über
  die Erweiterung *Hangar XPLORer* oder von Hand; jedes Schiff trägt seine
  Herkunft (Echtgeld oder im Spiel gekauft).
- **Passt in dein Schiff** — zu jedem Bauplan steht in der Herstellung, in
  welche deiner Schiffe das Teil passt und in wie viele Steckplätze.
- **Ausstattung planen** — je Steckplatz festlegen, was dort sitzen soll, mit
  Güte und Klasse an jedem Teil („A · Militär · nur über Bauplan"). Auch
  militärische Teile stehen zur Wahl: nicht kaufbar, aber herstellbar.
- **Wunschliste** — Schiffe, die du dir vornimmst, mit Kaufpreis und Ort. Die
  Ausstattung lässt sich planen, bevor du das Schiff besitzt.
- **Was noch fehlt** — die Rechnung über alle Schiffe: jede Position mit Schiff
  und Steckplatz, kaufen oder selbst bauen je Posten, Summe und Einkaufsroute.
  Was du eingebaut hast, hakst du ab.
- **Was ich farmen muss** — dein Rohstofflager gegen alles gerechnet, was du
  selbst bauen willst. Erz mit zu geringer Güte wird genannt, nicht
  verschwiegen.
- **Was steckt drin?** — was ein Wrack ab Werk an Bord hat und was es im Laden
  wert ist.
- **Lohnt das Zerlegen?** — was der Fabricator zurückgibt. Sechs Rohstoffe
  kommen nie wieder; bei den meisten Teilen ist mindestens einer davon dabei.
- **Fertig gefittete Schiffe sind markiert** — mit dem Hinweis, dass ein neu
  geclaimtes Schiff in der Werksausstattung zurückkommt und ohne passende
  Versicherung alles Eingebaute weg ist.
- **Geräte-Hub** auf der Steuerungsseite: welches Gerät welche Nummer im Spiel
  hat, mit laufender Überwachung.
- **Blickwinkel** — Bildschirm ausmessen und den Sitzabstand bewerten.
- Ein Klick ins Leere nimmt den Cursor aus dem Eingabefeld.

### Verbessert

- **Der Ablage-Ordner nimmt beim Umstellen die Daten mit** — geprüft kopiert,
  der alte Ordner bleibt liegen.
- Die Achsen-Tabelle zeigt, welche Flugfunktion auf welcher Achse liegt — nur
  dort gibt es eine Empfindlichkeit einzustellen.
- Die Empfindlichkeit lässt sich einstellen, ohne dass die Regler zu zwei
  Balken zusammengequetscht werden.
- Gleiche Steckplätze stehen in einer Zeile: aus 46 Zeilen bei einer Cutlass
  Black werden 18.

### Behoben

- **Fenster öffneten sich irgendwo** — auf mehreren Bildschirmen auch außerhalb
  des sichtbaren Bereichs, was das Programm unbedienbar machte. Alle Fenster
  setzen sich jetzt mittig über das Hauptfenster.
- **Das Auftrags-Protokoll blockierte beim ersten Aufbau neun Sekunden** — es
  liest die Logs jetzt im Hintergrund.
- Die Wunschliste wurde beim Speichern gelöscht, sobald an einem Schiff etwas
  geändert wurde.
- Meldungen erscheinen im Programmstil statt als Systemfenster in der Sprache
  des Betriebssystems.
- Mengen unter zehn werden auf zwei Stellen genau angezeigt.
- Zwei gleichzeitige Schreibvorgänge auf dieselbe Ablage stören sich nicht mehr.

### Dank

Für Vorschläge und Rückmeldungen: **Zwaersch (KRT)** für die Bergungs-Idee, die
Wunschliste und die Korrektur an „unbrauchbar", **Bushwick4712** für den Hinweis
auf das nicht nachgeführte Auftrags-Protokoll.

## v3.19.0-rc24 - 2026-09-06

> **Kleine Mengen standen falsch da.** Aus 0,64 wurde „0,6", aus 0,32 ein
> „0,3" — bei Rohstoffmengen, die fast alle unter eins liegen, ist das keine
> Rundung mehr, sondern eine andere Zahl.

### Behoben

- **Mengen unter zehn werden auf zwei Stellen genau angezeigt.** Betrifft den
  Zerlege-Rechner und die Farmliste.
- „1 Rohstoffe bekommst du nicht zurück" heißt jetzt „Riccite bekommst du
  nicht zurück."

## v3.19.0-rc23 - 2026-09-06

> **Lohnt sich das Zerlegen?** Ein neuer Reiter unter Bergung sagt dir vor dem
> Ausbauen, welche Rohstoffe der Fabricator zurückgibt — und welche dabei
> ersatzlos verschwinden. Das ist der Unterschied zwischen einem lohnenden
> Wrack und einem umsonst geschleppten Bauteil.

### Neu

- **„Lohnt das Zerlegen?"** unter Bergung: Teil aussuchen, und es steht da,
  was drinsteckt und was du zurückbekommst.
- **⚠ Sechs Rohstoffe kommen nie zurück** — darunter Quantainium und Stileron.
  Bei den meisten Teilen ist mindestens einer davon dabei; wer nur wegen des
  Quantainiums zerlegt, hat umsonst geschleppt. Die Seite sagt es an jedem
  Rohstoff einzeln.
- Ausbeute und Dauer kommen aus den Spieldaten, nicht aus dem Programm —
  ändert CIG sie mit einem Patch, ändert sich die Auskunft mit.

## v3.19.0-rc22 - 2026-09-06

> **Ein Fehler hat die Wunschliste gelöscht.** Sobald du an irgendeinem Schiff
> eine Komponente eingetragen hast, war sie weg — die Datei wurde nur zur
> Hälfte geschrieben. Behoben, und eine Prüfung wacht darüber.

### Behoben

- **⚠ Die Wunschliste verschwand**, sobald an einem Schiff etwas geändert
  wurde. In der Datei stehen zwei Listen, geschrieben wurde nur eine — die
  andere war damit gelöscht. Änderungen an einem Wunschschiff gingen aus
  demselben Grund verloren.
- **Die Seite „Was noch fehlt" brach mit einem Fehler ab**, sobald ein
  Wunschschiff dabei war: Die Einkaufsroute erwartete einen Steckplatz, den
  ein ganzes Schiff nicht hat.
- **Drei Sicherheitsabfragen lösten einen Fehler aus** statt zu fragen.
- **Eingebautes steht nicht mehr in den Listen.** Weder im Warenkorb unter dem
  Schiff noch unter „Was noch fehlt" — beide zeigen, was noch zu tun ist. Die
  Zahl der erledigten Posten steht darunter, im Warenkorb holt ein Klick sie
  zurück.
- Aus „1 Positionen aus 1 Schiffen" wird „Eine Position an 1 Schiff".

## v3.19.0-rc21 - 2026-09-06

> **Fenster gehen jetzt dort auf, wo du hinsiehst.** Alle Fenster des Programms
> setzen sich mittig über das Hauptfenster — vorher entschied das der
> Fenstermanager, und auf mehreren Bildschirmen ging das schief. Dazu klappt
> der Warenkorb weg, was schon eingebaut ist.

### Neu

- **Ein Klick ins Leere nimmt den Cursor aus dem Eingabefeld** — wie überall
  sonst auch.

### Verbessert

- **Eingebautes verschwindet aus dem Warenkorb.** Eine Zeile nennt die Zahl,
  ein Klick holt es zurück. Ein Korb zeigt, was noch hineingehört.

### Behoben

- **Sechs Fenster hatten keine Position**, nur eine Größe: Belegung,
  Assistent, Bestand, Einstellungen, Blickwinkel und Versionen. Sie gehen
  jetzt mittig über dem Hauptfenster auf.

## v3.19.0-rc20 - 2026-09-06

> **Ein Dialog konnte das Programm unbedienbar machen.** Er erschien außerhalb
> der Bildschirme, hielt als modales Fenster alles fest — und weil man ihn
> nicht sah, wirkte es, als sei die Bedienung kaputt. Alle 46 solcher Fenster
> sind ersetzt.

### Behoben

- **Fenster außerhalb des Bildschirms.** Beim Speichern der Joystick-Belegung
  öffnete sich ein System-Fenster irgendwo am Rand, an das man nicht herankam.
  Das Programm ließ sich danach nicht einmal mehr beenden.
- **Die Gruppen der Seitenleiste ließen sich nicht auf- und zuklappen** — es
  war derselbe unsichtbare Dialog, der die Oberfläche festhielt.
- **Meldungen erscheinen jetzt im Programmstil**, mittig über dem Fenster und
  in der eingestellten Sprache statt der des Systems.
- **Abgehaktes fällt überall heraus**: aus der Zählung „noch zu besorgen", aus
  der Warenkorb-Überschrift, aus der Summe und aus der Farmliste. Was gebaut
  und eingebaut ist, braucht kein Material mehr.

## v3.19.0-rc19 - 2026-09-06

> **Ein Kristall für die Farmliste.** Der neue Werkstatt-Reiter hatte noch kein
> eigenes Bild und zeigte deshalb nur ein Ersatzzeichen.

### Verbessert

- **„Was ich farmen muss" hat jetzt ein Symbol** — ein Kristall, passend zum
  Erz, um das es dort geht.

## v3.19.0-rc18 - 2026-09-06

> **Abhaken, was erledigt ist — und sehen, was noch im Boden liegt.** Das
> Werkzeug kann nicht wissen, ob ein gekauftes Teil schon im Schiff steckt;
> jetzt hakst du es ab, und die Summe schrumpft mit. Dazu eine neue Seite in
> der Werkstatt, die dein Lager gegen alles rechnet, was du selbst bauen
> willst.

### Neu

- **Posten abhaken.** Was du gekauft oder gebaut und eingebaut hast, hakst du
  ab — es bleibt in der Liste stehen, zählt aber nicht mehr zur Summe. Ein
  Haken für beide Wege.
- **„Was ich farmen muss"** in der Werkstatt: alle Rohstoffe, die dir für deine
  geplanten Bauteile fehlen, über alle Posten zusammengerechnet. Erz mit zu
  geringer Güte wird genannt statt verschwiegen.
- **Fertig gefittete Schiffe sind markiert** — mit dem Hinweis, dass ein
  neu geclaimtes Schiff in der Werksausstattung zurückkommt und ohne die
  passende Versicherung alles Eingebaute weg ist.
- **In der Schiffszeile steht, wie viel noch offen ist** — ohne die
  Ausstattung aufzuklappen.
- **Militärische Teile stehen jetzt in der Auswahl.** Sie sind nicht kaufbar,
  nur herstellbar — bisher fehlten sie deshalb ganz. Bei einem Quantenantrieb
  der Größe 2 kommen zwei dazu.
- **Güte und Klasse an jedem Teil**, in der Auswahlliste und am Steckplatz:
  „A · Militär · nur über Bauplan". Wo die Klasse unbekannt ist, wird sie
  weggelassen und nicht geraten.

### Verbessert

- **Der Reiter heißt „Was noch fehlt"** statt „Einkaufsliste" — er führt beide
  Wege, kaufen und selbst herstellen.
- Die Teileauswahl lässt sich auch nach Güte und Klasse durchsuchen: „stealth"
  findet die Tarn-Komponenten.

### Behoben

- **Der Knopf „Kaufen" war unerreichbar**, sobald der Ladenname lang war — der
  Preistext hat ihn aus dem Fenster geschoben. Wer einmal auf „Selbst
  herstellen" gewechselt hatte, kam nicht zurück.
- Der Verkaufsort stand doppelt in der Preiszeile.
- **Die Güte stand bei zwei von drei Teilen als Zahl** statt als Buchstabe.
- Ein Wunschschiff bekam seine Steckplätze erst nach einem Neustart.

## v3.19.0-rc17 - 2026-09-06

> **Der Hub sagt jetzt, was zu tun ist.** Ein Stick mit neuer Kennung sieht
> aus wie zwei Probleme und ist eines — das Werkzeug erkennt das und bietet
> den einen Handgriff an, der es behebt. Und wenn es nicht eindeutig ist,
> rät es nicht, sondern sagt bei jedem Gerät einzeln, was ihm fehlt.

### Neu

- **Einkaufsliste als eigener Reiter.** Was für den ganzen Hangar zu besorgen
  ist, an einem Ort.
- **Zuordnungs-Assistent.** Der Geräte-Hub sagt jetzt nicht nur, was los ist,
  sondern was zu tun ist. Der häufigste Fall sieht nach zwei Problemen aus und
  ist eines: Ein Stick mit neuer Kennung steht zweimal da — einmal als
  fehlend, einmal als unbekannt. Ein Knopf hängt die Belegung um, ohne eine
  einzige Belegungszeile anzufassen.
- **Geraten wird dabei nicht.** Der Vorschlag zum Umhängen entsteht nur, wenn
  genau ein Gerät fehlt und genau eines neu dasteht. Bei mehreren wäre die
  Zuordnung Ratearbeit — und ein falsch geratener Ersatz vertauscht zwei
  Sticks, was man erst im Gefecht merkt. Dann steht bei jedem Gerät einzeln,
  was ihm fehlt.

### Behoben

- **Ein fehlendes Symbol riss das ganze Fenster mit.** Fehlt eine Bilddatei,
  fällt der betroffene Knopf jetzt auf Text zurück, statt den Aufbau
  abzubrechen.
- **Wunschschiffe bekommen ihre Steckplätze auch nach dem Eintragen** — bisher
  blieben sie leer, bis man das Fenster neu öffnete.

## v3.19.0-rc16 - 2026-09-06

> **Welcher Stick ist eigentlich js1?** Über einen Joystick gibt es im Rechner
> drei Aussagen — was das System sieht, was das Spiel zuletzt sah, und was in
> der Belegung steht. Ihre Nummern stimmen nicht überein: Derselbe Stick kann
> am System `js0` sein und im Spiel `js2`. Genau daran scheitern die meisten
> Anleitungen. Oben auf der Steuerungsseite steht jetzt beides nebeneinander,
> und wenn du einen Stick absteckst, merkt es das Werkzeug von selbst.

### Neu

- **Geräte-Hub.** Alle Eingabegeräte an einem Ort: die Nummer, die Star
  Citizen benutzt, der Name, unter dem das System das Gerät führt, und der
  Zustand. Vier Fälle, jeder mit einem Satz, der sagt was los ist — bereit,
  ohne Nummer in der Belegung, nicht angesteckt, oder dem Spiel noch nie
  begegnet.
- **Laufende Überwachung.** Wird ein Gerät ab- oder angesteckt, steht es
  binnen Sekunden da. Ohne Systemdienst und ohne Zusatzpaket: Die Geräteliste
  wird gelesen und mit der vorigen verglichen, auf Windows und Linux gleich.
- **Die Wunschliste ist ein eigener Reiter** unter „Schiffe". Sie stand vorher
  unten auf der Hangar-Seite und war hinter vierzig Schiffen nicht zu finden.
- **Güte und Klasse an der Teileauswahl.** Man baut ein Schiff auf einen Zweck
  hin — Tarnung, Kampf, Bergbau. Eine reine Namensliste sagt darüber nichts,
  und die Namen kennt kaum jemand auswendig. Jetzt steht an jedem Teil und an
  jedem Steckplatz, worum es sich handelt.
- **Einkaufsliste über alle Schiffe.** Was für den ganzen Hangar zusammen zu
  besorgen ist, statt Schiff für Schiff.

### Verbessert

- **Die Empfindlichkeit lässt sich jetzt wirklich einstellen.** Die Regler
  waren zwischen Beschriftung, eigenem Speichern-Knopf und Zahl so
  zusammengequetscht, dass zwei kurze Balken übrig blieben, die niemand als
  Regler erkennt. Sie laufen jetzt über dieselbe Breite und denselben
  Speichern-Knopf wie Totzone und Sättigung.

### Behoben

- **Der Kaufpreis im Warenkorb wurde nie nachgeschlagen** — „wird
  nachgeschlagen …" stand für immer da, und die Summe blieb bei 0 aUEC.
- **Zurück auf „Kaufen" ging nicht.** Der Knopf erschien nur bei bekanntem
  Preis; wer einmal auf „Selbst herstellen" gewechselt hatte, saß fest.
- **„Neu"-Marken nachgetragen** bei Hangar, Wunschliste und Bergung — bei
  allen dreien fehlten sie.

## v3.19.0-rc15 - 2026-09-06

> **Zwei Sticks, eine Einstellung.** „Achsen & Kurven" kann jetzt alles, was
> die Feinabstimmung braucht: Totzone, Sättigung und Empfindlichkeit an
> Reglern, mit der Kurve daneben, die sofort zeigt, was passiert. Ein Knopf
> überträgt alles auf den zweiten Stick, ein anderer tauscht die Belegungen
> über Kreuz, wenn sie nach einem Neustart auf der falschen Hand sitzen. Und
> ganze Einrichtungen lassen sich unter einem Namen sichern — „mit Pedalen"
> und „ohne Pedale" sind zwei Klicks auseinander.

### Neu

- **Belegungen zweier Sticks tauschen.** Sitzt nach einem Neustart die ganze
  Belegung auf der falschen Hand, tauscht ein Knopf, welcher Stick welche
  Nummer hat. Keine einzige Belegungszeile wird dabei angefasst, und Totzone
  und Sättigung bleiben beim jeweiligen Gerät.
- **Gerätesätze.** Eine ganze Einrichtung unter einem Namen — etwa „mit
  Pedalen" und „ohne Pedale". Gespeichert werden Totzone, Sättigung und
  Empfindlichkeit; fehlt beim Anwenden ein Gerät, wird es übersprungen und
  gesagt, welches.

- **Alte Geräte-Einträge entfernen.** Star Citizen legt bei jeder neuen
  Gerätekennung einen weiteren Eintrag an und räumt nie auf — für einen
  einzigen Stick standen so drei Stück in der Datei. Weil sie sich
  widersprechen, wurde man den Hinweis „alte Einstellungen, die nichts mehr
  tun" nie los: Übernahm man den einen, wich der nächste ab. Ein Knopf räumt
  jetzt alles weg, was zu keinem angeschlossenen Gerät mehr gehört.

- **Empfindlichkeit einstellen.** Die Mitte der Kurve — über 1 wird das Zielen
  feiner, unter 1 direkter. Auf einer Stickachse liegen oft mehrere Funktionen
  (auf der Y-Achse etwa Nicken **und** Schub hoch/runter), jede mit eigenem
  Wert; jede bekommt deshalb ihren eigenen Regler.

### Verbessert

- **„Achsen & Kurven" liegt unter „Für Fortgeschrittene".** Die Seite schreibt
  in die Datei, an der die komplette Steuerung hängt — wer nicht weiß, was
  Sättigung ist, macht sich damit den Stick unbrauchbar. „Blickwinkel" bleibt
  offen: Es schreibt nichts, es rechnet nur.
- **Der Hinweis auf alten Altbestand steht unten und ist zugeklappt.** Vorher
  stand er in Gold ganz oben und las sich wie ein Fehler, den man wegklicken
  muss — mit der Folge, dass man reihum draufdrückt und sich funktionierende
  Werte mit alten, widersprüchlichen überschreibt. Jetzt steht als Erstes da,
  dass nichts zu tun ist.
- **Die Sicherung nimmt auch die Spieleinstellungen mit.** In `attributes.xml`
  steht der Blickwinkel und die Grafik; bisher wäre nach einem Zurückholen
  zwar die Steuerung dagewesen, der eingestellte Blickwinkel aber weg.
- **Die Anleitung zeigt, was die drei Werte bewirken** — vier Kurven
  nebeneinander, in beiden Sprachen.
- **Der Hinweis auf alte Einstellungen erklärt sich selbst.** Vorher stand
  dort nur, die Einstellung hänge „an der alten Kennung des Geräts" — richtig,
  aber unverständlich. Jetzt steht da, was das für den Spieler bedeutet und
  was der Knopf daneben tut.

### Behoben

- **Eingestellte Werte kamen anders in der Datei an.** Der Regler zog jeden
  Wert auf sein Raster, schon beim Öffnen der Seite — dadurch stand dort
  sofort „Ungespeicherte Änderung", und beim Speichern landeten Werte in der
  Belegungsdatei, die niemand eingestellt hatte.
- **Die Dialoge auf „Achsen & Kurven" und „Blickwinkel" sahen aus wie vom
  Betriebssystem** — weißer Kasten mit grauen Knöpfen mitten im dunklen
  Fenster, und sie gingen unten statt in der Fenstermitte auf.
- **Knöpfe mit langen Gerätenamen wurden abgeschnitten.** Sie brechen jetzt
  um, statt aus dem Fenster zu laufen.
- **Die Seite sprang beim Anklicken einer Achse nach oben.** Die Rollstelle
  bleibt jetzt stehen.
- **„Totzone: war 0.1 → jetzt 0.1"** stand im Befund, wo es gar keinen
  Unterschied gab: Zwei Werte, die sich um weniger als ein Tausendstel
  unterscheiden, sehen in der Anzeige gleich aus und gelten jetzt auch als
  gleich.
- **Die Knöpfe zum Übertragen und Tauschen sagen jetzt, was sie tun.**

## v3.19.0-rc15 - 2026-09-06

> **Ausgewählte Teile landen jetzt wirklich im Steckplatz** — und auch ein
> Schiff auf der Wunschliste lässt sich schon ausstatten. Gerade da ist die
> Planung am meisten wert: vor dem Kauf, solange die Summe noch eine
> Entscheidung ist.

### Neu

- **Wunschschiffe lassen sich ausstatten.** Der Block „Ausstattung & Warenkorb"
  steht jetzt auch unter jedem Schiff auf der Wunschliste — wer sich ein Schiff
  vornimmt, kann gleich planen, was hineinsoll, und sieht Kaufpreis und
  Teilekosten zusammen. Was dort geplant wird, bleibt Planung: Ein Wunschschiff
  taucht nirgends bei „passt in dein Schiff" auf.

### Behoben

- **Ein angeklicktes Teil wurde nicht eingesetzt.** Die Auswahl klappte auf, der
  Klick tat aber nichts — ohne Meldung, ohne Fehler.

## v3.19.0-rc14 - 2026-09-06

> **Man sieht jetzt, dass die Steckplätze anklickbar sind.** Sie waren es
> vorher auch — nur sagte es niemand.

### Verbessert

- **Aufklapp-Pfeil an jedem Steckplatz.** Ein Klick auf die Zeile öffnet die
  Teileauswahl; erkennbar war das bisher nur am Mauszeiger, wenn man zufällig
  darüberfuhr. Überall sonst im Programm steht an solchen Zeilen ein Pfeil.

## v3.19.0-rc13 - 2026-09-06

> **Die Ausstattungsliste passt wieder auf einen Bildschirm.** Sechzehn
> identische Raketenplätze untereinander waren keine Liste, sondern eine Wand.

### Verbessert

- **Gleiche Steckplätze stehen in einer Zeile**, mit Anzahl davor: aus 46
  Zeilen bei einer Cutlass Black werden 18. Was du dort auswählst, gilt für
  alle Plätze der Zeile — bei „16 x Missile S2" meinst du alle sechzehn.
- ⚠ Belegst du einen einzelnen Platz anders, löst er sich aus der Gruppe und
  steht für sich. Sonst verschwände deine Änderung hinter einem „16 x".

## v3.19.0-rc12 - 2026-09-06

> **Eine Wunschliste für Schiffe, die du dir noch vornimmst.** Trag ein, was du
> dir erspielen oder kaufen willst — daneben steht, was es kostet und wo es das
> gibt.

### Neu

- **Wunschliste** unter „Mein Hangar". Schiffe und Fahrzeuge, die du haben
  möchtest, mit Kaufpreis und Ort, dazu der Mietpreis, falls es einen gibt. Was
  im Spiel gar nicht käuflich ist, sagt das auch. Vorschlag von Zwaersch (KRT).
- ⚠ Was schon in deinem Hangar steht, kommt nicht auf die Wunschliste — und die
  Wunschliste taucht nirgends bei „passt in dein Schiff" auf. Ein Wunsch ist
  kein Besitz.

### Verbessert

- **„Ausstattung" statt „Auslegung".** Das zweite war die wörtliche Übersetzung
  von „loadout" und klang nach etwas, das ein Ingenieur berechnet.

## v3.19.0-rc11 - 2026-09-06

> **Der Windows-Bau lief nicht mehr durch.** Ein einzelnes Sonderzeichen in
> einer Prüfmeldung hat gereicht — auf Linux fällt so etwas nie auf, weil dort
> jedes Zeichen ausgegeben werden kann.

### Behoben

- **Der Selbsttest zählt Reiter nicht mehr von Hand.** Eine notierte Zahl
  musste bei jedem neuen Bereich nachgepflegt werden und schlug an, obwohl
  nichts kaputt war. Jetzt wird verglichen, was der Sprachwechsel wirklich
  prüfen soll: dass danach kein Reiter fehlt.
- **Der Bau lief unter Windows nicht durch.** Ein Pfeilzeichen in einem
  Prüftext ließ den Selbsttest dort mit einer Ausnahme abbrechen — die
  Windows-Konsole kann es nicht ausgeben, unter Linux fällt das nie auf. Eine
  neue Prüfung fängt das künftig ab.

## v3.19.0-rc10 - 2026-09-06

> **Sag dem Werkzeug, wie dein Schiff aussehen soll — es sagt dir, was dir
> dafür fehlt.** Unter „Mein Hangar" lässt sich jetzt für jeden Steckplatz
> festlegen, was dort sitzen soll. Alles, was nicht ab Werk verbaut ist, landet
> in einem Warenkorb — und dort steht bei jedem Posten **beides**: was er im
> Laden kostet und was er dich an Material kostet, wenn du ihn selbst baust.
> Was davon du nimmst, entscheidest du.

### Verbessert

- **Der Rückfrage-Dialog sieht aus wie das Programm.** „Gemerkte Wracks
  verwerfen" öffnete den Dialog des Betriebssystems — weißer Kasten mit
  englischen Knöpfen mitten in einem deutschen, dunklen Fenster.
- **„Eingabe leeren" in der Bergung.** Ein Suchfeld, das man nur mit der
  Rücktaste leerbekommt, ist bei „Anvil F7C-M Super Hornet Mk II" eine
  Zumutung.
- **„Unbrauchbar" statt „Brikett".** Die erste Fassung übersetzte das
  englische Wort zu wörtlich. Gemeldet von Zwaersch (KRT).

### Neu

- **Achsen & Kurven.** Ein neuer Bereich zeigt, wie scharf jede Stick-Achse
  reagiert: Totzone und Sättigung als Kurve, umschaltbar zwischen Quadrant
  und Vollansicht, dazu eine große Ansicht in eigenem Fenster.
- **Einstellungen, die nichts mehr tun, werden gefunden.** Star Citizen hängt
  Totzone und Sättigung an die Kennung des Geräts. Bekommt ein Stick eine
  neue — anderer USB-Anschluss, neue Firmware —, legt das Spiel ihn als neues
  Gerät an, und die alten Werte bleiben wirkungslos in der Datei stehen. Im
  Spiel ist das nirgends zu sehen. Der Bereich zeigt sie und holt sie auf
  Knopfdruck zurück.
- **Zwei Sticks gleich einstellen.** Ein Knopf überträgt Totzone und
  Sättigung aller gemeinsamen Achsen auf das andere Gerät — wer HOSAS fliegt,
  tippt sonst ein Dutzend Mal dieselbe Zahl.
- **Blickwinkel.** Es gibt genau einen Blickwinkel, bei dem das Bild so groß
  erscheint wie das Gezeigte in Wirklichkeit wäre — dann stimmen Größen und
  Entfernungen. Der neue Bereich rechnet ihn aus und sagt dazu, wo du dafür
  sitzen müsstest.
- **Bildschirm mit einer Bankkarte ausmessen.** Ein Vollbildfenster zeigt ein
  Rechteck, das auf die Größe der Karte gezogen wird — jede Bankkarte misst
  85,60 × 53,98 mm. Genauer als jede Geräteabfrage, und bei mehreren
  Bildschirmen die einzige Angabe, die überhaupt stimmt.
- **Optimalpunkt und Ampel.** Neben dem eingestellten Wert aus dem Spiel steht,
  wie weit man dafür sitzen müsste — und ob der eigene Sitzabstand dazu passt.
- **Auslegung speichern.** Für jedes Schiff im Hangar lässt sich Platz für
  Platz festlegen, welches Teil dort sitzen soll. Daneben steht immer, was ab
  Werk drin ist.
- **Warenkorb.** Alles, was von der Werksausstattung abweicht, steht als Liste
  darunter — mit Summe.
- **Kaufen oder selbst herstellen, je Posten.** Beide Kosten stehen
  nebeneinander: Ladenpreis mit Ort, daneben Materialkosten und Bauzeit. Für
  Teile ohne Bauplan steht dort „nur kaufbar". Braucht ein Rezept einen
  Rohstoff, den kein Laden führt, wird das gesagt — die Materialkosten sind
  dann eine Untergrenze, keine Endsumme.
- **Einkaufsroute.** Für alles Gekaufte eine Route mit möglichst wenigen
  Stopps, nach Orten gruppiert. Zwei Läden an derselben Station sind ein Stopp.

## v3.19.0-rc9 - 2026-09-06

> **Alle 265 Schiffe einzeln durchgeprüft.** 225 finden ihre Bestückungsdaten,
> 35 sind Konzepte — übrig bleiben fünf, die es in der Quelle schlicht nicht
> gibt.

### Verbessert

- **Hammerhead, Idris-P und San tok.Yāi werden jetzt gefunden.** Ihre Namen
  weichen in der Quelle so weit ab, dass keine Regel sie erwischt — sie stehen
  deshalb in einer kurzen, sichtbaren Zuordnungsliste. Dasselbe Vorgehen wie
  bei den Bauplan-Korrekturen.
- **Namen mit Akzenten werden richtig gelesen.** Aus „San tok.Yāi" wurde beim
  Vergleich `santokyi` — das `ā` fiel heraus, und der Name passte zu nichts
  mehr.
- **„RSI Ursa" landet beim Rover.** Ohne Zusatz ist die Grundausführung
  gemeint; vorher standen drei gleichwertige Kandidaten da und es wurde
  keiner gewählt.

## v3.19.0-rc8 - 2026-09-06

> **Die Schiffserkennung ist durchgezählt.** Von 265 Schiffen finden jetzt
> **220** ihre Bestückungsdaten; 35 der übrigen sind Konzepte, die es im Spiel
> nicht gibt. Und die Bergung hat einen Knopf zum Verwerfen bekommen.

### Neu

- **„Gemerkte Wracks verwerfen"** in der Bergung. Nachgeschlagene Schiffe
  werden gemerkt, damit sie beim nächsten Mal sofort dastehen — jetzt lassen
  sie sich auch wieder loswerden, ohne eine Datei von Hand zu löschen.

### Verbessert

- **Zusammengezogene Herstellernamen werden erkannt.** „Aegis Gladius Valiant"
  fand seine Daten nicht, weil die Quelle den Hersteller als `aegs` führt — das
  ist kein Wortanfang von „Aegis", sondern eine Zusammenziehung. Dasselbe bei
  `anvl` für Anvil und `drak` für Drake.
- **Auch ganz andere Herstellernamen stören nicht mehr.** „C.O. Mustang Alpha"
  heißt in der Quelle `cnou_mustang_alpha`, „Greycat PTV" heißt `gama_ptv` —
  der Hersteller darf beim Zuordnen jetzt fehlen, solange der Rest passt.
- **Anbauteile stehen nicht mehr in der Schiffsliste.** „Retaliator Cargo
  Module" und die Endeavor-Pods sind keine Schiffe; bei der Bergung zeigten sie
  fälschlich die Ausstattung des ganzen Hauptschiffs.

### Behoben

- **„Fliegt im Spiel noch nicht" stand auch bei Schiffen, die fliegen** —
  Hammerhead und Idris-P zum Beispiel. Diese Aussage kommt jetzt nur noch, wenn
  UEX das Schiff wirklich als Konzept führt; sonst steht dort, dass keine Daten
  vorliegen.

## v3.19.0-rc7 - 2026-09-06

> **Vor dir treibt ein Wrack — lohnt das Aussteigen?** Der neue Bereich
> *Bergung* sagt, was ab Werk in einem Schiff steckt und was die Teile im Laden
> wert sind. Und er sagt dazu, wann die Zahl **nicht** gilt.

### Neu

- **Bergung: „Was steckt drin?"** Schiff aussuchen, und du siehst die
  Werksausstattung — Kühler, Schilde, Triebwerke, Waffen, jeweils mit Größe,
  Güte und Ladenwert, dazu die Summe. Einmal nachgeschlagene Schiffe stehen
  beim nächsten Mal sofort da.
- ⚠ **Der Hinweis steht ganz oben, nicht im Kleingedruckten:** Die Zahlen
  gelten für **NPC-Wracks**. Ein Spielerschiff wird unbrauchbar, sobald sein
  Besitzer die Versicherung beansprucht — ausgebaute Teile sind dann wertlos,
  und nur das Abkratzen der Hülle lohnt. Vorschlag von Zwaersch (KRT).

### Verbessert

- **Festverbautes wird nicht mitgerechnet.** Panzerung und Strukturteile lassen
  sich nicht ausbauen; sie würden einen Wert ausweisen, den niemand aus dem
  Wrack bekommt. Turmwaffen bleiben dabei erhalten — der Turm ist fest, die
  Waffen darin sind es nicht.

## v3.19.0-rc6 - 2026-09-06

> **Der Rat, der auf den Umzug gewartet hat.** Wer denselben Rechner mal unter
> Windows und mal unter Linux startet, führt sonst zwei getrennte Bestände —
> ohne es zu merken.

### Neu

- **Anleitung für Doppelstart-Nutzer**, in der Programmhilfe und in beiden
  READMEs: Ordner für die eigenen Daten auf eine Platte legen, die beide
  Systeme sehen, und in beiden Systemen gleich einstellen. Dann gibt es nur
  einen Bestand — nichts wird abgeglichen, also kann nichts auseinanderlaufen.

## v3.19.0-rc5 - 2026-09-06

> **Der Ablage-Ordner nimmt jetzt deine Daten mit.** Bisher setzte das
> Umstellen nur den Pfad — die Baupläne blieben liegen, und nach dem Neustart
> sah das Werkzeug leer aus. Dazu steht in der Herstellung endlich, welche
> Größe, Güte und Klasse ein Bauplan hat.

### Neu

- **Größe, Güte und Klasse in der Herstellung.** Beim aufgeklappten Bauplan
  steht jetzt neben dem Hersteller „Militär · Größe 4 · Güte A". Bei Rüstung
  und Handfeuerwaffen steht dort nichts — dort bedeuten diese Werte nichts.

### Verbessert

- **Der Ablage-Ordner zieht um.** Beim Umstellen fragt das Werkzeug, ob die
  vorhandenen Daten mitkommen sollen, prüft vorher, ob sich am neuen Ort
  überhaupt schreiben lässt, und vergleicht jede kopierte Datei mit dem
  Original. **Der alte Ordner bleibt vollständig liegen** — nichts wird
  gelöscht. Liegt am Ziel schon eine Ablage, wird sie nicht angerührt; du wirst
  gefragt, ob du sie benutzen willst.
- **„Passt in dein Schiff" ist nicht mehr zu übersehen** — die Zeile steht fett
  und farbig statt grau.

### Behoben

- **„Passt in keines deiner Schiffe" stand auch dann da, wenn schlicht die
  Daten fehlten.** Beides sah im Programm gleich aus und bedeutet das
  Gegenteil. Jetzt sagt es, dass die Daten noch geholt werden — und holt sie.
- Rüstungsteile und Handfeuerwaffen bekamen eine Größe und eine Güte
  angedichtet, die sie nicht haben.

## v3.19.0-rc4 - 2026-09-06

> **Von Hand eingetragene Schiffe finden ihre Steckplätze.** Und ein Knopf ist
> verschwunden, den niemand brauchte.

### Verbessert

- **Der Knopf „Steckplätze holen" ist weg.** Er holte nach, was nach Import und
  Handeintrag ohnehin geholt wird — und warf damit nur die Frage auf, wozu er
  gut sei. Fehlendes wird jetzt beim Öffnen der Seite nachgezogen, im
  Hintergrund und ohne Zutun.
- Die Anleitung hat ein Bild von **Mein Hangar**.

### Behoben

- **„Anvil Arrow" fand seine Daten nicht.** Wer ein Schiff von Hand einträgt,
  hat kein Herstellerkürzel dabei — und `anvl` ist kein Anfang von „Anvil",
  sondern eine Zusammenziehung. Die Zuordnung kennt jetzt alle 152
  Herstellernamen und ihre Kürzel.
- **Steckplätze aus einer älteren Testfassung werden neu geholt.** Ihnen fehlte
  eine Angabe, mit der die Zuordnung arbeitet; ohne sie sah ein Hangar aus, als
  hätte er keine Daten.

## v3.19.0-rc3 - 2026-09-06

> **Zwei Nachbesserungen an der Schiffserkennung.** Namen, die zusammen-
> geschrieben werden, wurden nicht gefunden — und wo eine Konzept-Angabe steht,
> steht jetzt auch, woher sie kommt.

### Behoben

- **Zusammengeschriebene Namen finden ihr Schiff.** „L-22 Alpha Wolf" wurde
  nicht erkannt, weil die Steckplatz-Daten `alphawolf` in einem Wort führen.
- **Bei „Konzept" steht jetzt die Quelle dabei.** Die Angabe kommt von UEX und
  ist nicht immer aktuell — das A.T.L.S. IKTI wird dort als Konzept geführt,
  obwohl es im Spiel geflogen wird. Statt einer Behauptung über das Spiel steht
  dort nun eine Fremdangabe mit Absender.

## v3.19.0-rc2 - 2026-09-06

> **Der Hangar findet jetzt seine Schiffe.** Jäger, Renner und Exo-Anzüge
> ließen sich gar nicht eintragen, und Schiffe, die längst fliegen, standen als
> „noch nicht im Spiel" da. Beides ist behoben; dazu ist der Hangar aus der
> Werkstatt in einen eigenen Bereich *Schiffe* umgezogen.

### Neu

- **Eigener Bereich *Schiffe*** in der Leiste, zwischen Bauplänen und
  Werkstatt. Der Hangar ist keine Zutat zum Bauen, und der Bereich wächst noch.

### Verbessert

- **Alle Schiffe stehen zur Auswahl, nicht nur die mit Laderaum.** Die Liste
  bot 134 von 280 an — Arrow, Gladius und A.T.L.S. IKTI fehlten darin komplett.
- **Die Auswahlliste rollt.** Statt zehn Namen und „und 124 weitere" siehst du
  alle Schiffe und kannst durchblättern; darüber steht, wie das Feld benutzt
  wird.
- **Punkte und Bindestriche zählen beim Suchen nicht mehr.** „ATLS" findet
  „A.T.L.S." und umgekehrt.

### Behoben

- **„noch nicht im Spiel" stand bei Schiffen, die längst fliegen** — die
  Ironclad Assault und die F7C-M Super Hornet Mk II wurden nur nicht erkannt.
  Die Zuordnung versteht jetzt ausgeschriebene Hersteller („Drake" ↔ `drak`)
  und römische Zahlen („Mk II" ↔ `mk2`). Wo wirklich nichts vorliegt, steht
  „keine Steckplatz-Daten"; „Konzept — noch nicht im Spiel" nur dann, wenn es
  auch belegt ist.
- **„im Spiel" heißt jetzt „im Spiel gekauft"** — es beschreibt die Herkunft,
  nicht den Aufenthaltsort.
- Die Meldung bei erfolgloser Suche sprach von Waren statt von Schiffen.

## v3.19.0-rc1 - 2026-09-06

> **Dein Hangar kommt dazu — und damit die Frage, die auf jeden neuen Bauplan
> folgt: Passt das Teil überhaupt in eines meiner Schiffe?** Trag deine Schiffe
> ein, und in der Herstellung steht künftig, wo der Bauplan hineingehört. Den
> Hangar holst du dir in einem Zug aus dem Pledge-Store; im Spiel gekaufte
> Schiffe trägst du daneben von Hand ein.

### Neu

- **Mein Hangar.** Ein neuer Bereich unter *Werkstatt*: welche Schiffe dir
  gehören, woher sie kommen und wie viele Steckplätze sie haben.
- **Import aus dem Pledge-Store.** Die Browser-Erweiterung *Star Citizen Hangar
  XPLORer* legt dir deinen Hangar als Datei hin — der Watcher liest sie und
  übernimmt Schiffe samt LTI und Paketnamen. Nimm die JSON-Datei; die CSV wird
  auch gelesen, war bei einem echten Export aber unvollständig.
- **Von Hand eintragen.** Im Spiel gekaufte Schiffe stehen in keinem Export.
  Jedes Schiff trägt, woher es kommt.
- **„Passt in dein Schiff".** In der Herstellung steht jetzt unter dem
  Ladenpreis, in welche deiner Schiffe das Teil passt — und in wie viele
  Steckplätze. Ohne eingetragene Schiffe steht dort der Hinweis, wo man sie
  einträgt, nicht „passt nirgends".
- Die Steckplatz-Daten kommen von **erkul.games** und werden auf deinem Rechner
  abgelegt. Geholt wird nur, was du wirklich im Hangar hast, und nur einmal je
  Spiel-Patch. Vorschlag von Zwaersch (KRT).

## v3.18.2 - 2026-09-06

> **Ein Knopf, der sagt, was er tut.** „Jetzt auffrischen" hieß er — dabei holt
> er die Daten und schreibt die Angaben komplett neu ins Spiel. Jetzt heißt er
> „Neu einsetzen".

### Verbessert

- **„Neu einsetzen" statt „Jetzt auffrischen".** Der Knopf unter *Texte im
  Spiel → Von Hand* holt die Vertragsdaten **und** trägt den ganzen Block neu
  ein. „Auffrischen" beschrieb nur die halbe Arbeit und klang nach
  Nachsehen. Die Erwähnung im Hinweis darunter zieht mit.

## v3.18.1 - 2026-09-06

> **Die Rufpunkte springen dir jetzt ins Auge.** Sie standen schon länger in
> den Auftragstexten — nur in derselben Farbe wie alles andere, und damit
> übersah man sie. Jetzt sind sie blau wie die übrigen Angaben. Und wo das
> Spiel keine Rufwerte kennt, steht das auch da, statt dass die Zeile einfach
> fehlt.

### Verbessert

- **Auch die Rufpunkte im Bauplan-Block sind blau.** Abklingzeit und
  Teilbarkeit waren es schon, die beiden Reputationszeilen darüber nicht —
  in einem echten Spielstand waren das rund tausend Zeilen, die zwischen den
  hervorgehobenen untergingen. Gemeldet von Bushwick4712 (KRT).
- **„Keine Angaben" statt einer fehlenden Zeile.** Für 109 Aufträge führt die
  Datenquelle keine Rufwerte. Bisher fehlte die Zeile dort ganz, und das sah
  aus wie ein Aussetzer des Werkzeugs. Jetzt steht da, dass es nichts zu
  holen gibt — nachgesehen wurde trotzdem.

## v3.18.0 - 2026-09-06

> **Dein Auftrags-Protokoll sagt endlich die Wahrheit.** Bisher galt jeder
> Auftrag, den du nicht ausdrücklich aufgegeben hast, als geschafft — auch die
> gescheiterten. Jetzt siehst du auf einen Blick, wie es ausgegangen ist: grün
> für geschafft, rot für aufgegeben und misslungen. Sechs Filterknöpfe zeigen
> dir jede Art einzeln. Und abgebrochene Aufträge verschwinden sofort aus dem
> Overlay, statt dort weiterzulaufen.

> [!important]
> **Ein Klick, der sich lohnt:** Unter *Für Fortgeschrittene → Bauplan-Bestand*
> gibt es „Protokolle erneut einlesen". Der Lauf bewertet jetzt auch dein
> Auftrags-Protokoll neu — nur so wirken die Verbesserungen auf das, was schon
> drinsteht. Bei einem gewachsenen Protokoll wurden dabei **102 Aufträge**
> berichtigt.

### Behoben

- **Ein abgebrochener Auftrag verschwindet jetzt aus dem Overlay.** Brichst du
  einen Auftrag ab, meldet das Spiel das ohne den Namen — nur mit einer
  Kennung. Genau diese Meldung hat der Watcher übersehen, und der Auftrag stand
  weiter als laufend da, obwohl das Auftrags-Protokoll ihn längst als
  abgebrochen führte.
- **Gescheiterte Aufträge galten als abgeschlossen.** Star Citizen
  unterscheidet zwischen aufgegeben, gescheitert und geschafft — der Watcher
  kannte nur „aufgegeben" und zählte alles andere als Erfolg. In einem
  gewachsenen Protokoll waren das **52** Aufträge, die grün dastanden, obwohl
  sie fehlgeschlagen sind.
- **Der Export für scmdb.net passt wieder.** scmdb hat sein Dateiformat
  gewechselt — der Watcher schrieb noch das alte, und beim Hochladen kam der
  Bestand dort nicht mehr an. Jede Zeile trägt jetzt die Kennung, mit der
  scmdb seine Baupläne führt. Aufgefallen an der Exportdatei von Zwaersch (KRT).

- **„Protokolle erneut einlesen" räumte nur die halbe Stube auf.** Der Lauf
  trug Baupläne nach, ließ das Auftrags-Protokoll aber unberührt. Damit wirkte
  jede Verbesserung an der Auswertung nur auf künftige Aufträge — was schon
  eingetragen war, blieb falsch. Jetzt werden beide erfasst, und die Meldung
  nennt für Aufträge, wie viele neu dazukamen und wie viele berichtigt wurden.

### Neu

- **Sechs Filterknöpfe im Auftrags-Protokoll.** Alle · läuft · abgeschlossen ·
  abgebrochen · fehlgeschlagen · nicht mehr offen. Jeder Knopf trägt die Farbe
  seiner Aufträge, so wie sie in der Liste stehen.

### Verbessert

- **Der Ausgang eines Auftrags ist an der Farbe zu sehen.** Grün für
  geschafft, blasses Rot für abgebrochen und fehlgeschlagen, Grau für „nicht
  mehr offen". Vorher standen abgebrochen und nicht mehr offen beide in Grau
  und waren nicht auseinanderzuhalten.

## v3.17.3 - 2026-09-05

> **Ein Kanalwechsel kostet dich nicht mehr deine Vorgeschichte.** Der Watcher
> liest jetzt auch die Protokolle der Nachbarkanäle — wer von HOTFIX auf LIVE
> wechselt, behält alles. Und Zurücksetzen sagt vorher, was es kostet.

### Behoben

- **Die Protokolle der Nachbarkanäle werden mitgelesen.** Wer von HOTFIX auf
  LIVE wechselt (oder von PTU zurück), ließ seine ganze Vorgeschichte im
  anderen Ordner liegen: Bei einem Melder kamen aus 221 Protokollen nur **drei**
  Baupläne heraus, weil der Rest im HOTFIX-Ordner lag.

  Es ist dieselbe Person mit demselben Spielstand — nur der Kanal ist ein
  anderer. **Nur LIVE und HOTFIX**: PTU, EPTU und Technical Preview laufen auf
  eigenen Spielständen, dort freigeschaltete Baupläne hat man auf LIVE nicht.
  Gesucht wird außerdem nur neben einem echten `StarCitizen`-Ordner; wer sein
  Spiel woanders liegen hat, bekommt weiterhin genau seinen Ordner.
  Gemeldet von **Zwaersch**.

### Verbessert

- **Die Warnung vor dem Zurücksetzen nennt Zahlen.** Ein Melder hat seinen
  Bestand damit von **232 auf 3** gesetzt — die Warnung war richtig, aber ohne
  Zahlen: Bei ihm gaben 221 Protokolle nur drei Baupläne her. Jetzt steht dort
  „Du hast 232 Baupläne. Aus deinen Protokollen kommen 3 zurück — 229 gehen
  verloren."
  Gemeldet von **Zwaersch**.

## v3.17.2 - 2026-09-05

> **Karteileichen im Auftrags-Protokoll räumen sich jetzt wirklich ab.** Die
> Regel dafür gab es seit v3.15.8 — ihr Ergebnis kam nur nie in der Datei an.
> Und die Bauplan-Liste sagt jetzt, was sie nicht wissen kann.

### Behoben

- **Ein Auftrag, den es längst nicht mehr gibt, blieb trotzdem auf „läuft".**
  Die Aufräumregel arbeitete richtig, wurde aber nur auf frisch gelesene
  Protokolle angewandt — wer alle längst gelesen hat (also jeder im Alltag),
  kam nie dorthin. Gemessen: gespeichert 3 offen, frisch eingelesen nur 2.

  Jetzt wird der gespeicherte Stand bei jedem Start noch einmal gegen die
  jüngsten Protokolle geprüft.

- **Der Bestands-Import kannte die neuere Ausfuhr von scmdb.net nicht** und
  wies sie mit „Diese Datei kenne ich nicht" ab. Die Seite hat ihr Format
  gewechselt; beide werden jetzt gelesen. Übernommen wird nur, was dort als
  erledigt markiert ist.
  Gemeldet von **Zwaersch**.

### Neu

- **Hinweis auf die Grenze der Aufzeichnung** in der Bauplan-Liste. Der Watcher
  kennt nur, was seit seiner Einrichtung im Protokoll stand; Star Citizen
  löscht ältere selbst. Wer vorher gespielt hat, hat Baupläne, von denen das
  Werkzeug nichts weiß.

  Das ist kein Fehler und lässt sich nicht beheben — an 194 Protokollen
  geprüft: **jede** Bauplan-Meldung, die darin stand, ist auch im Bestand
  gelandet. Deshalb steht dort jetzt, was hilft: einmal mit dem Fabricator im
  Spiel abgleichen und von Hand anhaken.

## v3.17.1 - 2026-09-05

> **Die Daten kommen jetzt von dem Weg, der dafür gedacht ist** — und der
> Mensch dahinter wird endlich genannt.

### Verbessert

- **Geholt wird vom GitHub-Spiegel**, den **Krovax** eigens für Programme
  angelegt hat, statt direkt von der Webseite. Ist dort etwas nicht zu finden,
  wird weiterhin die Webseite gefragt — beides ist ausdrücklich erlaubt.
- **Krovax steht jetzt auf der Danke-Seite.** Er hat die Nutzung erlaubt und
  die Daten dafür bereitgestellt; bisher stand dort nur die nackte Lizenz, als
  hätten wir uns bedient.

## v3.17.0 - 2026-09-05

> **Im Auftragstext steht jetzt, WEM der Ruf gutgeschrieben wird — und welcher
> Art.** Nicht mehr nur „150 XP", sondern
> `Headhunters +150 Standing`. Bei Aufträgen, die zwei Parteien bedienen,
> stehen beide da.

### Neu

- **Ruf nach Partei und Art.** Star Citizen kennt sechs Arten — **Standing**,
  Affinity, Bounty Hunting, Hauling, Security und Barter & Trade —, und ein
  Auftrag kann mehreren Organisationen gleichzeitig etwas gutschreiben. Genau
  das steht jetzt im Text:

  ```
  # Ruf: Citizens For Prosperity +100 Standing, Citizens For Prosperity +50 Affinity
  ```

  Gemessen an einer echten Installation: **661 Aufträge** bekommen die Zeile,
  102 davon mit mehr als einer Partei.
  Angeregt von **Bushwick4712**.

- **Die Angaben kommen aus einer zweiten Quelle**, weil die bisherige sie nicht
  hat: Sie kennt die Rufpunkte nur als Zahl, ohne Partei und ohne Art. Geholt
  wird einmal je Spielversion; beim Spieler bleiben davon 71 KB statt 12,5 MB
  liegen. Ohne Netz bleibt es einfach beim letzten Stand.

## v3.16.0 - 2026-09-05

> **Rufpunkte und Abklingzeit stehen jetzt in fast jedem Auftrag — und blau.**
> Bisher bekam sie nur, wer Baupläne im Auftrag hatte; alle anderen gingen leer
> aus, obwohl die Angaben längst vorlagen.

### Neu

- **Auftragsangaben in fast jedem Auftrag.** Rufpunkte, Abklingzeit und
  Teilbarkeit standen bisher nur dort, wo der Auftrag auch Baupläne mitbringt.
  Die Daten liegen aber für **816 von 818** Aufträgen vor — jetzt werden sie
  auch dort eingesetzt, wo es keinen Bauplan gibt. Gemessen an einer echten
  Installation: **von 339 auf 659** Aufträge.
  Gemeldet von **Bushwick4712**.

- **Die Angaben sind hervorgehoben**, in demselben Blau wie die
  `[BP!]`-Marke — sie standen vorher unauffällig mitten im Fließtext und wurden
  übersehen.

### Verbessert

- **Fremde Angaben bleiben stehen.** Wer MrKraken StarStrings zusätzlich
  installiert hat, bekommt seine Reputationszeile nicht doppelt: Wo schon eine
  steht, schreiben wir keine zweite — dieselbe Regel, die für die
  `[BP]`-Marke seit jeher gilt.

## v3.15.10 - 2026-09-05

> **Ein Klick neben ein Eingabefeld beendet die Eingabe** — überall im
> Programm, nicht nur dort, wo es gerade auffiel. Und die Spielzeit zeigt jetzt
> wirklich deine alten Protokolle statt „0 min".

### Behoben

- **Die Spielzeit stand auf „0 min", obwohl Protokolle da waren.** Sie bekam
  nur die Dateien zu sehen, die das Auftrags-Protokoll noch nicht kannte — und
  das kennt auf einem gewachsenen Rechner längst alle. Jetzt liest sie selbst
  nach und merkt sich, was sie hatte.
- **Ein Klick ins Leere beendet die Eingabe.** Bisher blinkte der Cursor
  weiter im Feld, und der eingetippte Text kam nicht im Fehlerbericht an —
  betroffen waren Name und Beschreibung. Das ist jetzt eine Regel des ganzen
  Fensters: Wer neben ein Eingabefeld klickt, ist mit der Eingabe fertig, egal
  welches Feld es war.

### Neu

- **Schalter für die Spielzeit** unter *Einstellungen → Allgemein*, ab Werk
  **aus**. Gezählt wird trotzdem von Anfang an — Star Citizen räumt seine alten
  Protokolle weg, und was weg ist, lässt sich nicht nachholen. Wer die Anzeige
  später einschaltet, hat die Zeit also trotzdem.

## v3.15.9 - 2026-09-05

> **Deine Spielzeit steht jetzt oben in der Leiste** — insgesamt, und während
> du spielst die laufende Sitzung daneben. Gezählt wird aus den Protokollen des
> Spiels und dauerhaft festgehalten: Star Citizen räumt seine alten Logs weg,
> das Gezählte bleibt.

### Neu

- **Spielzeit in der Kopfzeile.** Neben „Sicherung" steht, wie lange du
  gespielt hast, und beim Spielen dahinter in Klammern die laufende Sitzung.
  Frischt sich einmal pro Minute auf.

  Gezählt wird jede Sitzung, in der du wirklich im Spiel warst — ein Start, der
  nie so weit kam, ist keine Spielzeit. Kurze Sitzungen zählen mit; eine Grenze
  wäre eine Behauptung darüber, was „richtiges" Spielen ist.

  Die Zahl beginnt beim ersten Protokoll, das das Werkzeug findet — bei einer
  gewachsenen Installation sind das mehrere Wochen. Ab wann gezählt wird, sagt
  der Hinweis, wenn du mit der Maus darauf zeigst.

- **Eine eigene Datenbank dafür, und sie liegt in der Sicherung.** Star Citizen
  hebt nur eine begrenzte Zahl Protokolle auf; was daraus verschwindet, bleibt
  hier stehen. Beim Rechnerwechsel nimmt der Sicherungs-Knopf sie ohne Zutun
  mit.

### Verbessert

- **README aufgeräumt:** Der Hinweis unter den Bildern behauptete noch v3.0.0,
  Spielzeit und „Fehler melden" fehlten in der Funktionsliste, und zwei
  Einträge trugen ein Schriftzeichen statt eines Symbols.

## v3.15.8 - 2026-09-05

> **Karteileichen räumen sich jetzt auch ohne neuen Auftrag ab.** Wer sich
> ausloggt, ohne abzugeben, und danach eine Runde ohne Aufträge fliegt, hatte
> den alten Eintrag für immer im Protokoll stehen. Das ist vorbei.

### Behoben

- **Ein Auftrag, den es längst nicht mehr gibt, blieb offen stehen.** Bisher
  wurde so ein Eintrag nur abgeräumt, wenn eine spätere Sitzung *andere*
  Aufträge nannte. Wer danach lange spielt und dabei gar keinen Auftrag
  annimmt, kam nie an den Punkt — der Eintrag blieb.

  Jetzt zählt auch das Schweigen: Wer **eineinhalb Stunden im Spiel** ist und
  in der ganzen Zeit keinen einzigen Auftrag im Journal hat, hat keinen.

  Ein kurzer Fehlstart zählt dabei ausdrücklich **nicht** — der nennt oft keinen
  Auftrag, obwohl noch einer läuft. Die Grenze ist an allen vorhandenen
  Protokollen gemessen: Ab einer Stunde wird kein Auftrag mehr fälschlich
  geschlossen; genommen sind eineinhalb, mit Sicherheitsabstand.

## v3.15.7 - 2026-09-05

> **Kein „läuft" mehr bei geschlossenem Spiel.** Wer sich ausloggt, ohne
> abzugeben oder abzubrechen, sah seinen letzten Auftrag für immer als laufend.
> Jetzt heißt er „noch offen" — das stimmt auch am nächsten Morgen.

### Behoben

- **Der letzte Auftrag vor dem Ausloggen stand dauerhaft auf „läuft".** Das
  Spiel schreibt beim Ausloggen kein Ende ins Protokoll, und aufgeräumt wird so
  ein Fall erst, wenn eine spätere Sitzung den Auftrag nicht mehr nennt — die
  gibt es beim letzten aber nicht mehr. Solange das Spiel nicht läuft, steht
  dort jetzt **„noch offen"**.

  Beendet wird der Auftrag dabei ausdrücklich **nicht**: Er ist im Spiel weiter
  angenommen, und beim nächsten Einloggen meldet Star Citizen ihn erneut. Nur
  das Wort war falsch — „läuft" behauptet „jetzt gerade".

## v3.15.6 - 2026-09-05

> **Die Melde-Seite ist zum Schreiben da.** Das Feld für deine Beschreibung ist
> jetzt vier Zeilen hoch und geht über die ganze Breite — du kannst also
> nachlesen, was du meldest, bevor du es abschickst. Absätze bleiben erhalten.

### Verbessert

- **Ein richtiges Textfeld für „Was ist passiert?"** — vier Zeilen, volle
  Breite, unter der Beschreibung statt schmal daneben. Vorher sah man beim
  Tippen nur einen Ausschnitt.
- **Aufzählungen bleiben Aufzählungen.** Wer drei Schritte untereinander
  schreibt, findet sie im Bericht auch untereinander wieder — vorher wurde
  daraus ein Fließtext, und genau die Abfolge ist das Nützliche daran.
- **Der Hinweis „Du siehst vorher genau, was du verschickst" steht jetzt vor
  den Knöpfen**, nicht dahinter. Er beantwortet die Frage, die man sich stellt,
  bevor man klickt — und kann am unteren Rand nicht mehr wegfallen.
- **Die Gruppe „Info" lässt sich nicht mehr zuklappen.** Dort steht „Fehler
  melden" — wer sie wegklappt, blendet den Weg aus, auf dem er ein Problem
  loswird, und sucht ihn dann, wenn ohnehin etwas klemmt. Alle anderen Gruppen
  bleiben klappbar. Hattest du sie zugeklappt, ist sie ab jetzt wieder da.

## v3.15.5 - 2026-09-05

> **Der Fehlerbericht gibt nichts preis, was er nicht darf.** Und das Feld
> „Was ist passiert?" ist jetzt nach jedem Weg wieder leer, nicht nur nach dem
> Absenden.

### Behoben

- **Das Meldungsfeld wird auch von „Angaben kopieren" und „Melden" geleert.**
  Bisher nur beim Absenden — wer anders meldet, hätte seinen Satz von heute
  unbemerkt am nächsten Bericht hängen gehabt. Scheitert das Senden, bleibt er
  stehen.
- **Zugangsdaten verschwinden aus dem Fehlerbericht.** Schlug ein Sendeversuch
  fehl, konnte die Melde-Adresse über das Fehlerprotokoll im Bericht landen —
  und der Bericht wird öffentlich geteilt. Dasselbe gilt jetzt für
  Zugangsschlüssel und Token in Adressen. Die Adressen der Datenabrufe bleiben
  lesbar: Sie sagen, welcher Abruf schiefging, und genau das braucht man.

## v3.15.4 - 2026-09-05

> **Bauplan gefunden, Zahl stimmt.** Bisher meldete das Werkzeug den neuen
> Bauplan zwar — in der Liste stand aber weiter die alte Anzahl, ohne grünen
> Haken. Das ist behoben, und die Kästchen im Spiel ziehen jetzt auch beim
> Spielen mit statt erst Stunden später.

### Behoben

- **Ein neuer Bauplan steht sofort in der Liste** — mit der richtigen Anzahl
  und dem grünen Haken. Bisher zeigte die Liste den Stand von dem Moment, in
  dem du sie zum ersten Mal geöffnet hattest, und auch das Wechseln auf eine
  andere Seite und zurück half nicht.
  Gemeldet von **Bushwick4712**.
- **Vier weitere Seiten zogen ebenso wenig nach**: „Wie weit bin ich",
  „Herstellung", „Sichern & Übertragen" und „Über" zeigten Bauplan-Zahlen vom
  Programmstart. Auch der Fehlerbericht — dort stand der Bestand von damals.
- **Die Kästchen im Spiel hinken nicht mehr hinterher.** Sie wurden nur alle
  sechs Stunden nachgezogen, zusammen mit den Netzabfragen. Wer eine Runde
  spielte und aufhörte, sah einen frisch erhaltenen Bauplan im Auftragstext
  weiter als fehlend. Jetzt wird alle 30 Sekunden geschaut.

### Verbessert

- **Das Feld „Was ist passiert?" ist nach dem Absenden wieder leer.** Sonst
  hinge dein Satz von heute am nächsten Bericht in einer Woche. Scheitert das
  Senden, bleibt er stehen.

## v3.15.3 - 2026-09-05

> **Sag im Fehlerbericht, was passiert ist.** Neben deinem Namen gibt es jetzt
> ein Feld für einen Satz — er steht oben im Bericht, wo er hingehört.

### Neu

- **Feld „Was ist passiert?"** auf der Seite *Fehler melden*. Ein Satz genügt;
  er landet ganz oben im Bericht, noch vor allen technischen Angaben. Wird
  nicht gespeichert — er gehört zu diesem einen Bericht.
  Idee von **Bushwick4712**.

### Verbessert

- **Zwei Knöpfe weniger** auf der Melde-Seite: „Als Datei speichern" und
  „Eigenen Ordner öffnen". Beide hat in über einem Jahr niemand benutzt, und
  beide erzeugten Arbeit statt sie abzunehmen. „Angaben kopieren" tut dasselbe
  in einem Schritt weniger.

### Behoben

- **Das Auftrags-Protokoll frischte beim ersten Öffnen nicht auf.** Wer den
  Watcher morgens startet, mittags Aufträge abgibt und dann zum ersten Mal
  hierher wechselt, sah den Stand vom Programmstart.
  Gemeldet von **Bushwick4712**.

## v3.15.2 - 2026-09-05

> **Routen: Die Eingaben bleiben stehen, wenn du zu den Fahrten rollst.**
> Bisher verschwanden Startort, Frachtraum und Schiff nach oben aus dem Bild —
> auf kleineren Fenstern sofort.

### Behoben

- **Routen: Startort, Frachtraum und Schiff rollten mit weg.** Sie stehen
  jetzt fest, nur die Fahrten darunter rollen. Gemeldet von **Morkhan**.
- **Neben „Beste Routen überall suchen" stand nichts**, solange noch keine
  Handelsposten gesammelt waren. Jetzt steht dort „0 von 184 Handelsposten" —
  so ist überhaupt erkennbar, dass der Knopf etwas sammelt.
- **Routen: Start und Ziel stehen jetzt beide in der Zeile** — „Aluminum ·
  kaufen ab Nyx Gateway → verkaufen in Terra Gateway". Bisher stand dort nur
  das Ziel hinter einem Pfeil, der Einkaufsort ergab sich allein aus der
  Überschrift darüber.
- **Der Fehlerbericht zeigt mehr Seiten** (24 statt 12 Zeilen): Wer den Bericht
  holt, klickt sich vorher durch die Info-Seiten und schob damit genau die
  Seite hinaus, um die es ging.

## v3.15.1 - 2026-09-05

> Kleiner Nachschlag zu v3.15.0.

### Behoben

- **Verkauf: Der Schreibcursor blieb im Mengenfeld stehen**, auch wenn man
  danebengeklickt hat — als wäre man noch am Tippen.

## v3.15.0 - 2026-09-05

> **Läden zeigt jetzt alles, was jemand verkauft — nicht mehr nur, was du
> bauen kannst.** 1.528 Teile statt 893: Raketen, Bomben, Munition,
> Waffenaufsätze, dazu 174 Schiffe zum Kaufen und Mieten. An jeder Zeile
> stehen Klasse, Größe, Güte und Hersteller, und du filterst danach — so
> findest du den Quantenantrieb, der in dein Schiff passt, auch ohne die
> 44 Namen zu kennen.
>
> Bei den Routen funktioniert endlich die Rundreise, und die besten Ketten
> lassen sich über alle Handelsposten suchen. Dazu ein Dutzend Kleinigkeiten,
> die im Alltag mehr ausmachen als sie klingen: Listen, die sich wieder
> schließen, Menüs, die umbrechen statt abzuschneiden, und Daten, die nur noch
> nach einem Patch neu geladen werden statt jede Woche.

### Neu

- **Läden zeigt alles Kaufbare, nicht nur Craftbares.** 1.528 Teile aus 38
  Warengruppen — Raketen, Torpedorohre, Bomben, Waffenaufsätze, Munition. Der
  Boomtube Rocket steht mit seinen 19 Läden in Pyro da, wo man ihn sucht.
- **Schiffe zum Kaufen und Mieten.** 174 Schiffe mit Kaufpreis, Tagesmiete und
  Laderaum, nach Hersteller gruppiert, günstigster Anbieter oben.
- **Klasse, Größe und Güte je Teil.** Militär, Zivil, Industrie, Tarnung,
  Rennsport, Medizin, Bergbau, Bergung — und Güte A bis D. Als Auswahlmenü und
  an jeder Zeile, zusammen mit dem Hersteller.
- **Die Suche findet auch Warengruppen.** „Radar", „Raketen", „Kühler" — du
  musst nicht wissen, ob etwas unter Schiffskomponenten oder Schiffswaffen
  liegt. Deutsche und englische Bezeichnung funktionieren beide.
- **Beste Ketten im ganzen Verse.** Stationszahl, Rundreise und „kurze Strecke"
  wirken auch nach dem Rundumlauf über alle Handelsposten.
- **Hersteller-Auswahl beim Schiff (Routen).** 134 Schiffe mit Laderaum; wer
  den Namen nicht kennt, klickt sich hin, und der Frachtraum kommt von selbst.
- **Menge direkt im Verkauf eintragen.** Jede gewählte Ware hat ihr eigenes
  SCU-Feld — kein Umweg mehr über das Handelslager, wenn du gerade etwas im
  Laderaum hast.
- **„Zurücksetzen" in Läden und Routen.** Ein Klick, und alle Auswahlmenüs,
  Suchfelder und Eingaben stehen wieder auf Anfang.

### Verbessert

- **Die Liste ist nach Warengruppe gegliedert** — Kühler, Quantenantriebe,
  Kraftwerke und Schildgeneratoren mit Überschrift und Anzahl, statt 176 Namen
  am Stück. Eine gewählte Gruppe wird vollständig gezeigt.
- **„Mein Lager" heißt jetzt „Rohstofflager"** — passend zum Handelslager.
- **Läden und Schiffe laden nur nach einem Patch neu**, nicht mehr jede Woche.
  Der Katalog wird beim Programmstart im Hintergrund geholt, damit niemand vor
  einer leeren Liste wartet.
- **Auswahlfelder öffnen sich beim Hineinklicken** und schließen wieder, wenn
  du woanders hinklickst oder Escape drückst.
- **Suchfeld und Auswahlmenüs bleiben beim Rollen stehen.**
- **Ketten nennen Gewinn und Einsatz** — und was du am Anfang vorstrecken musst.
- **Verständlichere Bereichsnamen**: Bordelektronik statt Avionik,
  Schiffskomponenten statt Systeme, Zubehör statt Ausrüstung.
- **Der Änderungsverlauf fasst Patch-Versionen zusammen** — v3.13.0 bis .3
  stehen als eine Reihe „v3.13".

### Behoben

- **Rundreise fand nie eine Route.** Egal ob 2, 3 oder 4 Stationen: Es kam
  immer „keine Route". Jetzt steht da, was du auf dem Rückweg kaufst.
- **Auswahlmenüs werden nicht mehr abgeschnitten**, sondern brechen um.
- **HTML-Zeichen in Namen** — aus „Grey&apos;s Market" ist wieder „Grey's
  Market" geworden.
- **Steuerung: „Auf Werkseinstellung zurücksetzen" wurde abgeschnitten** und
  stand neben vier harmlosen Knöpfen. Jetzt allein in einer eigenen Zeile.
- **Steuerung: Maus, Tastatur und Gamepad verschwanden**, wenn man das Fenster
  kleiner zog.
- **Routen: großer leerer Bereich über den Eingaben**, und die Ortsliste stand
  ganz unten statt unter dem Suchfeld.
- **Läden: Nach dem Klick auf ein Teil blieb der Bildschirm leer.**

## v3.15.0-rc13 - 2026-09-05

> **Die Menge steht jetzt an der Ware selbst.** Jede gewählte Ware hat ihr
> eigenes SCU-Feld — damit gibt es nichts mehr zu verwechseln.

### Behoben

- **Mengen sprangen zwischen den Waren.** Ein einziges Mengenfeld musste
  raten, welche Ware gemeint ist, und riet falsch, sobald du die Zahl vor der
  Ware eingetippt hast. Jetzt hat jede Marke ihr eigenes Feld.
- **Ein Klick ins Leere schließt die Auswahlliste.** Bisher ging sie nur zu,
  wenn du in ein anderes Eingabefeld geklickt hast.
- **Eine Testfassung stand im Änderungsverlauf** (v3.11.1-rc2), obwohl es die
  Version nie gab. Ihr Inhalt steht in v3.12.0.

## v3.15.0-rc12 - 2026-09-05

> **Die Menge im Verkauf wirkt jetzt sofort** — auch wenn du sie nach der Ware
> eintippst. Und die Auswahlmenüs brechen um, statt rechts abgeschnitten zu
> werden.

### Behoben

- **Die eingetippte SCU-Menge wurde nicht übernommen**, wenn die Ware schon
  gewählt war — also im Normalfall. Jetzt zählt sie sofort: Die Marke zeigt
  „Gold · 100 SCU", und die Tabelle rechnet mit 100 statt mit 1.
- **Ware zu Ende tippen und Enter drücken übernimmt sie** — man muss sie nicht
  aus der Liste anklicken.
- **Auswahlmenüs werden nicht mehr abgeschnitten.** Bei fünf Menüs stand rechts
  „Alle Gü…"; jetzt rutscht das letzte in die zweite Zeile. Gilt überall, wo
  es Filtermenüs gibt.

### Verbessert

- Die Beschriftung am Mengenfeld nennt die Ware: „Wie viel SCU Gold hast du?"

## v3.15.0-rc11 - 2026-09-05

> **„Mein Lager" heißt jetzt „Rohstofflager"** — da liegen Rohstoffe drin, und
> es passt zum Handelslager daneben. Dazu: Im Verkauf kannst du die Menge
> direkt eingeben, ohne erst einzulagern.

### Neu

- **Menge direkt im Verkauf eintragen.** Neben dem Suchfeld ein SCU-Feld: Was
  dort steht, gilt für die Ware, die du als Nächstes wählst. Wer 120 SCU Gold
  im Laderaum hat, muss sie nicht erst ins Lager eintragen. Leer lassen geht
  weiter — dann zählt wie bisher das Handelslager.

### Verbessert

- **„Mein Lager" heißt jetzt „Rohstofflager".**
- **Aufgeklappte Auswahllisten schließen wieder**, wenn du woanders hinklickst
  oder Escape drückst.

## v3.15.0-rc10 - 2026-09-05

> **Zurücksetzen für Läden und Routen — und der Laden-Katalog lädt beim Start,
> nicht beim Hinschauen.** Wer den Reiter aufmacht, findet die Daten vor,
> statt auf sie zu warten.

### Neu

- **„Zurücksetzen" in Läden und Routen.** Ein Klick, und alle Auswahlmenüs,
  Suchfelder und Eingaben stehen wieder auf Anfang. Was schon geholt wurde
  (Katalog, gesammelte Handelsposten), bleibt — das wegzuwerfen hieße
  Wartezeit für nichts.

### Verbessert

- **Der Laden-Katalog wird beim Programmstart im Hintergrund geholt.** Bisher
  lief der Abruf erst, wenn man den Reiter öffnete — und dann stand man eine
  Minute vor einer leeren Liste. Dank der Patch-Bindung passiert das höchstens
  einmal je Spielversion.

## v3.15.0-rc9 - 2026-09-05

> **Bei den Ketten steht jetzt dran, was die Zahl bedeutet.** Gewinn, was du
> am Anfang vorstrecken musst, und je Schritt der Einsatz.

### Verbessert

- **Ketten nennen Gewinn und Einsatz.** Über einer Tour stand nur eine nackte
  Zahl. Jetzt: „Gewinn 544.620 aUEC" und darunter „Dafür brauchst du am Anfang
  495.900 aUEC" — nur die erste Fahrt zahlst du aus eigener Tasche, ab der
  zweiten kaufst du vom Erlös der vorigen.
- **Jeder Schritt nennt seinen Einsatz** — so siehst du, welcher Schritt das
  Geld bindet.

## v3.15.0-rc8 - 2026-09-05

> **Namen mit Apostroph stehen wieder richtig da.** Aus „Grey&apos;s Market"
> ist wieder „Grey's Market" geworden — überall, wo Namen aus der Preisquelle
> kommen.

### Behoben

- **HTML-Zeichen in Namen.** Apostrophe, Anführungszeichen und das
  kaufmännische Und kamen als `&apos;`, `&quot;` und `&amp;` an. Betroffen
  waren Hersteller, Teile, Terminals und Orte.

## v3.15.0-rc7 - 2026-09-05

> **Die Schalter gelten jetzt auch für „Beste Routen überall".** Rundreise über
> drei Stationen, kurze Strecke — das rechnet er jetzt über alle Handelsposten,
> statt weiter die Einzelfahrten von vorhin zu zeigen.

### Neu

- **Beste Ketten im ganzen Verse.** Stationszahl, Rundreise und „kurze
  Strecke" wirken auch nach dem Rundumlauf. Gerechnet wird mit dem, was schon
  gesammelt ist — in unter einer Sekunde.

### Behoben

- **Die Ortsliste stand ganz unten statt unter dem Suchfeld.** Wer „sera"
  tippte, fand den Vorschlag weit weg von der Eingabe.
- **Das Ortsfeld öffnet beim Hineinklicken die Handelsposten** — ohne dass man
  vorher rechts ein System wählen muss.

## v3.15.0-rc6 - 2026-09-05

> **Ein Klick ins Feld, und die Auswahl klappt auf.** Bisher musste man den
> kleinen Pfeil am rechten Rand finden — den sucht niemand.

### Verbessert

- **Auswahlfelder öffnen sich beim Hineinklicken.** Handelslager (Ware,
  Lagerort) und Verkauf: erst die ganze Liste, beim Tippen auf die Treffer
  eingedampft, und wenn du das Feld wieder leerst, steht sie wieder ganz da.

## v3.15.0-rc5 - 2026-09-05

> **Die Rundreise funktioniert.** Bisher kam bei jeder Stationszahl „keine
> Route" — jetzt bekommst du die Fahrt zurück zum Start samt Ware. Und die
> Listen laden nur noch neu, wenn wirklich ein Patch da war, statt jede Woche.

### Behoben

- **Rundreise fand nie eine Route.** Egal ob 2, 3 oder 4 Stationen: Es kam
  immer „Dafür findet sich gerade keine Route". Jetzt steht da, was du auf dem
  Rückweg kaufst.
- **Routen: Der Suchtext blieb beim Herstellerwechsel stehen** und blockierte
  die neue Auswahl.
- **Läden zeigte während des ersten Abrufs die falsche Liste** — die alten
  Bauplan-Arten statt der Warengruppen, ohne Schiffe. Jetzt steht dort nur der
  Hinweis, bis die Daten da sind.

### Verbessert

- **Läden und Schiffe laden nur nach einem Patch neu.** Ladenpreise,
  Schiffspreise und der Warengruppen-Katalog ändern sich mit einer neuen
  Spielversion, nicht mit dem Kalender — bisher wurde ein noch gültiger Stand
  jede Woche weggeworfen und die knappe Minute Wartezeit fiel umsonst an.
- Das Menü beim Schiff heißt jetzt „Hersteller", wie überall sonst.

## v3.15.0-rc4 - 2026-09-05

> **Klasse und Güte sind da — jetzt findet man das passende Teil.** Jede Zeile
> zeigt Klasse, Größe, Güte und Hersteller, und für die ersten drei gibt es
> Auswahlmenüs. Dazu bei Routen ein Werft-Menü fürs Schiff.

### Neu

- **Klasse und Güte je Teil.** Militär, Zivil, Industrie, Tarnung, Rennsport,
  Medizin, Bergbau, Bergung — und Güte A bis D. Beides als Auswahlmenü und an
  jeder Zeile, zusammen mit Größe und Hersteller.
- **Werft-Auswahl beim Schiff (Routen).** 134 Schiffe mit Laderaum, nach 15
  Werften sortiert. Wer den Namen nicht kennt, klickt sich hin; der Frachtraum
  steht an jeder Zeile und wird beim Auswählen übernommen.

### Behoben

- **Routen: großer leerer Bereich über den Eingaben.** Die Vorschlagslisten
  gaben ihren Platz nicht frei, wenn sie leer waren.
- **Routen: Der Hinweis nennt jetzt beide Wege** — Startort eintippen *oder*
  „Beste Routen überall suchen" drücken.

## v3.15.0-rc3 - 2026-09-05

> **Jetzt findet man auch, was man nicht beim Namen kennt.** An jeder Zeile
> stehen Größe und Hersteller, es gibt einen Größen-Filter, und eine gewählte
> Warengruppe wird vollständig gezeigt statt bei 40 Zeilen abzubrechen. Dazu
> zwei Ecken im Steuerungs-Reiter, an denen Knöpfe abgeschnitten wurden.

### Neu

- **Größe und Hersteller stehen an jeder Zeile.** Eine Liste aus 44
  Fantasienamen sagt niemandem etwas — „Größe 1 · Wen-Cassel" schon. So findet
  man den Quantenantrieb, der ins eigene Schiff passt.
- **Filter nach Größe.** Geschütze Größe 3, Kühler Größe 1 — was nicht
  hineinpasst, steht gar nicht erst da.

### Verbessert

- **Eine gewählte Warengruppe wird vollständig gezeigt.** Alle 87 Geschütze,
  alle 201 Helme — kein Abbruch bei 40 Zeilen mehr.
- **„… 38 weitere in dieser Gruppe" ist anklickbar** und öffnet die Gruppe.
- **Zwölf Zeilen je Gruppe** in der Übersicht statt sechs.
- Kein Hersteller-Menü mehr — der Hersteller steht an der Zeile, und die Suche
  findet ihn.

### Behoben

- **Steuerung: „Auf Werkseinstellung zurücksetzen" wurde abgeschnitten.** Der
  Knopf steht jetzt allein in einer eigenen Zeile — sichtbar, und nicht mehr
  direkt neben vier harmlosen Knöpfen.
- **Steuerung: Maus, Tastatur und Gamepad verschwanden, wenn man das Fenster
  kleiner zog.** Die Knopfreihen brechen jetzt um, statt rechts abgeschnitten
  zu werden.

## v3.15.0-rc2 - 2026-09-05

> **Läden ist jetzt zu gebrauchen.** Die Liste gliedert sich nach Warengruppe,
> die Suche findet auch Gruppennamen — tipp „Radar", und du bekommst die Radare,
> ohne zu wissen, wo sie einsortiert sind. Suchfeld und Auswahl bleiben beim
> Rollen stehen. Und **Schiffe** sind dazugekommen: Kaufpreis, Mietpreis pro Tag
> und Laderaum, nach Werft sortiert.

### Neu

- **Schiffe zum Kaufen und Mieten.** 174 Schiffe mit Kaufpreis, Tagesmiete und
  Laderaum — nach Werft gruppiert, günstigster Anbieter oben.
- **Die Suche findet auch Warengruppen.** „Radar", „Raketen", „Kühler" — du
  musst nicht wissen, ob etwas unter Schiffskomponenten oder Schiffswaffen
  liegt. Deutsche und englische Bezeichnung funktionieren beide.

### Verbessert

- **Die Liste ist nach Warengruppe gegliedert.** Statt 176 Namen am Stück
  stehen Kühler, Quantenantriebe, Kraftwerke und Schildgeneratoren mit
  Überschrift und Anzahl untereinander — von jeder Gruppe ein paar Zeilen,
  darunter wie viele noch folgen.
- **Suchfeld und Auswahlmenüs bleiben beim Rollen stehen.**
- **Die Auswahl passt zusammen.** Warengruppen, die es im gewählten Bereich
  nicht gibt, werden nicht mehr angeboten.
- **Verständlichere Bereichsnamen**: Bordelektronik statt Avionik,
  Schiffskomponenten statt Systeme, Zubehör statt Ausrüstung.

### Behoben

- **Beim Tippen verschwindet die vorige Antwort.** Vorher blieb sie stehen, und
  es sah aus, als reagiere das Suchfeld nicht.
- **Eine abgeschnittene Liste sagt es jetzt.** Vorher endete sie stumm bei 40
  Zeilen und wirkte, als gäbe es nicht mehr.

## v3.15.0-rc1 - 2026-09-04

> Der Reiter **Läden** zeigte bisher nur Teile, für die es einen Bauplan gibt.
> Wer wissen wollte, wo es Raketen, Bomben oder Railgun-Munition gibt, stand vor
> einer leeren Liste — obwohl die Läden längst bekannt waren. Jetzt steht dort,
> was tatsächlich verkauft wird: **1.528 Teile statt 893**, sortiert nach
> Bereich und Warengruppe.

### Neu

- **Läden zeigt alles Kaufbare, nicht nur Craftbares.** Raketen, Torpedorohre,
  Bomben, Waffenaufsätze, Munition — 1.528 Teile aus 38 Warengruppen. Der
  Boomtube Rocket steht jetzt mit seinen 19 Läden in Pyro da, wo man ihn sucht.
- **Zwei Auswahlmenüs statt einem**: erst der Bereich (Rüstung, Schiffswaffen,
  Systeme …), dann die Warengruppe darin. Beide auf Deutsch, größte Gruppe oben.
- Unter dem Filter steht, **wie viele Teile bereitstehen** — statt einer leeren
  Fläche, die nach einem Fehler aussieht.

### Behoben

- **Läden: Nach dem Klick auf ein Teil blieb der Bildschirm leer.** Die
  Vorschlagsliste gab ihren Platz nicht frei, die Läden wurden darunter
  gezeichnet — außerhalb des Fensters. Jetzt springt die Ansicht nach der
  Auswahl an den Anfang.
- **Teile ohne Entitäts-Kennung sind erreichbar.** Rund ein Drittel des
  Katalogs hat keine; für die gab es vorher grundsätzlich keine Ladenpreise.

## v3.14.0 - 2026-09-04

> Die größte Version bisher. Deine **Steuerung** gehört jetzt ins Werkzeug:
> Belegungen ansehen, ändern, als Profil speichern — und die Sicherung nimmt
> sie endlich mit. Dazu vier neue Antworten rund um Geld: was ein Teil im Laden
> kostet, ob Selberbauen sich lohnt, welche Handelsroute am meisten einbringt,
> und wo dein nächstes Schiff steht.
>
> Und über allem eine Regel, die vorher fehlte: Das Werkzeug sagt jetzt dazu,
> wenn es etwas nicht sicher weiß — nach einem Patch, bei alten Preisen, bei
> Lücken in den Daten.

### Neu

- **Steuerung im Werkzeug.** Welcher Stick welche Nummer hat, was darauf liegt,
  was noch frei ist — mit Klarnamen statt `v_eject`. Belegen per Knopfdruck,
  Konfliktwarnung, Zurücksetzen auf Werkseinstellung.
- **Belegung als Profil speichern.** Unter einem Namen, den du wählst, dort wo
  Star Citizen es sucht — im Spiel ladbar mit `pp_rebindkeys load <Name>`.
  Einspielen geht aus derselben Liste, ohne Dateisuche.
- **Die Sicherung nimmt deine Steuerung mit.** Aktive Belegung und alle
  Profile. Zurückgespielt wird nur, wenn du es verlangst.
- **Reiter „Läden".** Wo ein fertiges Teil im Regal steht und was es kostet,
  gezeigt wird nur, was irgendwo verkauft wird. In der Herstellung steht der
  Ladenpreis neben den Zutatenkosten — damit beantwortet sich „lohnt Bauen?"
  von selbst.
- **Reiter „Routen".** Standort, Frachtraum und Geld angeben und die
  lohnendsten Fahrten bekommen — einzeln, als Kette über bis zu vier Stationen
  oder als Rundreise. Wahlweise nach Gewinn oder kurzer Strecke. Vorgeschlagen
  von **YoshimitsuDE**.
- **Schiff statt Zahl.** Wähl dein Schiff und der Frachtraum trägt sich ein;
  dazu siehst du, wo es zu kaufen und zu mieten ist.
- **Rufpunkte, Abklingzeit, Teilbarkeit und Bauplan-Chance im Auftragstext.**
  Die Angaben lagen in den Daten, standen aber nirgends im Spiel.
  Vorgeschlagen von **Bushwick4712**.
- **Warnung nach einem Patch.** Stammen die Preise noch aus der Version davor,
  steht das dabei — mit beiden Versionsnummern.
- **Volle Lager werden angezeigt.** Ein Terminal ohne Bedarf nimmt deine
  Ladung nicht ab, auch wenn der Preis noch dransteht.

### Verbessert

- **Jede Zahl sagt, was sie ist.** Gewinn, Einsatz, verfügbare Menge — und
  woran die Menge hängt: mehr Geld, größeres Schiff oder schlicht der Vorrat
  vor Ort.
- **Auswahlmenüs überall gleich.** Suchfeld mit Vorschlägen ab zwei Buchstaben,
  Dropdowns nach Größe sortiert.

### Behoben

- **Text neben einem Symbol war schwarz auf dunklem Grund.**
- **Ausgegebene Belegungen ließen sich im Spiel nicht laden** — sie hatten
  nicht das Format, das Star Citizen für Profile erwartet.
- **Für viele Teile fehlten die Ladenpreise**, obwohl es sie im Spiel gibt.
- **Das Fehlerprotokoll meldete zwei Probleme, die es nicht gab.**

## v3.14.0-rc20 - 2026-09-04

> „Zeigt nur noch, was kaufbar ist" — und dann standen doch alle Teile da. Der
> Filter arbeitete richtig, brauchte aber eine Minute, und in der Zwischenzeit
> sagte nichts, dass er noch läuft.

### Behoben

- **In „Läden" standen weiter Teile, die nirgends verkauft werden.** Der
  Watcher sieht das beim ersten Öffnen einmal nach, und das dauert etwa eine
  Minute — bis dahin blieb die vollständige Liste stehen, ohne dass ein
  Hinweis darauf zu sehen war. Der Hinweis steht jetzt oben statt unter der
  Liste, und die Zahlen in den Auswahlmenüs werden danach berichtigt.

## v3.14.0-rc19 - 2026-09-04

> Zwei Fehler, die keine waren: Das Fehlerprotokoll meldete bei jedem Abruf
> der Preisdaten zwei erfundene Probleme. Ein Protokoll voller falscher Alarme
> liest bald niemand mehr — und der echte Fehler geht darin unter.

### Behoben

- **Das Fehlerprotokoll meldete zwei Probleme, die es nicht gab.** Bei jedem
  Abruf der Verkaufsdaten standen dort zwei Einträge über angeblich
  abgeschnittene Antworten — tatsächlich waren sie vollständig.

## v3.14.0-rc18 - 2026-09-04

> Der Auftragstext im Spiel verrät jetzt, was eine Mission einbringt und wann
> sie wieder verfügbar ist. Und der Reiter „Läden" zeigt nur noch, was es
> wirklich zu kaufen gibt — mit Schiffswaffen und FPS-Waffen als getrennte
> Gruppen.

### Neu

- **Rufpunkte, Abklingzeit, Teilbarkeit und Bauplan-Chance stehen jetzt im
  Auftragstext.** Die Angaben lagen längst in den Daten, standen aber nirgends
  im Spiel — du siehst jetzt vor dem Annehmen, was die Mission einbringt, wann
  sie wieder verfügbar ist, ob sie geteilt werden kann und wie
  wahrscheinlich ein Bauplan fällt. Vorgeschlagen von **Bushwick4712**.
- **Schiffswaffen und FPS-Waffen sind getrennte Gruppen.** Vorher standen 270
  Stück in einem Topf „Waffen" — wer ein Geschütz fürs Schiff sucht, sucht
  nicht dieselbe Liste wie jemand, der ein Gewehr braucht.

### Verbessert

- **„Läden" zeigt nur noch, was irgendwo verkauft wird.** Von 1.597 Teilen
  bleiben 893 — der Rest ist nirgends im Handel und hätte dich nur ins Leere
  klicken lassen. Beim ersten Öffnen sieht der Watcher dafür einmal nach; das
  dauert etwa eine Minute und zeigt seinen Fortschritt.
- **Die Auswahlmenüs sind nach Größe sortiert.** Die größten Gruppen standen
  alphabetisch ganz hinten und waren nur durch Rollen erreichbar — man hielt
  sie für nicht vorhanden.

## v3.14.0-rc17 - 2026-09-04

> Eine Route sagt jetzt nicht nur, was du verdienst, sondern auch was du dafür
> hinlegen musst — und woran die Menge hängt. Bei 69 von 120 SCU ist das der
> Unterschied zwischen „größeres Schiff kaufen" und „mehr Geld mitbringen".

### Neu

- **Der Einsatz steht bei jeder Fahrt.** Was du an Einkauf vorstrecken musst,
  und wie viel an dem Ort überhaupt liegt.
- **Woran die Menge hängt.** Statt nur „69 SCU" steht jetzt dabei, ob mehr
  Geld, ein größeres Schiff oder schlicht der Vorrat vor Ort die Grenze ist.

### Verbessert

- **Die Spalten sind beschriftet.** Die große Zahl ist der Gewinn — das stand
  nirgends und ließ sich leicht für den Einkaufspreis halten.
- **„Kaufen ab …" statt nur „ab …"**, damit klar ist, welcher der beiden Orte
  der Einkauf ist.

### Behoben

- **Es stand „187 von 184 Handelsposten" da** — mehr, als es gibt. Gezählt
  wurden auch Orte, die seit einer früheren Fassung im Zwischenspeicher lagen
  und gar keine Handelsposten sind.

## v3.14.0-rc16 - 2026-09-04

> Die Suche nach der besten Route zeigt ihr Ergebnis jetzt auch an — bisher
> lief sie durch und danach stand die Seite unverändert da. Dazu ein
> Systemauswahl-Menü bei den Routen, das Schiff als Suchfeld statt als
> Extrafenster, und eine Bestenliste im Verkauf, die nicht mehr leer bleibt.

### Neu

- **Systemauswahl bei „Wo stehst du gerade?".** Klapp Stanton, Pyro oder Nyx
  auf und du siehst die Handelsposten darin, ohne etwas tippen zu müssen.

### Verbessert

- **Das Schiff wählst du jetzt im Suchfeld**, nicht mehr in einem eigenen
  Fenster — mit Vorschlägen ab zwei Buchstaben, wie überall sonst im Werkzeug.
  Der Frachtraum steht in jeder Vorschlagszeile.
- **Die Umschalter für Gewinn, Stationen und Rundreise sind von Anfang an
  sichtbar.** Vorher erschienen sie erst, wenn schon ein Ort gewählt war — wer
  die Seite zum ersten Mal öffnete, sah nie, dass es sie gibt.

### Behoben

- **Die Suche nach der besten Route zeigte nichts an.** Sie sammelte alle
  Handelsposten ein und danach stand weiter „Tippe oben ein, wo du gerade
  bist" da.
- **Im Verkauf blieb die Liste „Was gerade am besten zahlt" leer.** Die
  Überschrift war da, die Zeilen fehlten.
- **Das Schiff-Fenster zeigte nur sieben von 134 Schiffen** und verschwieg den
  Rest.

## v3.14.0-rc15 - 2026-09-04

> Eine Route sagt jetzt, wo du was einkaufst — vorher stand nur das Ziel da
> und man musste sich den Rest denken. Dazu die Suche nach der besten Route
> überhaupt, und zwei Seiten, die nicht mehr leer beginnen.

### Neu

- **Die beste Route überhaupt, egal von wo nach wo.** Ein Knopf sammelt die
  Fahrten aller Handelsposten ein und zeigt die lohnendsten. Das dauert rund
  anderthalb Minuten und läuft nur, wenn du es anstößt — mit Fortschritt.
- **Der Reiter „Läden" hat jetzt Auswahlmenüs** für Art und Hersteller, wie du
  sie aus der Bauplan-Liste kennst. Ohne Suchbegriff füllen sie die Liste.
- **„Verkauf" zeigt ohne Auswahl, was gerade am besten zahlt** — die zwölf
  bestbezahlten Waren, anklickbar zum Übernehmen.

### Verbessert

- **Jeder Schritt einer Route nennt beide Orte.** „In Seraphim: 120 SCU Copper
  kaufen → verkaufen in Rat's Nest" statt nur „Copper → Rat's Nest". Über der
  Route steht zusätzlich der ganze Weg.

### Behoben

- **Nach der Ortswahl klaffte oben eine Lücke.** Die Vorschlagsliste
  verschwand, die Ansicht blieb aber stehen, wo sie war.

## v3.14.0-rc14 - 2026-09-04

> Die Ortsauswahl bei den Routen zeigt jetzt nur noch Stellen, an denen
> wirklich mit Ware gehandelt wird. Seraphim Station stand vorher mit sechzehn
> Zeilen da — fünfzehn davon waren Klamottenläden, Imbisse und Tankstellen.

### Behoben

- **In der Ortsauswahl standen Läden, Imbisse und Tankstellen.** Wer sie
  auswählte, bekam keine Route — dort gibt es nichts zu handeln. Angeboten
  werden jetzt nur noch die Stellen, die tatsächlich Ware kaufen und
  verkaufen: 184 statt 826.

## v3.14.0-rc13 - 2026-09-04

> Wähl einfach dein Schiff — der Frachtraum trägt sich dann von selbst ein.
> Und wenn du es noch nicht hast, siehst du gleich, wo es zu kaufen oder zu
> mieten ist.

### Neu

- **Schiff auswählen statt Frachtraum tippen.** 134 Schiffe mit Laderaum, die
  SCU-Zahl steht in der Liste gleich dabei. Nach der Wahl siehst du außerdem,
  wo dieses Schiff am günstigsten zu kaufen und zu mieten ist.

## v3.14.0-rc12 - 2026-09-04

> Routen können jetzt über drei oder vier Stationen gehen — und auf Wunsch
> dorthin zurückführen, wo du gestartet bist. Dazu vier Nachbesserungen an
> Stellen, die im ersten Anlauf zu wenig verrieten.

### Neu

- **Routen über mehrere Stationen, auch als Rundreise.** Wähle zwei, drei oder
  vier Stationen — und ob die Route dort enden soll, wo sie anfing. Eine
  Rundreise lässt sich wiederholen, statt dich mit leerem Laderaum irgendwo
  stehen zu lassen.

### Verbessert

- **Beträge stehen jetzt mit ihrer Einheit da.** Vorher war es eine nackte
  Zahl zwischen SCU-Mengen und Entfernungen — man konnte nur raten, was
  gemeint war.
- **Ankaufstellen sind unterscheidbar.** Eine Station hat viele Terminals, und
  die handeln mit Verschiedenem. Vorher stand achtmal derselbe Stationsname
  untereinander; jetzt steht das Terminal davor. Gesucht wird in beidem.
- **Der gewählte Ort bleibt stehen.** Er verschwand vorher aus dem Feld,
  sobald man ihn angeklickt hatte.

### Behoben

- **Für viele Teile fehlten die Ladenpreise, obwohl es sie im Spiel gibt.**
  Unsere Preisquelle führt manche Gegenstände unter einer anderen Kennung als
  das Spiel — für die wird jetzt zusätzlich über den vollständigen Namen
  gesucht. Bei den CF-Repeatern etwa haben statt zwei nun sechs von neun einen
  Preis.

## v3.14.0-rc11 - 2026-09-04

> Sag dem Watcher, wo du stehst und was in deinen Laderaum passt — und er
> sagt dir, womit sich die nächste Fahrt lohnt. Auf Wunsch gleich über zwei
> Stationen, wahlweise nach bestem Gewinn oder nach kurzer Strecke.

### Neu

- **Neuer Reiter „Routen".** Du gibst deinen Standort, deinen Frachtraum und
  dein Geld an und bekommst die lohnendsten Fahrten — einzeln oder als Kette
  über zwei Stationen. Umschaltbar zwischen bestem Gewinn und kurzer Strecke.
  Vorgeschlagen von **YoshimitsuDE**.
- **Gerechnet wird mit dem, was wirklich geht.** Nicht nur der Laderaum
  begrenzt die Menge, sondern auch der Vorrat am Kaufort und dein Geld — sonst
  stünde da ein Gewinn für eine Ladung, die es gar nicht zu kaufen gibt.

## v3.14.0-rc10 - 2026-09-04

> Bauen oder kaufen? Diese Frage beantwortet der Watcher jetzt selbst. Beim
> Aufklappen eines Bauplans steht dabei, was dasselbe Teil fertig im Laden
> kostet — und der neue Reiter **Läden** sagt dir, wo es im Regal liegt.

### Neu

- **„Fertig kaufen" in der Herstellung.** Über den Zutaten steht jetzt, was
  das Teil im Laden kostet und wo es am billigsten ist. Zusammen mit den
  Zutatenkosten darunter beantwortet sich damit von selbst, ob sich der
  Aufwand lohnt.
- **Neuer Reiter „Läden".** Namen eintippen, Teil anklicken — und du siehst
  alle Verkaufsstellen mit Preis, Ort und Zustand der Ware. Der günstigste
  steht oben.

### Verbessert

- **Zugeordnet wird über die Kennung des Gegenstands, nicht über den Namen.**
  Damit kann kein ähnlich heißendes Teil dazwischenrutschen.

## v3.14.0-rc9 - 2026-09-04

> Der Verkaufs-Reiter sagt jetzt dazu, was er nicht weiß. Nach einem Patch
> siehst du auf einen Blick, dass die Preise noch aus der Version davor
> stammen — und ein Terminal, dessen Lager volläuft, sagt es dir, bevor du
> hinfliegst.

### Neu

- **Warnung nach einem Spiel-Patch.** Star Citizen wirft mit jedem Patch
  Preise um. Stammen die Zahlen noch aus der Version davor, steht das jetzt
  dabei — mit beiden Versionsnummern, statt nur „veraltet".
- **Volle Lager werden angezeigt.** Ein Terminal, dessen Lager sich füllt oder
  schon voll ist, nimmt deine Ladung schlecht oder gar nicht mehr ab — obwohl
  der Preis noch dransteht. Das steht jetzt an der betroffenen Ware.

### Verbessert

- **Die Anzeige bleibt ruhig, wenn es nichts zu sagen gibt.** Über neun
  Zehntel aller Ankaufstellen haben Platz — dort erscheint bewusst kein
  Zeichen. Ein Hinweis, der immer da ist, wird nicht gelesen.

## v3.14.0-rc8 - 2026-09-04

> Der Weg zu deinen Profilen ist jetzt kurz: Beim Speichern siehst du, was
> schon da ist, und ein Klick übernimmt den Namen. Beim Einspielen wählst du
> dein Profil aus einer Liste, statt es im Dateisystem zu suchen.

### Neu

- **Profil einspielen ohne Suchen.** Beim Einspielen kannst du zwischen deinen
  gespeicherten Profilen wählen — anklicken genügt. Für eine Belegung, die dir
  jemand geschickt hat, bleibt der Dateiwähler daneben stehen.

### Verbessert

- **Die Namensabfrage passt jetzt zum Programm.** Sie kam vorher als grauer
  Systemdialog mit englischer Beschriftung. Jetzt hat sie dieselben Farben wie
  alles andere — und die vorhandenen Profile stehen untereinander statt in
  einer Zeile, die rechts aus dem Fenster lief. Ein Klick auf einen Namen
  übernimmt ihn.

## v3.14.0-rc7 - 2026-09-04

> Deine Belegung bleibt jetzt bei dir. Du kannst sie unter einem eigenen Namen
> als Profil speichern — dort, wo Star Citizen es sucht, und im Spiel mit einer
> Zeile wieder ladbar. Und die große Sicherung nimmt sie endlich mit: die
> aktive Belegung und jedes gespeicherte Profil.

### Neu

- **Belegung als Profil speichern, mit eigenem Namen.** Das Profil landet dort,
  wo Star Citizen es sucht — im Spiel zu laden mit `pp_rebindkeys load <Name>`.
  Bisher entstand nur eine Kopie, die man von Hand zurückschieben musste.

### Verbessert

- **Die Sicherung nimmt jetzt auch deine Steuerung mit** — die aktive Belegung
  und alle gespeicherten Profile. Zurückgespielt wird sie nur, wenn du es
  ausdrücklich verlangst; die aktive Belegung sogar nur auf gesonderte
  Nachfrage, und der alte Stand wird vorher zur Seite gelegt.

### Behoben

- **Eine ausgegebene Belegung ließ sich im Spiel nicht laden.** Die Datei hatte
  nicht das Format, das Star Citizen für Profile erwartet. Umgekehrt wird eine
  eingelesene Belegung jetzt ebenfalls richtig umgesetzt.
- **Belegungsprofile wurden übersehen, wenn der Ordner in zwei Schreibweisen
  vorlag.** Auf manchen Installationen gibt es `controls` und `Controls`
  nebeneinander; die Profile aus beiden werden jetzt zusammengeführt.

- **Text neben einem Symbol war schwarz auf dunklem Grund und dadurch kaum zu
  lesen.** Sichtbar im Herkunfts-Block eines Bauplans an der Zeile „n weitere
  Wege zu diesem Bauplan". Betroffen war jede Zeile, die ein Symbol mit einem
  Wort daneben zeigt.

## v3.14.0-rc6 - 2026-09-04

> Ein Fehler, der die halbe Werkseinstellung verschluckt hat: Wer eine Aktion
> auf den Stick legte, verlor in der Anzeige ihre Taste. Scheinwerfer,
> Respawn, Hocken und die linke Maustaste standen dadurch unter „noch nicht
> belegt", obwohl sie längst belegt sind.

### Behoben

- **Eine eigene Belegung verdrängte die Werkseinstellung auf allen Geräten
  statt nur auf dem eigenen.** Das Spiel macht es anders: Legst du „Respawn"
  auf einen Stick-Knopf, bleibt die Taste `F` trotzdem aktiv. Jetzt auch hier.
  Die Gesamtansicht zeigt dadurch **572 statt 326** Belegungen, und die Liste
  „noch nicht belegt" ist von 444 auf 310 geschrumpft — der Rest war nie frei.

## v3.14.0-rc5 - 2026-09-04

> Sichern, einspielen, zurücksetzen: Deine Belegung lässt sich jetzt als Datei
> wegschreiben und wieder einlesen — den Umweg über die Spielkonsole brauchst
> du dafür nicht mehr.

### Neu

- **Belegung sichern und einspielen.** Als `actionmaps.xml` zum Aufheben oder
  Weitergeben, oder als **CSV-Liste** zum Nachschlagen und Ausdrucken. Bisher
  ging das nur über `pp_rebindkeys export` in der Spielkonsole.
- **Auf Werkseinstellung zurücksetzen.** Ein Knopf, rot und mit ausdrücklicher
  Rückfrage, die auch sagt, wie viele eigene Belegungen betroffen sind.
  Totzonen, Kurven und Empfindlichkeit bleiben dabei unangetastet — das sind
  Geräteeinstellungen, keine Belegung. Vorher entsteht eine Sicherung.

### Verbessert

- **„X-Achse" statt „Achse X"** — im Deutschen herum, wie man es sagt.

## v3.14.0-rc4 - 2026-09-04

> Die Liste liest sich jetzt wie ein Satz statt wie ein Datenauszug: Statt
> `js2 · x · v_boost` steht dort der Name deines Sticks, „Achse X" und
> „Boost". Und die 444 Aktionen, für die es ab Werk keine Taste gibt, sind
> jetzt überhaupt erreichbar.

### Neu

- **Aktionen ohne jede Belegung** haben eine eigene Ansicht bekommen — *Noch
  nicht belegt*. Ohne sie kam man an sie nicht heran: Was nirgends belegt ist,
  stand in keiner Liste, und was in keiner Liste steht, kann man auch nicht
  anklicken, um es zu belegen. Betrifft **444 Aktionen** — Emotes, Bergbau-
  Feinheiten, Notfallbefehle.

### Verbessert

- **Der Gerätename steht in der Liste, nicht `js1`.** Die Nummer sagt nur dem
  Spiel etwas; wer davorsitzt, will wissen, ob das der linke oder der rechte
  Stick ist.
- **Eingaben im Klartext.** Aus `x` wird „Achse X", aus `button23` „Knopf 23",
  aus `hat1_up` „Hut 1 ↑", aus `lshift` „Umschalt links". Wichtig bei den
  Achsen: `x` allein las sich wie die Taste X.
- **Mehr Aktionen mit Namen.** Fehlt die deutsche Bezeichnung, wird jetzt die
  englische genommen statt gar keine. Für die 382 Aktionen, denen das Spiel
  selbst keinen Namen gibt, steht ein aufbereiteter Name da (`v_boost` →
  „Boost") — grau, damit der Unterschied zu einer echten Bezeichnung sichtbar
  bleibt.
- Dieselbe Belegung erscheint nicht mehr doppelt, wenn die Aktion in mehreren
  Gruppen des Spiels steht.

## v3.14.0-rc3 - 2026-09-04

> Jetzt kannst du auch belegen: Zeile anklicken, Knopf oder Taste drücken,
> übernehmen. Stick, Tastatur und Maus gleichermaßen — und wenn die Eingabe
> schon vergeben ist, steht das vorher da statt hinterher im Gefecht.

### Neu

- **Belegen per Knopfdruck.** Eine Zeile in der Liste anklicken, dann die
  Taste oder den Knopf drücken, den du haben willst — die Nummer musst du
  nicht kennen. Erkannt werden Joystick-Knöpfe und -Achsen, Tastatur, Maus
  und Mausrad.
- **Warnung bei doppelter Belegung.** Liegt auf der Eingabe schon etwas
  anderes, steht das im Fenster, **bevor** du übernimmst. Doppelt belegen
  darfst du trotzdem — es ist manchmal gewollt.
- **Belegung entfernen.** Auch das kennt das Spiel als eigenen Zustand: Eine
  entfernte Belegung bleibt entfernt und wird nicht durch die
  Werkseinstellung ersetzt.

### Verbessert

- Vor jeder Änderung an der Belegungsdatei entsteht eine Sicherung daneben.
  Der Rückweg ist ein Umbenennen, kein Neuaufbau der ganzen Steuerung.

## v3.14.0-rc2 - 2026-09-04

> Die Joystick-Seite spricht jetzt Klartext: Statt `v_eject` steht dort
> „Schleudersitz", und zwar in der Sprache, in der du das Werkzeug bedienst.
> Tastatur, Maus und Gamepad sind mit dabei, und du kannst zwischen deinen
> eigenen Änderungen, der Werkseinstellung und beidem zusammen umschalten.

### Neu

- **Aktionen im Klartext, deutsch und englisch.** Die Belegungsliste zeigt
  nicht mehr `v_eject`, sondern „Schleudersitz" bzw. „Eject" — je nachdem,
  welche Sprache im Werkzeug eingestellt ist (nicht im Spiel). Die
  Bezeichnungen kommen aus dem Spiel selbst, es wird nichts übersetzt und
  nichts geraten: Wo das Spiel keinen Namen vergibt, steht weiter der
  technische. Gesucht wird über beides.
- **Tastatur, Maus und Gamepad in derselben Liste.** Nachschlagen, welche
  Taste was tut, ohne ins Spiel zu wechseln. *Gewünscht von Morkhan.*
- **Drei Ansichten zum Umschalten** — *Von mir geändert* · *Alles* ·
  *Werkseinstellung*. Das beantwortet zwei verschiedene Fragen: „was habe ich
  umgestellt" und „was tut diese Taste eigentlich". Eigene Belegungen sind in
  der Gesamtansicht als solche gekennzeichnet.

### Behoben

- **Im Suchfeld ging nach jedem Buchstaben der Eingabezeiger verloren** — man
  musste für jeden weiteren Buchstaben neu hineinklicken. Die Seite baute sich
  bei jedem Tastendruck komplett neu auf, samt des Feldes, in das gerade
  getippt wurde.

## v3.14.0-rc1 - 2026-09-04

> Neu dabei: eine Seite für deine Joysticks. Sie zeigt, welcher Stick welche
> Nummer hat, ob deine Belegung noch auf ein Gerät zeigt, das wirklich da ist —
> und was eigentlich auf welchem Knopf liegt. Alles direkt aus den Dateien des
> Spiels, für jeden Stick, ohne dass irgendein Gerät vorher eingepflegt werden
> muss.

### Neu

- **Joysticks** — neuer Reiter unter „Einstellungen". Drei Dinge auf einer
  Seite:
  - **Welcher Stick ist welche Nummer.** Star Citizen merkt sich Belegungen an
    einer Nummer (`js1`, `js2`), nicht am Gerät. Die Seite stellt beides
    nebeneinander: was das Spiel zuletzt verbunden hat, und was in deiner
    `actionmaps.xml` steht.
  - **Ist noch alles da?** Fehlt ein belegtes Gerät, steht es rot in der Liste.
    Meldet sich eines unter neuer Kennung — anderer Anschluss, neue Firmware,
    Austauschgerät —, lässt sich seine alte Belegung auf Knopfdruck übernehmen.
    Vorher entsteht immer eine Sicherung neben der Originaldatei.
  - **Was liegt auf welchem Knopf.** Die komplette Belegung als durchsuchbare
    Liste, nach Gerät filterbar. Das geht für **jeden** Stick, jedes Pedal,
    jedes Gamepad — es muss kein Gerät vorher bekannt sein.

### Verbessert

- Die Geräteliste kommt aus dem Startprotokoll des Spiels statt aus einer
  Geräteabfrage des Systems. Damit stimmt sie unter Windows und Linux
  gleichermaßen und braucht kein einziges Zusatzpaket.

## v3.13.3 - 2026-09-04

> Zwei Dinge, die im Alltag stören: Aufträge, die als laufend stehen blieben
> obwohl sie längst weg waren, und ein Overlay, das bei jedem Start kleiner
> wurde.

### Behoben

- **Aufträge blieben als laufend stehen, obwohl sie weg waren.** Nimmt ein
  anderer Spieler einen Auftrag weg, den du gerade angenommen hast, meldet das
  Spiel dazu keinen Abschluss — es schreibt das Ende nur in eine technische
  Zeile. Der Watcher hat sie bisher nicht als Ende gewertet, und der Auftrag
  stand für den Rest der Sitzung im Overlay. Solche stillen Enden werden jetzt
  erkannt, über die Kennung des Auftrags.
- **Das Auftrags-Protokoll unterscheidet jetzt abgeschlossen und abgebrochen.**
  Weil die stillen Enden auch den Grund mitbringen, steht bei einem
  abgebrochenen Auftrag nicht mehr „läuft" oder „nicht mehr offen", sondern
  **abgebrochen**. An einem echten Protokoll mit 400 Durchläufen wurden dadurch
  60 Einträge richtiggestellt.
- **Das Overlay wurde bei jedem Start kleiner.** Wer eine feste Bildschirmecke
  eingestellt hat, bekam es beim Start auf die Mindestgröße gestaucht — aus
  620×316 wurden 564×150. Die gemerkte Größe gilt wieder.

## v3.13.2 - 2026-09-04

> Zwei Stellen, an denen das Werkzeug stillschweigend aufhörte, aktuell zu
> sein: Die Häkchen in den Auftragstexten folgen jetzt deinem Bestand, und das
> Auftrags-Protokoll wächst mit, statt nur beim Programmstart zu wachsen.

### Behoben

- **Die Häkchen in den Auftragstexten blieben stehen, wenn dein Bestand
  wuchs.** Neu geschrieben wurde bisher nur bei einer neuen Übersetzung, neuen
  Vertragsdaten oder wenn ein Spiel-Patch die Datei ersetzt hat — dein eigener
  Bestand stand nicht auf der Liste. Ab dem ersten Bauplan, den du danach
  freigeschaltet hast, zeigte das Spiel also zu wenige Häkchen, ohne dass etwas
  darauf hingewiesen hätte. Jetzt merkt sich das Werkzeug den Stand deines
  Bestands und schreibt neu, sobald er sich ändert.
  ⚠ Damit die neuen Häkchen im Spiel ankommen, musst du dich einmal neu
  einloggen — Star Citizen liest die Texte nur beim Anmelden.
- **Das Auftrags-Protokoll wuchs nur beim Programmstart.** Wer den Watcher
  morgens startet und mittags einen Auftrag abgibt, fand ihn dort nicht. Die
  Seite liest jetzt bei jedem Öffnen nach.

## v3.13.1 - 2026-09-04

> Das Werkzeug startet spürbar schneller — je mehr Baupläne du hast, desto
> deutlicher. Dazu findet es jetzt auch die Baupläne wieder, deren Namen von
> MrKrakens StarStrings umgeschrieben wurden.

### Verbessert

- **Der Start dauert nicht mehr Sekunden.** Beim Hochfahren wird der Bestand
  einmal gegen den Katalog geprüft — dabei wurde die Katalogdatei für **jeden
  einzelnen Bauplan** neu von der Platte gelesen. Bei 406 Bauplänen waren das
  3,6 Sekunden bei jedem Start, ohne dass sich etwas änderte. Jetzt sind es
  11 Millisekunden. Wer wenige Baupläne hat, merkt es kaum; wer viele hat,
  deutlich.

### Behoben

- **Baupläne galten als „nicht im Katalog", wenn StarStrings im Einsatz ist.**
  MrKrakens Übersetzung stellt Klasse, Größe und Gütegrad vor den Namen
  (`Ind/2/B Citadel`), während der Katalog `Citadel` kennt — der Bauplan fand
  sich dann nicht wieder. Betrifft 465 Gegenstände: Kühler, Schilde und
  Kraftwerke.
  Gemeldet von Haldjas

### Dank

- **Haldjas** — für den Bericht, aus dem beide Punkte kamen. Der zweite hätte
  ohne seinen Hinweis, dass er StarStrings benutzt, eine falsche Ursache
  bekommen.

## v3.13.0 - 2026-09-04

> Der meistgewünschte Knopf ist da: **Sicherung**. Alles, was nur du hast, in
> eine einzige Datei — und mit demselben Knopf wieder zurück. Gedacht für den
> Rechnerwechsel: Datei auf den Stick, am neuen Rechner einlesen,
> weiterspielen.

### Neu

- **Sicherung** (neuer Knopf oben, neben „Einrichtung starten"). Alles, was nur
  du hast, in **eine Datei**: Bauplan-Bestand, Werkstatt-Lager, Handelslager,
  Auftrags-Protokoll, Merkliste und Einstellungen. Derselbe Knopf spielt eine
  solche Datei auch wieder ein — gedacht für den Rechnerwechsel: Datei auf den
  Stick, am neuen Rechner einlesen, weiterspielen.
  Die heruntergeladenen Nachschlagewerke bleiben draußen, die holt sich das
  Programm von allein zurück; die Sicherung bleibt dadurch klein.
  Beim Einspielen wird der bisherige Stand vorher zur Seite gelegt, und Pfade
  des alten Rechners werden geleert, damit das Programm am neuen Ort selbst
  sucht statt ins Leere zu zeigen.

## v3.12.0 - 2026-09-04

> Neu ist das **Auftrags-Protokoll**: Es zeigt dir, welche Aufträge du wann
> gespielt hast, wie oft — und welcher Bauplan dabei herauskam. Damit ist die
> Frage beantwortet, die sich irgendwann jeder stellt: Welcher Auftrag war das
> nochmal, bei dem der Helm kam? Es ist beim ersten Start schon gefüllt, denn
> die aufgehobenen Protokolle des Spiels reichen Wochen zurück.
>
> Dazu geht die Bauplan-Liste jetzt deutlich schneller auf.

### Neu

- **Auftrags-Protokoll** (neuer Reiter unter „Baupläne"). Der neueste Auftrag
  steht oben, ein Suchfeld findet über den Namen. Kam bei einem Auftrag ein
  Bauplan heraus, steht er darunter. Am Ende siehst du, wie oft du denselben
  Auftrag schon gefahren bist.
- **Beim ersten Start schon gefüllt.** Die aufgehobenen Protokolle des Spiels
  werden einmal durchgesehen — du siehst also sofort die letzten Wochen. Danach
  wächst es mit und bleibt stehen, auch wenn das Spiel seine eigenen Protokolle
  längst gelöscht hat.
- **Das Protokoll wandert in die Ablage** — neben Bestand und den übrigen
  Listen. Wer seine Daten sichert, hat es dabei.

### Verbessert

- **Die Bauplan-Liste geht schneller auf** — spürbar bei großen Beständen.
  Gemeldet von Haldjas

### Behoben

- **Aufträge blieben für immer auf „läuft".** Sie gelten jetzt als **nicht mehr
  offen**, sobald eine spätere Spielsitzung sie nicht mehr kennt.
- **Baupläne hingen am falschen Auftrag.** Ein Auftrag gibt höchstens einen
  Bauplan her; scheinbar laufende alte Aufträge zogen trotzdem jeden neuen Fund
  an sich. Ein Bauplan geht jetzt nur noch an einen Auftrag, der in derselben
  Sitzung wirklich lief — und lieber an gar keinen als an den falschen.
- **Fremde Kennzeichen standen mitten im Auftragsnamen.** Sie stammen von
  anderen Übersetzungswerkzeugen und werden jetzt entfernt.
- **Lange Auftragsnamen wurden rechts abgeschnitten** — sie brechen jetzt um,
  ebenso die Bauplan-Zeile darunter.
- **Ein selbst gewählter Ablage-Ordner ging beim Neustart verloren.** Die
  Umstellung wurde gespeichert, aber an einer Stelle, die beim Start nicht
  gelesen wird.

### Dank

- **Haldjas** — für den Hinweis auf die hakende Bauplan-Liste, aus dem am Ende
  dieser ganze Bereich entstanden ist.

## v3.12.0-rc4 - 2026-09-04

> **Testfassung.** Das Auftrags-Protokoll zeigte Aufträge als laufend an, die
> längst vorbei waren — und weil sie scheinbar liefen, sammelten sie jeden
> später gefundenen Bauplan ein. Beides ist behoben, das Protokoll wird beim
> ersten Start einmal neu aufgebaut.

### Behoben

- **Aufträge blieben für immer auf „läuft".** Ein Auftrag ohne Ende im
  Protokoll stand dort dauerhaft als laufend, teils seit Monaten. Er gilt jetzt
  als **nicht mehr offen**, sobald eine spätere Spielsitzung ihn nicht mehr
  kennt — das Spiel meldet beim Einloggen jeden noch laufenden Auftrag erneut.
- **Baupläne hingen am falschen Auftrag.** Ein Auftrag gibt höchstens einen
  Bauplan her; die scheinbar laufenden alten Aufträge zogen trotzdem jeden
  neuen Fund an sich. Ein Bauplan geht jetzt nur noch an einen Auftrag, der in
  derselben Sitzung wirklich lief und noch keinen hat — und lieber an gar
  keinen als an den falschen.
- **Das Protokoll wird einmalig neu aufgebaut**, damit die alten
  Falschzuordnungen verschwinden. Das dauert beim ersten Start ein paar
  Sekunden länger.

## v3.12.0-rc3 - 2026-09-04

> **Testfassung.** Das Auftrags-Protokoll liest sich jetzt sauber: Fremde
> Kennzeichen verschwinden aus den Namen, und lange Auftragsnamen werden nicht
> mehr am rechten Rand abgeschnitten.

### Behoben

- **Im Auftrags-Protokoll standen Kennzeichen mitten im Auftragsnamen** — etwa
  `Blackbox Retrieval <EM4>[BP]?</EM4>`. Sie stammen von anderen
  Übersetzungswerkzeugen, die dieselben Aufträge markieren. Sie werden jetzt
  entfernt, auch rückwirkend in einem bereits geführten Protokoll.
- **Lange Auftragsnamen wurden rechts abgeschnitten.** Namen wie „Verified
  Bounty: … | HRT (Großes Mehrbesatzungsschiff, mittlere Unterstützung)" brechen
  jetzt um, ebenso die Bauplan-Zeile darunter, wenn bei einem Auftrag mehrere
  Baupläne herauskamen. Beides passt sich der Fensterbreite an.

## v3.12.0-rc2 - 2026-09-04

> **Testfassung.** Nachbesserung an rc1: Das Auftrags-Protokoll blieb leer,
> wenn man es gleich nach dem Start öffnete.

### Behoben

- **Das Auftrags-Protokoll blieb leer.** Beim Start werden die aufgehobenen
  Protokolle des Spiels erst eingelesen — wer die Seite in diesem Moment
  öffnete, sah „noch kein Auftrag" und danach nie wieder etwas anderes. Sie
  frischt sich jetzt bei jedem Öffnen auf.
  Betrifft alle, die rc1 ausprobiert haben.

## v3.12.0-rc1 - 2026-09-04

> **Testfassung.** Neu ist das Auftrags-Protokoll: Es zeigt dir, welche
> Aufträge du wann gespielt hast, wie oft — und welcher Bauplan dabei
> herauskam. Rückmeldung gern über den Fehlerbericht im Programm.

### Neu

- **Auftrags-Protokoll** (neuer Reiter unter „Baupläne"). Der neueste Auftrag
  steht oben, ein Suchfeld findet über den Namen. Laufende Aufträge zeigen
  ihren Stand („3 von 5 Zielen"), beendete stehen in einer Zeile. Kam bei einem
  Auftrag ein Bauplan heraus, steht er darunter — die Antwort auf „welcher
  Auftrag war das nochmal, bei dem der Helm kam?".
- **Es ist beim ersten Start schon gefüllt.** Die aufgehobenen Protokolle des
  Spiels werden einmal durchgesehen; du siehst also sofort die letzten Wochen.
  Danach wächst es mit und bleibt stehen, auch wenn das Spiel seine
  Protokolle längst gelöscht hat.
- **Das Protokoll wandert in die Ablage** — neben Bestand und den übrigen
  Listen. Wer seine Daten sichert, hat es dabei.

### Behoben

- **Die Bauplan-Liste geht schneller auf.** Betrifft alle, bei denen es beim
  Öffnen spürbar gehakt hat.
  Gemeldet von Haldjas

## v3.11.0 - 2026-09-03

> **Wer seinen LIVE-Ordner in HOTFIX umbenennt, wird nicht mehr im Stich
> gelassen.** Kommt eine ausgebesserte Fassung neben LIVE, lädt kaum jemand das
> Spiel neu — man benennt den vorhandenen Ordner um, damit der Launcher nur die
> Unterschiede holt. Damit war der eingetragene Spielordner weg, und der Watcher
> meldete „Star Citizen nicht gefunden", obwohl in den Einstellungen ein Pfad
> steht. Jetzt fällt ihm das auf und er fragt kurz nach.

### Neu

- **Der Watcher merkt, wenn dein Spielordner umgezogen ist.** Ist der
  eingetragene Ordner weg und liegt ein anderer Spielkanal daneben, kommt kurz
  nach dem Start eine Frage: umstellen oder beim Alten lassen. Bei mehreren
  Kanälen siehst du sie alle mit dem Zeitpunkt, an dem dort zuletzt gespielt
  wurde — der zuletzt bespielte steht oben. Gefragt wird **einmal je
  Programmstart**; wer „Jetzt nicht" wählt, hat Ruhe.
  Gemeldet von Haldjas

### Behoben

- **Ein HOTFIX-Spielordner wurde nicht gefunden.** Wer seinen Ordner umbenannt
  hatte, um nur die Unterschiede zu laden, bei dem fand der Watcher das Spiel
  von allein nicht mehr — keine Baupläne, keine Protokolle. Von Hand ausgewählt
  hat der Ordner immer funktioniert.
  Betrifft alle, die auf einer ausgebesserten Fassung spielen.
  Gemeldet von Haldjas
- **Ein Spielordner direkt neben dem eingetragenen wurde übersehen.** Betrifft
  alle, die Star Citizen nicht am Standardort installiert haben.
- **Im Einrichtungsassistenten standen unlesbare Knopfbeschriftungen.** In
  Schritt 4 von 5 und in den Einstellungen unter „Angaben im Spiel" waren die
  drei Knöpfe zur Textquelle nicht beschriftet, sondern zeigten interne Namen.
  Betrifft alle seit dem 26. August.
  Gemeldet von Haldjas

### Dank

- **Haldjas** — für den Fund der falschen Beschriftungen im
  Einrichtungsassistenten und dafür, dass er den umbenannten Spielordner so
  genau geschildert hat, dass daraus zwei Fehler und eine neue Funktion wurden.

## v3.10.0 - 2026-09-03

> **Die Raffinerie verrät dir vorher, welche Methode sich lohnt.** Neun
> Verarbeitungsmethoden stehen im Terminal zur Auswahl, und zu jeder zeigt das
> Spiel eine einzige dürre Zeile — vergleichen kann man nur durch Durchklicken.
> Der Bergbau-Bereich fragt dich stattdessen, was dir wichtig ist, und nennt
> die Methode. Zwei der neun lohnen sich übrigens nie.

### Neu

- **Welche Verarbeitungsmethode?** Im Bereich Bergbau: Wähle, was dir am
  wichtigsten ist — Ertrag, Kosten oder Geschwindigkeit — und was danach
  kommt. Du bekommst eine der neun Methoden genannt, dazu alle neun im
  Vergleich.
- **Zwei Methoden werden ausdrücklich abgeraten.** `Dinyx Solventation` und
  `XCR Reaction` werden von einer anderen in jeder Hinsicht geschlagen. Das
  rechnet das Werkzeug selbst aus.
- Die Empfehlung braucht **kein Netz** und keine geladenen Bergbaudaten.

### Verbessert

- Der Raffinerie-Vergleich je Erz ist jetzt richtig beschrieben: Er vergleicht
  die **Stationen**, nicht die Methoden.

## v3.9.8 - 2026-09-03

> **Mein Lager zeigt wieder alles an.** Wer den Bereich „Raffinerie-Ausbeute
> eintragen" aufgeklappt hatte, bekam die Seite beim nächsten Öffnen nur noch
> halb zu sehen — die Liste der eingetragenen Posten fehlte. Die Daten waren
> nie betroffen, nur die Anzeige.

> [!important]
> Betrifft alle Fassungen seit v3.4.1. Wenn deine Lager-Liste leer aussah,
> obwohl Posten eingetragen sind: Sie waren immer da. Nach dem Update sind sie
> wieder sichtbar, ohne dass du etwas tun musst.

### Behoben

- **Die Lager-Seite brach beim Aufbau ab, wenn der Raffinerie-Bereich
  aufgeklappt war.** Ein Ja/Nein-Wert wurde als Dateipfad gelesen; das warf
  einen Fehler und riss den Rest der Seite mit. Zuklappen half nicht, weil eine
  Seite nur einmal gebaut wird — erst ein Neustart brachte sie zurück.

## v3.9.7 - 2026-09-03

> **Aufträge verraten ihre Baupläne zuverlässiger.** Mehrteilige Auftragsreihen
> zeigten die Bauplan-Angabe bisher nur am ersten Schritt — wer im Spiel einen
> späteren aufschlug, fand dort nichts, obwohl es etwas zu holen gab. Jetzt
> trägt jeder Schritt seine Liste. Dazu bleibt der grüne Streifen nach dem
> Start dort, wo das Overlay wirklich sitzt, und die Herstellung sagt endlich,
> warum ihre Kopfzahl manchmal kleiner ist als der eigene Bestand.

### Behoben

- **Der grüne Streifen blieb nach dem Start in der alten Ecke zurück.** Das
  Overlay wanderte an seinen Platz, der Streifen nicht — er stand dann an einer
  Stelle, an der nichts mehr war, und öffnete das Fenster anderswo. Trat nur
  beim ersten Start nach einem Eckenwechsel auf. Gemeldet von Haldjas (pr0)
- **Bei mehrteiligen Auftragsreihen stand die Bauplan-Angabe nur am ersten
  Schritt.** Wer im Spiel einen späteren Schritt aufschlug, sah dort nichts
  davon — obwohl das Overlay den Bauplan meldete. Betrifft die
  Battaglia-Reihen; die Angabe steht jetzt an jedem Schritt.

### Verbessert

- **Die Herstellung sagt jetzt, wenn Baupläne unklar sind.** Trägt ein Bauplan
  einen Namen, den mehrere Gegenstände führen, zählt er bewusst nicht als
  herstellbar — die Kopfzahl war dadurch kleiner als der eigene Bestand, ohne
  dass etwas darauf hinwies. Sie nennt die unklaren jetzt daneben.

## v3.9.6 - 2026-09-03

> **Das Fenster gehorcht wieder.** Wer es einmal in die Breite gezogen hatte,
> bekam es hinterher nicht mehr niedriger — die Höhe von gerade eben galt ab
> da als Untergrenze. Breite und Höhe lassen sich jetzt wieder unabhängig
> voneinander einstellen.

### Behoben

- **Das Fenster liess sich nicht mehr niedriger ziehen, nachdem es einmal
  breiter geworden war.** Öffnete man eine Seite, deren Knopfreihe mehr Platz
  brauchte, galt ab da die aktuelle Fensterhöhe als Mindesthöhe.

## v3.9.5 - 2026-09-02

> **Weniger Warten, und die Auswahlfelder sitzen wieder.** Das Fenster öffnet
> sich spürbar schneller — es baut nur noch die Seite, die du angefordert
> hast, und zeigt sich erst, wenn sie fertig ist. Dazu drei Fehler weniger:
> weisse Titelleisten, Auswahllisten am Bildschirmrand und eine Bauplan-Liste,
> die sich beim ersten Öffnen Zeit liess.

### Verbessert

- **Die Einstellungen gehen spürbar schneller auf.** Das Fenster baute bisher
  immer zuerst die Bauplan-Liste auf und blendete sie sofort wieder aus — auch
  wenn eine ganz andere Seite gewollt war. Jetzt entsteht nur noch die Seite,
  die man angefordert hat. Gemeldet von Haldjas (pr0)
- **Das Fenster erscheint fertig**, statt sich vor den Augen aufzubauen.
  Gemeldet von Haldjas (pr0)

### Behoben

- **Windows: Der Watcher schrieb die Textdatei des Spiels mit anderen Zeilenenden zurück.** Dadurch änderte sich jede Zeile der Datei, obwohl inhaltlich nichts anders war — auch Kennzeichnungen anderer Werkzeuge waren davon betroffen. Der Inhalt selbst blieb immer unangetastet.
- **Auswahllisten klappten am linken Bildschirmrand auf** statt unter dem Feld.
- **Die Bauplan-Liste liess sich beim ersten Öffnen Zeit.** Sie fragte für
  jeden Bauplan einzeln beim Dateisystem nach, statt einmal nachzusehen.
  Gemeldet von Haldjas (pr0)
- **Die Titelleiste des Hauptfensters blieb weiss**, waehrend andere Fenster
  eine dunkle bekamen. Betroffen war je nach Zeitpunkt mal das eine, mal ein
  anderes Fenster; einmal hell, blieb es hell, bis das Programm neu startete.

## v3.9.4 - 2026-09-02

> **Zwei Nachbesserungen an den Ecken.** Die gewählte Ecke wirkt jetzt auch beim
> Start, und das Overlay legt sich nicht mehr hinter die Taskleiste.

### Behoben

- **Die gewählte Ecke wurde beim Start übergangen.** Das Overlay stand nach
  jedem Start wieder dort, wo es zuletzt war; die Einstellung griff erst, wenn
  man einmal ein- und ausklappte. Gemeldet von Haldjas (pr0)
- **Das Overlay legte sich in einer unteren Ecke hinter die Taskleiste.** Damit
  war der schmale grüne Streifen kaum zu treffen, und das Aufblenden bei
  Mausberührung ging ins Leere. Gerechnet wird jetzt mit der nutzbaren
  Bildschirmfläche statt mit der gesamten. Gemeldet von Haldjas (pr0)

### Verbessert

- **Der Fehlerbericht zeigt, wie lange jede Seite zum Aufbauen braucht** — und
  nennt die genaue Tk-Fassung statt nur der Hauptversion. Wer einen trägen
  Fensteraufbau meldet, liefert damit gleich die Zahlen mit, an denen sich die
  Ursache festmachen lässt.

## v3.9.2 - 2026-09-02

> **Das Overlay bleibt da, wo du es hinlegst.** Wer es in eine Ecke legte und
> zuklappte, hatte je nach Ecke nichts mehr davon — es stand außerhalb des
> Bildschirms und meldete nichts. Dazu überlebt der Bauplan-Block jetzt die
> Läufe von Smart Citizen.

> [!important]
> **Kommst du in einer älteren Fassung nicht mehr an dein Overlay?** Standen
> „Mausklicks durchreichen" und eine der Ecken oben rechts, unten links oder
> unten rechts zusammen an, rutschte das eingeklappte Overlay aus dem Bild —
> mitsamt dem Schloss, dem einzigen Rückweg. Diese Fassung installieren und neu
> starten genügt.

### Neu

- **Ein eingeklapptes Overlay meldet neue Baupläne.** Es klappt bei einem Fund
  auf und nach der eingestellten Zeit wieder zu. Vorher gab es nur den Ton.

### Verbessert

- **Die Titelleiste hängt an der Seite, die zur Ecke passt.** Liegt das Overlay
  unten, sitzt sie unten. Gemeldet von Haldjas (pr0)
- **Der Ziehgriff zeigt mit einem Pfeil dorthin, wohin du ziehen kannst** — und
  verdeckt die Statuszeile nicht mehr.
- **Ein Eckenwechsel wirkt sofort**, auch im Aufblend-Betrieb.

### Behoben

- **Das eingeklappte Overlay stand in drei von vier Ecken außerhalb des
  Bildschirms.** Gemeldet von Haldjas (pr0)
- **Der Bauplan-Block überlebt Smart Citizen.** Wer beide Werkzeuge benutzt,
  verlor bei jedem dessen Läufe alle eingetragenen Baupläne.
- **Die Fenstergröße ließ sich in einer unteren oder rechten Ecke nicht mehr
  ändern** — gezogen wurde gegen den Bildschirmrand.
- **Zurücksetzen ohne Merkdatei funktioniert wieder.** Wer sie verlor, bekam den
  Bauplan-Block nie wieder aus der Übersetzungsdatei heraus.
- **Der Schalter „Mausklicks durchreichen" zeigte den alten Zustand**, wenn das
  Schloss am Overlay benutzt wurde.
- **Die gewählte Ecke wirkte erst beim Klappen**, nicht schon beim Start.
- **Im Suchfeld der Bauplan-Liste ließ sich nichts eintippen.**
- **Zwei Patches hießen im Filter beide „4.10.0".**
- **Der Hinweis auf eine Testfassung führte zur falschen Datei.**

## v3.9.2-rc12 - 2026-09-02

Der Griff zum Größerziehen zeigt jetzt dorthin, wo Platz ist, verdeckt keinen
Text mehr und ist ein richtiges Symbol statt eines Schriftzeichens. Und wer im
Aufblend-Betrieb die Ecke wechselt, sieht den grünen Streifen sofort umziehen.

### Behoben

- ⭐ **Ein Eckenwechsel wirkt sofort, auch im Aufblend-Betrieb.** Dort ist das
  Overlay versteckt und nur der grüne Streifen zu sehen — der blieb aber an der
  alten Stelle stehen, bis man einmal mit der Maus darüberfuhr. Er zieht jetzt
  im selben Moment mit, in allen vier Ecken.
- **Der Ziehgriff zeigt in die Richtung, in die man zieht.** Er stand fest auf
  einem Dreieck nach unten rechts und wies damit in drei von vier Ecken gegen
  den Bildschirmrand — also dorthin, wo es nichts zu ziehen gibt.
- **Der Ziehgriff verdeckt keinen Text mehr.** In einer unteren Ecke sitzt er
  oben und lag damit auf der Statuszeile: Aus „405 Baupläne" wurde „5
  Baupläne". Die Zeile rückt jetzt auf der Seite ein, an der er sitzt.

### Geändert

- **Der Ziehgriff ist ein Symbol aus dem Icon-Satz**, kein Schriftzeichen mehr.
  Getippte Zeichen sehen auf jedem System anders aus und ignorieren teils die
  eingestellte Farbe; im Programm gilt dafür längst dieselbe Regel wie für alle
  anderen Symbole. Es sind vier Pfeile, einer je Zugrichtung.

## v3.9.2-rc11 - 2026-09-02

Das Overlay lässt sich in jeder Ecke wieder größer ziehen, und der grüne Griff
im Aufblend-Betrieb sitzt endlich dort, wo das Overlay steht — auch senkrecht.
Beides waren Folgen davon, dass eine Ecke zwei Richtungen hat und bisher nur
eine davon beachtet wurde.

### Behoben

- ⭐ **Die Fenstergröße lässt sich in jeder Ecke wieder ändern.** Klebt das
  Overlay unten oder rechts am Bildschirm, wurde beim Ziehen gegen genau diesen
  Rand gearbeitet — es ließ sich nur in eine Richtung ziehen, in der kein Platz
  ist. Jetzt bleiben die Kanten, die am Bildschirmrand liegen, stehen, und das
  Fenster wächst dorthin, wo Platz ist: bei einer unteren Ecke nach oben, bei
  einer rechten nach links. Der Ziehgriff sitzt dazu passend an der freien Ecke
  des Fensters — dort verdeckt er auch die Symbole der Titelleiste nicht.
- ⭐ **Der grüne Griff im Aufblend-Betrieb sitzt auch senkrecht richtig.** Er
  wurde an die Oberkante des zuletzt gemerkten Fensters gesetzt. Weil ein
  Overlay in einer unteren Ecke nach oben wächst, lag diese Oberkante fast am
  oberen Bildrand — der Griff saß dann auf halber Höhe statt unten. Das Schloss
  daneben ging denselben falschen Weg und wird jetzt am Griff ausgerichtet.

## v3.9.2-rc10 - 2026-09-02

Liegt das Overlay unten am Bildschirm, sitzt die Titelleiste jetzt auch unten
an seinem Rand — dort, wo man sie erwartet, statt eine ganze Fensterhöhe
darüber. Damit ist der letzte Punkt aus Haldjas' Rückmeldung erledigt.

> [!important]
> **Wer in einer älteren Fassung nicht mehr an sein Overlay kommt, ist nicht
> ausgesperrt.** Standen „Mausklicks durchreichen" und eine der Ecken oben
> rechts, unten links oder unten rechts gemeinsam an, rutschte das eingeklappte
> Overlay aus dem Bild — **mitsamt dem Schloss, das der einzige Rückweg ist.**
> Klicks gingen ins Spiel, anzuklicken war nichts mehr.
>
> Ab dieser Fassung kann das nicht mehr passieren. Wer noch feststeckt: Das
> Werkzeug beenden (Symbol in der Taskleiste), diese Fassung installieren und
> neu starten — die Ecke wird dann richtig berechnet, und das Schloss ist
> wieder da.

### Geändert

- ⭐ **Die Titelleiste hängt an der Seite, die zur gewählten Ecke passt.** Bei
  einer unteren Ecke sitzt sie am unteren Fensterrand, bei einer oberen wie
  bisher oben. Vorher klebte sie immer oben: Wer das Overlay unten platzierte,
  fand Balken und Schloss eine Fensterhöhe über dem Bildschirmrand — sichtbar,
  aber an der falschen Stelle. Der Ziehgriff weicht dabei auf die Gegenseite
  aus, damit er nicht auf dem Schließen-Knopf liegt. Gemeldet von Haldjas (pr0)

### Behoben

- **Der Schalter „Mausklicks ins Spiel durchreichen" geht jetzt mit.** Das
  Durchreichen lässt sich an zwei Stellen umlegen: mit dem Schloss am Overlay
  und mit dem Schalter auf der Seite „Anzeige". Wer die Seite offen hatte und
  das Schloss benutzte, sah dort weiter den alten Zustand — richtig wurde er
  erst, wenn man die Seite schloss und neu aufrief. Zwei Anzeigen für denselben
  Zustand, die sich widersprechen, sind schlimmer als eine.
- **Der Umbau der Titelleiste galt vier Anläufe lang als unmöglich** — er war es
  nie. Beim Umhängen wuchs ein eingeklapptes Fenster „von 22 auf 120 Pixel" und
  ragte „86 Pixel unter den Bildschirmrand"; beide Zahlen stammten aus der
  Mindestgröße des Fensters, die beim Einklappen stehenblieb. Mit deren
  Behebung in der vorigen Fassung ließ sich der Umbau auf Anhieb durchführen.
  Sechs neue Prüfungen halten die Stellen fest, an denen es kippen kann.

## v3.9.2-rc9 - 2026-09-02

Wer das Overlay in eine Ecke legt, findet es dort jetzt auch wieder — und zwar
eingeklappt genauso wie ausgeklappt. Bisher rutschte der schmale Streifen in
drei von vier Ecken über den Bildschirmrand hinaus; sichtbar blieb ein grüner
Strich, mit dem sich nichts mehr anfangen ließ. Dazu wandern Griff und Schloss
mit an die Seite, zu der die gewählte Ecke gehört.

### Behoben

- ⭐ **Ein eingeklapptes Overlay meldet neue Baupläne wieder.** Wer „Immer
  sichtbar" gewählt und die Leiste zugeklappt hatte, bekam bei einem Fund nur
  den Signalton — das Fenster rührte sich nicht, der Bauplan stand ungesehen in
  der Liste. Mit durchgereichten Mausklicks war das doppelt ärgerlich: Erst das
  Schloss treffen, dann aufklappen, und das mitten im Kampf. Das Overlay klappt
  jetzt bei einem Fund von selbst auf und nach der eingestellten Zeit wieder zu.
  Wer zugeklappt arbeiten will, bleibt dabei — der Zustand wird nicht
  überschrieben, und solange der Mauszeiger darauf steht, bleibt es offen.
- ⭐ **Das eingeklappte Overlay bleibt in allen vier Ecken im Bild.** In den
  Ecken oben rechts, unten links und unten rechts stand es teilweise oder ganz
  außerhalb des Bildschirms — je nach Ecke fehlten 252 Pixel zur Seite oder 86
  nach unten. Die Ursache lag nicht bei der Ecken-Rechnung, sondern bei der
  Mindestgröße des Fensters: Sie hielt es auf voller Breite und Höhe fest,
  während die Position bereits für den schmalen Streifen berechnet war. Beide
  ziehen jetzt gemeinsam um. Gemeldet von Haldjas (pr0)
- **Der Griff im Aufblend-Betrieb sitzt an der gewählten Seite.** Der grüne
  Streifen, mit dem sich das versteckte Overlay wieder hervorholen lässt, klebte
  unabhängig von der Einstellung immer in der Mitte — bei einer Ecke unten links
  saß er also mitten im Bild. Er folgt jetzt der Ecke, und das Schloss geht
  denselben Weg: bei einer rechten Ecke liegt es links neben dem Streifen, damit
  es nicht seinerseits über den Rand rutscht. Gemeldet von Haldjas (pr0)
- **Auf der Danke-Seite fehlten Leerzeichen** — dort stand „Einrichtung
  undUpdate" statt „Einrichtung und Update".

### Geändert

- **Eine Prüfung für die Ecken** liegt bei: Sie baut das Overlay unsichtbar auf,
  klappt es in jeder Ecke zu und wieder auf und vergleicht die tatsächliche
  Lage mit dem Bildschirmrand. Genau dieser Fehler wäre damit beim ersten Anlauf
  aufgefallen, statt über mehrere Fassungen zu überleben.

## v3.9.2-rc8 - 2026-09-02

### Behoben

- ⭐ **Der Bauplan-Block überlebt jetzt andere Werkzeuge.** Wer neben dem
  Watcher noch **Smart Citizen** benutzt, verlor bei jedem dessen Läufe die
  eingetragenen Baupläne — nachgemessen an der echten Übersetzungsdatei:
  **398 von 398** betroffenen Auftragstexten. Der Grund liegt nicht bei jenem
  Werkzeug: Es räumt vor jedem Lauf seinen eigenen Textblock ab, indem es
  dessen Beginn sucht und ab dort alles verwirft — und unser Block stand
  dahinter. Er wird jetzt **davor** eingesetzt und bleibt damit stehen.
  Erkannt wird dabei die **Form** solcher Blöcke, nicht ihr Name, damit eine
  Umbenennung auf der anderen Seite nichts kaputtmacht.
- **Zurücksetzen ohne Merkdatei funktioniert wieder.** Wer die Injektion auf
  einem Rechner einspielte und die Merkdatei verlor (Neuinstallation,
  aufgeräumter Ordner), bekam den Bauplan-Block nie wieder aus der
  Übersetzungsdatei heraus — die Notfall-Erkennung griff an ihm gar nicht.
  Dieser Fehler steckte seit der ersten Fassung darin und ist erst durch die
  neue Prüfung aufgefallen.
- **Der Hinweis auf eine Testfassung führte zur falschen Datei.** Die
  Versionsmeldung verlinkte immer auf die *neueste fertige* Version — bei einer
  Vorabfassung landete man damit bei der stabilen davor und bekam die
  Testfassung gar nicht zu sehen.

## v3.9.2-rc7 - 2026-09-02

### Behoben

- **Die gewählte Ecke wird jetzt beim Start angewandt.** Bisher wirkte sie nur
  beim Ein- und Ausklappen — wer eine Ecke einstellte und das Werkzeug neu
  startete, fand das Overlay dort wieder, wo es zuletzt *stand*.

### Zurückgenommen

- **Der Versuch, die Titelleiste bei den unteren Ecken an den unteren
  Fensterrand zu hängen (rc3–rc6), ist rückgängig gemacht.** Er ist an Tk
  gescheitert: Das nötige Neupacken lässt die Fenstergröße neu rechnen, ein
  **eingeklapptes** Fenster wuchs dadurch von 22 auf 120 Pixel und ragte unter
  den Bildschirmrand — mitsamt der Leiste. Und die Leiste ist im eingeklappten
  Zustand der einzige Bedienweg: Ist sie weg, kommt man an gar nichts mehr,
  auch nicht an die Einstellung, mit der man es zurücknehmen würde.
  Vier Anläufe, alle gemessen, alle gescheitert. Der Wunsch bleibt berechtigt
  und wird an einem Rechner umgesetzt, an dem sich das Overlay im Einsatz
  beobachten lässt. Eine Prüfung im Selbsttest hält den Rückbau fest, damit der
  nächste Anlauf den eingeklappten Zustand mitmisst.

## v3.9.2-rc6 - 2026-09-02

### Behoben

- **Im Suchfeld der Bauplan-Liste ließ sich nichts mehr eintippen.** Klicken
  sah aus, als klappe es, aber es kam kein Text an und die Schreibmarke fehlte.
  Ursache war die Neuerung aus rc4: Beim Vorbauen der Seiten im Hintergrund
  ruft das Suchfeld — wie beim normalen Öffnen auch — `focus_set()` auf und
  zog den Eingabefokus auf eine **unsichtbare** Seite. Der Vorbau gibt ihn
  jetzt zurück.
  ⚠️ Ein selbst eingebauter Fehler, entstanden aus einer Verbesserung: Was
  beim Anzeigen richtig ist, ist beim Vorbauen falsch.

## v3.9.2-rc5 - 2026-09-02

### Behoben

- **Zwei Patches hießen im Filter beide „4.10.0".** `4.10.0-live.12519617`
  und `4.10.0-live.12545750` kürzen auf dieselbe Nummer — im Auswahlfeld
  standen dann zwei gleich beschriftete Einträge mit verschiedenen Zahlen,
  „4.10.0 (34)" und „4.10.0 (24)", ohne dass man sie unterscheiden konnte.
  Jetzt steht die volle Version da, sobald die Kurzform doppelt vorkommt.
  Derselbe Fehler war in v3.9.1 bereits **im Bericht** behoben worden, im
  Menü aber nicht — der Selbsttest prüft jetzt beide Stellen.
  ⚠️ Warum es zwei 4.10.0 gibt: Ein Hotfix wurde in den Live-Kanal
  übernommen. Dabei ändern sich Werte an bestehenden Bauplänen, die
  Datenquelle nimmt sie neu auf — sie tragen dann den Stempel des neuen
  Patches, obwohl es sie im Spiel längst gibt.

### Geändert

- **Der Filter „kann zugehen" heißt jetzt „weg beim Aufsteigen"** — und
  erklärt sich. Der alte Name verstand niemand, und ein Knopf mit drei
  Wörtern kann eine Spielmechanik auch nicht erklären, die kaum jemand kennt.
  Deshalb steht jetzt eine Warnzeile **über der Liste**, sobald der Filter an
  ist: „Diese Baupläne gibt es nur bei Aufträgen, die verschwinden, sobald
  dein Ruf zu hoch ist." Damit beantwortet sich das „wieso?" an Ort und
  Stelle, statt in einem Hilfetext, den niemand findet.

## v3.9.2-rc4 - 2026-09-02

### Geändert

- **Das Fenster nimmt Klicks sofort an, auch beim ersten Öffnen einer Seite.**
  Jede Seite entstand bisher erst, wenn man sie anklickte — und das dauert bis
  zu einer Sekunde, in der nichts reagiert. Im eigenen Startverlauf steht es
  schwarz auf weiß: `Seite wasistneu: bauen beginnt` um 00:51:48,
  `steht` um 00:51:49. Jetzt werden die übrigen Seiten im Leerlauf vorgebaut,
  sobald die erste steht — eine nach der anderen, damit die Bedienung
  zwischendurch frei bleibt.
  ⚠️ Das ist **keine Beschleunigung**: Dieselbe Arbeit fällt weiter an, nur
  bevor jemand darauf wartet.

## v3.9.2-rc3 - 2026-09-02

### Behoben

- **Die Titelleiste wandert jetzt wirklich in die gewählte Ecke.** Bisher
  sprang zwar das Fenster dorthin, die Leiste blieb aber an seinem oberen
  Rand — bei einer unteren Ecke saß sie damit eine ganze Fensterhöhe über
  dem Bildschirmrand. Jetzt hängt sie sich bei den unteren Ecken an den
  unteren Fensterrand, samt Titel, Schloss und Schließen-Knopf.
  Gemeldet von **Haldjas (pr0)** — und zwar zweimal, weil der erste Versuch
  am eigentlichen Punkt vorbeiging und nur das Schloss nachzog.

## v3.9.2-rc2 - 2026-09-02

Eine Testfassung: die wählbare Ecke nimmt das Schloss mit, dazu zwei
Kleinigkeiten an der Sprache.

### Behoben

- **Das Schloss blieb in der alten Ecke stehen.** Wird eine Ecke für das
  Overlay gewählt, wandert das Fenster dorthin — das Schloss blieb aber, wo es
  war. Im eingeklappten Zustand fiel es besonders auf: Der schmale Streifen saß
  in der Ecke, das Schloss irgendwo mitten auf dem Bildschirm.
  Der Grund: Das Schloss ist ein **eigenes Fenster**, das passgenau über dem
  Schloss der Leiste liegt (nötig, weil ein durchklickbares Overlay auch seine
  eigenen Knöpfe nicht mehr annimmt). Es wurde bisher nur beim Verschieben und
  bei Größenänderungen nachgezogen — nicht beim Klappen und nicht beim Wechsel
  der Ecke. Beides holt es jetzt nach.
  Gemeldet von **Haldjas (pr0)**, samt einem Bildschirmfoto, das zeigt, wohin
  es gehört.
- **Einzahl im Bericht.** Dort stand „1 Baupläne daraus" und „1 Protokolle".
  Der Bericht ist das, was Nutzer verschicken — ein falscher Plural darin
  sieht nach Nachlässigkeit aus.

### Geändert

- **Auf der Update-Seite heißt der Kasten jetzt „Testversion · zum Testen".**
  Vorher „Auch Testversionen" — als einziger Kasten ohne Zusatz las sich das
  wie ein Nachsatz statt wie eine Wahl. Jetzt dasselbe Muster wie „Stabile
  Version · empfohlen" daneben.

## v3.9.1 - 2026-09-01

Drei Dinge, die der Fehlerbericht selbst ans Licht gebracht hat. Zwei davon
betrafen den Bericht: Er zeigte Angaben, die sich nicht zuordnen ließen oder
eine falsche Herkunft behaupteten — und schickte damit ausgerechnet die
Fehlersuche in die Irre, für die es ihn gibt. Das dritte lag tiefer und konnte
echten Schaden anrichten.

### Behoben

- **Die Patch-Historie im Bericht ließ sich nicht zuordnen.** Zwei
  Spielversionen mit gleicher Nummer — `4.10.0-live.12519617` und
  `4.10.0-live.12545750` — erschienen beide als „4.10.0". Im Bericht stand
  dann `4.10.0 (24), 4.10.0 (34)`, ohne dass erkennbar war, welche Zahl zu
  welchem Patch gehört. Betroffen war ausgerechnet die Zeile, die genau dafür
  eingebaut worden war, nachdem sich dort ein Fehler drei Wochen lang versteckt
  hatte. Die Kurzform steht jetzt nur noch da, solange sie eindeutig ist —
  sonst die volle Version.

- **Der Bericht behauptete eine falsche Herkunft der Suchbegriffe.** Hinter der
  ganzen Liste stand eine einzige Quelle — „aus der global.ini des Spiels" —,
  obwohl die Liste gemischt ist: belegte Formulierungen aus den
  Sprachdateien und die eingebaute Rückfalltabelle. Wer eine der übrigen in der
  `global.ini` sucht, sucht umsonst; „Bauplan überchoo" etwa ist
  Schweizerdeutsch aus der Tabelle und kann dort gar nicht stehen. Jede Gruppe
  wird jetzt einzeln ausgewiesen.

- ⭐ **Ein einziges unlesbares Protokoll machte die ganze Nachlese zunichte.**
  Stolperte das Durchsehen an einer Datei — oder fiel beim letzten Schritt die
  laufende `Game.log` aus, etwa weil eine Platte weg ist —, dann wurde der
  Lesestand **nie gespeichert**. Alle in diesem Lauf gelesenen Protokolle
  galten damit wieder als ungelesen, und beim nächsten Start begann alles von
  vorn: still, ohne Fehlermeldung, jedes Mal aufs Neue. Am alten Stand
  nachgemessen: **0 von 23** Protokollen festgehalten. Jetzt wird die eine
  Datei übersprungen und gezählt, der Rest ganz normal festgehalten — und die
  übersprungene bleibt für den nächsten Lauf offen, statt als erledigt zu
  gelten.

## v3.9.0 - 2026-08-31

Die Auftragsleiste zeigt nur noch, was wirklich läuft — beim Ausloggen verliert
man seine Aufträge, und das Werkzeug weiß das jetzt. Und „Was bringt am
meisten?" hört nicht mehr bei einer Zahl auf: Ein Klick auf den Auftrag zeigt,
welche Baupläne dahinterstecken und wo man ihn annimmt.

### Behoben

- ⭐⭐ **Aufträge von vorgestern standen als „laufend" da.** Gemeldet am
  31.08.2026: Star Citizen war nicht einmal gestartet, und in der Leiste stand
  „Willkommen im System". Der Grund ist eine Lücke im Protokoll — **beim
  Verlassen der Spielwelt meldet das Spiel kein einziges Auftrags-Ende.** Wer
  sich ausloggt, verliert seine Aufträge lautlos; das Werkzeug hörte nur auf
  Enden und führte deshalb Buch über einen Stand, den es im Spiel längst nicht
  mehr gab. Wegklicken half nicht: Beim nächsten Start stand die Zeile wieder
  da, denn sie wurde aus demselben Protokoll neu errechnet.

  Ausgewertet wird jetzt zusätzlich der Marker, den das Spiel beim Verlassen
  schreibt — sprachneutral, und er deckt beides ab: zurück ins Hauptmenü und
  Spiel beenden.

  ⚠ Das ist **nicht** das pauschale Räumen aus v3.4.4. Dort räumte ein Ende,
  das sich keinem Auftrag zuordnen ließ — also ein geratenes. Hier sagt das
  Spiel selbst, dass der Spieler draußen ist. An 23 Protokollen gemessen: 39
  Marker, 19 Annahmen, 3 echte Enden, 87 Zwischenziele — **kein einziger
  Auftrag hat ein Ausloggen überlebt.** Aufträge, die nach dem letzten
  Ausloggen angenommen wurden, bleiben unverändert stehen.

- **„Unsinn" wurde unter Wayland als Systemfehler gemeldet.** Beim Anmelden
  einer Tastenkombination prüfte das Werkzeug erst das System und dann die
  Eingabe. Unter Wayland kam es deshalb nie zur Eingabeprüfung: Wer sich
  vertippte, las „geht unter Wayland nicht", obwohl schon das Eingetippte
  keine gültige Kombination war. Jetzt wird zuerst die Eingabe geprüft.

### Geändert

- ⭐⭐ **„Was bringt am meisten?" beantwortet jetzt beide Anschlussfragen.**
  Die Seite nannte einen Auftrag und eine Zahl — und ließ einen damit stehen.
  Wo nimmt man ihn an? Und **welche** Baupläne sind das überhaupt?

  Beides war längst da und nur nicht verbunden: Der Annahmeort kam von Anfang
  an mit den Daten und wurde weggeworfen, und die Bauplan-Liste kann seit jeher
  auf einen einzelnen Auftrag filtern — man musste den Namen nur von Hand ins
  Suchfeld tippen und dann die Auftragszeile treffen.

  Jetzt steht der Ort unter jeder Zeile („Annehmen in Stanton: Hurston, Arial,
  Aberdeen, Magda und 12 weiteren"), und **ein Klick auf die Zeile öffnet die
  Bauplan-Liste, gefiltert auf genau diesen Auftrag** — mit jedem einzelnen
  Bauplan, abgehakt, was man schon hat.

- ⭐ **Die Zahl heißt jetzt Belohnungstopf, nicht Ausbeute.** Oben stand „44"
  und daneben, der Auftrag bringe „auf einen Schlag am meisten". Das versprach
  mehr, als die Daten hergeben: Die 44 sind der Topf eines **Missionstyps** —
  so viele verschiedene Baupläne können daraus fallen, nicht so viele bekommt
  man für einen Abschluss. Im Bauplan-Fenster war genau diese Zusage nach einer
  Meldung von Morkhan schon entschärft worden; auf dieser Seite stand sie noch.
  Die Zahl bleibt, sie ist richtig — sie ist jetzt nur beschriftet, als das,
  was sie ist.

## v3.8.1 - 2026-08-31

Strg+Alt+B holt die Bauplan-Liste jetzt wirklich nach vorn. Die
Tastenkombination gibt es seit v3.7.0 — gewirkt hat sie unter Windows nie, auf
keinem Rechner. Wer mitten im Vollbild wissen will, ob er einen Bauplan schon
hat, drückt jetzt zwei Tasten, statt herauszutabben und das Fenster blind zu
suchen.

### Behoben

- ⭐⭐ **Strg+Alt+B holte die Bauplan-Liste nicht nach vorn.** Die Kombination
  war beim System sauber angemeldet — der Tastendruck kam nur nie im Programm
  an, auf jedem Windows-Rechner, von Anfang an.

  Windows liefert einen so angemeldeten Druck als **Faden-Nachricht** an genau
  den Programmteil, der ihn angemeldet hat. Das war bisher derselbe, der auch
  das Fenster zeichnet — und der räumt seine Nachrichten selbst ab, ehe die
  Abfrage 300 Millisekunden später nachsieht. Der Druck war zu diesem Zeitpunkt
  längst weg. Nachgemessen: ohne Fenster kamen 3 von 3 an, mit Fenster 0 von 3.

  Jetzt wartet ein eigener Programmteil auf den Druck, dem niemand dazwischen
  aufräumt. Am Wesentlichen ändert das nichts: Angemeldet wird weiterhin genau
  **eine** Kombination, mitgelesen wird nichts.

## v3.8.0 - 2026-08-31

Das Overlay lässt sich endlich dorthin legen, wo man es haben will — und
eingeklappt ist es wirklich klein. Gemeldet von **Haldjas (pr0)**.

### Neu

- ⭐⭐ **Eine Bildschirmecke ist wählbar** (Anzeige → „Wo das Overlay sitzt"):
  oben links, oben rechts, unten links, unten rechts — oder frei verschiebbar
  wie bisher.

  ⚠ **Im Pop-up-Betrieb ist das der einzige Weg.** Dort reicht das Overlay
  Mausklicks durch, damit es im Kampf nicht stört — und was Klicks durchreicht,
  lässt sich auch nicht ziehen. Diese Nutzer konnten das Overlay bisher
  **überhaupt nicht** positionieren.

  ⚠ Gerechnet wird auf dem Bildschirm, auf dem das Fenster **gerade steht** —
  bei drei Monitoren nebeneinander wäre „oben rechts" sonst immer der linke.

### Behoben

- ⭐ **Eingeklappt war das Overlay so breit wie offen.** Nur die Höhe schrumpfte;
  bei 1160 Pixeln blieb ein Balken quer über den halben Bildschirm stehen, den
  man in keine Ecke bekommt. Gemeldet: „der Balken sitzt ja aber mittig vom
  Watcher-Fenster."

  Jetzt schrumpft auch die Breite — auf das, was die Titelleiste wirklich
  braucht. ⚠ Gemessen, nicht geraten: Ein fester Wert säße bei anderer
  Schriftgröße und in der anderen Sprache daneben. Beim Aufklappen kommt die
  alte Breite zurück.

## v3.7.0 - 2026-08-31

Eine Tastenkombination, die auch im Spiel greift. Dazu die weiße Titelleiste —
die war in v3.6.0 nicht wirklich weg.

### Neu

- ⭐⭐ **Strg+Alt+B holt die Bauplan-Liste nach vorn — mitten aus dem Spiel.**

  Star Citizen läuft im Vollbild und blendet den Mauszeiger aus: Wer nachsehen
  will, ob er einen Bauplan schon hat, musste heraustabben und das Fenster dann
  **blind** suchen und anklicken. Nutzerwunsch vom 31.08.2026.

  Einstellbar unter **Anzeige**. Strg, Alt und Umschalt lassen sich mit einem
  Buchstaben, einer Ziffer oder F1 bis F12 verbinden.

  ⚠ **Es wird nichts mitgehört.** Angemeldet wird genau **eine** Kombination,
  und das System weckt das Werkzeug nur bei genau dieser. Alles andere sieht es
  nie — kein Mitschreiben, kein Zugriff auf das, was du im Spiel tippst. Das
  ist der Unterschied zu einem Tastatur-Haken, und der Grund, warum nur dieser
  Weg in Frage kam.

  ⚠ **Ohne Modifikator geht es nicht.** Eine nackte Taste systemweit zu
  belegen hieße, sie im Spiel unbrauchbar zu machen.

  ⚠ **Unter Wayland kann das kein Programm** — das lässt das System aus gutem
  Grund nicht zu. Statt eines toten Eingabefeldes steht dort die Erklärung und
  der Weg über die Tastenkombinationen des Schreibtischs.

- ⭐ **Die Eigenschaften der Rezepte stehen auf Deutsch da.** „Damage
  Mitigation" heißt jetzt Schadensminderung, „Integrity" Integrität, „Max.
  Shield Strength" Schildstärke — alle 24. Gemeldet: „einige können kein
  Englisch und verstehen das nun nicht, und melden, es würde ihnen nicht
  helfen."

  ⚠ Übersetzt wird über den **sprachneutralen Schlüssel**, nicht über den
  englischen Text — sonst fällt beim nächsten Patch die Hälfte still auf
  Englisch zurück. Was noch fehlt, bleibt so stehen, wie das Spiel es nennt.

### Behoben

- ⭐ **Die Titelleiste war weiter weiß.** In v3.6.0 meldete Windows „Einstellung
  gesetzt" — und zeichnete den Rahmen trotzdem nicht neu. Dazu kam: Der Versuch
  **vor** dem ersten Anzeigen ging ins Leere, weil es das Fenster-Handle da noch
  gar nicht gibt. Beides gemessen und behoben.

- ⭐ **Man sieht jetzt, welcher Bauplan in der Herstellung aufgeklappt ist.**
  Gemeldet: „nicht klar genug, welcher Bauplan ausgewählt ist, steht auch
  nirgends." Die Zeile hebt sich ab, **und** der Name steht noch einmal über dem
  Rezept — der Kasten ist lang, und wer bis zu den Zutaten gerollt hat, sieht
  die Zeile nicht mehr.

### Geändert

- **Der Ordner unter „Pfade" heißt jetzt „Ordner für deine Daten"** und sagt,
  was drinliegt: Bauplan-Bestand, Merkliste, **Werkstatt-Lager, Handelslager**,
  Einstellungen und die ausgegebenen Dateien. Dazu der Satz, der bisher fehlte:
  Auf zwei Rechnern denselben Ordner einstellen — und beide arbeiten mit
  demselben Stand. ⚠ Umgestellt wird dabei nur, nicht kopiert.

## v3.6.0 - 2026-08-31

Von der Herstellung direkt zum Bauplan — und der Weg dorthin ist endlich zu
finden. Beides gewünscht von **Bushwick4712 (KRT)**. Dazu die dunkle
Titelleiste.

### Neu

- ⭐⭐ **„Woher gibt es den Bauplan?" — ein Knopf in der Herstellung.**

  Du klappst ein Rezept auf, dir fehlt der Bauplan dafür, und die nächste Frage
  ist immer dieselbe: *Welchen Auftrag muss ich machen?* Die Antwort stand
  schon im Werkzeug — aber auf einer anderen Seite. Man musste wissen, dass es
  sie gibt, und den Namen von Hand hinübertippen.

  Jetzt ein Klick: Die Bauplan-Liste springt auf genau diesen Bauplan, die
  Herkunft ist aufgeschlagen — Fraktion, Auftrag, nötiger Ruf, Belohnung.

  ⚠ **Nur wo er hinführt.** Der Knopf erscheint nur, wenn der Bauplan dir
  fehlt **und** der Katalog weiß, woher es ihn gibt. Der Katalog kennt 738
  Baupläne, die Rezepte sind 1.607 — ein Knopf auf eine leere Liste wäre
  schlimmer als keiner. Findet sich der Bauplan wider Erwarten nicht, bleibt
  die Liste stehen, wie sie war, statt leer zu springen.

### Geändert

- ⭐ **Der Herkunfts-Knopf in der Bauplan-Liste heißt jetzt „Woher?".**
  Vorher war es ein Symbol am rechten Rand der Zeile, ohne Wort — Bushwick hat
  es schlicht nicht gefunden. Ein Symbol erklärt sich nur dem, der es gebaut
  hat.

- ⭐ **Die Titelleiste ist dunkel** (Windows). Das Fenster war von innen
  komplett dunkel, und obendrauf saß eine **weiße** Leiste mit Titel und den
  drei Knöpfen. Sie gehört nicht zum Programm, sondern zu Windows — wer dort
  das helle Design fährt, bekam sie hell, egal wie dunkel der Inhalt ist.

  ⚠ Unter Linux macht das der Fenstermanager; dort ändert sich nichts. Und
  wenn Windows es nicht hergibt, bleibt es bei der hellen Leiste — häßlich,
  aber kein Grund für einen Absturz beim Fensterbau.

## v3.5.3 - 2026-08-31

Das Ergebnis von „Protokolle erneut einlesen" ging in der Fußzeile unter.

### Geändert

- ⭐ **Das Ergebnis kommt jetzt als Fenster, nicht als Zeile.**

  ```
  Protokolle erneut einlesen
  152 Protokolle noch einmal gelesen, 2 Baupläne dazugekommen.
                                                    [ Alles klar ]
  ```

  ⚠ **Die Fußzeile war der falsche Ort.** Sie zeigt vier Sekunden lang eine
  Zeile und ist dann wieder leer — und genau in diesen vier Sekunden sieht
  niemand dorthin, der gerade einen Lauf über hunderte Protokolle angestoßen
  hat. Man hat den Knopf gedrückt und wartet. Gemeldet am 31.08.2026: „in der
  Leiste steht es zu kurz oder gar nicht."

  ⚠ **Die Leiste bekommt es trotzdem.** Den Knopf gibt es auch am Overlay; ist
  das Hauptfenster zu, gibt es kein Fenster, über dem ein Dialog stehen könnte.
  Dann bleibt die Zeile der Weg. Verschluckt wird das Ergebnis nie.

  ⚠ **Ein Fenster nur für ein Ergebnis**, nicht für jede Meldung. Ein Werkzeug,
  das ständig Fenster aufreißt, wird weggeklickt, ohne gelesen zu werden.
  Bauplan-Funde bleiben deshalb, wo sie sind.

## v3.5.2 - 2026-08-31

Zwei Knöpfe für dieselbe Sache — einer davon konnte weniger und war obendrein
rot, obwohl er nichts kaputt machen kann. Gemeldet von **Haldjas**.

### Geändert

- ⭐ **„Protokolle erneut einlesen" ist nicht mehr rot.** Der Knopf kann
  nichts kaputt machen: Er **legt an**, mehr nicht. Es wird nichts entfernt,
  nichts überschrieben, und doppelt kann nichts werden. Der schlimmste Fall
  ist „dauert kurz".

  ⚠ **Und genau darum ging es.** Direkt darunter steht „Bestand
  zurücksetzen" — das löscht wirklich. Waren beide rot, sagte Rot nur noch
  „irgendwas Wichtiges" statt „das ist dann weg". Am 31.08.2026 genau so
  passiert: Haldjas drückte den harmlosen, und es brauchte hinterher einen
  Zuruf. **Rot bleibt jetzt dem vorbehalten, was wirklich etwas wegnimmt.**

- ⭐ **Der zweite „Protokolle neu lesen"-Knopf unter „Erkennung" ist weg.**
  Es gab ihn doppelt, und die beiden waren nicht gleichwertig:

  | Wo | Was er tat |
  |---|---|
  | Erkennung → „Von vorn lesen" | wirkte erst **beim nächsten Start** |
  | Bestand → „Protokolle erneut einlesen" | wirkt **sofort** und sagt, was dabei herauskam |

  Der zweite kann alles, was der erste konnte: Er ignoriert den Lesestand
  ebenso, geht jede aufgehobene Sitzung **und** die laufende `Game.log` durch —
  nur eben ohne Neustart und mit Rückmeldung. Haldjas dazu: „ersteres ist
  wahrscheinlich dann nicht mehr so sinnvoll?" Er hatte recht.

  ⚠ Zwei Knöpfe für eine Sache sind schlimmer als einer: Wer den schwächeren
  erwischt, glaubt, das Werkzeug könne es nicht.

## v3.5.1 - 2026-08-31

„Bestand zurücksetzen" tat bei manchen Leuten gar nichts — und sagte auch nicht,
warum. Und der Fehlerbericht beantwortet ab jetzt selbst die Frage, die man
sonst zurückfragen müsste: greift die Log-Erkennung überhaupt?

### Neu

- ⭐ **Der Bericht sagt jetzt selbst, ob die Log-Erkennung greift.** Die Zeile
  „Sicherungen" nennt drei Zahlen statt einer:

  ```
  Sicherungen   462 Protokolle · 462 durchgesehen · 0 Baupläne daraus
  ```

  ⚠ **Weil Rückfragen oft nicht gehen.** Der Bericht, der zu dieser Fassung
  geführt hat, kam ohne Absender und ohne Nachricht — nur „462 Protokolle" und
  „0 Baupläne". Ob die Erkennung bei diesem Menschen versagt oder ob er
  einfach neu im Spiel ist, war daraus **nicht** zu erkennen. Genau das ist
  aber der Unterschied zwischen „alles in Ordnung" und „das Werkzeug ist für
  ihn wertlos".

  | Was dasteht | Was es heißt |
  |---|---|
  | 462 · 462 durchgesehen · **0** daraus | die Erkennung findet nichts |
  | 462 · **0** durchgesehen · 0 daraus | die Nachlese lief nie |
  | 462 · 462 durchgesehen · 380 daraus | alles in Ordnung |

  Gezählt werden nur Funde **aus Protokollen**. Was vom Launcher, von Hand oder
  aus den Startbauplänen kam, sagt über die Log-Erkennung nichts aus.


### Behoben

- ⭐ **Der Knopf „Bestand zurücksetzen" schwieg.** Roter Knopf, Warnfrage
  bestätigt — und danach passierte nichts. Kein Haken, keine Meldung, kein
  Fehler. Von einem kaputten Knopf war das nicht zu unterscheiden.

  Ursache: Das Werkzeug löschte die Bestandsdatei, ohne den Fall zu bedenken,
  dass **gar keine da ist**. Der Fehler ging still in die Diagnose.

  ⚠ Das trifft nicht die Ausnahme, sondern den Anfang: **Wer noch keinen
  einzigen Bauplan hat, hat auch keine Bestandsdatei.** Genauso im Bericht —
  „Inventory 0 blueprints". Und jeden, der zweimal drückt.

  Jetzt gilt „war schon weg" als das, was es ist: das gewünschte Ergebnis. Der
  Knopf meldet in beiden Fällen dasselbe.

- ⚠ **Und wenn es wirklich schiefgeht, steht es auf dem Bildschirm.** Keine
  Rechte, Datei gesperrt — das stand bisher nur in der Diagnose, wo es nur
  findet, wer weiß, dass es sie gibt. Jetzt sagt es die Fußzeile.

## v3.5.0 - 2026-08-31

Unter jedem laufenden Auftrag steht jetzt, **was gerade zu tun ist**. Und der
Auftrag selbst verschwindet nicht mehr, während er im Spiel noch läuft — das war
eine Nebenwirkung von v3.4.4 und ist behoben, samt der falschen Annahme
dahinter.

### Neu

- ⭐⭐ **Die offenen Zwischenziele stehen unter ihrem Auftrag.**

  ```
  Laufende Aufträge (laut Log)
  Auftrag angenommen: Retake Platforms From Nine Tails  →  3 Baupläne
     ◆ Hartmoore-Inverter deaktivieren
     ◆ Knoten lokalisieren und zurücksetzen
  ```

  Der Auftrag sagt, ob Baupläne drin sind. Das Ziel sagt, wofür du gerade
  fliegst. Beides steht im Protokoll — bisher wurde nur die eine Hälfte
  genutzt. Schaffst du ein Ziel, fällt es raus und das nächste rückt nach.

  Woher die Angaben kommen, mit Absicht getrennt gehalten:

  | | Quelle | sprachneutral? |
  |---|---|---|
  | Zustand (läuft / erledigt / weg) | `<ObjectiveUpserted> … state …` | ja |
  | Wortlaut | die Meldung im Spiel, zugeordnet über die `ObjectiveId` | nein |

  ⚠ **Der Zustand kommt nie aus dem Wortlaut.** Auf Deutsch heißt die
  Ziel-Annahme „Neuer Auftrag" — wortgleich mit einer Auftragsmeldung. Wer
  darauf hört, zählt Ziele als Aufträge.

  ⚠ **Nur was das Spiel selbst ins Auftragsbuch schreibt.** Ein Auftrag führt
  daneben eine Menge interner Ziele mit: Zähler, Auslöser, Zonenwächter. Über
  alle 153 Protokolle gemessen: Von 2832 Zielen tragen 456 zwar das
  Kennzeichen `ShowInLog`, aber keinen Wortlaut — **kein einziges** hat einen
  Wortlaut ohne dieses Kennzeichen. Ohne Wortlaut wird geschwiegen statt
  geraten.

  ⚠ **Höchstens sechs Zeilen**, der Rest wird gezählt. Gemessen hat ein Auftrag
  fast immer genau **ein** offenes Ziel (182 von 226); der Ausreißer hatte
  sechs. Die Grenze fängt nur den unbekannten Fall ab — das Overlay darf die
  Bauplan-Liste nicht vom Bildschirm schieben. Eine abgeschnittene Liste, die
  sich für vollständig ausgibt, wäre schlimmer als gar keine.

### Behoben

- ⭐⭐ **Der laufende Auftrag war weg.** Seit v3.4.4 räumte jedes Ende, das
  sich keinem Auftrag zuordnen ließ, die ganze Liste leer. Gemeldet am
  31.08.2026 mit Bildschirmfoto: „Retake Platforms From Nine Tails" stand im
  Spiel sichtbar links am Rand, die Auftragsleiste war leer.

  Die Annahme hinter v3.4.4 war falsch. Es hieß dort, Star Citizen melde beim
  Zurückziehen das aktive Ziel statt des Auftrags. Richtig ist: **das Spiel
  meldet beides — auf zwei getrennten Ebenen.** Jede Meldung trägt am Zeilenende
  eine `MissionId` und eine `ObjectiveId`:

  ```
  "Auftrag angenommen: Retake Platforms From Nine Tails: "
      MissionId: [916223dd…]  ObjectiveId: []
  "Auftrag zurückgezogen: Obere Plattform erreichen: "
      MissionId: [916223dd…]  ObjectiveId: [40418b42…]
  ```

  Die zweite Zeile nimmt **ein Zwischenziel** weg, nicht den Auftrag — direkt
  danach steht im Protokoll schon das nächste Ziel. v3.4.4 hat solche Zeilen
  für Auftragsenden gehalten und deshalb geräumt.

  Jetzt zählt ein Ende nur noch als Auftragsende, wenn keine `ObjectiveId`
  dabeisteht. Über alle 153 Protokolle nachgemessen: von 473 Enden sind **111**
  bloße Zwischenziele, und in allen 111 Fällen lief die Mission danach
  nachweislich weiter.

- ⚠ **Ein wirklich abgebrochener Auftrag verschwindet trotzdem** — der Fall, für
  den v3.4.4 gedacht war (gemeldet von Morkhan, KRT). Passt der Endtitel nicht
  zum Annahmetitel, entscheidet jetzt die `MissionId`. Sie steht bei **allen**
  1102 gemessenen Annahmen und bei **allen** 362 echten Auftragsenden.

  ⚠ Die Messung in v3.4.4 („bei der Annahme in 26 von 28 Fällen keine
  Missions-Kennung") war ein Messfehler — gesucht wurde nur im Meldungstext,
  nicht am Zeilenende, wo die Kennung tatsächlich steht.

  Ergebnis: **Kein einziges** der 362 Auftragsenden bleibt unzuordenbar. Damit
  muss weder geraten noch pauschal geräumt werden — beides ist raus.

## v3.4.5 - 2026-08-31

Ein angenommener Auftrag stand zweimal im Overlay — einmal in der Auftragsleiste
und wortgleich noch einmal darunter. Jetzt steht er einmal da.

### Behoben

- ⚠ **Derselbe Auftrag wurde doppelt angezeigt.** Die Auftragsleiste („Laufende
  Aufträge") und die Hinweiszeile darunter zeigten denselben Satz. Beide
  Meldungen für sich waren richtig — erst zusammen ergaben sie die Dopplung,
  und im Quelltext war das nicht zu sehen.

  Steht ein Auftrag schon in der Leiste, entfällt die Hinweiszeile. Ohne Leiste
  — oder nachdem der Auftrag dort weggeklickt wurde — erscheint sie weiterhin.

## v3.4.4 - 2026-08-31

Ein zurückgezogener Auftrag verschwindet endlich — bisher stand er nach jedem
Start wieder als laufend da. Und der Wechsel auf die Bauplan-Liste ist wieder
sofort da statt nach einer halben Sekunde.

### Behoben

- ⭐⭐ **Ein zurückgezogener Auftrag stand nach jedem Start wieder da.** Wer
  einen Auftrag abbricht, bekam ihn beim nächsten Start des Werkzeugs erneut
  als laufend angezeigt — und beim übernächsten wieder.

  Der Grund liegt im Spiel: **Beim Zurückziehen meldet Star Citizen nicht den
  Auftrag, sondern das gerade aktive Ziel.** Angenommen wird „Secure Our
  Airspace", zurückgezogen wird „der Außenbereich eines Asteroidenstützpunkts
  aufsuchen und Target finden". Über 152 Protokolle nachgemessen: von 112
  Rücknahmen tragen **genau zwei** einen Titel, der auch als Annahme vorkommt.
  Der Watcher fand also nichts zum Streichen.

  Ein Ende, das sich keinem offenen Auftrag zuordnen lässt, räumt die Liste
  jetzt leer. Der nächste angenommene Auftrag steht sofort wieder da.

  ⚠ **Bewusst nicht geraten.** Naheliegend wäre, einfach den zuletzt
  angenommenen Auftrag zu streichen — das geht aber nicht auf: Bei einem nicht
  zuzuordnenden Ende war nur in 36 von 172 Fällen überhaupt genau ein Auftrag
  offen, meist waren es drei bis acht. Dann verschwände ein Auftrag, den du
  noch hast, und der abgebrochene bliebe stehen. Auch die Missions-Kennung
  hilft nicht: Beim Ende steht sie im Protokoll, bei der Annahme in 26 von 28
  Fällen nicht. Lieber eine Zeile zu wenig als eine falsche.

  Gemeldet von **Morkhan (KRT)**.

- **Zwei fehlende Leerzeichen auf der Danke-Seite.** Dort stand „Star Citizen
  aus demWerkzeug starten" und „797 Baupläne, die niemand zusehen bekam".

### Geändert

- ⭐ **Der Wechsel auf die Bauplan-Liste ist wieder sofort da.** Gemessen kostete
  er **642 ms**, obwohl die Seite längst gebaut war — jetzt sind es **0,4 ms**.

  Schuld war die Routine, die beim erneuten Anzeigen die Filter zurücksetzt:
  Sie zeichnete jedes Mal alle 738 Zeilen neu, auch wenn gar kein Filter gesetzt
  war. Dasselbe auf der Herstellungs-Seite mit ihren 1597 Zeilen. Jetzt wird nur
  noch zurückgesetzt, wenn wirklich etwas gesetzt war — mit Filter passiert
  weiterhin genau dasselbe wie vorher.

  ⚠ Was bleibt: Beim ersten Öffnen einer großen Seite braucht das Zeichnen
  weiterhin seine Zeit, und beim Einblenden rendert das Fenstersystem die vielen
  Zeilen erneut. Das ist die schiere Menge, kein Fehler.

### Dank

- **Morkhan (KRT)** — für den zurückgezogenen Auftrag, der nicht verschwinden
  wollte.

## v3.4.3 - 2026-08-31

Das Handelslager zeigt seine Tabelle wieder. In v3.4.2 baute die Seite nur das
Formular auf — die Liste deiner Ware, die Gesamtsumme und das Löschen einzelner
Posten fehlten. Wer v3.4.2 hat, holt sich am besten gleich diese Fassung.

### Behoben

- ⚠ **Das Handelslager blieb ohne seine Tabelle.** Seit v3.4.2 baute die Seite
  nur das Formular auf; die Liste der eingetragenen Ware, die Gesamtsumme und
  das Löschen einzelner Posten fehlten. Ursache war ein Namenskonflikt, den
  Python still auflöst: Zwei Funktionen hiessen `_leeren` — eine räumt einen
  Rahmen aus, die andere (neu dazugekommen) leert das ganze Lager. Die spätere
  gewinnt, und der Aufbau der Liste starb mit einem Typfehler.

## v3.4.2 - 2026-08-31

Zieh das Fenster einmal auf die Größe, die du brauchst — beim nächsten Start
ist sie wieder da. Und die Anleitung zeigt endlich, was das Werkzeug heute
kann: alle Bilder sind neu, mit Werkstatt und Handel statt der Oberfläche von
vorletzter Woche.

### Geändert

- ⭐ **Das Fenster behält die Größe, die du eingestellt hast.** Wer es größer
  zieht — weil lange Listen sonst nicht hineinpassen —, fand es beim nächsten
  Start wieder auf der Mindestgröße vor und zog es jedes Mal von Neuem auf.
  Jetzt wird die Größe gemerkt.

  Die **Mindestgröße bleibt unverändert**, und gemerkt wird nur die *Größe*,
  nicht die Lage: Eine gespeicherte Position zeigt auf einem anderen Rechner
  ins Nichts — das Fenster geht deshalb weiter mittig auf. Eine Größe vom
  großen Bildschirm wird am kleineren auf dessen Maß gedeckelt.

## v3.4.1 - 2026-08-31

Dein Laderaum lässt sich jetzt sichern — und nach einem Patch, der alle Ware
zurücksetzt, mit einem Klick leeren statt Posten für Posten. Dazu spricht das
Projekt ab sofort zuerst Deutsch: Anleitung, Änderungsprotokoll und diese
Ankündigung stehen oben auf Deutsch, Englisch klappt darunter auf.

> [!important]
> **Die Doku-Dateien heißen anders.** `README.md`, `CHANGELOG.md` und
> `ROADMAP.md` sind jetzt die **deutschen** Fassungen; die englischen liegen
> daneben als `README.en.md`, `CHANGELOG.en.md` und `ROADMAP.en.md`. Alte Links
> auf `README.de.md` laufen ins Leere — wer aus einem Lesezeichen kommt, geht
> einmal über die Projektseite.

### Neu

- ⭐ **Das Handelslager lässt sich sichern, zurückholen und in einem Zug
  leeren** — dieselben vier Griffe, die das Werkstatt-Lager schon hatte, an
  derselben Stelle und mit denselben Worten: *Als Sicherung (.json)*, *Als
  Tabelle (.csv)*, *Sicherung einlesen* und *Lager löschen* in Rot.

  Der Grund ist derselbe wie drüben: Das Handelslager ist Handarbeit, die es
  nirgends sonst zu holen gibt. Und nach einem Patch, der alle Ware
  zurücksetzt, ist der Laderaum im Spiel leer — im Werkzeug aber noch voll.
  Posten für Posten von Hand zu löschen macht niemand, also blieb ein falsches
  Lager stehen und die Verkaufsrechnung log. Ein Klick räumt jetzt alles weg,
  nach einer Rückfrage mit der Zahl der Posten.

  Die Tabelle führt Ware, Menge, das Kennzeichen *gestohlen* und den Lagerort —
  Semikolon und Komma, wie ein deutsches Tabellenprogramm es erwartet.

  ⚠ **Die Sicherung des anderen Lagers wird abgelehnt.** Beide Dateien sehen
  von aussen gleich aus; ohne diese Weiche hätte das Handelslager eine
  Rohstoff-Sicherung klaglos angenommen, alles darin verworfen und ein
  **leeres** Lager gespeichert — mit der Meldung „0 Posten eingelesen". Jetzt
  steht stattdessen da, welche Sicherung wohin gehört.

### Geändert

- **Die Raffinerie-Ausbeute ist eingeklappt, bis man sie braucht.** Der Block
  war der längste auf der Lager-Seite — Einheit, Lagerort, ein sieben Zeilen
  hohes Tippfeld, Vorschau und Knopf. Wer nur schnell einen Posten von Hand
  einträgt, rollte an alldem vorbei, und die eigene Lagerliste lag darunter
  ausser Sicht. Ein Klick auf die Überschrift klappt ihn auf; die Lage wird
  gemerkt, wer also nach jedem Raffinerie-Lauf abtippt, findet ihn offen vor.

- ⭐ **Deutsch ist die Hauptsprache des Projekts.** Das betrifft alles, was man
  auf GitHub zu sehen bekommt: die Anleitung, das Änderungsprotokoll, den
  Ausblick, die Release-Texte (Deutsch oben, Englisch aufklappbar) und die
  Infobox der Projektseite. **Englisch bleibt vollständig gepflegt** — es steht
  nur nicht mehr zuerst.

- **Sicherheitsseite und Verhaltensregeln gibt es endlich auf Deutsch.** Beide
  lagen bisher **nur** auf Englisch. In einem Projekt, dessen Hauptsprache
  Deutsch ist, ist ausgerechnet die Sicherheitsseite die falsche Stelle zum
  Sparen.

- **Der Fußbereich der Anleitung sieht aus wie in den anderen Projekten.**
  Der Autor-Block stand mitten im Text; er steht jetzt unten, mittig, mit Bild
  — und der Ko-fi-Verweis dort, wo jemand ankommt, der bis zum Ende gelesen hat.

- Das sichere Schreiben steht jetzt **einmal** in `pfade.json_sichern()` statt in
  jedem Modul neu. Zwei Fassungen derselben Regel gehen irgendwann auseinander —
  genau das war hier passiert.

### Behoben

- **Beide Lager legen jetzt eine Vorgängerfassung an.** Werkstatt-Lager und
  Handelslager schrieben zwar atomar — erst in eine Nebendatei, dann umbenennen —,
  aber **ohne Rückfall**: Ein versehentlich geleertes oder beschädigtes Lager war
  endgültig weg. Der Bauplan-Bestand hatte diese Sicherung von Anfang an, die
  beiden Lager nie. Jetzt entstehen `rohstoffe.bak.json` und
  `handelslager.bak.json` genau wie `bestand.bak.json`.

  Dort wiegt es sogar schwerer als beim Bestand: Freigeschaltete Baupläne
  liessen sich aus der `Game.log` neu aufbauen, eingelagerte Ware nicht — das
  sind reine Handeingaben, die es nirgends sonst zu holen gibt.

- **Die Oberflächenprüfung hat die halbe Anwendung nie aufgebaut.** In ihrer
  Seitenliste fehlten sechs Seiten — die ganze Werkstatt (Herstellung, Bergbau,
  Mein Lager) und der ganze Handel (Verkauf, Handelslager). Sie meldete
  trotzdem zuverlässig „kein deutscher Text in der englischen Oberfläche".
  Jetzt besucht sie alle achtzehn und schlägt selbst an, wenn eine neue Seite
  dazukommt, ohne eingetragen zu werden.

## v3.4.0 - 2026-08-30

Der Laderaum ist voll — und jetzt? Der neue Bereich **Handel** sagt dir, wo du
deine Ware los wirst und was sie je SCU bringt. Für mehrere Waren auf einmal,
sortiert danach, wie viele davon ein Ort überhaupt abnimmt: Ein Stopp bringt
meist mehr als drei.

Dazu: ein eigenes Lager für Handelsware, der Filter **„Kann zugehen"** für
Baupläne, die du dir unbemerkt verbaust, und die Raffinerie-Ausbeute tippst du
jetzt in einem Rutsch ab statt in 24 Feldern.

### Neu
- ⭐⭐ **„Kann zugehen" — der Filter für das, was du dir unbemerkt verbaust.**
  280 der 353 Aufträge haben eine **Ruf-Obergrenze**: Steigst du bei der
  Fraktion darüber, werden sie dir nicht mehr angeboten — und ihre Baupläne sind
  für diesen Spielstand weg. Im Spiel steht das nirgends.

  Der neue Filter in der Bauplan-Liste zeigt genau die Baupläne, die dir fehlen
  und **nur** über solche Aufträge zu bekommen sind. In einem echten Bestand
  waren das **199 von 738**.

  Bei jedem betroffenen Bauplan steht die Grenze jetzt auch in der Herkunft:
  *„⚠ Zu ab Elite Contractor (95.250 Ruf)"*.

  ⚠ **Ein offener Weg genügt.** Führen fünf Aufträge zu einem Bauplan und einer
  davon hat keine Obergrenze, wird nicht gewarnt — sonst stünde die Warnung
  überall und niemand nähme sie noch ernst.

  ⚠ **Was das Werkzeug NICHT sagt: wie weit du noch weg bist.** Der eigene
  Ruf-Stand steht nicht in der `Game.log` — nachgemessen über 22 Protokolle,
  dort taucht der Ruf ausschließlich als Verbindungszeile zu CIGs Dienst auf.
  Deshalb heißt es „ab wann zu" und nicht „dir bleiben noch 4.200".

- ⭐ **Raffinerie-Ausbeute in einem Rutsch eintragen.** Unter „Mein Lager" gibt
  es jetzt ein Feld, in das du die Zeilen so abtippst, wie sie im Spiel stehen:

  ```
  Titanium 295 188
  Aslarite 287 8
  Heart of the Woods 500 12
  ```

  Beim Tippen rechnet das Werkzeug mit und zeigt, was hineinginge; kaputte
  Zeilen stehen einzeln mit Grund daneben. Ein Knopf, alle Posten drin — der
  Lagerort gilt für alle. Sechs Posten waren über das Formular darüber **24
  Eingaben**, jetzt sind es sechs Zeilen.

  **Einheit umschaltbar: cSCU oder SCU.** Das Raffinerie-Terminal rechnet in
  cSCU („GEWONNENE MATERIALIEN (cSCU)"), die Gegenstands-Anzeige im Lager in
  SCU (`0.889 SCU`). Beide Bildschirme lassen sich so abtippen. Gegengerechnet:
  272 cSCU aus dem Terminal sind dieselben 2,728 SCU wie die sieben Stapel im
  Inventar.

  **Und das Mengenfeld selbst kann jetzt cSCU.** Rechts daneben sitzt ein
  Kästchen: Haken rein, und die Zeile heißt „Menge (cSCU)" — dann tippst du die
  Zahl vom Raffinerie-Bildschirm ab, ohne durch 100 zu teilen. Ohne Haken bleibt
  es bei SCU, wie es die Gegenstands-Anzeige im Lager zeigt.

  Das ist der bequemere Weg: Am Raffinerie-Terminal stehen alle Zeilen
  untereinander, im Inventar musst du jeden Stapel einzeln mit der Maus
  anfahren. Die Einstellung bleibt gemerkt, und die Beschriftung sagt immer,
  welche Einheit gerade gilt — sonst wäre jede Eingabe stillschweigend
  hundertfach daneben.

  **Der Lagerort bleibt stehen** — auch nach dem Eintragen und über den
  Programmstart hinweg. Wer eine Ausbeute einträgt, trägt sechs Posten am
  selben Ort ein; ihn jedes Mal neu zu wählen war reine Tipparbeit.

  **Bei einem vertippten Namen steht der wahrscheinlichste Treffer daneben** —
  „Aslerite" gibt es nicht als Rohstoff. Meintest du: Aslarite? Zugeordnet wird
  trotzdem nichts von allein; die Entscheidung bleibt beim Menschen.

  ⚠ **Automatisch geht es nicht, und das ist gemessen, nicht vermutet.** Der
  Raffinerie-Auftrag steht **nicht** in der `Game.log` — über 22 Protokolle
  nachgesehen: `Refinery` kommt 58-mal vor, ausschließlich als Ladezeile für
  die 3D-Modelle des Decks; `Aslarite`, `Agricium` und `cSCU` **kein einziges
  Mal**. Bilderkennung bräuchte Zusatzpakete und fällt damit aus.

- ⭐ **„Was bringt am meisten?" — der nächste sinnvolle Schritt, unter dem
  Fortschritt.** Die Prozentzahl sagt, wo du stehst, aber nicht, was dich
  weiterbringt. Jetzt stehen darunter die zehn Aufträge, aus denen dir noch die
  meisten Baupläne fehlen — mit Fraktion, Belohnung und nötigem Rang.

  Der oberste bringt in einem echten Bestand **44 fehlende Baupläne auf einen
  Schlag**. Gerechnet wird auf Daten, die ohnehin geladen sind.

- **Zwei Angaben an jedem Bauplan, die es sonst nirgends gibt:**

  | | |
  |---|---|
  | 👥 **Im Team teilbar** | „den könnt ihr zu fünft laufen, jeder bekommt die Baupläne". ⚠ Steht nur da, wenn **alle** Wege teilbar sind — sonst stünde die Staffel am falschen Auftrag |
  | ⏱ **Wiederholsperre** | „Wieder verfügbar nach 2 Std 30 Min". Werte von einer Minute bis zu einer Woche; genannt wird die kürzeste |

  Beides kommt aus CIGs eigenen Vertragsdaten (`canBeShared`,
  `personalCooldownTime`) und war bisher nur in den Rohdaten zu sehen.


- ⭐⭐ **Handel — zwei neue Reiter: „Handelslager" und „Verkauf".**
  Die Frage, die bisher fehlte: *wo werde ich meine Ladung los, und was bringt
  sie je SCU?*

  **Der Verkaufs-Reiter** beantwortet sie für **mehrere Waren auf einmal**.
  Sortiert wird nicht nach dem höchsten Preis, sondern danach, **wie viele
  deiner Waren ein Ort überhaupt abnimmt** — denn das ist der Unterschied, der
  zählt. Gemessen am 30.08.2026 für 100 SCU Gold, 40 Copper und 25 Iron:

  | Weg | Erlös |
  |---|---|
  | alles an **einem** Ort | 3.533.000 aUEC |
  | jede Ware am je besten Ort | 3.566.000 aUEC |

  **Ein Prozent mehr für zwei zusätzliche Anflüge.** Genau diese Antwort geben
  die bekannten Handelsseiten nicht, weil sie immer nur eine Ware betrachten.

  **Das Handelslager** ist bewusst vom Werkstatt-Lager getrennt: Das eine ist
  Baumaterial, das man behält, das andere Ladung, die man loswerden will. Ein
  Knopf im Verkaufs-Reiter übernimmt den ganzen Bestand in die Auswahl.

  Beide Listen sind echte Tabellen — Ware · Ort · SCU · Preis 1 SCU ·
  Gesamtpreis, Zahlen rechtsbündig untereinander.

- **Gestohlene Ware.** Statt einer Güte (die beim Verkauf nichts ändert, und
  erbeutete Ladung hat ohnehin immer Q 0) gibt es im Handelslager den Haken
  *„als gestohlen markiert"*. Der Verkaufs-Reiter blendet dann auf die
  **15 Terminals**, die keine Fragen stellen (`is_nqa` bei UEX) — sieben davon
  mit Ankaufgeboten.

- **Preise selbst auffrischen.** Ein Knopf holt die Preise sofort, statt auf den
  täglichen Abruf zu warten — **einmal pro Stunde**. Solange die Sperre läuft,
  zählt der Knopf die Restzeit selbst herunter und wechselt dabei die Farbe
  (grau → gold → grün). Kein Rot: Der Knopf ist gesperrt, *weil* der Abruf
  geklappt hat.

- **Im Mengenfeld darf gerechnet werden** — `100+5` ergibt 105. Klickt man eine
  Zeile an, steht ihre Menge im Feld und lässt sich mit `+5` oder `−12`
  nachjustieren. Dasselbe Verhalten wie im Werkstatt-Lager.

### Geändert
- **Ware und Lagerort kommen aus geschlossenen Listen** — wie im Werkstatt-Lager.
  Eingetragen werden kann nur, was UEX auch kennt; zu Vertippern werden
  ähnliche Namen vorgeschlagen.

- `preise.py` schloss „Preise je Terminal" bisher ausdrücklich aus („weitere
  2,1 MB Daten und ein anderes Werkzeug"). Der Satz stimmte nicht mehr: Der
  volle Abzug ist 1,04 MB und aufgeräumt abgelegt 293 KB. Der Kopf der Datei
  sagt jetzt, wo die Grenze wirklich verläuft.

- ⭐ **Die Gruppen der Seitenleiste lassen sich zuklappen** — Baupläne,
  Werkstatt, Handel, Einstellungen, Info. Ein Klick auf die Überschrift, der
  Zustand bleibt bis zum nächsten Start erhalten.

  Das ist der dritte Hebel gegen die Fensterhöhe: Wer Werkstatt, Handel und
  Einstellungen zuklappt, drückt den Platzbedarf der Leiste von **1020 auf
  696 Pixel**, und die Mindesthöhe des Fensters geht mit. Vorschlag von
  **Morkhan (KRT)**.

  ⚠ Öffnet man einen Reiter aus einer zugeklappten Gruppe, klappt sie von
  selbst auf — sonst stünde man auf einer Seite, deren Eintrag in der Leiste
  gar nicht zu sehen ist.

- **Die Seitenleiste sieht überall gleich aus.** „Für Fortgeschrittene" hat
  jetzt denselben Klapp-Pfeil wie die Gruppen und sitzt in der Gruppe
  „Einstellungen", statt einzeln unten zu kleben — dahinter liegen Pfade,
  Erkennung und der Bauplan-Bestand, also Dinge, die man einstellt. Alle Klapp-Pfeile benutzen dasselbe Symbol
  wie der Rest des Programms — vorher waren es Textzeichen, die je nach
  Systemschrift anders aussahen.

- **„Star Citizen starten", Kaffee und Discord stehen fest am Fuß der Leiste**
  und rollen nicht mehr mit. Dazu hat die Leiste einen sichtbaren Rollbalken:
  Ohne ihn wirkte eine aufgeklappte Gruppe leer, wenn ihre Einträge unterhalb
  des Fensterrands lagen.

- ⭐ **„Bauplan-Bestand" liegt jetzt hinter „Für Fortgeschrittene."** Die Seite
  schreibt am eigenen Bestand — einlesen, überschreiben, zurücksetzen —, stand
  aber zwischen lauter harmlosen Einstellungen und wurde im Vorbeigehen
  angeklickt. Erreichbar bleibt sie, nur nicht mehr nebenbei.

- **Der Knopf „Protokolle erneut einlesen" ist rot.** Er stößt einen Lauf über
  hunderte Protokolle an und schreibt dabei am Bauplan-Stand. Rot **dauerhaft**,
  nicht erst beim Überfahren — ein Knopf, der erst warnt, wenn die Maus schon
  darauf steht, warnt niemanden. Beides gefunden von **Morkhan (KRT)**, nachdem
  er ihn versehentlich gedrückt hatte.

- **„Mein Lager" bedient sich jetzt wie das Handelslager.** Rohstoff und
  Lagerort sind Auswahlfelder: tippen **oder** den Pfeil anklicken und
  aussuchen. Die Beschriftungen stehen über den Feldern statt daneben, damit
  die aufgeklappte Liste nichts verschiebt. Dieselbe Bedienung an beiden
  Stellen — wer die eine kann, kann die andere blind.

- ⭐⭐ **Die Raffinerie-Ausbeute verlor ihren Lagerort.** Wer „Levski" gewählt
  hatte, bekam die ganze Ausbeute **ohne Ort** eingebucht — und damit als eigene
  Stapel neben dem bereits vorhandenen Bestand. Ursache: Der Ortsname lief durch
  eine Funktion, die eine Eingabe auf einen bekannten **Rohstoff** zieht; ein
  Ortsname steht dort nie drin, also kam nichts zurück.

  Dazu hat der Block jetzt ein **eigenes Feld „Lagerort für diese Ausbeute"**.
  Vorher galt stillschweigend der Ort aus dem Formular weiter oben — das war
  weder sichtbar noch zu ändern, ohne hochzurollen.

- **„Bitte hol die neue Version selbst" kam zur falschen Zeit.** Wer auf
  „holen" klickt, während GitHub die Dateien noch baut, wurde auf die
  Releases-Seite geschickt — wo sie in dem Moment auch nicht liegen. Jetzt steht
  dort, was wirklich los ist: *„Diese Fassung wird gerade noch gebaut."*

- ⭐ **Das Fenster ließ sich nicht mehr kleiner ziehen.** Die Mindesthöhe wurde
  aus dem Platzbedarf der Seitenleiste gerechnet — mit jedem neuen Reiter wuchs
  sie mit und lag zuletzt bei **1028 Pixeln**. Jetzt sind es **380**: Seit die
  Leiste rollt und ihre Gruppen klappbar sind, geht bei einem kürzeren Fenster
  nichts verloren.

- **Beim Löschen sprang die Liste nach ganz oben.** Wer einen Posten weit unten
  entfernte, musste sich neu zurechtfinden — die Liste wird beim Löschen neu
  gezeichnet, und die Rollfläche stand danach wieder am Anfang. Die Stelle
  bleibt jetzt erhalten, im Werkstatt- wie im Handelslager.

### Behoben
- **Die Anleitung sagte Falsches über den SC Deutsch Launcher.** Dort stand, er
  „bestätige die Funde" — diese Zwischenstufe gibt es seit v3.0.0 nicht mehr:
  Was in der `Game.log` steht, steht im Spiel, da ist nichts zu bestätigen. Die
  englische Fassung war längst richtig, die deutsche nicht.

  Neu steht dafür der Punkt da, der wirklich zählt: **Beide schreiben in
  dieselbe Textdatei des Spiels.** Das ist kein Problem — der Watcher ersetzt
  die Liste des Launchers durch dieselbe mit Kästchen, statt eine zweite
  danebenzustellen, und beim Zurücknehmen steht dessen Stand wieder da. Läuft
  der Launcher aber danach noch einmal, sind die Kästchen weg, bis der Watcher
  wieder dran war (spätestens nach sechs Stunden, sofort über *Auffrischen*).

- ⚠⚠ **Bei der Textquelle „Original" wurde in die falsche Datei geschrieben.**
  Wer sein Spiel auf Deutsch stellt, bekam die Angaben in die **englische**
  `global.ini` — die das Spiel nie liest. Eingetragen wurde korrekt, angekommen
  ist nichts, und die Statuszeile meldete trotzdem Erfolg.

  Der Grund: Das Werkzeug ging eine feste Reihenfolge durch — erst `english`,
  dann `german_(germany)` — und nahm die erste Datei, die es gab. Beide gibt es
  fast immer, also gewann **immer Englisch**. Welche Sprache das Spiel wirklich
  liest, steht in der `user.cfg` (`g_language`) — die Zeile hat das Werkzeug
  seit jeher selbst **geschrieben**, aber nie gelesen.

  Jetzt entscheidet `g_language`. Steht dort nichts, bleibt es bei Englisch —
  ohne den Eintrag startet Star Citizen ohnehin so.

  Das erklärt vermutlich, warum Änderungen an den Auftragstexten monatelang
  nicht ankamen. Betroffen war nur „Original"; die Quellen **Deutsch** und
  **StarStrings** bringen ihre Sprache selbst mit.


- Ein Knopf, der zur Laufzeit umbeschriftet wird, holte nach dem Überfahren mit
  der Maus die alte Farbe zurück.


- ⭐ **Das Fenster passte auf 1920×1080 nicht mehr auf den Bildschirm.** Mit der
  Gruppe „Handel" brauchte die Seitenleiste 1020 Pixel, und daraus wurde eine
  **Mindesthöhe größer als der Monitor** — die hält Tk dann gegen jedes
  Verkleinern, das Fenster stand über der Taskleiste hinaus und man kam an
  alles darunter nicht mehr heran. Gefunden von **Morkhan (KRT)** am ersten Testtag.

  Zwei Änderungen: Die Mindesthöhe wird jetzt auf den Bildschirm gedeckelt, und
  die **Seitenleiste rollt**, wenn sie nicht ganz hineinpasst — sonst wären die
  unteren Reiter einfach abgeschnitten gewesen.

- Der **Diagnosebericht** nennt jetzt Fenstergröße und Mindestmaß. Beim Fund
  oben stand dazu keine einzige Zahl darin, obwohl genau sie den Fehler
  ausmachte.

### Danke
- **Morkhan (KRT)** für die Idee zu diesem Reiter, für den Fund, dass das
  Fenster nicht mehr auf den Bildschirm passte, und für den Gedanken, dass
  ein Ort, der die ganze Ladung nimmt, mehr wert ist als der beste Einzelpreis.

## v3.3.5 - 2026-08-30

### Behoben

- ⚠ **Drei Baupläne konnten nie zueinander finden.** Beim Vergleichen werden
  Anführungszeichen angeglichen — gerade, typografische, französische. In der
  Tabelle fehlte ausgerechnet das **öffnende** typografische:
  `SW16BR1 “Buzzsaw” Repeater` wurde zu `sw16br1 “buzzsaw' repeater`, das
  schließende angeglichen, das öffnende nicht.

  Betroffen sind die drei `SW16BR…`-Repeater. Wer sie aus einer anderen Quelle
  hatte — Log, Launcher, Import —, bei dem galten sie dauerhaft als **fehlend**,
  obwohl sie im Bestand standen.

  Aufgefallen beim Abgleich einer von Hand geführten Liste gegen den Katalog,
  nicht durch eine Meldung: Niemand vermutet ein Anführungszeichen dahinter.
  Der Selbsttest zieht jetzt alle gängigen Anführungszeichen durch die
  Vergleichsform und verlangt dasselbe Ergebnis.

## v3.3.4 - 2026-08-30

### Behoben

- **Baupläne, die im Spiel einmal anders hießen, werden wiedererkannt.** Die
  Übersetzung benennt Gegenstände gelegentlich um. Wer den Bauplan vorher bekam,
  trug den alten Namen für immer im Bestand — und der Katalog kannte ihn nicht.

  | Im Bestand | Heute im Katalog |
  |---|---|
  | `BlackFire Racing Flight Suit` | `Neutrino Racing Flight Suit BlackFire` |
  | `BlueFlame Racing Helmet` | `Neutrino Racing Helmet BlueFlame` |

  Dieselben Wörter, andere Reihenfolge, ein Reihenname mehr — ein
  Zeichenketten-Vergleich fängt das nie.

  ⚠ **Zugeordnet wird nur, wenn es eindeutig ist:** wenn **genau ein**
  Katalogeintrag sämtliche Wörter des alten Namens enthält, und der Name
  mindestens zwei Wörter hat. `Parallax` allein steckt in fünf Einträgen und
  bleibt deshalb stehen. Ein falsch zugeordneter Bauplan wäre schlimmer als
  einer, der offen als unbekannt ausgewiesen ist.

  Der vorhandene Stand wird beim nächsten Start mit angeglichen.

  Gefunden an den Daten von **Morkhan (KRT)** 🙏

## v3.3.3 - 2026-08-30

### Behoben

- ⚠⚠ **Das Werkzeug hat sich die eigene Erkennung verdorben.** Wer die Angaben
  am Gegenstand eingeschaltet hat — Klasse, Größe, Gütegrad im Spielnamen —,
  bekam ab dann jeden neu freigeschalteten Bauplan **falsch gespeichert**.

  Der Grund: Das Spiel meldet den Bauplan mit dem Namen, der gerade in seiner
  Textdatei steht. Und dort steht seit der Einfügung nicht mehr „Balandin",
  sondern **„Balandin (S3 B Military)"**. Genau das wurde abgelegt. Der Katalog
  kennt den Namen nicht — der Bauplan galt als **nicht vorhanden**, in der
  Liste fehlte das Häkchen, der Fortschritt blieb zu niedrig, und mit jedem
  weiteren Fund wurde es schlimmer.

  Bei einem Melder waren es **zwölf** Baupläne. Aufgefallen ist es erst, weil
  seit v3.3.2 im Bericht steht, welche Namen der Katalog nicht kennt — die
  Liste las sich wie ein Auszug aus dem Spiel, nur mit Anhang.

  **Behoben in beide Richtungen:** Neue Funde werden unter ihrem Katalognamen
  abgelegt, und der vorhandene Stand wird beim nächsten Start einmal
  angeglichen. Es geht nichts verloren und niemand muss etwas tun.

  ⚠ Die Klammer wird dabei **nur** abgeschnitten, wenn sie die Ursache ist:
  39 Baupläne heißen selbst so („A03 Sniper Rifle Magazine (15 cap)",
  „Artimex Arms (Modified)"). Die Regel greift nur, wenn der volle Name
  unbekannt und der gekürzte bekannt ist — damit auch bei einem Anhang, den es
  heute noch gar nicht gibt.

  Gemeldet von **Morkhan (KRT)** 🙏

### Dank

**Morkhan (KRT)** ist an diesem Tag dreimal fündig geworden, und der letzte
Fund war der schwerste: ein Fehler, der jeden Nutzer mit eingeschalteten
Angaben betrifft und mit der Zeit immer weiter auseinanderläuft. Danke 🙏

## v3.3.2 - 2026-08-30

### Neu

- **Der Bericht sagt jetzt auch, *welche* Baupläne der Katalog nicht kennt** —
  nicht nur, wie viele. Bis zu zwölf Namen, danach „… und N weitere".

  Die Zahl allein sagt nur, dass etwas nicht zusammenpasst. Die Namen sagen
  meistens auch gleich, warum: ein ganzes Rüstungsset, das der Katalog noch
  nicht führt, oder eine abweichende Schreibweise. Ohne sie müsste jemand die
  Datei von Hand mit dem Katalog vergleichen — dann ist die Angabe im Bericht
  wertlos.

## v3.3.1 - 2026-08-30

### Behoben

- ⚠⚠ **Der eingetippte Name kam nicht mit.** Wer im Fehlerbericht seinen Namen
  einträgt, sieht ihn sofort im Kasten — abgeschickt, kopiert und gespeichert
  wurde trotzdem die Fassung von vorhin, also „Von: nicht angegeben".

  Der Grund: Die vier Knöpfe arbeiteten mit dem Bericht, der beim **Öffnen der
  Seite** gebaut wurde; das Nachzeichnen änderte nur die Anzeige. Über dem
  Kasten steht „Du siehst vorher genau, was du verschickst" — dann muss auch
  genau das rausgehen. Jetzt kommt der Text aus dem Kasten.

  Gemeldet von **Morkhan (KRT)** 🙏 — *„bei mir stehts drin, aber wenn ichs
  verschicke wohl nicht."*

- ⚠ **Zwei Zahlen für denselben Bestand.** Der Fehlerbericht meldete 315
  Baupläne, die Bauplan-Liste zeigte 292 — und beide hatten recht: Der Bericht
  zählt die gespeicherten Einträge, die Liste geht den Katalog durch und hakt
  ab, was man davon hat. Ein Bauplan, den der Katalog nicht kennt, fehlt in der
  zweiten Zahl.

  Der Bericht nennt die Differenz jetzt selbst: „315 Baupläne · 292 davon im
  Katalog, 23 unbekannt". Damit ist es kein Widerspruch mehr, sondern eine
  Auskunft — und zwar die interessantere.

  Gemeldet von **Morkhan (KRT)** 🙏

### Dank

**Morkhan (KRT)** hat beide Fehler am Tag der Veröffentlichung gefunden, mit
Bildschirmfotos und einer Beschreibung, die den Fehler auf Anhieb erklärte.
Danke dafür 🙏

## v3.3.0 - 2026-08-30


### Neu

- ⭐⭐ **Die Werkstatt — drei neue Seiten.** Der Bauplan war bisher das Ende der
  Auskunft: „du hast ihn" oder „dir fehlt er". Jetzt beantwortet das Werkzeug
  auch, was danach kommt.

  | Seite | Die Frage, die sie beantwortet |
  |---|---|
  | **Herstellung** | Was braucht dieser Bauplan — und was wird daraus? Zutaten, Herstellzeit und die Werte des fertigen Gegenstands, für **1.597** herstellbare Dinge |
  | **Mein Lager** | Was habe ich? Material, Menge, Qualität und Lagerort, von Hand gepflegt. Im Rezept steht dann, was fehlt |
  | **Bergbau** | Wo bekomme ich das? Rohstoff eintippen → seine Fundorte. Ort eintippen → was es dort gibt. **48 Orte, 38 Erze** |

  **Und die Qualität zählt mit.** Ein Regler je Zutat zeigt, was *dein* Material
  aus den Werten macht — bei **1.524 der 1.597** Baupläne tragen die Daten das
  mit. Wer 900er Iron hat und 500er Riccite, sieht genau, was dabei herauskommt.

- **Der Urheber der deutschen Übersetzung ist jetzt genannt** — mit Name,
  Repository und Lizenz. Sie stammt von **rjcncpt**
  ([StarCitizen-Deutsch-INI](https://github.com/rjcncpt/StarCitizen-Deutsch-INI))
  und steht unter **CC BY-NC-SA 4.0**; die Lizenz verlangt das ausdrücklich.
  Bisher stand dort nur der SC Deutsch Launcher — der Verteiler, nicht der Autor.

  Zu finden unter **Danke & Lizenzen** und in beiden Anleitungen.

  Der Watcher **liefert die Übersetzung nicht mit** und gibt auch keine
  veränderte Fassung weiter: Er ergänzt die Datei ausschließlich auf deinem
  Rechner, und die **Quellenangabe in ihrer ersten Zeile bleibt unangetastet** —
  so verlangt es der Autor, damit jeder zur ursprünglichen Übersetzung
  zurückfindet.

- ⭐⭐ **Ins Lager kommt nur noch, was es im Spiel wirklich gibt** — Rohstoff
  **und** Lagerort. Der Knopf „Trotzdem eintragen" ist weg.

  Der Grund ist kein Ordnungssinn: Ein freies Textfeld heißt, dass jemand
  Schimpfwörter, Religiöses oder Politisches einträgt, ein Bildschirmfoto macht
  und es verbreitet. Am Ende fragt niemand, wer getippt hat — es steht in diesem
  Werkzeug.

  | Feld | Auswahl | Quelle |
  |---|---|---|
  | Rohstoff | **52 Namen** — 39 Mineralien, 13 Pflanzen | Spieldaten |
  | Lagerort | **158 Stationen, Städte und Außenposten** | UEX Corp |
  | Qualität | 0–1000, alles andere wird abgelehnt | |

  Der Lagerort bleibt **freiwillig** — leer ist weiterhin erlaubt. Und liegt
  noch keine Ortsliste vor (erster Start ohne Netz), blockiert das Feld nicht.

- ⭐ **Die 13 Pflanzen sind neu dabei** — Flareweed, Heart of the Woods, Sunset
  Berry, Golden Medmon und die übrigen. Der Watcher kannte sie nicht: Sie stehen
  nicht bei den Mineralien, sondern als Vorkommen an den Fundorten. Sie werden
  von Hand geerntet und lassen sich jetzt mit Qualität einlagern.

- ⭐ **Die Suche in der Herstellung findet auch die Zutat.** „ric" brachte
  „Lo**ric**a" und „Fab**ric**ation" — Zufallstreffer — und nie die 83 Baupläne
  mit Riccite. Und wo nichts herauskommt, steht das jetzt da: **26 der 52**
  Rohstoffe kommen in keinem Rezept vor, alle Pflanzen darunter. Das Suchfeld
  heißt deshalb jetzt „Bauplan oder Rohstoff …" statt „Suchen …".

- ⭐ **„Kaufen oder abbauen?" — die Frage, die nach „dir fehlt" kommt.** Neben
  jeder fehlenden Zutat steht jetzt, was das Zukaufen kosten würde — oder dass
  es **gar nicht geht**.

  Der Befund dahinter ist der eigentliche Gewinn: Von den 26 Rohstoffen, die in
  Rezepten vorkommen, sind **sieben nirgends käuflich** — Aslarite, Lindinium,
  Ouratite, Quantainium, Riccite, Savrilium, Torite. Und **fünf davon stehen
  gleichzeitig auf der Zerlege-Sperrliste**: weder zu kaufen noch aus einem
  zerlegten Stück zurückzuholen. Das sind die echten Engpässe beim Herstellen,
  und bisher stand das nirgends.

  > ⚠ „Nicht kaufbar" wird auch so geschrieben — nie als „0 aUEC". Sonst sucht
  > jemand am Terminal nach einem Schnäppchen, das es nie gab.

  > ⭐ **Am Terminal gekaufte Ware hat immer Qualität 500** — den Nullpunkt.
  > Ein daraus gebauter Gegenstand bekommt auf **jede** Eigenschaft genau
  > ×1,000. Besser wird er ausschließlich mit selbst abgebautem Erz darüber.
  > Deshalb steht die Qualität jetzt am Preis: Ohne sie liest sich „kaufen"
  > wie ein gleichwertiger Weg, der bloß Geld statt Zeit kostet — und das ist
  > er nicht.
  >
  > Gemessen über alle Rezepte des Spielstands 4.10.0: Bei **5.025 von 5.219**
  > Qualitätswirkungen liegt der Nullpunkt exakt bei Q 500.


  Die Preise kommen von der [UEX Corp](https://uexcorp.space)-Schnittstelle,
  **höchstens einmal am Tag** und im Hintergrund. ⚠ Sie werden **nicht
  mitgeliefert** — dieselbe Regel wie bei scmdb. Ohne Netz bleibt der letzte
  Stand; ist gar keiner da, entfällt die Angabe still, und die Seite sieht aus
  wie vorher.

  Keine Handelsrouten, keine Preise je Terminal, keine Frachtplanung: Der
  Watcher beantwortet „kaufen oder abbauen?", nicht „wo am teuersten
  verkaufen?".

- ⭐⭐ **Scan-Signatur — aus der Zahl des Scanners wird ein Name.** Der
  Bergbau-Scanner im Spiel zeigt einen Wert und verrät nicht, was dahintersteckt.
  Tipp ihn im Bergbau ein, und der Watcher sagt dir, **welches Erz** es ist und
  aus **wie vielen Brocken** das Vorkommen besteht.

  | Eingabe | Bedeutung |
  |---|---|
  | `8600` | genau dieser Wert |
  | `~8600` | ±10 % Spielraum |
  | `12000-13000` | alles dazwischen |

  > ⚠ Ohne die Tilde wird **nichts** gerundet. Wer daneben liegt, bekommt „Kein
  > Erz hat diese Signatur" statt eines Treffers, der ihn zum falschen Brocken
  > schickt.

  Die Seltenheit begrenzt dabei, wie viele Brocken es überhaupt sein können —
  Quantainium ist legendär, also höchstens zwei. Ein Vorkommen mit drei kann es
  nicht geben, und der Rechner behauptet es auch nicht.

- ⭐ **Welche Raffinerie am meisten herausholt** — unter jedem Erz stehen jetzt
  alle zwanzig Raffinerien mit ihrem Bonus, beste zuerst, dazu die Spannweite.
  Und die ist kein Rundungsfehler: Bei **Bexalite liegen 18 Prozentpunkte**
  zwischen der besten und der schlechtesten Wahl, bei Quartz 16, bei Titanium 15.

  Stationen mit gleichem Profil stehen in einer Zeile (`CRU-L1 +1 weitere`).
  Erze, bei denen es keinen Unterschied macht, sagen das ausdrücklich, statt
  zehn Nullzeilen zu zeigen.

- **Was beim Zerlegen NICHT zurückkommt.** Sechs Rohstoffe stehen auf CIGs
  Sperrliste — Lindinium, Quantainium, Riccite, Ouratite, Stileron, Savrilium.
  Beim Rest bekommt man die Hälfte wieder. Enthält ein Rezept einen davon, steht
  es jetzt darunter: Ein Bauteil daraus ist eine Einbahnstraße.

- **Prozent und Spanne bei jeder Qualitätswirkung.** `× 0.867` muss man im Kopf
  umrechnen — daneben steht jetzt `−13,28 %`. Und darunter, was überhaupt
  erreichbar wäre: `Q 0–1000 · ×1.2–0.8 · Nullpunkt 500`. Ohne das sagt ein
  Faktor nicht, ob noch viel geht oder fast nichts mehr.

- **Star Citizen Fan Content** — die offizielle „Made by the Community"-Grafik
  aus dem Fankit steht jetzt in der Anleitung, und der vollständige Hinweis nach
  dem Fankit Agreement auch **im Programm** unter „Danke & Lizenzen". Wer ein
  Werkzeug benutzt, liest die Anleitung meist nie.

- **Ein Qualitäts-Regler je Material statt einem für alle.** Bisher gab es
  einen einzigen Regler, der allen Zutaten dieselbe Qualität gab — eine Lage,
  die man praktisch nie hat. Jetzt hat jedes Material seinen eigenen, und jeder
  startet bei deinem tatsächlichen Lagerwert.

  Damit lässt sich die Frage stellen, um die es wirklich geht: „Ich habe 500er
  Iron — was kommt raus, wenn ich 900er nähme, und was ändert sich dadurch am
  Riccite-Wert?" Ein Material, das drei Eigenschaften anhebt, hat trotzdem nur
  **einen** Regler; die drei Zeilen bewegen sich gemeinsam.

- **Das Lager zeigt, womit man den Rohstoff holt** — Hand, Fahrzeug oder
  Schiff, als eigene Spalte. Die Angabe steckt in den Bergbaudaten und
  beantwortet die Frage, die nach „habe ich genug?" kommt: „und wie komme ich
  an mehr?"

- **Das Lager ist durchsuchbar** — das Suchfeld ist jetzt immer da, nicht erst
  ab fünf Posten. Wer viel eingetragen hat, findet sonst nichts mehr; wer wenig
  hat, sieht am leeren Feld, dass es Suchen gibt.

- **Einen einzelnen Posten löschen**, während man ihn bearbeitet — roter Knopf
  neben „Änderung speichern", mit Rückfrage samt Name und Menge.

- **Die Herstellung filtert nach dem Material:** „Material reicht" oder
  „Material fehlt", gerechnet gegen dein Lager. Bei 1597 Bauplänen und dem
  aktuellen Lagerstand sind das 19 gegen 1573 — die Liste wird damit erst
  benutzbar.

  > ⚠ Gerechnet wird mit **deiner Liste**, nicht mit deinem Frachtraum. Den
  > kennt der Watcher nicht, und das steht auch oben auf der Seite.

  Dazu weiterhin der Filter „Bauplan vorhanden / fehlt" — beides zusammen
  beantwortet „was kann ich jetzt sofort bauen?".

- **Ein roter Knopf „Lager löschen"** — mit Rückfrage, damit niemand
  versehentlich seinen Bestand verliert. In der Frage steht, **wie viele
  Posten** verschwinden; „4 Posten werden entfernt" wiegt anders als „wirklich
  löschen?". Das Lager ist Handarbeit, die sonst nirgends liegt: kein Log,
  keine Datenquelle, nur deine Eingaben. Sichern lässt es sich mit dem Knopf
  daneben.

- ⭐ **Suche nach dem Auftrag.** „Retake" fand bisher nichts, obwohl sechs
  Baupläne aus Aufträgen mit diesem Wort stammen. Gesucht wird jetzt auch in
  **Auftragsname, Fraktion und Auftragsart** — „nine tails" findet drei
  Baupläne, „headhunters" 141, „bounty" 77.

  Darüber steht eine Übersicht, die die eigentliche Frage beantwortet: **Was
  gibt es in dieser Quest?** Bei „retake" etwa `Retake Platforms From Nine
  Tails — 3 Baupläne` und `Need multiple CFP outposts retaken — 3 Baupläne`.

  > **Und die Aufträge sind anklickbar.** Ein Klick zeigt nur noch die
  > Baupläne dieses einen Auftrags — bei „Retake Platforms From Nine Tails"
  > also `BUL-H4 Armor`, `BUL-H4 Helmet` und `H4-PBF Ammo Carrier`. Derselbe
  > Auftrag noch einmal angeklickt löst den Filter wieder; ein Filter, aus dem
  > man nicht herauskommt, wäre schlimmer als keiner.

  Darunter stehen die Baupläne selbst, mit Info-Zeichen, Abgabeort und Ruf.

- ⭐⭐ **Zwei Ebenen statt einer langen Liste — Oberkategorie und Unterart.**
  Die Art-Auswahl hatte dreissig Einträge: „Rüstung (Arme)", „Rüstung (Beine)",
  „Rüstung (Torso)", „Helm", „Rucksack", „Kleidung (Jacke)" … Wer eine ganze
  Rüstung zusammenstellt, sucht sich darin einen Wolf.

  Jetzt gibt es **sieben Gruppen** — Schiffswaffen, Schiffsmodule,
  Schiffswerkzeuge, FPS-Waffen, Ausrüstung, Rüstung, Kleidung — und darunter
  die feinen Arten:

  | Gruppe | Unterarten |
  |---|---|
  | Schiffswaffen (87) | Laserkanone 22 · Laser-Repeater 15 · Ballistische Kanone 13 · Ballistische Gatling 9 · Scattergun 6 · Mass Driver 4 · je 3 Distortion, Neutron, Tachyon |
  | Rüstung (303) | Helm 84 · Torso 70 · Arme 69 · Beine 69 · Unteranzug 11 |
  | FPS-Waffen (89) | Pistole 20 · Gewehr 18 · Schrotflinte 15 · MP 12 · Scharfschütze 11 · LMG 8 |
  | Schiffsmodule (157) | Kühler 45 · Generator 44 · Schild 37 · Radar 18 · Quantenantrieb 13 |
  | Ausrüstung (52) | Magazin 34 · Rucksack 15 · Behälter 3 |

  > **Was sich nicht bündeln lässt, bleibt allein stehen** — Andockkragen,
  > Frachtmodul und die übrigen Einzelgänger verschwinden nicht in einem
  > Sammeltopf, sie stehen unter den Gruppen.

  **Bauplan-Liste und Herstellung teilen sich dieselbe Einteilung.** Es sind
  dieselben Baupläne, also muss man auf dieselbe Art suchen können; beide
  Seiten fragen dasselbe Modul, damit es keine zwei Wahrheiten gibt.

  Die feinen Waffenarten stehen in keinem Datenfeld — sie stecken im Tag der
  Rezeptdaten (`BP_CRAFT_APAR_BallisticGatling_S4`).

- **Das Unterart-Feld sagt jetzt, dass es eines ist.** Statt „Alle Unterarten"
  steht dort „12 Unterarten — hier verfeinern", sobald es etwas zu holen gibt:
  *„niemand hat es auf Anhieb gefunden, erst nach Erklärung."*

- **Eigene Beobachtungen lassen sich abwählen** — jede Zeile hat ein ×.
  Wechselt die Staffel ein Rüstungsteil, wirft man die Beobachtung wieder raus.

- ⚠ **Wird ein beobachtetes Teil im Spiel verfügbar, siehst du es jetzt auch.**
  Der Filter „beobachtet" prüfte nur angeklickte Namen — ein Treffer auf ein
  Suchmuster blieb unsichtbar. Man beobachtete etwas und erfuhr nicht, dass es
  da ist. Jetzt erscheint es als ganz normale Zeile, mit Info-Zeichen,
  Abgabeort und Ruf. Im vorliegenden Bestand trifft das bereits auf zwei zu:
  `FBL-8u Undersuit SecondWind` und `Warden Backpack Purgatory Camo`.

- **Der Watcher zeigt jetzt, welche Aufträge gerade laufen** — und behält das
  über einen Neustart hinweg. Bisher war ein angenommener Auftrag nur eine
  Zeile im Verlauf; nach einem Neustart des Watchers war sie weg.

  Möglich wird das, weil Star Citizen nicht nur die Annahme ins Log schreibt,
  sondern auch jedes Ende. In den Protokollen eines einzigen Rechners: 701
  Annahmen, 303 Abschlüsse, 112 Rücknahmen, 57 Fehlschläge — jeweils mit
  derselben Missions-Kennung. Der Watcher geht das laufende Log einmal durch
  und führt Buch: angenommen und danach kein Ende gesehen heisst offen.

  > **Abgeschlossene verschwinden.** Wer an einem Abend zehn Aufträge macht,
  > soll nicht zehn tote Zeilen ansehen. Abschluss, Abbruch und Fehlschlag
  > nehmen den Auftrag aus der Anzeige — sofort, auch im laufenden Betrieb.

  Auch **geteilte** Aufträge zählen: Wer in der Gruppe einen Auftrag
  weitergereicht bekommt, sieht genauso, ob darin Baupläne für ihn stecken.

  Zwei Dinge kann das Log nicht wissen, deshalb stehen sie auch nicht da:
  Nach einem Neustart des **Spiels** beginnt ein frisches Protokoll — was
  davor lief, wird nicht behauptet. Und geht ein Auftrag durch einen Fehler im
  Spiel verloren, meldet das Spiel nichts. Für genau den Fall lässt sich jede
  Zeile mit einem Klick auf das × selbst ausblenden.

### Geändert

- **Die Daten kommen jetzt vom offiziellen SCMDB-Spiegel.** Krovax hat dafür
  eigens ein öffentliches Repo eingerichtet
  ([KrovaxCode/SCMDB_DATA](https://github.com/KrovaxCode/SCMDB_DATA)) — „for
  programmatic consumers". Das ist stabiler als der Weg über die Webseite, vor
  der ein Bot-Schutz steht. **scmdb.net bleibt als Rückfall**, falls der Spiegel
  einmal ausfällt. Danke an Krovax 🙏
- **„Fortschritt" heißt jetzt „Bauplan-Fortschritt".** Mit den neuen Seiten wäre
  der alte Name mehrdeutig gewesen.

### Behoben

- ⚠⚠ **Ein Klick auf „alte Protokolle neu einlesen" holte beim nächsten Start
  den kompletten Einrichtungsassistenten zurück** — bei einem Werkzeug, das
  längst eingerichtet war. Und wer ihn dann zumachte, hatte gar nichts mehr:
  Das Programm beendete sich **wortlos**, kein Overlay, keine Meldung, nichts
  im Fehlerbericht.

  Zwei Fehler in einer Kette:

  | | |
  |---|---|
  | Woran „erster Start" erkannt wurde | am Fehlen des **Lesestands** (`logstand.json`) — genau der Datei, die der Knopf mit Absicht löscht |
  | Was Abbrechen tat | das Programm beenden, **immer** — auch bei fertiger Einrichtung |

  Jetzt merkt sich das Werkzeug die abgeschlossene Einrichtung selbst, und
  Abbrechen beendet nur beim **echten** ersten Start. Wer den Assistenten
  wegklickt, will weiterarbeiten — nicht aufhören.

- ⚠⚠ **„Kaffee spendieren" und „Discord" taten gar nichts.** Beide Knöpfe unten
  links meldeten „wird geöffnet", und dann passierte nie etwas — auch im
  Fehlerbericht stand dazu keine Zeile.

  Der Grund steckt in der Linux-Fassung: Im AppImage zeigen die
  Bibliothekspfade in unser eigenes entpacktes Paket. Jedes daraus gestartete
  Systemprogramm lädt unsere Bibliotheken statt seiner eigenen und stirbt
  sofort. Pythons `webbrowser` meldet trotzdem Erfolg — es prüft nur, ob es
  etwas **gestartet** hat, nicht ob es überlebt.

  Die Hälfte der Verweise im Programm hatte die Gegenmaßnahme schon, die andere
  nicht. Jetzt gehen **alle** durch dieselbe Stelle: saubere Umgebung, `xdg-open`
  zuerst, `webbrowser` nur als Rückfall — und wenn es wirklich nicht klappt,
  steht die Adresse in der Statuszeile, statt dass der Knopf schweigt. Der
  Selbsttest lässt keinen direkten `webbrowser`-Aufruf mehr durch.

- ⚠⚠ **`SC_BP_NO_NET=1` hat nicht alles abgeschaltet, was es versprochen hat.**
  Katalog, Preise, Lagerorte, Serverstatus und die Update-Frage hielten sich
  daran — die **Übersetzungsquellen** und die **Auftragsdaten** nicht. Wer den
  Schalter setzt, will keine halbe Zusicherung. Jetzt hält sich jeder Abruf
  daran; einzige Ausnahme bleibt der Fehlerbericht, der ohnehin nur auf
  Knopfdruck rausgeht. Der Selbsttest lässt kein Modul mit Netzabruf mehr
  durch, das den Schalter nicht kennt.

  Die Anleitung nennt jetzt außerdem **jede** Verbindung einzeln samt Häufigkeit
  — vorher standen dort „zwei Dinge", inzwischen sind es fünf.

- ⚠ **Die Zahlen in der Anleitung waren einen Patch alt** — „655 von 722
  Bauplänen" statt der tatsächlichen **670 von 738**. Solche Zahlen veralten
  mit jedem Spiel-Patch, ohne dass etwas anschlägt; der Selbsttest vergleicht
  sie jetzt gegen die echten Daten.

- ⚠ **„Bestand zurücksetzen" stand unter „Fehler melden" — dort sucht es
  niemand.** Es steht jetzt am Ende der Seite **Bauplan-Bestand**, direkt unter
  „Protokolle erneut einlesen". Nebeneinander wird auch der Unterschied
  sichtbar, auf den es ankommt: Einlesen **ergänzt**, was fehlt.
  Zurücksetzen **wirft weg** und baut aus den Protokollen neu auf.

- ⚠ **Im Rezept war nicht mehr zu erkennen, welche Spanne zu welchem Wert
  gehört.** Die Zeilen „Q 0–1000 · ×0,9–1,1" sammelten sich unter dem letzten
  Wert, statt jeweils unter ihrem eigenen zu stehen — bei drei Materialien
  standen dort drei fast gleich aussehende Zeilen ohne erkennbare Zuordnung.

- ⚠ **Im Fenster standen Rückstriche mitten im Text** — „`8600` für genau
  diesen Wert" statt „8600". Sie stammen aus der Auszeichnung der Textdatei;
  Tk zeigt sie einfach mit. Betroffen waren der Hilfetext zum Scan-Wert und
  Absätze unter „Was ist neu". Die Oberflächenprüfung schlägt jetzt auch bei
  Rückstrichen an, nicht nur bei Sternchen.

- ⚠⚠ **Der Startverlauf im Diagnose-Bericht war unbrauchbar geworden.** Statt
  der Startschritte stand dort zwölfmal dieselbe Zeile „Liste: zeichnen
  beginnt" — und genau dieser Abschnitt ist bei einem harten Absturz das
  Einzige, was übrig bleibt: Seine letzte Zeile sagt, wie weit das Programm kam.

  Zwei Ursachen, beide behoben:

  | Was | Vorher | Jetzt |
  |---|---|---|
  | Trennung Start ↔ Bedienung | alles, was nicht mit „Seite " anfing, galt als Startschritt | getrennt wird an der Zeile, mit der der Start endet |
  | Wiederholungen | jede Zeile einzeln | zusammengefasst als „(12×)" |

  Der alte Weg war eine Liste von Vorsilben — er brach in dem Moment, als
  irgendwo im Programm ein neuer Eintrag dazukam. Der neue kann das nicht mehr:
  Was nach dem Start passiert, steht zwangsläufig hinter der Grenzzeile.

- ⚠ **Auf Schweizerdeutsch wurde kein einziger Auftrag erkannt.** Die
  `live-CH`-Fassung schreibt „**Uftrag** angenommen", „Uftrag abgschlosse",
  „Uftrag fehlgschlage" — ohne „A". Direkt in der Quelle nachgesehen, nicht
  geraten. Ohne diese Einträge blieb der Watcher dort still: keine Meldung,
  keine übersprungene Datei, einfach keine Aufträge.

- ⚠⚠ **Die Prozentangaben waren abgeschnitten** — „× 1.047  +4.(" statt
  „+4,70 %". Das Etikett hatte eine feste Breite von neun Zeichen; als die
  Prozentzahl dazukam, schnitt Tk sie stumm ab. Prozent hat jetzt eine eigene
  Spalte, und der Selbsttest misst **jedes** Etikett im Rezept gegen die Breite,
  die es bekommt.

- ⚠ **Gleiches Material, gleiche Qualität, gleicher Ort wird zusammengezählt**
  statt ein zweites Mal in die Liste gestellt. Wer nach jedem Abbauflug nachträgt,
  hatte sonst nach einer Woche zehn Zeilen desselben Stapels.

- ⚠ **„Löschen" in der Lagertabelle war abgeschnitten** („chen"). Es wurde nach
  den Spalten gepackt und bekam nur den Rest.

- ⚠ **Ein aufgeklapptes Auswahlmenü blieb beim Seitenwechsel stehen** — offen in
  der Herstellung, dann auf „Mein Lager" geklickt, und die Liste schwebte weiter
  über der neuen Seite. Sie hört jetzt darauf, dass ihr Feld ausgeblendet wird.

- ⚠ **Der Rollbalken war praktisch unsichtbar** — Kontrast **1,6 : 1** auf einer
  aufgeklappten Liste. Jetzt 2,9 : 1 dort, 3,6 : 1 auf einer Seite, dazu eine
  sichtbare Bahn und 10 statt 8 Pixel. Gilt für jede Rollfläche.

- ⚠ **Die Auswahlfelder waren so breit wie ihr längster Eintrag.** Unter den 64
  Herstellern steht „Musashi Industrial & Starflight Concern" — das Feld wurde
  314 Pixel breit, und die vierte Auswahl passte nicht mehr in die Zeile. Jetzt
  gedeckelt; die aufgeklappte Liste bleibt voll breit.

- ⚠ **Das Fenster verließ bei großer Schrift den Bildschirm.** Bei zwei
  übereinander stehenden Monitoren lief es in den zweiten hinein. Es bleibt jetzt
  auf seinem Monitor, solange man es nicht selbst zieht. **„Sehr groß" ist als
  Schriftgröße entfallen** — die Stufe machte das Fenster größer, als ein
  Bildschirm hoch ist.

- ⚠ **Im Lager ließ sich die Menge nicht so ändern, wie man es tut.** Beim
  Bearbeiten steht die Menge schon im Feld; wer drei dazulegen will, hängt `+3`
  an und hat `1.04+3` dastehen — genau das wurde abgelehnt. Jetzt geht beides,
  und beides ergibt dasselbe. Daneben steht beim Tippen, was herauskommt:
  „ergibt 4,04 SCU".

- ⚠ **Der Namensvorschlag stand 557 Pixel unter dem Eingabefeld**, unten bei den
  Knöpfen. Jetzt 15 Pixel daneben — beides gemessen.

- ⚠⚠ **Im Lager ließ sich die Menge nicht so ändern, wie man es tut.** Beim
  Bearbeiten steht die aktuelle Menge schon im Feld — wer drei dazulegen will,
  hängt hinten `+3` an und hat `1.04+3` dastehen. Genau das wurde abgelehnt
  („Trag eine Menge ein, zum Beispiel 12,5"), weil nur ein **führendes**
  Vorzeichen zählte.

  Jetzt geht **beides**, und beides ergibt dasselbe: `+3` und `1.04+3` machen
  aus 1,04 gleichermaßen 4,04. Niemand muss wissen, welche Form gemeint ist.

- ⚠ **Der Hinweis dazu war eine Zumutung.** „Menge überschreiben — oder +5 bzw.
  -2 tippen, dann wird auf- oder abgebucht" beschrieb eine Mechanik in
  Buchhaltersprache, ohne zu sagen, wohin die Zeichen gehören.

  Die eigentliche Erklärung ist jetzt keine: **Neben dem Feld steht beim Tippen,
  was herauskommt** — „ergibt 4,04 SCU", „ergibt 0 — der Posten wird gelöscht",
  „mehr als vorhanden (1,04 SCU)". Der Hinweistext ist auf eine Zeile mit
  Beispiel geschrumpft.

- ⚠ **Der Namensvorschlag stand 557 Pixel unter dem Eingabefeld** — unten bei
  den Knöpfen, während man oben tippt. Ein Vorschlag, den man suchen muss, ist
  keiner. Er steht jetzt direkt neben dem Feld (15 Pixel), beides gemessen.

- ⚠⚠ **Der ganze Qualitäts-Block war verschwunden** — Regler, Wirkungen und
  sogar der Wert hinter „Herstellzeit". Betraf rc37 und rc38.

  Ursache: Beim Einbau der Zerlege-Sperrliste bekam eine Variable den Namen
  `_dauer` — und überschrieb damit die gleichnamige **Funktion** in derselben
  Datei. Ein paar Zeilen später warf `_dauer(stufe['zeit'])` dann
  `TypeError: 'int' object is not callable`, was den Aufbau mitten im Rezept
  abbrach: Alles ab der Herstellzeit fehlte ersatzlos.

  > ⚠ Der Selbsttest hat es nicht gesehen, weil er die Seite zwar **baute**,
  > aber nie eine Rezeptzeile **aufklappte** — genau dort läuft der Code. Das
  > tut er jetzt, und zusätzlich prüft er, dass kein lokaler Name eine Funktion
  > derselben Datei verdeckt. Gegen die ausgelieferte rc38 gemessen: Beide
  > Prüfungen schlagen dort an, an genau der richtigen Zeile.

- ⚠ **Schweizerdeutsch wurde nicht erkannt.** Es gibt eine eigene Fassung der
  deutschen Übersetzung (`live-CH`), die „**Bauplan überchoo**" schreibt statt
  „Bauplan erhalten". Ohne den Eintrag fand der Watcher dort **still null
  Baupläne** — keine Fehlermeldung, keine übersprungene Datei, einfach nichts.

  Betrifft nur den Rückfall: Eine lesbare `global.ini` gewinnt immer. Für eine
  englische Werksinstallation, deren Textdatei in der `Data.p4k` steckt, ist
  diese Liste aber das Einzige, was bleibt.

- ⚠ **Eine umgestellte Übersetzung hätte den Watcher lautlos blind gemacht.**
  Aus der Textdatei des Spiels wurde bisher nur der Teil **vor** dem Platzhalter
  genommen. Bei „Bauplan erhalten: %s" stimmt das. Würde CIG je umstellen —
  „%s ist eingetroffen" —, stünde davor nichts, und die Erkennung fiele auf die
  mitgelieferte Liste zurück, die dann nicht mehr passt. Wieder ohne jeden
  Hinweis.

  Heute formuliert keine Sprache so; der Zweig kostet nichts und deckt den Tag
  ab, an dem es passiert.

  > ⚠ Das ist der Weg, auf dem **jeder** Bauplanfund läuft. Der Selbsttest
  > sichert deshalb zuerst ab, dass der Suchausdruck ohne umgestellte
  > Formulierung **zeichengleich** mit dem alten ist — gemessen, nicht behauptet.

  Beide Funde stammen aus dem Bauplan-Ausleser des **KRT-Basetools** (GPL-3.0),
  der dieselbe `Game.log` liest. Danke dafür!

- ⚠⚠ **Bei mehr als einem Stück log die Zutatenliste.** Wer 10 in das
  Stückzahl-Feld tippte, sah weiter den Bedarf für ein einziges Stück — „1.16
  SCU" und „dir fehlt 1.16", obwohl 11,6 gebraucht wurden. Der Abzug rechnete
  richtig, nur die Anzeige nicht. Sie rechnet jetzt beim Tippen mit und zeigt
  zusätzlich, woraus sich die Menge ergibt: `11.6 SCU (1.16 × 10)`.

- ⚠⚠ **Reicht das Material nicht, wird jetzt GAR NICHTS abgezogen.** Bisher
  wurde genommen, so weit es reichte, und der Rest gemeldet. Wer mit „Anzahl
  10" klickte und Material für drei hatte, stand danach mit einem leergeräumten
  Lager und ohne die zehn Stück da.

  Fehlt eine Zutat, war der Gegenstand überhaupt nicht herstellbar — der Klick
  war ein Versehen oder ein Vertipper. Gemeldet wird jetzt die **Fehlmenge**,
  nicht nur der Name, und die eingegebene Stückzahl bleibt stehen, damit man
  sie berichtigen kann. (Ins Minus konnte der Bestand nie geraten, aber „auf
  null geräumt" ist fast so schlimm.)

- ⚠⚠ **Gute Werte standen in der Warnfarbe.** Die Anzeige färbte stur nach der
  Zahl: alles ab `× 1.000` grün, alles darunter gold. Bei **852 der 6524**
  Qualitätswirkungen im Spielstand 4.10.0 ist das genau verkehrt — dort senkt
  bessere Qualität den Wert, und das ist die Verbesserung:

  | Eigenschaft | Fälle |
  |---|---|
  | Recoil Smoothness / Handling / Kick | je 245 |
  | Quantum Fuel Burn | 114 |
  | Damage Mitigation | 3 |

  Beim FS-9 LMG stand der bestmögliche Rückstoß (`× 0.800`) in Warnfarbe und
  der schlechteste (`× 1.200`) in Grün. Die Richtung wird jetzt **aus den
  Spieldaten selbst** gelesen, nicht nach Eigenschaftsnamen geraten — damit
  stimmt sie auch dort, wo dieselbe Eigenschaft mal so und mal anders läuft.
  Zeilen, bei denen weniger besser ist, sagen das jetzt auch.

  Gegenprobe: Bei Qualität 0 ist nun **jeder** Wert gold, bei Qualität 1000
  **jeder** grün.

- ⚠ **„Power Pips" sind keine Multiplikatoren.** Sie standen als `× -1.000`
  da — ein Faktor, den es nicht geben kann. In Wirklichkeit sind es
  Stückzahlen von **−3 bis +3** in festen Qualitätsstufen; das betrifft
  sämtliche Kraftwerke (598 der 6524 Wirkungen). Jetzt steht dort `-1` bzw.
  `+3`, mit Vorzeichen. Erkannt wird das an der Zahl, nicht am Namen: Ein
  Multiplikator liegt immer über null.

- ⚠⚠ **Die aufgeklappten Auswahllisten liessen sich nicht rollen** — man drehte
  am Rad, die Liste blieb stehen und stattdessen wanderte die **Seite dahinter**.
  Weil das Auswahlfeld dabei wegrutschte, klappte die Liste zu. Die unteren
  Einträge waren dadurch **überhaupt nicht erreichbar**: im Bergbau ab „microTech"
  bei den 48 Orten, in der Herstellung ab „Greycat Industrial" bei den Herstellern.

  Ursache: Das Mausrad hängt an einer einzigen Stelle für das ganze Programm und
  sucht sich die Rollfläche, indem es vom Element unter dem Zeiger die Elternkette
  hinaufgeht. Die aufgeklappte Liste ist zwar ein eigenes Fenster — ihr Elternteil
  ist aber das Auswahlfeld, und das steht mitten in der rollbaren Seite. Die Kette
  lief also aus der Liste heraus in die Seite dahinter.

  Das Rad wird jetzt am Listenfenster selbst abgefangen und dort beendet; die Seite
  dahinter bekommt es gar nicht mehr zu sehen. Gemessen: gegen den alten Stand
  wandert die Seite um 10,3 %, gegen den neuen um 0,0 %, und die Liste rollt bis
  zum letzten Eintrag durch. Rollt man **neben** der Liste, klappt sie weiterhin zu.

- ⚠ **Die aufgeklappte Liste war zu lang.** Sie reichte vom Auswahlfeld bis weit
  unter den Fensterrand; stand das Fenster tief im Bild, wurde sie am Bildrand
  abgeschnitten. Begrenzt war sie bis dahin nur nach dem verfügbaren *Platz* — und
  der ist auf einem grossen Bildschirm riesig.

  Jetzt zeigt sie höchstens **15 Zeilen**, alles darüber wird gerollt. Damit ist
  auch die Rollleiste sichtbar und sagt, dass noch mehr kommt. Bei den 48 Orten
  im Bergbau sind das 497 statt 1090 Pixel.

  Und eine harte Obergrenze dazu: Eine Auswahlliste wird nie höher als das
  **kleinstmögliche** Fenster (760 Pixel). Wer sein Fenster gross zieht, bekäme
  sonst eine Liste, die nach dem Verkleinern nicht mehr hineinpasst.

- ⚠ **Ein abgeschlossener Auftrag stand weiter als „angenommen" da.** Gemeldet
  am 30.08.2026 an „Retake Platforms From Nine Tails": im Spiel um 01:18
  angenommen, um 01:59 abgeschlossen — und als der Watcher um 02:22 startete,
  meldete er ihn als frisch angenommen.

  Zwei Fehler, die sich gegenseitig getragen haben:

  1. Beim Start liest der Watcher die `Game.log` einmal, nur um zu wissen, wo er
     stehengeblieben ist. Dabei sammelt er nebenbei alle Auftragsereignisse ein.
     Stand beim nächsten Durchlauf nichts Neues im Log, wurde diese Sammlung
     **nicht geleert** — er hat sie ein zweites Mal ausgewertet.
  2. Die Auswertung nahm erst *alle* Abschlüsse und dann *alle* Annahmen. In
     einem Abschnitt, der beides enthält, traf der Abschluss deshalb ins Leere,
     und die Annahme stellte den Auftrag danach wieder hin.

  Punkt 2 trifft jeden, der den Watcher startet, während das Spiel schon läuft —
  dann liest der erste Abschnitt alles nach, was seit dem letzten Lauf geschah.

  Die Listen werden jetzt vor jedem Lesen geleert, und die Ereignisse werden **in
  der Reihenfolge des Logs** durchgegangen. Was am Ende offen ist, wird gezeigt.
  Wer einen Auftrag abbricht und sofort neu annimmt, sieht ihn weiterhin.

- ⚠ **Die Auftragszeile in der Liste liess sich nicht wegklicken.** Das rote
  Zeichen gab es nur in der Auftragsleiste, nicht an der Zeile darunter — wer
  eine Meldung loswerden wollte, kam nicht an sie heran.

  Die Zeile gehört jetzt zum Auftrag: Sie trägt dasselbe rote Zeichen, verschwindet
  von allein, sobald das Spiel das Ende meldet, und lässt sich von Hand wegnehmen.

- ⚠ **Das Overlay startete immer in der kleinsten Grösse**, egal wie gross man
  es gezogen hatte. Gespeichert war die Grösse durchaus — sie wurde nur sofort
  wieder überschrieben.

  Ursache war die Mindestbreite aus rc10: Sie prüft kurz nach dem Start, ob das
  Fenster schmaler ist als seine Symbolleiste. Zu dem Zeitpunkt meldet Tk für
  ein noch nicht angezeigtes Fenster aber die Breite **1** — der Vergleich traf
  also immer zu, und das Overlay wurde auf die Mindestbreite gestellt.
  Ausgerechnet die Änderung, die die Symbole retten sollte.

  Sie greift jetzt nur, wenn das Fenster wirklich schon steht. Geprüft: 900×400
  bleibt 900×400, und ein zu schmal gemerktes Fenster wird weiterhin angehoben.

- ⚠ **Im Lager verlor das Suchfeld nach jedem Buchstaben den Cursor.** Man
  musste für jeden weiteren Buchstaben neu hineinklicken.

  Ursache: Das Feld wurde **in** der Zeichenfunktion gebaut, und die räumt bei
  jeder Änderung den ganzen Listenbereich leer — mit jedem getippten Buchstaben
  zerstörte sich das Feld also selbst. Es entsteht jetzt einmal, ausserhalb.
  Prüfung 52p hält fest, dass dort kein Eingabefeld mehr gebaut wird: Alles,
  woran ein Cursor stehen kann, gehört ausserhalb der Zeichenroutine.

- ⚠⚠ **Ohne Internet liess sich „Fehler melden" nicht mehr öffnen** — das
  Fenster blieb starr, bis ein Netz-Timeout ablief. Ausgerechnet die Seite, die
  man bei Störungen braucht.

  Ursache: Der Diagnosebericht fragte beim Bauen die aktuelle Spielversion bei
  scmdb.net ab — im Hauptfaden, mehrfach. Er zeigt jetzt den **gespeicherten**
  Katalogstand; das ist ohnehin die interessantere Angabe, weil sie sagt, womit
  dieser Rechner arbeitet. Gemessen: von **6,1 Sekunden auf 0,1**.

- ⚠⚠ **Der Serverstatus konnte das Fenster zum Absturz bringen**, wenn kein
  Internet da war. Der Abruf läuft im Hintergrund und meldet sich danach im
  Fenster zurück — wer währenddessen die Seite wechselte oder das Fenster
  schloss, bekam einen Absturz, der in keinem Fehlerhaken landete, weil er in
  einem eigenen Faden passierte. Ohne Netz dauert der Abruf am längsten, also
  traf es genau dann. Jeder Rückweg ins Fenster ist jetzt abgesichert.

- **Ohne Verbindung sagt der Serverstatus das auch.** Vorher stand dort „Noch
  nichts abgerufen. Klick auf „Jetzt nachsehen"" — ein Rat, der ohne Internet zu
  nichts führt und einen den Fehler bei sich suchen lässt. Jetzt: „Keine
  Internetverbindung". Gibt es einen älteren Stand, wird der gezeigt, mit dem
  Hinweis, dass er alt ist.

- **Die Auswahlliste ragt nicht mehr aus dem Bild.** Sie begrenzte sich am
  Bildschirm, nicht am Fenster: Bei 38 Rohstoffen im Bergbau und einem Fenster
  weit unten im Bild lief sie unten heraus und wurde abgeschnitten. Sie ist
  jetzt höchstens so hoch wie das Fenster und rollt, wenn ihr Inhalt länger ist.

- ⚠ **Der Ziehgriff des Overlays fehlte ganz.** Er hing an der Bauplan-Liste —
  eine gute Idee, solange die Liste den Rest des Fensters bekam. Seit die
  Leiste mit den laufenden Aufträgen darüber Platz nimmt, kann die Liste
  **niedriger werden als der Griff selbst**: Bei einem schmalen Overlay mit
  einem laufenden Auftrag blieben ihr rund 20 Pixel, der Griff braucht 26.

  Er hängt jetzt am Fenster und ist in jeder Höhe da — geprüft bei 190, 130 und
  110 Pixeln. Beim Einklappen verschwindet er weiterhin, sonst läge er über dem
  Schliessen-Kreuz. Dazu grösser und in der Akzentfarbe: Er ist der einzige Weg,
  das Overlay in der Grösse zu ändern, und wer ihn nicht sieht, hält die Grösse
  für fest.

- **Das Kreuz zum Ausblenden eines Auftrags ist ein durchgestrichener Kreis
  geworden**, rot und deutlich grösser. Das Kreuz steht im Programm für
  „Fenster schliessen"; hier wird eine einzelne Zeile weggenommen, und das
  sagt der durchgestrichene Kreis besser.

- ⚠⚠ **Das Overlay meldete 405 Baupläne, der Fortschritt 382 von 738.** Zwei
  Zahlen für dasselbe, und keine erklärte die andere.

  Die Ursache lag im Katalog auf der Platte: Er wurde vor Monaten geschrieben,
  als Magazine dort noch `FS-9 Magazine (75 cap)` hießen. Der Bestand führt sie
  längst als `FS-9 Magazine (75)` — die Angleichung der Mengenangabe kam später
  dazu. **23 Magazine und Batterien** galten dadurch überall als fehlend,
  obwohl sie im Bestand standen: im Fortschritt, an den Häkchen der Liste und
  bei „404 von 1597 herstellbar".

  Der Katalog wird beim Laden jetzt neu verschlüsselt — es zählt der Name, nicht
  die Schreibweise von damals. Passt schon alles, wird nichts angefasst.

- ⚠ **Die aufgeklappte Auswahlliste blieb beim Scrollen stehen.** Sie schwebt
  als eigenes Fenster über der Seite; rollt man die Liste darunter weg, lag sie
  quer über fremden Zeilen. Ein Fokuswechsel findet dabei nicht statt, und nur
  darauf hatte sie bisher geachtet.

  Sie schliesst jetzt auch beim **Scrollen**, beim **Verschieben oder
  Vergrössern des Fensters** und auf **Esc**. Innerhalb der Liste selbst darf
  weiter gescrollt werden — die gehört ihr.

- ⚠⚠ **„Nichts gefunden", sobald eine Kategorie gewählt war** — jetzt wirklich.
  Der Filter rechnete richtig, aber **eine zweite Stelle** sortierte ganze
  Gruppen vorab aus und verglich dabei Katalog-Art gegen Oberkategorie. Das
  trifft nie zu, also fiel jede Gruppe heraus. Diese Abkürzung ist weg —
  geprüft wird an genau **einer** Stelle. Gemessen: „Schiffsmodule" zeigt
  wieder 157 von 738, „Generator" 44.

- ⭐ **Der Watcher merkt solche Fälle jetzt selbst.** Steht im Auswahlfeld
  „Schiffsmodule (157)" und die Liste bleibt leer, ist das ein Widerspruch:
  Die eine Zahl kommt aus dem Katalog, die andere aus dem Filter. Der Watcher
  schreibt das in sein Fehlerprotokoll, und es steht im Diagnosebericht — statt
  dass jemand ein Bildschirmfoto schicken muss.

  > Der Fehler war zweimal nur am leeren Bildschirm zu sehen; abgestürzt ist
  > nichts, also stand auch nichts im Bericht. Ein Werkzeug, das solche
  > Widersprüche anzeigt, aber nicht meldet, liegt in der Ecke.

- **Der Bergbau zeigt die Rohstoffe zuerst.** Im Grundzustand standen dort die
  48 Orte — man kommt aber mit „wo finde ich Titanium?" herein, nicht mit „wo
  bin ich?". Die Orte stehen jetzt darunter und beantworten die zweite Frage.

- ⚠⚠ **„Nichts gefunden", sobald eine Kategorie gewählt war.** Die Liste zeigte
  `0 von 738`, obwohl „Schiffsmodule (157)" und „Generator (44)" ausgewählt
  waren. Der Filter selbst rechnete richtig — das **Zeichnen** brach ab.

  > Ursache: Beim Wechsel der Oberkategorie werden die Auswahlfelder neu
  > gebaut. Die alte Anordnungs-Funktion hing aber weiter am Rahmen und griff
  > auf die zerstörten Felder zu (`TclError: bad window path name … !canvas14`,
  > acht Stück im Fehlerprotokoll). Sie brach mittendrin ab, die Felder blieben
  > ungesetzt und die Liste zeichnete nichts mehr.

  Die Anordnung übersteht den Neubau jetzt: Tote Elemente werden übersprungen,
  der Merker für „unverändert" wird beim Neubau geleert.
  — **gefunden über das Fehlerprotokoll**, das den Absturz mitgeschrieben hat,
  obwohl am Bildschirm nur eine leere Liste zu sehen war.

- ⚠ **Die Unterart liess sich auf der Herstellung nicht auswählen.** Man klickte
  sie an, und nichts war gewählt. Die Prüfung „gehört diese Unterart zur
  gewählten Kategorie?" verglich gegen eine Liste aus **Paaren** statt aus
  Werten — sie traf deshalb nie zu, und die Auswahl wurde sofort wieder
  geleert.

- **Die Knopfreihe auf „Fehler melden" verschafft sich jetzt selbst Platz.**
  Reicht die Breite nicht, fordert sie die fehlenden Pixel vom Fenster an,
  statt umzubrechen — bis zur Bildschirmbreite.

  > Zwei feste Mindestbreiten (1100, dann 1160) hatten nicht gereicht: Wie
  > breit ein Knopf wirklich wird, steht erst fest, wenn er gezeichnet ist, und
  > das fällt je nach System anders aus. Eine geratene Zahl kann das nicht
  > treffen — die Reihe muss selbst messen.

- ⚠⚠ **Beobachtungs-Muster trafen mitten im Wort — und meldeten das Falsche.**
  Das Muster `arden backpack` traf auf *W**arden** Backpack Purgatory Camo*:
  Der Watcher meldete ein Rüstungsteil als verfügbar, das mit dem gesuchten
  nichts zu tun hat. Wer sich darauf verlässt, fliegt umsonst los.

  Muster greifen jetzt nur an **Wortgrenzen** — vor und hinter dem Muster darf
  kein Buchstabe und keine Ziffer stehen. Bindestriche und Leerzeichen zählen
  als Grenze, `abc-mk4 legs grey` passt also weiterhin.

  > **Warum das zählt:** Wer ein bestimmtes Ausrüstungsteil beobachtet, meint
  > genau dieses eine. Ein „fast passendes" ist wertlos.

- ⚠ **Auf „Fehler melden" standen die fünf Knöpfe untereinander.** Die
  Mindestbreite des Fensters war 1100 px, die Knopfreihe braucht auf Deutsch
  aber 869 px zuzüglich Seitenleiste und Rändern. Jetzt sind es 1160 px, und
  die Reihe steht nebeneinander.

- **Die Rüstungsrolle ist wieder raus** (Kampf, Technik, Tarnung). Sie war als
  Filter angeboten, aber: *„danach sucht laut Rückmeldung niemand."* Bei
  Rüstung zählen die Körperteile.

- ⭐ **Filter nach Unterart — endlich sieht man, welche Waffe was ist.** In der
  Bauplan-Liste stand unter „Schiffswaffen" alles zusammen: *„ich weiß grad
  nicht, welche Ballistik sind, welche Laser, welche Repeater oder Cannon."*
  Jetzt gibt es ein zusätzliches Auswahlfeld — bei Schiffswaffen mit
  **Ballistisch (32) · Laser (40) · Distortion (6) · Neutron (6) · Tachyon (3)**,
  bei Rüstung mit den **Rollen** (Kampf, Technik, Jagd, Tarnung, Bergbau …).

  > **Es erscheint nur, wenn es etwas zu wählen gibt.** Bei Kühlern gäbe es
  > bloß Größen, und die haben ihr eigenes Feld — ein Auswahlfeld, das nur
  > „alle" anbietet, lässt einen suchen, was es filtern soll.

  Möglich wird das, weil zwei Quellen zusammengeführt werden: Der Katalog kennt
  die Körperteile der Rüstung (Helm, Torso, Arme, Beine), die Rezeptdaten die
  Waffenart. Verbunden über den Namen — **738 von 738** Bauplänen passen.
 

- ⭐ **Die Herstellung hat jetzt dieselben Filter**: Art, Unterart bzw.
  Rüstungsrolle, Hersteller und „Bauplan vorhanden / fehlt". Vorher gab es dort
  nur ein Suchfeld, und wer nicht wusste, wonach er sucht, blätterte 1597
  Zeilen durch.

- **Der Bergbau bekommt Auswahlfelder** für Rohstoff und Ort — 38 und 48
  Einträge, die man vorher auswendig kennen musste, um sie eintippen zu können.

  > Alle drei Seiten benutzen dieselben Bedienelemente wie die Bauplan-Liste.
  > *„egal wo, sollte das Bedienkonzept nicht jedes Mal ändern —
  > die Leute wollen es nutzen und nicht erst lernen, wie sie es nutzen."*

- ⚠ **„Du beobachtest noch nichts", obwohl neun Beobachtungen hinterlegt
  waren.** Die Merkliste führt zwei Sorten: angeklickte Baupläne aus dem
  Katalog — und eigene Beobachtungen mit Suchmustern. Die Ansicht
  zeigte nur die erste Sorte, der Diagnosebericht zählte beide. Jetzt stehen
  die eigenen Beobachtungen mit ihren Suchmustern oben in der Ansicht.
 

- **Beim Herstellen lässt sich eine Anzahl angeben.** Wer zehn Stück am Stück
  baut, klickte bisher zehnmal — und beim elften Klick stimmte der Bestand nicht
  mehr, ohne dass es auffiel. Jetzt steht neben dem Knopf ein Feld: Anzahl
  eintragen, einmal klicken, fertig. Danach springt es von selbst auf 1 zurück,
  damit der nächste Klick nicht unbemerkt wieder zehn abzieht.

- **Das Lager lässt sich ausgeben und wieder einlesen.** Als Sicherung (`.json`)
  — die kommt hier auch wieder herein — oder als Tabelle (`.csv`) zum Ansehen
  und Weitergeben. Die Tabelle nutzt Semikolon und Komma, damit ein deutsches
  Tabellenprogramm sie richtig aufteilt.

  > Der Lagerbestand ist Handarbeit, die sonst nirgends liegt: kein Log, keine
  > Datenquelle, nur deine Eingaben. Ohne Ausgabe wäre sie beim nächsten
  > Rechnerwechsel weg.

- **Der Reiter „Bestand" heißt jetzt „Bauplan-Bestand".** Seit es „Mein Lager"
  gibt, waren zwei Reiter mit dem Wort Bestand einer zu viel — der eine führt
  Baupläne, der andere Rohstoffe.

- **Zwei neue Seiten: „Herstellung" und „Bergbau".** Sie beantworten die Frage,
  die nach dem Bauplan kommt — *was brauche ich dafür, und wo hole ich das?*

  **Herstellung** listet alle **1.597** herstellbaren Gegenstände. Ein Klick
  zeigt die Zutaten mit Menge und die Herstellzeit. Und weil der Watcher deinen
  Bestand kennt, steht an jeder Zeile, ob du den Bauplan hast — bei 404
  Bauplänen sind das 403 Häkchen.

  > Bei **zwei** Zeilen steht ein `?` statt eines Häkchens. Drei Namen meinen
  > mehrere verschiedene Gegenstände („BroadSpec" gibt es in S02 und S03, „Main
  > Powerplant" für Idris und Reclaimer). Der Bestand kennt nur den Namen, nicht
  > die Variante — dann behaupten wir nichts.

  **Bergbau** beantwortet beide Richtungen in einer Suche: Tipp einen Rohstoff
  ein, und du bekommst seine Fundorte (Iron: 27 Orte). Tipp einen Ort ein, und
  du bekommst alles, was es dort gibt (Daymar: 14 Erze). Dazu steht jeweils, ob
  per FPS, Fahrzeug oder Schiff abgebaut wird.

  **Beides hängt zusammen:** Im Rezept ist jeder Rohstoff anklickbar und springt
  direkt zu seinen Fundorten.

  ⚠ **Was der Watcher nicht sagt: ob du es herstellen kannst.** Er kennt deine
  Baupläne, nicht deinen Frachtraum. „Braucht 0,3 SCU Iron" — ja. „Du kannst das
  jetzt bauen" — nie.

  Für Wahrscheinlichkeiten und den Refinery-Vergleich ist **scmdb.net** weiter
  die bessere Adresse; die Seite verweist auch dorthin.

- **Mein Lager — und was deine Materialqualität aus dem Produkt macht.**
  Vorgeschlagen von **Horthy (KRT)** 🙏

  Du trägst ein, was du an Rohstoffen hast: **Material, Menge, Qualität,
  Lagerort**. Im Rezept steht dann an jeder Zutat, ob sie da ist oder wie viel
  fehlt — und ein Knopf **„Das stelle ich jetzt her"** zieht die Zutaten ab,
  ohne dass du rechnen musst.

  **Und die Qualität zählt wirklich.** Die Rezepte tragen mit, wie stark sie die
  Werte des fertigen Stücks verändert — bei **1.524 der 1.597 Baupläne**.
  Deshalb steht im Rezept, was mit *deinem* Material herauskäme:

  ```
  Mit deinem Material
     Damage Mitigation    × 1,044     Ouratite · Q 720
     Min Temp             × 1,088     Aslarite · Q 800
  ```

  Liegt Material da, das die geforderte Qualität nicht erreicht, steht das
  ausdrücklich dabei — sonst hieße es „dir fehlt 0,3", während 12 SCU im Lager
  liegen.

  **Beim Eintragen schlägt der Watcher die Materialien vor**, die es wirklich
  gibt — 26 Stück aus den Rezepten. Wer „Aslerite" tippt, bekommt „Aslarite"
  angeboten, statt stillschweigend nie einen Treffer zu haben.

  **Das Lager ist eine sortierbare Tabelle**: Spaltenköpfe für Material, Menge,
  Qualität und Lagerort sortieren auf Klick, ab sechs Posten kommt ein Filter
  dazu. Zwei Posten desselben Materials an verschiedenen Orten stehen sauber
  getrennt.

  **Am Rezept steht, was du schon hast** — nicht nur was fehlt: „hast 0,02 von
  0,09 · fehlt 0,07". Sonst fliegt man los, um 0,09 zu holen, obwohl 0,07
  reichen.

  **Und du kannst eine Qualität durchspielen.** Ein Regler von 0 bis 1000 zeigt,
  was mit besserem oder schlechterem Erz herauskäme — dieselbe Frage, die man
  sonst auf scmdb.net von Hand stellt, nur mit deinem Lager als Ausgangspunkt.

  ⚠ **Das Lager wird von Hand geführt**, weil das Spiel nichts darüber verrät:
  In 17 MB Protokollen steht kein Wort zu Rohstoffen oder Herstellung. Deshalb
  sagt der Watcher auch nie „du kannst das nicht bauen", sondern nur „dir fehlt
  Iron". Ein Lager, das zwei Einträge hinterherhinkt, darf nicht zum Lügner
  werden.

- **Dein Name im Fehlerbericht.** Auf der Seite „Fehler melden" lässt sich ein
  Name eintragen, der oben im Bericht steht. Damit lassen sich Rückfragen
  zuordnen. **Freiwillig** — leer bleibt leer, und vorausgefüllt wird nie etwas.

- ⚠ **Bei vielen Wegen zu einem Bauplan ließ sich nicht bis nach unten
  rollen.** Wer die Herkunft aufklappte — beim „Hart Scraper Module" sind es
  zwölf Wege —, sah die unteren Einträge nicht und kam auch nicht an sie heran.

  > Ursache: Die Länge der Rollfläche entsteht aus **geschätzten** Zeilenhöhen.
  > Das stimmt, solange jede Zeile gleich hoch ist; ein aufgeklappter Bauplan
  > ist aber ein Vielfaches höher, und die Schätzung wusste nichts davon.

  Die Liste misst jetzt nach, sobald ein gebauter Abschnitt von der Schätzung
  abweicht, rückt die folgenden nach und verlängert die Rollfläche. Das läuft
  von selbst — es hängt nicht daran, dass jemand an jeder Klickstelle daran
  denkt.

- ⚠ **Der Zurücksetzen-Knopf der Bauplan-Liste war nicht zu finden.** Es gab
  ihn — als kleinen grauen Unterstrich-Text unten rechts neben dem
  Trefferzähler. Er wurde vergeblich gesucht und die Filter von Hand zurückgestellt:
  „nervt auf Dauer". Was man nicht findet, ist nicht da.

  Er steht jetzt **oben in der Zustandszeile** neben „alle / habe ich / neu im
  Spiel", ganz rechts und mit Abstand, als Knopf mit Rahmen und ×. Er erscheint
  weiterhin nur, wenn wirklich etwas eingegrenzt ist — und nimmt jetzt **alles**
  zurück: Auswahlfelder, Suchfeld und Zustandswahl. Vorher hätte er nur die
  Auswahlfelder geleert, und die Liste wäre trotzdem gefiltert geblieben.

- **Die Bauplan-Liste startet ohne Filter.** Wer „Andockkragen, Größe 2, Grad A"
  eingestellt und den Reiter später wieder aufgerufen hatte, sah „Nichts
  gefunden" — und konnte das leicht für einen leeren Bestand halten. Auswahl und
  Suchfeld sind beim erneuten Aufrufen leer.

- ⚠ **„Mit deinem Material" stand auch dann da, wenn nichts davon im Lager
  lag.** Rechts meldete die Zeile „dir fehlt: 1.2", darunter wurde trotzdem
  gerechnet — mit dem Standardwert des Reglers, nicht mit deinem Material. Wer
  das liest, hält den Faktor für sein Ergebnis. Die Überschrift sagt jetzt, was
  sie zeigt: „Was Qualität 500 bringen würde", sobald durchgespielt wird oder
  nichts im Lager liegt.

- **Die Suchfelder bei Herstellung und Bergbau merkten sich ihren Inhalt.** Wer
  „titan" gesucht und den Reiter später wieder aufgerufen hatte, sah weiter nur
  Titan — und hielt das leicht für den ganzen Bestand. Sie sind jetzt beim
  erneuten Aufrufen leer.

  > Ursache: Eine Seite wird **einmal** gebaut und danach nur ein- und
  > ausgeblendet. Alles, was frisch sein soll, muss sich dafür eigens anmelden.

- **Beide Suchfelder haben ein × zum Leeren**, sichtbar nur, wenn etwas drinsteht.

- ⚠ **Knöpfe schnitten ihre Beschriftung ab** — auf einem Knopf stand „erung
  speichern" statt „Änderung speichern", und im Overlay endete die Auftragszeile
  mitten im Wort. Das ist kein Schönheitsfehler: Wer ein halbes Wort liest,
  sucht einen Fehler, den es nicht gibt. *„sonst suchen die User
  Symbole, die sie selber abgeschnitten haben."*

  Ursache: Die Fläche wurde mit `measure()` bemessen, gezeichnet wird aber mit
  der Schrift, die das System liefert — und unter **Wayland** steht die erst
  fest, wenn das Fenster angezeigt wird. Jeder Knopf misst jetzt dreimal nach:
  beim Bauen, beim ersten Anzeigen und einmal im Leerlauf. Wächst er, wächst
  der Rahmen mit. Gilt für alle Knöpfe samt Filterreihen.

- ⚠ **Das Overlay ließ sich schmaler ziehen als seine eigene Symbolleiste.**
  Glocke und die Symbole rechts verschwanden einfach — bei 290 px Breite war
  kein einziges mehr zu sehen. Es hat jetzt eine Mindestbreite, die sich an der
  Leiste bemisst (gemessen: 520 px für Titel und zehn Symbole), und eine zu
  klein gespeicherte Größe von früher wird beim Start angehoben.

  > Der erste Anlauf half nichts, weil er die Leiste nach ihrer Wunschbreite
  > fragte — die läuft aber mit `pack_propagate(False)`, gibt die Größe ihrer
  > Kinder also bewusst nicht weiter und meldete **1 Pixel**. Jetzt werden die
  > Elemente einzeln zusammengezählt.

 

- **Die Auftragszeile im Overlay bricht um, statt abgeschnitten zu werden.**

- ⚠ **Ein offenes Fenster kam unter Wayland nicht nach vorn.** Der Klick aufs
  Overlay schien wirkungslos, und es half nur, das Programm neu zu starten —
  *„nen User findet das nervig, und wer's nicht nervig findet, rafft
  es nicht."* Unter Wayland darf sich ein Fenster nicht selbst vordrängen; was
  der Compositor annimmt, ist ein Fenster, das sich **neu anmeldet**. Genau das
  passiert jetzt — aber nur unter Wayland und nur, wenn das Fenster wirklich
  verdeckt ist. Der Tastaturfokus bleibt dabei beim Spiel.

- ⚠ **Knöpfe schnitten ihre Beschriftung ab.** Auf einem Knopf stand „erung
  speichern" statt „Änderung speichern". Ursache: Die Fläche wurde mit
  `measure()` bemessen, gezeichnet wurde aber mit der Schrift, die das System
  wirklich hergibt — weichen die ab, steht der Text über den Rand und wird
  beidseitig gekappt. Jeder Knopf misst jetzt nach dem Setzen seines Textes
  selbst nach. Betraf alle Knöpfe, nicht nur den einen.

- ⚠ **Der Lagerbestand liess sich nicht berichtigen.** Wer sich vertippt oder
  Material weitergegeben hatte, konnte den Posten nur löschen und neu tippen —
  und beim Neutippen entstand leicht ein zweiter Name für dasselbe Material.
  Jetzt **öffnet ein Klick auf eine Zeile** sie oben in den Feldern: Menge,
  Qualität und Lagerort ändern, speichern, fertig.

  > **Auf- und Abbuchen statt Kopfrechnen.** Bei einem offenen Posten kannst du
  > `+5` oder `-2` tippen, dann wird dazugelegt oder abgezogen. Wer alles
  > abgegeben hat, tippt die volle Menge mit Minus — der Posten verschwindet.
  > Mehr, als da ist, lässt sich nicht abbuchen; dann steht der Bestand da.

- ⚠ **Ein Vertipper im Materialnamen machte den Bestand still unbrauchbar.**
  Die Vorschläge liessen sich übergehen: Wer `Aslerite` eintrug, sah eine Liste,
  die richtig aussah — nur fand kein Rezept den Bestand, und niemand erfuhr,
  warum. Der Name wird jetzt **abgeglichen**: Gross- und Kleinschreibung, die
  Bergbau-Schreibweise mit Klammer (`Aslarite (Raw)`), `Aluminium` gegen
  `Aluminum` und ein knapper Vertipper werden auf den richtigen Namen gezogen,
  sichtbar gemeldet. Ist ein Name gänzlich unbekannt, wird **nachgefragt**
  statt eingetragen — mit einem Knopf „Trotzdem eintragen" für den Fall, dass
  du wirklich etwas hast, das in keinem Rezept steht.

- **Das Ortsfeld hiess „Fundort".** Das gehört zum Bergbau. Hier steht, wo dein
  Material **liegt**, also heisst es jetzt „Lagerort" — und bleibt freiwillig,
  weil nicht jeder mehrere Lager hat. **Menge und Qualität sind dagegen Pflicht:**
  Ohne Qualität kann der Watcher nicht rechnen, was dein Material aus dem
  fertigen Teil macht, und genau dafür ist das Lager da.

- **Komma und Punkt gelten beim Eintragen gleich.** Die einen tippen `12,5`,
  die anderen `12.5`. Bisher warf das Komma eine Fehlermeldung.

- **Ein Klick auf das Overlay holt ein offenes Fenster jetzt wirklich nach
  vorn.** Bisher blieb es hinter dem Spiel, und der Klick schien nichts zu tun.
  Ursache: `lift()` allein wird unter **Wayland** ignoriert — dort darf sich ein
  Fenster nicht selbst in den Vordergrund setzen. Jetzt wird kurz „immer oben"
  gesetzt und gleich wieder abgeschaltet; das nimmt der Compositor an. Ein
  **minimiertes** Fenster wird dabei ebenfalls wiederhergestellt, vorher blieb
  es eingeklappt. Betrifft Bauplan-Liste, Einstellungen und „Was ist neu".

  > **Dein Spiel behält die Tastatur.** Das Fenster kommt nach vorn, reißt aber
  > nicht den Eingabefokus an sich — wer gerade fliegt, fliegt weiter. Wer im
  > Fenster tippen will, klickt hinein. Nur beim Programmstart bekommt es den
  > Fokus, denn den hast du ja selbst ausgelöst.

- ⚠ **Der Qualitäts-Regler ruckelte, weil bei jeder Mausbewegung 4 MB von der
  Platte gelesen wurden.** Die Rezeptdatei wurde bei **jedem** Zugriff neu
  eingelesen — 22 ms pro Aufruf, und der Regler ruft bei jedem Pixel. Das waren
  über 600 ms Rechenzeit pro Sekunde. Jetzt bleiben die Daten im Speicher und
  werden nur neu gelesen, wenn sich die Datei wirklich ändert: **0,33 ms statt
  21,9 ms**. Nebenbei werden die Werte beim Ziehen nur noch neu beschriftet
  statt neu aufgebaut — das nahm den Rest des Flackerns.

- ⚠ **Die neuen Daten kamen bei niemandem an, der schon einen Katalog hatte.**
  Der Abruf brach ab, sobald der Bauplan-Katalog aktuell war — und das ist er
  bei jedem bisherigen Nutzer. Herstellung, Bergbau und Lager wären dauerhaft
  leer geblieben, bis Star Citizen das nächste Mal patcht. Die beiden Abrufe
  werden jetzt **immer** geprüft; sie bringen ihre eigene „schon aktuell?"-Frage
  mit und laden nichts doppelt.
- **Die Qualitätsskala stand falsch da.** Im Lager hieß das Feld „Güte %" und
  zeigte Werte als „720 %" an. Die Rezepte rechnen aber mit **0 bis 1000**. Wer
  im Spiel „72" abliest und einträgt, hätte danach lauter falsche Ergebnisse
  bekommen — sein Erz gälte als unbrauchbar, obwohl es gut ist.

- **„Netzfehler", wo die Seite den Abruf nur abgelehnt hatte.** Ein 403 ist eine
  Absage, kein Wackelkontakt: Das Werkzeug sagt das jetzt klar, arbeitet mit dem
  zuletzt geladenen Stand weiter — und wiederholt den Versuch nicht mehr dreimal
  (das kostete sechs Sekunden für nichts).

### Dank

Die Idee zum Rohstoff-Lager kam von **Horthy (KRT)** — und aus ihr ist die Qualitätsrechnung geworden, die jetzt zeigt, was das eigene Material aus einem Bauplan macht. Danke dafür 🙏

Dazu **Krovax** (SCMDB), der auf Anfrage eigens einen öffentlichen Datenspiegel eingerichtet hat, damit Werkzeuge wie dieses eine verlässliche Quelle haben.

## v3.2.1 - 2026-08-29

### Behoben

- **Andere Werkzeuge werden nicht mehr überschrieben.** Drei Programme
  kennzeichnen Bauplan-Aufträge im Spiel, und alle drei benutzen dieselbe Marke
  `[BP]`: dieses hier, **MrKrakens StarStrings** und der **SC Deutsch Launcher**
  (Watcher und Launcher schöpfen sogar aus derselben Datenquelle und schreiben
  darum wortgleiche Listen). Bisher hat der Watcher nicht unterschieden, was von
  ihm stammt und was nicht. Alles an der echten Fassung vom 29.08.2026
  nachgezählt:

  - **17** von MrKrakens Kennzeichnungen wurden beim Eintragen **gelöscht** —
    und weil sich der Watcher danach den bereits gekürzten Wortlaut als
    Urfassung merkte, kamen sie auch beim Zurücksetzen nie wieder.
  - **297** weitere standen danach **doppelt**.
  - **136** Gegenstandsnamen bekamen ihr Kürzel zweimal:
    `[CS1] Spark-G Missile (CS1)`.
  - Wer den **SC Deutsch Launcher** parallel benutzt, hätte die Bauplan-Liste
    an **336** Aufträgen zweimal untereinander gelesen und beim Zurücksetzen
    den Stand des Launchers verloren.

  **Die neue Regel ist einfach: Wo schon eine Marke steht, kommt keine zweite
  dazu.** Und was vor der ersten eigenen Einfügung dastand, gehört dem Spieler —
  es wird beim Zurücksetzen wiederhergestellt, auch wenn es von einem anderen
  Werkzeug stammt.

  Beim Launcher geht der Watcher einen Schritt weiter: Seine Liste **ersetzt**
  dessen Liste, statt eine zweite danebenzusetzen. Denn sie ist dieselbe — nur
  mit **Kästchen**, also mit dem Abgleich gegen deine eigenen Baupläne. Nimmst du
  die Angaben zurück, steht seine Liste wieder da, Zeichen für Zeichen.

  Trägt ein Gegenstandsname schon ein Kürzel in eckigen Klammern, bleibt er, wie
  er ist.

  **Danke an MrKraken** für [StarStrings](https://github.com/MrKraken/StarStrings)
  und an das Team des **SC Deutsch Launchers** — und Entschuldigung für das
  Hineinschreiben. 🙏

- **Der Watcher meldete „Angaben stehen im Spiel", wo nichts von ihm stand.**
  Erkannt wurde die Injektion an der Marke `[BP]` und an der Überschrift der
  Bauplan-Liste — beides schreiben die anderen beiden Werkzeuge genauso. Jetzt
  zählt nur, was es ausschließlich beim Watcher gibt: das **Kästchen**.

- **Kästchen standen vor Regionen und Abgabeorten.** Im Spiel las man
  `[  ] Stanton-System - Gefahr 4-6/10`, als könnte man eine Region besitzen.
  Ursache: Die Bauplan-Blöcke gliedern mit Überschriften, und unter dreien davon
  stehen Listen — `# Baupläne` (4.379 Zeilen), `# Abgabe` (323) und `# Region`
  (239). Angekreuzt wurde jede. Jetzt bekommt nur ein Kästchen, was unter
  **Baupläne** steht; in einer fertigen Datei fallen damit **838** falsche
  Kästchen weg. Auf Englisch (`# Blueprints`, `# Delivery`) genauso.

- **Nach dem Einsetzen einer neuen Grundlage wird die Urfassungs-Merkdatei
  geleert.** Sie gehörte zur alten Datei und hätte auf einen überholten Stand
  zurückgeschrieben. Zugleich schützt der Vermerk die frische Datei: In etwas,
  das eben erst eingesetzt wurde, hat der Watcher noch nie geschrieben — also
  gibt es dort auch nichts von ihm zu entfernen.

### Geändert

- **MrKraken steht jetzt in der Danksagung der Anleitung.** Auf der Seite
  „Danke & Lizenzen" im Programm stand er längst, in der Anleitung fehlte er.
- **Die Lizenzangabe zu StarStrings ist berichtigt.** Dort stand
  „CC BY-NC-SA 4.0". Das Projekt gibt gar keine Lizenz an — weder im Repo noch
  in seiner Anleitung. Eine Lizenz zuzuschreiben, die der Autor nie vergeben
  hat, ist falsch; jetzt steht dort „keine Lizenzangabe".

## v3.2.0 - 2026-08-29

### Neu

- **Der Watcher sagt dir beim Annehmen eines Auftrags, ob Baupläne dabei sind —
  und welche dir davon noch fehlen.** Bisher erfuhrst du es erst, wenn der
  Bauplan kam. Jetzt steht es in der Liste, sobald du den Auftrag annimmst:

  ```
  Auftrag angenommen: Retake Platforms From Nine Tails
    →  3 Baupläne · dir fehlt: H4-PBF Ammo Carrier
  ```

  Es ist bewusst **keine Auftragsverwaltung**: keine Liste, kein Reiter, kein
  zweites Fenster. Eine Zeile wie bei einem Bauplanfund auch. Das Werkzeug
  bekommt keine zweite Aufgabe — es beantwortet seine eigene früher.

  **Kennt der Katalog den Auftrag nicht, wird geschwiegen.** Eine falsche
  Bauplan-Zusage wäre schlimmer als gar keine Meldung.

  Erkannt wird die Annahme über den Schlüssel `mobiGlas_ui_MissionEvent_Activated`
  aus der Spieldatei, nicht über den Wortlaut — auf Deutsch heißen sonst auch die
  **Zwischenziele** „Neuer Auftrag", und die Meldung käme bei jedem Etappenziel.
  Läuft dein Spiel auf Englisch, funktioniert es genauso.

### Geändert

- **Der Dank an die Tester steht nicht mehr in der Anleitung.** Er gehört in das
  Änderungsprotokoll und auf die Seite „Danke & Lizenzen" im Programm — dort ist
  er weiterhin vollständig.

## v3.1.0 - 2026-08-29

### Neu

- **Nachgelesene Baupläne werden gemeldet, nicht nur still eingetragen.** Findet
  der Watcher beim Start oder auf Knopfdruck etwas in den Protokollen, steht es
  jetzt in der Liste — gekennzeichnet mit *nachgelesen*, damit es nicht wie ein
  Fund von eben aussieht.

  Bis zu zehn Stück einzeln; darüber bleibt es bei der Summe in der Statuszeile.
  Der Grund für diese Grenze: Beim allerersten Start geht die Nachlese über
  **alle** aufgehobenen Sitzungen — auf einem gewachsenen Rechner sind das über
  hundert, und die will niemand einzeln wegklicken. Im Alltag sind es null bis
  drei, und genau die will man sehen.

### Behoben

- **Derselbe Bauplan zählte zweimal, wenn das Spiel auf Deutsch läuft.** Der SC
  Deutsch Launcher liest den **englischen** Katalog und schreibt
  `Ravager-212 Twin Shotgun Magazine (16 cap)`. Die Nachlese aus den Protokollen
  liest dieselbe Kiste in der Sprache, in der Star Citizen läuft — auf Deutsch
  also `… (16 Schuss)`. Für den Watcher waren das zwei verschiedene Baupläne.

  Gemessen an einem echten Bestand: **405 angezeigt, 403 vorhanden.** Der Fehler
  ist still — es geht nichts kaputt, es steht nur eine zu große Zahl da.

  Die Mengenangabe in Klammern wird jetzt entsprachlicht: `(16 Schuss)` und
  `(16 cap)` sind derselbe Bauplan. **Die Zahl bleibt stehen** — ein 40er- und
  ein 60er-Magazin sind verschiedene Baupläne und müssen es bleiben. Klammern
  ohne führende Ziffer bleiben unangetastet, `Singe Cannon (S2)` heißt weiter so.

  Ein bereits gespeicherter Bestand zieht beim nächsten Start automatisch mit —
  die Dubletten werden zu einem Eintrag zusammengeführt, wobei der **ältere**
  Fund gewinnt.

- **„Mit System starten" funktionierte unter Linux nie.** In die Autostart-Datei
  schrieb der Watcher den **temporären Einhängepunkt** des AppImage
  (`/tmp/.mount_SC-BP-ji95vH/…`). Der bekommt bei jedem Start einen neuen
  Zufallsnamen — nach einem Neustart des Rechners zeigte der Eintrag ins Leere
  und der Watcher kam nicht hoch. Ohne Fehlermeldung: Die Datei sah richtig aus.

  Ursache war die Reihenfolge im Code. Ein AppImage gilt ebenfalls als
  „eingefroren", deshalb gewann diese Abfrage, und der Pfad zur echten
  AppImage-Datei kam nie an die Reihe. Jetzt andersherum.

  Gefunden am 29.08.2026 auf einem Rechner, auf dem der Eintrag seit dem Umstieg
  auf Linux tot dalag.


- **Das schwebende Schloss saß sieben Pixel zu weit rechts.** Der Ausgleich
  dafür stammte aus einer Messung auf einem **anderen Bildschirm** (5120×1440
  statt 4096×1152) — dort sind die Symbole 24 px breit statt 22, und ein in
  Pixeln gemessener Ausgleich gilt genau für den einen Bildschirm.

  Am laufenden Programm nachgemessen: Ohne Ausgleich sitzt es deckungsgleich.
  Er steht wieder auf null.

## v3.0.3 - 2026-08-28

### Behoben

- **An drei Stellen stand der Schlüsselname statt des Textes.** Am auffälligsten
  am Raketen-Symbol: Der Hinweis dort lautete wörtlich `s_sp_start`. Jetzt steht
  da, was gemeint war — „Star Citizen starten".

  Die beiden anderen wären beim nächsten fehlgeschlagenen Herunterladen und im
  Versionsfenster aufgetaucht.

  Der Grund ist ein Notnagel, der zu gut versteckt: Kennt die Sprachtabelle
  einen Schlüssel nicht, gibt sie **den Schlüssel zurück**. Das ist besser als
  ein Absturz — aber der Fehler bleibt unsichtbar, bis ihn jemand im laufenden
  Programm sieht.

  Der Selbsttest prüft das jetzt: Er sammelt **jeden** Aufruf mit festem
  Schlüssel aus dem ganzen Programm und gleicht ihn gegen die Tabelle ab. Bei
  über 600 Einträgen ist das von Hand nicht zu halten — gefunden hat die drei
  auch kein Mensch, sondern diese Prüfung.

  Gemeldet von **der Autor** am 28.08.2026.

### Geändert

- **„Täglich nach neuen Versionen sehen" hieß es, stündlich war es.** Der
  Abstand steht seit jeher bei einer Stunde; der Text daneben sagte etwas
  anderes. Aufgefallen ist es erst, seit die Prüfung tatsächlich wiederholt
  läuft.

## v3.0.2 - 2026-08-28

### Behoben

- **Ein laufender Watcher erfuhr nie von einer neuen Fassung.** Die Meldung kam
  erst nach einem Neustart — wer das Programm tagelang durchlaufen lässt, sah
  nie etwas.

  Nachgesehen wurde **genau einmal**, zwei Sekunden nach dem Start. Der
  Stundenabstand in der Abfrage begrenzt nur, wie oft gefragt werden *darf*;
  fragen muss trotzdem jemand. Das passiert jetzt stündlich.

  Gemeldet von **der Autor** am 28.08.2026: v3.0.1 war draußen, der laufende
  Watcher schwieg — obwohl er sie längst abgerufen hatte und sie in seinem
  Zwischenspeicher stand.

- **Ein erwarteter Fehler machte das Fehlerprotokoll unbrauchbar.** Beim
  Herunterladen kommt der Fortschritt im Sekundentakt; geht dabei das Fenster
  zu, scheitert jede einzelne Meldung — abgefangen, aber jedes Mal
  protokolliert.

  In einem Bericht waren dadurch **50 von 50** Plätzen mit derselben Zeile
  belegt, alle innerhalb von acht Sekunden. Jeder echte Fehler war daraus
  verdrängt. Diese Meldung wird jetzt nur beim ersten Mal festgehalten.

## v3.0.1 - 2026-08-28

### Behoben

> [!important]
> **War der Watcher zu, während Star Citizen weiterlief, gingen die Baupläne
> dieser Sitzung verloren** — und zwar dauerhaft. Wer das kennt: einmal auf den
> neuen Knopf **Protokolle erneut einlesen** drücken, dann sind sie da.

- **Die laufende `Game.log` wurde beim Start nur beim allerersten Mal gelesen.**
  Danach galt sie als erledigt: Das Mitlesen setzte beim gemerkten Stand an, und
  alles davor war unerreichbar. In den Sicherungsordner wandert die Datei erst
  beim nächsten Spielstart — bis dahin fehlte der Bauplan, ohne dass irgendwo
  etwas darauf hindeutete.

  Nachgemessen: Der Bauplan stand bei Byte 11.987.664, der Lesestand bei
  12.759.872. Er wäre nie gefunden worden.

  Die laufende Datei wird jetzt bei jedem Start ganz gelesen. Das kostet den
  Bruchteil einer Sekunde — die Nachlese geht ohnehin über alle Sicherungen —
  und doppelte Einträge kann es nicht geben, der Bestand prüft jeden Namen.

  Gemeldet von **der Autor**, wenige Stunden nach v3.0.0.

- **Nach einem Spielneustart sprang der Lesestand ans Dateiende statt an den
  Anfang.** Legt Star Citizen eine frische `Game.log` an, ist sie kürzer als der
  gemerkte Stand. Der Kommentar an der Stelle sagt richtig „dann lief eine neue
  Spielsitzung" — der Code setzte aber auf das **Ende** der neuen Datei, statt
  von vorn zu lesen. Alles, was die frische Sitzung schon gemeldet hatte, war
  damit übersprungen.

### Neu

- **Ein Knopf „Protokolle erneut einlesen"** — in der Titelleiste des Overlays
  und in den Einstellungen unter *Bestand*. Er sieht jede aufgehobene Sitzung
  noch einmal durch, auch die schon gelesenen, und trägt nach was fehlt.

  Hilft nicht nur im Fall oben, sondern auch dann, wenn beim ersten Lauf die
  Spielsprache noch nicht erkannt war: Dann wurden die Protokolle mit der
  falschen Formulierung durchsucht und trotzdem als gelesen abgehakt.

### Geändert

- **Zwei Texte, die nicht mehr stimmten.** Der Hinweis am Schloss beschrieb es
  „oben rechts am Overlay" — dort steht es seit v3.0.0 nicht mehr. Und die
  Erklärung in den Einstellungen schickte zum Zurückholen noch zu einem zweiten
  Programmstart, obwohl das Schloss genau dafür da ist.

## v3.0.0 - 2026-08-28

> [!important]
> **Unter Windows gibt es jetzt einen Installer statt einer einzelnen `.exe`.**
> Beim Update öffnet sich deshalb einmal ein Installationsfenster — das ist
> richtig so und keine fremde Software. Danach startet der Watcher von selbst
> wieder. Unter Linux bleibt es bei einer Datei: dem AppImage.
>
> **Der SC Deutsch Launcher wird nicht mehr gebraucht.** Die Baupläne kommen aus
> Star Citizens eigener `Game.log`. Wer den Launcher hat, behält deutsche
> Bezeichnungen und ein paar Zusatzangaben — wer nicht (unter Linux immer), dem
> fehlt nichts Wesentliches.

Ein Jahr nach der ersten Fassung ist aus der schmalen Melde-Leiste ein Werkzeug
geworden, das die Frage „welchen Bauplan habe ich, und wo bekomme ich den Rest?"
vollständig beantwortet — ohne aus dem Spiel zu gehen.

### Das Wichtigste

- **Ein eigenes Fenster mit allem drin.** Bauplan-Liste zum Durchsuchen und
  Abhaken, Fortschritt nach Bereichen, Einstellungen, Serverstatus, „Was ist
  neu" — statt verstreuter kleiner Fenster.
- **Herkunft je Bauplan.** Ein Klick zeigt Fraktion, Auftrag, nötigen Rang und
  Belohnung — für **655 von 722** Bauplänen, sortiert nach dem leichtesten Weg.
  „Mir fehlt X" ist die halbe Auskunft; „X gibt es bei Foxwell ab Veteran" ist
  die ganze.
- **Neu im Spiel.** Ein Filter zeigt, was der aktuelle Patch gebracht hat, ein
  Auswahlfeld dazu jeden früheren Patch. Jeder Bauplan trägt die Spielversion,
  in der es ihn zuerst gab.
- **Angaben im Spiel.** Der Watcher schreibt in die Auftragstexte, **welche**
  Baupläne ein Auftrag ausschüttet — mit `[x]` für die, die du schon hast. Und
  auf Wunsch Klasse, Größe und Gütegrad an den Gegenstandsnamen, sodass am
  Traktorstrahl „Glacier (Mil/1/A)" steht statt nur „Glacier".
- **Das Overlay macht Platz, wenn du es brauchst.** Auf Wunsch blendet es nur
  noch bei einem Neuzugang kurz auf; Mausklicks lassen sich ins Spiel
  durchreichen, und ein Schloss in der Leiste holt es zurück. Einklappen geht
  auch, dann bleibt nur die Titelzeile stehen.
- **Fehler melden ohne Rätselraten.** Ein roter Knopf sammelt System, Fassung,
  Spielstand und die letzten Fehler in einen Bericht — ohne Namen und ohne
  Pfade. Das ist der Grund, warum die Fehler in diesem Änderungsprotokoll so
  genau beschrieben sind.
- **Deutsch und Englisch, vollständig.** Umschaltbar im Programm. Die
  Bauplan-Meldung im Log erkennt der Watcher in **jeder** Spielsprache — er
  findet die Formulierung selbst heraus.
- **Windows und Linux aus einer Codebasis**, mit Autostart, Selbst-Update und
  Ablagesymbol auf beiden.

### Dank

Ohne die drei hier wäre v3.0.0 deutlich schlechter — sie haben auf ihren eigenen
Rechnern getestet und Fehler so beschrieben, dass sie zu finden waren:

- **Bomb20** (pr0) — dass das Werkzeug unter Linux nicht aktuell zu halten war,
  dazu der Absturz beim allerersten Start und ein Vormittag mit vier Funden, die
  sonst jeden Nutzer getroffen hätten.
- **Haldjas** (pr0) — der Aufblend-Betrieb und die durchgereichten Mausklicks
  gehen auf ihn zurück; ebenso der Weg **hin und zurück** zum Durchreichen, das
  Setup, das an der laufenden Datei abbrach, und die Konsolenfenster beim
  Update.
- **Morkhan** — die Angaben am Gegenstand im Spiel, und der Fund,
  dass sich mehrere Preisstufen eines Auftrags im Katalog gegenseitig
  überschrieben: **797 Baupläne** hatte davor nie jemand gesehen.

Die vollständige Liste jeder einzelnen Änderung steht in den Abschnitten
`v3.0.0-rc1` bis `v3.0.0-rc99` darunter.

## v3.0.0-rc99 - 2026-08-28

### Behoben

- **Das grüne Schloss lag nicht genau auf dem Schloss in der Leiste.** Rechts
  schaute dadurch ein schmaler Rand des Symbols darunter hervor — es sah aus wie
  zwei Schlösser statt wie eines, das die Farbe wechselt.

  Der Versatz wurde aus einem Bildschirmfoto **ausgemessen**, nicht geschätzt:
  Das obere Schloss stand bei x=1068–1091, vom unteren war nur x=1094–1098 zu
  sehen. Bei 24 px Breite beginnt das untere damit bei 1075 — **7 px weiter
  rechts**. Genau um diesen Wert rückt das obere jetzt nach.

  ⚠ Der Wert ist gemessen, seine **Ursache nicht gefunden**: In einem Nachbau
  mit gleicher Tk-Fassung und gleichen Symbolen sitzt das Schloss ohne Ausgleich
  exakt. Er steht deshalb als benannte Konstante an einer Stelle und gilt nur
  für den sichtbaren Zustand — der Aufblend-Betrieb rechnet anders und bleibt
  unangetastet.

## v3.0.0-rc98 - 2026-08-28

### Behoben

- **Das Schloss war deckender als das Overlay darunter.** Wer die
  Durchsichtigkeit heruntergestellt hat, sah beim Durchreichen zwei Schlösser
  mit verschiedener Sättigung übereinander — das in der Leiste schien durch, das
  darüber nicht.

  Ein eigenes Fenster erbt die Durchsichtigkeit des Hauptfensters **nicht**; sie
  muss ihm eigens gegeben werden. Jetzt tragen beide denselben Wert, und es
  sieht aus wie ein Schloss, das die Farbe wechselt — so wie es gedacht ist.

## v3.0.0-rc97 - 2026-08-28

### Behoben

- **Auf einem zweiten Bildschirm sprangen Streifen und Schloss auf den falschen
  Monitor.** Betroffen war der Aufblend-Betrieb: Wer das Overlay auf einem
  Monitor **oberhalb** des Hauptbildschirms liegen hat, fand den grünen Streifen
  samt Schloss an der Oberkante des Hauptmonitors wieder.

  Ein Monitor über dem Hauptbildschirm arbeitet mit **negativen** Y-Werten —
  das ist keine kaputte Angabe, sondern eine gültige. Beim Merken der Position
  wurde das ausdrücklich berücksichtigt, beim Anzeigen dann wieder verworfen:
  Ein `max(0, …)` klemmte jede Höhe unterhalb von null auf die Oberkante des
  Hauptmonitors.

  Der Streifen hatte diese Zeile von Anfang an; das Schloss hat sie beim Umzug
  an den Streifen (rc94) geerbt. Beide sind sie los.

## v3.0.0-rc96 - 2026-08-28

### Behoben

- **Beim Zublenden brauchte das Schloss drei Sekunden zurück an seinen Platz.**
  Blendet sich das Overlay im Aufblend-Betrieb weg, gehört das Schloss wieder an
  den Anfasser-Streifen — es blieb aber erst noch dort stehen, wo eben die Leiste
  war.

  Es waren **genau** die zehn Nachfass-Versuche à 300 ms aus rc92. Die sind für
  den Start gedacht, wo die Leiste gleich kommt: Solange sie noch gezeichnet
  wird, wartet das Schloss, statt an eine geratene Stelle zu springen. Nur lief
  dieses Warten auch dann, wenn das Overlay gerade **absichtlich** verschwunden
  ist — Warten auf etwas, das nicht kommt.

  Beide Fälle sehen am Knopf gleich aus, am Fenster aber nicht. Nachgemessen:

  | Fall | Fenster | Knopf |
  |---|---|---|
  | Start, wird noch gezeichnet | 1 | 0 |
  | absichtlich weggeblendet | 0 | 0 |

  Gefragt wird jetzt das Fenster. Ist es weg, springt das Schloss sofort.

  Gemeldet von **Haldjas (pr0)** am 28.08.2026 — samt der genauen Trennung von
  den sechs Sekunden, die das Overlay selbst noch stehen bleibt: „wenn der
  watcher minimiert wurde, dauert es nochmal 3 sekunden".

## v3.0.0-rc95 - 2026-08-28

### Geändert

> [!important]
> **Ein gefundener Bauplan ist ab sofort grün — kein gelbes „vorläufig" mehr.**
> Wer den SC Deutsch Launcher installiert hat, sah jeden Fund aus der `Game.log`
> zuerst gelb, bis der Launcher ihn bestätigte. Diese Bestätigung gibt es nicht
> mehr, und das gelbe Warten damit auch nicht.

- **Der Wartezustand ist raus, nicht nur die Farbe.** Der gelbe Punkt hieß „aus
  der Game.log gelesen, wartet auf Bestätigung durch den Launcher". Seit die
  `Game.log` die Quelle ist und der Launcher nur noch ergänzt, kann diese
  Bestätigung gar nicht mehr kommen.

  Übrig geblieben war ein Zustand, aus dem nichts mehr herausführt: Wer den
  Launcher hatte, sah dauerhaft Gelb — wer ihn nicht hat, dauerhaft Grün, bei
  **genau derselben Sicherheit**. Zwei Farben für dieselbe Aussage sind keine
  Auskunft, sondern eine Sackgasse.

  Entfernt wurde die ganze Mechanik, nicht nur die Anzeige: der Merker für
  unbestätigte Zeilen, die Zuordnung von Log-Namen zu Launcher-Schlüsseln, das
  Nachträgliche-Bestätigen einer Zeile, der Text „vorläufig" — und der gelbe
  Punkt aus der Anleitung, damit niemand nach einem Symbol sucht, das es nicht
  gibt.

  Der Launcher bleibt, was er ist: eine Ergänzung. Deutsche Bezeichnungen,
  gepflegte Angaben zu Typ, Größe und Gütegrad, und er meldet nach, was im Log
  fehlte.

## v3.0.0-rc94 - 2026-08-28

### Verbessert

- **Im Aufblend-Betrieb sitzt das Schloss jetzt am Anfasser-Streifen.** Es stand
  an der rechten oberen Ecke der gemerkten Overlay-Lage — richtig gerechnet,
  aber einsam: Der Streifen, der zeigt wo das Overlay wartet, sitzt mittig, das
  Schloss gut zweihundert Pixel weiter rechts, wo nichts zu sehen ist.

  Zwei Marken für dieselbe Sache gehören zusammen. Jetzt liest es sich als
  eines: hier wartet das Overlay, und hier ist das Schloss.

  Gemeldet von **Haldjas (pr0)** am 28.08.2026: „das schloss sitzt jetzt neben
  dem watcher".

## v3.0.0-rc93 - 2026-08-28

### Behoben

- **Im Aufblend-Betrieb schwebte das Schloss neben dem Overlay.** Der Fix aus
  rc92 griff für alle, die das Overlay dauerhaft sehen — im Betrieb „nur bei
  einem Neuzugang" blieb es beim alten Verhalten.

  Der Grund: Dort wird das Overlay beim Start **versteckt**, bevor es je
  gezeichnet wurde. Damit gibt es keine Leiste, an der sich das Schloss
  ausrichten könnte, und die Ersatzrechnung nahm die Lage eines unsichtbaren
  Fensters — nachgemessen meldet ein nie gezeichnetes Fenster Breite 1 und
  Position 0. Das Schloss landete irgendwo neben dem Overlay.

  Es hängt jetzt an derselben gemerkten Position wie der Anfasser-Streifen, der
  im Aufblend-Betrieb ohnehin zeigt, wo das Overlay wartet — und rückt auf die
  Leiste, sobald das Overlay aufblendet.

  Gemeldet von **Haldjas (pr0)** am 28.08.2026. Sein Fehlerbericht hat es
  entschieden: Ohne die Zeile `overlay_modus=popup` darin wäre weiter geraten
  worden, warum es bei ihm auftritt und bei anderen nicht.

## v3.0.0-rc92 - 2026-08-28

### Behoben

- **Nach dem Start stand das Schloss neben dem Overlay statt darauf.** Wer das
  Durchreichen eingeschaltet gespeichert hatte, sah nach jedem Start **zwei**
  Schlösser: eines an der falschen Stelle neben dem Fenster, eines in der
  Leiste. Erst das erste Umschalten rückte es an seinen Platz — und beim
  nächsten Start ging es wieder von vorn los.

  Die Ursache ist eine alte `tkinter`-Falle: Der Zustand wird unmittelbar vor
  dem Start der Fensterschleife angewendet. Die Leiste steht da zwar schon im
  Baum, aber Tk hat noch nichts gezeichnet — weder „ist sichtbar" noch die Maße
  stimmen zu diesem Zeitpunkt. Das Schloss wurde also an einen geratenen Platz
  gesetzt.

  Jetzt wird **gewartet statt geraten**: Solange die Leiste noch nicht steht,
  wird gar kein Schloss gebaut, sondern nachgefasst, bis sie da ist. Ein kurz
  aufblitzendes falsches Schloss wäre nur die halbe Reparatur gewesen.

  Gemeldet von **Haldjas (pr0)** am 28.08.2026, mit dem vollständigen Ablauf zum
  Nachstellen: „Starte Watcher — Schloss ist an 2 Positionen … position bleibt so
  bis man den watcher neu startet".

## v3.0.0-rc91 - 2026-08-28

### Verbessert

- **Ein Schloss statt zwei.** Bisher saß das grüne Schloss in der Ecke des
  Overlays, während in der Leiste weiter ein offenes stand — zwei Schlösser,
  von denen eines das Gegenteil des wahren Zustands zeigte.

  Jetzt liegt das grüne Schloss **passgenau über** dem in der Leiste: gleiche
  Stelle, gleiche Größe, gleiches Bauteil. Für den Spieler ist es ein Schloss,
  das die Farbe wechselt — zu und grün heißt „Klicks gehen ins Spiel", offen
  und grau heißt „das Overlay fängt sie ab". Entsperrt wird an derselben Stelle,
  an der man zugesperrt hat.

  Ein **eigenes Fenster** bleibt es trotzdem, und das lässt sich nicht ändern:
  Wer Klicks durchreicht, reicht sie für das ganze Fenster durch — ein Knopf in
  der Leiste wäre in dem Moment genauso wenig zu treffen wie der Rest. Ist die
  Leiste eingeklappt oder das Overlay im Pop-up-Betrieb versteckt, fällt das
  Schloss auf seinen alten Platz in der Ecke zurück.

## v3.0.0-rc90 - 2026-08-28

### Verbessert

- **Das Schloss steht jetzt fest in der Leiste des Overlays.** Klicks ins Spiel
  durchreichen ging bisher nur über Einstellungen → Overlay; zurück kam man
  bequem über das Schloss, das dabei erscheint.

  Ein Weg hin und her gehört an dieselbe Stelle. In der Titelleiste steht
  deshalb ein **offenes** Schloss — es heißt „das Overlay fängt Klicks ab". Ein
  Klick sperrt zu, und ab dann übernimmt das schwebende Schloss oben rechts, wie
  bisher. Kein Umweg über die Einstellungen mehr.

  Der Knopf erscheint nur dort, wo das System Klicks überhaupt durchreichen kann
  — unter nativem Wayland wäre er wirkungslos. Klappt es wider Erwarten nicht,
  wird die Einstellung zurückgenommen, statt ein „an" zu speichern, das nichts
  bewirkt.

  Vorgeschlagen von **Haldjas (pr0)** am 28.08.2026: „man kann das durckclicken
  entfernen, aber eventuell kann der button zum locken stehen bleiben? sonst
  muss man ja erst wieder in die einstellungen".

## v3.0.0-rc89 - 2026-08-28

### Behoben

- **Das Auswahlfeld versprach mehr, als die Liste zeigte.** Nach dem Fix an
  der Patch-Historie stand im Feld „4.10.0 (24)" — darunter drei Zeilen.

  Zwei Ursachen, beide dieselbe Art Fehler:

  **Zwei Quellen für dieselbe Frage.** Das Feld zählte die Historie, der Filter
  prüft den Stempel `seit` im Katalog. Die Zahl in Klammern ist aber eine
  Zusage, wie viele Zeilen kommen. Gezählt wird jetzt der Katalog — was nicht
  gestempelt ist, kann die Liste ohnehin nicht zeigen.

  **Und der Stempel kam zu spät.** Nachgezogen wurde er nur im Netz-Takt, der
  irgendwann nach dem Start in einem eigenen Faden läuft. Gemessen am
  28.08.2026: Fenster um 10:44:02 gebaut, Katalog um 10:44:03 fertig gestempelt
  — eine Sekunde zu spät, und die Liste blieb bis zum nächsten Öffnen falsch.
  Das Fenster stempelt jetzt selbst nach, **bevor** es den Katalog liest. Das
  trifft jeden Nutzer beim ersten Start nach einer Fassung mit neuer Historie.

## v3.0.0-rc88 - 2026-08-28

### Behoben

- **Der Patch-Filter verlor fast den ganzen Patch.** Im Auswahlfeld stand
  „4.10.0 (3)", und die Liste zeigte drei Schiffswaffen. In Wahrheit hat 4.10.0
  **24** Baupläne gebracht — die 21 mitgelieferten waren aus der Anzeige
  verschwunden.

  Ursache: Das Programm legte die selbst beobachtete Historie über die
  mitgelieferte. Bei gleicher Spielversion gewann die eigene komplett. Nur:
  Was das Programm selbst einträgt, ist immer bloß der **Zuwachs seit dem
  letzten Lauf** — hier drei Waffen, die die Quelle zwei Tage später
  nachreichte. Als vollständige Patch-Liste gelesen ist das zwangsläufig falsch.

  Beide Listen werden jetzt **vereinigt** statt ersetzt, und beim Datum gilt
  das frühere. Das gleiche galt für zwei eigene Funde nacheinander: Der zweite
  löschte den ersten. Auch das ist behoben.

### Verbessert

- **Der Diagnosebericht nennt jetzt die Patch-Historie.** Eine neue Zeile
  unter dem Katalogstand: welche Spielversionen die Historie führt und mit
  wie vielen Bauplänen — zum Beispiel `4.10.0 (24)`.

  Der Fehler oben konnte sich verstecken, weil der Bericht nur den Katalogstand
  zeigte. Der war völlig in Ordnung, die Historie darunter nicht. Wer jetzt
  „der Patch-Filter zeigt fast nichts" meldet, hat die Zahlen im Bericht
  stehen, ohne dass jemand erst eine Datei aufmachen muss.

## v3.0.0-rc87 - 2026-08-28

### Verbessert

- **Die Sicherheitsabfragen sehen jetzt aus wie der Rest des Programms.**
  Bisher kam an drei Stellen der graue System-Kasten von Tk: heller Hintergrund
  im dunklen Fenster, fremde Schrift — und schmal und hoch, sodass ein längerer
  Satz zu einer Säule wurde.

  Jetzt ist es ein eigener Dialog in denselben Farben und mit denselben Knöpfen
  wie überall sonst, **breit statt hoch** (620 px), mittig über dem Fenster.
  Eingabetaste heißt ja, Escape heißt nein.

  Betrifft: Textquelle wechseln · Fehlerbericht absenden · Bestand zurücksetzen.

  Die Vorgabe dahinter: Die Abfrage soll das Design des Programms tragen — und
  eher breit als hoch sein.


- **„Texte im Spiel" steht jetzt in der Reihenfolge, in der man es liest.**
  Zuerst die Textquelle — woher die Grundlage kommt —, dann was hineingeschrieben
  wird: erst die Bauplan-Angaben, dann die Angaben am Gegenstand. Vorher stand
  der Schreib-Schalter über der Quelle, auf die er sich bezieht.

### Behoben

- **Abfragen hatten deutschen Text, aber englische Knöpfe.** Beim Umstellen
  der Textquelle stand „Einsetzen?" über den Knöpfen **Yes** und **No**.

  Diese Knöpfe kommen nicht aus der Sprachdatei des Programms, sondern aus
  Tks eigener Tabelle — und die ist auf vielen Linux-Systemen unvollständig.
  Nachgemessen am 28.08.2026: Die Tk-Sprache stand bereits richtig auf
  `de_de`, die deutschen Wörter fehlten der Installation trotzdem. Unter
  Windows bringt Tk sie mit, deshalb ist es dort nie aufgefallen.

  Das Programm trägt die Wörter jetzt selbst ein — und zieht sie beim
  Sprachwechsel mit, statt sie beim Start einmal zu setzen.

## v3.0.0-rc86 - 2026-08-28

### Behoben

- **Auf „Texte im Spiel" standen Sternchen im Klartext.** In der Erklärung
  zur Textquelle war »danach ist das `**ganze Spiel**` in dieser Sprache« zu
  lesen — mit den Sternchen.

  Die Auszeichnung `**fett**` in der Sprachdatei ist für den gedacht, der die
  Datei liest; ein Tk-Label kann kein Mischformat und zeigt sie deshalb
  einfach mit an. Die Danke-Seite nahm sie schon heraus, die
  Einstellungszeilen nicht — dieselbe Aufgabe an zwei Stellen, eine davon
  vergessen. Beide gehen jetzt durch dieselbe Funktion.

  Aufgefallen auf einem Bildschirmfoto von rc85. Der Selbsttest hatte es nicht
  gesehen: Er suchte nach deutschem Text in der
  englischen Oberfläche, nicht nach Auszeichnung. **Er prüft es jetzt mit** —
  und die Prüfung wurde gegengeprobt, indem der Fehler noch einmal eingebaut
  wurde.

## v3.0.0-rc85 - 2026-08-28

### Behoben

- **Unter Linux wurden Beschreibungstexte abgeschnitten statt umgebrochen — und
  drückten die Schalter aus dem Fenster.** Betroffen war jede Seite mit
  Fließtext neben einem Bedienelement: „Texte im Spiel“, „Bestand“, „Fehler
  melden“. Bei kleiner Fenstergröße endeten die Sätze mitten im Wort, und die
  Schalter rechts waren gar nicht erreichbar.

  Der Grund lag eine Ebene tiefer, als es aussieht. Die Funktion, die den
  Zeilenumbruch an die Fensterbreite hängt, fragt beim Label nach seinem
  eigenen Rand. Tk gibt so eine Maßangabe je nach Aufbau als Zahl, als Text
  **oder als Tcl-Objekt** zurück — und auf Letzteres wirft `int()` einen
  `TypeError`. Aufgefangen wurden aber nur `TclError` und `ValueError`, und ein
  `TypeError` ist keins von beiden. Der Fehler flog also durch und beendete die
  Funktion, **bevor** sie den Umbruch setzen konnte. Der Text blieb einzeilig
  und breit — genau der Zustand, den diese Funktion verhindern soll.

  Warum es erst jetzt auffiel: Das Tk im Windows-Bau liefert diese Angaben als
  Zahl, das Tk im Linux-AppImage als Tcl-Objekt. Unter Windows konnte der
  Fehler nicht auftreten.

  Aufgefallen in der ersten Linux-Testrunde nach dem Update auf rc84 — zuerst am
  abgeschnittenen Text, dann bestätigt im Fehlerbericht: **50 von 50** aufgehobenen Fehlern kamen aus dieser
  einen Zeile.

  Maßangaben werden jetzt mit Tks eigenem Umwandler gelesen, der alle drei
  Formen versteht. Dieselbe Falle steckte an zwei weiteren Stellen im
  Zeilenumbruch und wurde dort gleich mit beseitigt.

- **Deinstallieren ließ den Autostart-Eintrag liegen.** Danach stand in der
  Registry weiter ein Verweis auf eine Datei, die es nicht mehr gab — Windows
  versuchte sie bei jeder Anmeldung zu starten und scheiterte still.

  Der Grund: Der Eintrag wird an **zwei** Stellen gesetzt. Der Installer legt ihn
  an, wenn man beim Installieren „Mit Windows starten“ wählt, und räumt genau
  diesen Fall auch wieder weg. Schaltet man den Autostart aber **im Programm**
  ein, schreibt das Programm denselben Wert — und davon wusste der Deinstaller
  nichts.

  Aufgefallen beim Aufräumen nach einem Testlauf. Es ist derselbe Autostart, der am selben Morgen das Update scheitern ließ
  (Code 5) — er war an beiden Enden nur halb geregelt.

  Der Deinstaller entfernt den Wert jetzt immer, unabhängig davon, wer ihn
  gesetzt hat. Nur diesen einen Wert — die Autostart-Einträge anderer Programme
  bleiben unangetastet.

## v3.0.0-rc84 - 2026-08-28

### Behoben

- **Das Update scheiterte, wenn der Autostart mitten hineinfuhr.**
  Gemessen beim Update rc75 → rc83: Der Installer lief bis zur Hälfte und brach dann ab mit

      Fehler beim Ersetzen einer vorhandenen Datei:
      DeleteFile schlug fehl; Code 5. Zugriff verweigert.

  Der Windows-Restart-Manager war **nicht** schuld — er hatte sauber gearbeitet.
  Das Setup-Protokoll zeigt die ganze Kette:

      05:43:47  Shutting down applications using our files. (forced)
      05:43:55  << der Watcher läuft wieder — Elternprozess explorer.exe >>
      05:44:17  DeleteFile: The existing file appears to be in use (5).

  Acht Sekunden nach dem Schließen hat der **Autostart** das Programm wieder
  hochgefahren. Windows arbeitet die Autostart-Einträge verzögert nach dem Start
  von `explorer.exe` ab; war die Bedienoberfläche kurz vorher neu gestartet
  (Absturz, frische Anmeldung), fällt diese Verzögerung genau in die laufende
  Installation. Bewiesen ist es über den **Elternprozess**: `explorer.exe` —
  hätte sich der Watcher selbst neu gestartet, stünde dort etwas anderes.

  Das Löschen des laufenden Programms ist damit chancenlos: Der Installer
  schließt **einmal**, und was danach hochkommt, sieht er nicht mehr. Von sich
  aus wiederholt er nur viermal im Sekundenabstand.

  Der Installer fasst jetzt direkt vor dem Kopieren nach und beendet ein wieder
  hochgefahrenes Programm — dreimal mit kurzem Abstand, damit auch ein Autostart
  erwischt wird, der genau in diesem Moment feuert. Nur beim **Update**; wer neu
  installiert, wartet keine Sekunde länger.

### Geändert

- **Ein Schalter, der „aus“ sagt, macht jetzt auch aus.** Beide Schalter auf
  der Seite „Texte im Spiel“ setzten bisher nur die Einstellung — die Textdatei
  blieb unangetastet, bis jemand unten unter „Von Hand“ auf „Jetzt eintragen“
  drückte. Wer die Angaben abschaltete, das Spiel neu startete und alles
  unverändert vorfand, hielt das Werkzeug für kaputt.

  Verschlimmert wurde es durch den Kasten darüber: Der versprach „Änderungen
  wirken beim nächsten Spielstart“ — also genau das, was nicht stimmte.

  Gemessen im Test: Schalter aus, Statuszeile meldete „aus“ — und in der
  Textdatei standen unverändert **1.217** Angaben. Beim zweiten Schalter
  passierte dasselbe, obwohl der Hinweis danebenstand: Gelesen wird das Fette,
  nicht das Kleingedruckte. Damit war die Frage entschieden — ein Hinweis im
  Kleingedruckten ist keine Lösung.

  Jetzt wirkt das Umlegen sofort — aus heißt weg, an heißt da. Das ist
  verlustfrei: Der ursprüngliche Wortlaut des Spiels ist gemerkt und wird beim
  Entfernen buchstabengenau wiederhergestellt. Bleibt doch etwas stehen, sagt
  der Kasten das jetzt auch, statt „es wird nichts geschrieben“ zu melden.


- **„Star Citizen starten" steht nicht mehr doppelt.** Auf der Seite „Texte im
  Spiel" gab es einen eigenen Abschnitt dafür — obwohl der Knopf ohnehin
  dauerhaft unten links in der Leiste steht, auf jeder Seite erreichbar.
  Der Abschnitt ist weg, der Knopf in der Leiste bleibt unverändert.

## v3.0.0-rc83 - 2026-08-28

### Behoben

- **Der Bericht sagt jetzt, ob die Bauplan-Angaben im Spiel stehen.**
  Der häufigste Support-Fall lautet „ich sehe deine Angaben im Spiel nicht
  mehr". Dahinter steckt fast immer dasselbe: Ein Übersetzungs-Update oder ein
  Spiel-Patch hat die Textdatei des Spiels neu geschrieben und die Angaben
  dabei stillschweigend hinausgeworfen. Das Werkzeug merkt davon nichts.

  Im Bericht stand bisher nur, welche Textquelle eingestellt ist — ob
  tatsächlich etwas eingetragen war, ließ sich daraus nicht ablesen, sondern
  nur erraten. Genau so am 28.08.2026 bei **Morkhan** geschehen.

  Neu sind zwei Zeilen: ob die Angaben eingetragen sind, ob das Einspielen
  überhaupt eingeschaltet ist, ob automatisch aufgefrischt wird — und welche
  Textdatei gemeint ist. Wer unter Linux ohne Übersetzung spielt, bekommt
  dabei **keine** Warnung: Dort gibt es keine solche Datei, und das ist der
  Normalzustand, kein Fehler.

- **Abgeschnittener Text statt Umbruch — überall dort, wo es knapp wurde.**
  Aufgefallen ist es an einer einzigen Stelle: Die englische Warnzeile auf der
  Spiel-Seite („Every translation update and every game patch wipes the
  details.") ragte um 5 Pixel heraus und wurde stillschweigend abgeschnitten.

  Die Ursache lag nicht am Text, sondern an einer Rechnung, der ein Posten
  fehlte. Die Umbruchgrenze begrenzt nur den **Text**; was eine Beschriftung am
  Ende belegt, ist Text plus Rand plus Innenabstand. Stand die Grenze auf der
  vollen verfügbaren Breite, brauchte die Beschriftung ein paar Pixel mehr, als
  sie bekam — und Tk schneidet ein zu breites Element stumm am Rahmen ab, ohne
  Fehler, ohne Hinweis.

  Der Rand wird jetzt beim Element selbst erfragt statt geschätzt und
  abgezogen. Das wirkt an **jeder** Stelle mit selbsttätigem Umbruch, auch an
  denen, die heute knapp durchgingen und beim nächsten längeren Text gekippt
  wären. Nachgemessen: nichts wird mehr abgeschnitten, über 11 Seiten × 2
  Sprachen × 2 Fenstergrößen.

## v3.0.0-rc82 - 2026-08-28

### Behoben

- **Ein Auftrag mit mehreren Preisstufen verlor fast alle seine Baupläne.**
  Verträge, die sich einen Textschlüssel teilen, haben sich beim Aufbauen des
  Katalogs gegenseitig überschrieben — der zuletzt eingelesene gewann, alle
  anderen fielen weg. Gemessen am Spielstand 4.10.0: **123 von 353**
  Auftrags-Schlüsseln sind mehrfach belegt, **319** Verträge fielen weg, und
  **797 Bauplan-Einträge** hat dadurch nie jemand zu Gesicht bekommen. Beim
  Kopfgeld-Auftrag standen 8 Baupläne statt 25.

  Gefunden von **Morkhan**, der nicht lockergelassen hat: „ich bekomme nicht
  angezeigt, welche Baupläne ich beim Neulingsauftrag bekommen kann, sondern
  NUR die auf der höchsten Stufe." Es war nicht die höchste Stufe — es war die
  zuletzt gelesene. Jetzt werden alle Stufen zusammengeführt.

- **Ein Katalog, der schon auf der Platte lag, hätte den Umbau nie
  mitbekommen.** Er wurde bisher nur erneuert, wenn Star Citizen eine neue
  Version bringt. Er trägt jetzt eine eigene Aufbau-Nummer — ändert sich sein
  Inneres, wird er neu gebaut, auch ohne Patch.

### Geändert

- **Die Überschrift heißt jetzt „MÖGLICHE BAUPLÄNE FÜR DIESEN MISSIONSTYP".**
  Vorher stand dort „BAUPLÄNE AUS DIESEM AUFTRAG" — und das versprach mehr, als
  die Daten hergeben. Wer das wörtlich liest, nimmt den Auftrag an und bekommt
  nichts. Morkhan am 28.08.2026: „is trotzdem verwirrend, egal wie man's dreht."
  Er hatte recht, und die Verwirrung saß in der Überschrift, nicht in der Liste.

  Der SC Deutsch Launcher formuliert es aus demselben Grund so — 367 mal in
  seiner Datendatei.


- **Die Zählung `[BP 3/12]` im Titel ist weg, es steht nur noch `[BP]`.** Die
  Zahl sah nützlich aus, war aber nicht wahr: Die Liste eines Auftrags führt
  alle Preisstufen zusammen, und welche davon die eigene Stufe hergibt, lässt
  sich nicht auflösen — 123 von 353 Aufträgen teilen sich den Textschlüssel
  über ihre Stufen hinweg. „3 von 12" hieß in Wahrheit „3 von 12, die
  irgendjemand irgendwo bekommen kann". Dieselbe Zahl ist auch aus der
  Listen-Überschrift verschwunden.

  Was bleibt, ist das Ehrliche: **Angehakt heißt „hab ich"** — unabhängig
  davon, ob diese Stufe den Bauplan hergibt oder woher er kam.

- **Wo sich die Stufen unterscheiden, steht der nötige Rang hinter dem
  Bauplan.** Zum Beispiel „erst ab Head Contractor (38.000 XP)" neben Plänen,
  die es erst weit oben gibt, während andere desselben Auftrags schon ab 800
  XP fallen. Steht nur dort, wo es die Baupläne wirklich unterscheidet —
  brauchen alle denselben Rang, steht er ohnehin oben unter „Min. Reputation".

- **Aufträge, bei denen einzelne Stufen leer ausgehen, sagen das jetzt.**
  „Achtung: 1 der 3 Stufen dieses Auftrags geben gar keine Baupläne."


### Geändert

- **Der Reiter „Diagnose" heißt jetzt „Fehler melden" und trägt Rot.** Niemand
  sucht unter „Diagnose", wenn etwas klemmt — und schon gar nicht in einem
  zugeklappten Menü, wo er vorher steckte.

  Das Rot arbeitet in zwei Stufen, damit es etwas bedeutet: **Das Wort ist
  immer rot**, damit man den Reiter findet. **Das Symbol wird nur rot, wenn
  wirklich Fehler mitgeschrieben wurden** — sonst stünde der Watcher dauerhaft
  auf Alarm, obwohl alles läuft, und niemand nähme die Farbe noch ernst.

### Behoben

- **Beim zweiten Besuch einer Seite fehlte die Spur im Bericht.** Sie wurde nur
  beim ersten Aufbauen geschrieben; ging beim erneuten Einblenden etwas schief,
  fehlte die Zeile ganz statt zur Hälfte — und der Bericht verspricht, dass die
  letzte Zeile ohne „steht" die ist, an der es hing. Jetzt steht dort „zeigen",
  und man sieht den Unterschied zwischen „beim Aufbauen gestorben" und „beim
  Einblenden gestorben".
- **Im Fehlerbericht ließ sich erst rollen, wenn die Seite ganz unten war.**
  Das Mausrad ging an die Seite dahinter statt an das Textfeld unter dem
  Zeiger — man musste also erst die ganze Diagnose-Seite nach unten schieben,
  bevor sich im Bericht etwas bewegte. Jetzt rollt, was unter dem Zeiger liegt,
  wie man es aus dem Browser kennt. Gemeldet von **Morkhan**.
- **Der Knopf zum Absenden ist dauerhaft rot**, nicht erst beim Überfahren —
  ein Warnknopf, den man erst sieht, wenn die Maus darauf steht, warnt
  niemanden.
- **Der zweite Meldeweg heißt jetzt „GitHub Issue"** statt „Fehler melden".
  Zwei Knöpfe, die dasselbe versprachen, während der eine den Browser öffnet
  und ein GitHub-Konto verlangt.

## v3.0.0-rc81 - 2026-08-28

> **Ein Knopf statt neun Schritten: Fehlerbericht absenden.**

### Hinzugefügt

- **Die Diagnose-Seite steht jetzt in der Hauptleiste**, direkt unter
  „Serverstatus“ — nicht mehr im zugeklappten Menü „Für Fortgeschrittene“.
  Wer sie braucht, hat ein Problem und sucht sie nicht dort, wo „nichts für
  mich“ draufsteht.
- **Ein roter Knopf „Fehlerbericht absenden".** Klemmt etwas, drückst du ihn —
  und der Bericht ist beim Entwickler. Kein Kopieren, kein Suchen nach dem
  richtigen Kanal, kein „die Nachricht ist zu lang".

  Vorher waren es neun Schritte: aufklappen, kopieren, Discord finden,
  einfügen, feststellen dass es zu lang ist, als Datei speichern, die Datei
  wiederfinden, hochladen, abschicken. Jetzt einer.

  **Du siehst vorher genau, was rausgeht** — derselbe Text, der auf der Seite
  steht, in einem Fenster zum Nachlesen, und erst dann wird gefragt. Namen,
  Pfade und Zugangsdaten sind ohnehin schon herausgenommen. Ohne dein Ja
  passiert nichts.

## v3.0.0-rc80 - 2026-08-28

> **Baupläne aus dem Launcher werden wieder abgehakt — vorhandene Bestände ziehen selbst um.**

### Behoben

- **Baupläne aus dem Launcher oder einer Sicherung wurden nicht abgehakt.** Wer
  seinen Stand aus dem SC Deutsch Launcher, dem KRT Profit Basetool, von
  scmdb.net oder aus einer eigenen Sicherung mitbrachte, sah in der Liste leere
  Kästchen — obwohl die Baupläne im Bestand standen.

  Der Grund: Namen dieser Quellen tragen oft den Klassen-Zusatz
  (`XL-1 (Mil/2/A)`), abgeschnitten wurde er aber nur beim Lesen der
  Spielprotokolle. Damit standen `xl-1 (mil/2/a)` und `xl-1` als zwei
  verschiedene Einträge da und fanden nie zueinander. Das passiert jetzt an der
  zentralen Stelle — gleich, woher ein Name kommt.

  Betroffen war ausgerechnet, wer schon länger spielt und seinen Stand
  mitbringt. Gefunden beim Nachgehen einer Meldung von **Morkhan**.

  **Vorhandene Bestände ziehen beim ersten Start selbst um.** Die Schlüssel
  werden einmalig neu gebildet, doppelte Einträge zusammengeführt — dabei
  gewinnt der ältere Fund, denn wann ein Bauplan zum ersten Mal auftauchte, ist
  die Angabe, die zählt. Nichts geht verloren, nichts muss von Hand gemacht
  werden.

- **Das Werkzeug sagte nicht, dass die Änderungen erst beim nächsten
  Spielstart wirken.** Star Citizen liest die Textdatei **einmal beim
  Hochfahren**. Wer das Spiel offen hatte, spielte die Angaben ein, las
  „eingetragen (1608 Stellen)" — und sah im Spiel nichts. Naheliegender
  Schluss: kaputt. Der Hinweis steht jetzt direkt in der Erfolgsmeldung und im
  Zustandskasten unter *Texte im Spiel*.

## v3.0.0-rc79 - 2026-08-28

> **Drei Funde aus Morkhans Fragen — einer davon hätte still Baupläne verschluckt.**

### Behoben

- **Baupläne, deren Name ein Kürzel trägt, wurden nicht mehr abgehakt.** Seit
  die Angaben am Gegenstand eingetragen werden, schreibt das Spiel den Namen
  **mitsamt Kürzel** in seine Logdatei — `Bauplan erhalten: Spectre (Sth/1/A)`.
  Abgeschnitten wurden bisher nur die fünf Fraktions-Kürzel; alles Neue blieb
  am Namen kleben, und der Bauplan landete unter falschem Namen im Bestand.
  Betroffen wären **344 Waffen und 62 Raketen** gewesen — und niemand hätte es
  bemerkt, weil ja etwas angezeigt wurde. Gefunden beim Nachgehen einer Frage
  von **Morkhan**.

- **Eine Mission versprach „12 Baupläne" im Titel und zeigte darunter
  keine.** Eine Mission hat im Spiel **mehr Beschreibungen**, als der Katalog
  kennt — verschiedene Zielorte und Waren derselben Mission. Gemessen:
  `Covalex_HaulCargo_SingleToMulti` führt drei Beschreibungen im Katalog, in
  der Textdatei des Spiels stehen **acht**. Wer eine der übrigen fünf erwischte,
  sah den Zähler und darunter nichts. Der Weg über die Vertragsdaten des
  SCDL-Teams löste das längst, der eigene Weg über den Bauplan-Katalog nicht.
  Gemeldet von **Morkhan**.

### Hinzugefügt

- **Ein Rufzeichen im Auftragstitel, wenn die Baupläne an Bedingungen hängen.**
  `[BP 0/19!]` statt `[BP 0/19]`. Bei **332 von 818 Aufträgen** (41 %) fallen
  Baupläne nur in bestimmten Preisstufen oder ab einem Rang — „nur für
  256.500 / 264.000 aUEC", „nur ab Meister-Rang". Das stand zwar im
  Beschreibungstext, aber in der Auftragsliste sah man nur den Zähler, und
  genau danach entscheidet man, ob man annimmt. Gemeldet von **Morkhan**, der
  eine Transportmission mehrfach flog, in der nie einer fallen konnte.

  ⚠️ Warum es nicht sauberer geht: Alle Preisstufen einer Mission teilen sich
  **einen** Beschreibungstext im Spiel. Für die kleine Variante zeigt Star
  Citizen denselben Text wie für die große — unterscheiden lässt sich das nicht.

## v3.0.0-rc78 - 2026-08-28

> **Klicks ins Spiel durchreichen ist keine Einbahnstraße mehr.**

### Hinzugefügt

- **Ein Schloss am Overlay holt dich zurück, wenn Klicks ins Spiel
  durchgereicht werden.** Bisher war das eine Einbahnstraße: Wer die
  Einstellung einschaltete, kam an das Overlay nicht mehr heran — kein Knopf,
  keine Leiste, und die Einstellungen selbst schon gar nicht. Der einzige
  Rückweg war, das Programm ein zweites Mal zu starten. Dafür muss man aus dem
  Spiel heraus — also genau das tun, was die Einstellung vermeiden soll.

  Jetzt liegt oben rechts am Overlay ein kleines Schloss, das als Einziges
  klickbar bleibt. Ein Klick, und das Overlay fängt wieder Klicks ab. Es
  erscheint nur, wenn wirklich durchgereicht wird, und verschwindet von selbst
  — auch wenn du drüben in den Einstellungen umschaltest.

## v3.0.0-rc77 - 2026-08-27

> **„Originaltexte aus dem Spiel" funktioniert jetzt ohne Zusatzprogramm.**

### Behoben

- **Wer die Textquelle „Original" wählte, lief oft gegen eine Wand.** Diese
  Quelle holt die englische `global.ini` aus deiner eigenen `Data.p4k` — ohne
  Download, ohne fremde Übersetzung. CIG komprimiert diese Datei allerdings mit
  **zstd**, und das gebündelte Python konnte das nicht. Übrig blieb die
  Meldung, man möge sich 7-Zip installieren — für ein Werkzeug, das man
  herunterlädt und startet, eine Zumutung.

  Das Programm bringt den Entpacker jetzt selbst mit. Betroffen war vor allem,
  wer **englisch spielt und nur die Angaben am Gegenstand** möchte, ohne
  Übersetzung: Für den war dieser Weg der einzige.

  Falls du bisher 7-Zip nur deswegen installiert hast — du brauchst es nicht
  mehr.

## v3.0.0-rc76 - 2026-08-27

> **Am Traktorstrahl steht jetzt, womit man es zu tun hat — und unter Windows
> gibt es nur noch einen Weg.**

> [!important]
> **Windows: Es gibt nur noch den Installer.** Die einzelne
> `SC-BP-Watcher.exe` hängt ab dieser Fassung nicht mehr am Release.
>
> Der Grund betrifft dich, nicht uns: Ein Update legte die neue Fassung
> **neben** die alte Datei, statt sie zu ersetzen. Wer danach seine gewohnte
> Verknüpfung anklickte, benutzte monatelang unbemerkt die alte Version. Mit
> dem Installer kann das nicht passieren.
>
> **Wenn du bisher die einzelne Datei benutzt hast:** Lade einmal
> `SC-BP-Watcher-Setup.exe`, installiere darüber — dein Bauplan-Bestand bleibt,
> er liegt ohnehin woanders. Die alte Datei kannst du danach löschen.
> Unter Linux ändert sich nichts.

### Behoben

- **Unter Windows gibt es nur noch einen Download: den Installer.** Die
  einzelne `SC-BP-Watcher.exe` entfällt.

  **Was du davon hast:** Du musst nicht mehr überlegen, welche der beiden
  Dateien die richtige ist. Der Watcher steht danach im Startmenü, statt
  irgendwo im Download-Ordner zu liegen. Updates ersetzen wirklich das
  Programm, statt eine zweite Fassung danebenzulegen — der häufigste Grund
  dafür, dass jemand monatelang unbemerkt eine alte Version benutzt. Autostart
  ist ein Häkchen bei der Installation, und über *Apps & Features* wird alles
  wieder sauber los.

  Die einzelne Datei stammte aus der Anfangszeit: Ein unsigniertes Programm
  ohne Installer wirkt harmloser, und es ging darum, überhaupt erst Vertrauen
  zu gewinnen. Das ist erreicht — und zwei Wege nebeneinander heißen doppelt so
  viele Stellen, an denen etwas klemmen kann. Lieber ein Weg, der zuverlässig
  funktioniert.

  Unter Linux ändert sich nichts: dort bleibt es beim AppImage.
- **Wer noch v2.0.0 hat, kommt trotzdem mit.** Deren Update-Weg greift die
  erste Datei auf `.exe` — das ist jetzt der Installer — und startet sie
  anschließend. Er läuft damit von selbst und richtet alles ordentlich ein.
  Der eigene Bauplan-Bestand zieht beim ersten Start automatisch mit um.
- **Ein Update installiert dorthin, wo das Programm liegt** — statt eine zweite
  Fassung daneben anzulegen. v2.0.0 gab es nur als nackte `.exe`, alle ihre
  Nutzer laufen also „portabel", ohne es gewollt zu haben. Ohne diesen Zusatz
  hätte der Installer beim übernächsten Update unter
  `%LOCALAPPDATA%\Programs` installiert und die alte Datei liegen lassen — wer
  sie per Verknüpfung startet, benutzte für immer die alte Fassung.

### Hinzugefügt

- **Angaben am Gegenstand — Klasse, Größe und Gütegrad stehen jetzt am Namen.**
  Wer im Spiel etwas mit dem Traktorstrahl anvisiert, sah bisher nur
  „Glacier". Jetzt steht dort **„Glacier (Mil/1/A)"** — militärisch, Größe 1,
  Gütegrad A. Bei Raketen zählt etwas anderes, deshalb steht dort der Suchkopf:
  **„'Arrow' I Missile (IR1)"** für Infrarot, `EM` für elektromagnetisch, `CS`
  für Querschnitt. Im Gefecht klappt niemand eine Beschreibung auf.

  **856 Gegenstände** bekommen so eine Angabe: 450 mit Klasse, Größe und Güte,
  344 Waffen mit ihrer Klasse (ballistisch, Laser, Plasma …) und 62 Raketen.

  Die Angaben stammen aus der Textdatei des Spiels **selbst** — sie stehen dort
  längst, nur in der Beschreibung, die man erst aufklappen muss. Das Werkzeug
  schreibt sie dorthin um, wo man sie im Gefecht auch sieht.

  Vorgeschlagen von **Morkhan**.

  Abschaltbar unter *Texte im Spiel → Angaben am Gegenstand*. Wer sie wieder
  loswerden will, nimmt „Wieder entfernen" — die ursprünglichen Namen kommen
  auf das Zeichen genau zurück.

## v3.0.0-rc75 - 2026-08-27

> **Der Startverlauf steht wieder im Bericht.**

### Behoben

- **Der Startverlauf wurde von der Bedienung aus dem Bericht gedrängt.** rc74
  schrieb Startschritte und Seitenwechsel in einen Topf, und der Bericht zeigt
  nur die letzten zwölf Zeilen — fünf Klicks genügten, und der komplette Start
  war nicht mehr zu sehen. Ausgerechnet der Teil, für den die Spur gebaut wurde.
  Beides steht jetzt in **zwei getrennten Abschnitten**, jeder für sich
  gedeckelt; auch beim Kürzen der Datei bleibt der Startverlauf stehen.
  Gefunden im ersten rc74-Bericht, eine Viertelstunde nach der Veröffentlichung.
- **Die Diagnose-Seite stand als letzte Zeile in ihrem eigenen Bericht.** Der
  Bericht entsteht, während die Seite gebaut wird — dadurch endete jede Spur mit
  „Seite diagnose: bauen beginnt" und sah aus, als wäre genau dort Schluss
  gewesen. Diese Zeilen bleiben jetzt draußen.

## v3.0.0-rc74 - 2026-08-27

> **Ein Absturz hinterlässt jetzt eine Spur.**

### Hinzugefügt

- **Harte Abbrüche werden festgehalten.** Bisher fing das Programm nur
  Python-Fehler ab. Ein Absturz, der den Prozess mitten im Befehl beendet
  (etwa aus der Tk-Bibliothek heraus), hinterließ **nichts**: keinen Eintrag,
  keine Meldung, nichts zum Mitschicken. Ab jetzt schreibt ein Fänger den
  Aufrufweg aller Fäden in eine Datei, und der nächste Diagnose-Bericht zeigt
  ihn unter „Harter Abbruch beim vorigen Lauf".
- **Die Spur führt jetzt auch über die Bedienung.** Sie hörte nach dem letzten
  Startschritt auf — welche Seite jemand geöffnet hat, stand nirgends. Jetzt
  schreibt jeder Seitenwechsel zwei Zeilen mit. Fehlt die zweite, hat es beim
  Bauen genau dieser Seite geknallt. Damit die Datei nicht wächst, wird sie
  gedeckelt.

### Hinweise

- **Der von Bomb20 gemeldete Absturz beim Öffnen von „Was ist neu" ist damit
  nicht behoben, sondern messbar.** Er ließ sich hier nicht nachstellen, und
  sein Bericht konnte ihn gar nicht zeigen — genau diese Lücke schließt rc74.
  Tritt er erneut auf, steht er im nächsten Bericht.

### Dank

- **Bomb20** (pr0) — für die Meldung, die sich am Ende als etwas
  Größeres entpuppte als ein einzelner Absturz: Das Werkzeug war an dieser
  Stelle blind. Und dafür, dass er sie geschickt hat, obwohl sie nach einem
  Fehlalarm aussah.
- **Haldjas** (pr0) — für den Gegentest unter Windows: Update
  von rc71 auf rc73 und die Oberfläche seit rc61, beides ohne Befund.

## v3.0.0-rc73 - 2026-08-27

> **Die Danke-Seite sagt jetzt, was heute wirklich passiert ist.**

### Geändert

- **Die Seite „Danke & Lizenzen" im Programm nennt Bomb20s heutige Funde.** Sie
  stand noch auf seinem Beitrag vom 25.08., während er an diesem Vormittag drei
  Fehler freigelegt hat, die am Ausliefertag **jeden** Nutzer getroffen hätten:
  der Startknopf für Star Citizen, der abgebrochene Download und der Neustart,
  der nie kam.
  - Der Dank stand ordentlich in beiden CHANGELOGs — nur sieht die im Programm
    niemand. **Wer im Programm nicht auftaucht, dem wurde nicht gedankt.** Die
    Release-Checkliste führt diese dritte Stelle jetzt ausdrücklich auf.

### Bestätigt

- **Der Neustart nach dem Update funktioniert** — nachgewiesen auf einem zweiten
  Rechner (CachyOS), von rc71 auf rc72, ohne einen einzigen Eintrag im
  Fehlerprotokoll. Damit hängt es an keiner Eigenheit einer einzelnen
  Installation.

### Dank

- **Bomb20** (pr0) — für einen Vormittag, an dem er dreimal einen Bericht
  geschickt hat, obwohl er eigentlich arbeiten musste, und für die Geduld, als
  seine Meldungen zunächst nach Bedienfehler aussahen. Sie waren es nie.


## v3.0.0-rc72 - 2026-08-27

> **Die Update-Seite sagt jetzt die Wahrheit** — sie sieht von allein nach, und
> der Weg zur stabilen Version ist keine Sackgasse mehr.

### Behoben

- **Die Seite zeigte eine veraltete Versionsnummer, solange sie offen blieb.**
  Nachgefragt wurde **einmal je Seitenaufbau**. Wer die Seite offen hatte,
  während draußen eine neue Version erschien, sah weiter die alte Nummer auf dem
  Knopf — und hielt sich für aktuell. Gemeldet von **Bomb20** (pr0): „ich
  krieg noch 67 angezeigt", während rc68 seit Minuten veröffentlicht war.
  Nachgesehen wird jetzt alle fünf Minuten, solange die Seite offen ist.
  - Fünf Minuten sind der Kompromiss: oft genug, dass niemand eine Version
    verpasst, und selten genug für GitHubs Grenze von 60 Abfragen pro Stunde.
- **Der Kasten „Stabile Version" war eine Sackgasse.** Statt eines Knopfes stand
  dort „Erst oben auf ‚Jetzt nachsehen' drücken" — wer die stabile Version
  wollte, sah keinen Weg, sondern eine Hausaufgabe.
  - **Der Grund war eine zu kleine Abfrage:** Geholt wurden die letzten **20**
    Freigaben, und darunter war bei inzwischen 83 Veröffentlichungen **keine
    einzige stabile** mehr — nur Testversionen. Jetzt werden 100 geholt (das
    Höchste, was GitHub in einer Abfrage hergibt), und es bleibt bei **einer**
    Anfrage: Die Stundengrenze zählt Anfragen, nicht Einträge.
  - Gemessen: 20 Freigaben → 0 stabile, 100 Freigaben → 3.

### Dank

- **Bomb20** (pr0) — für „ich krieg noch 67 angezeigt". Das klang nach
  einer Kleinigkeit und war der Hinweis auf zwei Fehler auf einmal.


## v3.0.0-rc71 - 2026-08-27

> **Der Neustart nach dem Update funktioniert** — die Ursache war eine ganz
> andere, als alle dachten.

### Behoben

- **Nach dem Update ging der Watcher aus und kam nicht wieder.** Gemeldet von
  **Bomb20** (pr0) am Morgen, hier den ganzen Vormittag über
  reproduziert. Drei Anläufe (rc67, rc68, rc70) haben es nicht gelöst, weil sie
  von einem Absturz der neuen Version ausgingen.
  - **Es war kein Absturz.** Die neue Version startet, sieht den
    Einzelinstanz-Wächter noch belegt, hält sich für die **zweite** Instanz und
    beendet sich planmäßig — mit Rückgabewert 0. Ein sauber beendeter Prozess
    sieht im Nachhinein genauso aus wie ein abgestürzter, bis jemand den
    Rückgabewert liest.
  - **Warum der Port belegt blieb:** Vor dem Neustart wird der Wächter mit
    `close()` geschlossen. Das weckt aber den Faden nicht, der in `accept()`
    wartet — der bleibt hängen, der Deskriptor bleibt gültig, der Port belegt.
    `shutdown()` bricht das wartende `accept()` ab; erst danach gibt `close()`
    den Port wirklich frei.
  - Belegt statt vermutet: Die Probe scheiterte vorher mit `Address already in
    use` und läuft jetzt durch. Selbsttest-Abschnitt 24 hält das fest.

### Dank

- **Bomb20** (pr0) — für die erste Meldung und dafür, nicht lockergelassen
  zu haben, als es nach einem Bedienfehler aussah. Er lag richtig, wir nicht.


## v3.0.0-rc70 - 2026-08-27

> **Wenn der Neustart scheitert, steht künftig im Bericht, warum.**

### Behoben

- **`'Overlay' object has no attribute '_dx'` beim Ziehen des Overlays.** Tk
  liefert eine Mausbewegung nicht immer nach einem Klick auf dasselbe Fenster:
  Wer den Knopf außerhalb drückt und ins Overlay zieht, löst nur die Bewegung
  aus — und den Startpunkt gab es dann nicht. Das Ziehen tat einmal nichts, der
  Fehler landete lautlos im Protokoll. Gemeldet von **Bomb20** (pr0, am
  25.08.2026 auf rc18) und erneut am 27.08.2026 auf rc69 — dazwischen nie
  behoben, weil er nichts kaputt macht, was man sieht.

### Geändert

- **Ein gescheiterter Neustart hinterlässt jetzt eine Spur.** Die
  Fehlerausgabe der frisch gestarteten Version lief bisher nach `/dev/null` —
  deshalb war „geht aus, kommt nicht wieder" nicht aufzuklären: Im
  Diagnosebericht stand dazu **gar nichts**. Sie wird jetzt aufgefangen, und
  kommt die neue Version nicht hoch, hängt ihr letztes Wort im Fehlerprotokoll
  und damit im Bericht.
  - Das ist keine Reparatur, sondern eine Messung. Nach zwei Anläufen, die den
    Neustart nicht gelöst haben, wird nicht ein drittes Mal
    geraten.

### Dank

- **Bomb20** (pr0) — für den Ziehen-Fehler, der zwei Tage lang in
  Berichten stand, ohne dass ihn jemand ernst genommen hat.


## v3.0.0-rc69 - 2026-08-27

> **Das Update wurde bei manchen gar nicht erst heruntergeladen** — schuld war
> die Fortschrittsanzeige.

### Behoben

- **Klick auf „Version holen", und es passierte nichts.** Kein Fortschritt, kein
  Neustart, keine Meldung — nach einem Neustart lief weiter die alte Version.
  Gemeldet von **Bomb20** (pr0): „ich habe auf get 68 geklickt, aber da
  kam nix mit restart oder install."
  - **Die Ursache war die Anzeige, nicht der Download.** Heruntergeladen wird in
    einem eigenen Faden, der den Fortschritt ans Fenster meldet. Dieser Aufruf
    kann werfen (`RuntimeError: main thread is not in main loop`) — und die
    Ausnahme riss den **ganzen Faden** mit, gleich beim ersten Prozentschritt. In
    Bomb20s Bericht stand der Fehler dreimal, einmal pro Klick.
  - Zeichnen ist Beiwerk, das Herunterladen ist der Zweck. Jede Anzeige im
    Update-Faden läuft jetzt gekapselt: Geht sie schief, wird das vermerkt und
    der Vorgang läuft weiter.
- **„Auf Aktualität prüfen" gab fälschlich Entwarnung.** Bomb20 bekam „du hast
  die neueste rc67" gemeldet, während rc68 seit zwei Minuten veröffentlicht war.
  GitHub erlaubt anonym **60 Abfragen pro Stunde und Adresse**; wer an einem
  Vormittag viel klickt, läuft dagegen. Der Abruf scheiterte — und wurde still
  verschluckt, sodass mit dem alten Stand weitergerechnet wurde.
  - „Nichts Neues" und „konnte nicht nachsehen" sind das Gegenteil voneinander
    und werden jetzt auseinandergehalten. Bei erreichter Stundengrenze steht da,
    was los ist und dass es in einer Stunde wieder geht.
  - **Ein Prüfknopf, der fälschlich Entwarnung gibt, ist schlimmer als keiner.**

### Dank

- **Bomb20** (pr0) — für den dritten Diagnosebericht an einem Vormittag,
  genau im richtigen Moment abgeschickt. Ohne ihn wäre „da kam nix" nicht von
  „Download klemmt" zu unterscheiden gewesen; mit ihm stand die Ursache in einer
  Zeile da.


## v3.0.0-rc68 - 2026-08-27

> **Der Update-Knopf steht da, wo man ihn sucht** — und „Fassung" heißt jetzt
> überall „Version".

### Geändert

- **Der Knopf „Jetzt die neueste Version holen" steht ganz oben**, direkt unter
  der Versionskarte. Vorher kam er erst nach der Knopfreihe und dem
  Tagesschalter und lag bei der Mindestgröße des Fensters **unterhalb der
  Kante** — wer ihn nicht findet, updatet nicht.
  - Das Fenster größer zu machen wäre die falsche Antwort gewesen: Auf einem
    1366×768-Laptop passt es dann gar nicht mehr. Der wichtigste Knopf gehört
    nach oben, nicht das Fenster in die Höhe.
- **Auch die beiden Kanal-Kästen sind bei der Mindestgröße vollständig
  sichtbar** — in ihnen sitzt der Knopf, mit dem man gezielt die stabile Version
  holt. Der Tagesschalter steht dafür jetzt darunter; er ist eine
  Nebeneinstellung, die Kästen sind der Zweck der Seite.
- **„Nur fertige Fassungen" heißt jetzt „Stabile Version".** „Fertig" klingt nach
  abgeschlossen — das Werkzeug wird laufend weiterentwickelt.
- **„Fassung" heißt überall „Version".** Ein sperriges Wort, das sonst niemand
  benutzt; in der Oberfläche, in der Anleitung und in den Kommentaren steht jetzt
  durchgehend „Version". Einzige Ausnahme ist die **Sprachfassung** — damit ist
  die Übersetzung gemeint, nicht die Programmversion.
- **„rcXX ist schon da" heißt jetzt „rcXX ist schon installiert"** — klarer, und
  im Englischen stand es längst so.

### Dank



## v3.0.0-rc67 - 2026-08-27

> **Der Neustart nach dem Update funktioniert unter Linux** — und kann nicht mehr
> stumm scheitern.

### Behoben

- **Nach dem Update ging der Watcher aus und kam nicht wieder.** Er lud die neue
  Version, spielte sie ein, schloss sich — und blieb zu. Gemeldet von **Bomb20**
  (pr0) mit dem entscheidenden Satz „es geht dann aus aber startet nicht",
  am selben Tag auf einem zweiten Rechner reproduziert.
  - **Die Ursache:** Beim Start der neuen Version wurden nur `APPIMAGE`, `APPDIR`,
    `OWD` und `ARGV0` aus der Umgebung entfernt — `LD_LIBRARY_PATH`, `PYTHONHOME`
    und `PYTHONPATH` blieben stehen. Die zeigen im AppImage in den **entpackten
    Mount der alten Version**. Zwei Sekunden später beendet sich die alte, ihr
    Mount verschwindet, und die neue sucht ihre Bibliotheken in einem Verzeichnis,
    das es nicht mehr gibt. Sie stirbt, bevor ein Fenster erscheint.
  - Die passende Wäsche gab es längst (`saubere_umgebung`), nur führte der
    Neustart eine eigene, unvollständige Version davon mit. Beide liegen jetzt in
    `scbp/pfade.py` — **eine** Wäsche, benutzt von allen.
- **Und er kann nicht mehr stumm scheitern.** Die alte Version tritt erst ab,
  wenn die neue die ersten Sekunden überlebt hat. Stirbt sie, bleibt der Watcher
  offen und sagt es: „Die neue Version ist nicht hochgekommen." Vorher schloss
  sich die alte pflichtschuldig, während die neue schon tot war — und der Rechner
  stand ohne Watcher da, ohne ein Wort dazu.
  - Dahinter derselbe Merksatz wie beim Startknopf in rc65: **Ein Programm zu
    starten heißt nicht, dass es läuft.** `Popen` meldet Erfolg, sobald der
    Prozess angelegt ist.

### Dank

- **Bomb20** (pr0) — fürs Dranbleiben. Seine nüchterne Beschreibung „es
  geht dann aus aber startet nicht" hat den Fehler festgenagelt, nachdem er
  zunächst für einen Bedienfehler gehalten wurde. Er lag richtig, wir nicht.

## v3.0.0-rc66 - 2026-08-27

> **Die Ausgabe-Dateien halten sich von allein aktuell** — und die Dateiauswahl
> sieht endlich nach dem System aus, auf dem sie läuft.

### Hinzugefügt

- **Die Ablage wird bei jedem neuen Bauplan mitgeschrieben.** Bisher entstanden
  die drei Ausgabe-Dateien (KRT Profit Basetool, scmdb.net, Vollsicherung) nur
  auf Knopfdruck — wer einmal geklickt hatte, hielt sie für aktuell, dabei
  standen sie für immer auf dem Stand jenes Klicks. Jetzt hängt das Schreiben am
  Bestand selbst: Jeder Fund im Spiel, jede Nachlese beim Start, jede Bestätigung
  durch den Launcher und jeder Import ziehen die Dateien mit.
  - **Feste Dateinamen in der Ablage.** Mit Datum im Namen wären dort täglich
    drei neue Dateien entstanden, und niemand wüsste, welche die aktuelle ist.
    Der Speichern-Dialog schlägt weiterhin einen Namen mit Datum vor — wer von
    Hand speichert, hält bewusst einen Stand fest.
  - **Früher abgelegte Dateien mit Datum wandern nach `Ältere/`** — verschoben,
    nicht gelöscht. Was sonst noch im Ordner liegt, bleibt unangetastet.
- **Ein Speichern-Knopf je Version**, direkt an der Version, statt eines
  gemeinsamen Knopfes weiter unten.

### Behoben

- **„Einzeln speichern …" speicherte immer die Basetool-Version.** Die Version
  war im Code fest verdrahtet; scmdb und die Vollsicherung waren über den Dialog
  überhaupt nicht erreichbar.
- **Die Dateiauswahl unter Linux war der alte Tk-Kasten** — eine Spaltenliste mit
  jedem versteckten Ordner, kein Sortieren, keine Vorschau. Jetzt öffnet sich der
  Dialog des Schreibtischs (`kdialog` unter KDE, sonst `zenity`), überall dort,
  wo eine Datei oder ein Ordner gewählt wird: Bestand einlesen, Bestand
  speichern, Spielordner, Launcher-Ordner, eigener Ordner und der
  Einrichtungs-Assistent. Fehlt beides, bleibt der Tk-Dialog als Rückfall —
  **nichts hängt davon ab.** Unter Windows und macOS ändert sich nichts, dort
  reicht Tk schon den echten Systemdialog durch.
  - Für Ordner gab es diesen Weg längst; für Dateien nicht. Beides steht jetzt
    an einer Stelle (`scbp/dateiwahl.py`) statt an dreien.


### Dank


## v3.0.0-rc65 - 2026-08-27

> **Der Startknopf rief unter Linux das falsche Programm auf.**

### Behoben

- **Der Knopf „Star Citizen starten" startete unter Linux nichts.** Er meldete
  „Star Citizen wird gestartet …" und danach geschah nichts — ohne jede
  Fehlermeldung. Aufgerufen wurde der `lug-helper`, und der **kann das Spiel gar
  nicht starten**: Er verwaltet Wine-Präfix, Runner und DXVK; eine Startoption
  hat er nicht. Der Watcher nimmt jetzt das Startskript `sc-launch.sh`, das der
  Helper beim Einrichten im Präfix anlegt, und findet es über den Spielordner
  (eine Ebene über `drive_c`) — unabhängig davon, wohin jemand installiert hat.
  Gemeldet von **Bomb20** (pr0).
  - Kein Rückfall mehr auf den `lug-helper`: Er würde gefunden, der Knopf
    erschiene, und er täte wieder nichts. Wer über Lutris oder Heroic spielt,
    trägt seinen Startbefehl weiterhin in der Einstellung `spielstarter` ein.


### Dank

- **Bomb20** (pr0) — für die Meldung, dass Star Citizen sich nicht aus dem
  Werkzeug starten lässt, und für die Geduld mit zwei Diagnoseberichten an einem
  Vormittag. Ohne den zweiten wäre nicht herausgekommen, dass der `lug-helper`
  das Spiel überhaupt nicht starten kann.

## v3.0.0-rc64 - 2026-08-27

> **Der Neuaufbau frisst die Meldung** — dreimal dieselbe Falle, an drei
> verschiedenen Stellen.

### Behoben

- **„Auf Aktualität prüfen" meldete weiterhin kein Ergebnis.** Der Fehler aus
  rc63 war weg, die Antwort kam trotzdem nicht: Der Knopf blieb bei „Suche nach
  einer neuen Version …" stehen. `neu_aufbauen()` zerstört **alle** Kinder des
  Fensters — auch die Fußzeile, in der die Meldung steht. Sie wurde gesetzt und
  Millisekunden später mitzerstört. Jetzt wird erst aufgebaut, dann gemeldet.

- **Dieselbe Falle nach dem Update unter Linux.** „Fertig — jetzt neu starten"
  wurde bei `after(0)` gesagt und bei `after(50)` weggeräumt. Reihenfolge
  getauscht.

- **Bei „sehr groß" fehlte die halbe linke Leiste.** „Star Citizen starten",
  „Kaffee spendieren" und „Discord" fielen unten aus dem Fenster — sie werden
  von unten gepackt, und was zwischen Reitern und Fußzeile nicht hineinpasst,
  fällt heraus. Die Mindestgröße des Fensters hängt an der Höhe der
  Seitenleiste, und die hängt an der Schrift. Gerechnet hat das Programm das
  immer richtig, nur lief die Rechnung nie nach einem Schrift- oder
  Sprachwechsel — jetzt gehört sie zum Neuaufbau. Der Gedanke dahinter: Wer
  schlecht sieht und die Schrift größer stellt, braucht auch ein Fenster, das
  im Verhältnis mitwächst.

- **Die beiden Kästen unter „Wovon willst du Bescheid bekommen?" waren
  ungleich groß.** `pack(expand=True)` verteilt nur den **Überschuss**
  gleichmäßig — wer mehr Text hat, bleibt breiter. Sie liegen jetzt in einem
  `grid` mit `uniform`, der einzigen Zusage in Tk, die zwei Spalten wirklich
  gleich breit macht; gemessen 545 px zu 545 px, gleiche Höhe.

- **Bei „sehr groß" waren die Knöpfe abgeschnitten.** Ein benanntes Tk-Font
  wirkt sofort auf jeden Text — aber die gezeichneten Rundknöpfe legen ihre
  Leinwand beim Bauen **einmal** auf die gemessene Textbreite fest. Nachgemessen
  an der Overlay-Wahl: Kasten 177 px, Text 206 px, **29 px fehlten**. Das
  Umstellen der Schriftgröße baut die Oberfläche jetzt neu auf — wie der
  Sprachwechsel es längst tut —, damit jede Leinwand neu misst.

### Hinweise

- **Selbsttest-Abschnitt 21.** Prüft beides zusammen: dass ein fertiger
  Rundknopf tatsächlich nicht von allein wächst (sonst liefe die zweite Prüfung
  ins Leere), und dass der Schriftwechsel neu aufbaut **und danach** meldet.

## v3.0.0-rc63 - 2026-08-27

> **„Auf Aktualität prüfen" prüft wieder** — und der Hinweis vor dem Update
> kommt endlich an.

### Behoben

- **„Auf Aktualität prüfen" antwortete mit `name 'datei' is not defined`.**
  Im Knopf stand nicht das Nachsehen, sondern der **Holen**-Ablauf: herunter-
  laden, einspielen, abtreten — mit zwei Variablen, die es in dieser Funktion
  nie gab. Egal ob eine neue Version da war oder nicht, unten stand „Das hat
  nicht geklappt". Jetzt meldet der Knopf wieder, was er findet: die gefundene
  Version — oder **„Du hast die neueste Version."** Diesen Satz gab es die
  ganze Zeit, ihn zeigte nur niemand.

- **Der Hinweis vor dem Update kam bei keinem einzigen Update.** Seit rc52 soll
  der Watcher ansagen, dass er sich gleich schließt, das Setup läuft und danach
  ein Doppelklick nötig ist — ein Programm, das wortlos verschwindet, sieht aus
  wie ein Absturz. Der Dialog saß aber in **derselben toten Funktion** und ist
  deshalb nie erschienen. Er steht jetzt im echten Update, vor dem Einspielen,
  und das Setup wartet, bis er gelesen ist. Bestätigt beim Update
  auf rc62: Es kam kein Fenster.

- **Der Ablage-Ordner ging nach dem Export nie auf.** `os.startfile()` im
  Bestandsfenster griff auf ein `os`, das dort nie importiert war; der Fehler
  fiel still in ein `except Exception`. Beim Ordner-Umzug stand `t(...)` statt
  `sprache.t(...)` — dort blieb die Erfolgsmeldung weg. Beide gefunden von der
  neuen Prüfung unten, nicht von Hand.

### Hinweise

- **Der Selbsttest sucht jetzt Namen, die es nicht gibt** (Abschnitt 20, über
  `pyflakes`). Genau diese Fehlerklasse fliegt sonst erst beim **Klicken** auf:
  Python prüft Namen erst beim Ausführen, und wenn der Rückruf in einem
  `except` endet, sieht es niemand. Die Prüfung fand auf Anhieb drei Fälle. Sie
  läuft im Bau-Ablauf vor jedem Release mit; fehlt `pyflakes` auf einem
  Entwicklungsrechner, wird sie übersprungen statt zu scheitern.

### Geändert

- **Das ⓘ am rechten Rand der Bauplan-Liste ist größer** — es öffnet den
  Herkunftskasten und war in reiner Zeilengröße kaum als Schaltfläche zu
  erkennen. Neuer Größensatz `ANTIPPBAR`, eine Stufe über den übrigen
  Zeilenzeichen: 16 px statt 14 bei „normal", 22 statt 18 bei „sehr groß". Die
  Statuspunkte im Overlay bleiben unverändert — die will niemand anklicken.

## v3.0.0-rc62 - 2026-08-27

> **Der Patch-Filter zeigt wieder, was der Patch gebracht hat.**

### Behoben

- **Der Patch-Filter fand nichts, „neu im Spiel" blieb leer.** Wer den Watcher
  schon vor rc55 benutzt hat, sitzt auf einem Katalog ohne Herkunftsstempel —
  gestempelt wurde bisher nur beim Neubau, und neu gebaut wird nur bei einer
  neuen Spielversion. Das Auswahlfeld zeigte deshalb „4.10.0 (21)" (es liest die
  Historie direkt), die Liste darunter aber „Nichts gefunden". Die Stempel werden
  jetzt beim Start nachgetragen, ohne Neubau und ohne Netz.
- **Der nächste Patch wäre stumm geblieben.** Die Vergleichsgrundlage
  (`bauplaene-gesehen.json`) kam ebenfalls erst mit rc55. Fehlte sie, griff die
  Regel „erster Katalogbau überhaupt — nichts ist neu", und der nächste Patch
  hätte **keinen einzigen** Zugang gemeldet. Fehlt die Datei, gilt jetzt der
  vorhandene Katalog als Grundlage: Was darin steht, war vorher im Spiel.

### Hinweise

- **Der Selbsttest prüft diesen Fall jetzt selbst** (Abschnitt 19, elf neue
  Prüfungen). Er hat sich sofort gelohnt: Das Nachziehen stand zuerst *hinter*
  der Netzsperre `SC_BP_NO_NET` — wer ohne Netz startet, hätte nie einen Stempel
  bekommen, obwohl Historie und Katalog beide auf der Platte liegen.

## v3.0.0-rc61 - 2026-08-27

> **Die Meldung im Discord sagt jetzt, worum es geht.**

### Hinzugefügt

- **Die Release-Meldung im Discord ist jetzt eine lesbare Karte.** Statt
  `[Repo] New release published: v3.0.0-rc60` steht dort der Changelog-Abschnitt
  **dieser** Version — derselbe Text wie im Werkzeug unter „Was ist neu".
  Testfassungen in Gold mit dem Hinweis „weniger lange erprobt", fertige in
  Xharig-Grün, dazu das Programmsymbol — nach dem Vergleich mit dem
  StarStrings-Kanal. Ohne hinterlegten Schlüssel passiert nichts und der
  Bau bleibt grün — eine Chat-Meldung darf keine fertige Veröffentlichung rot
  färben.

## v3.0.0-rc60 - 2026-08-27

> **Was der Diagnosebericht verriet.** Ein unsichtbares Kreuz, acht Fehler je
> Seitenwechsel — und eine neue Prüfung, die beides künftig vorher findet.

### Behoben

- **Acht Fehler im Protokoll bei jedem Seitenwechsel.** `invalid command name
  …!label` — Rückrufe, die den Zeilenumbruch nachziehen, kamen dran, wenn ihr
  Label längst zerstört war. Sichtbar war davon nichts: Der Haken in `fehler.py`
  fing sie ab, sie füllten nur den Bericht und verdeckten damit, was wirklich
  wichtig gewesen wäre. Dieselbe Falle steckte in der Knopfreihe und im
  Eingabefeld mit gezeichnetem Rahmen; alle drei prüfen jetzt vorher, ob es ihr
  Widget noch gibt. Nachgemessen: 39 Seitenwechsel, **0** Fehler.

- **Das Kreuz zum Schließen des Herkunftskastens war unsichtbar.** In der
  Bauplan-Liste blieb dort eine leere Lücke: Das Symbol `schliessen` gab es nur
  in Knopfgröße, gebraucht wurde es in Zeilengröße. `zeichen.bild()` gibt bei
  einer fehlenden Datei still `None` zurück — mit Absicht, damit ein fehlendes
  Symbol das Programm nicht anhält, wodurch der Fehler aber unsichtbar blieb.
  `tools/oberflaeche_pruefen.py` prüft das ab sofort mit.

## v3.0.0-rc59 - 2026-08-27

> **Die Anleitung stimmt wieder.** Alle Bildschirmfotos neu, je Sprache ein
> eigener Satz, und alle Symbole darin stammen aus dem Satz des Programms.

### Hinzugefügt

- **Die farbigen Punkte standen im Fließtext noch als Emoji.** Die
  Zeichen-Erklärung zeigte längst die echten Bilder, die Beschreibung darunter
  aber weiter `🟢 🟡 🔵 ⭐` — zwei verschiedene Darstellungen desselben Zeichens
  auf einer Seite.

- **Auch die englische Anleitung zeigt jetzt die englische Oberfläche.** Sie
  führte bis hierher deutsche Bildschirmfotos vor — bei elf Bildern und einem
  Werkzeug, dessen Nutzer unter Linux überwiegend den englischen Client fahren,
  keine Kleinigkeit. `tools/sprachen_pruefen.py` achtet ab sofort darauf: Er
  zählte nur Abschnitte und hat Bilder nie angesehen.

- **Alle Bilder in der Anleitung sind neu.** Die alten stammten aus
  v3.0.0-rc11 und zeigten nicht nur die abgelösten Symbole, sondern auch einen
  Stand ohne Serverstatus und ohne Patch-Filter. Dazu zwei Seiten, die noch nie
  eins hatten: **Serverstatus** und **Danke & Lizenzen**.

- **Die Merkmalstabelle in der Anleitung zeigte Emoji statt der echten Symbole.**
  `⚡ 📋 🧭 ⭐ 🔔 …` haben mit dem Symbolsatz des Programms nichts zu tun und sehen
  auf jedem System anders aus. Alle sechzehn stammen jetzt aus demselben Satz wie
  die Oberfläche.

- **Ein Bildschirmfoto zeigte den Heimatpfad des Autors.** `screenshot-pfade.png`
  lag seit v3.0.0-rc11 im Repo und führte dreimal `/home/<benutzer>/` vor —
  genau das, was der Fehlerbericht mit `pfade.kuerzen()` sonst herausnimmt.
  Entfernt; die Ordner-Seite bekommt kein Bild mehr, weil dort zwangsläufig
  Pfade stehen. An ihrer Stelle steht jetzt der Serverstatus, der nie eins
  hatte.

### Behoben

- **Die Filterknöpfe auf „Was ist neu" blieben auf Englisch deutsch.** „Alles /
  Neu / Verbessert / Behoben" standen fest im Code statt in `sprache.py` — direkt
  neben einem sauber übersetzten Änderungstext. Aufgefallen auf einem
  Bildschirmfoto der englischen Oberfläche.

## v3.0.0-rc58 - 2026-08-27

> **Wem was gehört — an einer Stelle.** Neuer Reiter „Danke & Lizenzen", der
> die Lizenzen und die Beteiligten zusammenführt. Dazu Namen und Symbole, die
> endlich zu dem passen, was sie tun.

### Hinzugefügt

- **Der Reiter „Auftragstexte" heißt jetzt „Texte im Spiel".** Der alte Name
  sagte nicht, **wo** diese Texte auftauchen. „Ingame-Texte" stand kurz zur Wahl
  und ist unter Spielern gängig — dagegen sprach, dass jeder andere Reiter der
  Leiste deutsch ist und ein einzelner Anglizismus dazwischen auffällt.
- **Auf „Update & Über" steht das Programmsymbol neben der Version.** Die Seite
  hatte gar kein Bild mehr, seit der Autor-Block auf „Danke & Lizenzen" gewandert
  ist.

- **Die Anleitung zeigte Zeichen, die es im Werkzeug nicht mehr gibt.** Die
  Knopf-Legende in beiden READMEs führte `☰`, `ⓘ`, `⟳`, `⏻` und `🗑` auf — zwei
  davon sind längst entfernt, die anderen sehen anders aus. Sie zeigt jetzt die
  **echten Bilddateien** aus `assets/symbole/`; damit kann sie nicht mehr
  veralten, weil sich mit einem getauschten Symbol das Bild in der Anleitung von
  selbst mitändert. Dasselbe für die Zeichen-Erklärung der Meldungen.
- **„Wer das gebaut hat" stand plötzlich zweimal.** Der Block mit Autor,
  scmdb, SC Deutsch Launcher und StarStrings lag auf „Update & Über" — und die
  neue Seite „Danke & Lizenzen" nannte dieselben Projekte noch einmal. Er liegt
  jetzt nur noch auf „Danke & Lizenzen", und zwar mit dem Autor **ganz oben**:
  Eine Seite, die fremde Arbeit aufzählt, muss die eigene zuerst nennen.

- **Der Spenden-Link war auf GitHub nirgends zu sehen.** Der Knopf „Kaffee
  spendieren" gibt es im Werkzeug seit Langem — auf der Projektseite selbst
  fehlte er aber komplett: kein Sponsor-Knopf, keine Erwähnung in der Anleitung.
  Wer das Werkzeug noch nicht installiert hatte, konnte ihn also gar nicht
  finden. Jetzt gibt es beides.

- **Neuer Reiter „Danke & Lizenzen"** unter *Info*. Bis hierher stand im ganzen
  Programm **keine einzige Lizenzangabe** — weder die eigene (GPL-3.0) noch die
  der mitgelieferten Symbole, und fremde Projekte wurden nur nebenbei genannt,
  dort wo sie gerade gebraucht wurden. Jetzt steht an einer Stelle, wem was
  gehört: das Programm selbst, die Symbole von Lucide, die Daten von scmdb,
  StarStrings und der SC Deutsch Launcher — jeweils mit Lizenz und anklickbarem
  Verweis. Dazu der Dank an die, aus deren Rückmeldung etwas geworden ist.

## v3.0.0-rc57 - 2026-08-27

> **Ein Symbolsatz statt vierzehn Schriftzeichen.** Die Zeichen der Melde-Leiste
> waren unterschiedlich groß, im Stil gemischt und sahen auf jedem Betriebssystem
> anders aus. Ersetzt durch fertige Bilder aus einem einzigen, einheitlich
> gezeichneten Satz.

### Geändert

- **Alle Symbole sind jetzt gleich groß — und stammen aus einem Satz.** Die
  Zeichen in der Melde-Leiste waren unterschiedlich groß, die Glocke war die
  größte. Dahinter steckten drei Ursachen mit demselben Kern: *Die Schrift
  entschied, nicht das Programm.* Ein Schriftzeichen füllt seine Box nur zu
  50–70 % aus, und jedes anders; `🗑` und `▶` sind gefüllte Flächen, `⚙ ⟳ ✕`
  dünne Striche; und jedes Betriebssystem greift zu einer anderen Ersatzschrift.
  Ersetzt durch fertige Bilder aus dem **Lucide**-Satz — alle auf einem
  24×24-Raster mit gleicher Strichstärke gezeichnet.
- **Auf Windows, Linux und Mac sieht die Oberfläche jetzt gleich aus.** Das war
  vorher nicht so: Windows nahm `Segoe UI Symbol`, die anderen Systeme etwas
  anderes. Wer auf einem Mac entwickelt, sah damit andere Zeichen als die
  Nutzer unter Windows.
- **Die farbigen Punkte vor den Bauplänen sind keine Emoji mehr.** `🟢 🟡 🔵 ⭐`
  liegen außerhalb der Grundebene; Windows malte sie über die Farb-Emoji-Schrift
  als bunte Klötzchen, die die eingestellte Farbe **ignorierten** — ausgerechnet
  an der Stelle, die man am häufigsten sieht.
- **Star Citizen starten heißt jetzt Rakete statt Abspielpfeil.** Ein `▶` heißt
  überall „Video ab", nicht „Programm starten".
- **Meldungen wegräumen heißt jetzt Radiergummi statt Mülleimer.** Der Knopf
  löscht nichts — er räumt nur die Anzeige auf, die Baupläne bleiben. Ein
  Mülleimer verspricht Vernichtung und schreckt vom Klicken ab.
- **„Einrichtung" heißt jetzt „Einrichtung starten".** Ein Verb sagt, dass etwas
  losgeht; das Wort allein klang nach einem Ort zum Nachschlagen.
- Die Höhe der Melde-Leiste wächst jetzt mit der eingestellten Schriftgröße mit.
  Sie stand fest auf 26 Pixel, wodurch die Symbole bei „groß" oben und unten
  herausragten.

### Entfernt

- **Der Autostart-Schalter ist aus der Melde-Leiste verschwunden.** Ein
  Ein/Aus-Zeichen heißt überall „Gerät ausschalten", und es saß direkt neben dem
  Kreuz, das das Programm wirklich schließt — zwei Knöpfe, die beide nach „aus"
  aussahen. Die Einstellung steht unverändert unter „Allgemein".
- **Der Knopf für den Einrichtungs-Assistenten ist aus der Melde-Leiste
  verschwunden.** Er bleibt im großen Fenster oben rechts erreichbar — in den
  Einstellungen reicht er, dorthin geht ohnehin jeder, der merkt, dass etwas
  klemmt.

### Behoben

- **Ein Hilfetext zeigte auf ein Zeichen, das es nicht mehr gab.** „Mit ☰
  öffnest du jederzeit die Bauplan-Liste" stand noch im Einrichtungs-Assistenten,
  obwohl das `☰` seit v3.0.0-rc55 durch das Klemmbrett ersetzt war. Alle
  Texte benennen die Symbole jetzt in Worten statt sie abzubilden.

### Dank


## v3.0.0 - 2026-08-29

> **Ein Fenster für alles.** Bauplan-Liste und Einstellungen lagen bisher in zwei
> getrennten Fenstern, und man musste wissen, in welchem etwas steckt. Jetzt liegen sie
> zusammen — mit Reitern links, einer sichtbaren Ablage für deine Dateien und einem
> Installer, statt eine Datei von Hand irgendwohin zu ziehen.

### Das Wichtigste in Kürze

- **Die Liste zeigt, was mit dem Patch neu ins Spiel kam.** Neben „beobachtet"
  steht jetzt **🔵 neu im Spiel**. Der Katalog stempelt jedem Bauplan die
  Spielversion auf, in der es ihn zum ersten Mal gab; der Filter zeigt die des
  aktuellen Patches. Kommt der nächste, rücken die neuen nach und die alten
  fallen heraus — der Stempel bleibt aber stehen, du siehst später noch, mit
  welchem Patch ein Bauplan kam. Mit 4.10.0 sind es 21.
- **Eine eigene Patch-Historie**, damit die Angabe auch stimmt. Verglichen wird
  nicht mehr gegen den Katalog von letzter Woche, sondern gegen **alle je
  gesehenen** Baupläne. Der erste Versuch meldete 74 Zugänge, von denen 53
  längst im Spiel waren — die Datenquelle hatte sie zwischendurch schlicht nicht
  geführt. Nachsehen ließ es sich nicht mehr: scmdb hält nur die aktuelle
  Spielversion vor, die Daten zu 4.9.0 waren am selben Tag schon gelöscht.
  Deshalb schreibt das Werkzeug jetzt selbst mit, was ein Patch gebracht hat
  (`daten/patch-historie.json`, im Repo nachlesbar) — nur die Zugänge, nie der
  ganze Katalog.
- **Ein Auswahlfeld „Patch"** neben den übrigen Filtern: dort lässt sich jeder
  frühere Patch nachschlagen — „was kam mit 4.10.0?". Das Feld **erweitert sich
  von allein**; jeder Patch, der Baupläne bringt, steht beim nächsten Öffnen
  darin, mit der Anzahl dahinter.
- **Ein Installer für Windows** — herunterladen, starten, fertig. Kein Herumschieben
  von Dateien mehr.
- **Ein Fenster statt zwei**, mit Reitern links. Dazu ein Symbol neben der Uhr,
  über das du es jederzeit zurückholst.
- **Das Overlay kann sich zurückhalten** und blendet sich nur bei einem Fund ein —
  ein schmaler grüner Streifen bleibt am Rand, die Maus holt es zurück.
- **Das Selbst-Update funktioniert jetzt auch unter Linux.** Dort scheiterte es
  bisher **immer**; wer ein AppImage nutzt, musste jede Version von Hand holen.
- **Star Citizen lässt sich aus dem Werkzeug heraus starten**, und ein
  Diagnose-Bericht sammelt auf Knopfdruck alles, was eine Fehlermeldung braucht —
  ohne Namen und ohne Pfade.

### Beim Umstieg von v2.0.0

- **Dein Bauplan-Bestand zieht von allein mit.** Er lag versteckt in
  `%APPDATA%`, jetzt liegt er sichtbar unter `Dokumente\SC BP Watcher`. Beim
  ersten Start wird er **kopiert**, nicht verschoben — der alte Ordner bleibt
  unangetastet stehen, falls doch etwas fehlt.
- **Nimm für dieses eine Update das Setup, nicht den Knopf im Programm.** Der
  Knopf tut es auch, benutzt aber noch den Update-Weg von v2.0.0 — und der
  lässt unter Windows ein Konsolenfenster stehen, bis du das Programm beendest.
  Ein Fehler im Update-Weg kann sich nicht selbst reparieren; ab v3.0.0 ist das
  erledigt, ab dann genügt der Knopf.
- **Hast du die `.exe` bisher von Hand irgendwohin gelegt, lösch sie nach der
  Installation.** Das Setup legt das Programm unter
  `%LOCALAPPDATA%\Programs\SC BP Watcher` ab. Die alte Datei bleibt sonst
  liegen, und irgendwann startest du versehentlich wieder die alte Version.
- **Unter Linux ist nichts zu tun** — das AppImage tauscht sich selbst aus.

### Hinzugefügt

- **Ein eigener Reiter „Serverstatus".** Läuft Star Citizen gerade? Wer nicht
  ins Spiel kommt, sucht den Fehler zuerst bei sich — ein Blick ins Werkzeug
  beantwortet das vorher. Gezeigt wird, was CIG auf seiner Statusseite meldet:
  die Lage der drei Systeme, dazu die Meldungen der letzten zwei Monate im
  Volltext samt Update-Zeilen. Der Aufbau folgt der Statusseite, die Zustände
  bleiben im **Wortlaut von CIG** (`operational`, `maintenance`) — eine
  Übersetzung wäre eine Aussage, die RSI nie gemacht hat. Die Seite fragt
  jede Minute nach, solange der Reiter offen ist; das kostet fast nichts, weil
  mit `ETag` gefragt wird und der unveränderte Fall ohne Inhalt beantwortet
  wird. Die Quelle steht als anklickbarer Verweis darunter.
  ⚠️ Die Angaben sind **von Hand gepflegt, keine Messung** — das steht auch in
  der Anzeige, damit niemand sie für eine Messung hält.
- **Ein Knopf für „gib mir einfach die neueste".** Bisher musste man erst
  verstehen, was ein Kanal ist, und den richtigen der beiden Kästen anklicken —
  wer den falschen wählte, bekam gar nichts angeboten. Jetzt steht darüber ein
  Knopf über die volle Breite, der sofort holt, was es gerade gibt, auch eine
  Testfassung. An der Einstellung darunter ändert er nichts.

- **Star Citizen lässt sich aus dem Werkzeug heraus starten.** Auf der Seite
  „Angaben im Spiel" steht ein Knopf, der das Spiel über den Weg startet, den
  man ohnehin benutzt: den RSI Launcher unter Windows, den `lug-helper` unter
  Linux. Wird keiner der beiden gefunden, erscheint der Knopf gar nicht erst —
  wer einen eigenen Weg hat (Lutris, Heroic), trägt ihn als `spielstarter` in
  die Einstellungsdatei ein. Vorgeschlagen von Morkhan.

- **Die Maus holt das Overlay zurück.** Im Aufblend-Betrieb genügt es, dorthin zu fahren, wo
  es steht — es kommt von selbst und bleibt, solange der Zeiger darauf ist. Vorher musste
  man das Programm dafür neu starten, und das verlangt kein anderes Overlay.

- **Neustart direkt nach dem Update.** Bisher hieß es „beim nächsten Start läuft die neue
  Version" — man musste selbst beenden und wieder starten. Jetzt wird der Holen-Knopf nach
  dem Laden zu **„⟳ Jetzt neu starten"**. Der Einzelinstanz-Wächter wird dabei zuerst
  geschlossen, sonst hielte sich die neue Version für die zweite und beendete sich sofort.

- **Startverlauf im Diagnose-Bericht.** Ein Absturz beendet das Programm sofort — kein
  Fehlerbericht wird mehr geschrieben, und es bleibt nur „es stürzt ab". Jeder Startschritt
  wird jetzt sofort auf die Platte geschrieben; die letzte Zeile im Bericht sagt, wie weit
  es kam.

- **Version holen, direkt aus dem Fenster.** Unter jeder der beiden Karten („Nur fertige
  Versionen" / „Auch Testfassungen") steht ein Knopf über die volle Breite, der die letzte
  Version dieses Kanals lädt und einspielt — auch zurück von einer Testfassung auf die
  letzte fertige.

- **Eintrag im Startmenü (Linux).** Der Assistent bietet ihn am Ende an, die Einstellungen
  jederzeit. Unter Windows macht das der Installer — unter Linux lag das AppImage bisher
  im Download-Ordner und stand in keinem Menü. Auf den Eintrag lässt sich außerdem eine
  Tastenkombination legen, mit der das Overlay zurückkommt.
- **Symbol im Infobereich (Windows).** Linksklick holt das Fenster, Rechtsklick zeigt ein
  kleines Menü. Der Schalter dafür stand schon in den Einstellungen; das Symbol selbst
  gab es nie.

- **Das Overlay kann sich zurückhalten.** Neu wählbar: dauerhaft sichtbar wie bisher,
  oder nur kurz aufblenden, wenn wirklich ein Bauplan dazukommt. Zurück holt man es,
  indem man das Programm noch einmal startet — auf die Verknüpfung lässt sich eine
  Tastenkombination des Systems legen. Angeregt von Haldjas (pr0): „Wenn ich im
  Kampf mit der Maus ins Overlay komme, wird das unangenehm."
- **Mausklicks lassen sich ins Spiel durchreichen.** Das Overlay bleibt sichtbar, fängt
  aber keine Klicks mehr ab. Unter Windows über `WS_EX_TRANSPARENT`, unter Linux über die
  XShape-Erweiterung; unter nativem Wayland geht es nicht, und das sagt die Einstellung
  dann auch statt einen wirkungslosen Schalter zu zeigen.
- **Ein zweiter Programmstart öffnet keine zweite Version mehr,** sondern holt die
  laufende hervor.

- **Ein Fenster mit Reitern.** Oben die Baupläne, darunter die Einstellungen, ganz unten
  eingeklappt, was nur Fortgeschrittene brauchen. Das Overlay bleibt klein wie bisher;
  dieses Fenster ist das, was sich dahinter öffnet.
- **Ein Installer für Windows.** Startmenü-Eintrag, optionales Desktop-Symbol, optionaler
  Autostart — und eine ordentliche Deinstallation. Wer lieber nichts installiert, findet
  die blanke `.exe` weiterhin im Release.
- **Deine Dateien liegen jetzt sichtbar** unter `Dokumente\SC BP Watcher`, getrennt nach
  Bauplänen, Exporten, Einstellungen und Diagnose. Vorher lagen sie versteckt im
  System — dort sucht niemand seinen Bauplan-Bestand. Beim ersten Start werden sie
  **kopiert**, der alte Ordner bleibt als Rückweg liegen.
- **Vorhandenen Bestand einlesen** — aus dem KRT Profit Basetool, von scmdb.net, aus der
  Launcher-Datei oder einer eigenen Sicherung. Das Format wird am Inhalt erkannt, du
  wählst nur eine Datei. Zusammengeführt, nie ersetzt.
- **Fehler melden mit einem Klick.** „Fehler melden" öffnet ein fertig ausgefülltes
  Formular; du schreibst nur noch dazu, was passiert ist. Der Bericht enthält keine Namen
  und keine Pfade mit deinem Benutzernamen.
- **Testfassungen auf Wunsch.** Wer beim Prüfen helfen will, schaltet sie unter *Über*
  ein und bekommt neue Versionen vor allen anderen — über dieselbe Update-Meldung.
- **Schriftgröße in vier Stufen**, wirkt auf Schrift, Symbole und Knöpfe zugleich.
- **Woher Baupläne ohne Auftrag kommen.** 55 Baupläne schüttet kein regulärer Auftrag
  aus — sie stammen aus benannten Töpfen wie XenoThreat, RDC-Boss oder RedWind. Statt
  eines Fragezeichens steht dort jetzt die Quelle, und man kann danach filtern.
- **Was ist neu** als eigener Reiter, getrennt nach Neu, Verbessert und Behoben.
- **Startbaupläne** werden erkannt und eingetragen — die acht, die jeder von Anfang an
  hat, mit ◆ gekennzeichnet.
- **Bestand ausgeben** in drei Formaten: KRT Profit Basetool, scmdb.net und eine
  vollständige Sicherung.

### Geändert

- **„Pfade" ist zu den Fortgeschrittenen gewandert.** Spielordner und Launcher
  werden gesucht und gefunden; wer doch nachhelfen muss, wird vom
  Einrichtungsassistenten geführt, der erklärt, was die Seite nur als Felder
  zeigt. Ein Reiter, den fast niemand braucht, stand oben nur im Weg.

- **Star Citizen starten sitzt jetzt links unten**, im markanten Grün über
  „Für Fortgeschrittene". Vorher stand der Knopf auf der Seite „Auftragstexte" —
  dort, wo es um Bauplan-Angaben geht — und war danach nur im Overlay zu sehen,
  also nur solange das eingeblendet ist. Jetzt ist er auf **jeder** Seite da.

- **Ein Discord-Knopf** darunter, bewusst ruhiger gehalten: Das Spiel zu starten
  ist die Handlung, für die man das Fenster offen hat, der Weg zum Discord ist
  ein Angebot. Zwei gleich laute Knöpfe nehmen sich gegenseitig die Wirkung.

- **„Jetzt nachsehen" heißt jetzt „Auf Aktualität prüfen".** Der alte Text sagte
  nicht, wonach nachgesehen wird. „Aktualisieren" wäre falsch gewesen — der Knopf
  prüft nur, geholt wird nichts.

- **„Noch keine Version bekannt" klang nach einem Fehler.** Der Knopf sagte
  nicht, was zu tun ist — jetzt steht dort „Erst oben auf ‚Jetzt nachsehen'
  drücken". Und der Kasten „Nur fertige Versionen" trägt den Zusatz
  „empfohlen", damit niemand raten muss, was er wählen soll. Beides fiel bei
  Morkhans Test auf.

- **Der Reiter heißt „Update & Über".** „Über" allein findet niemand, der ein
  Update sucht — der Autor selbst hat dort nicht danach gesucht.

- **Der Startknopf für Star Citizen saß an einer Stelle, an der ihn niemand
  sucht.** Er stand unter „Angaben im Spiel", also dort, wo es um Auftragstexte
  geht — selbst der Autor fand ihn nicht wieder. Jetzt sitzt er als grünes „▶"
  oben im Overlay bei den übrigen Zeichen: Wer das Spiel starten will, hat das
  große Fenster ohnehin nicht offen. Beim Überfahren sagt die Statuszeile, was
  der Klick tut.

- **Vor dem Einsetzen einer Übersetzung wird gefragt.** „Deutsch" und
  „StarStrings" ersetzen die Textdatei des Spiels vollständig — danach ist das
  ganze Spiel in dieser Sprache, nicht nur die Bauplan-Angaben. Das stand
  nirgends; jetzt sagt es der Erklärtext, und vor dem ersten Einsetzen kommt
  eine Rückfrage. Einmal bestätigt, wird nicht wieder gefragt. „Original"
  fragt nicht, weil es die Sprache nicht ändert.

- **Das Overlay hinterlässt im Aufblend-Betrieb einen schmalen grünen Streifen.** Maus
  darauf, und es ist wieder da. Der erste Versuch fragte dafür die Mausposition ab — das
  kann unter Wayland nicht funktionieren: Gemessen meldete Tk zwölfmal denselben Wert,
  während die Maus quer über den Schirm fuhr. Eine Anwendung erfährt die Zeigerposition
  dort nur, solange er über einem **ihrer eigenen** Fenster steht. Der Streifen ist so ein
  Fenster — und nebenbei ehrlicher als eine unsichtbare Zauberzone: Man sieht, wo das
  Overlay wartet.

- **Der Fehlerbericht sagt, aus welcher Version ein Fehler stammt** — und markiert die, die
  aus einer älteren kommen. Der Speicher hebt die letzten zehn über Programmstarts hinweg
  auf; nach einem Update standen dort Fehler, die längst behoben waren, und der Bericht sah
  aus, als sei alles noch kaputt.

- **Bis zu zwölf Bezugswege je Bauplan** statt drei. Gemessen: Über die Hälfte aller
  Baupläne hatte vorher abgeschnittene Wege. Angezeigt wird weiterhin der leichteste, der
  Rest klappt auf.
- **Die Herkunft erscheint erst auf Klick** und lässt sich wieder schließen — bei kleinem
  Fenster fraß sie sonst ein Drittel der Liste.
- **Filtern nach Art, Klasse, Größe, Gütegrad und Quelle**, zusätzlich zu Suche und den
  Listen „beobachtet / vorhanden / fehlt noch".
- **Overlay einklappen** (▾): schiebt sich auf die Titelleiste zusammen.
- **Kein Speichern-Knopf mehr** — Änderungen greifen sofort.

### Behoben

- **Das eingeklappte Overlay ließ sich nicht wieder aufklappen.** Der Knopf
  schaltete um, sichtbar passierte nichts — das Werkzeug war zu und blieb es.
  Ursache: Beim Einklappen wurde die aktuelle Fensterhöhe als „offene" Höhe
  gemerkt. Liefen der gemerkte Zustand und die tatsächliche Geometrie einmal
  auseinander, schrieb der nächste Einklapp-Vorgang die **Leistenhöhe** als
  offene Höhe fest; ab da klappte das Fenster auf seine eigene Größe „auf".
  Jetzt wird die Höhe nur gemerkt, wenn das Fenster wirklich offen ist, und
  beim Aufklappen gilt eine Mindesthöhe.
- **Der Ziehgriff für die Fenstergröße deckte im eingeklappten Zustand das ✕
  zu.** Er sitzt unten rechts — bei einem auf Leistenhöhe geschrumpften Fenster
  ist das dieselbe Stelle wie oben rechts, und man musste zielen, um das
  Werkzeug überhaupt schließen zu können. Er hängt jetzt an der **Liste** statt
  am Fenster — ist die eingeklappt, hat sie keine Höhe, und der Griff ist
  zwangsläufig mit weg. Ihn stattdessen rechtzeitig auszublenden hat dreimal
  nicht verlässlich geklappt: Ein Zustand, der sich aus dem Aufbau ergibt, ist
  verlässlicher als einer, den man nachträglich herstellt.
- **Bauplan-Namen waren ohne Launcher unlesbar** — „Golemmc4Orepod" statt
  „GOLEM MC-4 Ore Pod". Der Rückfall war `.title()` auf den Vergleichsschlüssel,
  in dem es keine Wortgrenzen mehr gibt; der lesbare Name lag die ganze Zeit
  daneben im Zwischenspeicher. Betraf **jeden Linux-Nutzer**, weil es dort nie
  einen Launcher gibt.
- **Das Selbst-Update unter Windows kam nie an.** Wer auf „holen" klickte, bekam
  eine Warnung und danach passierte nichts — außer 14 MB verwaister Datei im
  Programmordner, bei jedem Versuch aufs Neue. Dahinter steckten **zwei**
  Fehler, von denen jeder allein schon gereicht hätte:

  Geholt wurde die **falsche Datei**. An jeder Freigabe hängen drei Anhänge,
  gesucht wurde die erste auf `.exe` — und weil GitHub alphabetisch sortiert und
  ein `-` vor einem `.` steht, kam `SC-BP-Watcher-Setup.exe` zuerst. Der
  Installer wurde also über die Programmdatei geschoben, ohne je ausgeführt zu
  werden: Wer den Watcher danach öffnete, bekam ein Setup-Fenster.

  Und der Tausch konnte ohnehin nicht stattfinden. Nach dem Beenden lebt der
  Bootloader weiter und räumt seinen Ordner unter `%TEMP%` auf; blieb dabei eine
  Datei gesperrt, stand er im Fenster „Failed to remove temporary directory"
  still — und hielt damit die `.exe`, auf deren Freigabe das Hilfsskript wartete.
  Nach zwei Minuten gab es auf. Der Nutzer hätte eine Warnung wegklicken müssen,
  von der niemand wusste, dass sie zum Update gehört.

  **Unter Windows startet jetzt der Installer**, statt dass das Programm seine
  eigene Datei tauscht. Er beendet den laufenden Watcher selbst, ersetzt ihn,
  pflegt den Eintrag in „Apps & Features" und fährt ihn wieder hoch. Unter Linux
  bleibt es beim bewährten Tausch des AppImage.

- **Das Symbol neben der Uhr erschien unter Windows nie.** Es wurde bei jedem
  Start angelegt und scheiterte jedes Mal an derselben Stelle, sichtbar nur im
  Fehlerbericht: `argument 11: OverflowError: int too long to convert`. Der
  Aufruf zum Anlegen des Fensters hatte keine Typangaben, und ohne die reicht
  Python jeden Wert als 32-Bit-Zahl weiter — die Kennung, um die es ging, ist
  unter 64-Bit-Windows breiter. Derselbe Fehler steckte im Rückgabetyp der
  Fensterfunktion. Beim Beenden räumt das Symbol sich jetzt auch wirklich auf:
  Der bisherige Weg durfte von außen gar nicht greifen und lief still ins Leere.

- **Die angezeigte Version in „Apps & Features" blieb stehen.** Nachgesehen
  wurde nur im Benutzerzweig der Registry. Wer beim Installieren „für alle
  Nutzer" gewählt hatte, dessen Eintrag liegt aber im Maschinenzweig — dort
  wurde nie nachgezogen, und Windows zeigte weiter eine Nummer, die es nicht
  mehr gab. Jetzt werden beide Zweige durchsucht. Zusätzlich fragt der Installer
  nicht mehr nach „für mich" oder „für alle": Das Programm landet ohnehin im
  eigenen Benutzerordner, damit entfällt die Rückfrage und jede
  Administrator-Abfrage beim Aktualisieren.

- **Die Symbole in der Leiste sahen unter Windows entstellt aus.** In
  `Segoe UI` steckt **kein einziges** der vierzehn Zeichen — Windows suchte
  sich je Zeichen selbst eine Ersatzschrift und griff dabei zu **Segoe UI
  Emoji**: bunte, quadratische Emoji-Bildchen in einer schlanken dunklen
  Leiste, dazu in ungleichen Breiten (10 bis 21 Pixel bei gleicher Größe).
  Deshalb ließen sich die Symbole auch nie über die Schriftgröße angleichen —
  sie kamen aus verschiedenen Schriftdateien. Jetzt wird unter Windows
  ausdrücklich **Segoe UI Symbol** verlangt: alle vierzehn Zeichen einfarbig,
  in der eingestellten Textfarbe, halb so breit gestreut. Unter Linux war es
  nie ein Problem und bleibt unverändert.

- **Das Overlay blieb beim Umschalten auf Englisch deutsch.** Wer die Sprache
  wechselte, bekam ein englisches Fenster und eine deutsche Melde-Leiste:
  „8 Baupläne · Log ✓ · ohne Launcher · geprüft", dazu „Warte auf neue
  Baupläne …" und der Autostart-Text. Die englischen Versionen dieser Sätze
  gab es längst — benutzt hat sie niemand, der Code setzte die deutschen
  weiter fest zusammen. Zusätzlich erfuhr das Overlay vom Sprachwechsel
  überhaupt nichts; nur das Einstellungsfenster beschriftete sich neu.
  Dasselbe betraf die Meldung „neu im Spiel craftbar" der Katalog-Wache.
  Und Meldungen, die beim Umschalten **schon in der Leiste standen**, blieben
  ebenfalls deutsch — etwa „Keine Log-Sicherungen gefunden". Sie wurden fertig
  zusammengesetzt in die Zeile geschrieben und waren damit in der Sprache von
  vorhin eingefroren; erst ein Neustart räumte das auf. Meldungen tragen jetzt
  ihren Textschlüssel mit und werden beim Sprachwechsel neu gesetzt — samt
  Datum, das im Englischen anders geschrieben wird (2026-08-22 statt
  22.08.2026).

- **Der Hinweis am Startknopf ▶ überschrieb die Statuszeile.** Als einziges der
  zehn Zeichen hatte er keine Erklärblase, sondern schrieb in die Statuszeile
  und stellte danach einen Merker wieder her, der nie fortgeschrieben wurde —
  eine Fundmeldung war nach einem Mausschlenker über das Zeichen weg.

- **Das Logo fehlte in der fertigen Version.** Auf „Update & Über" lud das
  Programm `assets/xharig.png`, der Bau packte diese Datei aber nie ein — beim
  Start aus dem Quellcode fiel das nie auf, weil sie dort liegt.

- **Das „ⓘ" am Overlay öffnete ein eigenes Fenster mit eigener Update-Logik** —
  und in dem fehlte der Neustart-Knopf. Wer darüber ging, lud die neue Version
  herunter und stand dann vor einem Satz statt vor einem Knopf. Jetzt führt es
  ins Hauptfenster auf „Was ist neu"; der Reiter „Update & Über" liegt daneben.
  **Ein Weg statt zwei.** Gemeldet von Morkhan.
- **Gestreckte Knöpfe füllten nur die halbe Breite.** Betraf vor allem die
  Knöpfe unter den beiden Update-Kästen. Gemeldet von Morkhan.

- **Das Update über das Infofenster kam nie an.** Wer über das grüne „ⓘ" am
  Overlay ging statt über die Einstellungen, bekam nach dem Laden nur den Satz
  „Beim nächsten Start läuft die neue Version" — **und keinen Knopf dafür**.
  Unter Windows stimmt der Satz zudem nicht: Dort tauscht ein Hilfsskript die
  Datei erst, wenn das Programm beendet ist, und gibt nach zwei Minuten auf. Wer
  weiterspielte, hatte am Ende gar kein Update. Jetzt steht dort derselbe
  „⟳ Jetzt neu starten"-Knopf wie in den Einstellungen. Gemeldet von Morkhan.
- **Beim Update blitzte kurz ein Konsolenfenster auf.** Das Hilfsskript läuft
  seit v3.0.0 unsichtbar — der `taskkill` davor, der ein schon laufendes Skript
  wegräumt, wurde dabei übersehen. Gemeldet von Morkhan.

- **Fünf Fehler scheiterten bisher lautlos.** Ließen sich Einstellungen, die
  Merkliste, der „Neu"-Stand, der Autostart oder ein gespeicherter Bericht nicht
  schreiben, passierte einfach nichts — die Einstellung war nach dem Neustart
  wieder alt, und im Fehlerbericht stand nichts. Diese Stellen melden jetzt.

- **Der Fehlerbericht ließ die Spielsprache leer.** Dort stand nur ein Strich,
  obwohl die Erkennung einwandfrei lief — die Abfrage lieferte zwei Werte, der
  Bericht erwartete einen, und der Fehler wurde stillschweigend verschluckt.
  Jetzt steht dort, wonach im Log gesucht wird **und woher die Formulierung
  stammt**: aus der `global.ini` des Spiels oder aus der eingebauten Tabelle.
  Das ist die erste Frage bei „er erkennt meine Baupläne nicht".
- **Abgeschnittene Beschreibungen an drei Stellen.** Bei schmalem Fenster fehlten
  wenige Pixel, und die letzten Zeichen fielen weg. Betroffen waren die
  Update-Kanäle, „Angaben in die Auftragstexte schreiben" und „Wie oft
  nachgesehen wird".

- **Der Assistent merkte sich die gewählte Textquelle nicht.** Er holte die
  Texte und setzte sie ein, schrieb die Wahl aber nirgends hin — unter „Angaben
  im Spiel" stand danach keine der drei Quellen angewählt. Gemeldet von Haldjas.
- **Update unter Windows spuckte Konsolenfenster aus.** Das Hilfsskript, das die
  laufende `.exe` austauscht, lief in einer Endlosschleife weiter, solange die
  Datei gesperrt war — und sie bleibt gesperrt, bis das Programm beendet wird.
  Jeder weitere Klick auf „holen" startete noch ein Fenster. Jetzt ist nach zwei
  Minuten Schluss, das Fenster bleibt unsichtbar, und ein schon laufendes
  Hilfsskript wird vorher beendet.
- **„Jetzt nachsehen" hat nicht nachgesehen.** Der Knopf zeigte die Meldung „Suche nach
  einer neuen Version …" und suchte nicht. Wessen Zwischenspeicher veraltet war, kam damit
  nicht heraus — ein Tester bekam auf rc18 weiterhin rc12 angeboten. Jetzt wird wirklich
  gefragt, das Ergebnis gesagt und die Anzeige nachgezogen.
- **Das Selbst-Update ging unter Linux in den Windows-Zweig** und meldete „[Errno 2] No such
  file or directory: 'cmd'". Der Riegel gegen fremde Programme verglich den eigenen Code mit
  `APPDIR` — nur entpackt sich PyInstaller in ein **eigenes** Verzeichnis, der Vergleich
  schlug also immer fehl. Maßgeblich ist jetzt der Dateiname.
- **Das Selbst-Update hätte fremde Programme überschreiben können.** Es hielt jede Datei
  für sich selbst, auf die die Umgebungsvariable `APPIMAGE` zeigte — und die steht in
  **jedem** Programm, das aus einem AppImage heraus gestartet wurde. Jetzt muss auch der
  eigene Code aus dem zugehörigen `APPDIR` kommen, und ein zweiter Riegel lehnt jede
  Zieldatei ab, deren Name nicht zum Programm gehört.
- **Das Selbst-Update scheiterte unter Linux immer.** Geladen wurde nach `/tmp`,
  eingespielt mit `os.replace()` — und `/tmp` ist auf so gut wie jedem Linux ein eigenes
  Dateisystem. Über Dateisystemgrenzen kann `os.replace` nicht verschieben, das endet mit
  „[Errno 18] Invalid cross-device link". Der Kommentar im Code versprach schon immer
  „neben das laufende Programm" — jetzt tut es der Code auch, und das Einspielen ist
  nebenbei atomar geworden.
- **Absturz beim allerersten Start** (`SIGSEGV`), gemeldet von Bomb20. Der Assistent legte
  eine **eigene** Tk-Instanz an und zerstörte sie am Ende; das Overlay legte danach eine
  zweite an. Nach dem `destroy()` der ersten leben Schriften, Bilder und offene Aufträge
  weiter und zeigen auf einen toten Interpreter — ob das gutgeht, hängt am Zeitpunkt. Sein
  Satz „mit Debugging an lief es durch" ist der Fingerabdruck dafür. Es gibt jetzt nur noch
  **eine** Tk-Instanz im ganzen Programm.
- **Die Marken `[SCBPW]` waren im Spiel sichtbar.** Im Auftragstitel stand „Security
  Patrol**[SCBPW]** [BP 3/6]**[/SCBPW]**". Sie sorgten dafür, dass sich Eingefügtes exakt
  wieder entfernen lässt — nur will das niemand in seinem Spiel lesen. Jetzt steht gar
  keine Marke mehr im Text: Der **Wortlaut vor der Einfügung** wird gemerkt, und das
  Zurücksetzen stellt ihn wieder her. Das ist genauer als vorher. Geprüft mit
  `tools/injektion_pruefen.py` an der echten Datei: einspielen und entfernen lässt 743
  Textstellen auf das Zeichen genau so, wie sie waren.
- **Im Spiel stand nur die Zahl, nicht welche Baupläne.** Ein Auftrag hat einen Titel, aber
  oft ein Dutzend Beschreibungen — je eine für „zur Ruinenstation", „zum Verteilzentrum"
  und so weiter. Die Vertragsdaten nennen dazu nur **eine**; die übrigen blieben leer. Im
  Titel stand „[BP 0/12]", und wer die Beschreibung öffnete, um zu sehen *welche* zwölf,
  fand nichts. Gemessen: allein bei Covalex 51 Beschreibungen im Spiel, davon 7 mit
  Angaben. Sie werden jetzt über den gemeinsamen Namensanfang mitversorgt.
- **„Handfeuerwaffe" und „FPS-Waffe" waren zwei Gruppen für dieselbe Sache** — 87 unter
  der einen Kennung, zwei unter der anderen.
- **„Zeilen im Overlay" hatte keine Wirkung.** Die Einstellung wurde gespeichert und nie
  gelesen; im Overlay galt fest die Zahl 200. Jetzt gilt der eingestellte Wert, mit 20 als
  Vorgabe — 200 Baupläne sammelt in einer Sitzung ohnehin niemand.
- **„Durchsuchen" öffnete keinen Dialog** — weder beim Star-Citizen-Ordner noch bei den
  eigenen Dateien. Beide tun es jetzt, und unter Linux mit dem Dialog des Systems statt
  dem grauen von Tk.
- **Die letzten Baupläne der Liste lagen übereinander.** X11 rechnet Fensterkoordinaten in
  16 Bit; alle 722 in einem Rahmen ergeben rund 33000 Pixel und damit 16 Zeilen jenseits
  der Grenze. Die Liste wird jetzt bei Bedarf in Blöcken gezeigt — sichtbar bleibt alles.
- **Die Rollleiste ließ sich nicht anfassen.** Gezeichnet wurde der Griff mit einer
  Mindesthöhe, geprüft wurde mit der rechnerischen — wer die untere Hälfte traf, galt als
  „daneben".
- **Das Fenster startete außerhalb des Bildschirms.** Ohne gemerkte Lage stellte Tk es
  nach `+0+0`; bei einem hochkant stehenden Monitor links außen liegt dort kein Bild.
  Start und „Fensterlage zurücksetzen" setzen es jetzt mittig auf den Hauptbildschirm.
- **Der Autostart war zwischen Overlay und Einstellungen nicht synchron.** Beide lasen
  ihren Zustand nur beim Zeichnen.
- **Das Fenster-Icon fehlte in jeder fertigen Version** — auf beiden Systemen. Die Datei
  lag zur Laufzeit gar nicht bei.

### Dank

Diese Version ist zu einem großen Teil das Verdienst von zwei Testern, die sich
die Mühe gemacht haben, Fehler nicht nur zu bemerken, sondern sie so genau zu
beschreiben, dass sie zu finden waren:

- **Haldjas** (pr0) — der Vorschlag mit dem Aufblend-Betrieb; dazu das
  Setup, das an der laufenden Datei abbrach, die Konsolenfenster beim Update,
  das verschwundene Symbol neben der Uhr, der Absturz nach dem Neustart, die
  Schriftgröße, die das Overlay nicht erreichte, die vergessene Textquelle im
  Assistenten — und der Fund, der alles erklärte: „da bleibt er bei rc25".
- **Bomb20** (pr0) — der Absturz beim allerersten Start (der Fehler, den nur
  neue Nutzer je gesehen hätten), der wirkungslose Knopf „Jetzt nachsehen" und
  der Hinweis, dass die Textquelle „Deutsch" das ganze Spiel übersetzt.
- **Morkhan** (KRT) — der Vorschlag, Star Citizen gleich aus dem Werkzeug
  heraus starten zu können.

Die Bauplan-Angaben beruhen auf den offen veröffentlichten Vertragsdaten des
**SC-Deutsch-Launcher-Teams** und auf **scmdb.net**.

## v2.0.0 - 2026-08-24

**Aus dem Windows-Overlay ist ein eigenständiges Werkzeug für Windows und Linux
geworden — und es schreibt die Bauplan-Angaben auf Wunsch direkt ins Spiel.**

Der SC Deutsch Launcher wird nicht mehr gebraucht. Geprüft an einer echten
Star-Citizen-Installation, mit deutschem **und** englischem Client.

### Ohne Launcher

- **Die `Game.log` ist die Quelle.** Der Bauplan-Bestand wird selbst geführt; beim ersten
  Start werden die aufgehobenen Spielprotokolle nachgelesen. Bleibt eine Lücke, sagt das
  Werkzeug das, statt eine unvollständige Liste als vollständig auszugeben.
- **Die Spielsprache erschließt sich von selbst.** Die Bauplan-Meldung im Log ist
  übersetzt; das Werkzeug leitet den Wortlaut aus den eigenen Logs ab — es kennt über 700
  Bauplan-Namen, und steht einer davon in einer Logzeile, ist der Text davor die gesuchte
  Formulierung. Deutsch und Englisch sind gemessen, andere Sprachen findet es selbst.
- **Ist der Launcher da, wird er weiter genutzt** — auch wenn er auf einer eingehängten
  Windows-Platte liegt, was bei Dual-Boot der Normalfall ist.

### Bauplan-Liste

- **Alle Baupläne zum Nachschlagen**, mit Suche, Filtern und Fortschritt. Gesucht wird
  über Name, Kategorie, Klasse (`military`, `stealth`, `civilian`, …), Hersteller und
  Gütegrad.
- **Woher jeder Bauplan kommt** — Fraktion, Auftrag, nötiger Ruf, Belohnung **und wo sich
  der Auftrag annehmen lässt**.
- **Vier Bereiche** zum Ein- und Ausblenden: Schiffsteile, FPS-Waffen, Rüstung & Kleidung,
  Sonstiges. Sortiert nach Bereichen statt nach Alphabet.
- **Merkliste per Klick.** Taucht ein beobachteter Bauplan auf, meldet das Werkzeug ihn
  auffällig — und trägt den erfüllten Wunsch selbst wieder aus.

### Bauplan-Angaben im Spiel

- **An jede Mission, die Baupläne ausschüttet**, kommt die Liste in den Missionstext —
  mit Kästchen: angehakt, was man hat, leer, was fehlt. Dazu ein Kürzel im Titel
  (`[BP 2/3]`), sichtbar schon in der Auftragsliste. **681 Textstellen**, deutsch und
  englisch.
- **Drei Wege zur Grundlage:** die deutsche Übersetzung von
  [rjcncpt](https://github.com/rjcncpt/StarCitizen-Deutsch-INI),
  [StarStrings](https://github.com/MrKraken/StarStrings) von MrKraken — oder die
  englischen Originaltexte aus dem eigenen `Data.p4k`, ganz ohne Download.
- **Rückgängig auf den Buchstaben genau.** Wer StarStrings nutzt, behält es: Dessen
  Auszeichnungen bleiben stehen, die eigenen kommen dazu.
- Es wird **gefragt**, nie stillschweigend gemacht. Voreingestellt ist nichts.
- **Es bleibt von selbst aktuell.** Beim Start und danach alle sechs Stunden wird
  nachgesehen: neue Übersetzung, neue Bauplan-Daten — oder eine `global.ini`, die ein
  Spiel-Patch ersetzt hat. Alles drei trägt sich dann selbst wieder ein.
  - **Warum das kein Beiwerk ist:** Jedes Übersetzungs-Update und jeder Patch schreibt
    die Datei neu, die Angaben sind dann **weg** — und nach einem Patch geben Missionen
    andere Baupläne aus. Beides fällt niemandem auf, weil das Spiel normal weiterläuft.
    Ohne diesen Abgleich spielt man irgendwann mit falschen Daten.
  - Angefasst wird nur, was der Spieler selbst eingerichtet hat.

### Bedienung

- **Einrichtungsassistent** in fünf Schritten, jederzeit wiederholbar — und ein
  **Einstellungsfenster** für alle Angaben auf einmal.
- **Deutsch und Englisch**, umschaltbar, wirkt sofort.
- Erklärtexte beim Überfahren jedes Zeichens, einstellbare Durchsichtigkeit (wichtig für
  alle mit nur einem Bildschirm), Signalton, Autostart.
- **Update-Meldung mit Änderungsprotokoll** — auch für übersprungene Versionen.

### Verteilung

- **Fertige Dateien für beide Systeme**, von GitHub bei jedem Versions-Tag gebaut. Das
  AppImage entsteht in einem Ubuntu-22.04-Container, damit es auf verbreiteten Systemen
  startet.
- ⚠️ **Wichtig für Arch, Fedora und openSUSE:** Genau dieser Container war auch eine
  Falle. Das gebündelte Python suchte seinen Zertifikatsspeicher unter dem Ubuntu-Pfad
  `/usr/lib/ssl`, den es dort nicht gibt — **jede** HTTPS-Verbindung scheiterte still.
  Kein Bauplan-Katalog, keine Übersetzung, keine Update-Meldung; das Programm startete,
  konnte aber nichts laden. Der Starter sucht den Speicher jetzt an allen üblichen
  Stellen. Auf Ubuntu und Debian fiel das nie auf.
- **Nichts Fremdes wird mitgeliefert.** Bauplan-Katalog (scmdb), Übersetzung und
  StarStrings werden zur Laufzeit beim Nutzer von ihrer eigenen Adresse geholt.

### Dank

Die Bauplan-Angaben beruhen auf den offen veröffentlichten Vertragsdaten des
**SC-Deutsch-Launcher-Teams** (813 Verträge, deutsch und englisch) und auf **scmdb.net**.
Ohne beide gäbe es diese Version nicht.

## v2.0.0-rc1 - 2026-08-24

> **Vorabversion zum Ausprobieren.** Der Umbau ist inhaltlich fertig und gründlich
> geprüft — aber noch nie an einer echten Star-Citizen-Installation gelaufen, nur
> an nachgebauten Logs. Wer sie testet, hilft genau dabei. Rückmeldungen gern als
> [Issue](../../issues).

**Aus dem Windows-Overlay ist ein eigenständiges Werkzeug für Windows und Linux
geworden.** Der SC Deutsch Launcher ist nicht mehr nötig, der Bauplan-Bestand wird
selbst geführt, und zu den meisten Bauplänen steht dabei, woher man sie bekommt.

### Hinzugefügt


- **Der Watcher findet die Spielsprache selbst heraus.** Die Bauplan-Meldung im Log ist übersetzt; bisher war nur die deutsche Formulierung gemessen, die englischen waren geraten und andere Sprachen gar nicht vorgesehen. Jetzt erschließt er sie aus den eigenen Logs: Er kennt über 700 Bauplan-Namen — steht in einer Logzeile einer davon, ist der Text davor die gesuchte Formulierung. An einer erfundenen französischen Version geprüft.
  - Verlangt werden **zwei** verschiedene Treffer für dieselbe Formulierung. Bei einem könnte es Zufall sein (ein Bauplan-Name taucht auch in anderen Meldungen auf).
  - Gefundenes landet in `phrasen.json` — derselben Datei, die man auch von Hand pflegen kann. Keine zweite, versteckte Wahrheit.
  - Damit ist das Werkzeug nicht mehr auf die Sprachen angewiesen, die jemand vorher eingetragen hat.
- **Projektseite auf Englisch und Deutsch**, mit Umschalter oben in beiden Versionen. **Englisch ist die Hauptseite** (`README.en.md`), Deutsch liegt daneben (`README.md`) — auf GitHub ist das Publikum international, und wer über die Star-Citizen-Foren kommt, sollte nicht erst einen Umschalter suchen müssen. Deutschsprachige Spieler kommen mit Englisch zurecht; umgekehrt gilt das seltener.
- **Merkliste per Klick** (`scbp/merkliste.py`). In der Bauplan-Liste macht ein Klick auf den Stern aus jedem Eintrag einen Wunsch — taucht er auf, meldet ihn der Watcher auffällig in Gold. Dafür muss niemand mehr eine `watchlist.json` von Hand anlegen.
  - Eigener Filter **⭐ beobachtet** zeigt, worauf man gerade wartet.
  - **Erfüllte Wünsche verschwinden von selbst.** Landet ein beobachteter Bauplan im Bestand, sagt der Watcher einmal Bescheid und trägt ihn aus — eine Liste voller längst erledigter Wünsche wäre keine Merkliste, sondern ein Archiv.
  - Von außen eingetragene **Muster** funktionieren weiter (ein eigenes Werkzeug des Autors schreibt dort Teile einer Rüstung hinein, deren endgültige Namen noch niemand kennt).
- **Fertige Dateien für beide Systeme, gebaut von GitHub.** Ein Versions-Tag löst den Bau aus: ein Windows-Rechner baut die `.exe`, ein Linux-Rechner das AppImage, beide werden ans Release gehängt — samt Beschreibung aus dem CHANGELOG, damit im Werkzeug unter „Was ist neu" dasselbe steht wie auf GitHub.
  - Das AppImage wird **in einem Ubuntu-22.04-Container** gebaut (glibc 2.35). Auf neuerem glibc gebaut, würde es auf verbreiteten Systemen gar nicht erst starten.
  - Der Bau bricht ab, wenn Tag und `__version__` nicht zusammenpassen. Wer „v2.0.0" lädt, soll im Fenster nicht etwas anderes lesen.
  - Niemand baut mehr selbst — weder die Nutzer noch der Entwickler.
- **Neue Versionen werden gemeldet und lassen sich nachlesen** (`scbp/aktualisierung.py`, `scbp/versionsfenster.py`). Das Werkzeug sieht höchstens einmal am Tag nach; gibt es etwas Neues, färbt sich ⓘ in der Titelleiste. Dahinter liegt die Versionsgeschichte — **auch für ältere Versionen**, damit man nachlesen kann, was man übersprungen hat.
  - Geladen wird ausschließlich von `github.com`; eine Datei von woanders wird abgelehnt.
  - Unter Linux ersetzt sich das AppImage selbst, unter Windows übernimmt ein Hilfsskript nach dem Beenden (eine laufende `.exe` kann sich nicht selbst überschreiben). Wer aus dem Quellcode startet, bekommt keinen Selbstersatz angeboten — dort ist `git pull` der richtige Weg.
- **Prüfintervall und Signalton sind einstellbar** (`pruefintervall_sekunden`, `signalton` in `einstellungen.json`). Grenzen 1–60; eine vertippte `0` wird auf 1 gezogen statt zur Dauerschleife.
- **Einrichtungsassistent** (`scbp/assistent.py`) — vier Schritte, **jederzeit wiederholbar** über ⟳ in der Titelleiste. Läuft beim ersten Start von allein und immer dann, wenn Star Citizen nicht gefunden wird.
  1. **Sprache** — zuerst, damit der Rest lesbar ist
  2. **Star Citizen finden** — mit Auswahlknopf und Prüfung *beim Tippen*, nicht erst beim Speichern. Der Spieler darf jede Ebene treffen: den LIVE-Ordner, den darüber, den Programme-Ordner oder gleich das Wine-Präfix — sogar die `Game.log` selbst. Es wird daraus der richtige Ordner gemacht und angezeigt, welcher genommen wird.
  3. **Bisherige Baupläne holen** — läuft von selbst, hier bekommt der Spieler seinen ganzen Bestand aus den aufgehobenen Logs geschenkt
  4. **Fertig** — was jetzt passiert und wo die Liste steckt
  - Wiederholbar ist Absicht: Wer sich mit Rechnern nicht auskennt, soll etwas nachstellen können, ohne zu wissen, in welchem Menü es steckt. Ein Assistent führt; ein Einstellungsfenster setzt voraus, dass man weiß, wonach man sucht.
- **Verwaltungsfenster aus der Melde-Leiste** — ☰ in der Titelleiste öffnet die Bauplan-Liste, ein zweiter Klick holt sie nach vorn statt ein zweites Fenster aufzumachen.

**Was sich am Verhalten ändert:** Wird Star Citizen nicht gefunden, zeigte das Programm bisher eine Meldung und **beendete sich** — der Spieler hätte eine JSON-Datei von Hand bearbeiten und neu starten müssen. Das macht niemand. Jetzt wird gefragt, und die Angabe wirkt sofort.

- **Bauplan-Katalog mit Herkunft** (`scbp/katalog.py`). 714 Baupläne, für 655 davon (92 %) steht dabei, **woher man sie bekommt**: Fraktion, Auftrag, nötiger Rang samt Rufpunkten, Belohnung in aUEC und Rufgewinn. Das kann der SC Deutsch Launcher nicht — „mir fehlt X" ist die halbe Information, „X droppt bei Fraktion Y ab Rang Z" die ganze.
  - Die Kette durch die scmdb-Daten: `contracts[].blueprintRewards[].blueprintPool` → `blueprintPools[…].blueprints[].name`, dazu `factions`, `minStanding` und `factionRewardsPools`.
  - Bezugsquellen sind nach **leichtestem Weg** sortiert (niedrigste Ruf-Anforderung zuerst), höchstens drei je Bauplan.
  - Der Sammel-Dump ist rund 12 MB und wird **nicht** aufgehoben, sondern sofort zu 347 KB eingedampft. Geholt wird einmal je Spielversion, mit Wiederholversuchen — bei der Größe reißt die Leitung gern mitten drin ab (beim Bauen zweimal passiert).
- **Verwaltungsfenster** (`scbp/bestandsfenster.py`): durchsuchbare Liste, nach Art gruppiert, Filter *alle / habe ich / fehlt mir*, Fortschrittsanzeige, Häkchen per Klick, Herkunft per Klick ausklappbar.
- **Deutsch und Englisch, umschaltbar** (`scbp/sprache.py`). Standard ist die Systemsprache, aber das Feld `sprache` in `einstellungen.json` (`de`/`en`/`auto`) sticht sie — wer ein englisches System fährt und trotzdem Deutsch lesen will, soll das dürfen. Umschalten wirkt sofort, ohne Neustart.
  - Auch die **Bauplan-Arten** hängen daran: `Char_Armor_Helmet` ist nichts für Menschen, „Helm" nichts für eine englische Liste.
  - Der Selbsttest prüft, dass jeder Text beide Sprachen hat und **jede Art aus dem Katalog übersetzt ist** — nach einem SC-Patch können neue dazukommen.

### Entfernt

- **`EXE bauen.bat`.** Seit GitHub die Dateien baut, braucht sie niemand mehr — und sie war bereits falsch: Sie baute ohne `--add-data`, die daraus entstandene `.exe` hätte weder Änderungsprotokoll noch Katalogdaten gehabt. Zum Ausprobieren lässt sich der Bau-Workflow ohne Tag von Hand starten.

### Wissenswert

- **714 Baupläne, nicht 1573.** Die Datei `crafting_items` zählt alle craftbaren Gegenstände; ein Bauplan droppt nur für einen Teil davon. Für eine Liste zum Abhaken wäre die große Zahl irreführend — maßgeblich sind die `blueprintPools`.
- **Die scmdb-Daten werden weiterhin nicht mitgeliefert** (CC BY-NC-ND), sondern beim Nutzer geholt. `SC_BP_NO_NET=1` schaltet es ab; ohne Katalog fehlt nur die Liste, die Erkennung läuft weiter.


- **Läuft unter Linux.** Eine Codebasis für beide Systeme, keine zweite Version. Wo die Dateien liegen, entscheidet der neue Baustein `scbp/pfade.py`: unter Windows `%APPDATA%` und `C:\Program Files`, unter Linux `~/.config` und das Wine-Präfix (gesucht wird an den Stellen, an denen lug-helper, Lutris, Bottles und Heroic ihre Installationen ablegen). Eigene Wege gehen über `SC_BP_HOME`, `SC_INSTALL_DIR` und `SC_BP_LAUNCHER`.
- **Eigener Bauplan-Bestand** (`bestand.json` im eigenen Ordner). Jeder Fund wird dauerhaft festgehalten, mit Herkunft (Log, Nachlese, Launcher, von Hand). Geschrieben wird über eine Nebendatei und Umbenennen, damit ein Absturz mitten im Speichern nichts zerreißt; die Vorgängerfassung bleibt als `bestand.bak.json` liegen.
- **Nachlese beim Start.** Die aufgehobenen Logs vergangener Sitzungen (`logbackups/`) werden durchgesehen und still in den Bestand übernommen — wer ohne laufenden Watcher gespielt hat, verliert nichts mehr. Beim allerersten Start wird auch die **laufende** Game.log von vorn gelesen, sonst wäre ausgerechnet die gerade laufende Sitzung ein Loch.
- **Ehrlicher Lückenhinweis.** Reichen die vorhandenen Sicherungen nicht bis zum letzten bekannten Stand zurück, sagt der Watcher das als eigene Zeile (ℹ) — statt eine unvollständige Liste als Bestand auszugeben. Dafür gibt es die Liste zum Abhaken.
- **Lesestand übersteht Neustarts** (`logstand.json`). Wer den Watcher neu startet, während Star Citizen läuft, verliert die Baupläne dieser Sitzung nicht mehr.
- **Spracherkennung statt fester deutscher Phrase** (`scbp/phrasen.py`). Gesucht wird nach einer Tabelle deutscher und englischer Formulierungen; liegt eine entpackte `global.ini` vor, wird der Wortlaut daraus exakt übernommen (Schlüssel `crafting_hud_notification_received_blueprint`). Eigene Ergänzungen gehen in `phrasen.json`. Bis v1.5.0 griff die Sofort-Meldung bei englischem Client gar nicht — unter Linux spielen die meisten auf Englisch.
- **Autostart auf beiden Systemen** (`scbp/autostart.py`): unter Windows wie bisher der Registry-Wert, unter Linux eine `.desktop`-Datei in `~/.config/autostart/`.
- **Startskript für Linux** (`SC-BP-Watcher starten.sh`) als Gegenstück zur `.bat` — prüft vorher, ob `tkinter` da ist, und nennt sonst den passenden Paketbefehl je Distribution.
- **Eigene Pfade eintragbar** (`einstellungen.json` im eigenen Ordner). Wer Star Citizen oder den Launcher woanders liegen hat, trägt den Ordner dort ein, statt auf die Suche angewiesen zu sein. Findet der Watcher das Spiel nicht, legt er die Datei beim Start selbst an und nennt sie in der Fehlermeldung. Rangfolge: Umgebungsvariable → Einstellungsdatei → Suche an den üblichen Stellen.
  - **Die durchsuchten Orte werden genannt** — ausgegraut im Fenster und als Zeile direkt unter dem jeweiligen Feld in der Datei. Ohne dieses Vorbild müsste man den einzutragenden Pfad raten; gerade wenn nichts gefunden wurde, hat man ja keinen zum Abschauen. Findet der Rechner keinen einzigen Wine-Präfix, werden trotzdem die typischen Orte gezeigt statt einer leeren Liste.
- **Selbsttest** (`tools/selbsttest.py`). Baut eine Spielinstallation im Wegwerf-Ordner nach und prüft die Erkennung samt ihrer bekannten Fallstricke.

### Behoben

- **Der Watcher wäre unter Linux beim Start abgestürzt.** Der Mauszeiger `size_nw_se` am Größengriff gibt es nur unter Windows; auf anderen Systemen wirft Tk dafür einen Fehler, bevor das Fenster überhaupt erscheint.
- **Fensterlage vom fremden Rechner.** Die gemerkte Position wurde ungeprüft übernommen. Auf einem Rechner mit anderem Monitoraufbau stand das Fenster damit außerhalb jedes Bildschirms — unsichtbar, unter macOS mit Absturz. Sie wird jetzt auf Plausibilität geprüft; die Vorgabe im Code enthält **gar keine Position** mehr, sondern nur noch eine Größe. Wo das Fenster stehen soll, zieht sich jeder selbst hin.
- **Endlosschleife ohne Launcher.** Beim Start wartete der Watcher, bis die Launcher-Datei lesbar war — ohne Launcher also ewig. Unter Linux wäre er nie hochgekommen.
- **Katalog-Wache lief ohne Launcher ins Leere.** „Was ist im Spiel neu craftbar" hing an einer Launcher-Datei. Fehlt sie, treten jetzt die scmdb-Craftdaten an ihre Stelle, die ohnehin schon vorliegen.
- **Signalton ohne `winsound`.** Unter Linux gibt es das Modul nicht; dort klingelt jetzt tkinter selbst.

### Geändert

- **Markenfarbe auf `#9ce430` gezogen.** Das Overlay lief noch mit `#47aa42` — der Xharig-Farbe von vor dem Logo-Wechsel. Betrifft `ACCENT` im Overlay und `GREEN` im Icon-Werkzeug. Für helle Flächen (README-Badges) bleibt es beim Text-Grün `#5fa522`.
- **Die Statuszeile zeigt den eigenen Bestand**, nicht mehr die Launcher-Zahl, und dazu, ob mit oder ohne Launcher gearbeitet wird. Grund: Der Launcher zählt nachweislich zu niedrig — ihm fehlt die P4-AR Rifle, obwohl sie im Fabricator als „im Besitz" steht (gemessen 11.08.2026). Startbaupläne wurden nie „erhalten" und stehen in keinem Log.
- **Der SC Deutsch Launcher ist optional.** Ist er da, wird er weiter genutzt: Er bestätigt die Funde (🟡 → 🟢) und liefert den gepflegten Katalog. Fehlt er, entfällt nur das — gemeldet wird trotzdem, denn die Game.log ist die eigentliche Quelle.
- **Startbedingung.** Der Watcher verlangt beim Start nicht mehr die Launcher-Datei, sondern nur noch, dass Star Citizen selbst gefunden wird.

## v1.5.0 - 2026-08-11

### Hinzugefügt

- **Werte-Rückfall über scmdb.net.** Kennt der Launcher-Katalog einen Gegenstand nicht, holt der Watcher Art, Größe, Gütegrad, Klasse und Hersteller jetzt aus den Craftdaten von scmdb (`versions.json` → `crafting_items-<version>.json`). Damit bekommen auch Baupläne ein Kürzel, die im Katalog fehlen — z. B. **QuadraCell**, **FR-66** und die Skin-Varianten. Reines `urllib` aus der Standardbibliothek, kein Zusatzpaket.
  - Zwischenspeicher: `%APPDATA%\sc-bp-watcher\scmdb-items.json`; neu geholt wird nur bei einer **neuen Spielversion** (Prüfung alle 6 Stunden).
  - Ohne Netz gilt der letzte Stand, ohne Zwischenspeicher läuft alles wie vor v1.5.0 — der Watcher bricht nie deswegen ab.
  - Abschaltbar über die Umgebungsvariable `SC_BP_NO_NET=1`.
- **Mit Windows starten — freiwillig.** Neuer Schalter `⏻` in der Titelleiste (grün = an, grau = aus). Er trägt den Watcher unter `HKCU\…\CurrentVersion\Run` ein bzw. wieder aus. Nichts wird ungefragt aktiviert, und der Zustand steht ausschließlich in der Registry — es gibt keine zweite Wahrheit, die damit auseinanderlaufen könnte.
  - Aus dem Quellcode heraus wird `pythonw.exe` eingetragen, nicht `python.exe`: Sonst stünde bei jedem Anmelden ein Konsolenfenster offen, das im Spiel den Fokus klaut.

- **Neues App-Icon.** Dunkles Rundemblem im Xharig-Grün: segmentierter Scanner-Ring, Blaupausen-Blatt mit Würfel, waagerechter Scanstrahl. Gebaut von `tools/make_icon_from_art.py` aus zwei Vorlagen — einer detaillierten ab 40 Pixel und einer **vereinfachten für 16–32 Pixel** (massiver Würfel statt Drahtgitter, keine Eckklammern). Ein einziges Motiv über alle Größen wäre klein zu Matsch zerfallen.

### Wissenswert

- **Rangfolge der Quellen:** `bp-overrides.json` → Launcher-Katalog/Spieldaten → scmdb. scmdb füllt nur Lücken und überschreibt nie. Grund: Ein Abgleich am 11.08.2026 gegen 56 Meldungen aus der Spiel-Log ergab **55 exakte Treffer** bei Größe, Gütegrad und Klasse — beim Kühler **Elsen** nennt scmdb aber Grad A, während die Spiel-Log *und* `components.ini` übereinstimmend B sagen (auch der Hersteller stimmt dort nicht). Sehr gute Quelle, aber keine unfehlbare.
- **Die scmdb-Daten werden bewusst NICHT mitgeliefert**, sondern auf dem Rechner des Nutzers direkt bei scmdb.net geholt — so wie es ein Browser täte. scmdb steht unter CC BY-NC-ND 4.0; eine mitgelieferte Kopie wäre eine Weitergabe und würde sowohl dieser Lizenz als auch der GPL dieses Projekts widersprechen. Der Abruf trägt eine ehrliche Kennung (`SC-BP-Watcher/<Version>` mit Projektadresse), damit der Betreiber sieht, wer abruft. Dank an scmdb steht in der `README.en.md`.
- **Rüstung und FPS-Waffen bekommen weiterhin kein Kürzel.** scmdb vergibt `size` und `grade` an jeden Gegenstand, auch an Helme — ungefiltert übernommen stünde hinter jedem Rüstungsteil ein erfundenes „Grade A, Size 1". Klasse und Gütegrad werden deshalb nur übernommen, wenn scmdb eine `componentClass` führt (echte Schiffskomponenten); Schiffswaffen bekommen nur die Größe.

## v1.4.0 - 2026-08-02

### Geändert

- **Lizenzwechsel von MIT auf GNU GPL v3.0** (nur Version 3, `SPDX-License-Identifier: GPL-3.0-only`). Der Quellcode wird offengelegt: ein einziges öffentliches Repo statt der geplanten Trennung in privates Quell- und öffentliches Auslieferungs-Repo. Die GPL erlaubt Nutzung und Änderung durch jeden, verlangt bei Weitergabe aber die Offenlegung des Quellcodes unter derselben Lizenz.
- `README.en.md`: neuer Abschnitt **„Star Citizen Fan Content"** mit dem von RSI vorgeschriebenen Wortlaut und dem Link zur offiziellen Seite — Voraussetzung für eine öffentliche Weitergabe.

### Behoben

- **Fest verdrahteter lokaler Pfad entfernt.** `OVERRIDES_FILE` zeigte auf ein Verzeichnis, das es nur auf dem Rechner des Entwicklers gibt — bei allen anderen lief die Datei ins Leere, und mit der Offenlegung wäre der Pfad öffentlich geworden. Die optionale Overrides-Datei wird jetzt unter `%APPDATA%\sc-bp-watcher\bp-overrides.json` gesucht; ein abweichender Ort lässt sich über die Umgebungsvariable `SC_BP_OVERRIDES` angeben. Fehlt beides, gilt der Launcher-Katalog unverändert.

## v1.3.0 - 2026-07-31

### Hinzugefügt

- **Katalog-Wache — meldet, was im Spiel NEU craftbar geworden ist.** Bisher meldete der Watcher nur, was *du* freischaltest. Jetzt behält er zusätzlich `bp_item_types.json` im Auge — die Liste dessen, was überhaupt einen Bauplan hat. Der SC Deutsch Launcher frischt sie mit den Patches auf; kommt etwas dazu, erscheint es als 🔵 **neu im Spiel craftbar**. So bekommt man mit, wenn CIG einen Gegenstand nachreicht, den es vorher schlicht nicht als Bauplan gab.
- **Beobachtungsliste für Wunsch-Gegenstände:** Liegt `%APPDATA%\sc-bp-watcher\watchlist.json`, werden Treffer daraus auffällig in Gold mit ⭐ und eigenem Signalton gemeldet (`<Titel> — jetzt craftbar!`). Format: `{"eintraege": [{"titel": "…", "muster": ["teilstring", …]}]}`, Muster kleingeschrieben, Treffer per Teilstring. Ohne die Datei meldet der Watcher einfach jeden Zuwachs.
- Der Vergleichsstand liegt in `%APPDATA%\sc-bp-watcher\catalog-seen.json` und **überlebt Neustarts** — sonst käme nach jedem Programmstart der halbe Katalog als „neu". Beim allerersten Start wird nur die Basis gesetzt, es wird nichts gemeldet.

### Behoben

- **Breiterziehen brachte nichts:** Die Breite der Liste war mit `312` Pixeln fest verdrahtet (`create_window(..., width=312)`). Wer das Fenster breiter zog, bekam trotzdem denselben schmalen Inhalt — lange Bauplan-Namen blieben abgeschnitten. Die Liste zieht jetzt bei jeder Größenänderung mit; lange Untertitel brechen um, statt am Rand zu verschwinden.
- **Standardgröße** von `341x1098` auf `440x1098` erhöht (rechte Fensterkante bleibt gleich), damit die längeren Meldungen der Katalog-Wache ohne Umbruch passen.

### Hinweise

- Die Katalogdatei wird nur **einmal pro Minute** und auch dann nur bei geändertem Zeitstempel gelesen (`CAT_POLL`) — sie ändert sich ohnehin nur bei Patches.
- Katalog-Zeilen sind reine Meldungen: Sie werden nie auf 🟢 „bestätigt", weil sie nichts mit dem eigenen Freischalt-Stand zu tun haben.
- Der Watcher führt seinen Katalogstand in einer **eigenen** Datei — so nimmt ein zweites Werkzeug auf denselben Daten ihm nicht die Meldung weg.

## v1.2.0 - 2026-07-30

### Hinzugefügt

- **Sofort-Meldung aus der `Game.log`:** Der Watcher liest die Star-Citizen-Log jetzt zusätzlich selbst mit und zeigt einen neuen Bauplan **in Sekunden** an, statt auf den Export des Launchers zu warten. Hintergrund: Der SC Deutsch Launcher schreibt `sc_bp_erledigt.json` nur alle paar Minuten neu — gemessen am 30.07.2026 lagen zwischen Freischaltung im Spiel (21:23:49) und Launcher-Export (21:26:24) **2,5 Minuten**. Genau diese Lücke schließt die Log-Mitlesung.
- **Zwei-Stufen-Anzeige:** Frisch aus der Log gemeldete Baupläne stehen als 🟡 **vorläufig** in der Liste; sobald der Launcher nachzieht, wird die Zeile auf 🟢 bestätigt und mit dessen Daten aufgefrischt. Die Launcher-Datei bleibt die verbindliche Quelle — Art, Size/Grade/Klasse kommen weiterhin aus dem Launcher-Katalog.
- **Namens-Abgleich Log ↔ Launcher:** Schiffskomponenten stehen im Log mit Zusatz (`7CA 'Nargun' (Civ/3/A)`), beim Launcher ohne — der Zusatz wird abgeschnitten (und dient als Rückfall fürs `M/A/1`-Kürzel, falls ein Item nach einem SC-Patch noch nicht im Katalog steht). Echte Namens-Klammern wie `(30 cap)` oder `Singe Cannon (S2)` bleiben unangetastet. Weichen die Übersetzungen ab (gesehen: `(12 Schuss)` im Log vs. `(12 cap)` beim Launcher), greift ein Notfall-Abgleich ohne Klammer-Zusatz — aber nur, wenn er eindeutig ist. Geprüft gegen alle 127 vorhandenen Log-Backups: 148 Bauplan-Meldungen, 147 exakte Treffer, der eine Rest über den Notfall-Abgleich.
- **Automatische Log-Findung** über den `Installfolder` aus `scdl-settings.json`, ersatzweise über den Lesestand des Launchers (`scan-state.json`) oder den Standard-Installationspfad. Spiel-Neustart (rotierte Log) wird erkannt.
- **Statuszeile** zeigt jetzt auch, ob die Log mitgelesen wird: `Überwache 377 BPs · Log ✓ · geprüft 21:26:27`.

### Behoben

- **„Neueste oben" hat nie funktioniert:** Neue Zeilen wurden per `winfo_children()` einsortiert — das ist die Reihenfolge der *Erzeugung*, nicht die im Fenster. Dadurch landete jeder Neuzugang ab dem dritten **unter** den älteren. Jetzt wird `pack_slaves()` genutzt.
- **`MAX_ROWS` war wirkungslos:** Die Einstellung stand in der README, wurde im Code aber nie angewendet — die Liste wuchs unbegrenzt. Jetzt fliegen die ältesten Zeilen über `MAX_ROWS` (Standard 200) raus.
- **Art-Nachschlag frischt sich auf:** Ist ein gerade freigeschaltetes Item noch nicht in `bp_item_types.json`, wird die Datei einmal neu geladen, statt sofort `—` anzuzeigen.

### Hinweise

- Die Log-Mitlesung erkennt die **deutsche** Spielmeldung (`Bauplan erhalten: <Name>: `). Bei anderer Spielsprache greift sie nicht — dann verhält sich das Tool wie bisher (Meldung, sobald der Launcher exportiert hat). Weitere Sprachen lassen sich in `LOG_PHRASES` ergänzen.
- Weiterhin nur lesend: die `Game.log` wird ausschließlich gelesen, nie verändert.

## v1.1.0 - 2026-07-19

### Hinzugefügt

- **Size / Grade / Klasse je Bauplan** als Kompakt-Kürzel `Klasse/Grade/Size`, z. B. `M/A/1` (Military · Grade A · Size 1). Kürzel: **M** Military, **S** Stealth, **I** Industrial, **C** Civilian, **K** Competition. Schiffswaffen haben nur Size → `–/–/2`; FPS-Waffen und Rüstung haben nichts davon → kein Kürzel. Datenbasis: Launcher-Katalog `catalog\components.ini` + `items_raw.ini`, plus manuelle Korrekturen aus `bp-overrides.json` (Vorrang).
- **Fenster merkt sich Position & Größe:** beim Verschieben, Skalieren und Beenden wird die Lage in `%APPDATA%\sc-bp-watcher\watcher.json` gespeichert und beim nächsten Start wiederhergestellt.

### Geändert

- **Standard-Startposition** ist jetzt der obere Monitor (nicht der Spiel-Monitor) → man tabbt nicht mehr versehentlich aus Star Citizen. Wird über `DEFAULT_GEOM` gesetzt (nur beim allerersten Start relevant, danach greift die gemerkte Position).

## v1.0.3 - 2026-06-29

### Hinzugefügt

- **GitHub-Release** mit der fertigen `SC-BP-Watcher.exe` als Anhang — herunterladen, Doppelklick, läuft (kein Python, kein Selbst-Bauen nötig)

### Geändert

- README: „Fertige `.exe` herunterladen" ist jetzt die **empfohlene** Start-Variante (A); Python (B) und Selbst-Bauen (C) dahinter

## v1.0.2 - 2026-06-29

### Hinzugefügt

- **App-Icon** im Xharig-Stil (dunkler Grund, Xharig-Grün, Scope-Ring mit „neu"-Punkt) — `icon.ico` für die EXE, `assets/icon.png` als Vorschau
- EXE wird jetzt mit dem Icon gebaut (`EXE bauen.bat` → `--icon`)
- Fenster-/Taskleisten-Icon wird auch beim Start als Skript gesetzt (falls `icon.ico` daneben liegt)
- Icon-Generator `make_icon.py` (reproduzierbar; braucht nur Pillow, nicht fürs Tool selbst)

## v1.0.1 - 2026-06-29

### Hinzugefügt

- **Danksagung & Credits** an den SC Deutsch Launcher (Datenquelle des Tools) inkl. Hinweis, dass SC BP Watcher ein eigenständiges, inoffizielles Zusatz-Tool ist
- Offizieller Link zum **[SC Deutsch Launcher](https://www.sc-deutsch-launcher.de/)** im Pflicht-Hinweis und in den Credits

### Geändert

- Pflicht-Voraussetzung (SC Deutsch Launcher) prominent ganz oben in der README hervorgehoben

## v1.0.0 - 2026-06-29

Erstveröffentlichung.

### Hinzugefügt

- Live-Overlay (randlos, immer im Vordergrund, durchscheinend), das neue Star-Citizen-Baupläne in Echtzeit anzeigt
- Hintergrund-Überwachung von `sc_bp_erledigt.json` (Prüf-Intervall 3 s, eigener Thread)
- Anzeige je Neuzugang: 🟢 Name · Art · Uhrzeit, neueste oben
- Signalton bei jedem neuen Bauplan
- Fenster verschiebbar (Titelleiste) und skalierbar (Griff ◢), Liste leeren (🗑), schließen (✕)
- Art-Anzeige zweisprachig — übernimmt den Wert direkt aus `bp_item_types.json` (deutsch oder englisch)
- Automatische Pfad-Findung über `%APPDATA%`
- Start per `SC-BP-Watcher starten.bat` (ohne Konsolenfenster) oder als eigenständige `.exe` via `EXE bauen.bat`

### Hinweise

- Reines Python-Standardbibliothek-Tool (`tkinter`) — keine Zusatzpakete nötig
- Nur lesend: verändert oder sendet keine Daten

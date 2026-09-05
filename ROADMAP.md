# Roadmap

**Deutsch** · [English](ROADMAP.en.md)

## Zielbild

SC BP Watcher ist ein kleines Overlay, das beim Spielen von Star Citizen live anzeigt, sobald ein neuer Bauplan freigeschaltet wird — unter **Windows und Linux**, aus einer gemeinsamen Codebasis.

Seit v3.3.0 kommt eine **Werkstatt** dazu: Was du aus einem Bauplan herstellen kannst, was dafür fehlt, wo die Rohstoffe liegen und was in deinem Lager steht. Der Bauplan bleibt der Ausgangspunkt — die Werkstatt beantwortet die Frage, die danach kommt: *und jetzt?*

Vier Dinge sind Absicht und bleiben so:

- **Leichtgewichtig.** Reine Python-Standardbibliothek, keine Zusatzpakete. Was keine Abhängigkeit hat, kann auch keine verlieren.
- **Es gehört dir.** Kein Konto, keine Anmeldung, keine Cloud. Was das Werkzeug weiß, steht in Dateien auf deiner Platte.
- **Es liest, es schreibt nicht.** An der Spielinstallation wird nichts verändert.
- **Ehrlich statt hübsch.** Wenn etwas fehlen könnte, sagt es das — lieber eine unbequeme Auskunft als eine schöne Zahl, auf die kein Verlass ist.

## Was es kann

| | Inhalt |
|---|---|
| ✅ | Live-Erkennung neuer Baupläne aus der Spiel-Log, Anzeige im Overlay |
| ✅ | **Eigener Bauplan-Bestand** — der SC Deutsch Launcher ist nicht nötig |
| ✅ | **Nachlese**: beim Start werden frühere Spielsitzungen ausgewertet |
| ✅ | **Bauplan-Liste** zum Nachschlagen, Filtern und Abhaken, mit Fortschritt |
| ✅ | **Herkunft je Bauplan** — Fraktion, Auftrag, nötiger Ruf, Belohnung; aus der Herstellung führt ein Knopf direkt hin |
| ✅ | **Beim Annehmen eines Auftrags**: bringt er Baupläne, und welche fehlen dir noch? |
| ✅ | **Was gerade zu tun ist** — die offenen Zwischenziele stehen unter ihrem Auftrag |
| ✅ | **Auftrags-Protokoll** — welche Aufträge wann gespielt wurden, wie oft, und welcher Bauplan dabei herauskam |
| ✅ | **Sicherung** — alles Eigene in eine Datei und wieder zurück, für den Rechnerwechsel |
| ✅ | **Tastenkombination** — holt die Bauplan-Liste aus dem laufenden Spiel nach vorn (Windows und Linux/X11) |
| ✅ | **Overlay in eine Bildschirmecke legen** — nötig im Pop-up-Betrieb, wo es sich nicht ziehen lässt; eingeklappt schrumpft es auf Streifenbreite |
| ✅ | Katalog-Wache: meldet, was im Spiel **neu craftbar** wird, dazu eine Merkliste |
| ✅ | Filter **neu im Spiel** und Auswahlfeld **Patch**: nachschlagen, was jeder Patch gebracht hat |
| ✅ | **Serverstatus**: eigener Reiter mit der Lage von CIG, frischt sich selbst auf |
| ✅ | Kürzel für Klasse, Größe und Gütegrad (`M/1/A`) |
| ✅ | Einrichtungsassistent, jederzeit wiederholbar |
| ✅ | Deutsch und Englisch, umschaltbar |
| ✅ | Windows und Linux, mit Autostart auf beiden |
| ✅ | Bestand ausgeben — für das KRT Profit Basetool, für scmdb.net und als vollständige Sicherung |
| ✅ | Overlay einklappen, für alle mit einem Bildschirm |
| ✅ | **Ablage-Symbol**: neben der Uhr unter Windows, im Startmenü unter Linux — der Weg zurück zu Liste und Einstellungen, während sich das Overlay zurückhält |
| ✅ | **Angaben am Gegenstand im Spiel** — Klasse, Größe und Gütegrad am Traktorstrahl, bei Raketen der Suchkopf |
| ✅ | **Herstellung**: zu jedem herstellbaren Gegenstand die Zutaten, die Dauer und die Werte — samt der Frage, ob du den Bauplan dafür hast |
| ✅ | **Materialqualität wirkt sich aus** — ein Regler je Zutat zeigt, was mit *deinem* Material herauskäme, und in welcher Spanne der Wert überhaupt liegen kann |
| ✅ | **Mein Lager**: Material, Menge, Qualität und Lagerort eintragen; im Rezept steht dann, was fehlt, und ein Knopf zieht die Zutaten ab |
| ✅ | **Bergbau** in beide Richtungen: Rohstoff → Fundorte, Ort → was es dort gibt, mit Abbauart, Raffinerie-Vergleich und Scan-Signatur |
| ✅ | **Verarbeitungsmethode empfehlen** — sag, ob dir Ertrag, Kosten oder Geschwindigkeit wichtiger ist, und du bekommst eine der neun Methoden genannt |
| ✅ | **Preise** von UEX Corp — was ein Rohstoff kostet und was er einbringt, damit „was fehlt mir" auch „was kostet mich das" beantwortet |
| ✅ | **Handelslager**: was zum Verkauf im Laderaum liegt — getrennt vom Werkstatt-Lager, mit Kennzeichen für als gestohlen markierte Ladung |
| ✅ | **Beide Lager sichern und zurückholen** (.json), als Tabelle ausgeben (.csv) und nach einem Patch-Wisch in einem Zug leeren |
| ✅ | Das Fenster **behält die Größe**, die du eingestellt hast |
| ✅ | **Verkauf**: wo du deine Ware los wirst und was sie je SCU bringt — für mehrere Waren auf einmal, sortiert danach, wie viele ein Ort abnimmt, mit Ampel für Orte, die schon voll sind |
| ✅ | **Läden**: wo ein fertiges Teil im Regal steht und was es dort kostet — die Gegenprobe zu „lohnt Selberbauen?" |
| ✅ | **Alles Kaufbare, nicht nur Craftbares**: 1.528 Teile aus 38 Warengruppen, dazu 174 Schiffe zum Kaufen und Mieten |
| ✅ | **Klasse, Größe, Güte und Hersteller** an jeder Zeile — und als Auswahlmenü, damit man das passende Teil auch ohne Namenskenntnis findet |
| ✅ | **Rohstofflager** heißt, was es ist — passend zum Handelslager daneben |
| ✅ | **Routen**: Handelsrouten mit Einkauf, Verkauf und echtem Gewinn — über mehrere Stationen, als Rundreise, oder die beste Route im ganzen Verse |
| ✅ | **Schiffsdaten**: Frachtraum, Kaufpreis und Mietpreis — im Routenplaner wählst du dein Schiff, der Laderaum kommt von selbst |
| ✅ | **Belegungen mitsichern**: Tastatur- und Joystick-Belegung gehen mit in die Sicherung und lassen sich als benanntes Profil dort ablegen, wo Star Citizen sie findet |

## Woran gearbeitet wird

Ohne Zeitplan und ohne feste Reihenfolge — Stand der Dinge steht im [`CHANGELOG.md`](CHANGELOG.md), und im Fenster „Was ist neu" lässt sich nachlesen, was jede Version gebracht hat.

Was als Nächstes kommt, ergibt sich aus den Rückmeldungen. Das Werkzeug ist täglich im Einsatz, und die meisten Änderungen haben als Nachricht von jemandem angefangen.

## Verhältnis zum SC Deutsch Launcher

**Wahlfreiheit, nicht Ersatz.**

Hier stand lange, ein selbst geführter Bestand sei zwangsläufig ungenau, weil er nur „ab heute" fortgeschrieben werden könne. Zwei Messungen haben das widerlegt:

- Der Watcher liest beim Start die **aufgehobenen Logs** nach. Ohne laufenden Watcher gespielt zu haben, reißt also kein Loch, solange Star Citizen die Sicherung noch hat. Bleibt doch eine Lücke, wird sie **gesagt** statt verschwiegen.
- Der Launcher selbst zählt **zu niedrig**: Ihm fehlt die P4-AR Rifle, obwohl sie im Fabricator als „im Besitz" steht. Startbaupläne wurden nie „erhalten" und stehen in keinem Log. Seine Zahl ist eine Untergrenze, kein Bestand.

Der Launcher bleibt trotzdem nützlich: Er bestätigt Funde und pflegt einen Katalog mit deutschen Bezeichnungen. Ist er da, wird er genutzt. Ist er nicht da — unter Linux immer —, läuft der Watcher trotzdem.

## Mitreden

Wünsche, Fehlermeldungen und Ideen gern als [Issue](../../issues).

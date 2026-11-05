<div align="center">

<img src="assets/icon.png" alt="VerseKit Symbol" width="128">

# VerseKit

**Dein Werkzeugkasten für Star Citizen: Baupläne, Aufträge, Herstellung, Handel und mehr, live beim Spielen**

<sub>Windows · Linux · kostenlos · ohne Konto, ohne Cloud</sub>

<sub>ehemals **SC BP Watcher** · dein Bestand und deine Einstellungen bleiben erhalten</sub>

[![Version](https://img.shields.io/github/v/release/Xharig/VerseKit?include_prereleases&label=Version&color=9ce430&labelColor=161c28)](https://github.com/Xharig/VerseKit/releases)
[![Heruntergeladen](https://img.shields.io/endpoint?url=https%3A%2F%2Fraw.githubusercontent.com%2FXharig%2FVerseKit%2Fzaehler%2Fdownloads.json&label=Heruntergeladen&color=9ce430&labelColor=161c28)](https://github.com/Xharig/VerseKit/releases)
[![Webseite](https://img.shields.io/badge/Webseite-Rundgang%20ansehen-9ce430?labelColor=161c28)](https://versekit.xharig.com/)
[![Lizenz](https://img.shields.io/badge/Lizenz-GPL--3.0-9ce430?labelColor=161c28)](LICENSE)
[![Discord](https://img.shields.io/badge/Discord-beitreten-9ce430?labelColor=161c28&logo=discord&logoColor=9ce430)](https://xharig.com/discord)
[![Ko-fi](https://img.shields.io/badge/Ko--fi-Kaffee%20spendieren-9ce430?labelColor=161c28&logo=kofi&logoColor=9ce430)](https://ko-fi.com/xharig)
<br>
[![Rust](https://img.shields.io/badge/Rust-1.99-34415a?labelColor=161c28&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![System](https://img.shields.io/badge/System-Windows%20%C2%B7%20Linux-34415a?labelColor=161c28)](#herunterladen)
[![Star Citizen](https://img.shields.io/badge/Star%20Citizen-kompatibel-34415a?labelColor=161c28)](https://robertsspaceindustries.com/enlist?referral=STAR-YC7L-S4W2)

**Deutsch** · [English](https://github.com/Xharig/VerseKit/blob/main/README.en.md)

</div>

---

<img src="assets/readme-kopf.png" alt="Die VerseKit-Webseite am PC und am Handy" width="100%">

VerseKit liegt als schmales Overlay über Star Citizen und meldet jeden neuen Bauplan in Sekunden. Im Fenster dahinter steht, was danach kommt: woher dir Fehlendes kommt, was du zum Herstellen brauchst, wo du Erz findest und wo du deine Ladung loswirst.

## Was VerseKit kann

- **Baupläne live:** Jeder neue Bauplan steht in Sekunden im Overlay. Deinen bisherigen Bestand liest VerseKit beim ersten Start aus den Spielprotokollen nach.
- **Woher du Fehlendes bekommst:** Zu jedem Bauplan, den es über einen Auftrag gibt, Fraktion, Auftrag, Rang und Belohnung, der leichteste Weg zuerst.
- **Angaben im Spiel:** Die Auftragstexte zeigen, welche Baupläne ein Auftrag bringt und welche du schon hast; an jedem herstellbaren Gegenstand im Terminal steht, ob du den Bauplan hast, woher er kommt und was das Herstellen braucht.
- **Herstellen und Lager:** Zutaten, Herstellzeit und was dein Material aus den Werten macht, dazu dein Lager, was dir noch fehlt und deine Käufe im Laden als Vorschlag aus dem Log.
- **Bergbau und Raffinerie:** Fundorte je Rohstoff, die beste Raffinerie und die passende Verarbeitungsmethode; unter Windows liest der Scanner die Signatur beim Scannen selbst ab und sagt dir im Overlay, welches Erz es ist.
- **Handel:** Shops, die besten Ankäufer und Handelsrouten mit echtem Gewinn.
- **Dein Hangar:** Schiffe aus dem Pledge-Store holen und sehen, welches Teil in welches Schiff passt.
- **Handle-Suche:** Wer steckt hinter einem Handle? Name, Organisationen und Rang, dazu Freund und Feind markieren, direkt aus der Info-Leiste über dem Spiel.
- **Spiel-Einstellungen von außen:** Steuerung, Grafik, Achsen, Sichtfeld und Shader-Cache, ohne ins Spiel zu wechseln.

## Herunterladen

| System | Datei | |
|---|---|---|
| **Windows** | [`VerseKit-Setup.exe`](https://github.com/Xharig/VerseKit/releases/latest) | Installer mit Startmenü-Eintrag, sauber deinstallierbar |
| **Linux** | [`VerseKit-x86_64.AppImage`](https://github.com/Xharig/VerseKit/releases/latest) | Eine Datei, einmal ausführbar machen |

Starten, und ein Assistent richtet in etwa einer Minute alles ein. VerseKit ist kostenlos und quelloffen, braucht kein Konto und hält sich selbst aktuell: Es sieht alle zehn Minuten nach und spielt ein neue Version ein, sobald Star Citizen geschlossen ist, ohne Schalter. Die Warnung von Windows beim ersten Start ist kein Virenfund, mehr dazu unten unter *Installation im Einzelnen*.

**[Rundgang auf der Webseite](https://versekit.xharig.com/)** · **[Discord](https://xharig.com/discord)** · **[Fehler oder Wunsch melden](https://github.com/Xharig/VerseKit/issues)** · **[Was ist neu](https://github.com/Xharig/VerseKit/blob/main/CHANGELOG.md)**

## Einzelheiten

Alles Weitere zum Aufklappen. Technisches wie Dateien, Einstellungen und Umgebungsvariablen steht im **[Handbuch](MANUAL.md)**.

### Bilder

<details>
<summary><b>Overlay und Bauplan-Liste</b></summary>

<table>
<tr>
<td width="32%" valign="top" align="center">
<img src="assets/screenshot-overlay.png" alt="Das Overlay beim Spielen" width="100%"><br>
<sub>Das Overlay: schmal, liegt über dem Spiel, Durchsichtigkeit einstellbar</sub>
</td>
<td width="68%" valign="top" align="center">
<img src="assets/screenshot-liste.png" alt="Das Fenster mit der Bauplan-Liste" width="100%"><br>
<sub>Die Bauplan-Liste: Suche, fünf Filter und die Herkunft je Bauplan</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Im Spiel, ohne herauszutabben</b></summary>

VerseKit schreibt in die Auftragstexte des Spiels, **welche** Baupläne ein Auftrag ausschüttet, mit `[x]` für das, was du schon hast. Im Titel steht `[BP]` (wo alle Stufen dieselbe Rufschwelle haben, gleich mit dem nötigen Ruf: `[BP 10.000 Rep]`), die Namen stehen in der Beschreibung, getrennt nach Stanton, Pyro und Nyx, wo ein Auftrag dort verschiedene Baupläne hergibt.

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-teils.jpg" alt="Auftrag mit teilweise vorhandenen Bauplänen" width="100%"><br>
<sub><b>3 von 6</b>: <code>[x]</code> hast du, <code>[&nbsp;&nbsp;]</code> fehlt noch</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-keine.jpg" alt="Auftrag ohne vorhandene Baupläne" width="100%"><br>
<sub><b>0 von 12</b>: hier ist noch nichts dabei, was du hast</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Meine Schiffe</b></summary>

Ein neuer Bauplan wirft sofort die nächste Frage auf: **passt das Teil überhaupt in eines meiner Schiffe?** Dafür muss das Werkzeug wissen, was in deinem Hangar steht. Das Spiel schreibt es nirgends auf.

> **So kommt dein Hangar ins Werkzeug:** Die Browser-Erweiterung **Star Citizen: Hangar Extension** von AlyxOne setzt auf deiner Pledge-Seite Export-Knöpfe für [Firefox](https://addons.mozilla.org/de/firefox/addon/star-citizen-hangar-extension/), [Chrome](https://chromewebstore.google.com/detail/star-citizen-hangar-exten/jeoidbfcbdplammndhpflojejmpkjcfm) und [Edge](https://microsoftedge.microsoft.com/addons/detail/star-citizen-hangar-exte/agahppbhmogalnjlaolmjbbemlcjbioe). Lade dort **beide** Dateien herunter und wähle sie unter **Meine Schiffe → „Aus dem Pledge-Store holen"** nacheinander aus: Die JSON bringt Paketzugehörigkeit und Versicherung, die CSV dazu Kaufdatum, Preis und ob ein Pledge schmelzbar ist. Doppelt wird nichts. Im Spiel gekaufte Schiffe kennt kein Export; die trägst du von Hand ein.

**Flottenanalyse:** Über der Schiffsliste klappt auf, welche Spielbereiche deine Schiffe abdecken, geordnet nach Kampf, Versorgung und Reisen, wo eine Lücke ist und welche Snubs ohne ihr Mutterschiff dastehen. An jedem Echtgeld-Schiff steht, ob es sich schmelzen oder per CCU upgraden lässt. Die Spielbereiche je Schiff stammen aus dem [Fleetmanifest](https://github.com/MaluhTV/SC_Fleetmanifest) von Maluh: eine Einschätzung, keine Spieldaten.

<table>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-hangar.png" alt="Mein Hangar mit vier eingetragenen Schiffen" width="100%"><br>
<sub><b>Mein Hangar</b>: in einem Zug aus dem Pledge-Store geholt oder von Hand eingetragen; danach steht in der Herstellung, in welche deiner Schiffe ein Bauplan passt</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-asop-ingame.jpg" alt="Der Flottenmanager im Spiel mit drei selbst benannten Schiffen" width="100%"><br>
<sub><b>Namen im Spiel</b>: so sieht es <b>im Spiel</b> aus: eigene Namen im Abrufterminal statt der Werksnamen, vergeben direkt im Hangar. Der Stern markiert nicht nur: Weil der Flottenmanager alphabetisch sortiert, rutschen die markierten Schiffe nach oben</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-einkaufsliste.png" alt="Einkaufsliste: Rechnung über alle Schiffe" width="100%"><br>
<sub><b>Einkaufsliste</b>: die Rechnung über alle Schiffe: kaufen oder selbst bauen je Posten, mit Summe und Einkaufsroute</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-bergung.png" alt="Was steckt in einem Wrack" width="100%"><br>
<sub><b>Was steckt drin?</b>: was ein Wrack ab Werk an Bord hat und was die Teile im Laden wert sind</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Die Werkstatt</b></summary>

Der Bauplan ist der Anfang. Die Werkstatt beantwortet, was danach kommt: **was brauche ich, habe ich das, und was wird daraus?**

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-herstellung.png" alt="Herstellung mit aufgeklapptem Rezept" width="100%"><br>
<sub><b>Herstellung</b>: Zutaten, Herstellzeit und was <i>dein</i> Material aus den Werten macht</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-meinlager.png" alt="Mein Lager mit eingetragenen Rohstoffen" width="100%"><br>
<sub><b>Mein Lager</b>: Rohstoffe, Verkaufsware und Items an einer Stelle: Art, Ort, Qualität und Menge, von Hand gepflegt</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-bergbau.png" alt="Bergbau mit den Fundorten von Iron" width="100%"><br>
<sub><b>Bergbau</b>: Rohstoff eintippen und sehen, wo er liegt; oder den Scan-Wert eingeben und sehen, was der Scanner da gefunden hat</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-farmliste.png" alt="Was ich farmen muss" width="100%"><br>
<sub><b>Was ich farmen muss</b>: welche Rohstoffe für alles Geplante noch fehlen, samt Fundorten</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-raffinerien.png" alt="Raffinerien" width="100%"><br>
<sub><b>Raffinerien</b>: was jede Raffinerie aus deinem Erz macht: Aufschlag und Abschlag je Material</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Handel & Einkauf</b></summary>

Kaufen und verkaufen: **Wo steht ein Teil oder Schiff im Regal, und wo werde ich die Ladung los, und was bringt sie je SCU?**

<table>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-laeden.png" alt="Shops" width="100%"><br>
<sub><b>Shops</b>: wo ein fertiges Teil im Regal steht und was es dort kostet</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-meinlager-verkauf.png" alt="Mein Lager, gefiltert auf Ware zum Verkaufen" width="100%"><br>
<sub><b>Mein Lager, zum Verkaufen</b>: was im Laderaum liegt, mit Ankaufspreis und Gewinn</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-verkauf.png" alt="Kaufen & Verkaufen mit den besten Ankaufsorten" width="100%"><br>
<sub><b>Kaufen & Verkaufen</b>: die besten Ankäufer, sortiert danach, wie viele deiner Waren ein Ort abnimmt; umgeschaltet auf Kaufen die günstigsten Einkaufsorte</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-routen.png" alt="Handelsrouten" width="100%"><br>
<sub><b>Routen</b>: sagen, wo du stehst und was in den Laderaum passt; die lohnendsten Fahrten von dort stehen oben</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Das Fenster</b></summary>

> [!NOTE]
> Die Bilder werden vor jeder Veröffentlichung neu erzeugt und zeigen diese Version. Wer eine ältere benutzt, findet manches hier Gezeigte noch nicht.

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-statistik.png" alt="Statistik" width="100%"><br>
<sub><b>Statistik</b>: Spielzeit, Sitzungen, Aufträge, Quantensprünge, Tode und wann du spielst</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-darstellung.png" alt="Darstellung" width="100%"><br>
<sub><b>Darstellung</b>: Farbschema mit Vorschau, Sprache, Größe der Oberfläche</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-fortschritt.png" alt="Fortschritt nach Bereichen" width="100%"><br>
<sub><b>Fortschritt</b>: je Bereich, Einzelheiten auf Klick</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-auftragstexte.png" alt="Einstellungen für die Auftragstexte" width="100%"><br>
<sub><b>Spiel</b>: Textquelle wählen, ein- und ausschalten, die user.cfg ansehen</sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-bestand.png" alt="Sichern und zurücksetzen" width="100%"><br>
<sub><b>Sichern & Zurücksetzen</b>: Sicherung, Bestand fürs Basetool ausgeben oder einlesen</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-anzeige.png" alt="Overlay-Einstellungen" width="100%"><br>
<sub><b>Overlay</b>: Aufblend-Betrieb, Klicks durchreichen, Durchsichtigkeit</sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-ueber.png" alt="Über und Update-Kanal" width="100%"><br>
<sub><b>Über</b>: stabile Version oder Testversionen, mit Knopf zum Holen</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-wasistneu.png" alt="Was ist neu" width="100%"><br>
<sub><b>Was ist neu</b>: jede Version aufklappbar, gefiltert nach Art</sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-danke.png" alt="Danke und Lizenzen" width="100%"><br>
<sub><b>Danke &amp; Lizenzen</b>: wem was gehört, und wer mitgeholfen hat</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-serverstatus.png" alt="Serverstatus" width="100%"><br>
<sub><b>Serverstatus</b>: läuft Star Citizen gerade?</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-auftragslog.png" alt="Aufträge und Protokoll" width="100%"><br>
<sub><b>Aufträge &amp; Protokoll</b>: welchen Auftrag du wann gespielt hast, auch wenn das Spiel die Protokolle längst gelöscht hat</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-patchaenderungen.png" alt="Geänderte Spielwerte" width="100%"><br>
<sub><b>Geänderte Spielwerte</b>: was ein Patch an Schiffen, Waffen und Komponenten verändert hat</sub>
</td>
</tr>
</table>

<details>
<summary>Und der Rest: Allgemein und FOV</summary>

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-allgemein.png" alt="Allgemeine Einstellungen" width="100%"><br>
<sub><b>Allgemein</b>: Sprache, Signalton, Autostart, Startmenü-Eintrag</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-blickwinkel.png" alt="Blickwinkel" width="100%"><br>
<sub><b>FOV</b>: das Sichtfeld passend zu deinem Monitor, gerechnet statt geraten</sub>
</td>
</tr>
</table>

</details>

</details>

<details>
<summary><b>Die Farbschemata</b></summary>

Unter **Einstellungen → Darstellung** wählst du, wie VerseKit aussieht: das Overlay und alle Fenster. Die Wahl gilt sofort, ohne Neustart.

<table>
<tr>
<td width="33%" valign="top" align="center">
<img src="assets/screenshot-farbschema-original.png" alt="VerseKit (Original)" width="100%"><br>
<sub><b>VerseKit (Original)</b></sub>
</td>
<td width="33%" valign="top" align="center">
<img src="assets/screenshot-farbschema-krt.png" alt="KRT (Orange)" width="100%"><br>
<sub><b>KRT (Orange)</b>: eckig, mit Eckwinkeln</sub>
</td>
<td width="33%" valign="top" align="center">
<img src="assets/screenshot-farbschema-eis.png" alt="Eis (Cyan)" width="100%"><br>
<sub><b>Eis (Cyan)</b></sub>
</td>
</tr>
<tr>
<td valign="top" align="center">
<img src="assets/screenshot-farbschema-nebel.png" alt="Nebel (Violett)" width="100%"><br>
<sub><b>Nebel (Violett)</b></sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-farbschema-glut.png" alt="Glut (Rot)" width="100%"><br>
<sub><b>Glut (Rot)</b>: Warnungen in Gelb-Orange</sub>
</td>
<td valign="top" align="center">
<img src="assets/screenshot-farbschema-kontrast.png" alt="Hoher Kontrast" width="100%"><br>
<sub><b>Hoher Kontrast</b>: Weiß und Gelb auf Schwarz</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Was Totzone, Sättigung und Empfindlichkeit bewirken</b></summary>

Unter **Spiel-Einstellungen → Achsen & Kurven** lässt sich einstellen, wie scharf jede Stick-Achse reagiert. Drei Werte, und jeder verbiegt die Kurve anders:

<img src="assets/erklaerung-kurve.png" alt="Vier Kurven: ohne Einstellung, mit Totzone, mit Sättigung, mit Empfindlichkeit" width="100%">

| Wert | Was er macht | Wofür |
|---|---|---|
| **Totzone** | Der Anfang des Wegs tut nichts | Gegen zitternde Sticks, die in der Mitte nicht ruhig stehen |
| **Sättigung** | Vollausschlag schon vor dem Anschlag | Wenn der Stick mechanisch nicht ganz durchgeht oder volle Wirkung früher kommen soll |
| **Empfindlichkeit** | Über 1 hängt die Kurve durch, unter 1 wölbt sie sich | Über 1: feineres Zielen um die Mitte. Unter 1: direkter, giftiger |

Die gestrichelte Linie zeigt, wie es ohne jede Einstellung liefe. Die dunklen Flächen sind der Weg, auf dem sich nichts mehr ändert.

> [!NOTE]
> Star Citizen merkt sich diese Werte an einer internen Nummer des Geräts, nicht an seinem Namen. Bekommt ein Stick eine neue Nummer (anderer USB-Anschluss, neue Firmware), hält das Spiel ihn für ein neues Gerät, und die alten Werte bleiben wirkungslos in der Datei stehen. VerseKit findet solche Altlasten und räumt sie auf Wunsch weg.

</details>

### Mehr wissen

<details>
<summary><b>Warum VerseKit</b></summary>

Bauplan-Listen gibt es mehrere. Vier Dinge machen den Unterschied im Alltag:

- **Du musst nicht aus dem Spiel.** Das Overlay liegt über Star Citizen. Kein zweites Fenster, kein Alt-Tab, kein Nachschlagen im Browser. Der neue Bauplan steht einfach da, während du weiterspielst.
- **Es weiß, was du schon hast.** VerseKit führt deinen Bauplan-Bestand selbst und liest beim ersten Start die aufgehobenen Spielprotokolle nach: Du bekommst deinen bisherigen Stand geschenkt, ohne etwas einzutippen. Bleibt trotzdem eine Lücke, sagt es das, statt eine unvollständige Liste als vollständig auszugeben.
- **Es sagt dir, woher du das Fehlende bekommst.** Zu jedem Bauplan, den es über einen Auftrag gibt, steht dabei, welche Fraktion ihn auslobt, in welchem Auftrag, ab welchem Rang und was er einbringt, sortiert nach dem leichtesten Weg. „Mir fehlt X" ist die halbe Information; die ganze ist „X gibt es bei Foxwell ab Veteran Contractor".
- **Es meldet auch, was du noch gar nicht haben kannst.** Die Katalog-Wache erkennt, wenn CIG mit einem Patch etwas **neu craftbar** macht (unabhängig von deinem eigenen Freischalt-Stand). Solche Zeilen sind blau. Wer auf ein bestimmtes Teil wartet, trägt es in die Beobachtungsliste ein und wird beim Auftauchen auffällig darauf gestoßen.
- **Kein Konto, keine Cloud.** Das Werkzeug liest Dateien, die ohnehin auf deiner Platte liegen, und schreibt nur ins Spiel, was du ausdrücklich einschaltest. Was es aus dem Netz holt und was es sendet, steht unter *Deine Daten und das Netz*.

Dazu: Klasse, Größe und Gütegrad stehen direkt in der Zeile (`M/1/A`), die Oberfläche gibt es auf Deutsch und Englisch, und das Ganze ist ein einziges Programm in Rust: eine Datei je System, nichts nachzuinstallieren.

</details>

<details>
<summary><b>Alle Funktionen</b></summary>

| | |
|---|---|
| <img src="assets/symbole/22/karten-gruen.png" width="22" alt=""> **Übersicht** | Die Startseite zeigt jeden Bereich als Kachel. Die Seiten, die du ständig brauchst, heftest du mit der Nadel an die Startseite |
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Sofort-Meldung** | Liest die Star-Citizen-`Game.log` mit → der Bauplan steht **in Sekunden** in der Liste |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Tastenkombination** | **Alt+Umschalt+V** klappt das Overlay mit der Bauplan-Liste auf und zu, mitten im Vollbild-Spiel, ohne blind nach dem Fenster zu suchen. **Alt+Umschalt+R** liest das Raffinerie-Terminal, auf Wunsch holt ein drittes Kürzel die Handle-Suche. Angemeldet werden genau diese Kombinationen; mitgehört wird nichts. **Unter Wayland** (Linux) kann kein Programm eine systemweite Kombination selbst belegen: Dort steht unter der Einstellung ein fertiger Befehl zum Kopieren, den du im Desktop auf eine Kombination legst, bei KDE Plasma unter *Systemeinstellungen → Tastatur → Kurzbefehle → Neu hinzufügen → Befehl oder Skript*. Läuft VerseKit schon, öffnet der Befehl das Hauptfenster, statt es ein zweites Mal zu starten |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Bauplan-Liste** | Alle Baupläne durchsuchen, nach Art gruppiert, Filter *alle / habe ich / fehlt mir / Merkliste / vorgemerkt / neu im Spiel*. Häkchen per Klick. Der eigene Reiter **Bauplan-Fortschritt** zeigt je Art, wie weit du bist (für alle Baupläne oder nur für deine Merkliste). Der Filter **Erhalten** (heute, 7 Tage, 14 Tage, dieser Monat) stellt die Liste flach, neueste zuerst, jede Zeile mit Datum und Quelle |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Herkunft je Bauplan** | Der Knopf **„Woher?"** zeigt Fraktion, Auftrag, nötigen Rang und Belohnung für jeden Bauplan, den es über einen Auftrag gibt, sortiert nach dem leichtesten Weg. Aus der **Herstellung** führt ein Knopf direkt dorthin: fehlt dir der Bauplan, siehst du mit einem Klick, welchen Auftrag du dafür machen musst |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Auftrag angenommen** | Nimmst du einen Auftrag an, steht sofort da, ob Baupläne dabei sind, und **welche dir davon noch fehlen**. Kennt der Katalog den Auftrag nicht, wird geschwiegen statt geraten |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Was gerade zu tun ist** | Unter jedem laufenden Auftrag stehen seine **offenen Zwischenziele**: „Hartmoore-Inverter deaktivieren", „Knoten lokalisieren und zurücksetzen". Sie kommen aus demselben Protokoll und wechseln mit, sobald du eines geschafft hast |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Aufträge & Protokoll** | Welche Aufträge du wann gespielt hast, wie oft, und **welcher Bauplan dabei herauskam**. Die Antwort auf „welcher Auftrag war das nochmal, bei dem der Helm kam?". Umgekehrt geht es auch: Auftrag suchen, anklicken, und du siehst, was er hergibt. Beim ersten Start schon gefüllt: Die aufgehobenen Protokolle des Spiels reichen Wochen zurück. Danach wächst es mit und bleibt, auch wenn das Spiel seine eigenen längst gelöscht hat. Hängt ein Auftrag, sucht der Käfer an der Zeile ihn im **Issue Council**, immer mit dem englischen Titel, auch wenn du auf Deutsch spielst |
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Herstellung** | Zu jedem herstellbaren Gegenstand die Zutaten mit Menge und die Herstellzeit, und ob du den Bauplan dafür hast. **Oben stehen die Produktwerte als Grundwert, gebauter Wert und Änderung** (Schaden, DPS, Schild, Kühlung, Rüstung), gelesen aus den Spieldaten deiner eigenen Installation und gegengeprüft mit scmdb.net. Bei Waffen steht auch die Integrität dabei. Ein Klick auf einen Rohstoff springt zu seinen Fundorten |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Bergbau** | Beide Richtungen in einer Suche: Rohstoff eintippen → seine Fundorte. Ort eintippen → was es dort gibt. Dazu je Fundort die **Konzentration**: welcher Anteil der Brocken dort dieses Erz ist, von „kaum etwas" bis „fast nur das", und die Liste steht nach Ergiebigkeit. Ein Auswahlfeld **„Womit?"** trennt Schiff, Fahrzeug und Handabbau, denn die drei sehen völlig verschiedene Vorkommen. Mit **Raffinerie-Vergleich** je Erz (welche Station den besten Bonus gibt) und der **Scan-Signatur** zum Wiedererkennen im Spiel. **Unter Windows liest VerseKit die Signatur beim Scannen selbst ab** und zeigt im Overlay, welches Erz es ist, auch ROC/Boden und Handabbau; bei Asteroiden zählt es die Brocken („2× Asteroid (C-Type)"). Ein- und ausschalten über das Auge in der Leiste. Das **Signatur-Fenster** steht direkt über der Signatur im Spiel: Unter *Einstellungen → Overlay* ziehst du es an seinen Platz und wählst Klein, Mittel oder Groß. Mit dem **Suchrahmen** sagst du, wo im Schiffs-HUD die Signatur steht: „Spielbild holen“, ins Spiel wechseln, im Bild einen Rahmen aufziehen, und mit „Rahmen im Spiel zeigen“ prüfst du ihn im Spiel selbst |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Welche Verarbeitungsmethode?** | Das Terminal bietet neun Methoden an und zeigt zu jeder nur eine Zeile. Sag stattdessen, was dir wichtig ist (**Ertrag, Kosten oder Geschwindigkeit**), und du bekommst die Methode dazu. Zwei der neun lohnen sich übrigens nie: Sie werden von einer anderen in jeder Hinsicht geschlagen, und das steht dann auch da |
| <img src="assets/symbole/22/raffinerie-gruen.png" width="22" alt=""> **Raffinerien** | **Welche Station am meisten aus deinem Erz macht (als eine Tafel).** Eine Zeile je Material, eine Spalte je Raffinerie, und in jeder Zelle der Aufschlag oder Abschlag dieser Station. **Grün ist der beste Wert der Zeile, Rot kostet dich Ausbeute**; über den Spalten steht das System, damit du sofort siehst, ob sich der weitere Weg überhaupt lohnt. Die Gegenrichtung zu „Welche Verarbeitungsmethode?": Dort wählst du die Methode, hier die Station |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Mein Lager** | **Dein ganzes Lager an einer Stelle**, in der Gruppe **Flotte & Logistik** neben Hangar und Wunschliste: oben die Reiter **Material** und **Items**, darunter Filter nach **Art** (Erz, Raffiniert, Mineral, Handelsware, Schiffskomponente, Rüstung …), **Ort** und **Zweck** (zum Herstellen oder zum Verkaufen) und eine Suche. Eine Zeile je Posten, mit der Art als Abzeichen und bei Gegenständen dem Kürzel Klasse/Größe/Grad, etwa `(Mil/3/A)`, ohne Netz und ohne Basetool. Trag ein, was du an Rohstoffen hast: **Material, Menge, Qualität, Lagerort**. Gleiche Materialien stehen gebündelt in einer Zeile mit Gesamtmenge und Güte-Spanne, die einzelnen Posten klappst du auf. **Jeder Posten ist eine Kiste wie im Spiel:** Kisten gleicher Güte am gleichen Ort stehen unter einer Sammelzeile, beim Herstellen kommt jede Portion aus einer Kiste, in der genug liegt, und eine Kiste, die für kein Rezept mehr reicht, ist als **„Rest: nur verkaufen"** markiert. Im Rezept steht dann, was fehlt, und ein Knopf zieht die Zutaten ab, wenn du etwas herstellst, in genau der Güte, die du eingestellt hast: per Regler, eingetippt oder wie am Terminal mit **Bestes / Schlechtestes Material**. **Und weil die Rezepte mittragen, wie die Materialqualität die Werte des Produkts verändert, siehst du, was mit *deinem* Material herauskäme** |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Preise** | Was ein Rohstoff am Terminal kostet und was er einbringt: die Zahlen kommen von **[UEX Corp](https://uexcorp.space/)** und frischen sich täglich auf. Damit beantwortet die Herstellung nicht nur „was fehlt mir", sondern auch „was kostet mich das". Ohne Netz bleibt das Feld einfach leer |
| <img src="assets/symbole/22/handelslager-gruen.png" width="22" alt=""> **Mein Lager: zum Verkaufen** | Was du zum Verkauf im Laderaum hast: im selben Lager, aber mit dem Abzeichen **Verkauf** und eigenem Filter: Das eine ist Baumaterial, das du behältst, das andere Ladung, die du loswerden willst. Ware, Ort und SCU eintragen; im Mengenfeld darfst du rechnen (`100+5`). Statt einer Güte gibt es den Haken **„als gestohlen markiert"**. Beim Verkauf zählt die Qualität nicht, und erbeutete Ware hat ohnehin immer Q 0. Wer einträgt, was er **je SCU bezahlt** hat, sieht je Posten und je Verkaufsort den **Gewinn** |
| <img src="assets/symbole/22/verkauf-gruen.png" width="22" alt=""> **Kaufen & Verkaufen** | Oben umschalten zwischen **Verkaufen** und **Kaufen**. **Kaufen:** Ware suchen (z. B. Gold) und sehen, welche Terminals sie verkaufen: die günstigste Stelle zuerst, mit Vorrat (leer und fast leer sind markiert), Kistengrößen und Alter der Meldung. **Verkaufen:** Wo du deine Ware los wirst und was sie **je SCU** bringt, für **mehrere Waren auf einmal**. Sortiert wird nicht nach dem höchsten Preis, sondern danach, **wie viele deiner Waren ein Ort abnimmt**: 100 SCU Gold, 40 Copper und 25 Iron bringen an einem Ort 3.533.000 aUEC, verteilt auf drei Orte 3.566.000 (ein Prozent mehr für zwei zusätzliche Anflüge). Ist die Ladung als gestohlen markiert, blendet der Reiter auf die 15 Terminals ein, die keine Fragen stellen |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Mein Hangar** | **Welche Schiffe dir gehören, und ob ein Bauplan überhaupt hineinpasst.** Deinen Hangar holst du in einem Zug aus dem Pledge-Store: Die Browser-Erweiterung [Star Citizen: Hangar Extension](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W) legt ihn dir als Datei hin, VerseKit liest sie: JSON **und** CSV, mit LTI oder Versicherungslaufzeit je Schiff (der ältere [Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR) wird weiterhin gelesen). Auch der [Hangar-Exporter](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest) von Drachennarr liest den Hangar von der RSI-Seite; seine Erweiterung für Firefox legt der Installer gleich mit ab, und auf der Hangarseite und im Einrichtungsassistenten gibt es je einen Knopf für Firefox, Chrome und Edge. Auch Schiffslisten aus **Fleetview** (CCU Game), **Fleetyards** und **StarJump FleetViewer** liest es ein. Die ergänzen nur, ausgetragen wird dabei nichts. Im Spiel gekaufte Schiffe trägst du daneben von Hand ein. Jedes Schiff behält, woher es kommt. Danach steht in der Herstellung unter jedem Bauplan, **in welche deiner Schiffe das Teil passt** und in wie viele Steckplätze. Die Steckplatz-Daten kommen von [erkul.games](https://erkul.games) und liegen auf deinem Rechner; geholt wird nur, was du wirklich im Hangar hast, und nur einmal je Spiel-Patch |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Namen im Spiel** (im Hangar) | **Deine Schiffe heißen im Abrufterminal (ASOP) so, wie du sie nennst.** Statt „Anvil F7C-M Super Hornet Mk II" steht dort, was du eingetragen hast, und ein Sternchen davor markiert eines, ohne es umzutaufen. Das Feld dafür steht im Hangar direkt an jedem Schiff, die Suche dort findet es auch über deinen eigenen Namen. ⚠ Ein Name gehört zum **Muster**, nicht zum einzelnen Schiff: Zwei gleiche Hornets bekommen denselben Namen; Abwandlungen wie F7C, F7C-M, Mk I und Mk II lassen sich unterscheiden. Der Werksname kommt jederzeit zeichengenau zurück |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Ruf-Stufen** | **Im Reputationsmenü steht hinter jedem Rang, ab wie viel Ruf er beginnt**: „Gildenmitglied [ab 10.000]". Den Balken füllt nur der Spielserver, aber du siehst, was als Nächstes kommt: Kopfgeldjäger, Auftragnehmer, Transport, Sicherheit, Techniker, Battaglia und Wikelo. Abschaltbar unter „Spiel" |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Bauplan-Angaben in den Beschreibungen** | **Unter der Beschreibung jedes herstellbaren Gegenstands steht im Spiel, ob du den Bauplan hast**, und wenn nicht, aus welchem Auftrag er kommt, der leichteste zuerst. Dazu, wie viele Zutaten und wie viel Zeit das Herstellen braucht und wo es das Teil am günstigsten zu kaufen gibt. Fremder Text bleibt stehen, der Block kommt darunter. Hat das Spiel für ein herstellbares Teil keinen Namen im Text, schreibt VerseKit einen Notnamen aus dem Klassennamen. Text, den das Spiel hat, bleibt unberührt, und beim Zurücknehmen verschwindet der Notname wieder. Abschaltbar unter „Spiel" |
| <img src="assets/symbole/22/wunschliste-gruen.png" width="22" alt=""> **Wunschliste** | **Was du dir vornimmst, mit Preis, Ort und der Ausstattung, die du schon vorher planen kannst.** Trag ein Schiff ein, und daneben steht, was es im Spiel kostet und wo es verkauft wird. Die Ausstattung lässt sich planen, **bevor** du das Schiff besitzt, genau dann, wenn die Summe noch eine Entscheidung ist und keine Quittung. Was hier steht, taucht nirgends bei „passt in dein Schiff" auf: Ein Wunsch ist kein Besitz |
| <img src="assets/symbole/22/einkaufsliste-gruen.png" width="22" alt=""> **Einkaufsliste** | **Die Rechnung über alle deine Schiffe.** Leg je Steckplatz fest, was dort sitzen soll. An jedem Teil stehen **Güte und Klasse** („A · Militär · nur über Bauplan"), damit du auf einen Zweck hin bauen kannst, ohne alle Namen auswendig zu kennen. Auch **militärische Teile** sind dabei: Die gibt es in keinem Laden, nur über Baupläne. Je Posten wählst du **kaufen oder selbst herstellen**, und darunter steht die Summe samt Einkaufsroute mit möglichst wenigen Stopps. Was du eingebaut hast, hakst du ab. Es fällt aus der Liste und aus der Summe |
| <img src="assets/symbole/22/farmliste-gruen.png" width="22" alt=""> **Was ich farmen muss** | **Dein Lager gegen alles gerechnet, was du selbst bauen willst.** Über alle Posten zusammen, nicht Rezept für Rezept: Zwei Bauteile mit je 2 Iron bei 3 Iron im Lager: einzeln geprüft „reicht", zusammen fehlt eines. Erz mit zu geringer Güte zählt nicht als Bestand, wird aber genannt statt verschwiegen. Zu jedem fehlenden Rohstoff stehen die **ergiebigsten Fundorte** dabei, dazu die Orte, an denen du **mehrere auf einmal** bekommst. Vorgemerkte Teile bekommen eine **Stückzahl**: zwei Rüstungen heißen doppeltes Material. Ein Klick auf einen Rohstoff zeigt ihn im Bergbau mit allen Fundorten |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Was steckt drin?** | **Was ein Wrack ab Werk an Bord hat, und was es im Laden wert ist.** Schiff eintippen, und du siehst jedes verbaute Teil mit Ladenwert. ⚠ NPC-Wracks sind lootbar; Spielerschiffe werden unbrauchbar, sobald die Versicherung beansprucht wird. Dann sind auch ausgebaute Teile wertlos. Der Hinweis steht **vor** jeder Zahl |
| <img src="assets/symbole/22/zerlegen-gruen.png" width="22" alt=""> **Lohnt das Zerlegen?** | **Was der Fabricator zurückgibt, bevor du den Schneidbrenner ansetzt.** 50 % der Materialien, aber **sechs Rohstoffe kommen nie wieder**, darunter Quantainium und Stileron. Bei den meisten Teilen ist mindestens einer davon dabei; wer nur deswegen zerlegt, hat umsonst geschleppt. Die Werte kommen aus den Spieldaten, nicht aus dem Programm |
| <img src="assets/symbole/22/blickwinkel-gruen.png" width="22" alt=""> **FOV** | **Sitzt du richtig vor deinem Bildschirm?** Miss deine Bildschirmbreite mit einer Bankkarte aus (die ist genormt), gib deinen Sitzabstand an, und du bekommst den Blickwinkel, den dein Aufbau wirklich hergibt, samt Bewertung |
| <img src="assets/symbole/22/laeden-gruen.png" width="22" alt=""> **Shops** | **Wo steht das fertige Teil im Regal, und was kostet es dort?** Die Gegenrichtung zur Herstellung: Statt „was brauche ich zum Bauen" die Frage „lohnt der Aufwand überhaupt". Teile aller Warengruppen, nicht nur Craftbares, sondern auch Raketen, Bomben, Munition und Waffenaufsätze, dazu Schiffe zum Kaufen und Mieten. **Jedes Teil legst du mit einer Menge in den Einkaufskorb**, etwa eine ganze FPS-Ausrüstung: Die Einkaufsliste rechnet die Kosten zusammen, zeigt zu jedem Teil alle Einkaufsorte, plant die Route mit den wenigsten Stopps und lässt dich jeden Stopp abhaken. Eine Zusammenstellung speicherst du als Vorlage unter einem Namen. An jeder Zeile stehen **Klasse, Größe, Güte und Hersteller**, und du filterst danach: So findest du den Quantenantrieb, der in dein Schiff passt, ohne alle Namen zu kennen. Mit dem Feld **Ort** siehst du nur, was es dort gibt, wo du gerade bist, etwa alles, was in Levski im Regal steht. Teil anklicken, und du bekommst jeden Laden mit Preis, Ort und System, der günstigste oben, gebrauchte Ware mit ihrem Zustand. Was nirgends verkauft wird, taucht gar nicht erst auf |
| <img src="assets/symbole/22/routen-gruen.png" width="22" alt=""> **Routen** | **Handelsrouten mit echtem Gewinn statt einer Preisliste.** Sag, wo du stehst, wie viel Frachtraum du hast und wie viel Geld, und du bekommst die Fahrt samt Einkauf, Verkauf und dem, was am Ende übrig bleibt. Auf Wunsch über mehrere Stationen hintereinander, wahlweise auf den höchsten Gewinn oder den kürzesten Weg sortiert, auch als Rundreise zurück zum Start. Ein Knopf sucht die beste Route **im ganzen Verse**, ohne dass du einen Startort angibst. Dein Schiff kannst du auswählen, der Frachtraum kommt dann von selbst |
| <img src="assets/symbole/22/joysticks-gruen.png" width="22" alt=""> **Steuerung** | **Was liegt auf welcher Taste, und umbelegen ohne ins Spiel zu wechseln.** Die komplette Belegung im Klartext („Schleudersitz" statt `v_eject`, in der Sprache des Werkzeugs), Joystick, Tastatur, Maus und Gamepad in einer durchsuchbaren Liste, umschaltbar zwischen *von mir geändert*, *alles* und *Werkseinstellung*. Zum **Neubelegen** eine Zeile anklicken und den Knopf drücken, den du meinst. Die Nummer musst du nicht kennen, und ist die Eingabe schon vergeben, steht das vorher da. Dazu: welcher Stick welche Nummer hat und ob deine Belegung noch auf ein angeschlossenes Gerät zeigt. Funktioniert für **jedes** Gerät. Es muss nichts vorher eingepflegt werden |
| <img src="assets/symbole/22/einrichtung-gruen.png" width="22" alt=""> **Einrichtungsassistent** | Führt beim ersten Start durch alles Wichtige, auch die wichtigsten Einstellungen, und **jederzeit wiederholbar**, ohne sich durch Menüs zu klicken |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Sicherung** | Alles, was nur du hast, in **eine Datei**: Bauplan-Bestand, beide Lager, Auftrags-Protokoll, Merkliste, Einstellungen und deine Steuerung (aktive Belegung und gespeicherte Profile). Derselbe Knopf spielt sie auch wieder ein: Datei auf den Stick, am neuen Rechner einlesen, weiterspielen. Beim Einspielen fragt VerseKit getrennt, ob die Profile zurückgelegt werden und ob auch die aktive Belegung ersetzt wird; bei laufendem Spiel passiert nichts, und vorher entsteht immer eine Kopie. Die heruntergeladenen Nachschlagewerke bleiben draußen, die holt sich das Programm selbst. **Beim ersten Start von VerseKit 5** sichert es außerdem deine Daten aus 4.12 samt Steuerung im Ordner „Sicherungen“ deines Datenordners, bevor es irgendetwas ändert |
| <img src="assets/symbole/22/anzeige-gruen.png" width="22" alt=""> **Grafik** | Renderer, Fenstermodus, VSync, Upscaling samt Technik und Modell, Bewegungsunschärfe, Filmkörnung, Schärfen und chromatische Aberration, eingestellt, ohne ins Spiel zu müssen. Darüber, was das Spiel über deinen Rechner misst: Leistungsindex und Einstufung von Niedrig bis Ultra. Die Einstufung merkt sich VerseKit und sagt dazu, wann sie zuletzt gemessen wurde („zuletzt eingestuft am …“). Geschrieben wird nur bei geschlossenem Spiel, und vorher legt das Programm eine Sicherung der Datei daneben |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Shader-Cache leeren** | Grafikfehler oder Ruckler nach einem Patch? Ein eigener Reiter unter *Spiel-Einstellungen* leert die Shader-Speicher von Star Citizen und der Grafiktreiber sowie alte Absturzberichte. „Nur alte Shader löschen“ nimmt allein die Ordner älterer Star-Citizen-Versionen mit; installierte und unklare Ordner bleiben stehen. „Alles löschen“ bleibt die zweite Wahl. Die Ordner findet das Programm selbst, unter Linux auch im Wine-Präfix. Deine Grafikeinstellungen bleiben, und solange das Spiel läuft, passiert nichts |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **Katalog-Wache** | Meldet auch, wenn im **Spiel** etwas neu craftbar wird, also wenn CIG einen Bauplan nachreicht, den es vorher gar nicht gab (nicht nur, was du selbst freischaltest) |
| <img src="assets/symbole/22/serverstatus-gruen.png" width="22" alt=""> **Serverstatus** | Eigener Reiter: **Läuft Star Citizen gerade?** Zeigt, was CIG auf seiner Statusseite meldet: die drei Systeme und die Meldungen der letzten zwei Monate im Volltext. Frischt sich jede Minute selbst auf. Die Zustände bleiben im Wortlaut von CIG; die Angaben sind von Hand gepflegt, keine Messung |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Geänderte Spielwerte** | Eigener Reiter: **was ein Spiel-Patch an Werten verändert hat**: Schiffe, Waffen, Quantum-Antriebe, Komponenten, Feld für Feld mit altem und neuem Wert. Gestiegen grün, gefallen rot. Die Quelle hebt nur die letzten zehn Patches auf; was du einmal geholt hast, bleibt bei dir. |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **Neu im Spiel** | Eigener Filter in der Liste: **nur das, was mit dem aktuellen Patch dazukam**. Jeder Bauplan trägt die Spielversion, in der es ihn zuerst gab; beim nächsten Patch rücken die neuen nach und die alten fallen aus dem Filter. Der Stempel bleibt. Ein Auswahlfeld **Patch** zeigt zusätzlich jeden früheren Patch und erweitert sich von allein |
| <img src="assets/symbole/18/gemerkt-gruen.png" width="22" alt=""> **Merkliste** | Klick auf den Stern in der Liste: Taucht der Bauplan auf, wird er auffällig gemeldet und **auf der Merkliste abgehakt**. So zeigt der Fortschritt, wie weit du mit deinen gemerkten Bauplänen bist |
| <img src="assets/symbole/22/kuerzel-gruen.png" width="22" alt=""> **Klasse · Size · Grade** | Kompakt-Kürzel `Klasse/Size/Grade` je Bauplan, z. B. `M/1/A` (Military · Size 1 · Grade A) |
| <img src="assets/symbole/22/ton-gruen.png" width="22" alt=""> **Signalton** | Kurzer Ton bei jedem Neuzugang: du musst nicht aufs Fenster schauen |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Info-Leiste** | **Eine eigene schmale Leiste über dem Spiel**, über dem laufenden Spiel ab Werk an; ausschalten kannst du sie unter *Einstellungen → Overlay → Info-Leiste*. Sie zeigt den Server-Shard und dazu auf Wunsch drei Werte, jeder mit eigenem Schalter: **deine FPS**, die **Server-FPS** und die **Spieler** auf dem Shard und auf deinem Server. Die Werte liest VerseKit aus dem Spielbild; dafür müssen im Spiel `r_DisplayInfo 1` (FPS) bzw. `r_DisplayInfo 3` (Spieler) an sein. Beim Bergbau steht rechts das Scanner-Ergebnis, oder, wenn du es so einstellst, in einem kleinen Fenster über der Signatur. Bei offenem Schloss ziehst du die Leiste an ihren Platz (oben links, Mitte oder rechts), VerseKit merkt ihn sich. Mit **„Auch ohne laufendes Spiel zeigen“** bleibt sie auch ohne Star Citizen sichtbar |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Handle-Suche** | **Wer steckt hinter einem Handle?** Im Feld der Info-Leiste und auf der Seite *Handle-Suche*: **Enter** sucht, **Strg+Enter** holt die Daten neu von RSI. Die Karte zeigt Name, Hauptorganisation und weitere Organisationen, Rang, SID und das Profil bei RSI. Handles und Organisationen markierst du als **freundlich, neutral oder feindlich** und siehst die Liste gefiltert auf der Seite; die Beziehungen lassen sich exportieren und importieren. Zu einer Organisation zeigt die Karte die **Mitglieder** („n von gesamt geladen“, mit „Mehr laden“); verborgene Mitglieder fehlen in der Liste. **Zuletzt gesucht** merkt sich den Verlauf. Der **Log-Monitor** zeigt Kills, Tode und Ladebildschirme aus dem Spielprotokoll; ein Klick auf den Handle sucht ihn. Gefundene Handles speichert VerseKit so viele Tage, wie du einstellst. Vorbild ist das [Star Citizen Handle Query](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query) von Kuehlwagen |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Über dem Spiel** | Randloses, leicht durchscheinendes Overlay, das dem Spiel folgt: Es liegt über Star Citizen, solange du spielst |
| <img src="assets/symbole/22/schloss_auf-gruen.png" width="22" alt=""> **Klicks ins Spiel durchreichen** | Ein Klick auf das Schloss in der Leiste, und das Overlay lässt Mausklicks durch. Es steht weiter im Bild, ist aber nicht mehr im Weg. Dasselbe Schloss wird dabei grün und holt es mit einem Klick wieder zurück, ohne Umweg über die Einstellungen |
| <img src="assets/symbole/22/verschieben-gruen.png" width="22" alt=""> **Verschiebbar & skalierbar** | An der Titelleiste ziehen, Größe am Griff ◢ unten rechts: **Position & Größe werden gemerkt**. Wer mehr will, stellt unter *Einstellungen → Overlay* die **Größe des Overlays** und die **Größe der Info-Leiste** von 100 bis 200 Prozent ein, dann wachsen Schrift, Abstände und Fenster mit |
| <img src="assets/symbole/22/darstellung-gruen.png" width="22" alt=""> **Sechs Farbschemata** | Unter **„Darstellung"** mit kleiner Vorschau: **VerseKit (Original), KRT (Orange), Eis (Cyan), Nebel (Violett), Glut (Rot) und Hoher Kontrast**, jedes mit eigenen Symbolen, fürs Overlay und alle Fenster. Der Wechsel wirkt sofort, ohne Neustart. Bilder oben unter *Die Farbschemata* |
| <img src="assets/symbole/22/darstellung-gruen.png" width="22" alt=""> **Größe der Oberfläche** | Schrift, Symbole und Knöpfe **stufenlos** per Regler, oder die Voreinstellung für deinen Bildschirm: Auto, Full HD, WQHD, UHD (125 %), UHD (150 %) |
| <img src="assets/symbole/22/darstellung-gruen.png" width="22" alt=""> **Scharfe Darstellung** | Auch unter Windows mit Skalierung über 100 %, etwa 150 % am 4K-Bildschirm, zeichnet VerseKit selbst in voller Auflösung, statt von Windows hochgerechnet und unscharf zu werden. Ohne Schalter, ab Werk |
| <img src="assets/symbole/22/module-gruen.png" width="22" alt=""> **Module** | Basetool, Schiffe, Herstellen (mit Bergbau und Raffinerien), Bergung, Handel und Statistik lassen sich **aus der Seitenleiste ausblenden**, wenn du sie nicht brauchst, je Seite, egal in welcher Gruppe sie steht; eine leere Gruppe verschwindet. Es wird nichts gelöscht |
| <img src="assets/symbole/22/startprogramme-gruen.png" width="22" alt=""> **Startprogramme** | Weitere Programme **mit dem RSI Launcher**, sobald Star Citizen läuft, oder **statt** des Launchers starten, auf Wunsch beim Spielende wieder beenden |
| <img src="assets/symbole/22/startprogramme-gruen.png" width="22" alt=""> **Mit MangoHud starten** (Linux) | Ein Schalter unter *Installation & Pfade* schaltet beim Start über „RSI Launcher starten" **MangoHud** ein: Bilder pro Sekunde, Prozessor- und Grafikkarten-Last direkt im Spiel. MangoHud selbst kommt aus der Paketverwaltung; fehlt es, steht dort, woher |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **user.cfg** | Auf der Seite **„Spiel"**: was VerseKit einträgt, deine eigenen Zeilen und die ganze Datei, je Spielkanal. Vor der ersten Änderung wird die alte Datei gesichert |
| <img src="assets/symbole/22/sprachen-gruen.png" width="22" alt=""> **Deutsch und Englisch** | Oberfläche umschaltbar. Die Bauplan-Meldung im Log erkennt VerseKit **in jeder Spielsprache**. Es findet die Formulierung selbst heraus |
| <img src="assets/symbole/22/abhaken-gruen.png" width="22" alt=""> **Hält sich selbst aktuell** | Sieht alle zehn Minuten nach einer neuen Version, lädt sie mit Prüfsumme und spielt sie ein, sobald Star Citizen geschlossen ist, **nie, während das Spiel läuft**. Ohne Schalter. Mit „Was ist neu" zum Nachlesen, auch für ältere Versionen |
| <img src="assets/symbole/22/nurlesend-gruen.png" width="22" alt=""> **Nur lesend** | Liest die `Game.log`. **Eine Ausnahme, und die fragt vorher:** Auf Wunsch trägt VerseKit die Bauplan-Kennzeichnung in die `global.ini` ein. Das lässt sich jederzeit wieder zurücknehmen, auch Text anderer Werkzeuge bleibt dabei erhalten. Dazu der Anti-AFK (nur Windows, standardmäßig aus): Eingeschaltet drückt er alle 5 Minuten Alt+X, wie du es selbst tun würdest |
| <img src="assets/symbole/22/eigenbuch-gruen.png" width="22" alt=""> **Eigener Bestand** | Führt selbst Buch, welche Baupläne du hast, ohne ein weiteres Programm |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Spielzeit** | Oben in der Leiste steht, **wie lange du gespielt hast**: insgesamt und, während du spielst, die laufende Sitzung dahinter. Gezählt wird aus den Protokollen des Spiels, und zwar **fortgeschrieben**: Star Citizen räumt seine alten Logs weg, das Gezählte bleibt. Die Sicherung nimmt es beim Rechnerwechsel mit |
| <img src="assets/symbole/22/statistik-gruen.png" width="22" alt=""> **Statistik** | Eigener Bereich mit fünf Reitern: die **Übersicht** (Spielzeit, Sitzungen, Aufträge und Quantensprünge mit einer Wärmekarte, **wann du spielst**), dazu **Schiffe & Ausrüstung** (meistgenutzte und verlorene Schiffe, getragene Waffen), **Aufträge**, **Quantenreisen** und **Stabilität** (Abstürze, Verbindungsabbrüche). Was du im Laden ausgegeben hast, zeigt die Statistik auch. Gezählt wird nur, was in jeder Spielsprache verlässlich im Log steht; jede Sitzung wird einmal festgehalten und bleibt, auch wenn die Logs gelöscht werden. Lässt sich als Datei speichern |
| <img src="assets/symbole/22/sprachen-gruen.png" width="22" alt=""> **Übersetzung: je Kanal** | Welche Textdatei Star Citizen liest, stellst du für **jeden Kanal getrennt** ein: LIVE, PTU und Tech-Preview. Jeder Kanal bekommt Textdatei, Sprache und Bauplan-Angaben, oder gar nichts, wenn du ihn in Ruhe lassen willst. Neue Fassungen holt VerseKit für jeden Kanal selbst: Es sieht alle fünf Minuten nach, auch nach einem Patch, und die Statuszeile zeigt den Stand der Datei und wann zuletzt geholt und nachgesehen wurde. Legt ein anderes Programm eine eigene Textdatei hinein oder stellt die Sprache um, setzt VerseKit die gewählte wieder ein und sichert die fremde daneben. Zur Wahl stehen **Deutsch** (Dymerz), **StarStrings** (MrKraken, auch fürs PTU), **Français, Italiano, Español, Português und Türkçe** (Dymerz), acht Sprachen vom **SC Launch Configurator** (Luftwerft: Deutsch, Englisch, Français, Italiano, Español, Boarisch, Nederlands und Português, für LIVE und PTU) und das **Englisch des Spiels**, oder **gar nichts**, dann bleibt die Textdatei unberührt. Wer eine andere nutzt, trägt ihre **Adresse** ein: eine Zip oder eine `global.ini`, nur `https://`; eine GitHub-Dateiansicht wird selbst in den Download umgewandelt, und was keine Textdatei des Spiels ist, wird abgelehnt |
| <img src="assets/symbole/22/basetool-gruen.png" width="22" alt=""> **Abgleich mit dem KRT Profit Basetool** | Baupläne, Mein Lager und Hangar gleichen sich mit deinem Basetool ab, in beide Richtungen. Dazu der Bedarf und das **Lager deiner Einheit** (in der Gruppe **Flotte & Logistik**, eine kompakte Zeile je Posten mit Art und Kürzel); als Logistiker buchst du dort direkt aus VerseKit, über die Symbole am Eintrag. Eigene Ware und Items gibst du in **Mein Lager** mit einem Knopf an der Zeile ins Lager einer deiner Einheiten und nimmst sie dort auch zurück. Du bleibst der Besitzer; ein Umschalter **Alle / Persönlich / Gemeinsam** zeigt, was wo liegt. Verbunden wird mit einem Code, den du im Browser eintippst; ein Passwort sieht VerseKit nie, die Zugangsdaten liegen im Schlüsselspeicher deines Systems. Jeder Bereich ist ab Werk aus. Gelöscht wird nie still, bei verschiedenen Mengen entscheidest du, Kaufdaten deiner Schiffe gehen nie hinaus. Was übertragen wird, steht in der [Datenschutzerklärung](PRIVACY.md). Die Verbindung steht unter **Einstellungen → Verbindung & Abgleich**, der Bedarf unter **Beschaffung**, das Lager der Einheit unter **Flotte & Logistik**, dieselben Gruppen wie in der Seitenleiste des Basetools |
| <img src="assets/symbole/22/quellen-gruen.png" width="22" alt=""> **Mit scmdb.net verbinden** | Neue Baupläne und laufende Aufträge landen direkt in deinem scmdb-Konto, ohne Export, ohne Hochladen und ohne zusätzliches Programm. Einschalten unter *Einstellungen → Allgemein*, dann auf scmdb.net „Watcher" aktivieren. Übernommen wird nur aus LIVE und HOTFIX, ab Werk ist es aus |
| <img src="assets/symbole/22/bestaetigt-gruen.png" width="22" alt=""> **Alle Baupläne oder nur die erspielbaren** | Star Citizen kann viel herstellen, doch nur ein Teil der Baupläne ist über Aufträge zu bekommen. Der Rest kommt aus Kiosken, aus Events oder hängt an gar keiner Mission. Ab Werk zählen nur die erspielbaren, damit dein Fortschritt eine Zahl ist, an der du dich messen kannst; unter *Erkennung* schaltest du die übrigen dazu |
| <img src="assets/symbole/22/diagnose-gruen.png" width="22" alt=""> **Fehler melden in einem Klick** | Ein Feld für einen Satz, darunter der fertige Bericht, daneben ein roter Knopf. Mehr ist nicht zu tun. Bis zu vier **Bildschirmfotos** gehen auf Wunsch mit. Der Bericht enthält System, Verpackung, Spielstand und die letzten Fehler, aber **weder deinen Benutzernamen noch deine Pfade** (ein Name steht nur oben als Melder und lässt sich leeren), und du siehst ihn vorher vollständig. Wer lieber selbst schreibt, bekommt mit einem Knopf ein vorbereitetes Issue |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Nachlese** | Liest beim Start die aufgehobenen Logs früherer Sitzungen **und die laufende** und holt nach, was ohne laufendes VerseKit freigeschaltet wurde. Die Funde werden gemeldet, nicht still eingetragen. Ein Knopf **Protokolle erneut einlesen** (Overlay und Einstellungen) geht auf Wunsch noch einmal alles durch |
| <img src="assets/symbole/22/anzeige-gruen.png" width="22" alt=""> **Streamer-Modus** | Unter *Einstellungen → Allgemein → Datenschutz* einschaltbar: Spielername, Pfade, Org und Einheit, Basetool-Name und Gerätecodes erscheinen auf jeder Seite und im Overlay als „[Geschwärzt]“. Solange er an ist, zeigt die Kopfzeile den Knopf „Streamer-Modus an“. Ab Werk aus. Die Kopieren-Knöpfe kopieren weiter den Klartext |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Overlay klappt auf** | Unter *Einstellungen → Overlay* legst du je Ereignis fest, ob es das Overlay aufklappt: Bauplan, Scanner-Treffer, Auftrag, fertige Raffinerie und Katalogeintrag. Ab Werk klappt nur der Bauplan auf |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Käufe als Vorschlag** | Was du im Spiel im Laden kaufst, schlägt dir *Mein Lager* unter „Laut Log neu“ zum Übernehmen vor, mit dem Preis je Stück. Du übernimmst oder verwirfst jeden Kauf einzeln oder alle auf einmal und kannst es zurücknehmen; gebucht wird nie von allein. Unter *Einstellungen → Erkennung* abschaltbar |
| 🐧 **Windows und Linux** | Eine Version für beide Systeme, inklusive Autostart und Spracherkennung im Log. Unter Linux trägst du VerseKit unter *Einstellungen → Allgemein* ins Startmenü ein, der Signalton kommt über die Klänge deines Desktops und die Systemmeldung über `notify-send`. Unter **KDE** liegt das Overlay im Vollbild ohne Startzusatz über dem Spiel; mit der Umgebungsvariable `VERSEKIT_X11_FENSTERTYP` wählst du einen anderen Fenstertyp, und sie hat Vorrang. Ablagesymbol und Scanner gibt es nur unter Windows |

</details>

<details>
<summary><b>Installation im Einzelnen</b></summary>

**Voraussetzungen:** Windows oder Linux und ein installiertes Star Citizen. Gesucht wird der Ordner mit der `Game.log`, unter Linux in den üblichen Wine-Präfixen (lug-helper, Lutris, Bottles auch als Flatpak, Heroic, Steam mit Proton). Findet VerseKit nichts, fragt der Assistent danach. Sonst brauchst du nichts: kein Konto, nichts nachzuinstallieren.

**Windows:** `VerseKit-Setup.exe` installiert mit Startmenü-Eintrag, auf Wunsch mit Desktop-Symbol und Autostart, und lässt sich über *Apps & Features* sauber entfernen. Eine einzelne `.exe` ohne Installer gibt es seit v3.0.0 nicht mehr: Ein Update legte die neue Fassung neben die alte Datei, und wer danach die gewohnte Verknüpfung anklickte, benutzte unbemerkt weiter die alte Version. Mit dem Installer ersetzt ein Update wirklich.

**Linux:** `VerseKit-x86_64.AppImage` ist eine einzelne Datei. Mach sie einmal ausführbar (Rechtsklick → Eigenschaften → *Als Programm ausführbar*, oder `chmod +x VerseKit-x86_64.AppImage`). Einen Startmenü-Eintrag bietet der Assistent auf Wunsch an, später findest du ihn unter *Einstellungen → Allgemein*.

**Beim ersten Start** führt dich ein Assistent durch die Einrichtung: Sprache, Star Citizen finden, bisherige Baupläne holen, Anzeige, Programmstart und Angaben im Spiel. Das dauert eine Minute, danach steht dein Bestand.

#### ⚠️ Windows meldet „Der Computer wurde durch Windows geschützt"

Das kommt beim ersten Start, und es ist **kein Virenfund**:

> Von Microsoft Defender SmartScreen wurde der Start einer unbekannten App verhindert.

**So startest du trotzdem:** **Weitere Informationen** anklicken → **Trotzdem ausführen**. Danach kommt die Meldung nicht wieder.

**Warum das passiert:** SmartScreen prüft nicht, *ob* ein Programm schädlich ist, sondern ob es **bekannt** ist. Bekannt wird eine Datei durch eine gekaufte Code-Signatur (mehrere hundert Euro im Jahr) oder dadurch, dass sie sehr viele Leute heruntergeladen haben. Ein kostenloses Fan-Werkzeug hat beides nicht. Jede neue Version fängt wieder bei null an.

**Wenn du das nicht einfach glauben willst, musst du auch nicht:**

- Der **Quellcode ist offen** ([hier](https://github.com/Xharig/VerseKit)), und die Datei wird nicht von mir gebaut, sondern von **GitHub Actions** aus genau diesem Quellcode. Wer will, kann den Bauvorgang nachlesen: [`.github/workflows/`](.github/workflows/)
- Jede Datei auf der Releases-Seite trägt ihre **SHA-256-Prüfsumme**: GitHub zeigt sie direkt an
- Jede Version lässt der Bau bei **VirusTotal** prüfen: Ergebnis und Link zum Bericht stehen unten in der [Release-Notiz](https://github.com/Xharig/VerseKit/releases/latest). Einzelne Prüfprogramme melden bei neuen, noch unbekannten Programmen gelegentlich einen Treffer aus maschineller Heuristik, ein bekannter Fehlalarm

Unter **Linux** gibt es diese Meldung nicht. Dort muss die Datei nur einmal ausführbar gemacht werden.

#### Signatur der Dateien

Für dieses Projekt ist eine kostenlose Code-Signatur bei der
[SignPath Foundation](https://signpath.org/) beantragt, einem Angebot für
quelloffene Projekte. Sobald sie bewilligt ist, werden die Windows-Dateien
von SignPath unterschrieben, und Windows zeigt statt „unbekannter
Herausgeber" einen Namen an.

Gebaut wird ausschließlich über einen öffentlichen GitHub-Actions-Ablauf:
[SECURITY.md](https://github.com/Xharig/VerseKit/blob/main/SECURITY.md) beschreibt, wie eine Version entsteht, und
[PRIVACY.md](https://github.com/Xharig/VerseKit/blob/main/PRIVACY.md), was das Programm liest, holt und sendet
(und was nicht).

> ℹ️ Geprüft an einer echten Star-Citizen-Installation, mit **deutschem und englischem** Spiel-Client. Rückmeldungen von anderen Rechnern sind weiter willkommen: andere Installationsorte, andere Bildschirmaufbauten, Windows. Gern als [Issue](https://github.com/Xharig/VerseKit/issues).

</details>

<details>
<summary><b>Aus dem Quellcode bauen</b></summary>

Für Neugierige und Entwickler. Du brauchst [Rust](https://www.rust-lang.org/tools/install) ab Version 1.99.

```bash
git clone https://github.com/Xharig/VerseKit.git
```

```bash
cd VerseKit
```

```bash
cargo build --release
```

Die fertigen Dateien auf der Releases-Seite baut **GitHub Actions** bei jedem Versions-Tag aus genau diesem Quellcode ([Abläufe](.github/workflows/)). Von Hand baut sie niemand, auch der Autor nicht.

</details>

<details>
<summary><b>Bedienung des Overlays</b></summary>

Die schmale Leiste liegt über dem Spiel und meldet Neuzugänge. Alles Weitere steckt hinter den Zeichen in ihrer Titelleiste:

| Zeichen | Was es tut |
|---|---|
| <img src="assets/symbole/22/glocke-grau.png" width="22" alt=""> | **Glocke**: neue Version verfügbar; färbt sich grün, sobald es eine gibt |
| <img src="assets/symbole/22/starten-grau.png" width="22" alt=""> | **Rakete**: RSI Launcher starten. Erscheint nur, wenn ein Weg dorthin gefunden wurde |
| <img src="assets/symbole/22/einstellungen-grau.png" width="22" alt=""> | **Zahnrad**: das große Fenster öffnen oder zurückholen: Bauplan-Liste, Hangar, Herstellung, Einstellungen |
| <img src="assets/symbole/22/schloss_auf-grau.png" width="22" alt=""> | **Schloss**: Mausklicks ins Spiel durchreichen. Es wird grün und zu, solange sie durchgehen; ein Klick darauf fängt sie wieder ab |
| <img src="assets/symbole/22/einklappen-grau.png" width="22" alt=""> | **Pfeil**: Overlay einklappen, bis nur noch die Leiste dasteht |
| <img src="assets/symbole/22/leeren-grau.png" width="22" alt=""> | **Radiergummi**: angezeigte Meldungen wegräumen. Deine Baupläne bleiben |
| <img src="assets/symbole/22/schliessen-grau.png" width="22" alt=""> | **Kreuz**: beendet VerseKit, auch alle offenen Hauptfenster |

| Aktion | Wie |
|---|---|
| Fenster verschieben | Oben an der Leiste ziehen |
| Größe ändern | Griff **◢** unten rechts ziehen |

Die Hinweise zu den Knöpfen erscheinen in einem eigenen kleinen Fenster neben dem Mauszeiger, auch wenn das Overlay eingeklappt ist.

**Das X im Hauptfenster versteckt es nur**, VerseKit läuft weiter. Zurück holst du es über das Symbol neben der Uhr, das Zahnrad im Overlay oder einen zweiten Start. Beendet wird VerseKit mit dem Kreuz im Overlay. Ohne laufendes Overlay, während eine Sicherung geschrieben wird und bei Update oder Neustart beendet sich das Programm, wenn du das Hauptfenster schließt.

Was die Farbpunkte in der Liste bedeuten:

| | |
|---|---|
| <img src="assets/symbole/18/bestaetigt-gruen.png" width="18" alt=""> | Bauplan freigeschaltet: steht in deinem Bestand |
| <img src="assets/symbole/18/punkt-blau.png" width="18" alt=""> | im **Spiel** neu craftbar geworden: noch nichts, was *du* hast |
| <img src="assets/symbole/18/gemerkt-gelb.png" width="18" alt=""> | etwas von deiner Merkliste ist aufgetaucht |
| <img src="assets/symbole/18/hinweiszeile-grau.png" width="18" alt=""> | ein Hinweis, keine Freischaltung (z. B. eine Lücke im Bestand) |

</details>

<details>
<summary><b>Testversionen und Fehler melden</b></summary>

Vor jeder Veröffentlichung gibt es **Testversionen**, erkennbar am `rc` in der Nummer. Sie stehen unter [Releases](https://github.com/Xharig/VerseKit/releases), jede mit dem, was sie bringt und was sich seit der vorigen geändert hat. Angeboten werden sie nur, wenn du sie einschaltest: auf der Seite **Update & Über** die **Testversion** wählen. Danach kommen sie auf demselben Weg wie jedes Update, nichts von Hand herunterladen.

- **Testversionen sind fertig gebaut und lauffähig**, aber noch nicht lange erprobt.
  Es kann etwas klemmen. Genau dafür sind sie da.
- **Der Rückweg steht immer offen.** Schaltest du wieder um, bekommst du die nächste
  stabile Version angeboten: Eine stabile gilt immer als neuer als jede Testversion
  derselben Nummer. Man bleibt also nicht versehentlich im Testkanal hängen.
- **Ohne diese Einstellung merkst du von Testversionen nichts.** Wer Ruhe will, muss
  nichts tun. Das ist die Voreinstellung.
- **Wer schon eine Testfassung hat, bekommt weitere Testfassungen von selbst.**

Etwas gefunden? Der schnellste Weg ist die Seite **Fehler melden**: Dort steht ein Feld
„Was ist passiert?" für einen Satz, darunter der fertige Bericht, und der rote Knopf
**Fehlerbericht absenden** schickt ihn direkt. Mit **Bild anhängen** gehen bis zu vier
Bildschirmfotos mit. Mehr musst du nicht tun. Der Bericht
enthält alles, was zur Fehlersuche gebraucht wird, und keine persönlichen Angaben;
du siehst ihn vorher vollständig.

Lieber selbst? Ein [Issue](https://github.com/Xharig/VerseKit/issues) geht genauso: der Knopf **GitHub Issue …** auf
derselben Seite legt den Bericht schon hinein. Oder das Forum **Fehler-Melden** im
[Discord](https://xharig.com/discord), wenn ein Bildschirmfoto schneller geht als eine Beschreibung.

Es wird weitergebaut, was genau, steht in keiner Liste. Was eine Version gebracht hat, liest du im [`CHANGELOG.md`](https://github.com/Xharig/VerseKit/blob/main/CHANGELOG.md) oder im Werkzeug unter **„Was ist neu“**. Was in einer Testfassung steckt, steht in ihrem Release-Text; diese Seite zeigt immer die **veröffentlichte** Version.

Wünsche und Fehlermeldungen gern als [Issue](https://github.com/Xharig/VerseKit/issues) oder im [Discord](https://xharig.com/discord). Vorschläge landen eher im nächsten Bau als Gedankenlesen.

</details>

<details>
<summary><b>Deine Daten und das Netz</b></summary>

🔒 **Es gehört dir.** Kein Konto, keine Anmeldung, keine Cloud. VerseKit liest Dateien, die ohnehin auf deiner Platte liegen. An der Spielinstallation ändert es nur, was du ausdrücklich einschaltest (etwa die Angaben in den Auftragstexten), und sichert die Datei vorher.

**Was VerseKit holt:**

| Wofür | Wie oft |
|---|---|
| Werte und Herkunft von scmdb.net | einmal je Spielversion |
| Rohstoffpreise und Lagerorte von UEX Corp | höchstens einmal am Tag |
| Ruf-Stufen der Ränge aus scunpacked-data | einmal je Spielversion |
| Steckplätze deiner Schiffe von erkul.games | einmal je Spiel-Patch |
| Auftragstexte und Übersetzungen | wenn du sie einschaltest |
| Ob es eine neue Version gibt | im Hintergrund |
| Der Serverstatus von CIG | solange die Seite offen ist |
| Eine Handle-Suche bei RSI | wenn du suchst, oder mit Strg+Enter neu |
| Die Startzeit des Executive Hangars von contestedzonetimers.com | beim Öffnen der Seite Contested Zones, höchstens alle sechs Stunden |

**Was VerseKit sendet:**

| Was | Wann |
|---|---|
| Nutzung zählen: Version, System, Sprache und welche Seiten wie oft geöffnet wurden, ohne Kennung | einmal am Tag, ab Werk an, abschaltbar |
| Abgleich mit dem KRT Profit Basetool oder deinem scmdb.net-Konto | nur, wenn du ihn einschaltest |
| Fehlerbericht | nur, wenn du selbst den Knopf drückst, und du siehst vorher, was drinsteht |

Was genau in jeder Meldung steht, sagt die [Datenschutzerklärung](https://github.com/Xharig/VerseKit/blob/main/PRIVACY.md). **Alles Holen und Senden** lässt sich mit `SC_BP_NO_NET=1` abschalten.

Die Info-Leiste liest ihre Werte auf deinem Rechner aus dem Spielbild: alle paar Sekunden die Ecke oben rechts, und nur, wenn Star Citizen vorn ist.

**Weitergeben:** Gib einfach die Datei von der [Releases-Seite](https://github.com/Xharig/VerseKit/releases) weiter. Der Empfänger braucht nur Star Citizen.

</details>

📖 **[Handbuch](MANUAL.md):** wie VerseKit Baupläne erkennt, welche Dateien es anlegt, eigene Pfade, Windows und Linux auf einem Rechner, Merkliste mit Mustern, Einstellungen und Umgebungsvariablen.

**Neu in Star Citizen?** Meldest du dich mit meinem Empfehlungscode [`STAR-YC7L-S4W2`](https://robertsspaceindustries.com/enlist?referral=STAR-YC7L-S4W2) an, bekommst du nach dem Kauf eines Spielpakets 50.000 UEC Startguthaben im Spiel dazu.

## Star Citizen Fan Content

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/fankit/MadeByTheCommunity_White.png">
    <img alt="Star Citizen: Made by the Community" src="assets/fankit/MadeByTheCommunity_Black.png" width="150" height="150">
  </picture>
</p>

> This is an unofficial Star Citizen fan site, not affiliated with the Cloud Imperium group of
> companies. All content on this site not authored by its host or users are property of their
> respective owners.

VerseKit ist ein inoffizielles, nicht-kommerzielles Fan-Projekt für die
Star-Citizen-Gemeinschaft. Es steht in **keiner Verbindung zu** Cloud Imperium Rights LLC,
Cloud Imperium Rights Ltd. oder Roberts Space Industries und wird von ihnen weder unterstützt
noch gebilligt.

Dieses Projekt verwendet Material aus dem offiziellen
[Star Citizen Fankit](https://robertsspaceindustries.com/fankit). Dieses Material ist für die
Verwendung durch Fans veröffentlicht und darf nur nach den Bedingungen des
**Fankit Agreement**, des **Fan Style Guide** und der
[Roberts Space Industries Terms of Service](https://robertsspaceindustries.com/tos) verwendet
werden, dort besonders der Abschnitt über nutzergenerierte Inhalte (UGC).

> **Star Citizen®, Roberts Space Industries® und Cloud Imperium® sind eingetragene Marken der
> Cloud Imperium Rights LLC.**

Alle übrigen Star-Citizen-Inhalte, Grafiken, Namen, Logos und Marken gehören ihren jeweiligen
Eigentümern. © 2025 Cloud Imperium Rights LLC und Cloud Imperium Rights Ltd.

Offizielle Seite: **[robertsspaceindustries.com](https://robertsspaceindustries.com)**

## Lizenz

**GNU General Public License v3.0**: Volltext in [LICENSE](LICENSE).

Kurz: Du darfst das Programm nutzen, verändern und weitergeben. Wer es weitergibt, verändert oder nicht, muss den Quellcode unter derselben Lizenz mitliefern. Es gibt keine Garantie. Wenn du das Projekt abzweigst, lass die Nennung im Fußbereich stehen oder nenne die ursprüngliche Quelle.

## Dank

- **[scmdb.net](https://scmdb.net)**: Herkunft der Baupläne und Werte zum Gegenprüfen.
- **[UEX Corp](https://uexcorp.space)**: Rohstoffpreise, Shops und Lagerorte.
- **[erkul.games](https://erkul.games)**: Steckplätze der Schiffe.
- **[Star Citizen Wiki](https://github.com/StarCitizenWiki/scunpacked-data)**: Ruf-Stufen der Ränge.
- **Maluh**: das [Fleetmanifest](https://github.com/MaluhTV/SC_Fleetmanifest) hinter der Flottenanalyse.
- **[AlyxOne](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W)**, **[dolkensp](https://github.com/dolkensp/HangarXPLOR)** und **[Drachennarr](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest)**: die Browser-Erweiterungen für den Hangar-Import.
- **[greluc](https://github.com/krt-profit/basetool)**: das Profit Basetool und die [DPoP-Referenz](https://krt-profit.github.io/basetool/) für die Anmeldung.
- **[Luftwerft](https://www.luftwerft.com/)**, **[Dymerz](https://github.com/Dymerz/StarCitizen-Localization)**, **[Thord82](https://github.com/Thord82/Star_citizen_ES)** und **[MrKraken](https://starstrings.app)**: die Übersetzungen und StarStrings.
- **[Kuehlwagen](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query)**: das Vorbild der Handle-Suche und des Log-Monitors.
- **[Zane_DragonBorn](https://robertsspaceindustries.com/community-hub/post/accurate-pyro-contested-zone-maps-c-zs-7l8Ih9mXyPDpu)** und **[Kerast](https://www.reddit.com/r/starcitizen/comments/1i3iyq9/supervisor_exectutive_hangar_maps_pyamsupvisr345/)**: die Karten der Contested Zones.
- **[Appieman](https://contestedzonetimers.com/)**: die Startzeit des Executive Hangars von contestedzonetimers.com.
- **[Lucide](https://lucide.dev)**: die Symbole der Oberfläche.
- **[flag-icons](https://flagicons.lipis.dev)**: die Flaggen der Sprachwahl.

<details>
<summary><b>Quellen und Lizenzen im Einzelnen</b></summary>

Die Herkunft je Bauplan stammt aus der **[Star Citizen Mission DataBase (scmdb.net)](https://scmdb.net)**, ein Hobbyprojekt, das die Spieldaten aufbereitet und frei zugänglich macht. Die Werte zu Art, Größe, Gütegrad und Klasse sowie der Herstellung liest VerseKit zuerst aus den Spieldaten deiner eigenen Installation; scmdb.net dient dabei als Gegenprobe und springt ein, wo sich die Spieldaten nicht lesen lassen. **Herzlichen Dank** dafür! 🙏

> VerseKit **liefert diese Daten nicht mit**, sondern lädt sie auf deinem Rechner direkt bei scmdb.net, so wie es ein Browser täte. scmdb steht unter [CC BY-NC-ND 4.0](https://creativecommons.org/licenses/by-nc-nd/4.0/); eine mitgelieferte Kopie wäre eine Weitergabe und würde sowohl dieser Lizenz als auch der GPL dieses Projekts widersprechen. Abgerufen wird sparsam: nur, wenn eine **neue Spielversion** vorliegt.

Als Grundlage für die Bauplan-Angaben lässt sich **[StarStrings](https://github.com/MrKraken/StarStrings)** von **MrKraken** wählen: aufgeräumte englische Spieltexte, die in vielen Organisationen benutzt werden. **Danke** an MrKraken! 🙏

> Auch StarStrings **liegt nicht bei**, sondern wird auf Wunsch von der Original-Adresse geholt. Eine Lizenz gibt das Projekt nicht an. Umso mehr gilt: Der Text bleibt seiner.

**VerseKit verträgt sich mit anderen Werkzeugen.** StarStrings und andere Werkzeuge kennzeichnen Bauplan-Aufträge ebenfalls, mit derselben Marke `[BP]`. VerseKit setzt deshalb **keine zweite dazu, wo schon eine steht**, und lässt jeden Gegenstandsnamen in Ruhe, der bereits ein Kürzel trägt. Steht schon die Bauplan-Liste eines anderen Werkzeugs im Auftrag, **ersetzt** VerseKit sie durch die eigene, statt eine zweite danebenzustellen, mit den **Kästchen** für deinen eigenen Bestand. Nimmst du die Angaben zurück, steht der Stand des anderen Werkzeugs wieder da, Zeichen für Zeichen.

**Die Übersetzungen** stammen von **[Luftwerft](https://www.luftwerft.com/)**, **[Dymerz](https://github.com/Dymerz/StarCitizen-Localization)** (Deutsch, Français, Italiano, Español, Português, Türkçe) und **[MrKraken](https://starstrings.app)** (StarStrings, Englisch mit Auftragsmarken). Auch sie **liegen nicht bei**. VerseKit holt die gewählte Fassung auf deinem Rechner und hält sie aktuell.

**Luftwerft** bietet im **[SC Launch Configurator](https://www.luftwerft.com/)** Deutsch, Englisch, Français, Italiano, Español, Boarisch, Nederlands und Português für LIVE und PTU an. Der Entwickler hat sie für VerseKit freigegeben. Du wählst sie unter *Übersetzung*. Sie entstehen weitgehend automatisch aus den Spieldaten und werden weiter verfeinert; VerseKit zeigt vor dem Umschalten den Hinweis des Entwicklers. **Danke** an Luftwerft, Dymerz, Thord82 und MrKraken! 🙏

**Die Rohstoffpreise** kommen von **[UEX Corp](https://uexcorp.space)**, ein von Spielern gepflegtes Datenprojekt. Damit steht neben jeder fehlenden Zutat, was sie kostet, oder dass sie sich gar nicht kaufen lässt. Auch diese Daten **liegen nicht bei**, sondern werden auf deinem Rechner geholt, höchstens einmal am Tag. **Danke** an UEX Corp! 🙏

**Die Ruf-Stufen der Ränge** („Gildenmitglied [ab 10.000]") stammen aus **[scunpacked-data](https://github.com/StarCitizenWiki/scunpacked-data)** des **Star Citizen Wiki**: die Spieldateien, je Patch entpackt und frei zugänglich gemacht. Auch sie **liegen nicht bei**, sondern werden auf deinem Rechner geholt, einmal je Spielversion. **Danke** an das Star Citizen Wiki! 🙏

**Die Steckplätze der Schiffe** kommen von **[erkul.games](https://erkul.games)**, dem Auslegungs-Werkzeug, das die Star-Citizen-Gemeinschaft seit Jahren benutzt. Ohne diese Daten könnte VerseKit die Frage nicht beantworten, die auf jeden neuen Bauplan folgt: *passt das Teil überhaupt in eines meiner Schiffe?* Auch sie **liegen nicht bei**, sondern werden auf deinem Rechner geholt, und zwar nur für die Schiffe, die du wirklich eingetragen hast, dazu die Listen der Racks, Raketen, Bomben und Lackierungen, und nur einmal je Spiel-Patch. **Danke** an erkul.games! 🙏

**Den Hangar-Import** ermöglichen Browser-Erweiterungen: der **[Hangar-Exporter](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest)** von **Drachennarr** (GPL-3.0), der den Hangar als Datei speichert oder ohne Datei direkt an VerseKit auf diesem Rechner schickt, die **[Star Citizen: Hangar Extension](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W)** von **AlyxOne** (MIT-Lizenz, gepflegt, empfohlen) und der ursprüngliche **[Star Citizen Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR)** von **dolkensp** (MIT-Lizenz). Sie holen deinen Pledge-Store, die Dateien liest VerseKit ein. Ohne sie müsste jedes Schiff von Hand eingetragen werden. **Danke** dafür! 🙏

**Das Profit Basetool** von **greluc** (GPL-3.0) gleicht sich mit VerseKit ab, und seine **[DPoP-Referenz](https://krt-profit.github.io/basetool/)** (MIT) signiert die Anmeldung, ohne dass der Schlüssel deinen Rechner verlässt. Die Referenz ist im Programm eingebaut. **Danke** an greluc! 🙏

**Die Handle-Suche und der Log-Monitor** sind dem **[Star Citizen Handle Query](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query)** von **Kuehlwagen** (MIT) nachgebaut, mit eigenem Code. **Danke** für Idee und Vorbild! 🙏

**Die Karten der Contested Zones** sind in VerseKit eingebaut: Checkmate, Orbituary und Ruin Station von **Zane_DragonBorn** (zum [Beitrag](https://robertsspaceindustries.com/community-hub/post/accurate-pyro-contested-zone-maps-c-zs-7l8Ih9mXyPDpu)), PYAM-EXHANG-0-1 und PYAM-SUPVISR-3-4/5 von **Kerast** (zum [Beitrag](https://www.reddit.com/r/starcitizen/comments/1i3iyq9/supervisor_exectutive_hangar_maps_pyamsupvisr345/)). Beide haben sie zur freien Nutzung freigegeben, wenn sie genannt werden. Die Startzeit des Executive Hangars liest VerseKit von **[contestedzonetimers.com](https://contestedzonetimers.com/)**; **Appieman** betreibt die Seite und hat das erlaubt. **Danke** an alle drei! 🙏

Die Symbole der Oberfläche stammen aus dem **[Lucide](https://lucide.dev)**-Satz (ISC-Lizenz), alle auf demselben Raster mit gleicher Strichstärke gezeichnet, weshalb sie unter Windows, Linux und macOS gleich aussehen. **Danke** an die Lucide-Gemeinschaft! 🙏 Der Lizenztext liegt bei (`assets/symbole/LIZENZ.txt`) und steht im Werkzeug unter **Danke & Lizenzen**.

Die Flaggen an der Sprachwahl stammen aus **[flag-icons](https://flagicons.lipis.dev)** (MIT-Lizenz, © 2013 Panayiotis Lipiridis). **Danke** dafür! 🙏

VerseKit ist ein eigenständiges, inoffizielles Zusatz-Tool und steht in **keiner** offiziellen Verbindung zu Cloud Imperium Games. Alle Marken- und Projektnamen gehören ihren jeweiligen Eigentümern.

</details>

<div align="center">

[![Xharig](https://github.com/Xharig.png?size=80)](https://github.com/Xharig)

**Xharig**: Entwicklung und Gestaltung dieses Projekts

<sub>Gefällt Dir das Werkzeug? <a href="https://ko-fi.com/xharig">Ko-fi</a> ☕</sub>

</div>

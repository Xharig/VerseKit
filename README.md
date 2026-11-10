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
- **Angaben im Spiel:** Die Auftragstexte zeigen, welche Baupläne ein Auftrag bringt und welche du schon hast. An jedem herstellbaren Gegenstand im Terminal steht, ob du den Bauplan hast, woher er kommt und was das Herstellen braucht.
- **Herstellen und Lager:** Zutaten, Herstellzeit und was dein Material aus den Werten macht, unter „Für Schiff“ nur Teile, die in eines deiner Schiffe passen. Dazu dein Lager, was dir noch fehlt, Käufe und Verkäufe im Laden als Vorschlag aus dem Log, Fabrikator-Aufträge mit Restzeit und der Umzug nach dem Patch.
- **Bergbau und Raffinerie:** Fundorte je Rohstoff, die beste Raffinerie und die passende Verarbeitungsmethode. Unter Windows liest der Scanner die Signatur beim Scannen selbst ab, und Alt+Umschalt+R liest das Raffinerie-Terminal, auch im deutschen Spiel und bei langen Listen.
- **Handel:** Shops, die besten Ankäufer und Handelsrouten mit echtem Gewinn.
- **Dein Hangar:** Schiffe aus dem Pledge-Store holen, jeden Steckplatz einzeln ausstatten (aus den Spieldaten deiner Installation) und die Ausstattung als Code oder Datei teilen.
- **Handle-Suche:** Wer steckt hinter einem Handle oder einer Org? Name, Organisationen und Rang, dazu Freund, Feind oder Beobachtet markieren, direkt aus der Info-Leiste über dem Spiel.
- **Übersetzung je Kanal:** Deutsch, Französisch, Spanisch, Bayrisch, Niederländisch und mehr, getrennt für LIVE, HOTFIX, PTU, EPTU und Tech-Preview.
- **Spiel-Einstellungen von außen:** Steuerung, Grafik samt Shader-Cache, Achsen und Sichtfeld, ohne ins Spiel zu wechseln.

## Herunterladen

| System | Datei | |
|---|---|---|
| **Windows** | [`VerseKit-Setup.exe`](https://github.com/Xharig/VerseKit/releases/latest) | Installer mit Startmenü-Eintrag, sauber deinstallierbar |
| **Linux** | [`VerseKit-x86_64.AppImage`](https://github.com/Xharig/VerseKit/releases/latest) | Eine Datei, einmal ausführbar machen |

Starten, und ein Assistent richtet in etwa einer Minute alles ein. VerseKit ist kostenlos und quelloffen, braucht kein Konto und hält sich selbst aktuell: Es sieht alle zehn Minuten nach und spielt eine neue Version ein, sobald Star Citizen geschlossen ist. Die Warnung von Windows beim ersten Start ist kein Virenfund, mehr dazu unten unter *Installation im Einzelnen*.

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
<sub>Das Overlay: schmal, über dem Spiel, Durchsichtigkeit einstellbar</sub>
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

VerseKit schreibt in die Auftragstexte des Spiels, **welche** Baupläne ein Auftrag ausschüttet, mit `[x]` für das, was du schon hast. Im Titel steht `[BP]` (haben alle Stufen dieselbe Rufschwelle, gleich mit dem nötigen Ruf: `[BP 10.000 Rep]`). Die Namen stehen in der Beschreibung, getrennt nach Stanton, Pyro und Nyx, wo ein Auftrag dort Verschiedenes hergibt.

<table>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-teils.jpg" alt="Auftrag mit teilweise vorhandenen Bauplänen" width="100%"><br>
<sub><b>3 von 6</b>: <code>[x]</code> hast du, <code>[&nbsp;&nbsp;]</code> fehlt noch</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-ingame-keine.jpg" alt="Auftrag ohne vorhandene Baupläne" width="100%"><br>
<sub><b>0 von 12</b>: noch nichts dabei, was du hast</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Mein Hangar</b></summary>

Ein neuer Bauplan wirft die Frage auf: **passt das Teil in eines meiner Schiffe?** Dafür muss VerseKit wissen, was in deinem Hangar steht. Das Spiel schreibt es nirgends auf.

> **So kommt dein Hangar ins Werkzeug:** Die Browser-Erweiterung **Star Citizen: Hangar Extension** von AlyxOne setzt auf deiner Pledge-Seite Export-Knöpfe für [Firefox](https://addons.mozilla.org/de/firefox/addon/star-citizen-hangar-extension/), [Chrome](https://chromewebstore.google.com/detail/star-citizen-hangar-exten/jeoidbfcbdplammndhpflojejmpkjcfm) und [Edge](https://microsoftedge.microsoft.com/addons/detail/star-citizen-hangar-exte/agahppbhmogalnjlaolmjbbemlcjbioe). Lade **beide** Dateien herunter und wähle sie unter **Mein Hangar → „Schiffe holen oder eintragen“ → „Oder eine Datei einlesen“** nacheinander aus. Die JSON bringt Paketzugehörigkeit und Versicherung, die CSV Kaufdatum, Preis und ob ein Pledge schmelzbar ist. Doppelt wird nichts. Im Spiel gekaufte Schiffe kennt kein Export, die trägst du von Hand ein.

**Flottenanalyse:** Über der Schiffsliste klappt auf, welche Spielbereiche deine Schiffe abdecken (Kampf, Versorgung, Reisen), wo eine Lücke ist und welche Snubs ohne Mutterschiff dastehen. An jedem Echtgeld-Schiff steht, ob es sich schmelzen oder per CCU upgraden lässt. Die Spielbereiche stammen aus dem [Fleetmanifest](https://github.com/MaluhTV/SC_Fleetmanifest) von Maluh: eine Einschätzung, keine Spieldaten.

<table>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-hangar.png" alt="Mein Hangar mit vier eingetragenen Schiffen" width="100%"><br>
<sub><b>Mein Hangar</b>: aus dem Pledge-Store geholt oder von Hand eingetragen</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-asop-ingame.jpg" alt="Der Flottenmanager im Spiel mit drei selbst benannten Schiffen" width="100%"><br>
<sub><b>Namen im Spiel</b>: eigene Namen im Abrufterminal statt der Werksnamen. Markierte Schiffe rutschen im alphabetisch sortierten Flottenmanager nach oben</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-einkaufsliste.png" alt="Einkaufsliste: Rechnung über alle Schiffe" width="100%"><br>
<sub><b>Einkaufsliste</b>: die Rechnung über alle Schiffe, mit Summe und Einkaufsroute</sub>
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
<sub><b>Mein Lager</b>: Rohstoffe, Verkaufsware und Items an einer Stelle</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-bergbau.png" alt="Bergbau mit den Fundorten von Iron" width="100%"><br>
<sub><b>Bergbau</b>: Rohstoff eintippen und sehen, wo er liegt, oder den Scan-Wert eingeben</sub>
</td>
</tr>
<tr>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-farmliste.png" alt="Was ich farmen muss" width="100%"><br>
<sub><b>Was ich farmen muss</b>: welche Rohstoffe für alles Geplante noch fehlen, samt Fundorten</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-raffinerien.png" alt="Raffinerien" width="100%"><br>
<sub><b>Raffinerien</b>: Aufschlag und Abschlag je Material und Station</sub>
</td>
</tr>
</table>

</details>

<details>
<summary><b>Handel & Einkauf</b></summary>

Kaufen und verkaufen: **Wo steht ein Teil oder Schiff im Regal, wo werde ich die Ladung los, und was bringt sie je SCU?**

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
<sub><b>Mein Lager, zum Verkaufen</b>: Ladung mit Ankaufspreis und Gewinn</sub>
</td>
<td width="50%" valign="top" align="center">
<img src="assets/screenshot-verkauf.png" alt="Kaufen & Verkaufen mit den besten Ankaufsorten" width="100%"><br>
<sub><b>Kaufen & Verkaufen</b>: die besten Ankäufer, oder umgeschaltet die günstigsten Einkaufsorte</sub>
</td>
</tr>
<tr>
<td colspan="2" valign="top" align="center">
<img src="assets/screenshot-routen.png" alt="Handelsrouten" width="100%"><br>
<sub><b>Routen</b>: die lohnendsten Fahrten von deinem Standort aus</sub>
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
<sub><b>Über</b>: stabile Version oder Testversionen</sub>
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
<sub><b>Aufträge &amp; Protokoll</b>: welchen Auftrag du wann gespielt hast, auch wenn das Spiel die Protokolle gelöscht hat</sub>
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

Unter **Spiel-Einstellungen → Achsen & Kurven** stellst du ein, wie scharf jede Stick-Achse reagiert. Drei Werte, und jeder verbiegt die Kurve anders:

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

Was im Alltag den Unterschied macht:

- **Du bleibst im Spiel.** Das Overlay liegt über Star Citizen. Kein zweites Fenster, kein Alt-Tab, kein Browser.
- **Es kennt deinen Bestand.** VerseKit führt ihn selbst und liest beim ersten Start die aufgehobenen Spielprotokolle nach. Bleibt eine Lücke, sagt es das, statt eine unvollständige Liste als vollständig auszugeben.
- **Es sagt, woher das Fehlende kommt.** Fraktion, Auftrag, Rang und Belohnung, der leichteste Weg zuerst: „X gibt es bei Foxwell ab Veteran Contractor“.
- **Kein Konto, keine Cloud.** Es liest Dateien, die ohnehin auf deiner Platte liegen, und schreibt nur ins Spiel, was du einschaltest. Mehr unter *Deine Daten und das Netz*.

</details>

<details>
<summary><b>Alle Funktionen</b></summary>

| | |
|---|---|
| <img src="assets/symbole/22/karten-gruen.png" width="22" alt=""> **Übersicht** | Die Startseite zeigt jeden Bereich als Kachel. Die Seiten, die du ständig brauchst, heftest du mit der Nadel an |
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Sofort-Meldung** | Liest die Star-Citizen-`Game.log` mit. Der Bauplan steht **in Sekunden** in der Liste |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Tastenkürzel** | **Alt+Umschalt+V** öffnet und schließt das Overlay mit der Bauplan-Liste, auch im Vollbild. **Alt+Umschalt+R** liest das Raffinerie-Terminal, **Alt+Umschalt+H** setzt den Cursor ins Suchfeld der Handle-Suche (Enter sucht, Esc gibt die Tastatur ans Spiel zurück), **Alt+Umschalt+B** nimmt das Spielbild für den Suchrahmen auf, **Alt+Umschalt+F** holt das Hauptfenster nach vorn oder öffnet es wieder. Alle fünf sind ab Werk an; ändern oder ausschalten kannst du sie unter *Einstellungen → Overlay*. Statt einer Kombination geht auch eine einzelne Taste, die du nie tippst, etwa F13. Mitgehört wird nichts. Unter Wayland legst du den dort angezeigten Befehl im Desktop auf ein Kürzel, mehr im [Handbuch](MANUAL.md#unter-linux) |
| <img src="assets/symbole/22/liste-gruen.png" width="22" alt=""> **Bauplan-Liste** | Alle Baupläne durchsuchen, nach Art gruppiert. Filter: *alle, habe ich, fehlt mir, Merkliste, vorgemerkt, neu im Spiel*. Häkchen per Klick. Der Reiter **Bauplan-Fortschritt** zeigt je Art, wie weit du bist. Der Filter **Erhalten** (heute bis dieser Monat) listet neueste zuerst, mit Datum und Quelle |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Herkunft je Bauplan** | Der Knopf **„Woher?“** zeigt Fraktion, Auftrag, Rang und Belohnung, der leichteste Weg zuerst. Aus der **Herstellung** führt ein Knopf direkt dorthin |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Laufende Aufträge** | Nimmst du einen Auftrag an, steht sofort da, ob Baupläne dabei sind und **welche dir fehlen**. Kennt der Katalog den Auftrag nicht, wird geschwiegen statt geraten. Unter jedem laufenden Auftrag stehen die **offenen Zwischenziele** aus demselben Protokoll |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Aufträge & Protokoll** | Welche Aufträge du wann gespielt hast und **welcher Bauplan dabei herauskam**, auch umgekehrt. Beim ersten Start schon gefüllt aus den aufgehobenen Protokollen, danach wächst es mit, auch wenn das Spiel seine Logs löscht. Der Käfer an der Zeile sucht den Auftrag im **Issue Council**, immer mit dem englischen Titel |
| <img src="assets/symbole/22/blitz-gruen.png" width="22" alt=""> **Herstellung** | Zutaten mit Menge, Herstellzeit und ob du den Bauplan hast. Oben die Produktwerte als Grundwert, gebauter Wert und Änderung (Schaden, DPS, Schild, Kühlung, Rüstung, bei Waffen die Integrität), gelesen aus den Spieldaten deiner Installation und gegengeprüft mit scmdb.net. Ein Klick auf einen Rohstoff springt zu seinen Fundorten. Unter **„Für Schiff“** wählst du ein Schiff aus Mein Hangar, und die Liste zeigt nur Teile, die in eine Halterung passen; auf der Schiffskarte im Hangar springt „Herstellbare Teile“ direkt dorthin |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Fabrikator-Aufträge** | Auf der Seite Herstellung trägst du ein, was am Fabrikator läuft, mit der Zeit aus dem Spiel. Zwei laufen gleichzeitig, die übrigen rücken nach. VerseKit zeigt die Restzeit, meldet „fertig“ und zeigt die Aufträge auf Wunsch im Overlay |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Bergbau** | Rohstoff eintippen zeigt seine Fundorte, Ort eintippen zeigt, was es dort gibt. Je Fundort die **Konzentration**, die Liste steht nach Ergiebigkeit. **„Womit?“** trennt Schiff, Fahrzeug und Handabbau. Dazu der Raffinerie-Vergleich je Erz und die **Scan-Signatur**. **Unter Windows liest VerseKit die Signatur beim Scannen selbst ab** und zeigt im Overlay das Erz, auch bei ROC, Boden und Handabbau; bei Asteroiden zählt es die Brocken. Ein und aus mit dem Auge in der Leiste. Das **Signatur-Fenster** steht „Über der Signatur“ oder „In der Info-Leiste“, Platz, Größe und Anzeigedauer stellst du unter *Einstellungen → Overlay* ein. Der **Suchrahmen** sagt, wo im Schiffs-HUD die Signatur steht: „Spielbild holen“, Rahmen aufziehen, mit „Rahmen im Spiel zeigen“ prüfen. „Spielbild holen“ wartet 5, 10, 20 oder 30 Sekunden (ab Werk 10) und zählt sichtbar herunter, so kannst du im Spiel erst den Scanner anschalten und scannen. „Warten abbrechen“ hält es an. **Alt+Umschalt+B** nimmt das Bild im Spiel sofort auf |
| <img src="assets/symbole/22/herkunft-gruen.png" width="22" alt=""> **Welche Verarbeitungsmethode?** | Das Terminal bietet neun Methoden mit einer Zeile je Methode. Sag, was dir wichtig ist (**Ertrag, Kosten oder Geschwindigkeit**), und du bekommst die passende Methode. Ausbeute, Tempo und Kosten liest VerseKit aus deiner Data.p4k, so wie das Terminal sie zeigt. Lohnt sich eine Methode nie, steht das auch da |
| <img src="assets/symbole/22/raffinerie-gruen.png" width="22" alt=""> **Raffinerien** | **Welche Station am meisten aus deinem Erz macht, als eine Tafel.** Eine Zeile je Material, eine Spalte je Raffinerie, in der Zelle der Aufschlag oder Abschlag. Grün ist der beste Wert der Zeile, Rot kostet Ausbeute. Über den Spalten steht das System. Die Gegenrichtung zu „Welche Verarbeitungsmethode?“: dort wählst du die Methode, hier die Station |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Mein Lager** | **Dein ganzes Lager an einer Stelle**, in der Gruppe **Flotte & Logistik**. Reiter **Material** und **Items**, Filter nach **Art**, **Ort** und **Zweck** (herstellen oder verkaufen), dazu eine Suche. Eine Zeile je Posten, bei Gegenständen mit Kürzel wie `(Mil/3/A)`. Du trägst Material, Menge, Qualität und Lagerort ein, gleiche Materialien stehen gebündelt mit Gesamtmenge und Güte-Spanne. **Jeder Posten ist eine Kiste wie im Spiel:** Beim Herstellen kommt jede Portion aus einer Kiste, in der genug liegt, und eine Kiste, die für kein Rezept mehr reicht, steht als **„Rest: nur verkaufen“** da. Im Rezept steht, was fehlt, und ein Knopf zieht die Zutaten ab, in der eingestellten Güte (Regler, Eingabe oder **Bestes / Schlechtestes Material**). Du siehst, was *dein* Material aus den Werten macht. Alles ohne Netz und ohne Basetool. Bei der Raffinerie-Ausbeute startet das Feld Lagerort mit dem zuletzt benutzten Ort |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Preise** | Was ein Rohstoff am Terminal kostet und einbringt. Die Zahlen kommen von **[UEX Corp](https://uexcorp.space/)** und frischen sich täglich auf. Ohne Netz bleibt das Feld leer |
| <img src="assets/symbole/22/handelslager-gruen.png" width="22" alt=""> **Mein Lager: zum Verkaufen** | Ladung im Laderaum, im selben Lager mit dem Abzeichen **Verkauf** und eigenem Filter. Ware, Ort und SCU eintragen, im Mengenfeld darfst du rechnen (`100+5`). Statt einer Güte gibt es den Haken **„als gestohlen markiert“**. Trägst du ein, was du **je SCU bezahlt** hast, siehst du den **Gewinn** je Posten und Verkaufsort |
| <img src="assets/symbole/22/verkauf-gruen.png" width="22" alt=""> **Kaufen & Verkaufen** | Oben schaltest du zwischen **Verkaufen** und **Kaufen** um. **Kaufen:** Ware suchen und sehen, welche Terminals sie verkaufen, das günstigste zuerst, mit Vorrat (leer und fast leer sind markiert), Kistengrößen und Alter der Meldung. **Verkaufen:** was deine Ware **je SCU** bringt, für **mehrere Waren auf einmal**. Sortiert wird danach, **wie viele deiner Waren ein Ort abnimmt**, nicht nach dem höchsten Preis. Ist die Ladung als gestohlen markiert, blendet der Reiter auf die 15 Terminals ein, die keine Fragen stellen |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Mein Hangar** | **Welche Schiffe dir gehören, und ob ein Bauplan hineinpasst.** Du holst den Hangar in einem Zug aus dem Pledge-Store: JSON **und** CSV der Browser-Erweiterung [Star Citizen: Hangar Extension](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W) mit LTI oder Versicherungslaufzeit je Schiff (der ältere [Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR) wird weiter gelesen). Auch der [Hangar-Exporter](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest) von Drachennarr liest den Hangar von der RSI-Seite. Seine Firefox-Erweiterung legt der Installer gleich mit ab, und auf der Hangarseite und im Assistenten gibt es je einen Knopf für Firefox, Chrome und Edge. Schiffslisten aus **Fleetview** (CCU Game), **Fleetyards** und **StarJump FleetViewer** liest VerseKit auch, sie ergänzen nur. Im Spiel gekaufte Schiffe trägst du von Hand ein. In der Herstellung steht dann unter jedem Bauplan, **in welche deiner Schiffe das Teil passt**. Die Steckplätze liest VerseKit aus den Spieldaten deiner Installation, solange Star Citizen geschlossen ist. Nur für Schiffe, die dort fehlen, springen die Daten von [erkul.games](https://erkul.games) ein, nur für Schiffe aus deinem Hangar und einmal je Spiel-Patch |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Ausstattung** (im Hangar) | **Jedes Schiff zeigt seine Plätze bis ganz unten:** Turm und Waffen, Gestell und Raketen, Bergbaukopf und Module. Gleiche Plätze stehen in einer Zeile, der Pfeil öffnet jeden Platz für sich, jede Liste endet mit „Leer lassen“. Plätze, die man im Spiel nicht umrüsten kann, stehen als „fest verbaut“ da. **„Ausstattung teilen“** kopiert sie als Code für Discord oder einen Chat, **„Als Datei speichern“** legt alle Ausstattungen eines Schiffs in eine Textdatei. Hochgeladen wird nichts, beim Einfügen siehst du vorher, was ankommt. Die Ausstattung eines anderen Schiffs desselben Typs lässt sich übernehmen |
| <img src="assets/symbole/22/hangar-gruen.png" width="22" alt=""> **Namen im Spiel** (im Hangar) | **Deine Schiffe heißen im Abrufterminal (ASOP) so, wie du sie nennst.** Ein Sternchen davor markiert eines, ohne es umzutaufen. Das Feld steht im Hangar an jedem Schiff, die Suche findet auch deinen Namen. ⚠ Ein Name gehört zum **Muster**, nicht zum einzelnen Schiff: Zwei gleiche Hornets bekommen denselben Namen, Abwandlungen wie F7C, F7C-M, Mk I und Mk II lassen sich unterscheiden. Der Werksname kommt jederzeit zeichengenau zurück |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Ruf-Stufen** | **Im Reputationsmenü steht hinter jedem Rang, ab wie viel Ruf er beginnt**: „Gildenmitglied [ab 10.000]“. Für jede Fraktion mit Rängen, gelesen aus den Spieldaten deiner Installation. **Jeder Auftrag zeigt in seiner Beschreibung Wartezeit, Buy-in, Ruf und Rang.** Abschaltbar unter „Spiel“ |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **Bauplan-Angaben in den Beschreibungen** | **Unter der Beschreibung jedes herstellbaren Gegenstands steht im Spiel, ob du den Bauplan hast**, und wenn nicht, aus welchem Auftrag er kommt. Dazu Zutaten, Herstellzeit und der günstigste Kaufort. Fremder Text bleibt stehen, der Block kommt darunter. Fehlt dem Spiel der Name eines herstellbaren Teils, schreibt VerseKit einen Notnamen aus dem Klassennamen, der beim Zurücknehmen wieder verschwindet. Abschaltbar unter „Spiel“ |
| <img src="assets/symbole/22/wunschliste-gruen.png" width="22" alt=""> **Geplant** (im Hangar) | **Was du dir vornimmst, mit Preis, Ort und planbarer Ausstattung.** Ein Schiff unter *Geplant* zeigt, was es im Spiel kostet und wo es verkauft wird. Die Ausstattung planst du, **bevor** du es besitzt. Ein geplantes Schiff zählt nicht bei „passt in dein Schiff“. Kaufst du es, schiebst du es samt Ausstattung nach *Mein Hangar*, nach einem Wipe mit „Auf Geplant setzen“ zurück |
| <img src="assets/symbole/22/einkaufsliste-gruen.png" width="22" alt=""> **Einkaufsliste** | **Die Rechnung über alle deine Schiffe.** Leg je Steckplatz fest, was dort sitzen soll. An jedem Teil stehen **Güte und Klasse** („A · Militär · nur über Bauplan“), auch für **militärische Teile**, die es in keinem Laden gibt. Je Posten wählst du **kaufen oder selbst herstellen**, darunter steht die Summe samt Einkaufsroute mit möglichst wenigen Stopps. Eingebautes hakst du ab, es fällt aus Liste und Summe. Ist der letzte Posten eines Schiffs abgehakt, steht es auf **Einsatzbereit**. Stellst du ein Schiff auf Einsatzbereit, fragt VerseKit, ob seine offenen Posten als eingebaut gelten |
| <img src="assets/symbole/22/farmliste-gruen.png" width="22" alt=""> **Was ich farmen muss** | **Dein Lager gegen alles gerechnet, was du bauen willst**, über alle Posten zusammen statt Rezept für Rezept. Erz mit zu geringer Güte zählt nicht als Bestand, wird aber genannt. Zu jedem fehlenden Rohstoff stehen die **ergiebigsten Fundorte** dabei und Orte, an denen du **mehrere auf einmal** bekommst. Vorgemerkte Teile bekommen eine **Stückzahl**. Ein Klick auf einen Rohstoff zeigt ihn im Bergbau |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Was steckt drin?** | **Was ein Wrack ab Werk an Bord hat und was es im Laden wert ist.** Schiff eintippen, und du siehst jedes verbaute Teil mit Ladenwert. ⚠ NPC-Wracks sind lootbar. Spielerschiffe werden unbrauchbar, sobald die Versicherung beansprucht wird, dann sind auch ausgebaute Teile wertlos. Der Hinweis steht **vor** jeder Zahl |
| <img src="assets/symbole/22/zerlegen-gruen.png" width="22" alt=""> **Lohnt das Zerlegen?** | **Was der Fabricator zurückgibt, bevor du den Schneidbrenner ansetzt.** 50 % der Materialien, aber **sechs Rohstoffe kommen nie wieder**, darunter Quantainium und Stileron. Die Werte kommen aus den Spieldaten |
| <img src="assets/symbole/22/blickwinkel-gruen.png" width="22" alt=""> **FOV** | **Sitzt du richtig vor deinem Bildschirm?** Miss die Bildschirmbreite mit einer Bankkarte, gib deinen Sitzabstand an, und du bekommst den Blickwinkel, den dein Aufbau hergibt, samt Bewertung |
| <img src="assets/symbole/22/laeden-gruen.png" width="22" alt=""> **Shops** | **Wo steht das fertige Teil im Regal, und was kostet es dort?** Teile aller Warengruppen, auch Raketen, Bomben, Munition und Waffenaufsätze, dazu Getränke, Essen, Medizin, Kleidung, Lackierungen und Einrichtung, und Schiffe zum Kaufen und Mieten. Mit dem Feld **Ort** siehst du nur, was es dort gibt, etwa in Levski. Wählst du einen Ort, siehst du seine Shops, und ein Klick zeigt das ganze Angebot eines Shops mit Preis. Einen Shop wie CenterMass findest du auch über seinen Namen im Feld „Ort oder Shop“. Teil anklicken zeigt jeden Laden mit Preis, Ort und System, den günstigsten oben, gebrauchte Ware mit Zustand. **Jedes Teil legst du mit einer Menge in den Einkaufskorb**: Die Einkaufsliste rechnet die Kosten, zeigt alle Einkaufsorte, plant die Route mit den wenigsten Stopps und lässt dich Stopps abhaken. Eine Zusammenstellung speicherst du als Vorlage. An jeder Zeile stehen **Klasse, Größe, Güte und Hersteller**, danach filterst du |
| <img src="assets/symbole/22/routen-gruen.png" width="22" alt=""> **Routen** | **Handelsrouten mit echtem Gewinn statt einer Preisliste.** Sag, wo du stehst, wie viel Frachtraum und Geld du hast, und du bekommst die Fahrt mit Einkauf, Verkauf und Gewinn. Auf Wunsch über mehrere Stationen, nach Gewinn oder kürzestem Weg sortiert, auch als Rundreise. Ein Knopf sucht die beste Route **im ganzen Verse**. Wählst du dein Schiff, kommt der Frachtraum von selbst |
| <img src="assets/symbole/22/joysticks-gruen.png" width="22" alt=""> **Steuerung** | **Was liegt auf welcher Taste, und umbelegen ohne ins Spiel zu wechseln.** Die Belegung im Klartext („Schleudersitz“ statt `v_eject`) für Joystick, Tastatur, Maus und Gamepad in einer durchsuchbaren Liste: *von mir geändert*, *alles* oder *Werkseinstellung*. Zum **Neubelegen** eine Zeile anklicken und den Knopf drücken. Ist die Eingabe schon vergeben, steht das vorher da. Dazu, welcher Stick welche Nummer hat und ob deine Belegung noch auf ein angeschlossenes Gerät zeigt. Funktioniert für **jedes** Gerät |
| <img src="assets/symbole/22/einrichtung-gruen.png" width="22" alt=""> **Einrichtungsassistent** | Führt beim ersten Start durch alles Wichtige und ist **jederzeit wiederholbar** |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Sicherung** | Alles, was nur du hast, in **eine Datei**: Bauplan-Bestand, beide Lager, Auftrags-Protokoll, Merkliste, Einstellungen und Steuerung. Derselbe Knopf spielt sie wieder ein. Beim Einspielen fragt VerseKit getrennt, ob Profile und aktive Belegung ersetzt werden. Bei laufendem Spiel passiert nichts, und vorher entsteht immer eine Kopie. Die heruntergeladenen Nachschlagewerke bleiben draußen. **Beim ersten Start von VerseKit 5** sichert es deine Daten aus 4.12 im Ordner „Sicherungen“, bevor es etwas ändert |
| <img src="assets/symbole/22/anzeige-gruen.png" width="22" alt=""> **Grafik** | Renderer, Fenstermodus, VSync, Upscaling mit Technik und Modell, Bewegungsunschärfe, Filmkörnung, Schärfen und chromatische Aberration, ohne ins Spiel zu müssen. Dazu der Leistungsindex und die Einstufung von Niedrig bis Ultra, mit Datum der letzten Messung. Geschrieben wird nur bei geschlossenem Spiel, vorher legt VerseKit eine Sicherung der Datei an |
| <img src="assets/symbole/22/sicherung-gruen.png" width="22" alt=""> **Shader-Cache leeren** | Grafikfehler oder Ruckler nach einem Patch? Die Karte auf *Spiel-Einstellungen → Grafik* leert die Shader-Speicher von Star Citizen und der Grafiktreiber sowie alte Absturzberichte und zeigt Größe und Zeitpunkt der letzten Leerung. „Nur alte Shader löschen“ nimmt allein Ordner älterer Versionen mit, „Alles löschen“ ist die zweite Wahl. Die Ordner findet VerseKit selbst, unter Linux auch im Wine-Präfix. Deine Grafikeinstellungen bleiben, und solange das Spiel läuft, passiert nichts |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **Katalog-Wache** | Meldet auch, wenn im **Spiel** etwas neu craftbar wird, weil CIG einen Bauplan nachreicht, den es vorher nicht gab |
| <img src="assets/symbole/22/serverstatus-gruen.png" width="22" alt=""> **Serverstatus** | Eigene Seite: **Läuft Star Citizen gerade?** Zeigt die drei Systeme und die Meldungen der letzten zwei Monate im Wortlaut von CIG. Frischt sich jede Minute auf. Die Angaben sind von Hand gepflegt, keine Messung |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Geänderte Spielwerte** | Eigene Seite: **was ein Patch an Schiffen, Waffen, Quantum-Antrieben und Komponenten verändert hat**, Feld für Feld mit altem und neuem Wert. Gestiegen grün, gefallen rot. Eine Karte zeigt die **größten Änderungen** des Patches, neue Patches stehen als ungelesen da. Die Quelle hebt nur die letzten zehn Patches auf, was du geholt hast, bleibt bei dir |
| <img src="assets/symbole/18/punkt-blau.png" width="22" alt=""> **Neu im Spiel** | Eigener Filter: **nur das, was mit dem aktuellen Patch dazukam**. Jeder Bauplan trägt die Spielversion, in der es ihn zuerst gab. Ein Auswahlfeld **Patch** zeigt zusätzlich frühere Patches |
| <img src="assets/symbole/18/gemerkt-gruen.png" width="22" alt=""> **Merkliste** | Klick auf den Stern in der Liste: Taucht der Bauplan auf, wird er auffällig gemeldet und **auf der Merkliste abgehakt**. So zeigt der Fortschritt, wie weit du mit deinen gemerkten Bauplänen bist |
| <img src="assets/symbole/22/kuerzel-gruen.png" width="22" alt=""> **Klasse · Size · Grade** | Kompakt-Kürzel `Klasse/Size/Grade` je Bauplan, etwa `M/1/A` (Military · Size 1 · Grade A) |
| <img src="assets/symbole/22/ton-gruen.png" width="22" alt=""> **Signalton** | Kurzer Ton bei jedem Neuzugang, du musst nicht aufs Fenster schauen. Unter *Einstellungen → Allgemein* stellst du die Lautstärke aller Töne ein und den Ton „Raffinerie fertig“ an oder aus |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Info-Leiste** | **Eine schmale Leiste über dem laufenden Spiel**, ab Werk an, abschaltbar unter *Einstellungen → Overlay → Info-Leiste*. Sie zeigt den Server-Shard und auf Wunsch, mit eigenem Schalter, **deine FPS**, die **Server-FPS** und die **Spieler** auf Shard und Server. VerseKit liest die Werte aus dem Spielbild, dafür braucht das Spiel `r_DisplayInfo 1` (FPS) bzw. `r_DisplayInfo 3` (Spieler). Beim Bergbau steht rechts das Scanner-Ergebnis. Bei offenem Schloss ziehst du die Leiste an ihren Platz (oben links, Mitte oder rechts). Mit **„Auch ohne laufendes Spiel zeigen“** bleibt sie ohne Star Citizen sichtbar |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Handle-Suche** | **Wer steckt hinter einem Handle oder einer Org?** Im Feld der Info-Leiste und auf der Seite *Handle-Suche* (Gruppe *Info*) mit den Reitern **Suche** und **Beziehungen**. **Enter** sucht, **Strg+Enter** holt die Daten neu von RSI, **Alt+Umschalt+H** setzt dich aus dem Spiel ins Suchfeld und markiert den letzten Handle, du kannst gleich drübertippen. Das Feld nimmt einen Handle, die SID oder den Namen einer Org. Die Karte zeigt Name, Organisationen, Rang, SID und Profil bei RSI, bei einer Org auch die **Mitglieder** (verborgene fehlen). Du markierst Handles und Orgs als **freundlich, neutral, feindlich oder beobachtet**. Der Reiter **Beziehungen** zeigt alle Markierungen mit Export und Import. Er liest auch TOML-Listen im Format sc-social-overlay (wie bei IRIS), die Farben werden zu Haltungen. Eine Org kann ihre Liste unter einer https-Adresse teilen: „Liste laden“ holt sie auf Klick und ergänzt nur neue Namen, deine eigenen Markierungen bleiben. Einträge löschst du einzeln und holst sie zurück. **Zuletzt gesucht** merkt den Verlauf. Der **Log-Monitor** zeigt Kills, Tode und Ladebildschirme, ein Klick auf den Handle sucht ihn. Gefundene Handles bleiben so viele Tage gespeichert, wie du einstellst. Vorbild ist das [Star Citizen Handle Query](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query) von Kuehlwagen |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Overlay über dem Spiel** | Randlos, leicht durchscheinend, folgt dem Spiel. Ein Klick auf das **Schloss** in der Leiste lässt Mausklicks durch, das Schloss wird grün und holt sie mit einem Klick zurück. Ziehen an der Titelleiste, Größe am Griff ◢ unten rechts, **Position und Größe werden gemerkt**. Unter *Einstellungen → Overlay* stellst du die Größe von Overlay und Info-Leiste von 100 bis 200 Prozent ein. Das Overlay rastet am Bildschirmrand ein, ebenso die Info-Leiste. Die Zeichen der Leiste stehen unter *Bedienung des Overlays* |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Anzeigen im Overlay** | Unter *Einstellungen → Overlay* legst du je Ereignis fest, ob es das Overlay aufklappt: Bauplan, Scanner-Treffer, Auftrag, fertige Raffinerie und Katalogeintrag. Ab Werk klappt nur der Bauplan auf. Server, Ort, laufende Aufträge, Raffinerie-Aufträge, Fabrikator-Aufträge und den Update-Hinweis schaltest du einzeln ein und aus. Der Kopf der Aufträge zählt laufende gegen die Höchstzahl („3/10“) |
| <img src="assets/symbole/22/darstellung-gruen.png" width="22" alt=""> **Darstellung** | Unter **„Darstellung“** wählst du eines von **sechs Farbschemata** mit Vorschau: **VerseKit (Original), KRT (Orange), Eis (Cyan), Nebel (Violett), Glut (Rot) und Hoher Kontrast**, für Overlay und alle Fenster, ohne Neustart (Bilder oben unter *Die Farbschemata*). Die Größe von Schrift, Symbolen und Knöpfen stellst du **stufenlos** per Regler ein oder wählst die Voreinstellung: Auto, Full HD, WQHD, UHD (125 %), UHD (150 %). Auch unter Windows mit Skalierung über 100 % zeichnet VerseKit in voller Auflösung, statt unscharf hochgerechnet zu werden, ab Werk |
| <img src="assets/symbole/22/module-gruen.png" width="22" alt=""> **Module** | Basetool, Schiffe, Herstellung (mit „Was ich farmen muss“), Bergbau & Raffinerien, Bergung, Handel, Beschaffung (Einkaufsliste und Contested Zones) und Statistik lassen sich je Modul **aus der Seitenleiste ausblenden**. Eine leere Gruppe verschwindet, gelöscht wird nichts |
| <img src="assets/symbole/22/startprogramme-gruen.png" width="22" alt=""> **Startprogramme** | Weitere Programme **mit dem RSI Launcher** starten, sobald Star Citizen läuft, oder **statt** des Launchers, auf Wunsch beim Spielende wieder beenden. Unter Linux schaltet ein Schalter unter *Installation & Pfade* **MangoHud** dazu (FPS, Prozessor- und Grafikkarten-Last im Spiel). MangoHud kommt aus der Paketverwaltung, fehlt es, steht dort, woher |
| <img src="assets/symbole/22/auftragstexte-gruen.png" width="22" alt=""> **user.cfg** | Auf der Seite **„Spiel“**: was VerseKit einträgt, deine eigenen Zeilen und die ganze Datei, je Spielkanal. Vor der ersten Änderung wird die alte Datei gesichert |
| <img src="assets/symbole/22/sprachen-gruen.png" width="22" alt=""> **Deutsch und Englisch** | Die Oberfläche ist umschaltbar. Die Bauplan-Meldung im Log erkennt VerseKit **in jeder Spielsprache**, die Formulierung findet es selbst heraus |
| <img src="assets/symbole/22/abhaken-gruen.png" width="22" alt=""> **Hält sich selbst aktuell** | Sieht alle zehn Minuten nach einer neuen Version, lädt sie mit Prüfsumme und spielt sie ein, sobald Star Citizen geschlossen ist, **nie während das Spiel läuft**. Mit „Was ist neu“ zum Nachlesen, jede Version für sich |
| <img src="assets/symbole/22/nurlesend-gruen.png" width="22" alt=""> **Nur lesend** | Liest die `Game.log`. **Eine Ausnahme, und die fragt vorher:** Auf Wunsch trägt VerseKit die Bauplan-Kennzeichnung in die `global.ini` ein. Das lässt sich jederzeit zurücknehmen, Text anderer Werkzeuge bleibt erhalten. Dazu der Anti-AFK (nur Windows, ab Werk aus und nach jedem Start wieder aus): Eingeschaltet drückt er nach 5 Minuten ohne Eingabe (einstellbar) deine Taste für „Visier abwischen“, ab Werk Alt+X |
| <img src="assets/symbole/22/eigenbuch-gruen.png" width="22" alt=""> **Eigener Bestand und Nachlese** | VerseKit führt selbst Buch, welche Baupläne du hast. Beim Start liest es die aufgehobenen Logs früherer Sitzungen **und die laufende** und holt nach, was ohne laufendes VerseKit freigeschaltet wurde. Die Funde werden gemeldet, nicht still eingetragen. Der Knopf **Protokolle erneut einlesen** (Overlay und Einstellungen) geht alles noch einmal durch. Große Spielprotokolle liest VerseKit in Stücken und bleibt bei wenigen MB Speicher |
| <img src="assets/symbole/22/zeit-gruen.png" width="22" alt=""> **Spielzeit** | Oben in der Leiste steht, **wie lange du gespielt hast**: insgesamt und die laufende Sitzung. Gezählt wird aus den Protokollen und **fortgeschrieben**, auch wenn Star Citizen seine alten Logs wegräumt. Die Sicherung nimmt es mit |
| <img src="assets/symbole/22/statistik-gruen.png" width="22" alt=""> **Statistik** | Eigener Bereich mit fünf Reitern: **Übersicht** (Spielzeit, Sitzungen, Aufträge, Quantensprünge, Wärmekarte, **wann du spielst**), **Schiffe & Ausrüstung**, **Aufträge**, **Quantenreisen** und **Stabilität** (Abstürze, Verbindungsabbrüche). Dazu, was du im Laden ausgegeben und **eingenommen** hast, nach Ware und Ort, auch Verkäufe am Warenterminal, und Verletzungen nach leicht, mittel und schwer. Gezählt wird nur, was in jeder Spielsprache verlässlich im Log steht. Jede Sitzung bleibt, auch wenn die Logs gelöscht werden. Als Datei speicherbar |
| <img src="assets/symbole/22/sprachen-gruen.png" width="22" alt=""> **Übersetzung: je Kanal** | Welche Textdatei Star Citizen liest, stellst du für **jeden Kanal getrennt** ein: LIVE, HOTFIX, PTU, EPTU und Tech-Preview, je Kanal eine Karte mit Sprachliste und Quellen. Jeder Kanal bekommt Textdatei, Sprache und Bauplan-Angaben, oder nichts. HOTFIX und die Testkanäle können „Wie LIVE“ folgen. Neue Fassungen holt VerseKit selbst, es sieht alle fünf Minuten nach, auch nach einem Patch, und fragt bei GitHub nur noch, ob sich etwas geändert hat. Legt ein anderes Programm eine eigene Textdatei hinein, setzt VerseKit die gewählte wieder ein und sichert die fremde daneben. Zur Wahl stehen **Deutsch** (Dymerz), **StarStrings** (MrKraken, auch fürs PTU), **Français, Italiano, Español, Português und Türkçe** (Dymerz), acht Sprachen vom **SC Launch Configurator** (Luftwerft: Deutsch, Englisch, Français, Italiano, Español, Bayrisch, Nederlands und Português, für LIVE und PTU), das **Englisch des Spiels** oder **gar nichts**. Eine andere Quelle trägst du als **Adresse** ein: eine Zip oder eine `global.ini`, nur `https://`. Was keine Textdatei des Spiels ist, wird abgelehnt |
| <img src="assets/symbole/22/basetool-gruen.png" width="22" alt=""> **Abgleich mit dem KRT Profit Basetool** | Baupläne, Mein Lager und Hangar gleichen sich mit deinem Basetool ab, in beide Richtungen. Dazu der Bedarf und das **Lager deiner Einheit** (Gruppe **Flotte & Logistik**), in das du eigene Ware und Items mit einem Knopf an der Zeile legst und zurücknimmst. Du bleibst der Besitzer. Gesendet wird nur, was sich geändert hat. Bittet das Basetool um eine Pause, zeigt die Verbindungsseite, bis wann und warum. Schiffsteile und Geräte gehen mit ihrem Klassennamen. Klappt das Teilen ins Lager der Einheit nicht, holt VerseKit es selbst nach, und wo der Knopf fehlt, nennt ein grauer Knopf den Grund. Verbunden wird mit einem Code im Browser, ein Passwort sieht VerseKit nie. Jeder Bereich ist ab Werk aus, gelöscht wird nie still, Kaufdaten deiner Schiffe gehen nie hinaus. Was übertragen wird, steht in der [Datenschutzerklärung](PRIVACY.md). Die Verbindung steht unter **Einstellungen → Verbindung & Abgleich**, der Bedarf unter **Beschaffung** |
| <img src="assets/symbole/22/quellen-gruen.png" width="22" alt=""> **Mit scmdb.net verbinden** | Neue Baupläne und laufende Aufträge landen direkt in deinem scmdb-Konto. Einschalten unter *Einstellungen → Allgemein*, dann auf scmdb.net „Watcher“ aktivieren. Übernommen wird nur aus LIVE und HOTFIX, ab Werk aus |
| <img src="assets/symbole/22/bestaetigt-gruen.png" width="22" alt=""> **Alle Baupläne oder nur die erspielbaren** | Nur ein Teil der Baupläne ist über Aufträge zu bekommen, der Rest kommt aus Kiosken, Events oder hängt an keiner Mission. Ab Werk zählen nur die erspielbaren, damit dein Fortschritt eine messbare Zahl ist. Unter *Erkennung* schaltest du die übrigen dazu |
| <img src="assets/symbole/22/diagnose-gruen.png" width="22" alt=""> **Fehler melden in einem Klick** | Ein Feld für einen Satz, darunter der fertige Bericht, daneben ein roter Knopf. Bis zu vier **Bildschirmfotos** gehen auf Wunsch mit. Der Bericht enthält **weder deinen Benutzernamen noch deine Pfade**, und du siehst ihn vorher vollständig. Ist VerseKit beim letzten Mal abgestürzt, sagt es das beim nächsten Start einmal: „Bericht ansehen“ öffnet „Fehler melden“, gesendet wird nur, wenn du willst, „Nicht senden“ verwirft den Absturz |
| <img src="assets/symbole/22/anzeige-gruen.png" width="22" alt=""> **Streamer-Modus** | Unter *Einstellungen → Allgemein → Datenschutz*: Spielername, Pfade, Org und Einheit, Basetool-Name und Gerätecodes erscheinen überall als „[Geschwärzt]“. Solange er an ist, zeigt die Kopfzeile „Streamer-Modus an“. Ab Werk aus. Die Kopieren-Knöpfe kopieren weiter den Klartext |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Käufe und Verkäufe als Vorschlag** | Was du im Laden kaufst, schlägt *Mein Lager* unter „Laut Log neu“ mit dem Preis je Stück zum Übernehmen vor, was du verkaufst, zum Abbuchen. Du buchst oder verwirfst einzeln oder mit „Alle buchen“ und kannst es zurücknehmen. Warenverkäufe am Handelsterminal erkennt VerseKit nicht. Abschaltbar unter *Einstellungen → Erkennung* |
| <img src="assets/symbole/22/bestand-gruen.png" width="22" alt=""> **Umzug nach dem Patch** | *Mein Lager → „Umziehen“* bringt alle Posten an deinen neuen Heimatort und entfernt, was der Patch nicht sichert: Nahrung, Getränke, Pens und Injektoren, Magazine. Welche Arten gehen, stellst du vorher ein, Erze und Minerale bleiben ab Werk. Beide Lager werden vorher gesichert, danach zeigt VerseKit die entfernten Posten zum Abgleich im Spiel, und der Umzug lässt sich rückgängig machen |
| <img src="assets/symbole/22/raffinerie-gruen.png" width="22" alt=""> **Raffinerie lesen** (Windows) | Steht das Raffinerie-Terminal mit der Ausbeute vor dir, liest **Alt+Umschalt+R** es aus, auch im deutschen Spiel. Danach öffnet sich das Lager mit den gelesenen Zeilen zum Prüfen und Übernehmen. Die Station oben im Terminal wird der Lagerort, wenn sie sicher in der Ortsliste steht. Ist die Liste länger als der Bildschirm: scannen, im Terminal scrollen, noch einmal scannen. Beide Scans werden ein Auftrag, doppelte Zeilen zählen einmal. Unter *Bergbau* stehen deine Raffinerie-Aufträge, das Overlay zeigt sie mit Restzeit |
| <img src="assets/symbole/22/eigenbuch-gruen.png" width="22" alt=""> **Notizen** | Oben in der Seitenleiste, unter der Übersicht: dein Notizblock. **Fett, Kursiv, Unterstrichen, Durchgestrichen, Aufzählung, Nummerierung und kleine Tabellen** per Knopf oder mit Strg+B, Strg+I und Strg+U. Die erste Zeile ist der Name der Notiz |
| <img src="assets/symbole/22/vordergrund-gruen.png" width="22" alt=""> **Eigene Titelleiste** | VerseKit zeichnet die Leiste oben am Hauptfenster selbst. Unter Windows 11 erscheinen die Snap-Layouts am Maximieren-Knopf |
| 🐧 **Windows und Linux** | Eine Version für beide Systeme, mit Autostart und Spracherkennung im Log. Den Startmenü-Eintrag legst du unter *Einstellungen → Allgemein* an, unter Windows legt ihn der Installer an. Unter Linux kommen Signalton und Systemmeldung über deinen Desktop (Einzelheiten im [Handbuch](MANUAL.md#unter-linux)). Unter **KDE** liegt das Overlay im Vollbild ohne Startzusatz über dem Spiel, mit der Umgebungsvariable `VERSEKIT_X11_FENSTERTYP` wählst du einen anderen Fenstertyp, und sie hat Vorrang. Ablagesymbol und Scanner gibt es nur unter Windows |

</details>

<details>
<summary><b>Installation im Einzelnen</b></summary>

**Voraussetzungen:** Windows oder Linux und ein installiertes Star Citizen. VerseKit sucht den Ordner des Spiels selbst (die `Game.log` entsteht erst beim ersten Start), unter Linux in den üblichen Wine-Präfixen (Liste im [Handbuch](MANUAL.md#unter-linux)). Findet es nichts, fragt der Assistent. Konto und Nachinstallieren sind nicht nötig.

**Windows:** `VerseKit-Setup.exe` installiert mit Startmenü-Eintrag, auf Wunsch mit Desktop-Symbol und Autostart, und lässt sich über *Apps & Features* sauber entfernen. Eine einzelne `.exe` ohne Installer gibt es seit v3.0.0 nicht mehr, damit ein Update die alte Version wirklich ersetzt.

**Linux:** `VerseKit-x86_64.AppImage` ist eine einzelne Datei. Mach sie einmal ausführbar (Rechtsklick → Eigenschaften → *Als Programm ausführbar*, oder `chmod +x VerseKit-x86_64.AppImage`). Einen Startmenü-Eintrag bietet der Assistent an, später findest du ihn unter *Einstellungen → Allgemein*.

**Beim ersten Start** führt ein Assistent durch Sprache, Star Citizen finden, bisherige Baupläne holen, Anzeige, Programmstart und Angaben im Spiel. Das dauert eine Minute, danach steht dein Bestand.

#### ⚠️ Windows meldet „Der Computer wurde durch Windows geschützt"

Das kommt beim ersten Start, und es ist **kein Virenfund**:

> Von Microsoft Defender SmartScreen wurde der Start einer unbekannten App verhindert.

**So startest du trotzdem:** **Weitere Informationen** anklicken → **Trotzdem ausführen**. Danach kommt die Meldung nicht wieder.

**Warum das passiert:** SmartScreen prüft nicht, ob ein Programm schädlich ist, sondern ob es **bekannt** ist. Bekannt wird eine Datei durch eine gekaufte Code-Signatur (mehrere hundert Euro im Jahr) oder sehr viele Downloads. Ein kostenloses Fan-Werkzeug hat beides nicht, und jede neue Version fängt bei null an.

**Du musst das nicht glauben:**

- Der **Quellcode ist offen** ([hier](https://github.com/Xharig/VerseKit)), und die Datei baut **GitHub Actions** aus genau diesem Quellcode, nicht ich. Den Bauvorgang kannst du nachlesen: [`.github/workflows/`](.github/workflows/)
- Jede Datei auf der Releases-Seite trägt ihre **SHA-256-Prüfsumme**, GitHub zeigt sie an
- Jede Version lässt der Bau bei **VirusTotal** prüfen, das Ergebnis steht unten in der [Release-Notiz](https://github.com/Xharig/VerseKit/releases/latest). Einzelne Prüfprogramme melden bei neuen Programmen gelegentlich einen Treffer aus maschineller Heuristik, ein bekannter Fehlalarm

Unter **Linux** gibt es diese Meldung nicht.

#### Signatur der Dateien

Für dieses Projekt ist eine kostenlose Code-Signatur bei der [SignPath Foundation](https://signpath.org/) beantragt, einem Angebot für quelloffene Projekte. Sobald sie bewilligt ist, werden die Windows-Dateien unterschrieben, und Windows zeigt statt „unbekannter Herausgeber" einen Namen an.

Gebaut wird ausschließlich über einen öffentlichen GitHub-Actions-Ablauf. [SECURITY.md](https://github.com/Xharig/VerseKit/blob/main/SECURITY.md) beschreibt, wie eine Version entsteht, und [PRIVACY.md](https://github.com/Xharig/VerseKit/blob/main/PRIVACY.md), was das Programm liest, holt und sendet.

> ℹ️ Geprüft an einer echten Star-Citizen-Installation, mit **deutschem und englischem** Spiel-Client. Rückmeldungen von anderen Rechnern sind willkommen: andere Installationsorte, andere Bildschirmaufbauten, Windows. Gern als [Issue](https://github.com/Xharig/VerseKit/issues).

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

Die Dateien auf der Releases-Seite baut **GitHub Actions** bei jedem Versions-Tag aus genau diesem Quellcode ([Abläufe](.github/workflows/)). Von Hand baut sie niemand.

</details>

<details>
<summary><b>Bedienung des Overlays</b></summary>

Die schmale Leiste liegt über dem Spiel und meldet Neuzugänge. Alles Weitere steckt hinter den Zeichen in ihrer Titelleiste:

| Zeichen | Was es tut |
|---|---|
| <img src="assets/symbole/22/glocke-grau.png" width="22" alt=""> | **Glocke**: neue Version verfügbar, färbt sich dann grün |
| <img src="assets/symbole/22/starten-grau.png" width="22" alt=""> | **Rakete**: RSI Launcher starten. Erscheint nur, wenn ein Weg dorthin gefunden wurde |
| <img src="assets/symbole/22/einstellungen-grau.png" width="22" alt=""> | **Zahnrad**: das große Fenster öffnen oder zurückholen |
| <img src="assets/symbole/22/schloss_auf-grau.png" width="22" alt=""> | **Schloss**: Mausklicks ins Spiel durchreichen. Es wird grün und zu, solange sie durchgehen, ein Klick darauf fängt sie wieder ab |
| <img src="assets/symbole/22/einklappen-grau.png" width="22" alt=""> | **Pfeil**: Overlay einklappen, bis nur noch die Leiste dasteht |
| <img src="assets/symbole/22/leeren-grau.png" width="22" alt=""> | **Radiergummi**: angezeigte Meldungen wegräumen. Deine Baupläne bleiben |
| <img src="assets/symbole/22/schliessen-grau.png" width="22" alt=""> | **Kreuz**: beendet VerseKit, auch alle offenen Hauptfenster |

| Aktion | Wie |
|---|---|
| Fenster verschieben | Oben an der Leiste ziehen |
| Größe ändern | Griff **◢** unten rechts ziehen |

Die Hinweise zu den Knöpfen erscheinen in einem eigenen kleinen Fenster neben dem Mauszeiger, auch bei eingeklapptem Overlay.

**Das X im Hauptfenster versteckt es nur**, VerseKit läuft weiter. Zurück holst du es über das Symbol neben der Uhr, das Zahnrad im Overlay oder einen zweiten Start. Ohne laufendes Overlay, während eine Sicherung geschrieben wird und bei Update oder Neustart beendet das Schließen das Programm.

Was die Farbpunkte in der Liste bedeuten:

| | |
|---|---|
| <img src="assets/symbole/18/bestaetigt-gruen.png" width="18" alt=""> | Bauplan freigeschaltet: steht in deinem Bestand |
| <img src="assets/symbole/18/punkt-blau.png" width="18" alt=""> | im **Spiel** neu craftbar geworden: noch nichts, was *du* hast |
| <img src="assets/symbole/18/gemerkt-gelb.png" width="18" alt=""> | etwas von deiner Merkliste ist aufgetaucht |
| <img src="assets/symbole/18/hinweiszeile-grau.png" width="18" alt=""> | ein Hinweis, keine Freischaltung (etwa eine Lücke im Bestand) |

</details>

<details>
<summary><b>Testversionen und Fehler melden</b></summary>

Vor jeder Veröffentlichung gibt es **Testversionen**, erkennbar am `rc` in der Nummer. Sie stehen unter [Releases](https://github.com/Xharig/VerseKit/releases), jede mit dem, was sie bringt. Angeboten werden sie nur, wenn du sie auf der Seite **Update & Über** als **Testversion** wählst. Danach kommen sie wie jedes Update, nichts von Hand herunterladen.

- **Testversionen sind lauffähig**, aber noch nicht lange erprobt. Es kann etwas klemmen, dafür sind sie da.
- **Der Rückweg steht offen.** Schaltest du wieder um, bekommst du die nächste stabile Version angeboten, denn eine stabile gilt immer als neuer als jede Testversion derselben Nummer.
- **Ohne diese Einstellung merkst du nichts von Testversionen.** Wer schon eine Testfassung hat, bekommt weitere von selbst.
- **Du erkennst sie:** Läuft VerseKit als Testfassung oder auf einer Testkopie der Daten, sagt das die Kopfzeile neben der Version. Seiten mit Spieldaten aus PTU, EPTU oder TECH-PREVIEW nennen den Kanal und sagen, dass sich die Daten bis LIVE noch ändern können, auch Mein Hangar.

Etwas gefunden? Der schnellste Weg ist die Seite **Fehler melden**: ein Feld „Was ist passiert?“ für einen Satz, darunter der fertige Bericht, und der rote Knopf **Fehlerbericht absenden**. Mit **Bild anhängen** gehen bis zu vier Bildschirmfotos mit. Der Bericht enthält keine persönlichen Angaben, und du siehst ihn vorher vollständig.

Lieber selbst? Ein [Issue](https://github.com/Xharig/VerseKit/issues) geht genauso, der Knopf **GitHub Issue …** auf derselben Seite legt den Bericht schon hinein. Oder das Forum **Fehler-Melden** im [Discord](https://xharig.com/discord), wenn ein Bildschirmfoto schneller geht. Wünsche gern auf demselben Weg.

Was eine Version gebracht hat, liest du im [`CHANGELOG.md`](https://github.com/Xharig/VerseKit/blob/main/CHANGELOG.md) oder im Werkzeug unter **„Was ist neu“**. Was in einer Testfassung steckt, steht in ihrem Release-Text. Diese Seite zeigt immer die **veröffentlichte** Version.

</details>

<details>
<summary><b>Deine Daten und das Netz</b></summary>

🔒 **Es gehört dir.** Kein Konto, keine Anmeldung, keine Cloud. VerseKit liest Dateien, die ohnehin auf deiner Platte liegen. An der Spielinstallation ändert es nur, was du ausdrücklich einschaltest, und sichert die Datei vorher.

**Was VerseKit holt:**

| Wofür | Wie oft |
|---|---|
| Werte und Herkunft von scmdb.net (nur über https) | einmal je Spielversion |
| Rohstoffpreise und Lagerorte von UEX Corp | höchstens einmal am Tag |
| Ruf-Stufen der Ränge aus scunpacked-data | einmal je Spielversion |
| Steckplätze von erkul.games, nur für Schiffe, die in den Spieldaten deiner Installation fehlen | einmal je Spiel-Patch |
| Auftragstexte und Übersetzungen | wenn du sie einschaltest |
| Ob es eine neue Version gibt | im Hintergrund |
| Der Serverstatus von CIG | solange die Seite offen ist |
| Eine Handle-Suche bei RSI | wenn du suchst, oder mit Strg+Enter neu |
| Eine geteilte Beziehungsliste einer Org | nur auf Klick bei „Liste laden“ |
| Die Startzeit des Executive Hangars von contestedzonetimers.com | beim Öffnen der Seite Contested Zones, höchstens alle sechs Stunden |

**Was VerseKit sendet:**

| Was | Wann |
|---|---|
| Nutzung zählen: Version, System, Sprache und welche Seiten wie oft geöffnet wurden, ohne Kennung | einmal am Tag, ab Werk an, abschaltbar |
| Abgleich mit dem KRT Profit Basetool oder deinem scmdb.net-Konto | nur, wenn du ihn einschaltest |
| Fehlerbericht | nur, wenn du selbst den Knopf drückst, und du siehst vorher, was drinsteht |

Was genau in jeder Meldung steht, sagt die [Datenschutzerklärung](https://github.com/Xharig/VerseKit/blob/main/PRIVACY.md). **Alles Holen und Senden** schaltest du mit `SC_BP_NO_NET=1` ab. Die Info-Leiste liest ihre Werte auf deinem Rechner aus dem Spielbild, alle paar Sekunden die Ecke oben rechts und nur, wenn Star Citizen vorn ist.

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
- **[erkul.games](https://erkul.games)**: Steckplätze der Schiffe, die in den Spieldaten fehlen.
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

Die Herkunft je Bauplan stammt aus der **[Star Citizen Mission DataBase (scmdb.net)](https://scmdb.net)**, einem Hobbyprojekt, das die Spieldaten aufbereitet. Art, Größe, Gütegrad, Klasse und Herstellung liest VerseKit zuerst aus den Spieldaten deiner Installation, scmdb.net dient als Gegenprobe und springt ein, wo sich die Spieldaten nicht lesen lassen. **Danke!** 🙏

> VerseKit **liefert diese Daten nicht mit**, sondern lädt sie auf deinem Rechner bei scmdb.net, so wie es ein Browser täte, und nur bei einer **neuen Spielversion**. scmdb steht unter [CC BY-NC-ND 4.0](https://creativecommons.org/licenses/by-nc-nd/4.0/), eine mitgelieferte Kopie wäre eine Weitergabe und widerspräche dieser Lizenz und der GPL des Projekts.

Als Grundlage für die Bauplan-Angaben lässt sich **[StarStrings](https://github.com/MrKraken/StarStrings)** von **MrKraken** wählen: aufgeräumte englische Spieltexte, die in vielen Organisationen benutzt werden. **Danke** an MrKraken! 🙏

> Auch StarStrings **liegt nicht bei**, sondern wird auf Wunsch von der Original-Adresse geholt. Eine Lizenz gibt das Projekt nicht an. Umso mehr gilt: Der Text bleibt seiner.

**VerseKit verträgt sich mit anderen Werkzeugen.** StarStrings und andere Werkzeuge kennzeichnen Bauplan-Aufträge ebenfalls mit `[BP]`. VerseKit setzt **keine zweite Marke dazu, wo schon eine steht**, und lässt jeden Gegenstandsnamen mit Kürzel in Ruhe. Steht schon die Bauplan-Liste eines anderen Werkzeugs im Auftrag, **ersetzt** VerseKit sie durch die eigene mit den **Kästchen** für deinen Bestand. Nimmst du die Angaben zurück, steht der Stand des anderen Werkzeugs wieder da, Zeichen für Zeichen.

**Die Übersetzungen** stammen von **[Luftwerft](https://www.luftwerft.com/)**, **[Dymerz](https://github.com/Dymerz/StarCitizen-Localization)** (Deutsch, Français, Italiano, Español, Português, Türkçe) und **[MrKraken](https://starstrings.app)** (StarStrings, Englisch mit Auftragsmarken). Sie **liegen nicht bei**, VerseKit holt die gewählte Fassung auf deinem Rechner und hält sie aktuell. **Luftwerft** bietet im **[SC Launch Configurator](https://www.luftwerft.com/)** Deutsch, Englisch, Français, Italiano, Español, Bayrisch, Nederlands und Português für LIVE und PTU an und hat sie für VerseKit freigegeben. Sie entstehen weitgehend automatisch aus den Spieldaten und werden weiter verfeinert, VerseKit zeigt vor dem Umschalten den Hinweis des Entwicklers. **Danke** an Luftwerft, Dymerz, Thord82 und MrKraken! 🙏

**Die Rohstoffpreise** kommen von **[UEX Corp](https://uexcorp.space)**, einem von Spielern gepflegten Datenprojekt, **die Ruf-Stufen der Ränge** („Gildenmitglied [ab 10.000]“) aus **[scunpacked-data](https://github.com/StarCitizenWiki/scunpacked-data)** des **Star Citizen Wiki**. Beides **liegt nicht bei** und wird auf deinem Rechner geholt, die Preise höchstens einmal am Tag, die Ruf-Stufen einmal je Spielversion. **Danke** an UEX Corp und das Star Citizen Wiki! 🙏

**Die Steckplätze der Schiffe** liest VerseKit aus den Spieldaten deiner Installation. Wo die ein Schiff nicht haben oder einen Platz ohne passende Teile lassen, springt **[erkul.games](https://erkul.games)** ein, das Auslegungs-Werkzeug der Gemeinschaft. Diese Daten **liegen nicht bei** und werden nur für Schiffe geholt, die du eingetragen hast, dazu die Listen der Racks, Raketen, Bomben und Lackierungen, einmal je Spiel-Patch. **Danke** an erkul.games! 🙏

**Den Hangar-Import** ermöglichen Browser-Erweiterungen: der **[Hangar-Exporter](https://github.com/Drachennarr/Hangar-Exporter-Releases/releases/latest)** von **Drachennarr** (GPL-3.0), der den Hangar als Datei speichert oder ohne Datei direkt an VerseKit auf diesem Rechner schickt, die **[Star Citizen: Hangar Extension](https://robertsspaceindustries.com/community-hub/post/star-citizen-hangar-extension-browser-add-on-7xzYDnJDV6c2W)** von **AlyxOne** (MIT-Lizenz, gepflegt, empfohlen) und der ursprüngliche **[Star Citizen Hangar XPLORer](https://github.com/dolkensp/HangarXPLOR)** von **dolkensp** (MIT-Lizenz). Ohne sie müsste jedes Schiff von Hand eingetragen werden. **Danke** dafür! 🙏

**Das Profit Basetool** von **greluc** (GPL-3.0) gleicht sich mit VerseKit ab, und seine **[DPoP-Referenz](https://krt-profit.github.io/basetool/)** (MIT) signiert die Anmeldung, ohne dass der Schlüssel deinen Rechner verlässt. Die Referenz ist im Programm eingebaut. **Danke** an greluc! 🙏

**Die Handle-Suche und der Log-Monitor** sind dem **[Star Citizen Handle Query](https://github.com/Kuehlwagen/Star-Citizen-Handle-Query)** von **Kuehlwagen** (MIT) nachgebaut, mit eigenem Code. **Danke** für Idee und Vorbild! 🙏

**Die Karten der Contested Zones** sind in VerseKit eingebaut: Checkmate, Orbituary und Ruin Station von **Zane_DragonBorn** (zum [Beitrag](https://robertsspaceindustries.com/community-hub/post/accurate-pyro-contested-zone-maps-c-zs-7l8Ih9mXyPDpu)), PYAM-EXHANG-0-1 und PYAM-SUPVISR-3-4/5 von **Kerast** (zum [Beitrag](https://www.reddit.com/r/starcitizen/comments/1i3iyq9/supervisor_exectutive_hangar_maps_pyamsupvisr345/)). Beide haben sie zur freien Nutzung freigegeben, wenn sie genannt werden. Die Startzeit des Executive Hangars liest VerseKit von **[contestedzonetimers.com](https://contestedzonetimers.com/)**, **Appieman** betreibt die Seite und hat das erlaubt. **Danke** an alle drei! 🙏

Die Symbole der Oberfläche stammen aus dem **[Lucide](https://lucide.dev)**-Satz (ISC-Lizenz), alle auf demselben Raster mit gleicher Strichstärke, weshalb sie unter Windows, Linux und macOS gleich aussehen. Der Lizenztext liegt bei (`assets/symbole/LIZENZ.txt`) und steht im Werkzeug unter **Danke & Lizenzen**. Die Flaggen an der Sprachwahl stammen aus **[flag-icons](https://flagicons.lipis.dev)** (MIT-Lizenz, © 2013 Panayiotis Lipiridis). **Danke** an beide! 🙏

VerseKit ist ein eigenständiges, inoffizielles Zusatz-Tool und steht in **keiner** offiziellen Verbindung zu Cloud Imperium Games. Alle Marken- und Projektnamen gehören ihren jeweiligen Eigentümern.

</details>

<div align="center">

[![Xharig](https://github.com/Xharig.png?size=80)](https://github.com/Xharig)

**Xharig**: Entwicklung und Gestaltung dieses Projekts

<sub>Gefällt Dir das Werkzeug? <a href="https://ko-fi.com/xharig">Ko-fi</a> ☕</sub>

</div>

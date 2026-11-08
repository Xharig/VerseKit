# VerseKit: Handbuch

**Deutsch** · [English](MANUAL.en.md) · [Zurück zur Übersicht](README.md)

Hier steht das Technische: wie VerseKit Baupläne erkennt, welche Dateien es anlegt und was sich von Hand einstellen lässt. Zum Benutzen brauchst du nichts davon, der Assistent und die Einstellungen im Programm reichen.

## So arbeitet VerseKit

1. **Beim Start** sieht VerseKit die aufgehobenen Logs vergangener Sitzungen durch (`logbackups/`) und übernimmt alles Gefundene still in deinen Bestand. Wer ohne laufendes VerseKit gespielt hat, verliert also nichts. Diese Baupläne werden **nicht** als neu gemeldet. Reichen die Sicherungen nicht weit genug zurück, sagt VerseKit das als <img src="assets/symbole/16/hinweiszeile-grau.png" width="16" alt="">-Zeile, statt eine unvollständige Liste als vollständig auszugeben.
2. **Im Hintergrund** wird die **`Game.log`** gelesen, alle 3 Sekunden, einstellbar. Schreibt das Spiel beim Freischalten `Added notification "Bauplan erhalten: <Name>: "`, steht der Bauplan **sofort** in der Liste (<img src="assets/symbole/16/bestaetigt-gruen.png" width="16" alt="">) und im Bestand. Eine Zwischenstufe gibt es nicht: Was in der `Game.log` steht, steht im Spiel. Da ist nichts zu bestätigen.
3. Jede neue Zeile wird oben eingefügt (Name · Art · `M/1/A` · Uhrzeit) und ein kurzer Ton gespielt.
   - **Einmal pro Minute** wird der Craftbar-Katalog geprüft. Ist er gewachsen, hat CIG mit einem Patch etwas **neu craftbar** gemacht, und es erscheint eine blaue Zeile. Das hat nichts mit deinem Freischalt-Stand zu tun. Der Vergleichsstand liegt als `catalog-seen.json` im eigenen Ordner und überlebt Neustarts; beim allerersten Start wird nur die Basis gesetzt.
4. **Art, Größe, Gütegrad und Klasse** liest VerseKit aus den Spieldaten deiner eigenen Installation, gegengeprüft mit scmdb.net; wo sich die Spieldaten nicht lesen lassen, springt scmdb.net ein. Über allem stehen deine eigenen Korrekturen aus `bp-overrides.json`.
5. **Dein Bestand** wächst dabei mit und bleibt in `bestand.json` erhalten, mit Vermerk, woher jeder Bauplan stammt (Log, Nachlese, Import). Das ist die Liste „welche habe ich“.

> **Warum direkt aus der Log?** Wer selbst mitliest, ist in Sekunden dran und braucht dafür niemanden dazwischen.

Überwachte Dateien:

```text
…\StarCitizen\LIVE\Game.log                 (Spiel, die eigentliche Quelle)
…\StarCitizen\LIVE\logbackups\             (frühere Sitzungen, beim Start nachgelesen)
```

Eigene Dateien (Bestand, Einstellungen, Zwischenspeicher) liegen in einem eigenen Ordner unter *Dokumente*. Wo genau, zeigt **Einstellungen → Installation & Pfade**; dort lässt er sich auch verlegen. Für einen einzelnen Start geht das ebenso mit der Umgebungsvariablen `SC_BP_HOME`.

### Spielsprache

Die Bauplan-Meldung im Log ist übersetzt, und VerseKit **findet selbst heraus**, wie sie in deinem Client lautet. Es kennt über 700 Bauplan-Namen; steht in einer Logzeile einer davon, ist der Text davor die gesuchte Formulierung. Das klappt auch bei Sprachen, die niemand vorgesehen hat: Französisch und Spanisch genauso wie Englisch.

Deutsch und Englisch sind zusätzlich fest hinterlegt, und wer möchte, trägt eigene in `phrasen.json` im eigenen Ordner ein:

```json
{ "phrasen": ["Blueprint Received"] }
```

### Eigene Pfade eintragen

Liegt Star Citizen nicht an einer der üblichen Stellen, trägst du den Ordner selbst ein, in `einstellungen.json` im eigenen Ordner:

```json
{
  "spiel_ordner": "D:\\Spiele\\StarCitizen\\LIVE"
}
```

In `spiel_ordner` gehört der Ordner, in dem die `Game.log` liegt (meist `LIVE`). Ein leeres Feld heißt „bitte suchen“. Nach dem Ändern VerseKit neu starten.

> Findet VerseKit das Spiel nicht, legt es diese Datei beim Start **von selbst** an und sagt dir, wo sie liegt. Du musst sie nicht von Hand erzeugen. In der Datei stehen bei jedem Feld die Orte, an denen gesucht wurde; dieselben nennt auch das Fenster. So siehst du, wie so ein Pfad auf deinem System aussieht, statt ihn raten zu müssen.

### Windows und Linux nebeneinander? Ein Ordner für beide

Wer denselben Rechner mal unter Windows und mal unter Linux startet, führt sonst **zwei getrennte Bauplan-Bestände** und merkt es nicht. Jedes System liest die Spiel-Logs, die es gerade sieht, und schreibt in seinen eigenen Ordner unter *Dokumente*. Erst Monate später fällt auf, dass auf der einen Seite Baupläne fehlen.

**Die Lösung ist eine Einstellung, kein Abgleich:** Leg den Ordner für deine Daten auf eine Platte, die **beide Systeme sehen**, und stell ihn in beiden Systemen auf denselben Pfad, unter *Einstellungen → Installation & Pfade → Ordner für deine Daten*. Linux liest NTFS, eine gemeinsame Datenplatte reicht also aus.

Danach gibt es nur **einen** Bestand. Es wird nichts kopiert und nichts abgeglichen, also kann auch nichts auseinanderlaufen oder sich überschreiben.

> Beim Umstellen fragt VerseKit, ob deine bisherigen Daten mitkommen sollen, und prüft jede kopierte Datei gegen das Original. Der alte Ordner bleibt vollständig liegen, gelöscht wird nichts.

Dasselbe funktioniert über **zwei Rechner**, wenn beide denselben Cloud- oder Netzwerkordner benutzen. ⚠️ Dort sollte allerdings immer nur ein VerseKit gleichzeitig laufen.

### Unter Linux

**Wo das Spiel gesucht wird.** In diesen Wine-Präfixen, jeweils unter `drive_c/Program Files`, `drive_c/Program Files (x86)` und `drive_c`:

| Woher | Ordner |
|---|---|
| lug-helper | `~/Games/star-citizen`, `~/Games/star-citizen-live` |
| Wine | `~/.wine` |
| Lutris | `~/.local/share/lutris/prefixes/*`, als Flatpak `~/.var/app/net.lutris.Lutris/data/lutris/prefixes/*` |
| Bottles | `~/.local/share/bottles/bottles/*`, als Flatpak `~/.var/app/com.usebottles.bottles/data/bottles/bottles/*` |
| Eigene Ordner | `~/Games/*` |
| Heroic | `~/Games/Heroic/Prefixes/*`, mit Proton `…/pfx` |
| Steam mit Proton | `~/.local/share/Steam/steamapps/compatdata/*/pfx`, als Flatpak `~/.var/app/com.valvesoftware.Steam/data/Steam/steamapps/compatdata/*/pfx` |

Liegt dein Präfix woanders, trägst du den Ordner mit der `Game.log` im Assistenten oder in `spiel_ordner` ein (siehe oben).

**Startmenü.** *Einstellungen → Allgemein → Eintrag im Startmenü* legt `~/.local/share/applications/versekit.desktop` an (bzw. unter `$XDG_DATA_HOME`) und das Symbol daneben unter `icons/hicolor/256x256/apps/versekit.png`. Er startet die AppImage-Datei selbst, nicht den entpackten Zwischenordner. Ein Update zieht Name und Symbol eines vorhandenen Eintrags nach; einen gelöschten legt es nicht wieder an. Der Eintrag der Vorgängerversion 4.x wird übernommen.

**Beim Anmelden starten** schreibt `~/.config/autostart/versekit.desktop` (bzw. unter `$XDG_CONFIG_HOME`). Schaltest du den Eintrag in den Einstellungen deines Desktops aus, zählt das als aus.

**Ton und Systemmeldung.** Der Signalton läuft über `canberra-gtk-play`, `paplay` oder `pw-play` mit den freedesktop-Klängen, je nachdem, was da ist; ohne eines davon bleibt es still. Die Systemmeldung braucht `notify-send` (Paket `libnotify`); fehlt es, sagt die Einstellung „hier nicht möglich“.

**Tastenkombination.** Unter X11 greift sie überall. Unter Wayland kann kein Programm eine Taste für den ganzen Desktop belegen; VerseKit läuft dort über XWayland, und die Taste greift, solange das Spiel vorn ist. Für überall zeigt *Einstellungen → Overlay* unter der Tastenkombination einen fertigen Befehl, den du im System auf ein eigenes Kürzel legst (KDE Plasma: *Systemeinstellungen → Tastatur → Kurzbefehle → Neu hinzufügen → Befehl oder Skript*). Läuft VerseKit schon, öffnet der Befehl das Hauptfenster, statt VerseKit ein zweites Mal zu starten.

**Nur unter Windows:** das Symbol in der Ablage neben der Uhr, der Signatur- und der Raffinerie-Scanner und Anti-AFK. Die Einstellungen sagen das an der jeweiligen Stelle.

### Auf bestimmte Gegenstände warten

Wartest du auf einen ganz bestimmten Bauplan, klick in der Bauplan-Liste auf den **Stern** neben seinem Namen. Über das Suchfeld findest du ihn in Sekunden, und der Filter **Merkliste** zeigt dir, worauf du gerade wartest.

Taucht ein beobachteter Bauplan auf, meldet VerseKit ihn auffällig in Gold mit einem Stern und eigenem Signalton und **hakt ihn auf der Merkliste ab**. Er bleibt dort stehen, damit der Bauplan-Fortschritt „Nur Merkliste“ ihn mitzählt; über den Stern nimmst du ihn wieder herunter.

<details>
<summary>Für Fortgeschrittene: Muster statt Namen</summary>

Manchmal wartet man auf etwas, dessen genauen Namen es noch gar nicht gibt, etwa „irgendein Helm für den schweren Anzug“. Dafür kennt die `watchlist.json` im eigenen Ordner neben den angeklickten Namen auch **Muster**:

```json
{
  "namen": ["Attrition-5 Repeater"],
  "eintraege": [
    { "titel": "Helm für den schweren Anzug", "muster": ["manticore helmet"] },
    { "titel": "Kühler, egal welcher", "muster": ["cooler"] }
  ]
}
```

Ein Muster-Eintrag hat einen frei gewählten **Titel** (der steht später in der Meldung) und beliebig viele **Muster**, die kleingeschrieben als Teilstück gegen jeden neuen Katalog-Eintrag geprüft werden. `cooler` trifft also jeden Kühler, `manticore helmet` nur diesen einen.

Von Hand nötig ist das nicht: Für einen bestimmten Bauplan genügt der Stern in der Liste.

</details>

## Einstellungen

In `einstellungen.json` im eigenen Ordner, eine Textdatei, kein Code. Nach dem Ändern VerseKit neu starten. Die Datei wird beim ersten Start angelegt und erklärt jedes Feld selbst.

| Feld | Bedeutung | Standard |
|---|---|---|
| `sprache` | Oberflächensprache: `auto`, `de` oder `en` | `auto` |
| `spiel_ordner` | Wo Star Citizen liegt (leer = automatisch suchen) | leer |
| `pruefintervall_sekunden` | Wie oft die `Game.log` angesehen wird, erlaubt 1 bis 60 | `3` |
| `signalton` | Kurzer Ton bei einem Fund | `true` |

> Position und Größe des Overlays merkt sich VerseKit beim Verschieben und Beenden (`watcher.json` im selben Ordner). Zieh es einfach dorthin, wo du es haben willst. Eine feste Startlage gibt das Programm bewusst **nicht** vor: Wo ein Overlay gut sitzt, hängt am Monitoraufbau. Zum Zurücksetzen die Datei löschen.

> **Eigene Korrekturen:** Stimmt bei einem Bauplan die Angabe zu Klasse, Größe oder Gütegrad nicht, kannst du sie in `bp-overrides.json` im eigenen Ordner überschreiben. Sie hat Vorrang vor allen anderen Quellen. Liegt die Datei woanders, gib den Pfad über die Umgebungsvariable `SC_BP_OVERRIDES` an.

**Umgebungsvariablen**, für einen einmaligen Sonderfall, ohne etwas dauerhaft zu ändern:

| Variable | Wirkung |
|---|---|
| `SC_BP_HOME` | anderer Ordner für Bestand und Einstellungen |
| `SC_INSTALL_DIR` | anderer Spielordner |
| `SC_BP_NO_NET=1` | **keine** Netzabfragen, weder Spieldaten noch Versionsprüfung noch Meldungen |
| `SC_BP_SPRACHE` | Sprache für diesen Start (`de` / `en`) |

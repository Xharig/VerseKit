; Installer für Windows — gebaut mit Inno Setup (kostenlos, auf den
; GitHub-Baurechnern vorinstalliert).
;
; Warum überhaupt ein Installer: Nutzer haben gemeldet, dass sie die .exe
; selbst irgendwohin ziehen müssen. Genau daran springen Erstnutzer ab.
;
; ⚠ Ziel ist {localappdata}\Programs, NICHT "Programme":
;   * kein Administrator nötig — weder beim Installieren noch beim Update,
;   * und nur so überlebt das Selbst-Update. Es tauscht die laufende .exe per
;     move aus; in C:\Program Files scheitert das an den Rechten.
;   So machen es Discord, VS Code und der SC Deutsch Launcher auch.
;
; Die eigenen Dateien des Spielers (Dokumente\SC BP Watcher) fasst der
; Installer NICHT an — weder beim Installieren noch beim Deinstallieren.
; Ein Bauplan-Bestand, den man über Monate sammelt, gehört nicht dem Programm.

#define AppName "VerseKit"
#define AppPublisher "Xharig"
#define AppURL "https://github.com/Xharig/SC-BP-Watcher"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

; ⚠ Windows verlangt für die Datei-Version eine reine Zahlenfolge. Eine
; Vorabfassung wie "3.0.0-rc1" laesst Inno Setup mit "Value of [Setup] section
; directive VersionInfoVersion is invalid" abbrechen — der ganze Bau schlug fehl,
; als die erste Testfassung von v3.0.0 gebaut werden sollte. Deshalb wird hier
; alles ab dem Bindestrich abgeschnitten: Angezeigt wird weiterhin die volle
; Bezeichnung, nur die technische Datei-Version ist die nackte Zahl.
#define ZahlVersion AppVersion
#if Pos("-", ZahlVersion) > 0
  #define ZahlVersion Copy(ZahlVersion, 1, Pos("-", ZahlVersion) - 1)
#endif

[Setup]
; ⚠ Diese Kennung bleibt für immer gleich. Ändert sie sich, hält Windows jede
; Fassung für ein anderes Programm — der Nutzer hätte drei Einträge in "Apps &
; Features" und drei Startmenü-Ordner.
AppId={{7C4B1E93-2A6F-4D58-B0E1-9F3A5C8D2461}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
VersionInfoVersion={#ZahlVersion}

; ⚠ Das laufende Programm muss beendet werden, bevor die .exe ersetzt wird.
; Ohne Vorkehrung bricht das Setup mitten im Kopieren ab:
;
;     An error occurred while trying to replace the existing file:
;     DeleteFile failed; code 32.
;     Der Prozess kann nicht auf die Datei zugreifen, da sie von einem
;     anderen Prozess verwendet wird.
;
; Genau so beim Testen gemeldet (Haldjas, 25.08.2026) — mit der Folge, dass die
; Installation halb fertig liegen blieb und der Watcher danach nur noch das
; Setup startete.
;
; `CloseApplications=force` lässt den Windows-Restart-Manager das Programm
; schließen, `RestartApplications=yes` fährt es danach wieder hoch. `force`
; statt `yes`, weil `yes` nur **bittet** (ein WM_CLOSE über den Restart
; Manager) — ein Programm, das nicht mehr reagiert, hört das nicht, und danach
; scheitert das Kopieren wieder an „code 32". Auch das kam von Haldjas
; (25.08.2026): „der installer schafft es dann aber immer noch nicht zu
; installieren, selber code 32 fehler wie am anfang". Dass `force` notfalls hart
; beendet, ist hier vertretbar: Der Bauplan-Bestand liegt in
; `Dokumente\SC BP Watcher` und wird bei jeder Änderung geschrieben, nicht erst
; beim Beenden.
;
; ⚠ Hier stand einmal zusätzlich `AppMutex=SC-BP-Watcher-Einzelstart`, mit der
; Begründung, Inno erkenne das laufende Programm nur darüber. **Das stimmt
; nicht** — und die Zeile hat den Update-Weg am 26.08.2026 im Test vollständig
; blockiert:
;
;   * Die beiden sind verschiedene Mechanismen. Der Restart Manager erkennt ein
;     laufendes Programm daran, welche Dateien es offen hält; einen Mutex
;     braucht er dafür nicht.
;   * `AppMutex` prüft nur, ob der Mutex existiert, und zeigt dann „Bitte
;     schließen Sie jetzt alle laufenden Instanzen". **Beenden tut es nichts.**
;   * Dieser Test läuft **vor** `CloseApplications` — er blockiert also genau
;     den Automatismus, der die Arbeit machen soll, und zwar auch bei `/SILENT`.
;   * Da der Watcher weiterlief, hielt er den Mutex. Jedes „OK" führte zurück
;     zur selben Meldung. Verloren ist damit niemand — der Text sagt klar, dass
;     man das Programm schließen soll —, aber es ist genau die Handarbeit, die
;     `CloseApplications` abnehmen soll. Wer im Watcher auf „Fassung holen"
;     drückt, soll nicht anschließend das Fenster zumachen müssen, aus dem er
;     gerade geklickt hat.
CloseApplications=force
; ⚠ `no`, und das ist Absicht. Der Restart Manager faehrt nur wieder hoch, was er
; selbst **sanft** geschlossen hat — ein mit `force` hart beendeter Prozess zaehlt
; nicht dazu. Die beiden Zeilen arbeiten also gegeneinander: `force` ist noetig,
; damit das Ersetzen nicht an „code 32" scheitert, und genau deshalb kann der
; Neustart hier nicht klappen. Am 26.08.2026 im Test gesehen — das Update lief
; sauber durch, aber der Watcher blieb unten, und er musste von Hand
; starten.
;
; Den Neustart uebernimmt deshalb der [Run]-Abschnitt. Auf `yes` stehen zu
; lassen waere nicht nur wirkungslos, sondern gefaehrlich: Griffe beides, kaeme
; der Watcher doppelt hoch.
RestartApplications=no

; Kein Administrator — siehe Kopf.
;
; ⚠ Hier stand einmal `PrivilegesRequiredOverridesAllowed=dialog`. Das lässt Inno
; beim Installieren fragen, ob "für mich" oder "für alle Nutzer" — und wer "für
; alle" wählt, bekommt seinen Eintrag im **Maschinenzweig** der Registry. Genau
; das war am 26.08.2026 auf dem Testrechner passiert, mit zwei Folgen:
;
;   * `windows_eintrag_pflegen()` sucht unter HKCU und fand nichts. Die in
;     "Apps & Features" angezeigte Fassung wurde nie nachgezogen.
;   * Jedes Selbst-Update bräuchte eine Administrator-Abfrage, weil der
;     Installer wieder in den Maschinenzweig schreiben will.
;
; Ohne die Zeile landet alles im Benutzerzweig — passend zum Ziel unter
; `{localappdata}\Programs`, für das ohnehin nie Administratorrechte nötig sind.
PrivilegesRequired=lowest
; ⚠⚠ Nur der Vorschlag für eine NEUinstallation. Eine vorhandene Installation
; behält ihren Ordner — `UsePreviousAppDir` steht deshalb ausdrücklich hier und
; nicht als stiller Standard.
;
; Das ist bei der Umbenennung zu VerseKit (12.09.2026) entscheidend geworden:
; Bestandsnutzer haben `…\Programs\SC BP Watcher`, der Vorschlag lautet jetzt
; `…\Programs\VerseKit`. Ohne diese Zeile hinge daran die Frage, ob das Update
; die vorhandene Installation aktualisiert oder eine zweite anlegt — und der
; Autostart-Eintrag in der Registry zeigt auf den ALTEN Pfad.
UsePreviousAppDir=yes
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
; ⚠⚠ Ohne diese Zeile behielte Inno den GESPEICHERTEN Gruppennamen (Standard
; `UsePreviousGroup=yes`). Nach der Umbenennung hieße der Startmenü-Ordner also
; weiter „SC BP Watcher", während die Verknüpfungen darin „VerseKit" heißen —
; sichtbar inkonsistent.
;
; Mit `no` greift `DefaultGroupName`, und der alte Ordner wird über
; `[InstallDelete]` entfernt. Unkritisch, weil `DisableProgramGroupPage=yes`
; den Nutzer den Ordner ohnehin nie wählen lässt.
UsePreviousGroup=no
DisableProgramGroupPage=yes
DisableDirPage=auto

OutputDir=..\dist
; ⚠ Der Name bleibt so — die Testfassungen rc39 bis rc75 suchen beim Update
; gezielt nach einer Datei auf `-setup.exe`. Wird hier umbenannt, finden sie
; gar nichts mehr und bekommen nie wieder ein Update angeboten.
OutputBaseFilename=VerseKit-Setup
SetupIconFile=..\icon.ico
UninstallDisplayIcon={app}\SC-BP-Watcher.exe
UninstallDisplayName={#AppName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible

; Deutsch und Englisch — das Werkzeug spricht beide, der Installer auch.
[Languages]
Name: "deutsch"; MessagesFile: "compiler:Languages\German.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; \
  GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "Mit Windows starten"; \
  GroupDescription: "Nach der Installation:"; Flags: unchecked

[Files]
Source: "..\dist\SC-BP-Watcher.exe"; DestDir: "{app}"; Flags: ignoreversion

; ⚠⚠⚠ Umbenennung zu VerseKit (12.09.2026): Die Verknüpfungs-DATEINAMEN hängen
; an `AppName`. Beim Update entstehen sonst `VerseKit.lnk` **neben** den alten
; `SC BP Watcher.lnk` — im Startmenü, auf dem Desktop und beim
; Deinstallations-Link. `UsePreviousAppDir` schützt nur den Installationspfad,
; nicht die Verknüpfungen.
;
; Deshalb werden die alten hier entfernt, bevor die neuen entstehen.
;
; `Type: files` ohne vorhandene Datei ist stillschweigend in Ordnung; wer nie
; eine solche Verknüpfung hatte, merkt nichts.
[InstallDelete]
; ⚠⚠⚠ Hier standen schon DREI falsche Fassungen — alle drei vom Prüfer
; gefunden, und alle drei hatten dieselbe Ursache: eine unbelegte Annahme.
;
; **Versuch 1 (F01):** die Links einzeln aufgezählt, dabei den übersetzten
; Namen des Deinstallations-Links GERATEN — „… deinstallieren.lnk" statt
; „… entfernen.lnk", wie `German.isl` ihn wirklich bildet.
;
; **Versuch 2 (F05a):** daraufhin `filesandordirs` auf den ganzen Ordner. Löst
; das Sprachproblem, löscht laut Inno-Doku aber „all files and subdirectories
; in them" — also auch, was der NUTZER hineingelegt hat. `DefaultGroupName`
; reserviert den Ordner nicht exklusiv.
;
; **Versuch 3 (F05b):** dann Platzhalter auf den Produktnamen. Trifft aber
; auch eine Verknüpfung, die der Nutzer SELBST angelegt hat —
; „SC BP Watcher - Eigene Notizen.lnk" zeigt auf seine Notizen und wäre
; gelöscht worden. ⭐ **Ein Produktname im Dateinamen belegt keine
; Zugehörigkeit zum Installer.**
;
; ⭐⭐ **Jetzt: genau die Namen, die dieser Installer selbst anlegt** — und
; zwar über Innos EIGENE Übersetzung, nicht über eine geratene oder
; nachgepflegte. `{cm:UninstallProgram,…}` ist wörtlich derselbe Ausdruck, mit
; dem `[Icons]` den Link erzeugt (eine Klammer weiter unten). Damit können
; Anlegen und Entfernen nicht auseinanderlaufen, und es gibt nichts zu raten.
;
; ⚠ **Der Name steht hier absichtlich ausgeschrieben statt als `{#AppName}`:**
; Entfernt wird der Link der ALTEN Fassung. Zöge er mit `AppName` mit, würde
; beim nächsten Namenswechsel der falsche Link gelöscht.
;
; ⚠ Nicht prüfbar auf diesem Rechner: ob `{cm:…}` in diesem Parameter
; entfaltet wird (Inno liegt hier nicht, der Bau läuft in GitHub Actions). Die
; Doku sagt nur „the majority of the script entries can have constants
; embedded in them". Ein Fehler daran bricht den BAU — er wird also sichtbar,
; bevor ein Release entsteht, und trifft keinen Nutzer.
; ⚠ Bleibt die Sprache des Updates hinter der Erstinstallation zurück, bleibt
; ein übersetzter Link stehen. Sichtbar, harmlos, kein Datenverlust — und
; `UsePreviousLanguage` (Standard `yes`) macht genau das unwahrscheinlich.
Type: files; Name: "{autoprograms}\SC BP Watcher\SC BP Watcher.lnk"
Type: files; Name: "{autoprograms}\SC BP Watcher\{cm:UninstallProgram,SC BP Watcher}.lnk"
; `dirifempty` räumt den alten Gruppenordner weg — und lässt ihn stehen,
; sobald noch irgendetwas darin liegt. Ein Nutzerinhalt überlebt das Update,
; der Preis ist ein übrig gebliebener Ordner. Richtige Richtung: nichts
; Fremdes löschen ist wichtiger als ein aufgeräumtes Startmenü.
Type: dirifempty; Name: "{autoprograms}\SC BP Watcher"

; ⚠⚠ `Tasks: desktopicon` ist hier KEIN Beiwerk (F06). Der Ersatz unten
; entsteht nur mit dieser Aufgabe — ohne die Bedingung würde der alte Link
; bedingungslos verschwinden und kein neuer nachkommen. Konkreter Fall des
; Prüfers: Installation ohne Desktop-Aufgabe, der Nutzer legt sich die
; Verknüpfung später selbst an, das automatische Update übernimmt die alte
; Aufgabenauswahl — und hätte ihm den Desktop leer geräumt.
;
; ⭐ Die Regel dahinter: **Entfernt wird nur, wofür ein Ersatz gesichert ist.**
Type: files; Name: "{autodesktop}\SC BP Watcher.lnk"; Tasks: desktopicon

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\SC-BP-Watcher.exe"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\SC-BP-Watcher.exe"; \
  Tasks: desktopicon

[Registry]
; Autostart nur, wenn der Spieler es angehakt hat — und im Benutzerzweig,
; damit kein Administrator nötig ist.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; \
  ValueType: string; ValueName: "SC BP Watcher"; \
  ValueData: """{app}\SC-BP-Watcher.exe"""; Flags: uninsdeletevalue; \
  Tasks: autostart

[Run]
; ⚠ **`skipifsilent` gehört hierher.**
;
; Der Watcher ruft das Setup still auf, und den Neustart danach macht sein
; Helfer (`scbp/update_lauf.py`). Liefe dieser Eintrag im stillen Modus mit,
; kämen **zwei** Watcher hoch.
;
; ⚠ Richtiggestellt am 11.09.2026. Bis dahin stand hier, Inno 6.7 melde
;
;     Security validation failure: parent process has different executable!
;
; wenn ein Programm im Hintergrund ein anderes startet. Die Meldung stammt aber
; aus dem PyInstaller-Bootloader des Watchers, nicht aus Inno — nachgesehen in
; beiden Dateien. Sehr wahrscheinlich erbte der hier gestartete Watcher über
; das Setup die `_PYI_*`-Variablen des alten, hielt sich für das Kind eines
; Bootloaders und fand als Vater Innos Setup. Deshalb halfen die fünf Anläufe
; am Installer nichts (Umgebung säubern, Zwischenprozess, Ablösen,
; Kompatibilitäts-Shim entfernen): Sie suchten an der falschen Stelle. Der
; Helfer startet den Watcher heute mit einer Umgebung ohne diese Variablen.
;
; Bei einer Installation **von Hand** ändert sich nichts: Dort zeigt
; `postinstall` weiterhin das Häkchen „SC BP Watcher starten" auf der letzten
; Seite.
Filename: "{app}\SC-BP-Watcher.exe"; \
  Description: "{cm:LaunchProgram,{#AppName}}"; \
  Flags: nowait postinstall skipifsilent

; Kein [UninstallDelete] für die Nutzerdaten: Wer deinstalliert, will das
; Programm loswerden — nicht seinen über Monate gesammelten Bauplan-Bestand.

[Code]
{ ⚠ Der Restart Manager schliesst das Programm — er haelt es aber nicht unten.

  Gemessen am 28.08.2026 (beim Update rc75 → rc83), im Setup-Protokoll
  Zeile fuer Zeile belegt:

      05:43:47  RestartManager found an application using one of our files
      05:43:47  Shutting down applications using our files. (forced)
      05:43:55  << der Watcher laeuft wieder — Elternprozess explorer.exe >>
      05:44:17  DeleteFile: The existing file appears to be in use (5). Retrying.
                (viermal, dann: DeleteFile schlug fehl; Code 5)

  `CloseApplications=force` hat sauber gearbeitet. Acht Sekunden spaeter hat der
  **Autostart** den Watcher wieder hochgefahren — der Wert unter
  HKCU\...\CurrentVersion\Run, den der [Registry]-Abschnitt weiter oben selbst
  anlegt. Windows arbeitet diese Werte verzoegert nach dem Start von
  `explorer.exe` ab; war die Shell kurz vorher neu gestartet (Absturz, frische
  Anmeldung, `explorer.exe` von Hand neu gestartet), faellt diese Verzoegerung
  genau in die laufende Installation.

  Bewiesen ist es ueber den **Elternprozess**: `explorer.exe`. Haette der Watcher
  sich selbst neu gestartet — die naheliegende Vermutung ueber `neu_starten()` —
  stuende dort der alte Watcher-Prozess oder `cmd.exe`.

  ⚠ `CloseApplications` kann das prinzipiell nicht loesen: Es schliesst **einmal**,
  vor dem Kopieren, und was danach hochkommt, sieht es nicht mehr. Inno wiederholt
  von sich aus nur viermal im Sekundenabstand — gegen einen Autostart, der acht
  Sekunden nach dem Schliessen feuert, kommt es damit nicht an.

  Deshalb wird hier direkt vor dem Kopieren nachgefasst. `PrepareToInstall` ist
  die richtige Stelle: Inno ruft es **nach** `CloseApplications` und **vor** dem
  ersten Dateieintrag auf.

  ⚠ Nur beim **Update**, nicht bei der Erstinstallation — sonst warten Nutzer, bei
  denen gar nichts laufen kann. Erkannt daran, dass die Zieldatei schon da ist.

  Hart beenden ist hier vertretbar, aus demselben Grund wie bei
  `CloseApplications=force` weiter oben: Der Bauplan-Bestand liegt in
  `Dokumente\SC BP Watcher` und wird bei jeder Aenderung geschrieben, nicht erst
  beim Beenden. }

const
  WATCHER_EXE = 'SC-BP-Watcher.exe';

{ Einmal hart beenden. Rueckgabe ist der Rueckgabewert von taskkill:
  0 = etwas beendet, 128 = kein solcher Prozess, -1 = taskkill nicht startbar. }
function WatcherBeenden(): Integer;
var
  RC: Integer;
begin
  Result := -1;
  if Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM ' + WATCHER_EXE,
          '', SW_HIDE, ewWaitUntilTerminated, RC) then
    Result := RC;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  I: Integer;
begin
  Result := '';        { leer heisst: weitermachen — alles andere bricht ab }

  { Kein Update, also niemand, der die Datei halten koennte. }
  if not FileExists(ExpandConstant('{app}\') + WATCHER_EXE) then
    Exit;

  { Dreimal mit kurzem Abstand: Der erste Durchgang raeumt weg, was der Restart
    Manager stehen liess; die zwei folgenden fangen einen Autostart ab, der
    genau in diesem Moment nachfeuert. Kostet im Normalfall gut eine Sekunde. }
  for I := 1 to 3 do
  begin
    WatcherBeenden();
    if I < 3 then
      Sleep(600);
  end;
end;

{ ⚠ Der Autostart wird an ZWEI Stellen gesetzt — und nur eine raeumt auf.

  Der [Registry]-Abschnitt oben legt den Wert an, wenn beim Installieren das
  Haekchen "Mit Windows starten" gewaehlt wurde; `uninsdeletevalue` raeumt genau
  diesen Fall wieder weg. Das Programm selbst schreibt denselben Wert aber auch
  (`scbp/autostart.py`, NAME = 'SC BP Watcher') — und davon weiss der
  Deinstaller nichts.

  Folge, gemessen am 28.08.2026 (gemessen): Nach dem Deinstallieren stand

      HKCU\...\CurrentVersion\Run
        "SC BP Watcher" -> C:\Users\...\Programs\SC BP Watcher\SC-BP-Watcher.exe

  weiter in der Registry und zeigte auf eine Datei, die es nicht mehr gab.
  Windows versucht sie bei jeder Anmeldung zu starten und scheitert still.
  Wer deinstalliert, will das Programm loswerden — nicht einen Eintrag behalten,
  von dem er nichts weiss.

  Deshalb wird der Wert beim Deinstallieren **immer** entfernt, unabhaengig
  davon, wer ihn gesetzt hat. `RegDeleteValue` stoert sich nicht daran, wenn er
  gar nicht da ist.

  ⚠ Nur der eine Wert, nicht der Schluessel: Unter `Run` stehen die
  Autostart-Eintraege aller Programme. }

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RegDeleteValue(HKEY_CURRENT_USER,
                   'Software\Microsoft\Windows\CurrentVersion\Run',
                   'SC BP Watcher');
end;

; VerseKit - installer for Windows (Inno Setup 6).
; Copyright (C) 2026 Xharig
; SPDX-License-Identifier: GPL-3.0-only
;
; Build:  ISCC /DAppVersion=<version> packaging\installer.iss
; Input:  target\release\versekit.exe and dist\erweiterung\ (packaging\hangar-exporter.ps1 fills it).
; Output: dist\VerseKit-Setup.exe
;
; The installer puts the program into the user's own program folder (no administrator, also not for the
; silent update) and never touches the player's data when installing. Uninstalling by hand asks whether the
; player's data go too (default: no); the program removes them itself (`data_removal::SWITCH`), only its own
; files. A silent uninstall never removes them.
;
; The values below are defaults; a check of the installer on a working computer gives each of them its own
; value (/DTestRun=1 and the others), so that it never meets the real installation, its start menu entries,
; its start with Windows or its running program.

#ifndef AppName
  #define AppName "VerseKit"
#endif
#define AppPublisher "Xharig"
#define AppURL "https://github.com/Xharig/VerseKit"
#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
; The identity of the installation. It never changes: the update of every earlier version finds its
; installation by it (one entry in "Apps & Features", one folder).
#ifndef AppIdGuid
  #define AppIdGuid "7C4B1E93-2A6F-4D58-B0E1-9F3A5C8D2461"
#endif
; The program file in the installation folder. 5.0 keeps the file name of 4.12: the update helper of 4.12
; starts exactly this file once the setup has run.
#ifndef ProgramFile
  #define ProgramFile "SC-BP-Watcher.exe"
#endif
; The value under the user's Run key; the program writes the same name (`autostart::NAME`).
#ifndef RunValue
  #define RunValue "VerseKit"
#endif
; Where the installer names its program file for the update helper of 5.2.2 and earlier, which starts that file.
#ifndef ProgramKey
  #define ProgramKey "Software\Xharig\VerseKit"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\versekit.exe"
#endif
#ifndef ExtensionDir
  #define ExtensionDir "..\dist\erweiterung"
#endif
#ifndef OutputName
  #define OutputName "VerseKit-Setup"
#endif

; Windows wants plain numbers for the file version: "5.0.0-rc1" becomes "5.0.0".
#define NumberVersion AppVersion
#if Pos("-", NumberVersion) > 0
  #define NumberVersion Copy(NumberVersion, 1, Pos("-", NumberVersion) - 1)
#endif

[Setup]
AppId={{{#AppIdGuid}}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}/issues
AppUpdatesURL={#AppURL}/releases
VersionInfoVersion={#NumberVersion}
VersionInfoProductName={#AppName}
VersionInfoCopyright=Copyright (C) 2026 Xharig. GPL-3.0-only
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

; The running program is closed before its file is replaced; `force` ends it also when it does not answer.
; No AppMutex: it only asks the user to close the program and stops the silent update.
CloseApplications=force
; The Restart Manager starts nothing again: after an update the program started (/VKUPDATE) DeinitializeSetup
; starts it, after the silent update of the helper of 5.2.2 and earlier that helper, and [Run] after an
; installation by hand. Two ways would start it twice.
RestartApplications=no

; The user's own program folder, no administrator. An existing installation keeps its folder; the default is
; only for a new one.
PrivilegesRequired=lowest
UsePreviousAppDir=yes
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
; The start menu folder carries the current name, also over an installation with an earlier name.
UsePreviousGroup=no
DisableProgramGroupPage=yes
DisableDirPage=auto

OutputDir=..\dist
; Every update looks for a file ending in "-setup.exe": the name stays.
OutputBaseFilename={#OutputName}
SetupIconFile=..\icon.ico
UninstallDisplayIcon={app}\{#ProgramFile}
UninstallDisplayName={#AppName}
Compression=lzma2
SolidCompression=yes
WizardStyle=modern

[Languages]
Name: "deutsch"; MessagesFile: "compiler:Languages\German.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
deutsch.AutostartTask=Mit Windows starten
deutsch.AfterInstallGroup=Nach der Installation:
english.AutostartTask=Start with Windows
english.AfterInstallGroup=After installation:
deutsch.StillRunning=VerseKit läuft noch und lässt sich nicht beenden – wahrscheinlich wurde es als Administrator gestartet.%n%nBitte beende VerseKit selbst: Rechtsklick auf das VerseKit-Symbol im Infobereich neben der Uhr, dann „Beenden“ (oder im Task-Manager). Klicke danach auf „Wiederholen“.
english.StillRunning=VerseKit is still running and cannot be closed – it was probably started as administrator.%n%nPlease close VerseKit yourself: right-click the VerseKit icon in the notification area next to the clock, then "Quit" (or use Task Manager). Then click "Retry".
deutsch.StillRunningSilent=VerseKit läuft noch und lässt sich nicht beenden.
english.StillRunningSilent=VerseKit is still running and cannot be closed.
deutsch.CannotCheck=Es lässt sich nicht prüfen, ob VerseKit noch läuft.%n%nBitte beende VerseKit selbst, falls es läuft: Rechtsklick auf das VerseKit-Symbol im Infobereich neben der Uhr, dann „Beenden“. Klicke danach auf „Wiederholen“.
english.CannotCheck=It cannot be checked whether VerseKit is still running.%n%nPlease close VerseKit yourself if it runs: right-click the VerseKit icon in the notification area next to the clock, then "Quit". Then click "Retry".
deutsch.NetworkStays=Dein Datenordner auf dem Netzlaufwerk wird nicht angefasst und bleibt liegen:%n%n%1
english.NetworkStays=Your data folder on the network drive is not touched and stays:%n%n%1
deutsch.DeleteDataQuestion=Sollen auch deine VerseKit-Daten gelöscht werden?%n%nDazu gehören deine Baupläne, dein Lager, deine Einstellungen und alles andere, was VerseKit gespeichert hat. Gelöscht werden VerseKits Ordner und Dateien mit VerseKits Dateinamen an diesen Orten:%n%n%1%nAndere Dateien bleiben.%n%n%2Auch VerseKits eigene Sicherungen (vom ersten Start und vor Updates) werden gelöscht. Das lässt sich nicht rückgängig machen. Wähle „Nein“, wenn du VerseKit später wieder installieren und deine Daten behalten willst.
english.DeleteDataQuestion=Also delete your VerseKit data?%n%nThis includes your blueprints, your inventory, your settings and everything else VerseKit has saved. VerseKit's folders and the files with VerseKit's file names are deleted, at these places:%n%n%1%nOther files stay.%n%n%2VerseKit's own backups (from the first start and before updates) are deleted too. This cannot be undone. Choose "No" if you want to install VerseKit again later and keep your data.
deutsch.DeletingData=VerseKit-Daten werden gelöscht …
english.DeletingData=Deleting VerseKit data …
deutsch.DeleteDataFailed=Ein Teil deiner VerseKit-Daten konnte nicht gelöscht werden (vielleicht ist eine Datei noch geöffnet). Er liegt noch an den genannten Orten und kann von Hand gelöscht werden.
english.DeleteDataFailed=Part of your VerseKit data could not be deleted (perhaps a file is still open). It is still at the places shown and can be deleted by hand.

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "autostart"; Description: "{cm:AutostartTask}"; GroupDescription: "{cm:AfterInstallGroup}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#ProgramFile}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
; The Hangar-Exporter for Firefox (signed by Mozilla) with its licence and the address of its source. The
; installer only puts the file there; it installs nothing into a browser.
Source: "{#ExtensionDir}\*"; DestDir: "{app}\erweiterung"; Flags: ignoreversion

; Removed before the new files come: an older Hangar-Exporter, and the shortcuts of the earlier product names.
; Only the exact names an earlier installer made, never a pattern: a shortcut the user made with the product
; name in it stays. A desktop shortcut goes only together with its replacement (task "desktopicon").
[InstallDelete]
Type: files; Name: "{app}\erweiterung\*.xpi"
#ifndef TestRun
Type: files; Name: "{autoprograms}\SC BP Watcher\SC BP Watcher.lnk"
Type: files; Name: "{autoprograms}\SC BP Watcher\{cm:UninstallProgram,SC BP Watcher}.lnk"
Type: files; Name: "{autoprograms}\VerseKit\SC BP Watcher.lnk"
Type: files; Name: "{autoprograms}\VerseKit\{cm:UninstallProgram,SC BP Watcher}.lnk"
Type: files; Name: "{autoprograms}\Verse-Kit\Verse-Kit.lnk"
Type: files; Name: "{autoprograms}\Verse-Kit\{cm:UninstallProgram,Verse-Kit}.lnk"
Type: dirifempty; Name: "{autoprograms}\SC BP Watcher"
Type: dirifempty; Name: "{autoprograms}\Verse-Kit"
Type: files; Name: "{autodesktop}\SC BP Watcher.lnk"; Tasks: desktopicon
Type: files; Name: "{autodesktop}\Verse-Kit.lnk"; Tasks: desktopicon
#endif

[Icons]
Name: "{group}\{#AppName}"; Filename: "{app}\{#ProgramFile}"
Name: "{group}\{cm:UninstallProgram,{#AppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#ProgramFile}"; Tasks: desktopicon

[Registry]
; The start with Windows, when it was chosen, on a new installation only: over an existing one the program
; takes over the entry of an earlier name itself and keeps it switched off when the user switched it off.
; The switch tells the program that the system started it (`autostart::SYSTEM_START`): only a start by hand opens
; the main window by itself.
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#RunValue}"; \
  ValueData: """{app}\{#ProgramFile}"" ""--systemstart"""; Flags: uninsdeletevalue; Tasks: autostart; Check: IsNewInstallation
Root: HKCU; Subkey: "{#ProgramKey}"; ValueType: string; ValueName: "Programm"; ValueData: "{app}\{#ProgramFile}"; \
  Flags: uninsdeletekey

[Run]
; Only after an installation by hand: after an update DeinitializeSetup or the helper of 5.2.2 and earlier starts
; the program.
Filename: "{app}\{#ProgramFile}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent; Check: not IsUpdateRun

[Code]
{ An update the program started itself (`update_run::setup_arguments`) comes with these switches:
    /VKUPDATE                 the run is an update of the program
    /VKPIDS=1,2               the processes of the program to wait for before installing
    /VKSPERRE="..."           the lock, removed before the program is started again
    /VKERGEBNIS="..."         where the exit code goes
    /VKPROTOKOLL="..."        the update log the lines go to
    /VKKEINSTART              the player ended the program: it is not started again afterwards
  Without /VKUPDATE (by hand, or the helper of 5.2.2 and earlier) none of this happens.
  The program steps down only once this process has written LIFE_SIGN into the update log
  (`update_run::LIFE_SIGN`); both sides change together. }

const
  RUN_KEY = 'Software\Microsoft\Windows\CurrentVersion\Run';
  SYNCHRONIZE = $00100000;
  WAIT_OBJECT_0 = 0;
  { How long the update waits for the program to end by itself, in milliseconds. }
  WAIT_MILLISECONDS = 60000;
  LOCK_NAME = 'update-sperre.json';
  RESULT_NAME = 'update-ergebnis.txt';
  LOG_NAME = 'update-helfer.txt';
  LIFE_SIGN = 'Installer started';
  { The exit code written when the program did not end by itself (as the helper of 5.2.2 and earlier). }
  CODE_OLD_STUCK = 91;
  { How long a program ended by force may take to leave the process list, and how often it is asked, in
    milliseconds. }
  PROCESS_WAIT = 5000;
  PROCESS_POLL = 200;
  { What the process list says of the program. }
  PROGRAM_GONE = 0;
  PROGRAM_RUNS = 1;
  PROGRAM_UNKNOWN = 2;

var
  WasInstalled: Boolean;
  { How far Setup came, for the exit code the update writes. }
  Prepared: Boolean;
  Installing: Boolean;
  Installed: Boolean;
  Stuck: Boolean;
  { The program was ended and checked at the start of the uninstall. }
  UninstallStarted: Boolean;

function OpenProcess(DesiredAccess: DWORD; InheritHandle: BOOL; ProcessId: DWORD): THandle;
  external 'OpenProcess@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function WaitForSingleObject(Handle: THandle; Milliseconds: DWORD): DWORD;
  external 'WaitForSingleObject@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function CloseHandle(Handle: THandle): BOOL;
  external 'CloseHandle@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function GetTickCount(): DWORD;
  external 'GetTickCount@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function GetCurrentProcessId(): DWORD;
  external 'GetCurrentProcessId@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function ProcessIdToSessionId(ProcessId: DWORD; var SessionId: DWORD): BOOL;
  external 'ProcessIdToSessionId@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }

function IsNewInstallation(): Boolean;
begin
  Result := not WasInstalled;
end;

{ Was Setup started with the switch `Switch` (without a value, case does not matter)? }
function HasSwitch(const Switch: String): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), Switch) = 0 then
    begin
      Result := True;
      Exit;
    end;
end;

{ Was Setup started with /VKUPDATE? }
function IsUpdateRun(): Boolean;
begin
  Result := HasSwitch('/VKUPDATE');
end;

{ Was Setup started with /VKKEINSTART? The player ended the program and the update came at its end: the program is
  not started again. }
function IsNoStart(): Boolean;
begin
  Result := HasSwitch('/VKKEINSTART');
end;

{ The file a switch names, only when its name is `Name`; else empty. }
function UpdateFile(const Switch, Name: String): String;
begin
  Result := ExpandConstant('{param:' + Switch + '|}');
  if CompareText(ExtractFileName(Result), Name) <> 0 then
    Result := '';
end;

{ One line in the update log, in the form the program writes: date, time and the entry. }
procedure UpdateLog(const Entry: String);
var
  Path: String;
  Lines: TArrayOfString;
begin
  Path := UpdateFile('VKPROTOKOLL', LOG_NAME);
  if Path = '' then
    Exit;
  SetArrayLength(Lines, 1);
  Lines[0] := GetDateTimeString('yyyy/mm/dd hh:nn:ss', '-', ':') + ' ' + Entry;
  SaveStringsToUTF8FileWithoutBOM(Path, Lines, True);
end;

{ Waits until every process named in /VKPIDS has ended, at most Milliseconds in all (0: only a look). True when
  they have; a process that cannot be opened counts as ended. }
function WaitForProgram(Milliseconds: DWORD): Boolean;
var
  List, Item: String;
  Comma: Integer;
  Pid: Integer;
  Handle: THandle;
  Started, Spent, Left: DWORD;
begin
  Result := True;
  List := ExpandConstant('{param:VKPIDS|}');
  Started := GetTickCount();
  while List <> '' do
  begin
    Comma := Pos(',', List);
    if Comma > 0 then
    begin
      Item := Copy(List, 1, Comma - 1);
      Delete(List, 1, Comma);
    end
    else
    begin
      Item := List;
      List := '';
    end;
    Pid := StrToIntDef(Trim(Item), 0);
    if Pid > 0 then
    begin
      Handle := OpenProcess(SYNCHRONIZE, False, Pid);
      if Handle <> 0 then
      begin
        Spent := GetTickCount() - Started;
        if Spent < Milliseconds then
          Left := Milliseconds - Spent
        else
          Left := 0;
        if WaitForSingleObject(Handle, Left) <> WAIT_OBJECT_0 then
          Result := False;
        CloseHandle(Handle);
      end;
    end;
  end;
end;

{ The exit code Setup returns, as far as the script sees it: 0 installed, 5 the installation began and was
  rolled back, 91 the program did not end, 7 stopped while preparing, 2 ended before. }
function UpdateCode(): Integer;
begin
  if Installed then
    Result := 0
  else if Installing then
    Result := 5
  else if Stuck then
    Result := CODE_OLD_STUCK
  else if Prepared then
    Result := 7
  else
    Result := 2;
end;

{ In an update the program started: the first line of this process in the update log, the sign the program
  waits for before it steps down. }
function InitializeSetup(): Boolean;
begin
  Result := True;
  if IsUpdateRun() then
    UpdateLog(LIFE_SIGN);
end;

{ The program file to start after the update: in the installation folder, else in the folder of /DIR. }
function UpdatedProgram(): String;
begin
  try
    Result := ExpandConstant('{app}\{#ProgramFile}');
  except
    Result := AddBackslash(ExpandConstant('{param:DIR|}')) + '{#ProgramFile}';
  end;
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssInstall then
    Installing := True
  else if CurStep = ssPostInstall then
    Installed := True;
end;

{ At the end of an update the program started: the exit code and the log lines for the program. Once the
  processes of /VKPIDS have ended: the lock removed, and the program started again, the new one or, after a
  cancelled or failed installation, the old one. While one of them still runs, nothing is started (it would run
  twice) and the lock stays with whoever holds it. }
procedure DeinitializeSetup();
var
  Code, StartCode: Integer;
  Path, Exe: String;
begin
  if not IsUpdateRun() then
    Exit;
  Code := UpdateCode();
  UpdateLog(Format('Installer finished, exit code %d', [Code]));
  Path := UpdateFile('VKERGEBNIS', RESULT_NAME);
  if (Path <> '') and not SaveStringToFile(Path, IntToStr(Code) + #13#10, False) then
    UpdateLog('Exit code not written');
  if not WaitForProgram(0) then
  begin
    UpdateLog('The program still runs - not started again');
    Exit;
  end;
  Path := UpdateFile('VKSPERRE', LOCK_NAME);
  if Path <> '' then
    DeleteFile(Path);
  if IsNoStart() then
  begin
    UpdateLog('The program was ended by the player - not started again');
    Exit;
  end;
  Exe := UpdatedProgram();
  if not FileExists(Exe) then
    UpdateLog('No program to start: ' + Exe)
  else
  begin
    UpdateLog('Starting the program ' + Exe);
    if not Exec(Exe, '', ExtractFileDir(Exe), SW_SHOWNORMAL, ewNoWait, StartCode) then
      UpdateLog('Start failed: ' + SysErrorMessage(StartCode));
  end;
end;

{ The filter for taskkill and tasklist that keeps them to the Windows session of this process: the installation is
  the user's own, and the program of another user signed in on this computer is none of its business. Empty when
  the session cannot be asked. }
function SessionFilter(): String;
var
  Session: DWORD;
begin
  Result := '';
  if ProcessIdToSessionId(GetCurrentProcessId(), Session) then
    Result := '/FI "SESSION eq ' + IntToStr(Session) + '" ';
end;

{ Ends every process of the program file in this session once; returns the exit code of taskkill. }
function EndProgram(): Integer;
var
  Code: Integer;
begin
  Result := -1;
  if Exec(ExpandConstant('{sys}\taskkill.exe'), '/F ' + SessionFilter() + '/IM {#ProgramFile}', '', SW_HIDE,
      ewWaitUntilTerminated, Code) then
    Result := Code;
end;

{ Ends the program three times with a short pause: the start with Windows can bring it up again right after. }
procedure EndProgramThrice();
var
  I: Integer;
begin
  for I := 1 to 3 do
  begin
    EndProgram();
    if I < 3 then
      Sleep(600);
  end;
end;

{ Does a process of the program file still run in this session? Asks the process list, which also shows processes
  this one may not end, such as one started as administrator. PROGRAM_UNKNOWN when the list cannot be had. A check
  setup takes the answer from /VKLAEUFTNOCH (runs, as a program it may not end) and /VKLISTEFEHLT (unknown). }
function ProgramState(): Integer;
var
  List: String;
  Code: Integer;
  Lines: AnsiString;
begin
#ifdef TestRun
  if HasSwitch('/VKLAEUFTNOCH') then
  begin
    Result := PROGRAM_RUNS;
    Exit;
  end;
  if HasSwitch('/VKLISTEFEHLT') then
  begin
    Result := PROGRAM_UNKNOWN;
    Exit;
  end;
  { The program came back after the check at the start of the uninstall. }
  if HasSwitch('/VKLAEUFTWIEDER') and UninstallStarted then
  begin
    Result := PROGRAM_RUNS;
    Exit;
  end;
#endif
  Result := PROGRAM_UNKNOWN;
  List := ExpandConstant('{tmp}\processes.csv');
  DeleteFile(List);
  if Exec(ExpandConstant('{cmd}'), '/C ""' + ExpandConstant('{sys}\tasklist.exe') + '" ' + SessionFilter() +
      '/FI "IMAGENAME eq {#ProgramFile}" /FO CSV /NH > "' + List + '""', '', SW_HIDE, ewWaitUntilTerminated, Code)
    and (Code = 0) and LoadStringFromFile(List, Lines) then
  begin
    if Pos('"' + Lowercase('{#ProgramFile}') + '"', Lowercase(String(Lines))) > 0 then
      Result := PROGRAM_RUNS
    else
      Result := PROGRAM_GONE;
  end;
end;

{ The state once the program has had time to go: asks again every PROCESS_POLL milliseconds for at most
  PROCESS_WAIT while it still runs, since a process ended by force takes a moment to leave the list. }
function StateAfterEnd(): Integer;
var
  Started: DWORD;
begin
  Started := GetTickCount();
  Result := ProgramState();
  while (Result = PROGRAM_RUNS) and (GetTickCount() - Started < PROCESS_WAIT) do
  begin
    Sleep(PROCESS_POLL);
    Result := ProgramState();
  end;
end;

{ Ends the program and makes sure it has ended. While it still runs (it was started as administrator, and this
  process may not end it), and with `UnknownStops` also while nobody can tell, the user is asked to end it and
  retry; a silent run gets the default answer and stops. True once it has ended, or nobody can tell and
  `UnknownStops` is not set. }
function ProgramEnded(UnknownStops: Boolean): Boolean;
var
  State: Integer;
  Text: String;
begin
  EndProgramThrice();
  State := StateAfterEnd();
  while (State = PROGRAM_RUNS) or ((State = PROGRAM_UNKNOWN) and UnknownStops) do
  begin
    if State = PROGRAM_RUNS then
      Text := CustomMessage('StillRunning')
    else
      Text := CustomMessage('CannotCheck');
    if SuppressibleMsgBox(Text, mbError, MB_RETRYCANCEL, IDCANCEL) <> IDRETRY then
    begin
      Result := False;
      Exit;
    end;
    EndProgramThrice();
    State := StateAfterEnd();
  end;
  Result := True;
end;

{ Right before the files are copied. In an update the program started: first the wait until its processes have
  ended by themselves; one that has not ended after WAIT_MILLISECONDS stops the update (it is never ended by
  force: it may be writing). Then, over an existing installation or while the program runs, the program is ended
  three times with a short pause, because the start with Windows can bring it up again after the Restart Manager
  closed it. One that still runs then (started as administrator: neither this process nor the Restart Manager may
  end it) stops Setup before any file is copied, after the user was asked to end it. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if IsUpdateRun() then
  begin
    UpdateLog('Waiting for the program to exit');
    if WaitForProgram(WAIT_MILLISECONDS) then
      UpdateLog('Old version has exited')
    else
    begin
      UpdateLog('The old version does not exit - nothing installed');
      Stuck := True;
      Result := 'VerseKit did not exit.';
      Exit;
    end;
  end;
  WasInstalled := FileExists(ExpandConstant('{app}\{#ProgramFile}'));
  { A process list that cannot be read does not stop Setup or the update: the program was ended as always, and only
    one that is seen to run on stops it. }
  if (WasInstalled or (ProgramState() = PROGRAM_RUNS)) and not ProgramEnded(False) then
  begin
    { The file of a running program cannot be replaced (access denied): stopped here, nothing is half installed. }
    if IsUpdateRun() then
      UpdateLog('The program does not exit (started as administrator?) - nothing installed');
    Stuck := True;
    Result := CustomMessage('StillRunningSilent');
    Exit;
  end;
  Prepared := True;
end;

{ Removes the start with Windows under every name the program and the installers wrote, whoever wrote it; only
  the values, never the Run key. }
procedure RemoveStartWithWindows();
begin
  RegDeleteValue(HKEY_CURRENT_USER, RUN_KEY, '{#RunValue}');
#ifndef TestRun
  RegDeleteValue(HKEY_CURRENT_USER, RUN_KEY, 'Verse-Kit');
  RegDeleteValue(HKEY_CURRENT_USER, RUN_KEY, 'SC BP Watcher');
#endif
end;

{ Runs the installed program with `Switch` on the player's data and returns its exit code, -1 when it could not be
  started. A check setup runs it only on the copy of the data named by /VKDATEN (`--daten`, writing locked to it),
  never on the data of the computer it runs on, and not at all without it. }
function RunOnData(const Switch: String): Integer;
var
  Arguments: String;
  Code: Integer;
begin
  Result := -1;
  Arguments := Switch;
#ifdef TestRun
  if ExpandConstant('{param:VKDATEN|}') = '' then
    Exit;
  Arguments := Arguments + ' --daten ' + AddQuotes(ExpandConstant('{param:VKDATEN|}'));
#endif
  if Exec(ExpandConstant('{app}\{#ProgramFile}'), Arguments, ExpandConstant('{app}'), SW_HIDE,
      ewWaitUntilTerminated, Code) then
    Result := Code;
end;

{ The places with the player's own data, as the program finds them (`data_removal::PLACES_SWITCH`), one per line
  and indented for the question: `Clear` the ones it clears, `Kept` the data folders on the network it leaves
  (a line that begins with `data_removal::NETWORK_MARK`). Empty both when the program cannot name them (the data
  folder named from outside). }
procedure DataPlaces(var Clear, Kept: String);
var
  List, Line: String;
  Lines: TArrayOfString;
  I: Integer;
begin
  Clear := '';
  Kept := '';
  List := ExpandConstant('{tmp}\places.txt');
  if (RunOnData('--eigene-daten-orte ' + AddQuotes(List)) <> 0) or not LoadStringsFromFile(List, Lines) then
    Exit;
  for I := 0 to GetArrayLength(Lines) - 1 do
  begin
    Line := Trim(Lines[I]);
    if Line = '' then
      Continue;
    if Copy(Line, 1, 1) = '-' then
      Kept := Kept + '    ' + Copy(Line, 2, Length(Line) - 1) + #13#10
    else
      Clear := Clear + '    ' + Line + #13#10;
  end;
end;

{ The question whether the player's own data go too, with their places; asked only when uninstalling by hand,
  "No" is the default. A silent uninstall never removes them. A check setup takes the answer "yes" from
  /VKDATENLOESCHEN, without a question. Then the program removes them itself, only its own files
  (`data_removal::SWITCH`); it runs from the installation folder while its file is still there, so this comes
  before the files of the installation go. }
procedure OfferDataRemoval();
var
  Clear, Kept, Note: String;
  Chosen: Boolean;
begin
  Chosen := False;
#ifdef TestRun
  Chosen := HasSwitch('/VKDATENLOESCHEN');
#endif
  if UninstallSilent() and not Chosen then
    Exit;
  DataPlaces(Clear, Kept);
  Note := '';
  if Kept <> '' then
    Note := FmtMessage(CustomMessage('NetworkStays'), [Kept]) + #13#10;
  if Clear = '' then
  begin
    { Nothing to remove; a data folder on the network is named so the user knows it stays. }
    if (Note <> '') and not Chosen then
      MsgBox(Note, mbInformation, MB_OK);
    Exit;
  end;
  if not Chosen then
    Chosen := MsgBox(FmtMessage(CustomMessage('DeleteDataQuestion'), [Clear, Note]), mbConfirmation,
      MB_YESNO or MB_DEFBUTTON2) = IDYES;
  if not Chosen then
    Exit;
  UninstallProgressForm.StatusLabel.Caption := CustomMessage('DeletingData');
  if RunOnData('--eigene-daten-loeschen') <> 0 then
    SuppressibleMsgBox(CustomMessage('DeleteDataFailed'), mbInformation, MB_OK, IDOK);
end;

{ At the start of the uninstall: the program ended and checked to have ended, since the file of a running program
  cannot be removed (access denied) and would stay behind. A program that still runs, or a process list that cannot
  be read, stops the uninstall before anything is removed (False here is the documented way to stop it). }
function InitializeUninstall(): Boolean;
begin
  Result := ProgramEnded(True);
  UninstallStarted := Result;
end;

{ Before the files go: the program ended and checked once more (the start with Windows may have brought it back
  while the user confirmed); one that runs again and cannot be ended stops the uninstall here, before anything is
  removed (Inno's help on Abort names this step). Then the start with Windows removed, and the question about the
  player's data. After the files: the start with Windows once more. }
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usUninstall then
  begin
    if not ProgramEnded(True) then
      Abort;
    RemoveStartWithWindows();
    OfferDataRemoval();
  end
  else if CurUninstallStep = usPostUninstall then
    RemoveStartWithWindows();
end;

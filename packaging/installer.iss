; VerseKit - installer for Windows (Inno Setup 6).
; Copyright (C) 2026 Xharig
; SPDX-License-Identifier: GPL-3.0-only
;
; Build:  ISCC /DAppVersion=<version> packaging\installer.iss
; Input:  target\release\versekit.exe and dist\erweiterung\ (packaging\hangar-exporter.ps1 fills it).
; Output: dist\VerseKit-Setup.exe
;
; The installer puts the program into the user's own program folder (no administrator, also not for the
; silent update) and never touches the player's data folder, neither when installing nor when uninstalling.
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
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "{#RunValue}"; \
  ValueData: """{app}\{#ProgramFile}"""; Flags: uninsdeletevalue; Tasks: autostart; Check: IsNewInstallation
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

var
  WasInstalled: Boolean;
  { How far Setup came, for the exit code the update writes. }
  Prepared: Boolean;
  Installing: Boolean;
  Installed: Boolean;
  Stuck: Boolean;

function OpenProcess(DesiredAccess: DWORD; InheritHandle: BOOL; ProcessId: DWORD): THandle;
  external 'OpenProcess@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function WaitForSingleObject(Handle: THandle; Milliseconds: DWORD): DWORD;
  external 'WaitForSingleObject@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function CloseHandle(Handle: THandle): BOOL;
  external 'CloseHandle@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }
function GetTickCount(): DWORD;
  external 'GetTickCount@kernel32.dll stdcall'; { privacy-ok: a function of a Windows library, no address }

function IsNewInstallation(): Boolean;
begin
  Result := not WasInstalled;
end;

{ Was Setup started with /VKUPDATE? }
function IsUpdateRun(): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 1 to ParamCount do
    if CompareText(ParamStr(I), '/VKUPDATE') = 0 then
    begin
      Result := True;
      Exit;
    end;
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

{ Ends every process of the program file once; returns the exit code of taskkill. }
function EndProgram(): Integer;
var
  Code: Integer;
begin
  Result := -1;
  if Exec(ExpandConstant('{sys}\taskkill.exe'), '/F /IM {#ProgramFile}', '', SW_HIDE, ewWaitUntilTerminated, Code) then
    Result := Code;
end;

{ Right before the files are copied. In an update the program started: first the wait until its processes have
  ended by themselves; one that has not ended after WAIT_MILLISECONDS stops the update (it is never ended by
  force: it may be writing). Then, only over an existing installation, the program is ended three times with a
  short pause, because the start with Windows can bring it up again after the Restart Manager closed it. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  I: Integer;
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
  if WasInstalled then
    for I := 1 to 3 do
    begin
      EndProgram();
      if I < 3 then
        Sleep(600);
    end;
  Prepared := True;
end;

{ Uninstalling removes the start with Windows under every name the program and the installers wrote, whoever
  wrote it; only the values, never the Run key. }
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
  begin
    RegDeleteValue(HKEY_CURRENT_USER, RUN_KEY, '{#RunValue}');
#ifndef TestRun
    RegDeleteValue(HKEY_CURRENT_USER, RUN_KEY, 'Verse-Kit');
    RegDeleteValue(HKEY_CURRENT_USER, RUN_KEY, 'SC BP Watcher');
#endif
  end;
end;

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
; Where the installer names its program file for the update helper, which starts that file.
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
; The Restart Manager starts nothing again: after a silent update the update helper starts the program, and
; [Run] after an installation by hand. Two ways would start it twice.
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
; Only after an installation by hand: after the silent update the update helper starts the program.
Filename: "{app}\{#ProgramFile}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
const
  RUN_KEY = 'Software\Microsoft\Windows\CurrentVersion\Run';

var
  WasInstalled: Boolean;

function IsNewInstallation(): Boolean;
begin
  Result := not WasInstalled;
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

{ Right before the files are copied, and only over an existing installation: the program is ended three times
  with a short pause, because the start with Windows can bring it up again after the Restart Manager closed it. }
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  I: Integer;
begin
  Result := '';
  WasInstalled := FileExists(ExpandConstant('{app}\{#ProgramFile}'));
  if not WasInstalled then
    Exit;
  for I := 1 to 3 do
  begin
    EndProgram();
    if I < 3 then
      Sleep(600);
  end;
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

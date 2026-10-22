; AirQlality Windows installer (EXP29).
;
; One self-contained exe, no runtime DLLs, per-user install: no UAC, no service,
; no MSI. Inno Setup 6.3+ (compiled with `ISCC.exe`), driven by
; `.github/workflows/release.yml`; see `experiments/EXP29/README.md`.
;
; Everything the pipeline varies comes in on the command line:
;   ISCC.exe /DAppVersion=0.2.0 /DSourceExe=..\target\dist\airqlality.exe ^
;            /DSourceIcon=..\target\AirQlality.ico installer\airqlality.iss
; The defaults make a local run work from a `cargo build --release` tree.

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif
#ifndef SourceExe
  #define SourceExe "..\target\release\airqlality.exe"
#endif
#ifndef SourceIcon
  #define SourceIcon "..\target\AirQlality.ico"
#endif
#ifndef OutputDir
  #define OutputDir "..\dist"
#endif

#define AppName "AirQlality"
#define AppExeName "airqlality.exe"
#define AppUrl "https://github.com/Magid-William/AirQlality"
; The EXP12 startup value name, in `HKCU\...\Run`.
#define StartupValue "AirQlality"

[Setup]
; Stable across versions so an upgrade replaces the old install instead of
; stacking a second copy (the GUID is written into Add/Remove Programs).
AppId={{B7F4C2E1-5D3A-4C8B-9A16-7F2E9D4B8C31}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Magid William
AppPublisherURL={#AppUrl}
AppSupportURL={#AppUrl}/issues
AppUpdatesURL={#AppUrl}/releases/latest
; Per-user install: never an elevation prompt (the app writes only to HKCU and
; %APPDATA%, so it never needs admin either).
DefaultDirName={localappdata}\Programs\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=
; The same Windows baseline the README promises for the zip.
MinVersion=10.0.18362
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; One exe in, one exe out.
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
OutputDir={#OutputDir}
OutputBaseFilename={#AppName}-v{#AppVersion}-windows-x64-setup
SetupIconFile={#SourceIcon}
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#AppExeName}
LicenseFile=..\LICENSE
; Upgrade over a running tray app: Restart Manager offers to close it instead of
; failing on the locked exe. Never restart it behind the user's back.
CloseApplications=yes
RestartApplications=no
; A CI install/uninstall run leaves a parseable log next to the setup.
SetupLogging=yes
; No code signing certificate yet (EXP29): SmartScreen warns on first run, and
; the README says so. Wire a `SignTool=` name here when one exists.

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
; Not installed: extracted to {tmp} only when an upgrade has to stop a running
; copy (see PrepareToInstall below).
Source: "stop-app.ps1"; DestDir: "{tmp}"; Flags: dontcopy

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: desktopicon

[Run]
; Opt-in, and silent installs stay silent. "Run at startup" is the tray's job
; (EXP12), so the installer deliberately never writes the Run key itself.
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent

[Code]
// An upgrade must work while the widget sits in the tray - which is how it is
// normally running. `CloseApplications=yes` (above) is not enough: Restart
// Manager asks windows to close and the widget treats that as "hide to the tray"
// (EXP11), so setup fails with exit code 5 / "files in use". So stop *this*
// install's copy explicitly before any file is replaced. A portable copy running
// from another directory is left alone.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  ResultCode: Integer;
begin
  Result := '';
  if not FileExists(ExpandConstant('{app}\{#AppExeName}')) then
    Exit;
  ExtractTemporaryFile('stop-app.ps1');
  if not Exec(ExpandConstant('{sys}\WindowsPowerShell\v1.0\powershell.exe'),
      '-NoProfile -NonInteractive -ExecutionPolicy Bypass -File "' +
      ExpandConstant('{tmp}\stop-app.ps1') + '" -Path "' +
      ExpandConstant('{app}\{#AppExeName}') + '"',
      '', SW_HIDE, ewWaitUntilTerminated, ResultCode) then
  begin
    Result := 'AirQlality could not be stopped before the upgrade. Quit it from the tray menu (Quit) and run the setup again.';
    Exit;
  end;
  if ResultCode <> 0 then
    Result := 'AirQlality is still running. Quit it from the tray menu (Quit) and run the setup again.';
end;

// No [UninstallDelete]: only what the installer created is removed (the exe and
// the shortcuts Inno tracks). User settings live in %APPDATA%\AirQlality - API
// keys, the chosen location, the notify choices - and survive on purpose.
//
// A dangling autostart is a bug, so uninstall clears the EXP12 Run value - but
// only when it still points at *this* install's exe, never a value someone else
// (or a portable copy) owns.
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Stored: String;
  Expected: String;
begin
  if CurUninstallStep <> usPostUninstall then
    Exit;
  if not RegQueryStringValue(HKCU,
      'Software\Microsoft\Windows\CurrentVersion\Run', '{#StartupValue}', Stored) then
    Exit;
  Expected := '"' + ExpandConstant('{app}\{#AppExeName}') + '" --startup';
  if Lowercase(Trim(Stored)) <> Lowercase(Expected) then
    Exit;
  if RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run',
      '{#StartupValue}') then
    Log('removed the run-at-startup value');
end;

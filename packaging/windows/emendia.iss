; Compile with ISCC /DAppVersion=0.3.0 /DBuildLabel=v0.3.0
; /DSourceExe=<absolute executable path> /DOutputDir=<absolute output directory>.
#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef BuildLabel
  #define BuildLabel "v" + AppVersion
#endif
#ifndef SourceExe
  #define SourceExe "..\..\target\release\emendia.exe"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\target\dist"
#endif

[Setup]
AppId={{5BFA5FAF-5F24-4D2B-9D6B-A8DA0D05C353}
AppName=Emendia
AppVersion={#AppVersion}
AppPublisher=Emendia
AppPublisherURL=https://github.com/mbstdio/emendia
AppSupportURL=https://github.com/mbstdio/emendia/issues
AppUpdatesURL=https://github.com/mbstdio/emendia/releases
DefaultDirName={localappdata}\Programs\Emendia
DefaultGroupName=Emendia
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.18362
OutputDir={#OutputDir}
OutputBaseFilename=Emendia-{#BuildLabel}-windows-x64-setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
SetupIconFile=..\..\src\ressources\app-logo.ico
UninstallDisplayIcon={app}\emendia.exe
CloseApplications=yes
RestartApplications=no
LicenseFile=..\..\LICENSE

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "french"; MessagesFile: "compiler:Languages\French.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "emendia.exe"; Flags: ignoreversion
Source: "..\..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Emendia"; Filename: "{app}\emendia.exe"
Name: "{autodesktop}\Emendia"; Filename: "{app}\emendia.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\emendia.exe"; Description: "{cm:LaunchProgram,Emendia}"; Flags: nowait postinstall skipifsilent

[CustomMessages]
english.CloseEmendia=Please choose Quit from Emendia's tray menu, then retry. The executable must be closed and writable before continuing.
french.CloseEmendia=Choisissez Quitter dans le menu de l'icône Emendia, puis réessayez. L'exécutable doit être fermé et accessible en écriture avant de continuer.

[Code]
function ExecutableIsWritable: Boolean;
var
  Filename: String;
  Stream: TFileStream;
begin
  Filename := ExpandConstant('{app}\emendia.exe');
  Result := True;
  if FileExists(Filename) then
  begin
    try
      Stream := TFileStream.Create(Filename, fmOpenReadWrite or fmShareExclusive);
      Stream.Free;
    except
      Result := False;
    end;
  end;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not ExecutableIsWritable then
    Result := CustomMessage('CloseEmendia');
end;

function InitializeUninstall: Boolean;
begin
  Result := ExecutableIsWritable;
  if not Result then
    SuppressibleMsgBox(CustomMessage('CloseEmendia'), mbError, MB_OK, IDOK);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  Command: String;
  InstalledExe: String;
begin
  if CurUninstallStep = usUninstall then
  begin
    InstalledExe := ExpandConstant('{app}\emendia.exe');
    if RegQueryStringValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run',
      'Emendia', Command) then
    begin
      { Only remove an entry owned by this installation, not a portable copy. }
      if (CompareText(Command, '"' + InstalledExe + '"') = 0) or
         (CompareText(Command, InstalledExe) = 0) then
        RegDeleteValue(HKCU, 'Software\Microsoft\Windows\CurrentVersion\Run', 'Emendia');
    end;
  end;
end;

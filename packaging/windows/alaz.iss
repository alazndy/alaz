; A.L.A.Z — Windows installer (Inno Setup 6).
; Built by .github/workflows/windows.yml:  iscc /DAppVersion=0.1.0 /DSourceExe=..\..\target\release\alaz.exe packaging\windows\alaz.iss

#ifndef AppVersion
  #define AppVersion "0.2.0"
#endif
#ifndef SourceExe
  #define SourceExe "..\..\target\release\alaz.exe"
#endif

[Setup]
AppId={{DC45AD4E-A5F8-4E6F-AF92-A3EFB834683B}
AppName=A.L.A.Z
AppVersion={#AppVersion}
AppVerName=A.L.A.Z {#AppVersion}
AppPublisher=alazndy
DefaultDirName={autopf}\ALAZ
DefaultGroupName=A.L.A.Z
DisableProgramGroupPage=yes
OutputDir=..\..\dist
OutputBaseFilename=ALAZ-{#AppVersion}-Setup
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog
UninstallDisplayName=A.L.A.Z

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#SourceExe}"; DestDir: "{app}"; DestName: "alaz.exe"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\A.L.A.Z"; Filename: "{app}\alaz.exe"
Name: "{group}\{cm:UninstallProgram,A.L.A.Z}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\A.L.A.Z"; Filename: "{app}\alaz.exe"; Tasks: desktopicon

[Run]
Filename: "{app}\alaz.exe"; Description: "{cm:LaunchProgram,A.L.A.Z}"; Flags: nowait postinstall skipifsilent

; saves and key settings live in %APPDATA%\ALAZ and are kept on uninstall

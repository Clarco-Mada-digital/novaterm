; Installateur Windows de NovaTerm (Inno Setup 6).
; Appelé par packaging\build-windows.ps1 avec /DAppVersion, /DSourceDir, /DOutputDir.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif

[Setup]
AppId={{6B0C4E2A-9F7D-4E59-9C61-3F2B8D0A7E11}
AppName=NovaTerm
AppVersion={#AppVersion}
AppPublisher=NovaTerm
DefaultDirName={autopf}\NovaTerm
DefaultGroupName=NovaTerm
UninstallDisplayIcon={app}\novaterm.exe
OutputDir={#OutputDir}
OutputBaseFilename=NovaTerm-{#AppVersion}-setup-x64
SetupIconFile={#SourceDir}\novaterm.ico
Compression=lzma2
SolidCompression=yes
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequiredOverridesAllowed=dialog
WizardStyle=modern

[Languages]
Name: "french"; MessagesFile: "compiler:Languages\French.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"
Name: "contextmenu"; Description: "Ajouter « Ouvrir NovaTerm ici » au clic droit des dossiers"; GroupDescription: "Intégration :"

[Files]
Source: "{#SourceDir}\novaterm.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\novaterm.ico"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{group}\NovaTerm"; Filename: "{app}\novaterm.exe"; IconFilename: "{app}\novaterm.ico"
Name: "{group}\Désinstaller NovaTerm"; Filename: "{uninstallexe}"
Name: "{autodesktop}\NovaTerm"; Filename: "{app}\novaterm.exe"; IconFilename: "{app}\novaterm.ico"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Classes\Directory\shell\NovaTerm"; ValueType: string; ValueData: "Ouvrir NovaTerm ici"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\NovaTerm"; ValueName: "Icon"; ValueType: string; ValueData: "{app}\novaterm.ico"; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\shell\NovaTerm\command"; ValueType: string; ValueData: """{app}\novaterm.exe"" --cwd ""%V"""; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\NovaTerm"; ValueType: string; ValueData: "Ouvrir NovaTerm ici"; Flags: uninsdeletekey; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\NovaTerm"; ValueName: "Icon"; ValueType: string; ValueData: "{app}\novaterm.ico"; Tasks: contextmenu
Root: HKCU; Subkey: "Software\Classes\Directory\Background\shell\NovaTerm\command"; ValueType: string; ValueData: """{app}\novaterm.exe"" --cwd ""%V"""; Tasks: contextmenu

[Run]
Filename: "{app}\novaterm.exe"; Description: "{cm:LaunchProgram,NovaTerm}"; Flags: nowait postinstall skipifsilent

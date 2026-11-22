; KeyFrameXL Windows user-scope installer (Inno Setup 6).
; Installs into %LOCALAPPDATA%\Programs\KeyFrameXL without administrator rights
; so the built-in self-updater can replace keyframexl.exe in place.

#ifndef MyAppVersion
  #define MyAppVersion "0.2.1"
#endif
#ifndef MyNumericVersion
  #define MyNumericVersion "0.2.1.0"
#endif
#ifndef MyExePath
  #define MyExePath "..\target\release\keyframexl.exe"
#endif
#ifndef MyOutputDir
  #define MyOutputDir "..\dist"
#endif

[Setup]
AppId={{9C3A8B2E-4F1D-4A7C-8E25-7B6D9A1E3F04}
AppName=KeyFrameXL
AppVersion={#MyAppVersion}
AppVerName=KeyFrameXL {#MyAppVersion}
VersionInfoVersion={#MyNumericVersion}
AppPublisher=KeyFrameXL
AppPublisherURL=https://github.com/mallardask-ops/keyframexl
AppSupportURL=https://github.com/mallardask-ops/keyframexl/issues
AppUpdatesURL=https://github.com/mallardask-ops/keyframexl/releases
PrivilegesRequired=lowest
DefaultDirName={localappdata}\Programs\KeyFrameXL
DefaultGroupName=KeyFrameXL
DisableProgramGroupPage=yes
LicenseFile=..\LICENSE
SetupIconFile=icon.ico
UninstallDisplayIcon={app}\keyframexl.exe
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
OutputDir={#MyOutputDir}
OutputBaseFilename=keyframexl-windows-x86_64-setup

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"
Name: "ukrainian"; MessagesFile: "compiler:Languages\Ukrainian.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "associate_kfxl"; Description: "{cm:AssocFileExtension,KeyFrameXL,.kfxl}"; GroupDescription: "{cm:AdditionalIcons}"

[Files]
Source: "{#MyExePath}"; DestDir: "{app}"; DestName: "keyframexl.exe"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\docs\roadmap.md"; DestDir: "{app}\docs"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\KeyFrameXL"; Filename: "{app}\keyframexl.exe"
Name: "{autodesktop}\KeyFrameXL"; Filename: "{app}\keyframexl.exe"; Tasks: desktopicon

[Registry]
Root: HKCU; Subkey: "Software\Classes\.kfxl"; ValueType: string; ValueName: ""; ValueData: "KeyFrameXL.Project"; Flags: uninsdeletevalue; Tasks: associate_kfxl
Root: HKCU; Subkey: "Software\Classes\KeyFrameXL.Project"; ValueType: string; ValueName: ""; ValueData: "KeyFrameXL Project"; Flags: uninsdeletekey; Tasks: associate_kfxl
Root: HKCU; Subkey: "Software\Classes\KeyFrameXL.Project\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: "{app}\keyframexl.exe,0"; Tasks: associate_kfxl
Root: HKCU; Subkey: "Software\Classes\KeyFrameXL.Project\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\keyframexl.exe"" ""%1"""; Tasks: associate_kfxl

[UninstallDelete]
Type: files; Name: "{app}\keyframexl.old.exe"

[Run]
Filename: "{app}\keyframexl.exe"; Description: "{cm:LaunchProgram,KeyFrameXL}"; Flags: nowait postinstall skipifsilent

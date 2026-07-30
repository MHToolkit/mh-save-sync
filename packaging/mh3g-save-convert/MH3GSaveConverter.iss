; MH3G Save Converter Windows x64 installer.
; The package script supplies SourceDir, OutputDir, and AppVersion. Keep all
; runtime files together: the WinUI shell requires tools\mh3g-save-convert.exe.
#ifndef SourceDir
  #error SourceDir is required
#endif
#ifndef OutputDir
  #error OutputDir is required
#endif
#ifndef AppVersion
  #error AppVersion is required
#endif

[Setup]
AppId={{D095BF74-9E13-48D1-891C-EB0EE19D3AB4}
AppName=MH3G Save Converter
AppVersion={#AppVersion}
AppPublisher=MH ToolKit
DefaultDirName={autopf}\MH ToolKit\MH3G Save Converter
DefaultGroupName=MH ToolKit
DisableProgramGroupPage=yes
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename=MH3GSaveConverter-Setup-x64
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
UninstallDisplayName=MH3G Save Converter

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\MH ToolKit\MH3G Save Converter"; Filename: "{app}\MH3GSaveConverter.exe"
Name: "{autodesktop}\MH3G Save Converter"; Filename: "{app}\MH3GSaveConverter.exe"; Tasks: desktopicon

[Tasks]
Name: "desktopicon"; Description: "Create a desktop shortcut"; GroupDescription: "Additional shortcuts:"

[Run]
Filename: "{app}\MH3GSaveConverter.exe"; Description: "Launch MH3G Save Converter"; Flags: nowait postinstall skipifsilent

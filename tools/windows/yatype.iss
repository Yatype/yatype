; Установщик Yatype для Windows (Inno Setup 6).
;   iscc /DAppVersion=0.1.0 tools\windows\yatype.iss
; Ставит программу только для текущего пользователя, права администратора не нужны.

#ifndef AppVersion
  #define AppVersion "0.0.0"
#endif
; Пометка в имени файла, например "-beta".
#ifndef Channel
  #define Channel ""
#endif

[Setup]
AppId={{DD860A56-991E-4D9D-847E-A60C8447B0D8}
AppName=Yatype
AppVersion={#AppVersion}
AppPublisher=Yatype
AppComments=Исправляет текст, набранный не в той раскладке
DefaultDirName={localappdata}\Programs\Yatype
PrivilegesRequired=lowest
DisableProgramGroupPage=yes
DisableDirPage=yes
OutputDir=..\..\dist
OutputBaseFilename=Yatype-Setup-{#AppVersion}{#Channel}
SetupIconFile=yatype.ico
UninstallDisplayIcon={app}\Yatype.exe
UninstallDisplayName=Yatype
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
; Обновление поверх работающей программы: закрыть её перед заменой файлов.
CloseApplications=force

[Languages]
Name: "ru"; MessagesFile: "compiler:Languages\Russian.isl"
Name: "en"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "autostart"; Description: "Запускать Yatype при входе в Windows"

[Files]
Source: "..\..\target\x86_64-pc-windows-msvc\release\switcher.exe"; DestDir: "{app}"; DestName: "Yatype.exe"; Flags: ignoreversion
Source: "..\..\data\LICENSES.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{userprograms}\Yatype"; Filename: "{app}\Yatype.exe"

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "Yatype"; ValueData: """{app}\Yatype.exe"""; Tasks: autostart; Flags: uninsdeletevalue

[Run]
Filename: "{app}\Yatype.exe"; Description: "Запустить Yatype"; Flags: nowait postinstall skipifsilent

[UninstallRun]
Filename: "{sys}\taskkill.exe"; Parameters: "/IM Yatype.exe /F"; Flags: runhidden; RunOnceId: "StopYatype"

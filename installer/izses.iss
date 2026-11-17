; IZSES – Windows x64 kurulum betigi (Inno Setup 6)
;
; Bu betik, `scripts\kurulum-paketi.ps1` tarafindan cagrilir.
; Uygulamayi yukler, gerekli klasorleri olusturur, Whisper modelini
; yerel olarak kurar ve Baslat menusune kisayol ekler. Kaldirma desteklidir.
;
; Kullanici kurulumdan sonra uygulamayi dogrudan acabilir. Terminal, Python,
; Node.js veya elle yapilandirma gerekmez.

#ifndef MySourceDir
  #define MySourceDir "..\dist\izses"
#endif
#ifndef MyOutputDir
  #define MyOutputDir "..\dist"
#endif

#define MyAppName "IZSES"
#define MyAppVersion "0.1.0"
#define MyAppPublisher "IZSES"
#define MyAppExeName "izses.exe"

[Setup]
AppId={{9C4E1B7A-56F2-4C1D-9E3A-IZSES0000001}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
DisableProgramGroupPage=yes
OutputDir={#MyOutputDir}
OutputBaseFilename=izses-kurulum-{#MyAppVersion}-x64
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
; Kurulum paketi imzalanabilir. Sertifika verilirse asagidaki satirlar acilir:
; SignTool=signtool
; SignedUninstaller=yes
PrivilegesRequiredOverridesAllowed=dialog

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[Tasks]
Name: "desktopicon"; Description: "Masaustu kisayolu olustur"; GroupDescription: "Kisayollar:"

[Files]
; Uygulama ikilisi ve kaynaklar.
Source: "{#MySourceDir}\izses.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#MySourceDir}\README.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#MySourceDir}\NOTICE.md"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
Source: "{#MySourceDir}\izses.exe.sha256"; DestDir: "{app}"; Flags: ignoreversion skipifsourcedoesntexist
; PDF ciktisi icin gomulecek yazi tipi.
Source: "{#MySourceDir}\fonts\*"; DestDir: "{app}\fonts"; Flags: ignoreversion recursesubdirs skipifsourcedoesntexist
; Whisper modeli (pakete dahil edildiyse).
Source: "{#MySourceDir}\modeller\*"; DestDir: "{app}\modeller"; Flags: ignoreversion recursesubdirs createallsubdirs skipifsourcedoesntexist

[Dirs]
; Kullanici verisi klasorleri (Belgeler altinda).
Name: "{userdocs}\IZSES"; Permissions: users-modify
Name: "{userdocs}\IZSES\Incelemeler"; Permissions: users-modify
Name: "{userdocs}\IZSES\modeller"; Permissions: users-modify

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"
Name: "{group}\{#MyAppName} Kaldir"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; Tasks: desktopicon

[Run]
; Kurulum sonrasi kullaniciya modeli yerel klasore kopyalama secenegi.
Filename: "{cmd}"; Parameters: "/c xcopy /E /I /Y ""{app}\modeller"" ""{userdocs}\IZSES\modeller"""; \
  StatusMsg: "Model dosyalari yerel klasore kopyalaniyor..."; Flags: runhidden; Check: ModelPackaged
Filename: "{app}\{#MyAppExeName}"; Description: "IZSES uygulamasini simdi ac"; \
  Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Uygulama klasoru kalinti birakmaz. Kullanici verisi (Belgeler\IZSES)
; KALDIRILMAZ; inceleme verileri kullaniciya aittir.
Type: filesandordirs; Name: "{app}"

[Code]
function ModelPackaged(): Boolean;
begin
  Result := DirExists(ExpandConstant('{app}\modeller\whisper-large-v3-turbo'));
end;

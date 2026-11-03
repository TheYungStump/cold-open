; The Windows installer (docs/specs/release.md §3.1): per user, no
; administrator rights, the CLI on the user's PATH, file associations under
; HKCU. Built by package.sh with Inno Setup 6:
;   ISCC /DAppVersion=<v> /DNumVersion=<a.b.c.0> /DArch=x64|arm64 /DStage=<folder>
;        /DOut=<folder> coldopen.iss

#ifndef AppVersion
  #error AppVersion is required
#endif
#ifndef Arch
  #error Arch is required (x64 or arm64)
#endif
#ifndef Stage
  #error Stage is required
#endif
#ifndef NumVersion
  ; Explorer's file version: four numbers, so 0.1.0-alpha.1 is 0.1.0.0.
  #define NumVersion "0.0.0.0"
#endif
#ifndef Out
  #define Out "."
#endif

[Setup]
; The installer's identity across versions: an upgrade finds the old one.
AppId={{5E0B7C2A-3F4D-4E8B-9A61-0C7D2B8E4F19}
AppName=Cold Open
AppVersion={#AppVersion}
AppVerName=Cold Open {#AppVersion}
AppPublisher=The Cold Open contributors
AppPublisherURL=https://github.com/TheYungStump/cold-open
AppSupportURL=https://github.com/TheYungStump/cold-open/issues
VersionInfoVersion={#NumVersion}
VersionInfoProductTextVersion={#AppVersion}
; Per user: {autopf} is %LOCALAPPDATA%\Programs, and no UAC prompt.
PrivilegesRequired=lowest
DefaultDirName={autopf}\Cold Open
DisableProgramGroupPage=yes
DisableDirPage=auto
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible and not arm64
ArchitecturesInstallIn64BitMode=x64compatible
#endif
MinVersion=10.0
OutputDir={#Out}
OutputBaseFilename=ColdOpen-{#AppVersion}-windows-{#Arch}-setup
SetupIconFile=..\icon\coldopen.ico
UninstallDisplayIcon={app}\Cold Open.exe
UninstallDisplayName=Cold Open
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
ChangesEnvironment=yes
ChangesAssociations=yes
; A running Cold Open is closed by CloseColdOpen below, not by Windows'
; Restart Manager: iced's window doesn't answer it, so it waited out
; Windows' time limit, 20 to 30 seconds (release.md §3.1, the project
; owner's retest, 2026-11-02).
CloseApplications=no
RestartApplications=no

[Tasks]
Name: "addtopath"; Description: "Add the coldopen command to PATH (for terminals and AI agents)"
Name: "desktopicon"; Description: "Create a desktop shortcut"; Flags: unchecked

[Files]
Source: "{#Stage}\Cold Open.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#Stage}\bin\coldopen.exe"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "{#Stage}\licenses\*"; DestDir: "{app}\licenses"; Flags: ignoreversion
Source: "{#Stage}\README.txt"; DestDir: "{app}"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\Cold Open"; Filename: "{app}\Cold Open.exe"
Name: "{autodesktop}\Cold Open"; Filename: "{app}\Cold Open.exe"; Tasks: desktopicon

[Registry]
; One ProgId per type (docs/specs/release.md §5). HKA is HKCU here.
Root: HKA; Subkey: "Software\Classes\ColdOpen.fountain"; ValueType: string; ValueData: "Fountain screenplay"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ColdOpen.fountain\DefaultIcon"; ValueType: string; ValueData: "{app}\Cold Open.exe,0"
Root: HKA; Subkey: "Software\Classes\ColdOpen.fountain\shell\open\command"; ValueType: string; ValueData: """{app}\Cold Open.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\ColdOpen.fdx"; ValueType: string; ValueData: "Screenplay (.fdx)"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ColdOpen.fdx\DefaultIcon"; ValueType: string; ValueData: "{app}\Cold Open.exe,0"
Root: HKA; Subkey: "Software\Classes\ColdOpen.fdx\shell\open\command"; ValueType: string; ValueData: """{app}\Cold Open.exe"" ""%1"""
Root: HKA; Subkey: "Software\Classes\ColdOpen.coldopen"; ValueType: string; ValueData: "Cold Open project"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\ColdOpen.coldopen\DefaultIcon"; ValueType: string; ValueData: "{app}\Cold Open.exe,0"
Root: HKA; Subkey: "Software\Classes\ColdOpen.coldopen\shell\open\command"; ValueType: string; ValueData: """{app}\Cold Open.exe"" ""%1"""
; "Open with" for all three; the default only for .coldopen, which no
; other program claims. Windows lets only the writer pick the others.
Root: HKA; Subkey: "Software\Classes\.fountain\OpenWithProgids"; ValueType: string; ValueName: "ColdOpen.fountain"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.fdx\OpenWithProgids"; ValueType: string; ValueName: "ColdOpen.fdx"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.coldopen\OpenWithProgids"; ValueType: string; ValueName: "ColdOpen.coldopen"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\.coldopen"; ValueType: string; ValueName: ""; ValueData: "ColdOpen.coldopen"; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\Cold Open.exe"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "Cold Open"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\Cold Open.exe\SupportedTypes"; ValueType: string; ValueName: ".fountain"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\Cold Open.exe\SupportedTypes"; ValueType: string; ValueName: ".fdx"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\Cold Open.exe\SupportedTypes"; ValueType: string; ValueName: ".coldopen"; ValueData: ""

[Run]
Filename: "{app}\Cold Open.exe"; Description: "Start Cold Open"; Flags: nowait postinstall skipifsilent

[Code]
const
  EnvKey = 'Environment';
  AppExe = 'Cold Open.exe';
  { What Running does to each copy it finds. }
  JustCount = 0;
  AskToClose = 1;
  EndIt = 2;
  { What CloseColdOpen comes to. }
  Closed = 0;
  Cancelled = 1;
  StillOpen = 2;

{ The copies of Cold Open running from Exe: counted, asked to close as the
  window's close button does, or ended. WMI names each process's file, so
  a copy run from anywhere else, such as a zip, is left alone. }
function Running(Exe: string; Action: Integer): Integer;
var
  Locator, Service, Items, Item: Variant;
  I, Pid, Code: Integer;
  Path: string;
begin
  Result := 0;
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\CIMV2');
    Items := Service.ExecQuery('SELECT ProcessId, ExecutablePath FROM Win32_Process WHERE Name = ''' + AppExe + '''');
    for I := 0 to Items.Count - 1 do
    begin
      Item := Items.ItemIndex(I);
      { Another user's process has no path to read. }
      try
        Path := Item.ExecutablePath;
      except
        Path := '';
      end;
      if CompareText(Path, Exe) = 0 then
      begin
        Result := Result + 1;
        Pid := Item.ProcessId;
        if Action = AskToClose then
          Exec(ExpandConstant('{sys}\taskkill.exe'), '/PID ' + IntToStr(Pid), '', SW_HIDE, ewWaitUntilTerminated, Code)
        else if Action = EndIt then
          Item.Terminate();
      end;
    end;
  except
    Log('Could not list the running programs: ' + GetExceptionMessage);
  end;
end;

{ Waits up to Ms milliseconds for every copy running from Exe to end. }
function GoneWithin(Exe: string; Ms: Integer): Boolean;
var
  Waited: Integer;
begin
  Waited := 0;
  while (Running(Exe, JustCount) > 0) and (Waited < Ms) do
  begin
    Sleep(250);
    Waited := Waited + 250;
  end;
  Result := Running(Exe, JustCount) = 0;
end;

{ Closes the Cold Open running from the install folder before Who (Setup
  or the uninstaller) replaces or removes its files (release.md §3.1):
  asks, asks the app to close, and after 3 seconds ends it. Nothing is
  lost, since every keystroke is in the crash journal. }
function CloseColdOpen(Who: string): Integer;
var
  Exe: string;
begin
  Exe := ExpandConstant('{app}\' + AppExe);
  Result := Closed;
  if Running(Exe, JustCount) = 0 then
    Exit;
  if SuppressibleMsgBox('Cold Open is open, so ' + Who + ' will close it first.' + #13#10 + #13#10 +
      'Nothing you typed is lost: Cold Open keeps it, and offers it back when it starts again.',
      mbConfirmation, MB_OKCANCEL, IDOK) <> IDOK then
  begin
    Result := Cancelled;
    Exit;
  end;
  Running(Exe, AskToClose);
  if GoneWithin(Exe, 3000) then
    Exit;
  Log('Cold Open did not close when asked, so it is being ended.');
  Running(Exe, EndIt);
  if not GoneWithin(Exe, 3000) then
    Result := StillOpen;
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  case CloseColdOpen('Setup') of
    Cancelled: Result := 'Cold Open is still open, so nothing was installed.';
    StillOpen: Result := 'Cold Open could not be closed. Close it, then run Setup again.';
  end;
end;

function InitializeUninstall(): Boolean;
begin
  Result := True;
  case CloseColdOpen('the uninstaller') of
    Cancelled: Result := False;
    StillOpen:
      begin
        SuppressibleMsgBox('Cold Open could not be closed. Close it, then uninstall it again.', mbError, MB_OK, IDOK);
        Result := False;
      end;
  end;
end;

{ Whether Dir is one of the ';'-separated entries in Path. }
function InPath(Path, Dir: string): Boolean;
begin
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Path) + ';') > 0;
end;

procedure AddToPath(Dir: string);
var
  Path: string;
begin
  if not RegQueryStringValue(HKCU, EnvKey, 'Path', Path) then
    Path := '';
  if InPath(Path, Dir) then
    Exit;
  if (Path <> '') and (Copy(Path, Length(Path), 1) <> ';') then
    Path := Path + ';';
  RegWriteExpandStringValue(HKCU, EnvKey, 'Path', Path + Dir);
end;

procedure RemoveFromPath(Dir: string);
var
  Path: string;
  P: Integer;
begin
  if not RegQueryStringValue(HKCU, EnvKey, 'Path', Path) then
    Exit;
  Path := ';' + Path + ';';
  P := Pos(';' + Uppercase(Dir) + ';', Uppercase(Path));
  if P = 0 then
    Exit;
  Delete(Path, P, Length(Dir) + 1);
  { Drop the separators added above. }
  Path := Copy(Path, 2, Length(Path) - 2);
  RegWriteExpandStringValue(HKCU, EnvKey, 'Path', Path);
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if (CurStep = ssPostInstall) and WizardIsTaskSelected('addtopath') then
    AddToPath(ExpandConstant('{app}\bin'));
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(ExpandConstant('{app}\bin'));
end;

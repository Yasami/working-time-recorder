; Working Time Recorder のインストーラー (Inno Setup 6)
;
; ビルド:
;   ISCC.exe /DAppVersion=0.3.0 installer\working-time-recorder.iss
;   (実行ファイルは /DBuildDir で指定したフォルダーから取る。既定は ..\target\release)
;
; ユーザーごとにインストールする (管理者権限は不要)。
; ビューワーは更新時にこのインストーラーを /SILENT /SP- /NORESTART で実行する。

#ifndef AppVersion
  #error AppVersion を /DAppVersion=x.y.z で指定してください
#endif
#ifndef BuildDir
  #define BuildDir "..\target\release"
#endif

#define AppName "Working Time Recorder"
#define ViewerExe "working-time-viewer.exe"
#define RecorderExe "working-time-recorder.exe"
#define RepositoryUrl "https://github.com/Yasami/working-time-recorder"

[Setup]
; AppId はアップデートとアンインストールで同じアプリだと判断するために使う。変えないこと
AppId={{D1291B2C-EAD2-4C76-AE92-B493C7FB6ED5}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher=Masahide Sakamaki
AppPublisherURL={#RepositoryUrl}
AppSupportURL={#RepositoryUrl}
AppUpdatesURL={#RepositoryUrl}/releases
VersionInfoVersion={#AppVersion}
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
DefaultDirName={autopf}\{#AppName}
DisableProgramGroupPage=yes
UninstallDisplayName={#AppName}
UninstallDisplayIcon={app}\{#ViewerExe}
; PATH (HKCU\Environment) の変更を、起動中のエクスプローラーなどに知らせる
ChangesEnvironment=yes
; ビューワーは [Code] で閉じ、[Run] で起動し直す
RestartApplications=no
WizardStyle=modern
Compression=lzma2
SolidCompression=yes
OutputDir=..\target\installer
; ビューワーは -setup.exe で終わる添付ファイルをインストーラーとみなす
OutputBaseFilename=working-time-recorder-v{#AppVersion}-windows-x86_64-setup

[Languages]
Name: "japanese"; MessagesFile: "compiler:Languages\Japanese.isl"

[Tasks]
Name: "addtopath"; Description: "working-time-recorder を PATH に追加する (コマンドプロンプトなどから実行できるようにする)"
Name: "autostart"; Description: "ログオン時にビューワーを起動する"

[Files]
Source: "{#BuildDir}\{#ViewerExe}"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#BuildDir}\{#RecorderExe}"; DestDir: "{app}\bin"; Flags: ignoreversion
Source: "..\README.md"; DestDir: "{app}"; Flags: ignoreversion

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "WorkingTimeViewer"; ValueData: """{app}\{#ViewerExe}"""; Flags: uninsdeletevalue; Tasks: autostart

[Run]
; 更新 (/SILENT) の後にもビューワーを起動し直すため、skipifsilent は付けない
Filename: "{app}\{#ViewerExe}"; Description: "ビューワーを起動する"; Flags: nowait postinstall

[Code]
const
  ViewerMutex = 'Local\WorkingTimeViewer.SingleInstance';
  ViewerWindowClass = 'WorkingTimeViewer.Host';
  EnvironmentKey = 'Environment';
  RunKey = 'Software\Microsoft\Windows\CurrentVersion\Run';
  RunValue = 'WorkingTimeViewer';
  WM_CLOSE = $0010;
  CloseTimeoutMs = 10000;
  CloseIntervalMs = 100;

var
  { インストールのためにビューワーを閉じた }
  ViewerClosed: Boolean;
  { ファイルのインストールまで終わった }
  InstallCompleted: Boolean;

function ViewerPath(): String;
begin
  Result := ExpandConstant('{app}\{#ViewerExe}');
end;

function BinDir(): String;
begin
  Result := ExpandConstant('{app}\bin');
end;

{ 起動中のビューワーに終了を頼み、終わるまで待つ。終了した (または起動していない) なら True }
function CloseViewer(): Boolean;
var
  Wnd: HWND;
  Waited: Integer;
begin
  Result := True;
  if not CheckForMutexes(ViewerMutex) then
    Exit;
  Wnd := FindWindowByClassName(ViewerWindowClass);
  if Wnd <> 0 then
    PostMessage(Wnd, WM_CLOSE, 0, 0);
  Waited := 0;
  while CheckForMutexes(ViewerMutex) do
  begin
    if Waited >= CloseTimeoutMs then
    begin
      Result := False;
      Exit;
    end;
    Sleep(CloseIntervalMs);
    Waited := Waited + CloseIntervalMs;
  end;
  { ミューテックスを閉じてからプロセスが終わるまでの間、実行ファイルはまだ使用中 }
  Sleep(500);
end;

{ セミコロン区切りの Paths に Dir が含まれるか (大文字・小文字は区別しない) }
function PathContains(Paths, Dir: String): Boolean;
begin
  Result := Pos(';' + Uppercase(Dir) + ';', ';' + Uppercase(Paths) + ';') > 0;
end;

procedure AddToPath(Dir: String);
var
  Paths: String;
begin
  if not RegQueryStringValue(HKCU, EnvironmentKey, 'Path', Paths) then
    Paths := '';
  if PathContains(Paths, Dir) then
    Exit;
  if (Paths <> '') and (Copy(Paths, Length(Paths), 1) <> ';') then
    Paths := Paths + ';';
  RegWriteExpandStringValue(HKCU, EnvironmentKey, 'Path', Paths + Dir);
end;

procedure RemoveFromPath(Dir: String);
var
  Paths, Rest, Item, NewPaths: String;
  P: Integer;
begin
  if not RegQueryStringValue(HKCU, EnvironmentKey, 'Path', Paths) then
    Exit;
  if not PathContains(Paths, Dir) then
    Exit;
  NewPaths := '';
  Rest := Paths + ';';
  while Rest <> '' do
  begin
    P := Pos(';', Rest);
    Item := Copy(Rest, 1, P - 1);
    Rest := Copy(Rest, P + 1, Length(Rest));
    if (Item <> '') and (CompareText(Item, Dir) <> 0) then
    begin
      if NewPaths <> '' then
        NewPaths := NewPaths + ';';
      NewPaths := NewPaths + Item;
    end;
  end;
  if NewPaths = '' then
    RegDeleteValue(HKCU, EnvironmentKey, 'Path')
  else
    RegWriteExpandStringValue(HKCU, EnvironmentKey, 'Path', NewPaths);
end;

function PrepareToInstall(var NeedsRestart: Boolean): String;
begin
  Result := '';
  if not CheckForMutexes(ViewerMutex) then
    Exit;
  if CloseViewer() then
    ViewerClosed := True
  else
    Result := '作業時間ビューワーを終了できませんでした。' +
      'タスクトレイのアイコンを右クリックして「終了」を選んでから、もう一度インストールしてください。';
end;

procedure CurStepChanged(CurStep: TSetupStep);
begin
  if CurStep = ssPostInstall then
  begin
    InstallCompleted := True;
    if WizardIsTaskSelected('addtopath') then
      AddToPath(BinDir())
    else
      RemoveFromPath(BinDir());
    { 前回のインストールで登録した自動起動を外す }
    if not WizardIsTaskSelected('autostart') then
      RegDeleteValue(HKCU, RunKey, RunValue);
  end;
end;

procedure DeinitializeSetup();
var
  ResultCode: Integer;
begin
  { インストールが中断・失敗したら、閉じたビューワーを起動し直す }
  if ViewerClosed and not InstallCompleted then
  begin
    if FileExists(ViewerPath()) then
      Exec(ViewerPath(), '', '', SW_SHOWNORMAL, ewNoWait, ResultCode);
  end;
end;

function InitializeUninstall(): Boolean;
begin
  Result := CloseViewer();
  if not Result then
    SuppressibleMsgBox('作業時間ビューワーを終了できませんでした。' +
      'タスクトレイのアイコンを右クリックして「終了」を選んでから、もう一度アンインストールしてください。',
      mbError, MB_OK, IDOK);
end;

procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
begin
  if CurUninstallStep = usPostUninstall then
    RemoveFromPath(BinDir());
end;

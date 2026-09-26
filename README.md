# working-time-recorder

作業時間を記録するツールと、その記録を可視化するビューワーです。

```
recorder/  # 作業時間を記録する CLI (working-time-recorder)
viewer/    # 記録を可視化するタスクトレイ常駐アプリ (working-time-viewer, Windows 専用)
```

## インストール

[Releases](https://github.com/Yasami/working-time-recorder/releases/latest) から
`working-time-recorder-vX.Y.Z-windows-x86_64-setup.exe` をダウンロードして実行します。
ユーザーごとのインストールなので、管理者権限は要りません
(インストール先の既定は `%LOCALAPPDATA%\Programs\Working Time Recorder`)。

インストーラーには署名が無いため、Windows SmartScreen が「Windows によって PC が保護されました」と警告することがあります。
その場合は「詳細情報」→「実行」を選んでください。

インストール時に次の項目を選べます (どちらも既定でオン)。

- **working-time-recorder を PATH に追加する**: ユーザー環境変数 `Path` にインストール先の `bin` フォルダーを追加し、
  コマンドプロンプトや PowerShell から `working-time-recorder` を実行できるようにします
  (追加した後に開いたウィンドウから使えます)
- **ログオン時にビューワーを起動する**: Windows にサインインしたときにビューワーを起動します

スタートメニューには登録しません。ビューワーを手動で起動するときは、インストール先の `working-time-viewer.exe` を実行してください。

### 更新

ビューワーは起動の 1 分後と、その後 24 時間ごとに新しいバージョンが公開されていないかを確認し
(失敗したときは 1 時間後に再試行)、見つかると通知を表示します。
通知をクリックするか、タスクトレイのメニューの「バージョン X.Y.Z に更新」を選ぶと、
インストーラーをダウンロードして更新し、ビューワーを起動し直します。
メニューの「更新を確認」で、すぐに確認することもできます。
自動の確認は設定ファイルの `check_update` で止められます。

新しいインストーラーを手動でダウンロードして実行しても更新できます。

### アンインストール

Windows の「設定」→「アプリ」→「インストールされているアプリ」から「Working Time Recorder」をアンインストールします。
PATH と自動起動の設定も元に戻します。記録ファイルと設定ファイルは削除しません。

## ビルド

```bash
cargo build --release
```

インストーラーは [Inno Setup 6](https://jrsoftware.org/isinfo.php) で作ります (`target\installer` に出力します)。

```powershell
& "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe" /DAppVersion=X.Y.Z installer\working-time-recorder.iss
```

リリースは `vX.Y.Z` 形式のタグを push すると GitHub Actions が作ります
(タグと `Cargo.toml` のバージョンが一致している必要があります)。

### 更新の確認を試す

デバッグビルドのビューワーは、環境変数 `WORKING_TIME_VIEWER_UPDATE_REPO` (`owner/repo`) を設定すると、
そのリポジトリのリリースを使って更新を確認します (リリースビルドはこの環境変数を読みません)。
PR の CI が作るインストーラー (Actions の成果物 `working-time-recorder-vX.Y.Z-windows-x86_64-setup.exe`) はデバッグビルドなので、次のように試せます。

1. テスト用のリポジトリにリリースを作り、インストーラー (名前が `-setup.exe` で終わるもの) を添付する
2. `setx WORKING_TIME_VIEWER_UPDATE_REPO <owner>/<repo>` を実行してから、ビューワーを起動し直す
3. タグのバージョンを変えながら、メニューの「更新を確認」や通知のクリックで動作を確かめる
4. 終わったら `reg delete HKCU\Environment /v WORKING_TIME_VIEWER_UPDATE_REPO /f` で環境変数を消す

## recorder

```bash
working-time-recorder start <task_name>... [-f <file>]
working-time-recorder stop [-f <file>]
```

タスク名に複数の単語を渡すと、スペースで連結して1つのタスク名として記録します
(`start 設計 レビュー` は「設計 レビュー」)。`-f <file>` はタスク名の前後や途中のどこに置いても構いません。
記録ファイルは1行1レコードのため、改行を含むタスク名はエラーになります。

記録ファイルは環境変数 `WORKING_TIME_RECORD`、未設定ならホームディレクトリの `working_time_record.txt` です。

## viewer

`working-time-viewer.exe` を起動するとタスクトレイに常駐します (recorder と同じ記録ファイルを読みます)。

- アイコンを左クリック: 今日の作業時間パネルを表示
  - 8時間を全体とした横バーに、タスクごとの作業時間を色分けして表示
    - 8時間に満たない分は網掛けの「未稼働」として表示
    - 8時間を超えた日は総作業時間を全体とし、8時間の位置に点線を表示
  - 「履歴を表示」で日ごとの作業時間の一覧を表示
- アイコンを右クリック: メニュー (今日の作業時間 / 履歴を表示 / 更新を確認 / 終了)

start から次の start / stop までをそのタスクの作業時間として集計し、作業中のタスクは現在時刻までを数えます。
日をまたぐ作業は次のように扱います。

- stop が記録されないまま、翌日以降に start が記録された: 前日の作業は 24:00 で終了したとみなす
- 翌日最初の記録が stop: 前日の作業はその時刻に終了したとみなし、0:00 で日付ごとに分割して数える

### パネルの自動表示

- 記録ファイルが変化すると (start / stop を記録すると)、パネルを自動で表示します
- 変化が無くても、前回の変化 (または前回の自動表示) から一定時間が経つとパネルを表示します
- 自動で表示したパネルは、一定時間が経つと自動で消えます
  - マウスを乗せている間は消えません。クリックすると、通常どおりフォーカスが外れるまで表示します

### 設定ファイル

記録ファイルの横に、記録ファイル名の拡張子 `.txt` を `.config` に替えたファイル
(例: `working_time_record.txt` なら `working_time_record.config`) を置くと、動作を設定できます。
中身は JSON です。ファイルや各項目は省略でき、省略した項目は既定値を使います。
ファイルの変更はビューワーの起動中にも反映されます。

```json
{
  "popup_interval_minutes": 30,
  "auto_hide_seconds": 5,
  "check_update": true,
  "labels": {
    "design": { "label": "設計作業", "color": "#FF8800" },
    "review": { "color": "navy" },
    "meeting": { "label": "打ち合わせ" }
  }
}
```

| 項目 | 内容 | 既定値 |
| --- | --- | --- |
| `popup_interval_minutes` | 前回の変化から、パネルを自動で表示するまでの時間 (分)。0 なら表示しない | 30 |
| `auto_hide_seconds` | 自動で表示したパネルを消すまでの時間 (秒)。0 なら消さない | 5 |
| `check_update` | 新しいバージョンが公開されていないかを自動で確認する (`false` にしても、メニューからは確認できます) | `true` |
| `labels` | タスク名をキーに、表示名 (`label`) と色 (`color`) を指定 | なし |

- 色は `#RRGGBB` / `#RGB` または HTML の基本色名 (`red`, `navy` など16色)
- 表示名や色を省略したタスク、ラベルが定義されていないタスクは、タスク名と既定の色で表示します
- 設定ファイルの形式が正しくない場合は、パネルにエラーを表示して既定値で動作します

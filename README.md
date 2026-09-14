# working-time-recorder

作業時間を記録するツールと、その記録を可視化するビューワーです。

```
recorder/  # 作業時間を記録する CLI (working-time-recorder)
viewer/    # 記録を可視化するタスクトレイ常駐アプリ (working-time-viewer, Windows 専用)
```

## ビルド

```bash
cargo build --release
```

## recorder

```bash
working-time-recorder start <task_name> [-f <file>]
working-time-recorder stop [-f <file>]
```

記録ファイルは環境変数 `WORKING_TIME_RECORD`、未設定ならホームディレクトリの `working_time_record.txt` です。

## viewer

`working-time-viewer.exe` を起動するとタスクトレイに常駐します (recorder と同じ記録ファイルを読みます)。

- アイコンを左クリック: 今日の作業時間パネルを表示
  - 8時間を全体とした横バーに、タスクごとの作業時間を色分けして表示
    - 8時間に満たない分は網掛けの「未稼働」として表示
    - 8時間を超えた日は総作業時間を全体とし、8時間の位置に点線を表示
  - 「履歴を表示」で日ごとの作業時間の一覧を表示
- アイコンを右クリック: メニュー (今日の作業時間 / 履歴を表示 / 終了)

start から次の start / stop までをそのタスクの作業時間として集計し、作業中のタスクは現在時刻までを数えます。
日をまたぐ作業は次のように扱います。

- stop が記録されないまま、翌日以降に start が記録された: 前日の作業は 24:00 で終了したとみなす
- 翌日最初の記録が stop: 前日の作業はその時刻に終了したとみなし、0:00 で日付ごとに分割して数える

### ラベル

記録ファイルの横に `<記録ファイル名>.labels` (例: `working_time_record.txt.labels`) を置くと、
タスク名の代わりに表示名を使い、バーを指定した色で表示します。ファイルは無くても構いません。

```
タスク名<TAB>表示名<TAB>カラーコード
```

- カラーコードは `#RRGGBB` / `#RGB` または HTML の基本色名 (`red`, `navy` など16色)
- 表示名やカラーコードを省略したタスク、ラベルが定義されていないタスクは、タスク名と既定の色で表示します

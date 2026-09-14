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
  - 「履歴を表示」で日ごとの作業時間の一覧を表示
- アイコンを右クリック: メニュー (今日の作業時間 / 履歴を表示 / 終了)

start から次の start / stop までをそのタスクの作業時間として集計し、作業中のタスクは現在時刻までを数えます。

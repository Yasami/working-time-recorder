# AGENTS.md

作業時間を記録する CLI (`recorder/`) と、記録を可視化する Windows 専用のタスクトレイアプリ (`viewer/`) の Cargo ワークスペースです。

## push 前に必ず行うこと

push する前に、必ず以下を実行してください。

1. `cargo fmt --all` で整形する (整形による差分が出たらコミットに含める)
2. `cargo clippy --all-targets` で警告が出ないことを確認する
3. `cargo test` が通ることを確認する

CI でも `cargo fmt --all -- --check` を実行しており、整形されていないコードは CI が失敗します。

## コーディング規約

- エディションは 2024。Rust の最新 stable で使える書き方に合わせる
- エディション・`rust-version`・依存パッケージのバージョンはルートの `Cargo.toml` (`[workspace.package]` / `[workspace.dependencies]`) で管理する
- コメント・ユーザー向けメッセージは日本語

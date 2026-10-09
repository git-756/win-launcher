# win-launcher

Windows のタスクトレイに常駐する、軽量なランチャーです（Rust 製、単体の exe）。
アイコンをクリックするとメニューが開き、登録した Web サイト・フォルダ・ファイルを開けます。
メニューの中身（名称・リンク先）は、**ビルドしたあとでも、テキストファイルで自由に変えられます**。
シャットダウン時刻の予約機能も付いています。

## 使い方

1. `win-launcher.exe` を起動する（タスクトレイに白黒の縞アイコンが出る）
2. アイコンを**クリック**（左でも右でも同じ）するとメニューが開く
3. 項目を選ぶと、ブラウザやエクスプローラーが開く

初回起動時に設定ファイルが自動で作られます。メニュー最下部の「設定ファイルを開く」でメモ帳が開きます。
Windows 起動時に自動で立ち上げたいときは、exe のショートカットを `shell:startup`（エクスプローラーのアドレス欄に入力）のフォルダに入れてください。

## 設定ファイル

場所: `%APPDATA%\win-launcher\menu.txt`（ユーザーごと。同じ exe を複数人に配っても、設定は各自のものになります）

初回起動時に作られる内容（抜粋）:

```text
# 名称 | 開く先
Samurai Human Go | https://samurai-human-go.com/
Samurai Human Go (Brain) | https://brain.samurai-human-go.com/
Google | https://www.google.com/
-
この設定ファイルのフォルダ | %APPDATA%\win-launcher
デスクトップ | shell:Desktop
-
シャットダウン予約 | @shutdown
```

| 書き方 | 意味 |
|---|---|
| `名称 \| 開く先` | メニュー項目。開く先は URL・ファイル・フォルダ |
| `-` | 区切り線 |
| `名称 \| @shutdown` | シャットダウン予約 |
| `名称 \| @shutdown URL` | 先に URL をブラウザで開いてから、シャットダウン予約 |
| `# ...` | コメント（空行も無視） |
| `%USERPROFILE%` など | 環境変数。実在する変数だけが置き換わります（URL の `%20` などはそのまま） |

- 開く先は、エクスプローラーでダブルクリックするのと同じ方法で開きます。フォルダのパス（`C:\Users\...`、`\\server\share\...`）、ファイル、`shell:Desktop` のような特殊フォルダも指定できます。
- **UTF-8** で保存してください（メモ帳の標準）。違う文字コードだと、メニューにエラーが出ます。
- 保存すると、**数秒（最大 2 秒＋α）でメニューに反映**されます。再起動は不要です。
- 書き間違いの行は、メニューの下に「⚠ 設定エラー（○行目）」として表示されます。
- 設定ファイルを削除すると、次の確認のときに既定の内容で作り直されます。

## シャットダウン予約

1. （`@shutdown` に URL を書いていれば）そのページをブラウザで開く
2. 時刻を入力する画面（黒い画面）が開く
3. 時刻を入力する（`17:20` / `1720` / `9:05`、全角も可）。すでに過ぎた時刻は翌日になります
4. `shutdown.exe /s /f /t 秒` で予約される

- 入力を間違えたら聞き直します（何も入力せず Enter でやめる）。
- すでに予約がある場合は、取り消して予約し直すか確認します。
- 予約の取り消しは、コマンドプロンプトで `shutdown /a` です。

## ビルド

Windows 上で [Rust](https://rustup.rs/) を入れて、次を実行します。

```
cargo build --release
```

できあがり: `target\release\win-launcher.exe`（単体で動きます。配るのはこの 1 ファイルだけです）

リンカーとして、次のどちらかが必要です。

- MSVC 版（`x86_64-pc-windows-msvc`）: Visual Studio Build Tools。**利用条件（ライセンス）は各自で確認してください。**
- GNU 版（`x86_64-pc-windows-gnu`）: MinGW-w64（`gcc` と `dlltool` に PATH が通っていること）。

## テスト

```
cargo test
```

設定ファイルの解釈と、時刻の計算（翌日・月末・年末・うるう年）を確認します（Windows 以外でも動きます）。

## ファイル構成

- `src/main.rs` … 起動（`--shutdown [URL]` 付きなら予約画面、なければトレイ）
- `src/tray.rs` … トレイとメニュー（[tray-icon](https://crates.io/crates/tray-icon)）。設定ファイルの更新を 2 秒ごとに確認
- `src/config.rs` … 設定ファイルの読み込み
- `src/default_menu.txt` … 設定ファイルが無いときに作る内容（exe に埋め込み）
- `src/shutdown_ui.rs` … シャットダウン予約
- `src/schedule.rs` … 時刻の解釈と、秒数の計算
- `src/win.rs` … Win32 API の薄いラッパー（[windows-sys](https://crates.io/crates/windows-sys)）

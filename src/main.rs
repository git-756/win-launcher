// Windowsではコンソールを出さない（タスクトレイ常駐のため）。
// `--shutdown` で起動したときだけ、自分でコンソールを確保する（shutdown_ui.rs）。
#![cfg_attr(windows, windows_subsystem = "windows")]
#![cfg_attr(not(windows), allow(dead_code))]

mod config;
mod schedule;

#[cfg(windows)]
mod shutdown_ui;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod win;

#[cfg(windows)]
fn main() {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        // `--shutdown [URL]`: シャットダウン予約ウィンドウ（URLがあれば先に開く）
        Some("--shutdown") => shutdown_ui::run(args.next()),
        _ => tray::run(),
    }
}

#[cfg(not(windows))]
fn main() {
    eprintln!("このランチャーはWindows専用です。");
}

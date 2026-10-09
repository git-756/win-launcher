//! シャットダウン予約ウィンドウ（Windows専用）。
//!
//! 流れ:
//!  1. （設定ファイルで URL が指定されていれば）そのページをブラウザで開く
//!  2. 0.5秒待って、このコンソールを最前面に戻す
//!  3. 時刻（例: 17:20 / 1720）を入力
//!  4. `shutdown.exe /s /f /t 秒` で予約

use std::io::{self, Write};
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

use windows_sys::Win32::System::Console::{AllocConsole, GetConsoleWindow, SetConsoleTitleW};
use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

use crate::schedule::{self, Plan};
use crate::win;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// shutdown.exe が「すでに予約されている」ときに返す終了コード。
const ERROR_SHUTDOWN_IS_SCHEDULED: i32 = 1190;

/// 1行読む。入力が閉じていれば None。
fn read_line() -> Option<String> {
    let mut s = String::new();
    match io::stdin().read_line(&mut s) {
        Ok(0) | Err(_) => None,
        Ok(_) => Some(s.trim().to_string()),
    }
}

fn prompt(text: &str) -> Option<String> {
    print!("{text}");
    let _ = io::stdout().flush();
    read_line()
}

/// `shutdown.exe` を実行して終了コードを返す（起動できなければ None）。
fn shutdown_exe(args: &[&str]) -> Option<i32> {
    Command::new("shutdown.exe")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .ok()
        .and_then(|s| s.code())
}

fn reserve(secs: u64) -> Option<i32> {
    shutdown_exe(&["/s", "/f", "/t", &secs.to_string()])
}

/// 予約を実行して結果を表示する。
fn run_plan(plan: &Plan) {
    let mut code = reserve(plan.secs);

    // すでに予約があるなら、確認してから取り消して予約し直す。
    if code == Some(ERROR_SHUTDOWN_IS_SCHEDULED) {
        println!("\nすでにシャットダウンが予約されています。");
        match prompt("取り消して、この時刻で予約し直しますか？ (y/N): ") {
            Some(a) if a.eq_ignore_ascii_case("y") => {
                shutdown_exe(&["/a"]);
                code = reserve(plan.secs);
            }
            _ => {
                println!("予約は変更しませんでした。");
                return;
            }
        }
    }

    match code {
        Some(0) => {
            println!(
                "\n予約完了: {:04}/{:02}/{:02} {:02}:{:02}:00 に実行します。",
                plan.year, plan.month, plan.day, plan.hour, plan.minute
            );
        }
        Some(c) => println!("\n予約に失敗しました（shutdown.exe の終了コード: {c}）。"),
        None => println!("\nshutdown.exe を起動できませんでした。"),
    }
}

/// `url` は、設定ファイルの `@shutdown URL` に書かれたもの（予約の前に開きたいページなど）。
/// 指定があれば最初にブラウザで開く。
pub fn run(url: Option<String>) {
    // GUIアプリ（コンソールなし）なので、自前でコンソールを作る。
    unsafe {
        AllocConsole();
        SetConsoleTitleW(win::wide("シャットダウン予約").as_ptr());
    }

    if let Some(url) = url.as_deref().filter(|u| !u.is_empty()) {
        // 1. 指定のページを開く
        println!("指定のページをブラウザで開きます...");
        win::open("シャットダウン予約の開き先", url);

        // 2. ブラウザが前面に出るのを少し待ち、このコンソールを最前面に戻す
        sleep(Duration::from_millis(500));
        unsafe {
            let hwnd = GetConsoleWindow();
            if !hwnd.is_null() {
                SetForegroundWindow(hwnd);
            }
        }
    }

    // 3. 時刻の入力（正しい値になるまで聞き直す。空Enterでやめる）
    println!("\n--------------------------------------------------");
    println!("シャットダウン時刻を指定してください（例：17:20 または 1720）。");
    println!("予約せずに閉じるときは、何も入力せずに Enter。");
    loop {
        let Some(input) = prompt("時刻入力: ") else { break };
        if input.is_empty() {
            break;
        }
        match schedule::parse_time(&input) {
            Some((h, m)) => {
                // 4. 予約
                let plan = schedule::plan(&win::local_clock(), h, m);
                run_plan(&plan);
                break;
            }
            None => println!("入力エラーです。もう一度入力してください。"),
        }
    }

    println!("\nEnterキーでこの画面を閉じます。");
    let _ = read_line();
}

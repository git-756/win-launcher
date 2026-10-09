//! Win32 API の薄いラッパー（Windows専用）。

use std::ptr::{null, null_mut};

use windows_sys::Win32::System::SystemInformation::{GetLocalTime, GetTickCount64};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK, SW_SHOWNORMAL};

use crate::schedule::Clock;

pub const APP_NAME: &str = "My Multi Launcher";

/// UTF-8 → NUL終端のUTF-16。
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// URL・ファイル・フォルダを既定のアプリで開く。失敗したらメッセージを出す。
/// （エクスプローラーでダブルクリックするのと同じ ShellExecute "open"）
pub fn open(label: &str, target: &str) {
    let verb = wide("open");
    let file = wide(target);
    // 戻り値が 32 より大きければ成功（Win32の取り決め）。
    let result = unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), file.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    if (result as isize) <= 32 {
        message_box(&format!("「{label}」を開けませんでした。\n\n{target}"));
    }
}

/// エラー用のメッセージボックス。
pub fn message_box(text: &str) {
    let text = wide(text);
    let caption = wide(APP_NAME);
    unsafe {
        MessageBoxW(null_mut(), text.as_ptr(), caption.as_ptr(), MB_OK | MB_ICONERROR);
    }
}

/// ローカル時刻（ミリ秒まで）を取得する。
pub fn local_clock() -> Clock {
    let mut st: windows_sys::Win32::Foundation::SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    Clock {
        year: u32::from(st.wYear),
        month: u32::from(st.wMonth),
        day: u32::from(st.wDay),
        hour: u32::from(st.wHour),
        minute: u32::from(st.wMinute),
        second: u32::from(st.wSecond),
        millis: u32::from(st.wMilliseconds),
    }
}

/// システム起動からの経過ミリ秒。
pub fn tick_ms() -> u64 {
    unsafe { GetTickCount64() }
}

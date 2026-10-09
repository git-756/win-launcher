//! タスクトレイ常駐とメニュー（Windows専用）。
//!
//! メニューは設定ファイル（menu.txt）から作る。左クリックでも右クリックでも同じメニューが開く。
//! ファイルが変わったら（2秒ごとに確認）、メニューを作り直す。

use std::cell::RefCell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder, TrayIconEvent};
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostQuitMessage, SetTimer, TranslateMessage, MSG,
};

use crate::config::{self, Item, Stamp};
use crate::win;

/// 設定ファイルの更新を確認する間隔。
const POLL_MS: u32 = 2000;
/// トレイを操作した直後はメニューが開いている可能性があるので、この間は作り直しを見送る。
const QUIET_MS: u64 = 10_000;

/// 最後にトレイ（アイコン）が操作された時刻（GetTickCount64 のミリ秒）。
static LAST_ACTIVITY: AtomicU64 = AtomicU64::new(0);

/// メニュー項目を選んだときの動作。
#[derive(Clone)]
enum Action {
    Open { label: String, target: String },
    Shutdown { url: Option<String> },
    OpenConfig(PathBuf),
    Quit,
}

struct State {
    // 捨てるとアイコンが消えるので、アプリが終わるまで持っておく。
    tray: TrayIcon,
    path: PathBuf,
    stamp: Option<Stamp>,
    /// メニュー項目のID（"0", "1", ...）→ 動作
    actions: Vec<Action>,
    /// メニューが生きている間、項目も保持しておく
    _keep: Vec<MenuItem>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// アイコン画像（64x64、白黒の縦縞）。
fn make_icon() -> Icon {
    const SIZE: u32 = 64;
    const STRIPES: u32 = 8;
    const STRIPE_W: u32 = SIZE / STRIPES;

    let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for _y in 0..SIZE {
        for x in 0..SIZE {
            let v: u8 = if (x / STRIPE_W) % 2 == 1 { 0 } else { 255 };
            rgba.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Icon::from_rgba(rgba, SIZE, SIZE).expect("アイコンの生成に失敗")
}

// ---------------------------------------------------------------------------
// メニューの組み立て
// ---------------------------------------------------------------------------

struct Builder {
    menu: Menu,
    actions: Vec<Action>,
    keep: Vec<MenuItem>,
}

impl Builder {
    fn new() -> Self {
        Self { menu: Menu::new(), actions: Vec::new(), keep: Vec::new() }
    }

    /// 選べる項目。IDは actions の添字。
    fn action(&mut self, text: &str, action: Action) {
        let item = MenuItem::with_id(self.actions.len().to_string(), text, true, None);
        let _ = self.menu.append(&item);
        self.actions.push(action);
        self.keep.push(item);
    }

    /// 選べない表示だけの項目（エラー表示など）。
    fn note(&mut self, text: &str) {
        let item = MenuItem::with_id("note", text, false, None);
        let _ = self.menu.append(&item);
        self.keep.push(item);
    }

    fn separator(&mut self) {
        let _ = self.menu.append(&PredefinedMenuItem::separator());
    }
}

fn build_menu(items: &[Item], path: &PathBuf) -> Builder {
    let mut b = Builder::new();

    let mut shown = 0;
    for item in items {
        match item {
            Item::Separator => b.separator(),
            Item::Open { label, target } => {
                b.action(label, Action::Open { label: label.clone(), target: target.clone() });
                shown += 1;
            }
            Item::Shutdown { label, url } => {
                b.action(label, Action::Shutdown { url: url.clone() });
                shown += 1;
            }
            Item::Error { .. } => {}
        }
    }
    if shown == 0 {
        b.note("（項目がありません。設定ファイルに書き足してください）");
    }

    // 書き間違いは、メニューの下にまとめて出す。
    let errors: Vec<&Item> = items.iter().filter(|i| matches!(i, Item::Error { .. })).collect();
    if !errors.is_empty() {
        b.separator();
        for e in errors {
            if let Item::Error { line, message } = e {
                if *line == 0 {
                    b.note(&format!("⚠ 設定エラー: {message}"));
                } else {
                    b.note(&format!("⚠ 設定エラー（{line}行目）: {message}"));
                }
            }
        }
    }

    b.separator();
    b.action("設定ファイルを開く", Action::OpenConfig(path.clone()));
    b.action("終了", Action::Quit);
    b
}

// ---------------------------------------------------------------------------
// イベント
// ---------------------------------------------------------------------------

/// シャットダウン予約ウィンドウ（自分自身を `--shutdown [URL]` で起動）を開く。
fn spawn_shutdown_window(url: Option<String>) {
    let result = std::env::current_exe().and_then(|exe| {
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--shutdown");
        if let Some(url) = url {
            cmd.arg(url);
        }
        cmd.spawn()
    });
    if let Err(e) = result {
        win::message_box(&format!("シャットダウン予約を起動できませんでした。\n\n{e}"));
    }
}

fn on_menu(event: MenuEvent) {
    let id = event.id.0.as_str();

    // 動作だけ取り出して、借用を手放してから実行する（実行中にタイマーが動いても安全なように）
    let action = STATE.with(|s| {
        let s = s.borrow();
        let state = s.as_ref()?;
        let index = id.parse::<usize>().ok()?;
        state.actions.get(index).cloned()
    });
    let Some(action) = action else { return };

    match action {
        Action::Open { label, target } => win::open(&label, &target),
        Action::Shutdown { url } => spawn_shutdown_window(url),
        Action::OpenConfig(path) => win::open("設定ファイル", &path.to_string_lossy()),
        Action::Quit => unsafe { PostQuitMessage(0) },
    }
}

/// 設定ファイルが変わっていたら、メニューを作り直す。
fn reload_if_changed() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let Some(state) = s.as_mut() else { return };

        let current = config::stamp(&state.path);
        if current == state.stamp {
            return;
        }
        // 目印は読む前に取る（読んでいる間に更新されても、次回また読み直せる）
        state.stamp = current;

        let items = config::load(&state.path);
        let b = build_menu(&items, &state.path);
        state.tray.set_menu(Some(Box::new(b.menu)));
        state.actions = b.actions;
        state._keep = b.keep;
    });
}

unsafe extern "system" fn on_timer(_hwnd: HWND, _msg: u32, _id: usize, _time: u32) {
    let idle = win::tick_ms().saturating_sub(LAST_ACTIVITY.load(Ordering::Relaxed));
    if idle >= QUIET_MS {
        reload_if_changed();
    }
}

// ---------------------------------------------------------------------------
// 起動
// ---------------------------------------------------------------------------

pub fn run() {
    let Some(path) = config::config_path() else {
        win::message_box("環境変数 APPDATA が見つからないため、設定ファイルの場所を決められません。");
        return;
    };

    let stamp = config::stamp(&path);
    let items = config::load(&path); // 無ければ既定の内容で作られる
    let b = build_menu(&items, &path);

    // メニューの選択・トレイの操作は、メッセージループ中にこのスレッドで通知される。
    MenuEvent::set_event_handler(Some(on_menu));
    TrayIconEvent::set_event_handler(Some(|_event: TrayIconEvent| {
        LAST_ACTIVITY.store(win::tick_ms(), Ordering::Relaxed);
    }));

    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(b.menu))
        .with_menu_on_left_click(true) // 左クリックでもメニューを開く（右クリックも同じメニュー）
        .with_tooltip(win::APP_NAME)
        .with_icon(make_icon())
        .build()
        .expect("トレイアイコンの作成に失敗");

    STATE.with(|s| {
        *s.borrow_mut() = Some(State { tray, path, stamp, actions: b.actions, _keep: b.keep });
    });

    // --- 設定ファイルの更新確認タイマー + Windowsのメッセージループ ---
    unsafe {
        SetTimer(std::ptr::null_mut(), 0, POLL_MS, Some(on_timer));

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

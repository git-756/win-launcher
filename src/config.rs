//! 設定ファイル（menu.txt）の読み込み。OSに依存しないので、どのOSでも `cargo test` できる。
//!
//! 書式（1行1項目）:
//! ```text
//! 名称 | 開く先          … URL・ファイル・フォルダ
//! -                       … 区切り線
//! 名称 | @shutdown [URL]  … シャットダウン予約（URLがあれば先に開く）
//! # コメント
//! ```

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 設定ファイルが無いときに作る内容（exeに埋め込み）。
pub const DEFAULT_TEXT: &str = include_str!("default_menu.txt");

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Separator,
    Open { label: String, target: String },
    Shutdown { label: String, url: Option<String> },
    /// 書き間違い。`line` は1始まり（0ならファイル全体の問題）。
    Error { line: usize, message: String },
}

// ---------------------------------------------------------------------------
// パース
// ---------------------------------------------------------------------------

fn is_var_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// `%NAME%` を環境変数で置き換える。存在しない変数はそのまま残す。
/// （URLの `%20` や `%E5%85...` を壊さないため、「実在する変数名」のときだけ置換する）
pub fn expand_env(s: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            if is_var_name(name) {
                if let Some(value) = lookup(name) {
                    out.push_str(&value);
                    rest = &after[end + 1..];
                    continue;
                }
            }
        }
        out.push('%');
        rest = after;
    }
    out.push_str(rest);
    out
}

/// 設定ファイルの文字列を項目の一覧にする。
pub fn parse(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Vec<Item> {
    let text = text.trim_start_matches('\u{feff}'); // メモ帳が付けることがあるBOM
    let mut items = Vec::new();

    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.chars().all(|c| c == '-') {
            items.push(Item::Separator);
            continue;
        }

        let Some((label, target)) = line.split_once('|') else {
            items.push(Item::Error { line: n, message: "「名称 | 開く先」の形式で書いてください".into() });
            continue;
        };
        let label = label.trim();
        let target = expand_env(target.trim(), lookup);
        if label.is_empty() {
            items.push(Item::Error { line: n, message: "名称が空です".into() });
            continue;
        }
        if target.is_empty() {
            items.push(Item::Error { line: n, message: "開く先が空です".into() });
            continue;
        }

        if let Some(rest) = target.strip_prefix('@') {
            let (cmd, arg) = match rest.split_once(char::is_whitespace) {
                Some((c, a)) => (c, a.trim()),
                None => (rest, ""),
            };
            if cmd == "shutdown" {
                items.push(Item::Shutdown {
                    label: label.to_string(),
                    url: (!arg.is_empty()).then(|| arg.to_string()),
                });
            } else {
                items.push(Item::Error { line: n, message: format!("不明なコマンドです: @{cmd}") });
            }
            continue;
        }

        items.push(Item::Open { label: label.to_string(), target });
    }
    items
}

// ---------------------------------------------------------------------------
// ファイルの読み書き
// ---------------------------------------------------------------------------

/// 更新の検出に使う目印（サイズと更新日時）。
pub type Stamp = (u64, Option<SystemTime>);

/// `%APPDATA%\win-launcher\menu.txt`（ユーザーごと。exeを同じ場所に置いたまま全員に配れる）
pub fn config_path() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("win-launcher").join("menu.txt"))
}

pub fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.len(), meta.modified().ok()))
}

/// 無ければ、既定の内容で作る。
pub fn ensure_exists(path: &Path) -> std::io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, DEFAULT_TEXT)
}

fn env_lookup(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// 設定ファイルを読んで項目にする。読めない・UTF-8でないときは、エラー項目を1つ返す。
pub fn load(path: &Path) -> Vec<Item> {
    let _ = ensure_exists(path);
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return vec![Item::Error { line: 0, message: format!("設定ファイルを読めません: {e}") }],
    };
    match String::from_utf8(bytes) {
        Ok(text) => parse(&text, &env_lookup),
        Err(_) => vec![Item::Error { line: 0, message: "UTF-8 で保存し直してください（文字コードが違います）".into() }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(name: &str) -> Option<String> {
        match name {
            "USERNAME" => Some("taro".to_string()),
            "USERPROFILE" => Some(r"C:\Users\taro".to_string()),
            _ => None,
        }
    }

    fn open(label: &str, target: &str) -> Item {
        Item::Open { label: label.into(), target: target.into() }
    }

    #[test]
    fn basic_lines() {
        let text = "# コメント\n\nA | https://example.com/a\n-\nB | C:\\tools\\b.exe\n";
        assert_eq!(
            parse(text, &env),
            vec![open("A", "https://example.com/a"), Item::Separator, open("B", r"C:\tools\b.exe")]
        );
    }

    #[test]
    fn spaces_and_pipes_in_target() {
        // 名称は最初の「|」まで。開く先に「|」や空白があってもよい。
        let items = parse("サイト (A 1) | https://x/?q=a b&a=b|c", &env);
        assert_eq!(items, vec![open("サイト (A 1)", "https://x/?q=a b&a=b|c")]);
    }

    #[test]
    fn separator_variants() {
        assert_eq!(parse("-\n---\n  -  \n", &env), vec![Item::Separator; 3]);
    }

    #[test]
    fn bom_and_crlf() {
        let items = parse("\u{feff}A | x\r\nB | y\r\n", &env);
        assert_eq!(items, vec![open("A", "x"), open("B", "y")]);
    }

    #[test]
    fn shutdown_command() {
        let items = parse("S1 | @shutdown\nS2 | @shutdown https://x/y?z=1&w=2\n", &env);
        assert_eq!(
            items,
            vec![
                Item::Shutdown { label: "S1".into(), url: None },
                Item::Shutdown { label: "S2".into(), url: Some("https://x/y?z=1&w=2".into()) },
            ]
        );
    }

    #[test]
    fn errors_have_line_numbers() {
        let items = parse("ok | x\n名称だけ\n | x\nlabel |\nz | @foo\n", &env);
        let lines: Vec<usize> = items
            .iter()
            .filter_map(|i| if let Item::Error { line, .. } = i { Some(*line) } else { None })
            .collect();
        assert_eq!(lines, vec![2, 3, 4, 5]);
        assert_eq!(items[0], open("ok", "x"));
    }

    #[test]
    fn env_expansion() {
        let items = parse("A | https://x/%USERNAME%_name\nB | %USERPROFILE%\\Desktop\n", &env);
        assert_eq!(items, vec![open("A", "https://x/taro_name"), open("B", r"C:\Users\taro\Desktop")]);
    }

    #[test]
    fn percent_encoding_is_not_touched() {
        // URLの %20 や %E5%85 は、実在する環境変数名ではないのでそのまま残る。
        let url = "https://x/Shared%20Documents/%E5%85%B1%E6%9C%89/a%20b?d=1";
        assert_eq!(expand_env(url, &env), url);
        // 閉じていない % も壊さない。
        assert_eq!(expand_env("100% sure %USERNAME", &env), "100% sure %USERNAME");
        assert_eq!(expand_env("%%USERNAME%%", &env), "%taro%");
    }

    #[test]
    fn default_text_is_valid() {
        let items = parse(DEFAULT_TEXT, &env);
        assert!(items.iter().all(|i| !matches!(i, Item::Error { .. })), "{items:?}");
        assert!(items.iter().any(|i| matches!(i, Item::Shutdown { .. })));
        assert!(items.iter().filter(|i| matches!(i, Item::Open { .. })).count() >= 5);
    }

    #[test]
    fn load_creates_default_and_reports_bad_encoding() {
        let dir = std::env::temp_dir().join(format!("wl-test-{}", std::process::id()));
        let path = dir.join("sub").join("menu.txt");
        let items = load(&path); // 無ければ作る
        assert!(path.exists());
        assert!(items.iter().any(|i| matches!(i, Item::Open { .. })));

        std::fs::write(&path, [0x82u8, 0xA0, 0x82, 0xA2]).unwrap(); // Shift_JIS の「あい」
        let items = load(&path);
        assert!(matches!(items.as_slice(), [Item::Error { line: 0, .. }]));

        assert!(stamp(&path).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

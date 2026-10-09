//! シャットダウン時刻の解釈と、現在時刻からの待ち秒数の計算。
//! OSに依存しない純粋なロジックなので、どのOSでも `cargo test` できる。

/// ローカル時刻のスナップショット。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    pub millis: u32,
}

/// 予約内容。`secs` が `shutdown /t` に渡す秒数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    pub secs: u64,
    pub year: u32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
}

const DAY_MS: u64 = 24 * 60 * 60 * 1000;

/// 全角の数字とコロンを半角にそろえる（日本語IMEが入ったままでも通すため）。
fn normalize(c: char) -> char {
    match c {
        '０'..='９' => char::from_u32('0' as u32 + (c as u32 - '０' as u32)).unwrap_or(c),
        '：' => ':',
        _ => c,
    }
}

/// `17:20` / `9:05` / `1720` / `905` を (時, 分) に変換する。
/// 範囲外（25:00 や 17:75 など）や、数字以外が混ざっていれば None。
pub fn parse_time(input: &str) -> Option<(u32, u32)> {
    let s: String = input.trim().chars().map(normalize).collect();
    if !s.is_ascii() {
        return None;
    }

    let (h, m) = if let Some((h, m)) = s.split_once(':') {
        if !(1..=2).contains(&h.len()) || m.len() != 2 {
            return None;
        }
        (h, m)
    } else {
        if !(3..=4).contains(&s.len()) {
            return None;
        }
        s.split_at(s.len() - 2)
    };

    if !h.bytes().all(|b| b.is_ascii_digit()) || !m.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some((h, m))
}

fn is_leap(y: u32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: u32, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(y) {
                29
            } else {
                28
            }
        }
    }
}

fn next_day(y: u32, m: u32, d: u32) -> (u32, u32, u32) {
    if d < days_in_month(y, m) {
        (y, m, d + 1)
    } else if m < 12 {
        (y, m + 1, 1)
    } else {
        (y + 1, 1, 1)
    }
}

/// 今日の `hour:minute:00` を目標にし、すでに過ぎていれば翌日にする。
/// 秒数はミリ秒を切り捨てる。
pub fn plan(now: &Clock, hour: u32, minute: u32) -> Plan {
    let now_ms = u64::from((now.hour * 60 + now.minute) * 60 + now.second) * 1000 + u64::from(now.millis);
    let mut target_ms = u64::from(hour * 60 + minute) * 60 * 1000;
    let (mut y, mut mo, mut d) = (now.year, now.month, now.day);

    if target_ms < now_ms {
        target_ms += DAY_MS;
        (y, mo, d) = next_day(y, mo, d);
    }

    Plan {
        secs: (target_ms - now_ms) / 1000,
        year: y,
        month: mo,
        day: d,
        hour,
        minute,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clock(y: u32, mo: u32, d: u32, h: u32, mi: u32, s: u32, ms: u32) -> Clock {
        Clock { year: y, month: mo, day: d, hour: h, minute: mi, second: s, millis: ms }
    }

    #[test]
    fn parse_ok() {
        assert_eq!(parse_time("17:20"), Some((17, 20)));
        assert_eq!(parse_time("1720"), Some((17, 20)));
        assert_eq!(parse_time("  17:20  "), Some((17, 20)));
        assert_eq!(parse_time("9:05"), Some((9, 5)));
        assert_eq!(parse_time("905"), Some((9, 5)));
        assert_eq!(parse_time("0000"), Some((0, 0)));
        assert_eq!(parse_time("23:59"), Some((23, 59)));
    }

    #[test]
    fn parse_fullwidth() {
        assert_eq!(parse_time("１７：２０"), Some((17, 20)));
        assert_eq!(parse_time("１７２０"), Some((17, 20)));
    }

    #[test]
    fn parse_ng() {
        for s in ["", "   ", "24:00", "17:60", "2400", "1760", "17:5", "17", "12345", "ab:cd", "17-20", "17:20:00", "1７:2o", ":20", "１７時"] {
            assert_eq!(parse_time(s), None, "入力: {s:?}");
        }
    }

    #[test]
    fn same_day() {
        // 09:00:00.000 → 17:20 は 8時間20分 = 30000秒
        let p = plan(&clock(2026, 10, 9, 9, 0, 0, 0), 17, 20);
        assert_eq!(p.secs, 8 * 3600 + 20 * 60);
        assert_eq!((p.year, p.month, p.day, p.hour, p.minute), (2026, 10, 9, 17, 20));
    }

    #[test]
    fn truncates_millis() {
        // 17:19:59.500 → 17:20:00 まで 0.5秒 → 0秒に切り捨て
        let p = plan(&clock(2026, 10, 9, 17, 19, 59, 500), 17, 20);
        assert_eq!(p.secs, 0);
        // 17:19:58.500 → 1.5秒 → 1秒
        let p = plan(&clock(2026, 10, 9, 17, 19, 58, 500), 17, 20);
        assert_eq!(p.secs, 1);
    }

    #[test]
    fn past_time_goes_to_tomorrow() {
        // 18:00 に 17:20 を指定 → 翌日17:20 = 23時間20分
        let p = plan(&clock(2026, 10, 9, 18, 0, 0, 0), 17, 20);
        assert_eq!(p.secs, 23 * 3600 + 20 * 60);
        assert_eq!((p.year, p.month, p.day), (2026, 10, 10));
    }

    #[test]
    fn current_minute_already_started_goes_to_tomorrow() {
        // 17:20:30 に 17:20 を指定 → 目標(17:20:00)は過去 → 翌日
        let p = plan(&clock(2026, 10, 9, 17, 20, 30, 0), 17, 20);
        assert_eq!(p.secs, 24 * 3600 - 30);
        assert_eq!(p.day, 10);
    }

    #[test]
    fn month_and_year_rollover() {
        let p = plan(&clock(2026, 10, 31, 23, 0, 0, 0), 1, 0);
        assert_eq!((p.year, p.month, p.day), (2026, 11, 1));
        let p = plan(&clock(2026, 12, 31, 23, 0, 0, 0), 1, 0);
        assert_eq!((p.year, p.month, p.day), (2027, 1, 1));
        assert_eq!(p.secs, 2 * 3600);
    }

    #[test]
    fn leap_year() {
        let p = plan(&clock(2028, 2, 28, 23, 0, 0, 0), 1, 0);
        assert_eq!((p.year, p.month, p.day), (2028, 2, 29));
        let p = plan(&clock(2028, 2, 29, 23, 0, 0, 0), 1, 0);
        assert_eq!((p.year, p.month, p.day), (2028, 3, 1));
        let p = plan(&clock(2027, 2, 28, 23, 0, 0, 0), 1, 0);
        assert_eq!((p.year, p.month, p.day), (2027, 3, 1));
        // 1900年は平年、2000年は閏年
        assert_eq!(days_in_month(1900, 2), 28);
        assert_eq!(days_in_month(2000, 2), 29);
    }
}

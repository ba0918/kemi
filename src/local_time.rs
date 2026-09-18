//! 一覧と選択画面に出すローカル時刻（R-SESSION）。`YYYY-MM-DD HH:MM`。
//!
//! 時刻の整形に新しい依存は足さない。unix は `localtime_r`、Windows は
//! `FileTimeToLocalFileTime` を使い、失敗したときは UTC で出す。

/// 一覧に出すローカル時刻。millis は UNIX エポックからのミリ秒。
pub fn format_local(millis: u128) -> String {
    let seconds = (millis / 1000) as i64;
    match local_parts(seconds) {
        Some((year, month, day, hour, minute)) => format_parts(year, month, day, hour, minute),
        None => {
            let (year, month, day, hour, minute) = utc_parts(seconds);
            format_parts(year, month, day, hour, minute)
        }
    }
}

fn format_parts(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> String {
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}")
}

/// UNIX 秒を UTC の年月日時分に直す（Howard Hinnant の civil_from_days）。
fn utc_parts(seconds: i64) -> (i32, u32, u32, u32, u32) {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    (
        year,
        month,
        day,
        (rest / 3_600) as u32,
        ((rest % 3_600) / 60) as u32,
    )
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year as i32, month as u32, day as u32)
}

#[cfg(unix)]
fn local_parts(seconds: i64) -> Option<(i32, u32, u32, u32, u32)> {
    // glibc / musl / macOS で先頭の 9 つの int は同じ並び。64 bit の対象だけを扱う。
    #[repr(C)]
    struct Tm {
        tm_sec: i32,
        tm_min: i32,
        tm_hour: i32,
        tm_mday: i32,
        tm_mon: i32,
        tm_year: i32,
        tm_wday: i32,
        tm_yday: i32,
        tm_isdst: i32,
        tm_gmtoff: i64,
        tm_zone: *const i8,
    }
    extern "C" {
        fn localtime_r(timep: *const i64, result: *mut Tm) -> *mut Tm;
    }
    let mut tm = std::mem::MaybeUninit::<Tm>::uninit();
    let result = unsafe { localtime_r(&seconds, tm.as_mut_ptr()) };
    if result.is_null() {
        return None;
    }
    let tm = unsafe { tm.assume_init() };
    Some((
        tm.tm_year + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
    ))
}

#[cfg(windows)]
fn local_parts(millis_seconds: i64) -> Option<(i32, u32, u32, u32, u32)> {
    #[repr(C)]
    struct FileTime {
        low: u32,
        high: u32,
    }
    #[repr(C)]
    struct SystemTime {
        year: u16,
        month: u16,
        day_of_week: u16,
        day: u16,
        hour: u16,
        minute: u16,
        second: u16,
        milliseconds: u16,
    }
    extern "system" {
        fn FileTimeToLocalFileTime(input: *const FileTime, output: *mut FileTime) -> i32;
        fn FileTimeToSystemTime(input: *const FileTime, output: *mut SystemTime) -> i32;
    }
    // 1601-01-01 から 1970-01-01 までの秒数。
    const EPOCH_DIFFERENCE: u64 = 11_644_473_600;
    let seconds = u64::try_from(millis_seconds).ok()?;
    let ticks = (seconds + EPOCH_DIFFERENCE) * 10_000_000;
    let utc = FileTime {
        low: (ticks & 0xffff_ffff) as u32,
        high: (ticks >> 32) as u32,
    };
    let mut local = FileTime { low: 0, high: 0 };
    let mut parts = SystemTime {
        year: 0,
        month: 0,
        day_of_week: 0,
        day: 0,
        hour: 0,
        minute: 0,
        second: 0,
        milliseconds: 0,
    };
    let local_ok = unsafe { FileTimeToLocalFileTime(&utc, &mut local) };
    let parts_ok = unsafe { FileTimeToSystemTime(&local, &mut parts) };
    if local_ok == 0 || parts_ok == 0 {
        return None;
    }
    Some((
        i32::from(parts.year),
        u32::from(parts.month),
        u32::from(parts.day),
        u32::from(parts.hour),
        u32::from(parts.minute),
    ))
}

#[cfg(not(any(unix, windows)))]
fn local_parts(_seconds: i64) -> Option<(i32, u32, u32, u32, u32)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_is_padded_to_the_documented_shape() {
        assert_eq!(format_parts(2026, 1, 2, 3, 4), "2026-01-02 03:04");
    }

    #[test]
    fn utc_fallback_is_correct_across_a_leap_boundary() {
        // 2000-02-29T23:59:00Z
        assert_eq!(utc_parts(951_868_740), (2000, 2, 29, 23, 59));
        // 1970-01-01T00:00:00Z
        assert_eq!(utc_parts(0), (1970, 1, 1, 0, 0));
        // 1969-12-31T23:58:00Z（負の時刻）
        assert_eq!(utc_parts(-120), (1969, 12, 31, 23, 58));
    }

    #[cfg(unix)]
    #[test]
    fn local_time_matches_the_date_command() {
        let expected = std::process::Command::new("date")
            .args(["-d", "@1700000000", "+%Y-%m-%d %H:%M"])
            .output();
        let Ok(expected) = expected else {
            return;
        };
        if !expected.status.success() {
            return;
        }
        let expected = String::from_utf8_lossy(&expected.stdout).trim().to_string();

        assert_eq!(format_local(1_700_000_000_000), expected);
    }
}

use std::path::Path;
use std::sync::LazyLock;
use chrono::{Datelike, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone};
use regex::Regex;

struct FilenamePattern {
    re: Regex,
    has_time: bool,
    is_unix_ms: bool,
}

static FILENAME_TIMESTAMP_PATTERNS: LazyLock<Vec<FilenamePattern>> = LazyLock::new(|| {
    vec![
        // YYYYMMDD[_-]HHMMSS - e.g. 20240709_182027.mp4, PXL_20231123_182518628.jpg
        FilenamePattern {
            re: Regex::new(r"(\d{4})(\d{2})(\d{2})[_-](\d{2})(\d{2})(\d{2})\d*").unwrap(),
            has_time: true,
            is_unix_ms: false,
        },
        // YYYY-MM-DD[sep HHMMSS] - e.g. 2022-10-24-150226287.mp4, Screenshot 2026-02-13 093505.png, 2026-07-23_18-35-57
        FilenamePattern {
            re: Regex::new(r"(\d{4})-(\d{1,2})-(\d{1,2})(?:[ _-](\d{2})[-:]?(\d{2})[-:]?(\d{2})\d*)?").unwrap(),
            has_time: false,
            is_unix_ms: false,
        },
        // [non-digit]YYYYMMDDHHMMSS[non-digit] - e.g. lv_7324034615860006160_20240617193045.mp4
        FilenamePattern {
            re: Regex::new(r"(?:^|[^0-9])(\d{4})(\d{2})(\d{2})(\d{2})(\d{2})(\d{2})(?:[^0-9]|$)").unwrap(),
            has_time: true,
            is_unix_ms: false,
        },
        // Unix milliseconds - e.g. FaceApp_1658848332262.jpg (covers 2001-2033)
        FilenamePattern {
            re: Regex::new(r"(?:^|[^0-9])(1\d{12})(?:[^0-9]|$)").unwrap(),
            has_time: true,
            is_unix_ms: true,
        },
    ]
});

/// Extracts a Unix timestamp (seconds) from common photo/video filename patterns.
/// Returns `None` if no valid date between 1990 and `current_year + 1` is matched.
pub fn parse_timestamp_from_filename(path: &Path) -> Option<i64> {
    let base = path.file_name()?.to_str()?;
    let max_year = Local::now().year() + 1;

    for pat in FILENAME_TIMESTAMP_PATTERNS.iter() {
        let Some(caps) = pat.re.captures(base) else {
            continue;
        };

        if pat.is_unix_ms {
            let Some(ms_str) = caps.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let Ok(ms) = ms_str.parse::<i64>() else {
                continue;
            };
            let secs = ms.div_euclid(1000);
            if let Some(dt) = Local.timestamp_opt(secs, 0).single() {
                if dt.year() >= 1990 && dt.year() <= max_year {
                    return Some(dt.timestamp());
                }
            }
            continue;
        }

        let year: i32 = caps.get(1)?.as_str().parse().ok()?;
        let month: u32 = caps.get(2)?.as_str().parse().ok()?;
        let day: u32 = caps.get(3)?.as_str().parse().ok()?;

        if year < 1990 || year > max_year {
            continue;
        }

        let (hour, min, sec) = if pat.has_time || caps.get(4).is_some() {
            let h: u32 = caps.get(4).and_then(|m| m.as_str().parse().ok()).unwrap_or(12);
            let m: u32 = caps.get(5).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            let s: u32 = caps.get(6).and_then(|m| m.as_str().parse().ok()).unwrap_or(0);
            (h, m, s)
        } else {
            (12, 0, 0)
        };

        let Some(date) = NaiveDate::from_ymd_opt(year, month, day) else {
            continue;
        };
        let Some(time) = NaiveTime::from_hms_opt(hour, min, sec) else {
            continue;
        };
        let naive = NaiveDateTime::new(date, time);
        if let Some(local_dt) = Local.from_local_datetime(&naive).earliest() {
            return Some(local_dt.timestamp());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pixel_and_standard_datetime() {
        let ts = parse_timestamp_from_filename(Path::new("/photos/PXL_20231123_182518628.jpg")).unwrap();
        let dt = Local.timestamp_opt(ts, 0).single().unwrap();
        assert_eq!((dt.year(), dt.month(), dt.day()), (2023, 11, 23));

        let ts2 = parse_timestamp_from_filename(Path::new("20240709_182027.mp4")).unwrap();
        let dt2 = Local.timestamp_opt(ts2, 0).single().unwrap();
        assert_eq!((dt2.year(), dt2.month(), dt2.day()), (2024, 7, 9));
    }

    #[test]
    fn test_parse_dashed_date_and_instagram_style() {
        let ts = parse_timestamp_from_filename(Path::new("Screenshot 2026-02-13 093505.png")).unwrap();
        let dt = Local.timestamp_opt(ts, 0).single().unwrap();
        assert_eq!((dt.year(), dt.month(), dt.day()), (2026, 2, 13));

        let ts2 = parse_timestamp_from_filename(Path::new("x_2026-07-23_18-35-57_2_DbJpQAtmNx4.jpg")).unwrap();
        let dt2 = Local.timestamp_opt(ts2, 0).single().unwrap();
        assert_eq!((dt2.year(), dt2.month(), dt2.day()), (2026, 7, 23));

        let ts3 = parse_timestamp_from_filename(Path::new("photo_2024-5-6.jpg")).unwrap();
        let dt3 = Local.timestamp_opt(ts3, 0).single().unwrap();
        assert_eq!((dt3.year(), dt3.month(), dt3.day()), (2024, 5, 6));
    }

    #[test]
    fn test_parse_unix_ms_and_reject_invalid() {
        let ts = parse_timestamp_from_filename(Path::new("FaceApp_1658848332262.jpg")).unwrap();
        assert_eq!(ts, 1658848332);

        assert_eq!(parse_timestamp_from_filename(Path::new("IMG_0001.jpg")), None);
        assert_eq!(parse_timestamp_from_filename(Path::new("18000101_120000.jpg")), None);
    }
}

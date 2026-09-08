//! One timestamp format, everywhere.
//!
//! ISO-8601 UTC with milliseconds, stored as TEXT. The format is fixed width, so SQLite's
//! lexicographic string comparison *is* chronological comparison — which is what lets
//! `updated_at > ?` work without a date type, and what lets a merge compare two timestamps from
//! two devices as plain strings.
//!
//! There is no clock in this crate. Every function here takes the instant it needs, because a
//! rule that reads the wall clock cannot be tested and cannot be run identically on two devices.

/// `2026-09-08T14:30:00.000Z` — always 24 characters, always UTC, always three decimal places.
pub const FORMAT_WIDTH: usize = 24;

/// Formats a Unix millisecond instant.
pub fn format(epoch_ms: i64) -> String {
    let days = epoch_ms.div_euclid(86_400_000);
    let time = epoch_ms.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);

    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        time / 3_600_000,
        time / 60_000 % 60,
        time / 1_000 % 60,
        time % 1_000
    )
}

/// Reads `2026-09-08`, `2026-09-08T14:30:00Z` and `2026-09-08T14:30:00.123Z`, with or without
/// the `Z`. Anything else is not a timestamp — including a local-offset one, which the app never
/// writes and must not silently misread as UTC.
pub fn parse(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();

    if bytes.len() < 10 {
        return None;
    }

    let number = |from: usize, to: usize| -> Option<i64> { text.get(from..to)?.parse().ok() };

    if bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }

    let days = days_from_civil(number(0, 4)?, number(5, 7)?, number(8, 10)?)?;

    if bytes.len() == 10 {
        return Some(days * 86_400_000);
    }

    if bytes[10] != b'T' && bytes[10] != b' ' {
        return None;
    }

    if bytes.len() < 19 || bytes[13] != b':' || bytes[16] != b':' {
        return None;
    }

    let (hour, minute, second) = (number(11, 13)?, number(14, 16)?, number(17, 19)?);

    if hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let millis = match bytes.get(19) {
        Some(b'.') => {
            let digits: String = text[20..]
                .chars()
                .take_while(char::is_ascii_digit)
                .collect();

            match digits.len() {
                0 => return None,
                _ => {
                    let fraction: i64 = digits.parse().ok()?;
                    let scale = 10_i64.pow(digits.len() as u32);

                    match text[20 + digits.len()..].trim_end_matches('Z') {
                        "" => fraction * 1_000 / scale,
                        _ => return None,
                    }
                }
            }
        }
        Some(b'Z') if bytes.len() == 20 => 0,
        None => 0,
        _ => return None,
    };

    Some((days * 86_400 + hour * 3_600 + minute * 60 + second) * 1_000 + millis)
}

/// The start of the UTC day an instant falls in. Whole days, not hours: a set scheduled for
/// today is "today" all day, and yesterday's set is one day old at breakfast as well as at
/// midnight.
pub fn midnight(epoch_ms: i64) -> i64 {
    epoch_ms.div_euclid(86_400_000) * 86_400_000
}

/// Whole days between two instants, rounded — the unit a person means by "in three days".
pub fn days_between(from_ms: i64, to_ms: i64) -> i64 {
    (midnight(to_ms) - midnight(from_ms)) / 86_400_000
}

/// Days since 1970-01-01 from a civil date, and the inverse. Howard Hinnant's algorithms: exact
/// for every proleptic Gregorian date, and no table.
fn days_from_civil(year: i64, month: i64, day: i64) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;

    Some(era * 146_097 + day_of_era - 719_468)
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = month_index + if month_index < 10 { 3 } else { -9 };

    (year + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_fixed_width_utc_timestamp() {
        assert_eq!(format(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(format(1_757_332_200_123), "2025-09-08T11:50:00.123Z");
        assert_eq!(format(1_757_332_200_123).len(), FORMAT_WIDTH);
    }

    /// The whole reason for the format: string order is time order, which is what the `>` in
    /// `updated_at > ?` relies on.
    #[test]
    fn sorts_as_a_string_the_way_it_sorts_as_a_time() {
        let mut stamps: Vec<String> = [2_000_000_000_000, 999, 1_757_332_200_123, 0]
            .iter()
            .map(|instant| format(*instant))
            .collect();
        stamps.sort();

        assert_eq!(
            stamps,
            [
                format(0),
                format(999),
                format(1_757_332_200_123),
                format(2_000_000_000_000)
            ]
        );
    }

    #[test]
    fn reads_back_everything_it_writes() {
        for instant in [0, 999, 1_757_332_200_123, 2_000_000_000_000, -86_400_000] {
            assert_eq!(parse(&format(instant)), Some(instant), "{instant}");
        }
    }

    #[test]
    fn reads_the_shapes_the_database_and_a_user_supply() {
        assert_eq!(parse("2026-09-08"), Some(1_788_825_600_000));
        assert_eq!(parse("2026-09-08T00:00:00Z"), Some(1_788_825_600_000));
        assert_eq!(parse("2026-09-08T00:00:00"), Some(1_788_825_600_000));
        assert_eq!(parse("2026-09-08T00:00:00.5Z"), Some(1_788_825_600_500));
    }

    #[test]
    fn refuses_what_is_not_a_timestamp() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("not a date"), None);
        assert_eq!(parse("2026-13-01"), None);
        assert_eq!(parse("2026-09-08T25:00:00Z"), None);
        // A local offset would be read as UTC and silently shift the day.
        assert_eq!(parse("2026-09-08T12:00:00+02:00"), None);
    }

    #[test]
    fn counts_whole_days_however_late_in_the_day_it_is_asked() {
        let morning = parse("2026-09-06T08:00:00.000Z").unwrap();
        let night = parse("2026-09-06T23:59:59.999Z").unwrap();
        let sunday = parse("2026-09-08").unwrap();

        assert_eq!(days_between(morning, sunday), 2);
        assert_eq!(days_between(night, sunday), 2);
        assert_eq!(days_between(sunday, morning), -2);
        assert_eq!(midnight(night), parse("2026-09-06").unwrap());
    }

    #[test]
    fn gets_the_leap_years_right() {
        assert_eq!(
            format(parse("2024-02-29").unwrap()),
            "2024-02-29T00:00:00.000Z"
        );
        assert_eq!(
            format(parse("2000-02-29").unwrap()),
            "2000-02-29T00:00:00.000Z"
        );
        assert_eq!(
            days_between(parse("2100-02-28").unwrap(), parse("2100-03-01").unwrap()),
            1
        );
    }
}

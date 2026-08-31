/// Port of `packages/shared/src/timer.ts` + `formatTime`.

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub fn current_elapsed_seconds(
    stored_elapsed: i64,
    is_running: bool,
    start_time_ms: Option<i64>,
    now_ms: i64,
) -> i64 {
    match (is_running, start_time_ms) {
        (true, Some(start)) => stored_elapsed + (now_ms - start).max(0) / 1000,
        _ => stored_elapsed,
    }
}

pub fn format_time(total_seconds: i64) -> String {
    let s = total_seconds.max(0);
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

/// Parse `HH:MM:SS`, `MM:SS`, seconds, or plain minutes (e.g. `90` → 5400s).
pub fn parse_time_input(value: &str) -> Option<i64> {
    let input = value.trim();
    if input.is_empty() {
        return None;
    }
    if regex_like_colon_time(input) {
        let parts: Vec<i64> = input.split(':').filter_map(|p| p.parse().ok()).collect();
        if parts.len() != input.split(':').count() || parts.iter().any(|&n| n < 0) {
            return None;
        }
        let (h, m, s) = match parts.len() {
            3 => (parts[0], parts[1], parts[2]),
            2 => (0, parts[0], parts[1]),
            1 => (0, 0, parts[0]),
            _ => return None,
        };
        return Some(h * 3600 + m * 60 + s);
    }
    let normalized = input.replace(',', ".");
    let as_number: f64 = normalized.parse().ok()?;
    if !as_number.is_finite() || as_number < 0.0 {
        return None;
    }
    Some((as_number * 60.0).round() as i64)
}

fn regex_like_colon_time(input: &str) -> bool {
    let mut parts = input.split(':');
    let Some(first) = parts.next() else {
        return false;
    };
    if first.is_empty() || !first.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    for part in parts {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_returns_stored() {
        assert_eq!(current_elapsed_seconds(90, false, Some(0), 10_000), 90);
        assert_eq!(current_elapsed_seconds(90, true, None, 10_000), 90);
    }

    #[test]
    fn running_adds_live_seconds() {
        assert_eq!(current_elapsed_seconds(10, true, Some(1_000), 4_000), 13);
        assert_eq!(current_elapsed_seconds(10, true, Some(5_000), 4_000), 10);
    }

    #[test]
    fn format_pads() {
        assert_eq!(format_time(0), "00:00:00");
        assert_eq!(format_time(3661), "01:01:01");
    }

    #[test]
    fn parse_colon_formats() {
        assert_eq!(parse_time_input("01:30:00"), Some(5400));
        assert_eq!(parse_time_input("5:30"), Some(330));
        assert_eq!(parse_time_input("45"), Some(45));
    }

    #[test]
    fn parse_minutes_fallback() {
        assert_eq!(parse_time_input("90"), Some(90));
        assert_eq!(parse_time_input("1.5"), Some(90));
    }

    #[test]
    fn parse_rejects_invalid() {
        assert_eq!(parse_time_input(""), None);
        assert_eq!(parse_time_input("abc"), None);
        assert_eq!(parse_time_input("-1"), None);
    }
}

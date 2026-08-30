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
}

// Port of `packages/shared/src/report.ts` — keep output identical to web/MCP.

use crate::timer::seconds_to_story_points;

pub struct ReportTask<'a> {
    pub label: &'a str,
    pub description: Option<&'a str>,
    pub elapsed_seconds: i64,
}

/// Google-Chat-friendly daily report: bold header + only tasks with recorded time.
pub fn build_markdown_report(tasks: &[ReportTask<'_>], date_iso: &str) -> String {
    let mut lines = vec![format!("*Daily Report {date_iso}*")];
    for t in tasks {
        if t.elapsed_seconds <= 0 {
            continue;
        }
        let mut line = format!(
            "- [{:.2}] {}",
            seconds_to_story_points(t.elapsed_seconds),
            t.label
        );
        if let Some(desc) = t.description.map(str::trim).filter(|d| !d.is_empty()) {
            line.push_str(&format!(" - {desc}"));
        }
        lines.push(line);
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skips_zero_elapsed_and_formats_points() {
        let tasks = [
            ReportTask {
                label: "quiet",
                description: None,
                elapsed_seconds: 0,
            },
            ReportTask {
                label: "work",
                description: Some("notes"),
                elapsed_seconds: 3600,
            },
        ];
        let md = build_markdown_report(&tasks, "2026-08-31");
        assert_eq!(
            md,
            "*Daily Report 2026-08-31*\n- [1.00] work - notes"
        );
    }
}

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

/// CSV with all tasks: Task, Description, Story Points.
pub fn build_csv_report(tasks: &[ReportTask<'_>]) -> String {
    fn field(s: &str) -> String {
        format!("\"{}\"", s.replace('"', "\"\""))
    }
    let mut rows = vec![format!(
        "{},{},{}",
        field("Task"),
        field("Description"),
        field("Story Points")
    )];
    for t in tasks {
        rows.push(format!(
            "{},{},{}",
            field(t.label),
            field(t.description.unwrap_or("")),
            field(&format!("{:.2}", seconds_to_story_points(t.elapsed_seconds)))
        ));
    }
    rows.join("\r\n")
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

    #[test]
    fn csv_quotes_fields_and_includes_zero_elapsed() {
        let tasks = [ReportTask {
            label: "work",
            description: Some("a \"quote\""),
            elapsed_seconds: 3600,
        }];
        let csv = build_csv_report(&tasks);
        assert_eq!(
            csv,
            "\"Task\",\"Description\",\"Story Points\"\r\n\"work\",\"a \"\"quote\"\"\",\"1.00\""
        );
    }
}

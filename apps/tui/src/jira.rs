use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
struct Credentials {
    email: String,
    token: String,
}

#[derive(Debug, Deserialize)]
struct IssueResponse {
    fields: Option<IssueFields>,
}

#[derive(Debug, Deserialize)]
struct IssueFields {
    summary: Option<String>,
}

fn load_credentials() -> Option<Credentials> {
    let mut email = env::var("JIRA_EMAIL").ok();
    let mut token = env::var("JIRA_TOKEN").ok();

    if email.is_none() || token.is_none() {
        let path = env::var("JIRA_ENV_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|_| dirs_home().join(".aimsis").join("jira.env"));
        if let Ok(contents) = fs::read_to_string(path) {
            for line in contents.lines() {
                let Some((key, value)) = line
                    .trim()
                    .strip_prefix("export ")
                    .unwrap_or_else(|| line.trim())
                    .split_once('=')
                else {
                    continue;
                };
                let value = value.trim().trim_matches(['"', '\'']);
                match key.trim() {
                    "JIRA_EMAIL" if email.is_none() => email = Some(value.to_string()),
                    "JIRA_TOKEN" if token.is_none() => token = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }

    match (email, token) {
        (Some(email), Some(token)) if !email.is_empty() && !token.is_empty() => {
            Some(Credentials { email, token })
        }
        _ => None,
    }
}

fn dirs_home() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn issue_key(label: &str, description: &str) -> Option<String> {
    label
        .split_whitespace()
        .chain(description.split_whitespace())
        .find_map(|word| {
            let key = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '-');
            let (project, number) = key.split_once('-')?;
            if !project.is_empty()
                && project
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                && !number.is_empty()
                && number.chars().all(|c| c.is_ascii_digit())
            {
                Some(key.to_string())
            } else {
                None
            }
        })
}

fn merge_description(summary: &str, description: &str) -> String {
    if description.trim().is_empty() {
        summary.to_string()
    } else if description.contains(summary) {
        description.to_string()
    } else {
        format!("{summary}\n\n{description}")
    }
}

/// Fetch the JIRA summary for a key in the label/description and merge it into
/// the task description, matching the web task creation behavior.
pub fn description_for_task(label: &str, description: &str) -> Result<Option<String>> {
    let Some(key) = issue_key(label, description) else {
        return Ok(None);
    };
    let Some(credentials) = load_credentials() else {
        return Ok(None);
    };
    let site = env::var("JIRA_SITE")
        .unwrap_or_else(|_| "https://aimsis.atlassian.net".to_string())
        .trim_end_matches('/')
        .to_string();
    let url = format!("{site}/rest/api/3/issue/{key}?fields=summary");
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .context("build JIRA client")?;
    let response = client
        .get(url)
        .basic_auth(credentials.email, Some(credentials.token))
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .context("request JIRA issue")?
        .error_for_status()
        .context("JIRA issue request failed")?;
    let issue: IssueResponse = response.json().context("parse JIRA issue response")?;
    Ok(issue
        .fields
        .and_then(|fields| fields.summary)
        .map(|summary| merge_description(&summary, description)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_issue_key_in_label_or_description() {
        assert_eq!(issue_key("US-1459 fix", ""), Some("US-1459".into()));
        assert_eq!(issue_key("fix", "see AIM-22"), Some("AIM-22".into()));
        assert_eq!(issue_key("ordinary task", "no ticket"), None);
    }

    #[test]
    fn merges_summary_without_duplicating_existing_text() {
        assert_eq!(merge_description("Fix login", ""), "Fix login");
        assert_eq!(
            merge_description("Fix login", "repro steps"),
            "Fix login\n\nrepro steps"
        );
        assert_eq!(
            merge_description("Fix login", "Fix login\n\nrepro steps"),
            "Fix login\n\nrepro steps"
        );
    }
}

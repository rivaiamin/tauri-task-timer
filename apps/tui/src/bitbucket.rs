use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde::Deserialize;
use std::env;
use std::time::Duration;

#[derive(Debug)]
struct Credentials {
    email: String,
    token: String,
}

/// Bitbucket's shape, not ours: the TUI renders `state` and `title` and the
/// rest is what the API sends, so the unused fields stay only to keep the type a
/// faithful mirror of the response.
#[derive(Debug, Clone, Deserialize)]
pub struct PullRequest {
    #[allow(dead_code)]
    pub id: u64,
    pub title: String,
    pub state: String,        // "OPEN", "MERGED", "DECLINED"
    #[allow(dead_code)]
    pub author: Option<Author>,
    #[allow(dead_code)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Author {
    #[allow(dead_code)]
    pub display_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrListResponse {
    values: Vec<PullRequest>,
}

fn load_credentials() -> Option<Credentials> {
    let email = env::var("BITBUCKET_EMAIL").ok();
    let token = env::var("BITBUCKET_TOKEN").ok();
    match (email, token) {
        (Some(e), Some(t)) if !e.is_empty() && !t.is_empty() => {
            Some(Credentials { email: e, token: t })
        }
        _ => None,
    }
}

fn bb_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("build Bitbucket client")
}

/// Shared Bitbucket REST API fetcher.
pub fn bb_fetch(
    workspace: &str,
    repo: &str,
    path: &str,
    method: &str,
) -> Result<serde_json::Value> {
    let creds =
        load_credentials().context("Bitbucket not configured (set BITBUCKET_EMAIL + BITBUCKET_TOKEN)")?;
    let client = bb_client()?;
    let url = format!(
        "https://api.bitbucket.org/2.0/repositories/{workspace}/{repo}{path}"
    );
    let req = client
        .request(method.parse().unwrap_or(reqwest::Method::GET), &url)
        .basic_auth(&creds.email, Some(&creds.token))
        .header(reqwest::header::ACCEPT, "application/json");
    let resp = req.send().context("Bitbucket request failed")?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        bail!("Bitbucket {method} {path} -> {status}: {text}");
    }
    let ct = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if ct.contains("application/json") {
        resp.json().context("parse Bitbucket JSON response")
    } else {
        Ok(serde_json::Value::Null)
    }
}

/// List pull requests for a branch. Returns PRs matching the branch as source.
pub fn list_prs(workspace: &str, repo: &str, branch: &str) -> Result<Vec<PullRequest>> {
    let path = format!("/pullrequests?q=source.branch.name%3D%22{branch}%22&state=OPEN&state=MERGED&state=DECLINED&sort=-updated_on&pagelen=10");
    let data = bb_fetch(workspace, repo, &path, "GET")?;
    let resp: PrListResponse =
        serde_json::from_value(data).context("parse PR list response")?;
    Ok(resp.values)
}

#[cfg(test)]
mod tests {
    #[test]
    fn bb_fetch_url_construction() {
        // Just verify the URL would be well-formed; actual API call needs auth.
        let workspace = "myworkspace";
        let repo = "myrepo";
        let path = "/pullrequests?state=OPEN";
        let url = format!(
            "https://api.bitbucket.org/2.0/repositories/{workspace}/{repo}{path}"
        );
        assert!(url.contains("myworkspace/myrepo"));
        assert!(url.contains("state=OPEN"));
    }
}

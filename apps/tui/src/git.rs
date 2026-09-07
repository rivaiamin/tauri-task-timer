use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
}

/// Get the current branch name for a repo.
pub fn current_branch(repo: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("git: failed to run rev-parse")?;
    if !out.status.success() {
        anyhow::bail!("git: not a repository or detached HEAD");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// List recent commits on a branch (default: HEAD).
/// Returns up to `limit` (hash, subject) tuples.
pub fn commits_for_branch(repo: &Path, branch: &str, limit: usize) -> Result<Vec<Commit>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["log", "--oneline", &format!("-{limit}"), branch])
        .output()
        .context("git: failed to run log")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("git log failed: {stderr}");
    }
    let mut result = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some((hash, subject)) = line.split_once(' ') {
            result.push(Commit {
                hash: hash.to_string(),
                subject: subject.to_string(),
            });
        }
    }
    Ok(result)
}

/// Check how many commits `branch` is ahead/behind compared to its upstream or main.
/// Returns (ahead, behind).
pub fn branch_ahead_behind(repo: &Path, branch: &str) -> Result<(u32, u32)> {
    // Try upstream first
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "rev-list",
            "--left-right",
            "--count",
            &format!("{branch}...@{{u}}"),
        ])
        .output();
    if let Ok(o) = out {
        if o.status.success() {
            let text = String::from_utf8_lossy(&o.stdout);
            let parts: Vec<&str> = text.trim().split_whitespace().collect();
            if parts.len() == 2 {
                let ahead = parts[0].parse().unwrap_or(0);
                let behind = parts[1].parse().unwrap_or(0);
                return Ok((ahead, behind));
            }
        }
    }
    // Fallback: compare against main/master
    for base in &["main", "master"] {
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["rev-list", "--left-right", "--count", &format!("{branch}...{base}")])
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let text = String::from_utf8_lossy(&o.stdout);
                let parts: Vec<&str> = text.trim().split_whitespace().collect();
                if parts.len() == 2 {
                    let ahead = parts[0].parse().unwrap_or(0);
                    let behind = parts[1].parse().unwrap_or(0);
                    return Ok((ahead, behind));
                }
            }
        }
    }
    Ok((0, 0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_struct_holds_data() {
        let c = Commit {
            hash: "abc1234".into(),
            subject: "fix: something".into(),
        };
        assert_eq!(c.hash, "abc1234");
        assert_eq!(c.subject, "fix: something");
    }
}

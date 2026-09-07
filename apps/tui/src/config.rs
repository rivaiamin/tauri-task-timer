use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub database_path: String,
    pub user_email: String,
    pub timer_mode: Option<String>,
    pub jira_board: Option<String>,
    pub jira_sprint_id: Option<String>,
    pub bitbucket_workspace: Option<String>,
    pub bitbucket_repo: Option<String>,
    pub git_repo_path: Option<String>,
}

const EXAMPLE: &str = r#"# ~/.config/task-timer-tui/config.toml
database_path = "/home/amin/projects/tauri/tauri-task-timer/apps/web/local.db"
user_email = "you@example.com"
# timer_mode = "focus"  # optional: "focus" | "parallel"
# jira_board = "AIMSIS"
# jira_sprint_id = "123"
# Bitbucket (E5) — env vars: BITBUCKET_EMAIL, BITBUCKET_TOKEN
# bitbucket_workspace = "your-workspace"
# bitbucket_repo = "your-repo"
# git_repo_path = "/path/to/repo"  # auto-detected from database_path if omitted
"#;

pub fn default_path() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("", "", "task-timer-tui")
        .context("cannot resolve config directory")?;
    Ok(dirs.config_dir().join("config.toml"))
}

pub fn load(explicit: Option<&Path>) -> Result<Config> {
    let path = match explicit {
        Some(p) => p.to_path_buf(),
        None => default_path()?,
    };
    if !path.exists() {
        bail!(
            "config not found at {}\n\nCreate it with:\n{}",
            path.display(),
            EXAMPLE
        );
    }
    let raw = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    let cfg: Config = toml::from_str(&raw).with_context(|| format!("parse {}", path.display()))?;
    if cfg.database_path.trim().is_empty() || cfg.user_email.trim().is_empty() {
        bail!("config must set database_path and user_email");
    }
    Ok(cfg)
}

impl Config {
    /// Resolve the git repo root: explicit config, or walk up from database_path.
    pub fn resolve_repo_root(&self) -> Option<PathBuf> {
        if let Some(ref p) = self.git_repo_path {
            let path = PathBuf::from(p);
            if path.join(".git").exists() {
                return Some(path);
            }
        }
        // Walk up from database_path looking for .git
        let mut cur = PathBuf::from(&self.database_path);
        cur.pop(); // strip filename
        loop {
            if cur.join(".git").exists() {
                return Some(cur);
            }
            if !cur.pop() {
                return None;
            }
        }
    }
}

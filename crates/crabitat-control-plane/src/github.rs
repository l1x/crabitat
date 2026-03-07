use serde::{Deserialize, Serialize};

use crate::models::Issue;
use crate::models::system::SystemStatus;

#[derive(Deserialize)]
struct GhIssue {
    number: i64,
    title: String,
    body: Option<String>,
    labels: Vec<GhLabel>,
    state: String,
}

#[derive(Deserialize)]
struct GhLabel {
    name: String,
}

pub async fn check_status() -> SystemStatus {
    let mut status = SystemStatus {
        gh_installed: false,
        gh_auth_status: false,
        gh_version: None,
        gh_user: None,
    };

    // Check installation and version
    let version_output = tokio::process::Command::new("gh")
        .arg("--version")
        .output()
        .await;

    if let Ok(output) = version_output
        && output.status.success()
    {
        status.gh_installed = true;
        let stdout = String::from_utf8_lossy(&output.stdout);
        status.gh_version = stdout.lines().next().map(|l| l.to_string());
    }

    if !status.gh_installed {
        return status;
    }

    // Check auth status
    let auth_output = tokio::process::Command::new("gh")
        .args(["auth", "status"])
        .output()
        .await;

    if let Ok(output) = auth_output {
        // gh auth status outputs to stderr
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let combined = format!("{}{}", stdout, stderr);

        if combined.contains("Logged in to github.com") {
            status.gh_auth_status = true;
            // Extract user: "Logged in to github.com as USER"
            if let Some(user_line) = combined
                .lines()
                .find(|l| l.contains("Logged in to github.com"))
                && let Some(user) = user_line.split_whitespace().last()
            {
                status.gh_user = Some(user.trim_matches('(').trim_matches(')').to_string());
            }
        }
    }

    status
}

pub async fn fetch_issues(owner: &str, name: &str) -> Result<Vec<Issue>, String> {
    let repo_slug = format!("{owner}/{name}");
    let output = tokio::process::Command::new("gh")
        .args([
            "issue",
            "list",
            "--repo",
            &repo_slug,
            "--json",
            "number,title,body,labels,state",
            "--limit",
            "100",
        ])
        .output()
        .await
        .map_err(|e| format!("failed to run gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let gh_issues: Vec<GhIssue> =
        serde_json::from_str(&stdout).map_err(|e| format!("failed to parse gh output: {e}"))?;

    let issues = gh_issues
        .into_iter()
        .map(|gi| Issue {
            repo_id: String::new(), // filled by caller
            number: gi.number,
            title: gi.title,
            body: gi.body,
            labels: gi.labels.into_iter().map(|l| l.name).collect(),
            state: gi.state,
            fetched_at: String::new(), // filled by DB
        })
        .collect();

    Ok(issues)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GhRepo {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
}

pub async fn search_repos(query: &str) -> Result<Vec<GhRepo>, String> {
    // Split "owner/partial" into owner + filter prefix
    let (owner, filter) = match query.split_once('/') {
        Some((o, f)) => (o, f),
        None => (query, ""),
    };

    let output = tokio::process::Command::new("gh")
        .args([
            "repo",
            "list",
            owner,
            "--json",
            "nameWithOwner",
            "--limit",
            "50",
        ])
        .output()
        .await
        .map_err(|e| format!("failed to run gh: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let repos: Vec<GhRepo> =
        serde_json::from_str(&stdout).map_err(|e| format!("failed to parse gh output: {e}"))?;

    if filter.is_empty() {
        return Ok(repos);
    }

    let filter_lower = filter.to_lowercase();
    let filtered = repos
        .into_iter()
        .filter(|r| {
            r.name_with_owner
                .split_once('/')
                .map(|(_, name)| name.to_lowercase().starts_with(&filter_lower))
                .unwrap_or(false)
        })
        .collect();

    Ok(filtered)
}

#[cfg(test)]
mod github_test;

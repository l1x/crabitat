use serde::Deserialize;

use crate::models::Issue;

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

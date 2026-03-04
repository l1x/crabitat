use crate::error::ApiError;
use serde::Deserialize;
use tracing::info;

#[derive(Clone)]
pub(crate) struct GitHubClient {
    pub(crate) http: reqwest::Client,
    pub(crate) token: Option<String>,
}

/// Unified issue type returned by both backends.
#[derive(Debug, Clone)]
pub(crate) struct GhIssue {
    pub(crate) number: i64,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) labels: Vec<String>,
    pub(crate) state: String,
}

/// Unified issue-detail type (title + body only).
#[derive(Debug, Clone)]
pub(crate) struct GhIssueDetail {
    pub(crate) title: String,
    pub(crate) body: String,
}

/// Unified PR-status type.
#[derive(Debug, Clone)]
pub(crate) struct GhPrStatus {
    pub(crate) state: String,
    pub(crate) merged_at: Option<String>,
}

// -- GraphQL response deserialization helpers --------------------------------

#[derive(Debug, Deserialize)]
struct GqlIssue {
    number: i64,
    title: String,
    body: String,
    labels: GqlLabels,
    state: String,
}

#[derive(Debug, Deserialize)]
struct GqlLabels {
    nodes: Vec<GqlLabel>,
}

#[derive(Debug, Deserialize)]
struct GqlLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct GqlIssueDetail {
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GqlPrStatus {
    state: String,
    merged_at: Option<String>,
}

// -- gh CLI response deserialization helpers ---------------------------------

#[derive(Debug, Deserialize)]
struct CliLabel {
    name: String,
}

#[derive(Debug, Deserialize)]
struct CliIssue {
    number: i64,
    title: String,
    body: String,
    labels: Vec<CliLabel>,
    state: String,
}

#[derive(Debug, Deserialize)]
struct CliIssueDetail {
    title: String,
    body: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CliPrStatus {
    state: String,
    merged_at: Option<String>,
}

impl GitHubClient {
    pub(crate) fn new() -> Self {
        Self { http: reqwest::Client::new(), token: std::env::var("GITHUB_TOKEN").ok() }
    }

    pub(crate) fn has_token(&self) -> bool {
        self.token.is_some()
    }

    // -- Public API (dispatches to GraphQL or gh CLI) -----------------------

    pub(crate) async fn list_issues(&self, repo: &str) -> Result<Vec<GhIssue>, ApiError> {
        info!(repo = %repo, "github: listing issues");
        if self.has_token() {
            let (owner, name) = parse_repo(repo)?;
            self.list_issues_graphql(owner, name).await
        } else {
            self.list_issues_cli(repo).await
        }
    }

    pub(crate) async fn get_issue(&self, repo: &str, number: i64) -> Result<GhIssueDetail, ApiError> {
        info!(repo = %repo, number, "github: fetching issue");
        if self.has_token() {
            let (owner, name) = parse_repo(repo)?;
            self.get_issue_graphql(owner, name, number).await
        } else {
            self.get_issue_cli(repo, number).await
        }
    }

    pub(crate) async fn get_pr_status(&self, repo: &str, number: i64) -> Result<GhPrStatus, ApiError> {
        info!(repo = %repo, number, "github: checking PR status");
        if self.has_token() {
            let (owner, name) = parse_repo(repo)?;
            self.get_pr_status_graphql(owner, name, number).await
        } else {
            self.get_pr_status_cli(repo, number).await
        }
    }

    // -- GraphQL backend ----------------------------------------------------

    async fn graphql(
        &self,
        query: &str,
        variables: serde_json::Value,
    ) -> Result<serde_json::Value, ApiError> {
        let token = self
            .token
            .as_deref()
            .ok_or_else(|| ApiError::internal("graphql called without GITHUB_TOKEN"))?;

        let resp = self
            .http
            .post("https://api.github.com/graphql")
            .bearer_auth(token)
            .header("User-Agent", "crabitat-control-plane")
            .json(&serde_json::json!({ "query": query, "variables": variables }))
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("GitHub API request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::internal(format!("GitHub API returned {status}: {body}")));
        }

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("failed to parse GitHub response: {e}")))?;

        if let Some(errors) = body.get("errors") {
            return Err(ApiError::internal(format!("GitHub GraphQL errors: {errors}")));
        }

        Ok(body)
    }

    async fn list_issues_graphql(&self, owner: &str, repo: &str) -> Result<Vec<GhIssue>, ApiError> {
        info!(owner, repo, "github/graphql: listing issues");
        let query = r#"
            query($owner: String!, $repo: String!) {
                repository(owner: $owner, name: $repo) {
                    issues(first: 50, states: OPEN, orderBy: {field: CREATED_AT, direction: DESC}) {
                        nodes {
                            number
                            title
                            body
                            labels(first: 10) { nodes { name } }
                            state
                        }
                    }
                }
            }
        "#;

        let body = self.graphql(query, serde_json::json!({ "owner": owner, "repo": repo })).await?;

        let nodes = body
            .pointer("/data/repository/issues/nodes")
            .ok_or_else(|| ApiError::internal("unexpected GitHub response structure"))?;

        let gql_issues: Vec<GqlIssue> = serde_json::from_value(nodes.clone())
            .map_err(|e| ApiError::internal(format!("failed to parse issues: {e}")))?;

        Ok(gql_issues
            .into_iter()
            .map(|i| GhIssue {
                number: i.number,
                title: i.title,
                body: i.body,
                labels: i.labels.nodes.into_iter().map(|l| l.name).collect(),
                state: i.state,
            })
            .collect())
    }

    async fn get_issue_graphql(
        &self,
        owner: &str,
        repo: &str,
        number: i64,
    ) -> Result<GhIssueDetail, ApiError> {
        info!(owner, repo, number, "github/graphql: fetching issue");
        let query = r#"
            query($owner: String!, $repo: String!, $number: Int!) {
                repository(owner: $owner, name: $repo) {
                    issue(number: $number) {
                        title
                        body
                    }
                }
            }
        "#;

        let body = self
            .graphql(query, serde_json::json!({ "owner": owner, "repo": repo, "number": number }))
            .await?;

        let issue = body
            .pointer("/data/repository/issue")
            .ok_or_else(|| ApiError::internal("issue not found in GitHub response"))?;

        let d: GqlIssueDetail = serde_json::from_value(issue.clone())
            .map_err(|e| ApiError::internal(format!("failed to parse issue: {e}")))?;

        Ok(GhIssueDetail { title: d.title, body: d.body })
    }

    async fn get_pr_status_graphql(
        &self,
        owner: &str,
        repo: &str,
        number: i64,
    ) -> Result<GhPrStatus, ApiError> {
        info!(owner, repo, number, "github/graphql: checking PR status");
        let query = r#"
            query($owner: String!, $repo: String!, $number: Int!) {
                repository(owner: $owner, name: $repo) {
                    pullRequest(number: $number) {
                        state
                        mergedAt
                    }
                }
            }
        "#;

        let body = self
            .graphql(query, serde_json::json!({ "owner": owner, "repo": repo, "number": number }))
            .await?;

        let pr = body
            .pointer("/data/repository/pullRequest")
            .ok_or_else(|| ApiError::internal("PR not found in GitHub response"))?;

        let s: GqlPrStatus = serde_json::from_value(pr.clone())
            .map_err(|e| ApiError::internal(format!("failed to parse PR status: {e}")))?;

        Ok(GhPrStatus { state: s.state, merged_at: s.merged_at })
    }

    // -- gh CLI backend -----------------------------------------------------

    async fn list_issues_cli(&self, repo: &str) -> Result<Vec<GhIssue>, ApiError> {
        info!(repo, "github/cli: listing issues");
        let output = tokio::process::Command::new("gh")
            .args([
                "issue",
                "list",
                "--repo",
                repo,
                "--json",
                "number,title,body,labels,state",
                "--state",
                "open",
                "--limit",
                "50",
            ])
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("failed to run gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!("gh issue list failed: {stderr}")));
        }

        let issues: Vec<CliIssue> = serde_json::from_slice(&output.stdout)
            .map_err(|e| ApiError::internal(format!("failed to parse gh output: {e}")))?;

        Ok(issues
            .into_iter()
            .map(|i| GhIssue {
                number: i.number,
                title: i.title,
                body: i.body,
                labels: i.labels.into_iter().map(|l| l.name).collect(),
                state: i.state,
            })
            .collect())
    }

    async fn get_issue_cli(&self, repo: &str, number: i64) -> Result<GhIssueDetail, ApiError> {
        info!(repo, number, "github/cli: fetching issue");
        let output = tokio::process::Command::new("gh")
            .args(["issue", "view", &number.to_string(), "--repo", repo, "--json", "title,body"])
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("failed to run gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!("gh issue view failed: {stderr}")));
        }

        let d: CliIssueDetail = serde_json::from_slice(&output.stdout)
            .map_err(|e| ApiError::internal(format!("failed to parse gh output: {e}")))?;

        Ok(GhIssueDetail { title: d.title, body: d.body })
    }

    async fn get_pr_status_cli(&self, repo: &str, number: i64) -> Result<GhPrStatus, ApiError> {
        info!(repo, number, "github/cli: checking PR status");
        let output = tokio::process::Command::new("gh")
            .args(["pr", "view", &number.to_string(), "--repo", repo, "--json", "state,mergedAt"])
            .output()
            .await
            .map_err(|e| ApiError::internal(format!("failed to run gh: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::internal(format!("gh pr view failed: {stderr}")));
        }

        let s: CliPrStatus = serde_json::from_slice(&output.stdout)
            .map_err(|e| ApiError::internal(format!("failed to parse gh output: {e}")))?;

        Ok(GhPrStatus { state: s.state, merged_at: s.merged_at })
    }
}

pub(crate) fn parse_repo(repo: &str) -> Result<(&str, &str), ApiError> {
    repo.split_once('/').ok_or_else(|| ApiError::bad_request("repo must be in 'owner/repo' format"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn parse_repo_valid() {
        let (owner, repo) = parse_repo("owner/repo").unwrap();
        assert_eq!(owner, "owner");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_repo_invalid_no_slash() {
        let err = parse_repo("noslash").unwrap_err();
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn parse_repo_multiple_slashes() {
        let (owner, repo) = parse_repo("a/b/c").unwrap();
        assert_eq!(owner, "a");
        assert_eq!(repo, "b/c");
    }

    #[test]
    fn client_no_token() {
        let client = GitHubClient { http: reqwest::Client::new(), token: None };
        assert!(!client.has_token());
    }
}

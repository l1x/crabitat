use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command};

use reqwest::Client;
use serde_json::{Value, json};

/// RAII guard that kills the child process and cleans up temp DB files on drop.
struct ServerGuard {
    child: Child,
    db_path: PathBuf,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.db_path);
        let _ = std::fs::remove_file(self.db_path.with_extension("db-wal"));
        let _ = std::fs::remove_file(self.db_path.with_extension("db-shm"));
    }
}

/// Find an available port by binding to port 0 and reading the assigned port.
fn free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind to port 0");
    listener.local_addr().unwrap().port()
}

/// Spawn the control-plane binary with a temp DB and return the guard + base URL.
fn spawn_server() -> (ServerGuard, String) {
    let port = free_port();
    let addr = format!("127.0.0.1:{port}");
    let db_path = std::env::temp_dir().join(format!("crabitat-test-{port}.db"));

    let child = Command::new("cargo")
        .args(["run", "-p", "crabitat-control-plane"])
        .env("DATABASE_PATH", &db_path)
        .env("LISTEN_ADDR", &addr)
        .env("RUST_LOG", "crabitat_control_plane=info")
        .spawn()
        .expect("failed to spawn server");

    let base = format!("http://{addr}");
    let guard = ServerGuard { child, db_path };
    (guard, base)
}

/// Wait until the server responds to GET /v1/settings (handles build + startup time).
async fn wait_ready(client: &Client, base: &str) {
    let url = format!("{base}/v1/settings");
    for i in 0..60 {
        if client.get(&url).send().await.is_ok() {
            return;
        }
        if i % 10 == 0 && i > 0 {
            eprintln!("  still waiting for server... ({i}s)");
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    panic!("server did not become ready within 60 seconds");
}

#[tokio::test]
#[ignore] // requires `gh` CLI authenticated — run with: cargo test --test full_flow -- --ignored
async fn test_full_flow() {
    let (_guard, base) = spawn_server();
    let client = Client::new();
    wait_ready(&client, &base).await;

    // ── 1. Settings: set prompts_root ────────────────────────────────────
    let prompts_root = std::fs::canonicalize("../../.agent-prompts")
        .expect("cannot resolve .agent-prompts path")
        .to_string_lossy()
        .to_string();

    let resp = client
        .post(format!("{base}/v1/settings/prompts_root"))
        .json(&json!({"value": prompts_root}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let setting: Value = resp.json().await.unwrap();
    assert_eq!(setting["key"], "prompts_root");
    assert_eq!(setting["value"], prompts_root);

    // ── 2. Repos: register l1x/crabitat ──────────────────────────────────
    let resp = client
        .post(format!("{base}/v1/repos"))
        .json(&json!({
            "owner": "l1x",
            "name": "crabitat",
            "repo_url": "https://github.com/l1x/crabitat"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "repo create should return 201");
    let repo: Value = resp.json().await.unwrap();
    let repo_id = repo["repo_id"].as_str().expect("repo_id must be a string");
    assert_eq!(repo["owner"], "l1x");
    assert_eq!(repo["name"], "crabitat");

    // ── 3. Repos: list contains the repo ─────────────────────────────────
    let resp = client.get(format!("{base}/v1/repos")).send().await.unwrap();
    assert_eq!(resp.status(), 200);
    let repos: Vec<Value> = resp.json().await.unwrap();
    assert_eq!(repos.len(), 1);
    assert_eq!(repos[0]["repo_id"], repo_id);

    // ── 4. Repos: duplicate returns 409 ──────────────────────────────────
    let resp = client
        .post(format!("{base}/v1/repos"))
        .json(&json!({
            "owner": "l1x",
            "name": "crabitat",
            "repo_url": "https://github.com/l1x/crabitat"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409, "duplicate repo should return 409");

    // ── 5. Issues: refresh from GitHub ───────────────────────────────────
    let resp = client
        .post(format!("{base}/v1/repos/{repo_id}/issues/refresh"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "issue refresh should return 200");
    let issues: Vec<Value> = resp.json().await.unwrap();
    assert!(!issues.is_empty(), "repo should have at least one issue");

    // Pick the first issue for mission creation
    let issue_number = issues[0]["number"]
        .as_i64()
        .expect("issue number must be an integer");

    // ── 6. Issues: cached via GET ────────────────────────────────────────
    let resp = client
        .get(format!("{base}/v1/repos/{repo_id}/issues"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let cached_issues: Vec<Value> = resp.json().await.unwrap();
    assert!(
        !cached_issues.is_empty(),
        "cached issues should be non-empty"
    );

    // ── 7. Workflows: verify develop-feature exists ──────────────────────
    let resp = client
        .get(format!("{base}/v1/workflows"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let workflows: Vec<Value> = resp.json().await.unwrap();
    let has_develop_feature = workflows.iter().any(|w| w["name"] == "develop-feature");
    assert!(has_develop_feature, "develop-feature workflow must exist");

    // ── 8. Missions: create ──────────────────────────────────────────────
    let resp = client
        .post(format!("{base}/v1/missions"))
        .json(&json!({
            "repo_id": repo_id,
            "issue_number": issue_number,
            "workflow_name": "develop-feature"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201, "mission create should return 201");
    let mission: Value = resp.json().await.unwrap();
    let mission_id = mission["mission_id"]
        .as_str()
        .expect("mission_id must be a string");
    assert_eq!(mission["status"], "pending");
    assert_eq!(mission["branch"], format!("mission/issue-{issue_number}"));

    // ── 9. Mission detail: verify tasks ──────────────────────────────────
    let resp = client
        .get(format!("{base}/v1/missions/{mission_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let detail: Value = resp.json().await.unwrap();
    let tasks = detail["tasks"].as_array().expect("tasks must be an array");
    assert_eq!(tasks.len(), 2, "develop-feature should have 2 tasks");

    let step_ids: Vec<&str> = tasks
        .iter()
        .map(|t| t["step_id"].as_str().unwrap())
        .collect();
    assert_eq!(step_ids, vec!["implement", "qa"]);

    for task in tasks {
        assert_eq!(task["status"], "queued", "tasks should start as queued");
    }

    // Verify task prompts contain the issue title
    let issue_title = issues[0]["title"].as_str().unwrap_or("");
    if !issue_title.is_empty() {
        let implement_prompt = tasks[0]["assembled_prompt"].as_str().unwrap_or("");
        assert!(
            implement_prompt.contains(issue_title),
            "implement task prompt should contain issue title '{issue_title}'"
        );
    }

    // ── 10. Tasks: /tasks/next returns the implement task ────────────────
    let resp = client
        .get(format!("{base}/v1/tasks/next"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let next_task: Value = resp.json().await.unwrap();
    assert_eq!(next_task["task"]["step_id"], "implement");
    assert_eq!(
        next_task["git"]["branch"],
        format!("mission/issue-{issue_number}")
    );
    assert!(
        next_task["git"]["repo_url"].as_str().is_some(),
        "git.repo_url should be present"
    );

    // ── 11. Repo missions: mission listed under repo ─────────────────────
    let resp = client
        .get(format!("{base}/v1/repos/{repo_id}/missions"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let repo_missions: Vec<Value> = resp.json().await.unwrap();
    assert!(
        repo_missions.iter().any(|m| m["mission_id"] == mission_id),
        "mission should be listed under the repo"
    );

    eprintln!("All assertions passed!");
}

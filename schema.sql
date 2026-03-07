CREATE TABLE IF NOT EXISTS repos (
    repo_id    TEXT PRIMARY KEY,
    owner      TEXT NOT NULL,
    name       TEXT NOT NULL,
    local_path TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(owner, name)
);

CREATE TABLE IF NOT EXISTS github_issues_cache (
    repo_id    TEXT NOT NULL REFERENCES repos(repo_id) ON DELETE CASCADE,
    number     INTEGER NOT NULL,
    title      TEXT NOT NULL,
    body       TEXT,
    labels     TEXT NOT NULL DEFAULT '[]',
    state      TEXT NOT NULL DEFAULT 'open',
    fetched_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (repo_id, number)
);

CREATE TABLE IF NOT EXISTS workflow_flavors (
    flavor_id     TEXT PRIMARY KEY,
    workflow_name TEXT NOT NULL,
    name          TEXT NOT NULL,
    prompt_paths  TEXT NOT NULL DEFAULT '[]',
    UNIQUE(workflow_name, name)
);

CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Execution Layer (FR-4)

CREATE TABLE IF NOT EXISTS missions (
    mission_id    TEXT PRIMARY KEY,
    repo_id       TEXT NOT NULL REFERENCES repos(repo_id) ON DELETE CASCADE,
    issue_number  INTEGER NOT NULL,
    workflow_name TEXT NOT NULL,
    flavor_id     TEXT REFERENCES workflow_flavors(flavor_id) ON DELETE SET NULL,
    status        TEXT NOT NULL DEFAULT 'pending', -- pending, running, completed, failed
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY (repo_id, issue_number) REFERENCES github_issues_cache(repo_id, number)
);

CREATE TABLE IF NOT EXISTS tasks (
    task_id          TEXT PRIMARY KEY,
    mission_id       TEXT NOT NULL REFERENCES missions(mission_id) ON DELETE CASCADE,
    step_id          TEXT NOT NULL,
    step_order       INTEGER NOT NULL,
    assembled_prompt TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'queued', -- queued, running, completed, failed
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS runs (
    run_id      TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES tasks(task_id) ON DELETE CASCADE,
    status      TEXT NOT NULL, -- running, completed, failed
    logs        TEXT,
    summary     TEXT,
    started_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    finished_at TEXT
);

CREATE TABLE IF NOT EXISTS repos (
    repo_id    TEXT PRIMARY KEY,
    owner      TEXT NOT NULL,
    name       TEXT NOT NULL,
    local_path TEXT,
    repo_url   TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT,
    deleted_at TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS repos_owner_name_uniq
    ON repos(owner, name) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS github_issues_cache (
    repo_id    TEXT NOT NULL REFERENCES repos(repo_id),
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
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT,
    deleted_at    TEXT
);

CREATE UNIQUE INDEX IF NOT EXISTS workflow_flavors_name_uniq
    ON workflow_flavors(workflow_name, name) WHERE deleted_at IS NULL;

CREATE TABLE IF NOT EXISTS settings (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at TEXT
);

CREATE TABLE IF NOT EXISTS environment_paths (
    environment   TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_name TEXT NOT NULL,
    path          TEXT NOT NULL,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT,
    PRIMARY KEY (environment, resource_type, resource_name)
);

-- Execution Layer (FR-4)

CREATE TABLE IF NOT EXISTS missions (
    mission_id    TEXT PRIMARY KEY,
    repo_id       TEXT NOT NULL REFERENCES repos(repo_id),
    issue_number  INTEGER NOT NULL,
    workflow_name TEXT NOT NULL,
    flavor_id     TEXT REFERENCES workflow_flavors(flavor_id),
    status        TEXT NOT NULL DEFAULT 'pending',
    branch        TEXT,
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at    TEXT,
    repo_owner    TEXT,
    repo_name     TEXT,
    last_worker_id TEXT,
    FOREIGN KEY (repo_id, issue_number) REFERENCES github_issues_cache(repo_id, number)
);

CREATE TABLE IF NOT EXISTS tasks (
    task_id          TEXT PRIMARY KEY,
    mission_id       TEXT NOT NULL REFERENCES missions(mission_id),
    step_id          TEXT NOT NULL,
    step_order       INTEGER NOT NULL,
    assembled_prompt TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'queued',
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at       TEXT,
    retry_count      INTEGER DEFAULT 0,
    max_retries      INTEGER DEFAULT 3
);

CREATE TABLE IF NOT EXISTS runs (
    run_id      TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES tasks(task_id),
    status      TEXT NOT NULL,
    logs        TEXT,
    summary     TEXT,
    duration_ms INTEGER,
    tokens_used INTEGER,
    started_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    finished_at TEXT
);

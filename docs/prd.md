# Crabitat — Product Requirements Document

**Version:** 0.2.1
**Date:** 2026-03-07

---

## Overview

Crabitat is a local-first orchestration platform for autonomous coding agents. It manages workflows that turn GitHub issues into executable missions. The system consists of two components:

1. **Control-Plane** — Rust backend serving the REST API and managing the SQLite database.
2. **Console** — Web UI for operators to manage repos, issues, workflows, and missions.

---

## Core Concepts

1. **Repo** is onboarded — the user connects a GitHub repository to Crabitat.
2. **Issues** are loaded from that repo.
3. **Agent-prompts** is a local directory containing prompts and workflow TOML files. The path is stored in the `settings` table and can be configured via the Console.
4. **Workflows** are defined in `{prompts_root}/workflows/*.toml`. They are **global** (available to all repos) and **read-only** from the filesystem.
5. **Flavors** are user-defined combinations of additional prompt files (e.g., `rust` flavor includes `stacks/rust.md` and `pm/github-issue.md`). Flavors are persisted in the database and keyed by the workflow name.
6. **Mission** = Issue + Workflow (+ optional Flavor) → produces **tasks** → tasks produce **runs**.

---

## FR-0: System Configuration

- **FR-0.1**: Configure the `prompts_root` path. This is the directory where the control-plane looks for workflows and prompts.
- **FR-0.2**: The `prompts_root` is persisted in the `settings` table.

---

## FR-1: Repository Management


Operators onboard GitHub repositories so the system knows what to work on.

- **FR-1.1**: Create a repo by providing owner, name, and local filesystem path.
- **FR-1.2**: List all onboarded repos.
- **FR-1.3**: View a single repo's details.
- **FR-1.4**: Delete a repo and all its associated data (cascading).

---

## FR-2: Issue Loading

The system fetches and caches open issues from GitHub for onboarded repos.

- **FR-2.1**: Fetch open issues for a repo using the `gh` CLI.
- **FR-2.2**: Cache fetched issues in SQLite. No TTL — updated only on explicit refresh.
- **FR-2.3**: List issues for a repo, including number, title, body, labels, and state.

---

## FR-3: Workflows and Flavors

Workflows are templates for multi-step agent operations. Flavors allow these templates to be customized for specific tech stacks or contexts.

### FR-3.1: Global Workflow Registry
- Workflows are loaded from `{agent_prompts_root}/workflows/*.toml`.
- The control-plane acts as a registry, parsing these files on demand.
- **Workflow Format:**
  ```toml
  [workflow]
  name = "develop-feature"
  description = "Implement a feature with QA loop"

  [[steps]]
  id = "implement"
  prompt_file = "prompts/do/implement.md"

  [[steps]]
  id = "qa"
  prompt_file = "prompts/do/qa.md"
  depends_on = ["implement"]
  ```

### FR-3.2: Layered Prompt Flavors
- A flavor is a named collection of additional prompt files.
- Users create flavors in the database, mapping a `workflow_name` to a list of relative prompt paths (e.g., `["stacks/rust.md", "pm/github.md"]`).
- Flavors are available to be combined with their parent workflow when launching a mission.

---

## FR-4: Missions and Prompt Assembly

A mission is the execution of a workflow against a specific issue.

### FR-4.1: Mission Expansion
- Creating a mission expands a workflow's steps into persistent `tasks` in the database.
- Each task stores a fully "assembled" prompt.

### FR-4.2: Layered Prompt Assembly
When expanding a task, the prompt is assembled in three layers:
1. **Base Layer:** The content of the `prompt_file` defined in the workflow TOML step.
2. **Flavor Layer:** Concatenated content of all prompt files listed in the selected flavor.
3. **Issue Layer:** The title and body of the target GitHub issue, wrapped in XML tags.

**Assembly Template:**
```markdown
# Instructions
{{base_layer}}

# Context & Standards
{{flavor_layer}}

# Target Issue
<issue>
  <title>{{issue_title}}</title>
  <body>{{issue_body}}</body>
</issue>
```

---

## Data Model

```
FileSystem (.agent-prompts/)
  └── workflows/*.toml (Read-only source for Workflows)
  └── prompts/**/*.md (Source for Base and Flavor layers)

Database (SQLite)
  ├── repos (repo_id, owner, name, path)
  ├── workflow_flavors (flavor_id, workflow_name, name, prompt_paths_json)
  ├── github_issues_cache (repo_id, number, title, body, labels, state)
  ├── missions (mission_id, repo_id, issue_number, workflow_name, flavor_id, status)
  ├── tasks (task_id, mission_id, step_id, assembled_prompt, status)
  └── runs (run_id, task_id, status, logs, summary)
```

---

## Technical Constraints

- **Control-Plane:** Rust, Axum, SQLite (WAL mode).
- **Console:** Astro (SSR), Bun.
- **GitHub:** `gh` CLI for API operations.
- **Workflow Source:** Purely file-based; no DB storage for workflow steps.

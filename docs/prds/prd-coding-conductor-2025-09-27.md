# Coding Conductor PRD

## Overview

Develop an agent orchestrator, named **Conductor**, that operates as a TUI and IDE plugin. It will orchestrate tasks by selecting approved local small language models (SLMs) via a simple TOML configuration, execute them inside a Docker container, and provide developers with an interactive interface to request code implementations, refactorings, or documentation.

## Goals

1. Enable developers to issue a single command or IDE action and have the conductor plan and execute a complete coding task (e.g., scaffolding a web page, writing a backend endpoint, running an ETL job).
2. Support configuration of multiple local SLMs through a TOML file, allowing the conductor to choose the most suitable model for a given task.
3. Provide a TUI that displays execution progress, logs, and results in real time.
4. Integrate with a secret store to retrieve API keys for external services when required.
5. Deliver a reproducible Docker‑based environment that isolates task execution.

## User Stories

- As a developer, I want to run `conductor run --task implement-login` so that the agent creates a new authentication endpoint, generates unit tests, and updates documentation, all without leaving my IDE.
- As a developer, I want to configure which SLM to use for a given task via a `conductor.toml` file, so that I can experiment with different model capabilities.
- As a developer, I want to see a live TUI that shows step‑by‑step execution, logs, and final artifacts, so that I can monitor progress and intervene if needed.
- As a developer, I want my API keys to be stored securely in a secret store and automatically injected into the container when the task requires them, so that I do not hard‑code credentials.

## Functional Requirements

1. **FR-1:** The conductor must accept a task identifier, repository path, and optional context as input and produce a Docker‑based execution plan.
   - Acceptance: The input is parsed correctly and a Docker command is generated with the provided parameters.
2. **FR-2:** The conductor must read a `conductor.toml` configuration file that lists approved SLMs and their configuration parameters.
   - Acceptance: The configuration file is parsed, and the conductor can select a model based on task tags or heuristics.
3. **FR-3:** The conductor must launch a Docker container, mount the repository, and pass the task and context as environment variables or files.
   - Acceptance: The container starts, the task runs, and exits with a success status when completed.
4. **FR-4:** The conductor must provide a TUI that streams logs, shows progress bars, and allows user interruption.
   - Acceptance: The TUI updates in real time and responds to Ctrl‑C within 2 seconds.
5. **FR-5:** The conductor must integrate with a secret store (e.g., `keyring` or environment‑variable vault) to retrieve API keys and inject them into the container securely.
   - Acceptance: Secrets are fetched without exposing them in logs or command lines.
6. **FR-6:** The conductor must produce output artifacts (e.g., modified files, generated tests, documentation) in the original repository upon successful completion.
   - Acceptance: Files are written back to the repository with appropriate permissions.

## Non‑functional Requirements

1. **NFR-1:** The conductor must be implemented in Rust 2024 edition, using Tokio as the async runtime.
   - Acceptance: Cargo builds without errors and passes `cargo test` for all unit tests.
2. **NFR-2:** The user interface must be a terminal‑based TUI built with `ratatui` or similar crate.
   - Acceptance: The TUI renders correctly on macOS, Linux, and Windows terminals.
3. **NFR-3:** The conductor must support Docker version 20.10+ and fail gracefully if Docker is unavailable.
   - Acceptance: A clear error message is displayed when Docker is not running.
4. **NFR-4:** All external API keys must be stored in a secret store and never written to disk in plaintext.
   - Acceptance: The secret store API is used, and no key appears in repository history.
5. **NFR-5:** The conductor must have at least 80% code coverage for unit and integration tests.
   - Acceptance: `cargo tarpaulin` reports ≥80% coverage.

## Non‑Goals

- Implement a full‑featured IDE integration for all editors (VS Code, JetBrains, etc.) – only a minimal VS Code plugin will be prototyped.
- Support execution of non‑deterministic AI‑generated code without human review.
- Provide extensive natural‑language conversation capabilities beyond the current TUI command flow.
- Replace existing CI/CD pipelines; the conductor is an auxiliary tool, not a replacement for manual review.

## Success Metrics

- **Adoption Rate:** At least 30% of active developers on the repo use the conductor weekly within the first 3 months.
- **Task Completion Rate:** 80% of tasks launched via the conductor finish successfully (exit code 0) without manual intervention.
- **Latency:** Average time from task submission to completion should be under 2 minutes for typical small tasks.
- **Error Reduction:** Number of merge‑conflict incidents related to auto‑generated code should decrease by 15% compared to baseline.
- **User Satisfaction:** Post‑implementation survey scores of ≥4.0/5 for ease of use and usefulness.

## Design Considerations

- The TUI should use a clean, color‑coded layout: command status, progress bar, and output log.
- Configuration file (`conductor.toml`) should be human‑editable and support comments.
- Logging should be structured and limited to INFO level unless debug mode is enabled.

## Technical Constraints

- Runtime: Rust 1.75+, Tokio, crossterm.
- Docker API: Docker must be accessible via the local daemon; no remote Docker services.
- Secret Store: Use `secret_backend` crate with support for macOS Keychain, Linux Secret Service, and Windows Credential Locker.
- Model Selection: Based on a `task.tags` field in TOML, the conductor picks a model from the `[models]` section.

## Open Questions

1. Which Docker networking mode should be used to share environment variables securely?
2. How should the conductor handle long‑running tasks that exceed typical TUI session lifetimes?
3. What is the preferred method for passing large context files (e.g., >10 MB) into the container?
4. Should the conductor support multiple concurrent task executions, or only a single queue?

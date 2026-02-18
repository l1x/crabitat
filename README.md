# Crabitat Workspace

Crabitat is organized as a Rust workspace with a SQLite-backed control-plane and agent skeleton binaries.

## Workspace Layout

- `crates/crabitat-core`: shared domain types (`Mission`, `Task`, `Run`, IDs, metrics).
- `crates/crabitat-protocol`: message envelope and protocol payloads.
- `crates/crabitat-control-plane`: HTTP API service with SQLite state persistence.
- `crates/crabitat-chief`: chief orchestration runtime skeleton.
- `crates/crabitat-crab`: crab executor runtime skeleton.
- `apps/crabitat-console`: Astro-based operations console (in progress).

## Control-plane API (skeleton)

- `GET /healthz`
- `POST /v1/crabs/register`
- `GET /v1/crabs`
- `POST /v1/missions`
- `GET /v1/missions`
- `POST /v1/tasks`
- `GET /v1/tasks`
- `POST /v1/runs/start`
- `POST /v1/runs/update`
- `POST /v1/runs/complete`
- `GET /v1/status`

State is persisted in SQLite (default path: `./var/crabitat-control-plane.db`).

## Build and Run (mise)

Use `mise` tasks for build and verification:

```bash
mise run check
mise run build
mise run test
mise run verify
```

Run core services:

```bash
mise run run-control-plane
mise run run-chief
mise run run-crab
```

# Crabitat

Crabitat is a colony-based orchestration platform for AI coding agents. It coordinates
multiple **Crabs** (executor agents) under a **Chief** (orchestrator), organized into
**Colonies** (projects), tracking **Missions**, **Tasks**, and **Runs** through a
SQLite-backed control-plane.

## Workflow

![Colony Workflow](docs/illustrations/colony-workflow.svg)

## Colony Terminology

| Term       | Description                                                  |
|------------|--------------------------------------------------------------|
| **Colony** | Top-level project grouping — one Chief, many Crabs           |
| **Chief**  | Orchestrator that decomposes missions into tasks             |
| **Crab**   | Executor agent that picks up and runs tasks                  |
| **Mission**| High-level objective submitted to a colony                   |
| **Task**   | Discrete unit of work derived from a mission                 |
| **Run**    | Single execution attempt of a task by a crab                 |
| **Burrow** | Isolated workspace (worktree or external repo) for a run     |

## Quickstart

Prerequisites: [mise](https://mise.jdx.dev/), Rust 1.90+, Node 24+, [bun](https://bun.sh/)

```bash
mise install
mise run build
```

## Complete Onboarding

### 1. Start the control-plane

```bash
mise run run-control-plane
```

The control-plane starts on `http://127.0.0.1:8800` with a SQLite database
at `./var/crabitat-control-plane.db`.

Verify it's running:

```bash
curl -s http://127.0.0.1:8800/healthz
# {"ok":true}
```

### 2. Start the console

In a second terminal:

```bash
mise run console-install   # first time only
mise run console-start     # builds and serves on port 4321
```

Open `http://localhost:4321` to see the colony dashboard.

### 3. Create a colony

```bash
curl -s -X POST http://127.0.0.1:8800/v1/colonies \
  -H 'Content-Type: application/json' \
  -d '{"name":"my-project","description":"Building feature X"}'
```

Save the `colony_id` from the response — you'll need it for the next steps.

### 4. Start a crab agent

In a third terminal, run the crab binary. It registers itself, connects via
WebSocket, and waits for task assignments:

```bash
cargo run -p crabitat-crab -- connect \
  --control-plane http://127.0.0.1:8800 \
  --colony-id <colony_id> \
  --name Alice \
  --role coder \
  --repo .
```

The crab appears as **idle** in the console. You can start multiple crabs
in separate terminals with different names and roles.

### 5. Create a mission and assign a task

```bash
# Create a mission
curl -s -X POST http://127.0.0.1:8800/v1/missions \
  -H 'Content-Type: application/json' \
  -d '{"colony_id":"<colony_id>","prompt":"Implement feature X"}'

# Create a task and assign it to the crab (use crab_id from step 4 logs)
curl -s -X POST http://127.0.0.1:8800/v1/tasks \
  -H 'Content-Type: application/json' \
  -d '{"mission_id":"<mission_id>","title":"Write the implementation","assigned_crab_id":"<crab_id>"}'
```

The control-plane pushes a `TaskAssigned` message over WebSocket. The crab
automatically:

1. Creates a git worktree in `burrows/<task_id_short>/`
2. Writes a `CLAUDE.md` system prompt into the worktree
3. Starts a run via the control-plane API
4. Spawns `claude -p "<task title>"` inside the worktree
5. Reports results back via `POST /v1/runs/complete`
6. Cleans up the worktree

The console updates to show run progress and completion.

## Workspace Layout

```
crates/
  crabitat-core/           Shared domain types (Colony, Mission, Task, Run, IDs, metrics)
  crabitat-protocol/       Message envelope and protocol payloads
  crabitat-control-plane/  HTTP API + SQLite persistence + WebSocket task dispatch
  crabitat-chief/          Chief orchestration runtime (skeleton)
  crabitat-crab/           Crab agent runtime (WebSocket + Claude Code spawner)
apps/
  crabitat-console/        Astro SSR operations console
scripts/
  onboard-crab.sh          Register a crab via the control-plane API
```

## Control-plane API

Base URL: `http://127.0.0.1:8800` (default)

| Method | Endpoint              | Description                   |
|--------|-----------------------|-------------------------------|
| GET    | /healthz              | Health check                  |
| POST   | /v1/colonies          | Create a colony               |
| GET    | /v1/colonies          | List all colonies             |
| POST   | /v1/crabs/register    | Register or update a crab     |
| GET    | /v1/crabs             | List all crabs                |
| POST   | /v1/missions          | Create a mission              |
| GET    | /v1/missions          | List all missions             |
| POST   | /v1/tasks             | Create a task                 |
| GET    | /v1/tasks             | List all tasks                |
| POST   | /v1/runs/start        | Start a run                   |
| POST   | /v1/runs/update       | Update a run's progress       |
| POST   | /v1/runs/complete     | Complete a run                |
| GET    | /v1/status            | Full status snapshot          |
| GET    | /v1/ws/crab/{crab_id} | WebSocket for crab agent      |

State is persisted in SQLite (default: `./var/crabitat-control-plane.db`).

## mise Commands

| Command                      | Description                              |
|------------------------------|------------------------------------------|
| `mise run fmt`               | Format all Rust code                     |
| `mise run check`             | Typecheck the workspace                  |
| `mise run clippy`            | Lint the workspace                       |
| `mise run test`              | Run all tests                            |
| `mise run build`             | Build all workspace members              |
| `mise run verify`            | Run fmt + clippy + test                  |
| `mise run run-control-plane` | Start the control-plane on port 8800     |
| `mise run run-chief`         | Start the chief in watch mode            |
| `mise run run-crab`          | Start a crab agent (set COLONY_ID env)   |
| `mise run console-install`   | Install Astro console dependencies       |
| `mise run console-dev`       | Run console in dev mode (port 4321)      |
| `mise run console-build`     | Build the console for production         |
| `mise run console-start`     | Build and serve console (port 4321)      |

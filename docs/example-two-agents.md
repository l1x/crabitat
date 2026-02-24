# Example: Two Agents Working on the Same Repo

This walkthrough shows two Claude Code agents collaborating through the workflow engine on a single crabitat repo.

## Prerequisites

```bash
# Build everything
cargo build

# Have agent-prompts repo at ~/code/home/projectz/agent-prompts
# with workflows/dev-task.toml and do/*.md prompt files
```

## Setup

### Terminal 1: Start the control-plane

```bash
crabitat-control-plane serve \
  --port 8800 \
  --db-path ./var/crabitat-control-plane.db \
  --prompts-path ~/code/home/projectz/agent-prompts
```

You should see:
```
loaded workflow name=dev-task steps=5
workflow registry loaded count=1
control-plane listening on http://0.0.0.0:8800
```

### Terminal 2: Create a colony

```bash
curl -s -X POST http://localhost:8800/v1/colonies \
  -H 'Content-Type: application/json' \
  -d '{"name": "crabitat-dev", "description": "Crabitat development colony"}' | jq .
```

Save the `colony_id` from the response. We'll call it `$COLONY_ID`.

### Verify workflows are loaded

```bash
curl -s http://localhost:8800/v1/workflows | jq .
# Should return: ["dev-task"]
```

## Launch Agent 1

### Terminal 3: Open a Claude Code session

Paste this onboarding prompt (replace `$COLONY_ID`):

```
You are a Crabitat crab agent.

Register with the colony, then poll for tasks in a loop.

1. Register:
   crabitat-crab register --colony-id $COLONY_ID --name "Atlas"

2. Save your crab_id, then poll every 5 seconds:
   crabitat-crab poll --crab-id <YOUR_CRAB_ID>

3. When you get a task, read its prompt field for instructions.
   Start a run, do the work, then complete the run.
   For detailed instructions see: docs/onboarding-prompt.md

Work in the worktree path specified by the task. Never stop polling.
```

## Launch Agent 2

### Terminal 4: Open another Claude Code session

```
You are a Crabitat crab agent.

Register with the colony, then poll for tasks in a loop.

1. Register:
   crabitat-crab register --colony-id $COLONY_ID --name "Coral"

2. Save your crab_id, then poll every 5 seconds:
   crabitat-crab poll --crab-id <YOUR_CRAB_ID>

3. When you get a task, read its prompt field for instructions.
   Complete the run with --result PASS or --result FAIL for review tasks.
   For detailed instructions see: docs/onboarding-prompt.md

Never stop polling.
```

Both agents are interchangeable — the scheduler assigns queued tasks to whichever crab is idle first. The `prompt_file` on each workflow step determines what the agent does for that step.

## Create a Workflow Mission

### Terminal 2: Submit the mission

```bash
curl -s -X POST http://localhost:8800/v1/missions \
  -H 'Content-Type: application/json' \
  -d '{
    "colony_id": "'$COLONY_ID'",
    "prompt": "Add a GET /v1/missions/:id endpoint that returns a single mission by ID. Include proper 404 handling.",
    "workflow": "dev-task"
  }' | jq .
```

Save the `mission_id`. This creates 5 tasks:

```
[plan]       → Queued  (no deps)
[implement]  → Blocked (depends on plan)
[review]     → Blocked (depends on implement)
[fix]        → Blocked (depends on review, condition: review.result == 'FAIL')
[pr]         → Blocked (depends on review, condition: review.result == 'PASS')
```

### Verify tasks were created

```bash
curl -s http://localhost:8800/v1/tasks | jq '.[] | {task_id, title: .title, status, step_id}'
```

## Trigger the Scheduler

The scheduler matches queued tasks to idle crabs:

```bash
curl -s -X POST http://localhost:8800/v1/scheduler/tick | jq .
```

### What happens

1. **plan** task is `Queued`, Atlas is idle → scheduler assigns plan to Atlas
2. Atlas receives a `TaskAssigned` WebSocket message with the rendered planning prompt

```bash
curl -s -X POST http://localhost:8800/v1/scheduler/tick | jq .
# {"ok": true, "assigned": 1}
```

## The Workflow Unfolds

### Step 1: Plan (Atlas)

Atlas receives the plan task. His prompt instructs him to:
- Read the mission prompt
- Explore the codebase
- Output an implementation plan

He starts a run, does the work, and completes:

```bash
crabitat-crab complete-run --run-id <RUN_ID> --status completed \
  --summary "Plan: Add handler get_mission_by_id at /v1/missions/:id, modify build_router, add fetch helper, add 404 test"
```

### Cascade: implement becomes Queued

After Atlas completes the plan, `cascade_workflow` runs:
- `implement` depends on `plan` (now completed) → becomes `Queued`
- All other tasks stay `Blocked`

Trigger the scheduler to assign implement:

```bash
curl -s -X POST http://localhost:8800/v1/scheduler/tick | jq .
```

### Step 2: Implement (whichever crab is idle)

The idle crab picks up the implement task. The prompt includes the plan summary as context. The agent implements the endpoint and completes:

```bash
crabitat-crab complete-run --run-id <RUN_ID> --status completed \
  --summary "Added get_mission_by_id handler, route, and test. cargo test passes."
```

### Cascade: review becomes Queued

Trigger scheduler — the next idle crab gets the review task:

```bash
curl -s -X POST http://localhost:8800/v1/scheduler/tick | jq .
```

### Step 3: Review

The assigned crab receives the review task. The prompt instructs it to review the diff and output a JSON verdict. Context includes both the plan and implementation summaries.

**If the code looks good:**

```bash
crabitat-crab complete-run --run-id <RUN_ID> --status completed \
  --summary '{"result": "PASS", "summary": "Implementation looks correct, tests cover the happy path and 404 case."}' \
  --result PASS
```

Cascade:
- `pr` has condition `review.result == 'PASS'` → **Queued**
- `fix` has condition `review.result == 'FAIL'` → **Skipped**

**If the code needs work:**

```bash
crabitat-crab complete-run --run-id <RUN_ID> --status completed \
  --summary '{"result": "FAIL", "summary": "Missing input validation on mission_id format.", "issues": ["No UUID format check"]}' \
  --result FAIL
```

Cascade:
- `fix` has condition `review.result == 'FAIL'` → **Queued**
- `pr` stays `Blocked` (condition not met, but not skipped because review might pass next time)

### Step 4a: Fix -- only if review FAIL

The idle crab picks up the fix task. Context includes the review comments. It fixes the issues and completes.

After fix completes, the engine automatically re-queues `review`. The cycle repeats until review passes (max 3 retries).

### Step 4b: PR -- only if review PASS

The idle crab picks up the PR task and creates a PR using `gh pr create`.

### Mission Complete

Once all tasks are terminal (completed/skipped), the mission status changes to `Completed`.

```bash
curl -s http://localhost:8800/v1/status | jq '.missions[] | {mission_id, status, workflow_name}'
# {"mission_id": "...", "status": "completed", "workflow_name": "dev-task"}
```

## Monitoring

### Watch status in real time

```bash
# Full snapshot
curl -s http://localhost:8800/v1/status | jq .

# Just tasks
curl -s http://localhost:8800/v1/tasks | jq '.[] | {title, status, step_id, assigned_crab_id}'
```

### WebSocket console

Connect to `ws://localhost:8800/v1/ws/console` for live events (task created, updated, run completed, etc). The Astro console at `apps/crabitat-console/` renders these in real time.

## Variations

### Scale up with more crabs

Register additional crabs to handle more concurrent missions:

```bash
crabitat-crab register --colony-id $COLONY_ID --name "Extra1"
crabitat-crab register --colony-id $COLONY_ID --name "Extra2"
```

All crabs are interchangeable. The scheduler assigns queued tasks to idle crabs in FIFO order. More crabs means more missions can run in parallel.

### One agent does everything

Register a single crab:

```bash
crabitat-crab register --colony-id $COLONY_ID --name "Solo"
```

All steps are assigned to Solo sequentially. Useful for testing but defeats the purpose of multi-agent collaboration.

### Multiple concurrent missions

Submit two missions at the same time:

```bash
curl -s -X POST http://localhost:8800/v1/missions \
  -d '{"colony_id": "'$COLONY_ID'", "prompt": "Add GET /v1/missions/:id", "workflow": "dev-task"}'

curl -s -X POST http://localhost:8800/v1/missions \
  -d '{"colony_id": "'$COLONY_ID'", "prompt": "Add pagination to GET /v1/tasks", "workflow": "dev-task"}'
```

Each mission gets its own worktree (`burrows/mission-<id1>/`, `burrows/mission-<id2>/`). The scheduler can run steps from different missions in parallel on different crabs, but enforces one running step per mission to avoid worktree conflicts.

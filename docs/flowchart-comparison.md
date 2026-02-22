# Flowchart Comparison: agents/ worktree-task vs crabitat

## agents/ worktree-task (current)

Everything runs in **one Claude Code session** on **one machine**.

```
┌─────────────────────────────────────────────────────────────────┐
│  Claude Code Session (Supervisor)                               │
│                                                                 │
│  1. Load issue via /github-pm                                   │
│  2. Create worktree: .worktrees/issue-42-slug/                  │
│  3. Spawn subagent ──► planner (read-only)                      │
│     │                   - explores worktree                     │
│     │                   - reads AGENTS.md                       │
│     │                   - outputs implementation prompt          │
│     ◄── returns plan ──┘                                        │
│  4. Spawn subagent ──► worker (read/write)                      │
│     │                   - implements in worktree                 │
│     │                   - runs quality gates                     │
│     │                   - commits to branch                      │
│     ◄── returns summary ┘                                       │
│  5. Spawn subagent ──► reviewer (read-only)                     │
│     │                   - reads diff in worktree                 │
│     │                   - outputs PASS/FAIL                      │
│     ◄── returns review ─┘                                       │
│  6. If FAIL → go to 4 (max 3x)                                 │
│  7. git push + gh pr create                                     │
│  8. Cleanup worktree (on user approval)                         │
│                                                                 │
│  State: in supervisor's context window                          │
│  Isolation: git worktree                                        │
│  Parallelism: run multiple terminals manually                   │
└─────────────────────────────────────────────────────────────────┘
```

**Key properties:**
- Supervisor holds all state in memory (context window)
- Subagents start with zero context — supervisor constructs their prompt
- All subagents share the same worktree on disk
- Flow control is imperative (if/else in SKILL.md)
- Single machine, single repo clone
- Parallelism = user opens N terminals

---

## crabitat

Orchestration runs in the **control-plane** (Rust server). Agents are **independent sessions** on **one machine**, each sharing a per-mission worktree.

```
┌──────────────────────────────────────────────────────────────────────┐
│  Console UI / API                                                    │
│                                                                      │
│  User: "run dev-task on issues/6"                                    │
│    └──► POST /v1/missions {workflow: "dev-task", input_ref: "6"}     │
└──────────┬───────────────────────────────────────────────────────────┘
           │
           ▼
┌──────────────────────────────────────────────────────────────────────┐
│  Control-Plane (single machine)                                      │
│                                                                      │
│  1. Load workflow manifest (workflows/dev-task.toml)                 │
│  2. Fetch issue content (gh issue view 6)                            │
│  3. Create Mission + worktree: burrows/mission-<id>/                 │
│  4. Create Tasks from workflow steps:                                │
│                                                                      │
│     ┌──────────┐     ┌───────────┐     ┌──────────┐     ┌────────┐  │
│     │  plan    │────►│ implement │────►│  review  │────►│   pr   │  │
│     │ (planner)│     │ (worker)  │     │(reviewer)│  ┌─►│ (any)  │  │
│     └──────────┘     └───────────┘     └─────┬────┘  │  └────────┘  │
│                                              │       │              │
│                                         FAIL │  PASS │              │
│                                              ▼       │              │
│                                        ┌─────────┐   │              │
│                                        │   fix   │───┘              │
│                                        │(worker) │                  │
│                                        │ max 3x  │                  │
│                                        └─────────┘                  │
│                                                                      │
│  5. Scheduler: assign each task to an idle crab matching role        │
│     All tasks in this mission share burrows/mission-<id>/            │
│                                                                      │
│  State: SQLite database                                              │
│  Live updates: WebSocket → Console                                   │
└──────────┬──────────────────────────┬──────────────────┬─────────────┘
           │                          │                  │
           ▼                          ▼                  ▼
  ┌────────────────┐  ┌────────────────────┐  ┌──────────────────────┐
  │ Crab A         │  │ Crab B             │  │ Crab C               │
  │ role: planner  │  │ role: worker       │  │ role: reviewer       │
  │ (Claude Code)  │  │ (Gemini CLI)       │  │ (Claude Code)        │
  │                │  │                    │  │                      │
  │ polls → gets   │  │ polls → gets       │  │ polls → gets         │
  │ plan task      │  │ implement task     │  │ review task          │
  │                │  │                    │  │                      │
  │ works in:      │  │ works in:          │  │ works in:            │
  │ burrows/       │  │ burrows/           │  │ burrows/             │
  │  mission-<id>/ │  │  mission-<id>/     │  │  mission-<id>/       │
  │                │  │                    │  │                      │
  │ complete-run   │  │ complete-run       │  │ complete-run         │
  │ → idle         │  │ → idle             │  │ → idle               │
  │ → picks up     │  │ → picks up         │  │ → picks up           │
  │   next mission │  │   next mission     │  │   next mission       │
  └────────────────┘  └────────────────────┘  └──────────────────────┘
```

---

## Worktree Model

```
1 agent, 1 machine, 1 worktree:

  Crab A ──► burrows/mission-7/     (plan → implement → review → pr)

N agents, 1 machine, N worktrees:

  Crab A ──► burrows/mission-7/     (working on issue #6)
  Crab B ──► burrows/mission-8/     (working on issue #12)
  Crab C ──► burrows/mission-9/     (working on issue #15)

Within a mission, steps are sequential (depends_on).
Across missions, crabs work in parallel in separate worktrees.
```

---

## Side-by-Side Comparison

```
                agents/ worktree-task          crabitat
                ─────────────────────          ────────
Orchestrator    Supervisor (Claude Code)       Control-plane (Rust server)
State store     Context window (volatile)      SQLite (persistent, survives crashes)
Flow control    Imperative (if/else in MD)     Declarative (TOML: depends_on, condition)
Agents          Subagents (same process)       Independent sessions (any runtime)
Runtimes        Claude Code only               Claude Code, Gemini CLI, OpenAI Codex, ...
Isolation       Git worktree (shared)          Worktree per mission
Context pass    Supervisor constructs prompt   Mission prompt + task description via API
Parallelism     Manual (N terminals)           Automatic (scheduler assigns to idle crabs)
Visibility      None (locked in session)       Console UI + WebSocket live updates
Recovery        Lost if session dies           Resume from DB state
Machine         Single                         Single (N worktrees on one machine)
```

---

## Implementation Status

All components are implemented and working.

```
Workflow manifest (TOML)          ✓ Implemented
  │
  ├── Workflow loader             ✓ WorkflowRegistry loads TOML at boot
  │     in control-plane             --prompts-path CLI arg
  │
  ├── Task dependency engine      ✓ cascade_workflow() resolves deps
  │     depends_on resolution        evaluate_condition() for branching
  │     conditional steps            fix/review retry loop (max 3x)
  │
  ├── Context forwarding          ✓ build_accumulated_context() collects
  │     step outputs become          run summaries from dependency chain
  │     next step inputs             injected via {{context}} template var
  │
  ├── Worktree management         ✓ burrows/mission-<id>/ per mission
  │     per mission                  scheduler enforces 1 running task
  │                                  per mission (worktree safety)
  │
  ├── Scheduler                   ✓ Two-pass role matching:
  │     role matching                1. exact role match first
  │                                  2. fallback to "any"
  │
  └── Role enforcement            ✓ One named role per colony
        per colony                   serialized registration (Mutex)
                                     "any" role allows unlimited crabs
```

See [workflow-engine.md](workflow-engine.md) for full architecture details.

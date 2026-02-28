# Crabitat v2 Implementation Gaps

Date: 2026-02-23  
Scope: PRD review against current code and the proposed polling-only transport model.

Compared artifacts:

- `docs/prds/prd-crabitat-v2.md`
- `docs/prds/prd-workflow-engine.md`
- `crates/crabitat-control-plane/src/main.rs`
- `crates/crabitat-crab/src/main.rs`
- `crates/crabitat-protocol/src/lib.rs`
- `apps/crabitat-console/src/pages/*`
- `apps/crabitat-console/src/components/*`
- `apps/crabitat-console/src/scripts/ws-client.ts`

## 1) Executive Summary

What is already in place:

- Repo CRUD exists.
- Role CRUD exists (including prompt file assignment and skills arrays).
- Workflow CRUD exists in DB.
- Mission queue from GitHub issues exists.
- Task DAG/cascade engine exists (dependencies, conditions, retry hooks).

Main blockers to align PRD + current build + polling-only proposal:

1. Workflow source of truth is split (DB workflows vs file-based runtime registry).
2. Agent protocol does not match PRD polling contract (no atomic next/claim endpoint).
3. Messaging model from PRD is not implemented (no `messages` table/API/inbox poll path).
4. UI execution views are still mostly mock data.
5. WS is still wired deeply in server + console + legacy crab runtime.

## 2) Gap Matrix (PRD vs Current vs Polling Proposal)

| Area | PRD expectation | Current implementation | Gap | Proposed direction |
|---|---|---|---|---|
| Workflow source | PRD v2 says prompts/workflows are TOML-driven; UI workflow editor is non-goal | Workflows are CRUD in SQLite (`/v1/workflows`), but mission expansion looks up `WorkflowRegistry` manifests loaded from `prompts_path` | Created/edited DB workflows may not execute; missions can be set running without task expansion | Pick one authority: DB or files. For repo/role/workflow management in UI, make DB authoritative and build runtime manifests from DB |
| Agent register + next poll | `POST /v1/agents/register`, `GET /v1/agents/{id}/next` | `POST /v1/crabs/register`; crab polls `GET /v1/tasks` and filters client-side | Contract mismatch, no atomic claim, higher race risk | Add atomic polling endpoint (`/claim-next` or `/agents/{id}/next`) and migrate crab CLI to it |
| Message protocol | `POST /v1/messages`, message delivery on next poll | No messages table/API; protocol only has task/run/heartbeat envelope types | No agent-to-agent questions/replies path | Add `messages` table + send/inbox endpoints + cursor-based polling |
| Targeted communication | Agents must know who to ask | No directory endpoint; no role-based message routing | Agent cannot reliably discover recipient identity | Add `/v1/colonies/{colony_id}/directory`; support `to_role` and `to_crab_id` |
| Transport | PRD currently includes WS broadcast to console | WS routes exist for crabs + console; console client expects WS | Conflicts with polling-only simplification decision | Remove WS from critical path and use HTTP polling for both agents and console |
| Queue/scheduler scope | Shared queue and capability matching | Scheduler matches queued tasks to any idle crab by role only | No domain matching; colony boundaries are ignored during assignment | Enforce colony-safe assignment first; add domain matching if PRD keeps that requirement |
| Agent capabilities | Domains + roles + agent type (`task`/`persistent`) | Crabs have single `role`; no domain set; no `agent_type` | Capability model differs from PRD | Either reduce PRD scope now or extend schema and scheduler |
| Heartbeat/freshness | Scheduler should prefer freshest heartbeat | Heartbeat currently WS-only; no polling heartbeat endpoint; scheduler does not use freshness ranking | Ranking rule not implemented | Add polling heartbeat/update path and scoring logic (phase 2) |
| View: Issues | Create mission + execute from UI | Issue modal execute is no-op (`setTimeout`) and many pages use mock data | End-to-end from issue to mission is incomplete in UI | Wire Execute to real mission queue/create endpoints |
| View: Missions | Live pipeline, run metrics, PR links | Missions page uses mock missions | Live mission operations not visible | Replace with status/task/run-backed data |
| View: Agents | Live states/domains/roles/current mission | Agents page uses mock agents | Not connected to runtime state | Replace with live crab/agent data model |
| View: Messages | Live agent-to-agent log with filters | Messages page uses mock messages | Missing end-to-end feature | Back with messages table + API filters |

## 3) Gaps Specific to Repo/Role/Workflow Management (First Part)

These are the immediate implementation gaps for your current phase.

### 3.1 Workflow CRUD does not drive execution

- UI/DB workflow edits do not consistently affect mission expansion runtime.
- Runtime expansion uses `WorkflowRegistry` from filesystem manifests.
- Consequence: operator edits workflow in UI, but scheduler may still execute old/no manifest behavior.

Needed to close:

1. Convert DB workflows to in-memory executable manifests at read time.
2. Use DB-backed manifest lookup in mission creation and queue activation.
3. Keep prompt-file resolution deterministic and validated.

### 3.2 Prompt file safety/validity checks are weak

- Prompt file reads join raw relative path onto prompts root.
- Role/workflow prompt references can include invalid or unsafe traversal paths.

Needed to close:

1. Canonicalize and enforce prompt path stays under prompts root.
2. Reject `..`, absolute paths, and symlink escapes.
3. Add validation when creating/updating roles/workflows.

### 3.3 UI for management is partially live

- Repos are mostly live.
- Roles/workflows pages exist but broader navigation counts and surrounding views still depend on mocks.
- This causes confusing state for operators.

Needed to close:

1. Remove mock-derived counts from sidebar in management views.
2. Ensure all management pages use control-plane APIs only.
3. Add error states when control-plane is unavailable.

## 4) Gaps to Enable Polling-Only Agent Workflow

### 4.1 Missing atomic task-claim API

Current pattern (`GET /v1/tasks` + client filter) is not safe at scale and is inefficient.

Needed:

1. `POST /v1/crabs/{crab_id}/claim-next` (or PRD-style `/v1/agents/{id}/next`).
2. Single transaction: select eligible task, assign, mark crab busy, return payload.
3. Return `idle` when no task.

### 4.2 Messaging endpoints missing

Needed:

1. `POST /v1/messages` for send.
2. `GET /v1/crabs/{crab_id}/inbox?after=<cursor>` for receive.
3. Optional ack endpoint.
4. Thread keys: `mission_id`, `task_id`, `thread_id`.

### 4.3 Directory endpoint missing

Needed:

1. `GET /v1/colonies/{colony_id}/directory`.
2. Return `crab_id`, `name`, `role`, `state`, `updated_at_ms`.
3. Allow agents to route questions by role and resolve to an identity.

## 5) Priority Backlog

### P0 (must have for reliable phase-1 + polling migration)

1. Resolve workflow source of truth mismatch.
2. Implement atomic claim-next endpoint and migrate crab poll flow.
3. Implement messages table + send/inbox endpoints.
4. Implement colony directory endpoint for role-based routing.
5. Remove no-op execute behavior in issues UI and wire real mission creation.
6. Enforce colony-safe scheduling (no cross-colony assignment).

### P1 (strongly recommended)

1. Replace missions/agents/messages mock views with live APIs.
2. Remove WS console dependency and use HTTP polling snapshots/updates.
3. Harden prompt-file path validation.

### P2 (follow-up after stable MVP)

1. Reintroduce domain capability matching if PRD keeps it.
2. Add heartbeat freshness ranking in scheduler.
3. Add auth and tighten CORS/bind defaults for non-local deployment.

## 6) PRD Updates Required If Polling-Only Direction Is Accepted

Update `docs/prds/prd-crabitat-v2.md` to avoid future drift:

1. Replace WS requirement with HTTP polling for both agents and console.
2. Replace `/v1/agents/{id}/next` wording with finalized claim/inbox API contract.
3. Clarify whether workflow source of truth is DB or TOML files.
4. Clarify whether domain matching is in-scope for phase 1 or deferred.
5. Clarify agent naming (`agent` vs `crab`) to one canonical term.

## 7) Suggested Acceptance Criteria for This Gap-Closure Epic

1. Creating/updating a workflow in UI changes actual runtime task expansion behavior.
2. Three agents can run a full mission pipeline using polling only (no WS) with no manual intervention.
3. Agent A can send a question to role `developer`; developer receives it via inbox poll and replies.
4. Issues -> Execute creates a real mission, and Missions/Agents/Messages views show live updates without mock data.
5. No task is assigned across colony boundaries.

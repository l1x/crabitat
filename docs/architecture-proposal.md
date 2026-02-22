# Crabitat Architecture Proposal

## Domain Model Mapping

| Concept | Maps To | Description |
|---------|---------|-------------|
| Skill | Mission template | A skill defines *what* to do. Invoking a skill from the UI creates a Mission with the skill's prompt template filled in. |
| Prompt | Crab system prompt | Runtime-specific instructions injected when a crab starts a run. Stored centrally, selected by crab's `runtime` field. |
| Ticket / Issue | Mission input | The ticket URL/content becomes the mission's `prompt` context, fed through the skill template. |
| Research subject | Mission input | Free-text input fed through a research skill template. |
| Colony | Project scope | One colony per repo/project. Skills are global, missions are scoped to a colony. |
| Chief | Decomposer | Breaks a mission into tasks with required capabilities. |
| Crab | Executor | Picks up tasks matching its capabilities, runs them in a burrow. |

## New Domain Fields

| Entity | New Field | Type | Purpose |
|--------|-----------|------|---------|
| `crabs` | `runtime` | `"claude-code" \| "gemini-cli" \| "openai-codex" \| "custom"` | Which AI runtime this crab uses |
| `crabs` | `capabilities` | `text[]` (JSON array) | What skills this crab can run, e.g. `["code", "review", "research"]` |
| `missions` | `skill_id` | `text` | Which skill template created this mission |
| `missions` | `input_type` | `"ticket" \| "prompt" \| "research"` | What kind of input seeded this mission |
| `missions` | `input_ref` | `text` | `"issues/6"`, a URL, or inline text |
| `missions` | `parent_mission_id` | `text` | For decomposition — chief breaks big mission into sub-missions |
| `tasks` | `skill_id` | `text` | Specific skill for this task (may differ from mission skill) |
| `tasks` | `required_capabilities` | `text[]` (JSON array) | What capabilities the assigned crab needs |

## Skill File Format

| Field | Type | Example | Purpose |
|-------|------|---------|---------|
| `id` | string (frontmatter) | `prepare-task` | Unique skill identifier |
| `name` | string (frontmatter) | `Prepare Task` | Display name in UI |
| `input_type` | enum (frontmatter) | `ticket` | What kind of input this skill expects |
| `required_capabilities` | string[] (frontmatter) | `["code"]` | Capabilities a crab needs to run this skill |
| `description` | string (frontmatter) | `Break down a ticket into implementation steps` | Shown in UI skill picker |
| body | markdown template | `Given this ticket: {{input}} ...` | Prompt template filled with input to create a mission |

## Central Repo Structure

| Path | Purpose |
|------|---------|
| `skills/prepare-task.md` | Skill: break down a ticket into implementation steps |
| `skills/review-code.md` | Skill: review code changes |
| `skills/research.md` | Skill: research a topic and summarize findings |
| `skills/write-tests.md` | Skill: write tests for existing code |
| `prompts/system/claude-code.md` | System prompt for Claude Code agents |
| `prompts/system/gemini-cli.md` | System prompt for Gemini CLI agents |
| `prompts/system/openai-codex.md` | System prompt for OpenAI Codex agents |
| `prompts/onboarding/universal.md` | Runtime-agnostic onboarding instructions |

## Storage: Hybrid (Git + DB)

| Aspect | Git Repo (source of truth) | SQLite (runtime cache) |
|--------|---------------------------|----------------------|
| Skills | `skills/*.md` files with frontmatter | `skills` table, synced on startup |
| Prompts | `prompts/**/*.md` files | `prompts` table, synced on startup |
| Versioning | Full git history | Overwritten on sync |
| Editing | Edit files, commit, push | Console UI writes to DB, exports to git |
| Sharing | Clone/symlink `.crabitat/` into each project repo | API serves from DB |

## API Changes

| Method | Route | Purpose |
|--------|-------|---------|
| GET | `/v1/skills` | List all skills |
| GET | `/v1/skills/:id` | Get skill by ID (with full template) |
| POST | `/v1/skills/sync` | Re-sync skills from disk into DB |
| GET | `/v1/prompts` | List all prompts |
| GET | `/v1/prompts/:runtime` | Get system prompt for a runtime |
| POST | `/v1/missions` (updated) | Now accepts `skill_id`, `input_type`, `input_ref` |
| POST | `/v1/crabs/register` (updated) | Now accepts `runtime`, `capabilities` |
| POST | `/v1/tasks` (updated) | Now accepts `skill_id`, `required_capabilities` |

## Orchestration Flow (UI → Execution)

| Step | Actor | Action | API Call |
|------|-------|--------|----------|
| 1 | User (Console UI) | Selects skill + provides input | — |
| 2 | Console | Creates mission from skill template | `POST /v1/missions` with `skill_id`, `input_ref` |
| 3 | Control-plane | Loads skill template, fills with input, stores mission | — |
| 4 | Chief | Reads mission, decomposes into tasks with required capabilities | `POST /v1/tasks` (one per sub-task) |
| 5 | Scheduler | Matches idle crabs to tasks by capabilities | Updates `tasks.assigned_crab_id` |
| 6 | Crab | Polls, picks up assigned task | `GET /v1/tasks` via `crabitat-crab poll` |
| 7 | Crab | Starts run in burrow | `POST /v1/runs/start` |
| 8 | Crab | Executes task using its own tools | (local work) |
| 9 | Crab | Reports completion | `POST /v1/runs/complete` |
| 10 | Console | Live update via WebSocket | `ws://.../v1/ws/console` |

## Agent Onboarding Flow

| Step | Actor | Action |
|------|-------|--------|
| 1 | User (Console UI) | Clicks "Add Agent", selects runtime, copies snippet |
| 2 | User | Pastes snippet into Claude Code / Gemini CLI / OpenAI Codex session |
| 3 | Agent | Runs `crabitat-crab guide --runtime claude-code` |
| 4 | `crabitat-crab` | Fetches runtime-specific system prompt + universal onboarding from control-plane |
| 5 | `crabitat-crab` | Prints combined paste-ready onboarding block |
| 6 | Agent | Follows instructions: register → poll → execute → complete → loop |

## Capability Matching

| Capability | Description | Typical Runtimes |
|------------|-------------|-----------------|
| `code` | Write and modify source code | Claude Code, Gemini CLI, OpenAI Codex |
| `review` | Review PRs and code changes | Claude Code, Gemini CLI |
| `research` | Research topics, summarize findings | All |
| `test` | Write and run tests | Claude Code, Gemini CLI, OpenAI Codex |
| `docs` | Write documentation | All |
| `debug` | Investigate and fix bugs | Claude Code, Gemini CLI |
| `plan` | Decompose work into subtasks | Claude Code (Chief role) |

## Changes Per Layer

| Layer | File(s) | Changes |
|-------|---------|---------|
| **crabitat-core** | `src/lib.rs` | Add `SkillId`, `Skill`, `Prompt` structs. Add `runtime`, `capabilities` to domain types. |
| **control-plane** | `src/main.rs` | New tables: `skills`, `prompts`. New routes. Skill-aware mission creation. Capability-based task assignment. Disk sync on startup. |
| **crabitat-chief** | `src/main.rs` | Implement mission decomposition: read mission prompt + skill → generate tasks with capabilities. |
| **crabitat-crab** | `src/main.rs` | `guide --runtime <rt>` fetches runtime-specific prompt. `register` accepts `--runtime`, `--capabilities`. |
| **console** | `src/components/`, `src/pages/` | "New Mission" modal gets skill selector + input field. "Add Agent" button with runtime picker. Skills management page. |
| **new directory** | `skills/`, `prompts/` | Skill definitions as markdown with frontmatter. Runtime-specific system prompts. |

## Open Questions

| # | Question | Options |
|---|----------|---------|
| 1 | Should Chief be an LLM agent or rule-based? | LLM (flexible decomposition) vs rules (predictable, fast) |
| 2 | How to handle cross-runtime differences? | Unified CLI (`crabitat-crab`) vs runtime-specific adapters |
| 3 | Should skills be composable (skill chains)? | Simple (one skill per mission) vs DAG (skill A → skill B) |
| 4 | Where does the central repo live? | Inside crabitat repo (`skills/`, `prompts/`) vs separate repo (`~/.crabitat/`) |

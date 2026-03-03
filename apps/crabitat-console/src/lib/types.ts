export interface RepoRecord {
  repo_id: string;
  owner: string;
  name: string;
  full_name: string;
  default_branch: string;
  language: string;
  local_path: string;
  stacks: string[];
  created_at_ms: number;
}

export interface CrabRecord {
  crab_id: string;
  repo_id: string;
  name: string;
  state: 'idle' | 'busy' | 'offline';
  current_task_id: string | null;
  current_run_id: string | null;
  updated_at_ms: number;
}

export interface MissionRecord {
  mission_id: string;
  repo_id: string;
  prompt: string;
  workflow_name: string | null;
  status: 'pending' | 'running' | 'completed' | 'failed';
  worktree_path: string | null;
  queue_position: number | null;
  github_issue_number: number | null;
  github_pr_number: number | null;
  created_at_ms: number;
}

export interface GitHubIssueRecord {
  number: number;
  title: string;
  body: string;
  labels: string[];
  state: string;
  already_queued: boolean;
}

export interface TaskRecord {
  task_id: string;
  mission_id: string;
  title: string;
  assigned_crab_id: string | null;
  step_id: string | null;
  status: 'queued' | 'assigned' | 'running' | 'blocked' | 'completed' | 'failed';
  created_at_ms: number;
  updated_at_ms: number;
}

export interface RunMetrics {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  first_token_ms: number | null;
  llm_duration_ms: number | null;
  execution_duration_ms: number | null;
  end_to_end_ms: number | null;
}

export interface RunRecord {
  run_id: string;
  mission_id: string;
  task_id: string;
  crab_id: string;
  status: 'queued' | 'running' | 'blocked' | 'completed' | 'failed';
  burrow_path: string;
  burrow_mode: 'worktree' | 'external_repo';
  progress_message: string;
  summary: string | null;
  metrics: RunMetrics;
  started_at_ms: number;
  updated_at_ms: number;
  completed_at_ms: number | null;
}

export interface StatusSummary {
  total_crabs: number;
  busy_crabs: number;
  running_tasks: number;
  running_runs: number;
  completed_runs: number;
  failed_runs: number;
  total_tokens: number;
  avg_end_to_end_ms: number | null;
  cached_issue_count: number;
}

export interface StatusSnapshot {
  generated_at_ms: number;
  summary: StatusSummary;
  repos: RepoRecord[];
  crabs: CrabRecord[];
  missions: MissionRecord[];
  tasks: TaskRecord[];
  runs: RunRecord[];
  repo_issue_counts: Record<string, number>;
}

export interface WorkflowStepRecord {
  step_id: string;
  prompt_file: string;
  depends_on: string[];
  condition: string | null;
  max_retries: number;
  position: number;
  include: string[];
}

export interface WorkflowRecord {
  workflow_id: string;
  name: string;
  description: string;
  include: string[];
  version: string;
  source: 'toml' | 'manual' | 'assembled';
  commit_hash: string | null;
  created_at_ms: number;
  steps: WorkflowStepRecord[];
}

export interface SyncResult {
  synced: number;
  removed: number;
  commit_hash: string | null;
  errors: string[];
}

export interface PromptFilePreview {
  path: string;
  content: string;
}

export interface SettingsRecord {
  prompts_path: string;
  [key: string]: string;
}

export interface SkillRecord {
  name: string;
  path: string;
  description: string;
}


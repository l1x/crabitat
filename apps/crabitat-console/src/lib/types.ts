export interface ColonyRecord {
  colony_id: string;
  name: string;
  description: string;
  created_at_ms: number;
}

export interface CrabRecord {
  crab_id: string;
  colony_id: string;
  name: string;
  role: string;
  state: 'idle' | 'busy' | 'offline';
  current_task_id: string | null;
  current_run_id: string | null;
  updated_at_ms: number;
}

export interface MissionRecord {
  mission_id: string;
  colony_id: string;
  prompt: string;
  created_at_ms: number;
}

export interface TaskRecord {
  task_id: string;
  mission_id: string;
  title: string;
  assigned_crab_id: string | null;
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
}

export interface StatusSnapshot {
  generated_at_ms: number;
  summary: StatusSummary;
  colonies: ColonyRecord[];
  crabs: CrabRecord[];
  missions: MissionRecord[];
  tasks: TaskRecord[];
  runs: RunRecord[];
}

export type ConsoleEvent =
  | { type: 'snapshot' } & StatusSnapshot
  | { type: 'crab_updated'; crab: CrabRecord }
  | { type: 'colony_created'; colony: ColonyRecord }
  | { type: 'mission_created'; mission: MissionRecord }
  | { type: 'task_created'; task: TaskRecord }
  | { type: 'task_updated'; task: TaskRecord }
  | { type: 'run_created'; run: RunRecord }
  | { type: 'run_updated'; run: RunRecord }
  | { type: 'run_completed'; run: RunRecord };

export interface Repo {
  repo_id: string;
  owner: string;
  name: string;
  local_path: string;
  created_at: string;
}

export interface CreateRepoRequest {
  owner: string;
  name: string;
  local_path: string;
}

export interface Issue {
  repo_id: string;
  number: number;
  title: string;
  body: string | null;
  labels: string[];
  state: string;
  fetched_at: string;
}

export interface WorkflowInfo {
  name: string;
  description: string;
  version?: string;
}

export interface WorkflowStepFile {
  id: string;
  prompt_file: string;
  depends_on?: string[];
  on_fail?: string;
  max_retries?: number;
}

export interface WorkflowFlavor {
  flavor_id: string;
  workflow_name: string;
  name: string;
  prompt_paths: string[];
}

export interface WorkflowDetail {
  name: string;
  description: string;
  version?: string;
  steps: WorkflowStepFile[];
  flavors: WorkflowFlavor[];
}

export interface WorkflowSummary {
  name: string;
  description: string;
  step_count: number;
  flavor_count: number;
}

export interface CreateFlavorRequest {
  name: string;
  prompt_paths: string[];
}

export interface Setting {
  key: string;
  value: string;
}

export interface SystemStatus {
  gh_installed: boolean;
  gh_auth_status: boolean;
  gh_version: string | null;
  gh_user: string | null;
}

export interface Mission {
  mission_id: string;
  repo_id: string;
  issue_number: number;
  workflow_name: string;
  flavor_id: string | null;
  status: string;
  created_at: string;
}

export interface Task {
  task_id: string;
  mission_id: string;
  step_id: string;
  step_order: number;
  assembled_prompt: string;
  status: string;
  created_at: string;
}

export interface CreateMissionRequest {
  repo_id: string;
  issue_number: number;
  workflow_name: string;
  flavor_id?: string;
}

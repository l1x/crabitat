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

export interface Workflow {
  workflow_id: string;
  repo_id: string;
  name: string;
  description: string;
  created_at: string;
}

export interface WorkflowStep {
  step_id: string;
  workflow_id: string;
  step_order: number;
  name: string;
  prompt_template: string;
}

export interface WorkflowFlavor {
  flavor_id: string;
  workflow_id: string;
  name: string;
  context: string | null;
}

export interface WorkflowDetail extends Workflow {
  steps: WorkflowStep[];
  flavors: WorkflowFlavor[];
}

export interface WorkflowSummary extends Workflow {
  flavor_count: number;
  repo_owner: string;
  repo_name: string;
}

export interface CreateWorkflowRequest {
  name: string;
  description?: string;
  steps: { name: string; prompt_template: string }[];
}

export interface CreateFlavorRequest {
  name: string;
  context?: string;
}

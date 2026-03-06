import type {
  Repo,
  CreateRepoRequest,
  Issue,
  WorkflowSummary,
  WorkflowDetail,
  CreateWorkflowRequest,
  WorkflowFlavor,
  CreateFlavorRequest,
} from "./types";

const API_BASE = "http://localhost:3001";

export async function listRepos(): Promise<Repo[]> {
  const res = await fetch(`${API_BASE}/v1/repos`);
  if (!res.ok) throw new Error(`Failed to list repos: ${res.status}`);
  return res.json();
}

export async function getRepo(repoId: string): Promise<Repo> {
  const res = await fetch(`${API_BASE}/v1/repos/${repoId}`);
  if (!res.ok) throw new Error(`Failed to get repo: ${res.status}`);
  return res.json();
}

export async function createRepo(body: CreateRepoRequest): Promise<Repo> {
  const res = await fetch(`${API_BASE}/v1/repos`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.error || `Failed to create repo: ${res.status}`);
  }
  return res.json();
}

export async function deleteRepo(repoId: string): Promise<void> {
  const res = await fetch(`${API_BASE}/v1/repos/${repoId}`, {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`Failed to delete repo: ${res.status}`);
}

export async function listIssues(repoId: string): Promise<Issue[]> {
  const res = await fetch(`${API_BASE}/v1/repos/${repoId}/issues`);
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.error || `Failed to list issues: ${res.status}`);
  }
  return res.json();
}

export async function refreshIssues(repoId: string): Promise<Issue[]> {
  const res = await fetch(`${API_BASE}/v1/repos/${repoId}/issues/refresh`, {
    method: "POST",
  });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.error || `Failed to refresh issues: ${res.status}`);
  }
  return res.json();
}

export async function listAllWorkflows(): Promise<WorkflowSummary[]> {
  const res = await fetch(`${API_BASE}/v1/workflows`);
  if (!res.ok) throw new Error(`Failed to list workflows: ${res.status}`);
  return res.json();
}

export async function listRepoWorkflows(
  repoId: string,
): Promise<WorkflowSummary[]> {
  const res = await fetch(`${API_BASE}/v1/repos/${repoId}/workflows`);
  if (!res.ok) throw new Error(`Failed to list repo workflows: ${res.status}`);
  return res.json();
}

export async function getWorkflow(id: string): Promise<WorkflowDetail> {
  const res = await fetch(`${API_BASE}/v1/workflows/${id}`);
  if (!res.ok) throw new Error(`Failed to get workflow: ${res.status}`);
  return res.json();
}

export async function createWorkflow(
  repoId: string,
  body: CreateWorkflowRequest,
): Promise<WorkflowDetail> {
  const res = await fetch(`${API_BASE}/v1/repos/${repoId}/workflows`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.error || `Failed to create workflow: ${res.status}`);
  }
  return res.json();
}

export async function deleteWorkflow(id: string): Promise<void> {
  const res = await fetch(`${API_BASE}/v1/workflows/${id}`, {
    method: "DELETE",
  });
  if (!res.ok) throw new Error(`Failed to delete workflow: ${res.status}`);
}

export async function createFlavor(
  workflowId: string,
  body: CreateFlavorRequest,
): Promise<WorkflowFlavor> {
  const res = await fetch(
    `${API_BASE}/v1/workflows/${workflowId}/flavors`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    },
  );
  if (!res.ok) {
    const err = await res.json();
    throw new Error(err.error || `Failed to create flavor: ${res.status}`);
  }
  return res.json();
}

export async function deleteFlavor(
  workflowId: string,
  flavorId: string,
): Promise<void> {
  const res = await fetch(
    `${API_BASE}/v1/workflows/${workflowId}/flavors/${flavorId}`,
    { method: "DELETE" },
  );
  if (!res.ok) throw new Error(`Failed to delete flavor: ${res.status}`);
}

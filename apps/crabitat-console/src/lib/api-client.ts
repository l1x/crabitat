import type {
  Repo,
  CreateRepoRequest,
  Issue,
  WorkflowSummary,
  WorkflowDetail,
  WorkflowFlavor,
  CreateFlavorRequest,
  Setting,
  SystemStatus,
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

export interface GhRepoResult {
  nameWithOwner: string;
}

export async function searchGithubRepos(
  query: string,
): Promise<GhRepoResult[]> {
  const res = await fetch(
    `${API_BASE}/v1/github/repos?q=${encodeURIComponent(query)}`,
  );
  if (!res.ok) throw new Error(`Failed to search repos: ${res.status}`);
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

export async function getWorkflow(name: string): Promise<WorkflowDetail> {
  const res = await fetch(`${API_BASE}/v1/workflows/${name}`);
  if (!res.ok) throw new Error(`Failed to get workflow: ${res.status}`);
  return res.json();
}

export async function createFlavor(
  workflowName: string,
  body: CreateFlavorRequest,
): Promise<WorkflowFlavor> {
  const res = await fetch(
    `${API_BASE}/v1/workflows/${workflowName}/flavors`,
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
  workflowName: string,
  flavorId: string,
): Promise<void> {
  const res = await fetch(
    `${API_BASE}/v1/workflows/${workflowName}/flavors/${flavorId}`,
    { method: "DELETE" },
  );
  if (!res.ok) throw new Error(`Failed to delete flavor: ${res.status}`);
}

export async function listSettings(): Promise<Setting[]> {
  const res = await fetch(`${API_BASE}/v1/settings`);
  if (!res.ok) throw new Error(`Failed to list settings: ${res.status}`);
  return res.json();
}

export async function updateSetting(key: string, value: string): Promise<Setting> {
  const res = await fetch(`${API_BASE}/v1/settings/${key}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ value }),
  });
  if (!res.ok) throw new Error(`Failed to update setting: ${res.status}`);
  return res.json();
}

export async function getSystemStatus(): Promise<SystemStatus> {
  const res = await fetch(`${API_BASE}/v1/system/status`);
  if (!res.ok) throw new Error(`Failed to get system status: ${res.status}`);
  return res.json();
}

export async function listPromptFiles(): Promise<string[]> {
  const res = await fetch(`${API_BASE}/v1/prompts/files`);
  if (!res.ok) throw new Error(`Failed to list prompt files: ${res.status}`);
  return res.json();
}

export async function listDirs(query: string): Promise<string[]> {
  const res = await fetch(`${API_BASE}/v1/system/dirs?q=${encodeURIComponent(query)}`);
  if (!res.ok) throw new Error(`Failed to list directories: ${res.status}`);
  return res.json();
}

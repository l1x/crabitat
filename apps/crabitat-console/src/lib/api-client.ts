import type { Repo, CreateRepoRequest, Issue } from "./types";

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

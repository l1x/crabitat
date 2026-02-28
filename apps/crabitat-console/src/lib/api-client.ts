import type { StatusSnapshot, CrabRecord, GitHubIssueRecord, MissionRecord, PromptFilePreview, RepoRecord, SettingsRecord, SkillRecord, SyncResult, TaskRecord, WorkflowRecord } from './types';

export interface StackEntry {
  name: string;
  path: string;
}

const CONTROL_PLANE_URL = import.meta.env.CONTROL_PLANE_URL || 'http://127.0.0.1:8800';

export async function fetchRepos(): Promise<RepoRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos`);
  if (!res.ok) throw new Error(`GET /v1/repos failed: ${res.status}`);
  return res.json();
}

export async function createRepo(body: {
  owner: string;
  name: string;
  default_branch?: string;
  language?: string;
  local_path: string;
}): Promise<RepoRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST /v1/repos failed: ${res.status}`);
  return res.json();
}

export async function deleteRepo(repoId: string): Promise<void> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}`, {
    method: 'DELETE',
  });
  if (!res.ok) throw new Error(`DELETE /v1/repos/${repoId} failed: ${res.status}`);
}

export async function fetchStatus(): Promise<StatusSnapshot> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/status`);
  if (!res.ok) throw new Error(`GET /v1/status failed: ${res.status}`);
  return res.json();
}

export async function fetchCrabs(): Promise<CrabRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/crabs`);
  if (!res.ok) throw new Error(`GET /v1/crabs failed: ${res.status}`);
  return res.json();
}

export async function registerCrab(body: {
  crab_id: string;
  repo_id: string;
  name: string;
  state?: 'idle' | 'busy' | 'offline';
}): Promise<CrabRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/crabs/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST /v1/crabs/register failed: ${res.status}`);
  return res.json();
}

export async function fetchRepoIssues(repoId: string): Promise<GitHubIssueRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}/issues`);
  if (!res.ok) throw new Error(`GET /v1/repos/${repoId}/issues failed: ${res.status}`);
  return res.json();
}

export async function queueIssue(
  repoId: string,
  issueNumber: number,
  workflow?: string,
): Promise<MissionRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}/queue`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ issue_number: issueNumber, workflow }),
  });
  if (!res.ok) throw new Error(`POST /v1/repos/${repoId}/queue failed: ${res.status}`);
  return res.json();
}

export async function fetchQueue(repoId: string): Promise<MissionRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}/queue`);
  if (!res.ok) throw new Error(`GET /v1/repos/${repoId}/queue failed: ${res.status}`);
  return res.json();
}

export async function removeFromQueue(repoId: string, missionId: string): Promise<void> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}/queue/${missionId}`, {
    method: 'DELETE',
  });
  if (!res.ok) throw new Error(`DELETE queue/${missionId} failed: ${res.status}`);
}

export async function fetchWorkflows(): Promise<WorkflowRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/workflows`);
  if (!res.ok) throw new Error(`GET /v1/workflows failed: ${res.status}`);
  return res.json();
}

export async function syncWorkflows(): Promise<SyncResult> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/workflows/sync`, {
    method: 'POST',
  });
  if (!res.ok) throw new Error(`POST /v1/workflows/sync failed: ${res.status}`);
  return res.json();
}

export async function fetchPromptFiles(): Promise<string[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/prompt-files`);
  if (!res.ok) throw new Error(`GET /v1/prompt-files failed: ${res.status}`);
  return res.json();
}

export async function fetchPromptFilePreview(path: string): Promise<PromptFilePreview> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/prompt-files/preview?path=${encodeURIComponent(path)}`);
  if (!res.ok) throw new Error(`GET /v1/prompt-files/preview failed: ${res.status}`);
  return res.json();
}

export async function fetchSettings(): Promise<SettingsRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/settings`);
  if (!res.ok) throw new Error(`GET /v1/settings failed: ${res.status}`);
  return res.json();
}

export async function updateSettings(body: Partial<SettingsRecord>): Promise<SettingsRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/settings`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST /v1/settings failed: ${res.status}`);
  return res.json();
}

export async function fetchRepoLanguages(repoId: string): Promise<Record<string, number>> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}/languages`);
  if (!res.ok) throw new Error(`GET /v1/repos/${repoId}/languages failed: ${res.status}`);
  return res.json();
}

export async function fetchSkills(): Promise<SkillRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/skills`);
  if (!res.ok) throw new Error(`GET /v1/skills failed: ${res.status}`);
  return res.json();
}

export async function fetchMissions(): Promise<MissionRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/missions`);
  if (!res.ok) throw new Error(`GET /v1/missions failed: ${res.status}`);
  return res.json();
}

export async function createMission(body: {
  repo_id: string;
  prompt: string;
  workflow?: string;
}): Promise<MissionRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/missions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST /v1/missions failed: ${res.status}`);
  return res.json();
}

export async function fetchTasks(): Promise<TaskRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/tasks`);
  if (!res.ok) throw new Error(`GET /v1/tasks failed: ${res.status}`);
  return res.json();
}

export async function fetchStacks(): Promise<StackEntry[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/stacks`);
  if (!res.ok) throw new Error(`GET /v1/stacks failed: ${res.status}`);
  return res.json();
}

export async function updateRepoStacks(repoId: string, stacks: string[]): Promise<RepoRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}/update`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ stacks }),
  });
  if (!res.ok) throw new Error(`POST /v1/repos/${repoId}/update failed: ${res.status}`);
  return res.json();
}

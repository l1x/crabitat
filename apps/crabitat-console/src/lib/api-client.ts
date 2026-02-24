import type { StatusSnapshot, CrabRecord, ColonyRecord, GitHubIssueRecord, MissionRecord, PromptFilePreview, RepoRecord, SettingsRecord, SkillRecord, WorkflowRecord } from './types';

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

export async function fetchColonies(): Promise<ColonyRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/colonies`);
  if (!res.ok) throw new Error(`GET /v1/colonies failed: ${res.status}`);
  return res.json();
}

export async function createColony(body: {
  name: string;
  description?: string;
}): Promise<ColonyRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/colonies`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`POST /v1/colonies failed: ${res.status}`);
  return res.json();
}

export async function registerCrab(body: {
  crab_id: string;
  colony_id: string;
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

export async function updateColony(
  colonyId: string,
  body: { repo?: string; name?: string; description?: string },
): Promise<ColonyRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/colonies/${colonyId}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`PATCH /v1/colonies/${colonyId} failed: ${res.status}`);
  return res.json();
}

export async function fetchRepoIssues(repoId: string): Promise<GitHubIssueRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/repos/${repoId}/issues`);
  if (!res.ok) throw new Error(`GET /v1/repos/${repoId}/issues failed: ${res.status}`);
  return res.json();
}

export async function fetchIssues(colonyId: string): Promise<GitHubIssueRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/colonies/${colonyId}/issues`);
  if (!res.ok) throw new Error(`GET /v1/colonies/${colonyId}/issues failed: ${res.status}`);
  return res.json();
}

export async function queueIssue(
  colonyId: string,
  issueNumber: number,
  workflow?: string,
): Promise<MissionRecord> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/colonies/${colonyId}/queue`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ issue_number: issueNumber, workflow }),
  });
  if (!res.ok) throw new Error(`POST /v1/colonies/${colonyId}/queue failed: ${res.status}`);
  return res.json();
}

export async function fetchQueue(colonyId: string): Promise<MissionRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/colonies/${colonyId}/queue`);
  if (!res.ok) throw new Error(`GET /v1/colonies/${colonyId}/queue failed: ${res.status}`);
  return res.json();
}

export async function removeFromQueue(colonyId: string, missionId: string): Promise<void> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/colonies/${colonyId}/queue/${missionId}`, {
    method: 'DELETE',
  });
  if (!res.ok) throw new Error(`DELETE queue/${missionId} failed: ${res.status}`);
}

export async function fetchWorkflows(): Promise<WorkflowRecord[]> {
  const res = await fetch(`${CONTROL_PLANE_URL}/v1/workflows`);
  if (!res.ok) throw new Error(`GET /v1/workflows failed: ${res.status}`);
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
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`PATCH /v1/settings failed: ${res.status}`);
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

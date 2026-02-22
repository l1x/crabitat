import { fetchIssues, fetchQueue, queueIssue, removeFromQueue } from '../lib/api-client';
import { store } from './store';
import type { GitHubIssueRecord, MissionRecord } from '../lib/types';

const CONTROL_PLANE_URL = 'http://127.0.0.1:8800';

function getSelectedColonyId(): string | null {
  const select = document.getElementById('queue-colony-select') as HTMLSelectElement | null;
  return select?.value ?? null;
}

function renderIssues(issues: GitHubIssueRecord[]) {
  const container = document.getElementById('issues-list');
  if (!container) return;

  if (issues.length === 0) {
    container.innerHTML = '<div class="empty-state"><div class="empty-state-title">No open issues</div></div>';
    return;
  }

  container.innerHTML = issues
    .map(
      (issue) => `
      <div class="card" style="margin-bottom:8px">
        <div class="card-title">#${issue.number} ${esc(issue.title)}</div>
        <div class="card-meta">${issue.labels.map((l) => `<span class="badge">${esc(l)}</span>`).join(' ')}</div>
        <div class="card-meta" style="margin-top:4px">
          ${
            issue.already_queued
              ? '<span class="badge badge--completed">queued</span>'
              : `<button class="btn btn--primary btn--sm" data-queue-issue="${issue.number}">Queue</button>`
          }
        </div>
      </div>`,
    )
    .join('');

  // Bind queue buttons
  container.querySelectorAll<HTMLButtonElement>('[data-queue-issue]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const colonyId = getSelectedColonyId();
      if (!colonyId) return;
      const issueNum = Number(btn.dataset.queueIssue);
      btn.disabled = true;
      btn.textContent = 'Queuing...';
      try {
        await queueIssue(colonyId, issueNum);
        await refreshAll();
      } catch (err) {
        btn.textContent = 'Error';
        console.error(err);
      }
    });
  });
}

function renderQueue(missions: MissionRecord[]) {
  const container = document.getElementById('queue-list');
  if (!container) return;

  if (missions.length === 0) {
    container.innerHTML = '<div class="empty-state"><div class="empty-state-title">No missions queued</div></div>';
    return;
  }

  container.innerHTML = missions
    .map(
      (m) => `
      <div class="card" style="margin-bottom:8px" data-id="${esc(m.mission_id)}">
        <div class="card-title">
          #${m.queue_position}
          ${m.github_issue_number ? `Issue #${m.github_issue_number}` : esc(m.prompt.slice(0, 40))}
          <span class="badge badge--${m.status}">${m.status}</span>
        </div>
        <div class="card-meta">${esc(m.prompt.length > 60 ? m.prompt.slice(0, 60) + '...' : m.prompt)}</div>
        ${
          m.status === 'pending'
            ? `<div class="card-meta" style="margin-top:4px"><button class="btn btn--sm" data-remove-mission="${m.mission_id}">Remove</button></div>`
            : ''
        }
      </div>`,
    )
    .join('');

  // Bind remove buttons
  container.querySelectorAll<HTMLButtonElement>('[data-remove-mission]').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const colonyId = getSelectedColonyId();
      if (!colonyId) return;
      const missionId = btn.dataset.removeMission!;
      btn.disabled = true;
      btn.textContent = 'Removing...';
      try {
        await removeFromQueue(colonyId, missionId);
        await refreshAll();
      } catch (err) {
        btn.textContent = 'Error';
        console.error(err);
      }
    });
  });
}

async function refreshAll() {
  const colonyId = getSelectedColonyId();
  if (!colonyId) return;

  try {
    const [issues, queue] = await Promise.all([fetchIssues(colonyId), fetchQueue(colonyId)]);
    renderIssues(issues);
    renderQueue(queue);
  } catch (err) {
    console.error('Failed to refresh queue:', err);
  }
}

function esc(str: string): string {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

// Init
document.addEventListener('DOMContentLoaded', () => {
  const refreshBtn = document.getElementById('queue-refresh-btn');
  refreshBtn?.addEventListener('click', refreshAll);

  const select = document.getElementById('queue-colony-select');
  select?.addEventListener('change', refreshAll);

  // Auto-refresh on mission events
  store.addEventListener('mission_created', () => refreshAll());
  store.addEventListener('mission_updated', () => refreshAll());
});

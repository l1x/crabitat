import { store } from './store';
import { formatMs, formatTokens, timeAgo } from '../lib/format';
import type { ColonyRecord, CrabRecord, MissionRecord, TaskRecord, RunRecord } from '../lib/types';

// ---- Metric updates ----

function updateMetrics() {
  const s = store.summary;
  setMetric('total_crabs', String(s.total_crabs));
  setMetric('busy_crabs', String(s.busy_crabs));
  setMetric('running_runs', String(s.running_runs));
  setMetric('completed_runs', String(s.completed_runs));
  setMetric('failed_runs', String(s.failed_runs));
  setMetric('total_tokens', formatTokens(s.total_tokens));
  setMetric('running_tasks', String(s.running_tasks));
  setMetric('avg_end_to_end_ms', formatMs(s.avg_end_to_end_ms));
}

function setMetric(name: string, value: string) {
  const card = document.querySelector<HTMLElement>(`[data-metric="${name}"]`);
  if (!card) return;
  const valEl = card.querySelector('.metric-card-value');
  if (valEl) valEl.textContent = value;
}

// ---- Count badge updates ----

function updateCounts() {
  setCount('colonies', store.colonies.length);
  setCount('crabs', store.crabs.length);
  setCount('missions', store.missions.length);
  setCount('tasks', store.tasks.length);
  setCount('runs', store.runs.length);
}

function setCount(name: string, value: number) {
  document.querySelectorAll<HTMLElement>(`[data-count="${name}"]`).forEach((el) => {
    el.textContent = String(value);
  });
}

// ---- Snapshot full re-render ----

export function renderSnapshot() {
  updateMetrics();
  updateCounts();
}

// ---- Colony ----

export function renderColonyCreated(colony: ColonyRecord) {
  updateMetrics();
  updateCounts();

  // Add to overview colony container
  const overviewContainer = document.getElementById('overview-colony-container');
  if (overviewContainer) {
    const card = createColonyCard(colony, true);
    overviewContainer.prepend(card);
  }

  // Add to detail colony container
  const detailContainer = document.getElementById('colony-container');
  if (detailContainer) {
    const card = createColonyCard(colony, false);
    detailContainer.prepend(card);
  }
}

function createColonyCard(colony: ColonyRecord, compact: boolean): HTMLDivElement {
  const card = document.createElement('div');
  card.className = 'card';
  card.dataset.id = colony.colony_id;
  if (!compact) {
    card.dataset.searchable = `${colony.name} ${colony.colony_id} ${colony.description}`;
    card.dataset.createdAtMs = String(colony.created_at_ms);
  }

  let html = `
    <div class="card-title">${esc(colony.name)}</div>
    <div class="card-meta">${esc(colony.colony_id.slice(0, 8))} &middot; ${esc(colony.description || 'No description')}</div>
  `;
  if (compact) {
    html += `<div class="card-meta">0 crabs</div>`;
  } else {
    html += `<div class="card-meta">0 crabs &middot; 0 missions</div>`;
    html += `<div class="card-meta">Created ${timeAgo(colony.created_at_ms)}</div>`;
  }
  card.innerHTML = html;
  return card;
}

// ---- Crab ----

export function renderCrabUpdated(crab: CrabRecord) {
  updateMetrics();
  updateCounts();

  // Update in detail crab container
  updateOrPrependCrabCard('crab-container', crab);
  // Update in overview crab container
  updateOrPrependCrabCard('overview-crab-container', crab);
}

function updateOrPrependCrabCard(containerId: string, crab: CrabRecord) {
  const container = document.getElementById(containerId);
  if (!container) return;

  const existing = container.querySelector<HTMLElement>(`[data-id="${crab.crab_id}"]`);
  if (existing) {
    existing.dataset.state = crab.state;
    existing.dataset.updatedAtMs = String(crab.updated_at_ms);
    // Re-render card contents
    existing.innerHTML = crabCardInnerHtml(crab, containerId === 'overview-crab-container');
  } else {
    const card = document.createElement('div');
    card.className = 'card';
    card.dataset.id = crab.crab_id;
    card.dataset.state = crab.state;
    if (containerId !== 'overview-crab-container') {
      card.dataset.searchable = `${crab.name} ${crab.crab_id} ${crab.role}`;
      card.dataset.updatedAtMs = String(crab.updated_at_ms);
    }
    card.innerHTML = crabCardInnerHtml(crab, containerId === 'overview-crab-container');
    container.prepend(card);
  }
}

function crabCardInnerHtml(crab: CrabRecord, compact: boolean): string {
  let html = `
    <div class="card-title">
      ${esc(crab.name)}
      <span class="badge badge--${crab.state}">${crab.state}</span>
    </div>
    <div class="card-meta">${esc(crab.role)} &middot; ${esc(compact ? crab.crab_id.slice(0, 12) : crab.crab_id)}</div>
  `;
  if (!compact) {
    html += `<div class="card-meta">Colony: ${esc(crab.colony_id.slice(0, 8))} &middot; Updated ${timeAgo(crab.updated_at_ms)}</div>`;
    if (crab.current_task_id) {
      html += `<div class="card-detail">Task: <code>${esc(crab.current_task_id.slice(0, 8))}</code></div>`;
    }
    if (crab.current_run_id) {
      html += `<div class="card-detail">Run: <code>${esc(crab.current_run_id.slice(0, 8))}</code></div>`;
    }
  } else {
    if (crab.current_task_id) {
      html += `<div class="card-detail">Task: <code>${esc(crab.current_task_id.slice(0, 8))}</code></div>`;
    }
  }
  return html;
}

// ---- Mission ----

export function renderMissionCreated(mission: MissionRecord) {
  updateMetrics();
  updateCounts();

  const container = document.getElementById('mission-container');
  if (!container) return;

  const existing = container.querySelector<HTMLElement>(`[data-id="${mission.mission_id}"]`);
  if (existing) return;

  const tr = document.createElement('tr');
  tr.dataset.id = mission.mission_id;
  tr.dataset.searchable = `${mission.mission_id} ${mission.colony_id} ${mission.prompt}`;
  tr.dataset.createdAtMs = String(mission.created_at_ms);
  const prompt = mission.prompt.length > 80 ? mission.prompt.slice(0, 80) + '...' : mission.prompt;
  tr.innerHTML = `
    <td><code>${esc(mission.mission_id.slice(0, 8))}</code></td>
    <td><code>${esc(mission.colony_id.slice(0, 8))}</code></td>
    <td>${esc(prompt)}</td>
    <td>${timeAgo(mission.created_at_ms)}</td>
  `;
  container.prepend(tr);
}

// ---- Task ----

export function renderTaskCreated(task: TaskRecord) {
  updateMetrics();
  updateCounts();
  renderTaskRow(task);
}

export function renderTaskUpdated(task: TaskRecord) {
  updateMetrics();
  updateCounts();
  renderTaskRow(task);
}

function renderTaskRow(task: TaskRecord) {
  const container = document.getElementById('task-container');
  if (!container) return;

  const existing = container.querySelector<HTMLElement>(`[data-id="${task.task_id}"]`);
  const html = `
    <td>${esc(task.title)}</td>
    <td><code>${esc(task.mission_id.slice(0, 8))}</code></td>
    <td>${task.assigned_crab_id ? esc(task.assigned_crab_id.slice(0, 12)) : '\u2014'}</td>
    <td><span class="badge badge--${task.status}">${task.status}</span></td>
    <td>${timeAgo(task.updated_at_ms)}</td>
  `;

  if (existing) {
    existing.dataset.status = task.status;
    existing.dataset.updatedAtMs = String(task.updated_at_ms);
    existing.innerHTML = html;
  } else {
    const tr = document.createElement('tr');
    tr.dataset.id = task.task_id;
    tr.dataset.searchable = `${task.title} ${task.task_id} ${task.mission_id}`;
    tr.dataset.status = task.status;
    tr.dataset.updatedAtMs = String(task.updated_at_ms);
    tr.innerHTML = html;
    container.prepend(tr);
  }
}

// ---- Run ----

export function renderRunUpdated(run: RunRecord) {
  updateMetrics();
  updateCounts();

  const container = document.getElementById('run-container');
  if (!container) return;

  const existing = container.querySelector<HTMLElement>(`[data-id="${run.run_id}"]`);
  const progress = run.progress_message.length > 50 ? run.progress_message.slice(0, 50) + '...' : run.progress_message;
  const html = `
    <td><code>${esc(run.run_id.slice(0, 8))}</code></td>
    <td>${esc(run.crab_id.slice(0, 12))}</td>
    <td><span class="badge badge--${run.status}">${run.status}</span></td>
    <td>${formatTokens(run.metrics.total_tokens)}</td>
    <td>${formatMs(run.metrics.end_to_end_ms)}</td>
    <td>${esc(progress)}</td>
    <td>${timeAgo(run.updated_at_ms)}</td>
  `;

  if (existing) {
    existing.dataset.status = run.status;
    existing.dataset.updatedAtMs = String(run.updated_at_ms);
    existing.innerHTML = html;
  } else {
    const tr = document.createElement('tr');
    tr.dataset.id = run.run_id;
    tr.dataset.searchable = `${run.run_id} ${run.crab_id} ${run.progress_message}`;
    tr.dataset.status = run.status;
    tr.dataset.updatedAtMs = String(run.updated_at_ms);
    tr.innerHTML = html;
    container.prepend(tr);
  }
}

// ---- Helpers ----

function esc(str: string): string {
  const div = document.createElement('div');
  div.textContent = str;
  return div.innerHTML;
}

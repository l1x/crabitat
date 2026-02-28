import type { StatusSnapshot, StatusSummary, RepoRecord, CrabRecord, MissionRecord, TaskRecord, RunRecord } from '../lib/types';

class ConsoleStore extends EventTarget {
  repos: RepoRecord[] = [];
  crabs: CrabRecord[] = [];
  missions: MissionRecord[] = [];
  tasks: TaskRecord[] = [];
  runs: RunRecord[] = [];
  summary: StatusSummary = {
    total_crabs: 0,
    busy_crabs: 0,
    running_tasks: 0,
    running_runs: 0,
    completed_runs: 0,
    failed_runs: 0,
    total_tokens: 0,
    avg_end_to_end_ms: null,
    cached_issue_count: 0,
  };

  init(snapshot: StatusSnapshot) {
    this.repos = snapshot.repos ?? [];
    this.crabs = snapshot.crabs;
    this.missions = snapshot.missions;
    this.tasks = snapshot.tasks;
    this.runs = snapshot.runs;
    this.summary = snapshot.summary;
    this.dispatch('snapshot');
  }

  addRepo(repo: RepoRecord) {
    const idx = this.repos.findIndex((r) => r.repo_id === repo.repo_id);
    if (idx >= 0) this.repos[idx] = repo;
    else this.repos.unshift(repo);
    this.recompute();
    this.dispatch('repo_created');
  }

  updateRepo(repo: RepoRecord) {
    const idx = this.repos.findIndex((r) => r.repo_id === repo.repo_id);
    if (idx >= 0) this.repos[idx] = repo;
    else this.repos.unshift(repo);
    this.recompute();
    this.dispatch('repo_updated');
  }

  removeRepo(repoId: string) {
    this.repos = this.repos.filter((r) => r.repo_id !== repoId);
    this.recompute();
    this.dispatch('repo_deleted');
  }

  updateCrab(crab: CrabRecord) {
    const idx = this.crabs.findIndex((c) => c.crab_id === crab.crab_id);
    if (idx >= 0) this.crabs[idx] = crab;
    else this.crabs.unshift(crab);
    this.recompute();
    this.dispatch('crab_updated');
  }

  addMission(mission: MissionRecord) {
    const idx = this.missions.findIndex((m) => m.mission_id === mission.mission_id);
    if (idx >= 0) this.missions[idx] = mission;
    else this.missions.unshift(mission);
    this.recompute();
    this.dispatch('mission_created');
  }

  updateMission(mission: MissionRecord) {
    const idx = this.missions.findIndex((m) => m.mission_id === mission.mission_id);
    if (idx >= 0) this.missions[idx] = mission;
    else this.missions.unshift(mission);
    this.recompute();
    this.dispatch('mission_updated');
  }

  addTask(task: TaskRecord) {
    const idx = this.tasks.findIndex((t) => t.task_id === task.task_id);
    if (idx >= 0) this.tasks[idx] = task;
    else this.tasks.unshift(task);
    this.recompute();
    this.dispatch('task_created');
  }

  updateTask(task: TaskRecord) {
    const idx = this.tasks.findIndex((t) => t.task_id === task.task_id);
    if (idx >= 0) this.tasks[idx] = task;
    else this.tasks.unshift(task);
    this.recompute();
    this.dispatch('task_updated');
  }

  updateRun(run: RunRecord) {
    const idx = this.runs.findIndex((r) => r.run_id === run.run_id);
    if (idx >= 0) this.runs[idx] = run;
    else this.runs.unshift(run);
    this.recompute();
    this.dispatch('run_updated');
  }

  private recompute() {
    const completedRuns = this.runs.filter((r) => r.status === 'completed');
    const totalE2e = completedRuns.reduce((sum, r) => sum + (r.metrics.end_to_end_ms ?? 0), 0);

    this.summary = {
      total_crabs: this.crabs.length,
      busy_crabs: this.crabs.filter((c) => c.state === 'busy').length,
      running_tasks: this.tasks.filter((t) => t.status === 'running').length,
      running_runs: this.runs.filter((r) => r.status === 'running').length,
      completed_runs: completedRuns.length,
      failed_runs: this.runs.filter((r) => r.status === 'failed').length,
      total_tokens: this.runs.reduce((sum, r) => sum + r.metrics.total_tokens, 0),
      avg_end_to_end_ms: completedRuns.length > 0 ? Math.round(totalE2e / completedRuns.length) : null,
      cached_issue_count: this.summary.cached_issue_count,
    };
  }

  private dispatch(type: string) {
    this.dispatchEvent(new CustomEvent(type));
  }
}

export const store = new ConsoleStore();

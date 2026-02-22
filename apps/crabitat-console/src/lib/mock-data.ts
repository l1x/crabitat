// Mock data for Crabitat v2 UI development
// All data is hardcoded — no backend calls needed

export type Domain = 'any' | 'web-dev' | 'backend' | 'infra' | 'docs';
export type Role = 'any' | 'plan' | 'code' | 'review';

export interface MockRepo {
  repo_id: string;
  owner: string;
  name: string;
  full_name: string;
  default_branch: string;
  domain: Domain;
  local_path: string;
}

export interface MockIssue {
  number: number;
  title: string;
  labels: string[];
  body: string;
  state: string;
  created_at: string;
  repo_id: string;
}

export interface PipelineStep {
  step: 'plan' | 'test' | 'implement' | 'review';
  role: Role;
  status: 'pending' | 'running' | 'completed' | 'failed';
  agent: string | null;
  tokens: number;
  wall_clock_ms: number;
  summary: string | null;
}

export interface MockMission {
  mission_id: string;
  repo_id: string;
  title: string;
  github_issue_number: number;
  status: 'pending' | 'running' | 'completed' | 'failed';
  branch: string | null;
  pr_number: number | null;
  created_at_ms: number;
  pipeline: PipelineStep[];
}

export interface MockAgent {
  agent_id: string;
  name: string;
  type: 'task' | 'persistent';
  domains: Domain[];
  roles: Role[];
  state: 'idle' | 'busy' | 'offline';
  current_mission: string | null;
  last_heartbeat_ms: number;
  message_count: number;
}

export interface MockMessage {
  message_id: string;
  from: string;
  to: string;
  mission_id: string | null;
  body: string;
  created_at_ms: number;
}

// --- Repos ---

export const mockRepos: MockRepo[] = [
  {
    repo_id: 'r1',
    owner: 'l1x',
    name: 'crabitat',
    full_name: 'l1x/crabitat',
    default_branch: 'main',
    domain: 'backend',
    local_path: '/Users/l1x/code/home/projectz/crabitat',
  },
  {
    repo_id: 'r2',
    owner: 'l1x',
    name: 'homepage',
    full_name: 'l1x/homepage',
    default_branch: 'main',
    domain: 'web-dev',
    local_path: '/Users/l1x/code/home/projectz/homepage',
  },
  {
    repo_id: 'r3',
    owner: 'l1x',
    name: 'infra',
    full_name: 'l1x/infra',
    default_branch: 'main',
    domain: 'infra',
    local_path: '/Users/l1x/code/home/projectz/infra',
  },
];

// --- Issues ---

export const mockIssues: MockIssue[] = [
  {
    number: 3,
    title: 'Add /colonies/:colony_id detail view',
    labels: ['enhancement'],
    body: 'Currently the console only shows a list of colonies. We need a detail view that shows all crabs, missions, and tasks for a single colony.',
    state: 'open',
    created_at: '2026-02-18T10:30:00Z',
    repo_id: 'r1',
  },
  {
    number: 4,
    title: 'Add /missions/:mission_id detail view',
    labels: ['enhancement'],
    body: 'Show full mission details including all tasks, runs, and associated GitHub issue/PR links.',
    state: 'open',
    created_at: '2026-02-18T14:15:00Z',
    repo_id: 'r1',
  },
  {
    number: 5,
    title: 'Fix token counting in run metrics',
    labels: ['bug'],
    body: 'Token counts are occasionally reported as 0 when Claude returns a valid response. Need to parse the JSON output more carefully.',
    state: 'open',
    created_at: '2026-02-19T09:00:00Z',
    repo_id: 'r1',
  },
  {
    number: 12,
    title: 'Redesign landing page hero section',
    labels: ['design', 'frontend'],
    body: 'The hero section needs a refresh. New copy is in the Figma file. Should include animated gradient background.',
    state: 'open',
    created_at: '2026-02-20T09:00:00Z',
    repo_id: 'r2',
  },
  {
    number: 13,
    title: 'Add dark mode toggle',
    labels: ['feature'],
    body: 'Add a theme toggle that persists preference to localStorage. Support system preference detection.',
    state: 'open',
    created_at: '2026-02-21T11:00:00Z',
    repo_id: 'r2',
  },
  {
    number: 7,
    title: 'Migrate CI from GitHub Actions to Dagger',
    labels: ['infra'],
    body: 'Replace the GitHub Actions workflows with Dagger pipelines for local reproducibility.',
    state: 'open',
    created_at: '2026-02-20T14:00:00Z',
    repo_id: 'r3',
  },
];

// --- Agents ---

const now = Date.now();

export const mockAgents: MockAgent[] = [
  {
    agent_id: 'a1',
    name: 'atlas',
    type: 'task',
    domains: ['any'],
    roles: ['any'],
    state: 'idle',
    current_mission: null,
    last_heartbeat_ms: now - 5_000,
    message_count: 0,
  },
  {
    agent_id: 'a2',
    name: 'builder',
    type: 'task',
    domains: ['backend', 'web-dev'],
    roles: ['code', 'plan'],
    state: 'busy',
    current_mission: 'm1',
    last_heartbeat_ms: now - 2_000,
    message_count: 3,
  },
  {
    agent_id: 'a3',
    name: 'critic',
    type: 'task',
    domains: ['any'],
    roles: ['review'],
    state: 'idle',
    current_mission: null,
    last_heartbeat_ms: now - 8_000,
    message_count: 0,
  },
  {
    agent_id: 'a4',
    name: 'ops',
    type: 'task',
    domains: ['infra'],
    roles: ['any'],
    state: 'offline',
    current_mission: null,
    last_heartbeat_ms: now - 120_000,
    message_count: 0,
  },
  {
    agent_id: 'p1',
    name: 'doc-search',
    type: 'persistent',
    domains: ['any'],
    roles: ['any'],
    state: 'idle',
    current_mission: null,
    last_heartbeat_ms: now - 3_000,
    message_count: 12,
  },
];

// --- Missions ---

export const mockMissions: MockMission[] = [
  {
    mission_id: 'm1',
    repo_id: 'r1',
    title: 'Add /colonies/:colony_id detail view',
    github_issue_number: 3,
    status: 'running',
    branch: 'feat/colony-detail-view',
    pr_number: null,
    created_at_ms: now - 3_600_000,
    pipeline: [
      {
        step: 'plan', role: 'plan',
        status: 'completed', agent: 'atlas',
        tokens: 4200, wall_clock_ms: 45_000,
        summary: 'Created implementation plan with 3 components: ColonyDetail.astro, colony API route, and sidebar link.',
      },
      {
        step: 'test', role: 'code',
        status: 'completed', agent: 'builder',
        tokens: 3100, wall_clock_ms: 38_000,
        summary: 'Wrote 6 test cases covering colony detail rendering, API response handling, and error states.',
      },
      {
        step: 'implement', role: 'code',
        status: 'running', agent: 'builder',
        tokens: 1800, wall_clock_ms: 22_000,
        summary: null,
      },
      {
        step: 'review', role: 'review',
        status: 'pending', agent: null,
        tokens: 0, wall_clock_ms: 0,
        summary: null,
      },
    ],
  },
  {
    mission_id: 'm2',
    repo_id: 'r1',
    title: 'Fix token counting in run metrics',
    github_issue_number: 5,
    status: 'completed',
    branch: 'fix/token-counting',
    pr_number: 8,
    created_at_ms: now - 86_400_000,
    pipeline: [
      {
        step: 'plan', role: 'plan',
        status: 'completed', agent: 'atlas',
        tokens: 2800, wall_clock_ms: 32_000,
        summary: 'Identified root cause: JSON parsing skips nested usage field. Fix in crabitat-crab/src/runner.rs.',
      },
      {
        step: 'test', role: 'code',
        status: 'completed', agent: 'builder',
        tokens: 1900, wall_clock_ms: 25_000,
        summary: 'Added test for zero-token edge case and nested JSON response parsing.',
      },
      {
        step: 'implement', role: 'code',
        status: 'completed', agent: 'builder',
        tokens: 2400, wall_clock_ms: 30_000,
        summary: 'Fixed JSON parser to extract usage from nested response object. All tests pass.',
      },
      {
        step: 'review', role: 'review',
        status: 'completed', agent: 'critic',
        tokens: 1500, wall_clock_ms: 18_000,
        summary: 'PASS. Clean fix, good test coverage, no regressions.',
      },
    ],
  },
  {
    mission_id: 'm3',
    repo_id: 'r2',
    title: 'Redesign landing page hero section',
    github_issue_number: 12,
    status: 'pending',
    branch: null,
    pr_number: null,
    created_at_ms: now - 1_800_000,
    pipeline: [
      { step: 'plan', role: 'plan', status: 'pending', agent: null, tokens: 0, wall_clock_ms: 0, summary: null },
      { step: 'test', role: 'code', status: 'pending', agent: null, tokens: 0, wall_clock_ms: 0, summary: null },
      { step: 'implement', role: 'code', status: 'pending', agent: null, tokens: 0, wall_clock_ms: 0, summary: null },
      { step: 'review', role: 'review', status: 'pending', agent: null, tokens: 0, wall_clock_ms: 0, summary: null },
    ],
  },
];

// --- Messages ---

export const mockMessages: MockMessage[] = [
  {
    message_id: 'msg1',
    from: 'builder',
    to: 'doc-search',
    mission_id: 'm1',
    body: 'What is the current structure of the colony API routes?',
    created_at_ms: now - 3_500_000,
  },
  {
    message_id: 'msg2',
    from: 'doc-search',
    to: 'builder',
    mission_id: 'm1',
    body: 'Colony routes are in crabitat-control-plane/src/routes/colonies.rs. Endpoints: GET /v1/colonies, GET /v1/colonies/:id, POST /v1/colonies, PATCH /v1/colonies/:id. The detail endpoint returns ColonyRecord with nested crab count.',
    created_at_ms: now - 3_498_000,
  },
  {
    message_id: 'msg3',
    from: 'atlas',
    to: 'doc-search',
    mission_id: 'm1',
    body: 'What test framework is used in the Astro console?',
    created_at_ms: now - 3_580_000,
  },
  {
    message_id: 'msg4',
    from: 'doc-search',
    to: 'atlas',
    mission_id: 'm1',
    body: 'No test framework is currently configured for the Astro console. The Rust crates use cargo test with tokio::test for async tests.',
    created_at_ms: now - 3_578_000,
  },
  {
    message_id: 'msg5',
    from: 'builder',
    to: 'doc-search',
    mission_id: 'm2',
    body: 'Where does the token counting happen in the crab runner?',
    created_at_ms: now - 86_300_000,
  },
  {
    message_id: 'msg6',
    from: 'doc-search',
    to: 'builder',
    mission_id: 'm2',
    body: 'Token counting is in crabitat-crab/src/runner.rs, function parse_claude_output(). It reads the usage field from the top-level JSON response. The bug is likely that Claude sometimes nests usage inside a result object.',
    created_at_ms: now - 86_298_000,
  },
  {
    message_id: 'msg7',
    from: 'critic',
    to: 'builder',
    mission_id: 'm2',
    body: 'Your implementation looks clean. One suggestion: consider adding a fallback parser for the old response format to avoid breaking existing runs.',
    created_at_ms: now - 86_100_000,
  },
  {
    message_id: 'msg8',
    from: 'builder',
    to: 'critic',
    mission_id: 'm2',
    body: 'Good call. Added a try_parse_legacy() fallback. Updated the PR.',
    created_at_ms: now - 86_050_000,
  },
];

// --- Helpers ---

export function getRepoForMission(mission: MockMission): MockRepo | undefined {
  return mockRepos.find((r) => r.repo_id === mission.repo_id);
}

export function getIssuesForRepo(repoId: string): MockIssue[] {
  return mockIssues.filter((i) => i.repo_id === repoId);
}

export function getMissionsForRepo(repoId: string): MockMission[] {
  return mockMissions.filter((m) => m.repo_id === repoId);
}

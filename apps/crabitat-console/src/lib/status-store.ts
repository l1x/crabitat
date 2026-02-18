export type AgentLifecycleState = 'idle' | 'busy' | 'offline';
export type ChunkLifecycleState = 'queued' | 'running' | 'blocked' | 'completed' | 'failed';
export type RepoMode = 'worktree' | 'external_repo' | 'unknown';

export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
}

export interface TimingInfo {
  first_token_ms?: number;
  llm_duration_ms?: number;
  execution_duration_ms?: number;
  end_to_end_ms?: number;
}

export interface AgentStatus {
  agent_id: string;
  agent_name: string;
  role: string;
  state: AgentLifecycleState;
  current_chunk_id: string | null;
  current_task: string | null;
  updated_at: string;
}

export interface ChunkStatus {
  chunk_id: string;
  task_id: string;
  agent_id: string;
  title: string;
  repo_mode: RepoMode;
  status: ChunkLifecycleState;
  progress_message: string;
  summary: string | null;
  token_usage: TokenUsage;
  timing: TimingInfo;
  started_at: string;
  updated_at: string;
  completed_at: string | null;
}

export interface StatusSummary {
  active_agents: number;
  running_chunks: number;
  completed_chunks: number;
  failed_chunks: number;
  total_tokens: number;
  avg_end_to_end_ms: number | null;
}

export interface StatusSnapshot {
  generated_at: string;
  summary: StatusSummary;
  agents: AgentStatus[];
  chunks: ChunkStatus[];
}

interface AgentUpsertInput {
  agent_id: string;
  agent_name: string;
  role: string;
  state: AgentLifecycleState;
  current_chunk_id?: string | null;
  current_task?: string | null;
  updated_at?: string;
}

interface ChunkStartInput {
  chunk_id: string;
  task_id: string;
  agent_id: string;
  title: string;
  repo_mode?: RepoMode;
  status?: ChunkLifecycleState;
  started_at?: string;
}

interface ChunkUpdateInput {
  chunk_id: string;
  status?: ChunkLifecycleState;
  progress_message?: string;
  token_usage?: Partial<TokenUsage>;
  timing?: TimingInfo;
  updated_at?: string;
}

interface ChunkCompleteInput {
  chunk_id: string;
  status: 'completed' | 'failed';
  summary?: string;
  token_usage?: Partial<TokenUsage>;
  timing?: TimingInfo;
  completed_at?: string;
}

const nowIso = (): string => new Date().toISOString();

const emptyTokenUsage = (): TokenUsage => ({
  prompt_tokens: 0,
  completion_tokens: 0,
  total_tokens: 0,
});

class StatusStore {
  private agents = new Map<string, AgentStatus>();
  private chunks = new Map<string, ChunkStatus>();

  upsertAgent(input: AgentUpsertInput): AgentStatus {
    const previous = this.agents.get(input.agent_id);
    const updated: AgentStatus = {
      agent_id: input.agent_id,
      agent_name: input.agent_name,
      role: input.role,
      state: input.state,
      current_chunk_id: input.current_chunk_id ?? previous?.current_chunk_id ?? null,
      current_task: input.current_task ?? previous?.current_task ?? null,
      updated_at: input.updated_at ?? nowIso(),
    };

    this.agents.set(updated.agent_id, updated);
    return updated;
  }

  startChunk(input: ChunkStartInput): ChunkStatus {
    const startedAt = input.started_at ?? nowIso();
    const chunk: ChunkStatus = {
      chunk_id: input.chunk_id,
      task_id: input.task_id,
      agent_id: input.agent_id,
      title: input.title,
      repo_mode: input.repo_mode ?? 'unknown',
      status: input.status ?? 'running',
      progress_message: 'Chunk started',
      summary: null,
      token_usage: emptyTokenUsage(),
      timing: {},
      started_at: startedAt,
      updated_at: startedAt,
      completed_at: null,
    };

    this.chunks.set(chunk.chunk_id, chunk);

    const existingAgent = this.agents.get(chunk.agent_id);
    if (existingAgent) {
      this.upsertAgent({
        ...existingAgent,
        state: 'busy',
        current_chunk_id: chunk.chunk_id,
        current_task: chunk.title,
        updated_at: startedAt,
      });
    }

    return chunk;
  }

  updateChunk(input: ChunkUpdateInput): ChunkStatus {
    const chunk = this.chunks.get(input.chunk_id);
    if (!chunk) {
      throw new Error(`Unknown chunk_id: ${input.chunk_id}`);
    }

    if (input.status) {
      chunk.status = input.status;
    }

    if (input.progress_message !== undefined) {
      chunk.progress_message = input.progress_message;
    }

    if (input.token_usage) {
      chunk.token_usage = {
        prompt_tokens: input.token_usage.prompt_tokens ?? chunk.token_usage.prompt_tokens,
        completion_tokens: input.token_usage.completion_tokens ?? chunk.token_usage.completion_tokens,
        total_tokens:
          input.token_usage.total_tokens ??
          (input.token_usage.prompt_tokens ?? chunk.token_usage.prompt_tokens) +
            (input.token_usage.completion_tokens ?? chunk.token_usage.completion_tokens),
      };
    }

    if (input.timing) {
      chunk.timing = {
        ...chunk.timing,
        ...input.timing,
      };
    }

    chunk.updated_at = input.updated_at ?? nowIso();

    const existingAgent = this.agents.get(chunk.agent_id);
    if (existingAgent) {
      const isTerminal = chunk.status === 'completed' || chunk.status === 'failed';
      this.upsertAgent({
        ...existingAgent,
        state: isTerminal ? 'idle' : 'busy',
        current_chunk_id: isTerminal ? null : chunk.chunk_id,
        current_task: isTerminal ? null : chunk.title,
        updated_at: chunk.updated_at,
      });
    }

    this.chunks.set(chunk.chunk_id, chunk);
    return chunk;
  }

  completeChunk(input: ChunkCompleteInput): ChunkStatus {
    const updated = this.updateChunk({
      chunk_id: input.chunk_id,
      status: input.status,
      token_usage: input.token_usage,
      timing: input.timing,
    });

    updated.summary = input.summary ?? updated.summary;
    updated.completed_at = input.completed_at ?? nowIso();
    updated.updated_at = updated.completed_at;

    const existingAgent = this.agents.get(updated.agent_id);
    if (existingAgent && existingAgent.current_chunk_id === updated.chunk_id) {
      this.upsertAgent({
        ...existingAgent,
        state: 'idle',
        current_chunk_id: null,
        current_task: null,
        updated_at: updated.completed_at,
      });
    }

    this.chunks.set(updated.chunk_id, updated);
    return updated;
  }

  getSnapshot(): StatusSnapshot {
    const agents = [...this.agents.values()].sort((a, b) => a.agent_name.localeCompare(b.agent_name));
    const chunks = [...this.chunks.values()].sort((a, b) => b.updated_at.localeCompare(a.updated_at));

    const completedChunks = chunks.filter((chunk) => chunk.status === 'completed');
    const avgEndToEndMs =
      completedChunks.length === 0
        ? null
        : Math.round(
            completedChunks.reduce((sum, chunk) => sum + (chunk.timing.end_to_end_ms ?? 0), 0) /
              completedChunks.length,
          );

    const summary: StatusSummary = {
      active_agents: agents.filter((agent) => agent.state === 'busy').length,
      running_chunks: chunks.filter((chunk) => chunk.status === 'running').length,
      completed_chunks: completedChunks.length,
      failed_chunks: chunks.filter((chunk) => chunk.status === 'failed').length,
      total_tokens: chunks.reduce((sum, chunk) => sum + chunk.token_usage.total_tokens, 0),
      avg_end_to_end_ms: avgEndToEndMs,
    };

    return {
      generated_at: nowIso(),
      summary,
      agents,
      chunks,
    };
  }
}

const storeKey = '__meshStatusStore__';
const globalScope = globalThis as typeof globalThis & { [storeKey]?: StatusStore };

if (!globalScope[storeKey]) {
  globalScope[storeKey] = new StatusStore();
}

export const statusStore = globalScope[storeKey];

export interface Repo {
  repo_id: string;
  owner: string;
  name: string;
  local_path: string;
  created_at: string;
}

export interface CreateRepoRequest {
  owner: string;
  name: string;
  local_path: string;
}

export interface Issue {
  repo_id: string;
  number: number;
  title: string;
  body: string | null;
  labels: string[];
  state: string;
  fetched_at: string;
}

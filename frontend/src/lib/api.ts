export interface DashboardStats {
  total_jobs: number;
  jobs_today: number;
  success_rate: number;
  queued_count: number;
  running_count: number;
}

export interface Job {
  id: number;
  repo_owner: string;
  repo_name: string;
  git_sha: string;
  git_ref: string;
  status: "queued" | "running" | "success" | "failed" | "cancelled";
  created_at: string;
  started_at?: string;
  finished_at?: string;
  commit_message?: string;
  commit_author?: string;
  commit_url?: string;
  duration_secs?: number;
  trigger_type?: "push" | "pull_request" | "manual";

  // Extended fields
  before_sha?: string;
  compare_url?: string;
  commits_count?: number;
  forced?: boolean;
  pusher_name?: string;
  sender_login?: string;
  sender_avatar_url?: string;
}

export interface StageMetrics {
  name: string;
  status: string;
  duration_ms: number;
  exit_code?: number;
}

export interface JobMetrics {
  clone_duration_ms: number;
  build_duration_ms?: number;
  stages: StageMetrics[];
  total_duration_ms: number;
}

export interface JobDetail extends Job {
  logs: LogEntry[];
  pr_number?: number;
  pr_title?: string;
  pr_url?: string;
  metrics?: JobMetrics;
}

export interface LogEntry {
  timestamp: string;
  message: string;
  level: string;
}

export interface Repo {
  id: number;
  owner: string;
  name: string;
  build_count: number;
  success_count: number;
  failure_count: number;
  last_build_at?: string;
  last_status?: string;
  html_url?: string;
  description?: string;
  language?: string;
}

export interface RepoDetail {
  id: number;
  owner: string;
  name: string;
  full_name?: string;
  html_url?: string;
  description?: string;
  language?: string;
  default_branch?: string;
  private: boolean;
  build_count: number;
  success_count: number;
  failure_count: number;
  last_build_at?: string;
  created_at: string;
}

const API_BASE = "/api";

/**
 * Generic API request wrapper that handles fetch, response checking, and error logging.
 */
async function apiRequest<T>(url: string, options?: RequestInit): Promise<T> {
  const method = options?.method ?? "GET";
  const res = await fetch(url, options);
  if (!res.ok) {
    console.error(`API error: ${method} ${url} returned ${res.status}`);
    throw new Error(`API error ${res.status}: ${method} ${url}`);
  }
  return res.json();
}

/** For requests that don't return JSON (delete, stop, etc.) */
async function apiRequestVoid(
  url: string,
  options?: RequestInit,
): Promise<void> {
  const method = options?.method ?? "GET";
  const res = await fetch(url, options);
  if (!res.ok) {
    console.error(`API error: ${method} ${url} returned ${res.status}`);
    throw new Error(`API error ${res.status}: ${method} ${url}`);
  }
}

export async function fetchStats(): Promise<DashboardStats> {
  return apiRequest<DashboardStats>(`${API_BASE}/stats`);
}

export async function fetchJobs(limit = 50): Promise<Job[]> {
  return apiRequest<Job[]>(`${API_BASE}/jobs?limit=${limit}`);
}

export async function fetchJob(id: number): Promise<JobDetail | null> {
  return apiRequest<JobDetail | null>(`${API_BASE}/job/${id}`);
}

export async function fetchRepos(): Promise<Repo[]> {
  return apiRequest<Repo[]>(`${API_BASE}/repos`);
}

export async function fetchRepo(id: number): Promise<RepoDetail> {
  return apiRequest<RepoDetail>(`${API_BASE}/repo/${id}`);
}

export async function fetchRepoJobs(id: number, limit = 50): Promise<Job[]> {
  return apiRequest<Job[]>(`${API_BASE}/repo/${id}/jobs?limit=${limit}`);
}

export interface Schedule {
  id: number;
  repo_id: number;
  repo_owner: string;
  repo_name: string;
  cron_expression: string;
  branch: string;
  timezone: string;
  enabled: boolean;
  last_run_at?: string;
  next_run_at?: string;
}

export async function fetchSchedules(): Promise<Schedule[]> {
  return apiRequest<Schedule[]>(`${API_BASE}/schedules`);
}

export async function toggleSchedule(
  id: number,
  enabled: boolean,
): Promise<void> {
  return apiRequestVoid(`${API_BASE}/schedule/${id}/toggle`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ enabled }),
  });
}

export async function deleteSchedule(id: number): Promise<void> {
  return apiRequestVoid(`${API_BASE}/schedule/${id}`, { method: "DELETE" });
}

// Docker Container Types and API

export interface Container {
  id: string;
  name: string;
  image: string;
  status: string;
  state: string;
  created: string;
  ports: string;
  project?: string;
}

export interface ContainerLogs {
  container_id: string;
  logs: string[];
}

export async function fetchContainers(project?: string): Promise<Container[]> {
  const url = project
    ? `${API_BASE}/containers?project=${encodeURIComponent(project)}`
    : `${API_BASE}/containers`;
  return apiRequest<Container[]>(url);
}

export async function fetchContainerLogs(
  containerId: string,
  lines = 100,
): Promise<ContainerLogs> {
  return apiRequest<ContainerLogs>(
    `${API_BASE}/containers/${containerId}/logs?lines=${lines}`,
  );
}

export function streamContainerLogs(
  containerId: string,
  onLog: (line: string) => void,
  onError?: (error: Error) => void,
  lines = 100,
): () => void {
  const eventSource = new EventSource(
    `${API_BASE}/containers/${containerId}/logs/stream?lines=${lines}`,
  );

  eventSource.onmessage = (event) => {
    onLog(event.data);
  };

  eventSource.onerror = () => {
    if (onError) {
      onError(new Error("Log stream connection failed"));
    }
    eventSource.close();
  };

  // Return cleanup function
  return () => eventSource.close();
}

export async function restartContainer(containerId: string): Promise<void> {
  return apiRequestVoid(`${API_BASE}/containers/${containerId}/restart`, {
    method: "POST",
  });
}

export async function stopContainer(containerId: string): Promise<void> {
  return apiRequestVoid(`${API_BASE}/containers/${containerId}/stop`, {
    method: "POST",
  });
}

export async function startContainer(containerId: string): Promise<void> {
  return apiRequestVoid(`${API_BASE}/containers/${containerId}/start`, {
    method: "POST",
  });
}

export async function fetchProjects(): Promise<string[]> {
  return apiRequest<string[]>(`${API_BASE}/projects`);
}

export async function restartProject(projectName: string): Promise<void> {
  return apiRequestVoid(
    `${API_BASE}/projects/${encodeURIComponent(projectName)}/restart`,
    { method: "POST" },
  );
}

export async function stopProject(projectName: string): Promise<void> {
  return apiRequestVoid(
    `${API_BASE}/projects/${encodeURIComponent(projectName)}/stop`,
    { method: "POST" },
  );
}

export async function startProject(projectName: string): Promise<void> {
  return apiRequestVoid(
    `${API_BASE}/projects/${encodeURIComponent(projectName)}/start`,
    { method: "POST" },
  );
}

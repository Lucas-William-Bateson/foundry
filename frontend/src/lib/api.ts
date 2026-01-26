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

const API_BASE = "/api";

export async function fetchStats(): Promise<DashboardStats> {
  const res = await fetch(`${API_BASE}/stats`);
  if (!res.ok) throw new Error("Failed to fetch stats");
  return res.json();
}

export async function fetchJobs(limit = 50): Promise<Job[]> {
  const res = await fetch(`${API_BASE}/jobs?limit=${limit}`);
  if (!res.ok) throw new Error("Failed to fetch jobs");
  return res.json();
}

export async function fetchJob(id: number): Promise<JobDetail | null> {
  const res = await fetch(`${API_BASE}/job/${id}`);
  if (!res.ok) throw new Error("Failed to fetch job");
  return res.json();
}

export async function fetchRepos(): Promise<Repo[]> {
  const res = await fetch(`${API_BASE}/repos`);
  if (!res.ok) throw new Error("Failed to fetch repos");
  return res.json();
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
  const res = await fetch(`${API_BASE}/schedules`);
  if (!res.ok) throw new Error("Failed to fetch schedules");
  return res.json();
}

export async function toggleSchedule(
  id: number,
  enabled: boolean,
): Promise<void> {
  const res = await fetch(`${API_BASE}/schedule/${id}/toggle`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ enabled }),
  });
  if (!res.ok) throw new Error("Failed to toggle schedule");
}

export async function deleteSchedule(id: number): Promise<void> {
  const res = await fetch(`${API_BASE}/schedule/${id}`, {
    method: "DELETE",
  });
  if (!res.ok) throw new Error("Failed to delete schedule");
}

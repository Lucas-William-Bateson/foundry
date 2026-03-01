import { useEffect, useState, useCallback } from "react";
import { Link } from "react-router-dom";
import Grid from "@mui/material/Grid";
import { Card, CardContent, CardHeader } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { StatusBadge } from "@/components/ui/StatusBadge";
import {
  fetchStats,
  fetchJobs,
  type DashboardStats,
  type Job,
} from "@/lib/api";
import { formatRelativeTime, formatDuration } from "@/lib/utils";
import { usePolling } from "@/lib/hooks";
import ShowChart from "@mui/icons-material/ShowChart";
import CheckCircle from "@mui/icons-material/CheckCircle";
import AccessTime from "@mui/icons-material/AccessTime";
import CommitIcon from "@mui/icons-material/Commit";
import Autorenew from "@mui/icons-material/Autorenew";
import { ErrorBoundary } from "@/components/ErrorBoundary";

const statusColors: Record<string, string> = {
  success: "#2D9D5E",
  failed: "#D44B4B",
  running: "#C89520",
  queued: "#8B8F96",
  cancelled: "#8B8F96",
};

export function Dashboard() {
  const [stats, setStats] = useState<DashboardStats | null>(null);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const [statsData, jobsData] = await Promise.all([
        fetchStats(),
        fetchJobs(20),
      ]);
      setStats(statsData);
      setJobs(jobsData);
    } catch (e) {
      console.error("Failed to load dashboard:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);
  usePolling(load, 5000);

  if (loading) {
    return (
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "center",
          height: "16rem",
        }}
      >
        <Autorenew
          className="spin-animation"
          style={{ fontSize: 32, color: "#8B8F96" }}
        />
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
      <div
        style={{
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
        }}
      >
        <h1
          style={{
            fontSize: "1.75rem",
            fontWeight: 700,
            letterSpacing: "-0.02em",
          }}
        >
          Dashboard
        </h1>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: "0.5rem",
            fontSize: "0.6875rem",
            color: "#8B8F96",
            fontWeight: 500,
            letterSpacing: "0.04em",
            textTransform: "uppercase",
          }}
        >
          <span
            style={{
              position: "relative",
              display: "inline-flex",
              width: "5px",
              height: "5px",
            }}
          >
            <span
              className="ping-animation"
              style={{
                position: "absolute",
                display: "inline-flex",
                width: "100%",
                height: "100%",
                borderRadius: "50%",
                backgroundColor: "#27875A",
                opacity: 0.75,
              }}
            />
            <span
              style={{
                position: "relative",
                display: "inline-flex",
                width: "5px",
                height: "5px",
                borderRadius: "50%",
                backgroundColor: "#27875A",
              }}
            />
          </span>
          Live
        </div>
      </div>

      {/* Stats Grid — asymmetric: hero left, 3 compact right */}
      <ErrorBoundary section="Dashboard Stats">
        <Grid container spacing={1.5}>
          <Grid size={{ xs: 12, sm: 6, md: 5 }}>
            <Card style={{ height: "100%" }}>
              <CardHeader>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                  }}
                >
                  <span className="metric-label">Total Builds</span>
                  <ShowChart style={{ fontSize: 14, color: "#8B8F96" }} />
                </div>
              </CardHeader>
              <CardContent>
                <div className="metric-value" style={{ fontSize: "3rem" }}>
                  {stats?.total_jobs ?? 0}
                </div>
                <div
                  style={{
                    fontSize: "0.75rem",
                    color: "#8B8F96",
                    marginTop: "0.75rem",
                  }}
                >
                  {stats?.jobs_today ?? 0} today
                </div>
              </CardContent>
            </Card>
          </Grid>
          <Grid size={{ xs: 12, sm: 6, md: 7 }}>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: "0.375rem",
                height: "100%",
              }}
            >
              <Card style={{ flex: 1 }}>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "0.75rem 1rem",
                  }}
                >
                  <span className="metric-label">Success Rate</span>
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "0.5rem",
                    }}
                  >
                    <span
                      className="metric-value"
                      style={{ fontSize: "1.5rem", color: "#4A9D6E" }}
                    >
                      {stats?.success_rate?.toFixed(1) ?? 0}%
                    </span>
                    <CheckCircle style={{ fontSize: 14, color: "#4A9D6E" }} />
                  </div>
                </div>
              </Card>
              <Card style={{ flex: 1 }}>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "0.75rem 1rem",
                  }}
                >
                  <span className="metric-label">Today</span>
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "0.5rem",
                    }}
                  >
                    <span
                      className="metric-value"
                      style={{ fontSize: "1.5rem" }}
                    >
                      {stats?.jobs_today ?? 0}
                    </span>
                    <AccessTime style={{ fontSize: 14, color: "#8B8F96" }} />
                  </div>
                </div>
              </Card>
              <Card style={{ flex: 1 }}>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "0.75rem 1rem",
                  }}
                >
                  <span className="metric-label">In Queue</span>
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "0.5rem",
                    }}
                  >
                    <span
                      className="metric-value"
                      style={{ fontSize: "1.5rem", color: "#C89520" }}
                    >
                      {(stats?.queued_count ?? 0) + (stats?.running_count ?? 0)}
                    </span>
                    <Autorenew style={{ fontSize: 14, color: "#C89520" }} />
                  </div>
                </div>
              </Card>
            </div>
          </Grid>
        </Grid>
      </ErrorBoundary>

      {/* Recent Builds */}
      <ErrorBoundary section="Recent Builds">
        <div>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "space-between",
              marginBottom: "0.75rem",
            }}
          >
            <h2 style={{ fontSize: "0.8125rem", fontWeight: 600, margin: 0 }}>Recent Builds</h2>
            <span className="metric-label">{jobs.length} builds</span>
          </div>
          <div style={{ borderTop: "1px solid rgba(255, 255, 255, 0.06)" }}>
            <ScrollArea style={{ maxHeight: "560px" }}>
              {jobs.length === 0 ? (
                <div
                  style={{
                    textAlign: "center",
                    padding: "3rem",
                    color: "#8B8F96",
                  }}
                >
                  No builds yet. Push a commit to get started!
                </div>
              ) : (
                <div style={{ display: "flex", flexDirection: "column" }}>
                  {jobs.map((job) => (
                    <Link
                      key={job.id}
                      to={`/job/${job.id}`}
                      className="build-row"
                      style={{
                        display: "flex",
                        alignItems: "center",
                        justifyContent: "space-between",
                        padding: "0.625rem 1rem 0.625rem 1.25rem",
                        borderBottom: "1px solid rgba(255, 255, 255, 0.04)",
                        textDecoration: "none",
                        color: "inherit",
                        ["--status-color" as string]:
                          statusColors[job.status] || "#8B8F96",
                      }}
                    >
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: "0.5rem",
                          minWidth: 0,
                        }}
                      >
                        <div
                          style={{
                            display: "flex",
                            flexDirection: "column",
                            minWidth: 0,
                          }}
                        >
                          <span
                            style={{ fontWeight: 500, fontSize: "0.875rem" }}
                          >
                            {job.repo_owner}/{job.repo_name}
                          </span>
                          <div
                            style={{
                              display: "flex",
                              alignItems: "center",
                              gap: "0.5rem",
                              fontSize: "0.75rem",
                              color: "#8B8F96",
                              marginTop: "2px",
                            }}
                          >
                            <CommitIcon style={{ fontSize: 12 }} />
                            <code
                              style={{
                                fontSize: "0.6875rem",
                                fontFamily: "'JetBrains Mono', monospace",
                              }}
                            >
                              {job.git_sha.substring(0, 7)}
                            </code>
                            {job.commit_message && (
                              <span
                                style={{
                                  overflow: "hidden",
                                  textOverflow: "ellipsis",
                                  whiteSpace: "nowrap",
                                  maxWidth: "300px",
                                }}
                              >
                                {job.commit_message}
                              </span>
                            )}
                          </div>
                        </div>
                      </div>
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: "1rem",
                          flexShrink: 0,
                        }}
                      >
                        <div
                          style={{
                            textAlign: "right",
                            fontVariantNumeric: "tabular-nums",
                          }}
                        >
                          <div style={{ fontSize: "0.75rem", color: "#B0B3B8", fontWeight: 500 }}>{formatDuration(job.duration_secs)}</div>
                          <div
                            style={{ fontSize: "0.625rem", color: "#666", marginTop: "1px" }}
                          >
                            {formatRelativeTime(job.created_at)}
                          </div>
                        </div>
                        <StatusBadge status={job.status} />
                      </div>
                    </Link>
                  ))}
                </div>
              )}
            </ScrollArea>
          </div>
        </div>
      </ErrorBoundary>
    </div>
  );
}

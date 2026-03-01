import { Link } from "react-router-dom";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { type Job } from "@/lib/api";
import { formatRelativeTime, formatDuration } from "@/lib/utils";
import { StatusIcon } from "@/components/ui/StatusBadge";
import AccessTime from "@mui/icons-material/AccessTime";
import CommitIcon from "@mui/icons-material/Commit";

const statusColors: Record<string, string> = {
  success: "#2D9D5E",
  failed: "#D44B4B",
  running: "#C89520",
  queued: "#8B8F96",
  cancelled: "#8B8F96",
};

interface RepoJobsListProps {
  jobs: Job[];
}

export function RepoJobsList({ jobs }: RepoJobsListProps) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Recent Builds</CardTitle>
      </CardHeader>
      <CardContent style={{ padding: 0 }}>
        {jobs.length === 0 ? (
          <p
            style={{ color: "#8B8F96", textAlign: "center", padding: "2rem 0" }}
          >
            No builds yet
          </p>
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
                  padding: "0.75rem 1rem 0.75rem 1.25rem",
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
                    gap: "0.75rem",
                  }}
                >
                  <StatusIcon status={job.status} />
                  <div>
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "0.5rem",
                      }}
                    >
                      <CommitIcon style={{ fontSize: 14, color: "#8B8F96" }} />
                      <span
                        style={{
                          fontFamily: "'JetBrains Mono', monospace",
                          fontSize: "0.8125rem",
                        }}
                      >
                        {job.git_sha.substring(0, 7)}
                      </span>
                    </div>
                    {job.commit_message && (
                      <p
                        style={{
                          fontSize: "0.8125rem",
                          color: "#8B8F96",
                          overflow: "hidden",
                          textOverflow: "ellipsis",
                          whiteSpace: "nowrap",
                          maxWidth: "28rem",
                          marginTop: "2px",
                        }}
                      >
                        {job.commit_message}
                      </p>
                    )}
                  </div>
                </div>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "1rem",
                    fontSize: "0.8125rem",
                    color: "#8B8F96",
                    fontVariantNumeric: "tabular-nums",
                  }}
                >
                  {job.duration_secs && (
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: "0.25rem",
                      }}
                    >
                      <AccessTime style={{ fontSize: 14 }} />
                      {formatDuration(job.duration_secs)}
                    </div>
                  )}
                  <span style={{ fontSize: "0.75rem" }}>
                    {formatRelativeTime(job.created_at)}
                  </span>
                </div>
              </Link>
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

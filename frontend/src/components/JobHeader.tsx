import { Link } from "react-router-dom";
import Grid from "@mui/material/Grid";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { statusConfig, type JobStatus } from "@/components/ui/StatusBadge";
import { cn } from "@/lib/utils";
import { formatDuration } from "@/lib/utils";
import type { JobDetail } from "@/lib/api";
import ArrowBack from "@mui/icons-material/ArrowBack";
import CommitIcon from "@mui/icons-material/Commit";
import AccountTree from "@mui/icons-material/AccountTree";
import MergeType from "@mui/icons-material/MergeType";
import Person from "@mui/icons-material/Person";
import OpenInNew from "@mui/icons-material/OpenInNew";
import TimerIcon from "@mui/icons-material/Timer";

interface JobHeaderProps {
  readonly job: JobDetail;
}

export function JobHeader({ job }: JobHeaderProps) {
  const config = statusConfig[job.status as JobStatus];
  const StatusIconComponent = config.icon;

  return (
    <>
      <div style={{ display: "flex", alignItems: "center", gap: "1rem" }}>
        <Link to="/" style={{ color: "#8B8F96", display: "inline-flex" }}>
          <ArrowBack style={{ fontSize: 18 }} />
        </Link>
        <div style={{ flex: 1 }}>
          <h1
            style={{
              fontSize: "1.5rem",
              fontWeight: 700,
              letterSpacing: "-0.02em",
            }}
          >
            Build #{job.id}
          </h1>
          <p style={{ color: "#8B8F96", fontSize: "0.8125rem", marginTop: "4px" }}>
            {job.repo_owner}/{job.repo_name}
          </p>
        </div>
        <Button
          variant="outline"
          onClick={() =>
            window.open(
              `https://github.com/${job.repo_owner}/${job.repo_name}/commit/${job.git_sha}`,
              "_blank",
            )
          }
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: "0.5rem",
          }}
        >
          <OpenInNew style={{ fontSize: 14 }} />
          View on GitHub
        </Button>
        <div
          className={config.bg}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "0.5rem",
            padding: "0.375rem 0.75rem",
            borderRadius: "4px",
          }}
        >
          <StatusIconComponent
            fontSize="small"
            className={cn(
              config.color,
              job.status === "running" && "spin-animation",
            )}
          />
          <span
            className={config.color}
            style={{
              fontWeight: 600,
              textTransform: "capitalize",
              fontSize: "0.8125rem",
            }}
          >
            {job.status}
          </span>
        </div>
      </div>

      {/* Metadata Grid */}
      <Grid container spacing={1.5}>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card style={{ backgroundColor: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}>
            <CardHeader style={{ paddingBottom: "0.25rem" }}>
              <CardTitle
                style={{
                  fontSize: "0.6875rem",
                  fontWeight: 500,
                  color: "#8B8F96",
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                  letterSpacing: "0.04em",
                  textTransform: "uppercase",
                }}
              >
                <CommitIcon style={{ fontSize: 14 }} />
                Commit
              </CardTitle>
            </CardHeader>
            <CardContent>
              <code
                style={{
                  fontSize: "0.875rem",
                  fontFamily: "'JetBrains Mono', monospace",
                }}
              >
                {job.git_sha.substring(0, 7)}
              </code>
              {job.commit_url && (
                <a
                  href={job.commit_url}
                  target="_blank"
                  rel="noopener noreferrer"
                  style={{
                    marginLeft: "0.5rem",
                    color: "#C65D00",
                    display: "inline-flex",
                    alignItems: "center",
                    gap: "0.25rem",
                  }}
                >
                  <OpenInNew style={{ fontSize: 12 }} />
                </a>
              )}
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card style={{ backgroundColor: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}>
            <CardHeader style={{ paddingBottom: "0.5rem" }}>
              <CardTitle
                style={{
                  fontSize: "0.875rem",
                  fontWeight: 500,
                  color: "#8B8F96",
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                }}
              >
                <AccountTree style={{ fontSize: 16 }} />
                Branch
              </CardTitle>
            </CardHeader>
            <CardContent>
              <span style={{ fontSize: "0.875rem" }}>
                {job.git_ref.replace("refs/heads/", "")}
              </span>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card style={{ backgroundColor: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}>
            <CardHeader style={{ paddingBottom: "0.5rem" }}>
              <CardTitle
                style={{
                  fontSize: "0.875rem",
                  fontWeight: 500,
                  color: "#8B8F96",
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                }}
              >
                <Person style={{ fontSize: 16 }} />
                Author
              </CardTitle>
            </CardHeader>
            <CardContent>
              <span style={{ fontSize: "0.875rem" }}>
                {job.commit_author || job.pusher_name || "-"}
              </span>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card style={{ backgroundColor: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}>
            <CardHeader style={{ paddingBottom: "0.5rem" }}>
              <CardTitle
                style={{
                  fontSize: "0.875rem",
                  fontWeight: 500,
                  color: "#8B8F96",
                  display: "flex",
                  alignItems: "center",
                  gap: "0.5rem",
                }}
              >
                <TimerIcon style={{ fontSize: 16 }} />
                Duration
              </CardTitle>
            </CardHeader>
            <CardContent>
              <span style={{ fontSize: "0.875rem" }}>
                {formatDuration(job.duration_secs)}
              </span>
            </CardContent>
          </Card>
        </Grid>
      </Grid>

      {/* Commit Message */}
      {job.commit_message && (
        <Card style={{ backgroundColor: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}>
          <CardHeader style={{ paddingBottom: "0.5rem" }}>
            <CardTitle style={{ fontSize: "0.875rem" }}>
              Commit Message
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p style={{ fontSize: "0.875rem", whiteSpace: "pre-wrap" }}>
              {job.commit_message}
            </p>
          </CardContent>
        </Card>
      )}

      {job.pr_number && (
        <Card style={{ backgroundColor: "rgba(255,255,255,0.015)", border: "1px solid rgba(255,255,255,0.03)" }}>
          <CardHeader style={{ paddingBottom: "0.5rem" }}>
            <CardTitle
              style={{
                fontSize: "0.875rem",
                display: "flex",
                alignItems: "center",
                gap: "0.5rem",
              }}
            >
              <MergeType style={{ fontSize: 16 }} />
              Pull Request #{job.pr_number}
            </CardTitle>
          </CardHeader>
          <CardContent>
            <p style={{ fontSize: "0.875rem" }}>{job.pr_title}</p>
            {job.pr_url && (
              <a
                href={job.pr_url}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  color: "#C65D00",
                  fontSize: "0.875rem",
                  display: "inline-flex",
                  alignItems: "center",
                  gap: "0.25rem",
                  marginTop: "0.25rem",
                }}
              >
                View on GitHub <OpenInNew style={{ fontSize: 12 }} />
              </a>
            )}
          </CardContent>
        </Card>
      )}
    </>
  );
}

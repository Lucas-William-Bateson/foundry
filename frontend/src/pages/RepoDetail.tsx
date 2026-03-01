import { useEffect, useState, useCallback } from "react";
import { useParams, Link } from "react-router-dom";
import Grid from "@mui/material/Grid";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  fetchRepo,
  fetchRepoJobs,
  fetchContainers,
  restartProject,
  type RepoDetail,
  type Job,
  type Container,
} from "@/lib/api";
import { ContainerList } from "@/components/ContainerList";
import { LogViewer } from "@/components/LogViewer";
import { RepoJobsList } from "@/components/RepoJobsList";
import { formatRelativeTime } from "@/lib/utils";
import AccountTree from "@mui/icons-material/AccountTree";
import OpenInNew from "@mui/icons-material/OpenInNew";
import Autorenew from "@mui/icons-material/Autorenew";
import ArrowBack from "@mui/icons-material/ArrowBack";
import Lock from "@mui/icons-material/Lock";
import Public from "@mui/icons-material/Public";
import RestartAlt from "@mui/icons-material/RestartAlt";
import Inventory2 from "@mui/icons-material/Inventory2";
import { ErrorBoundary } from "@/components/ErrorBoundary";

export function RepoDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [repo, setRepo] = useState<RepoDetail | null>(null);
  const [jobs, setJobs] = useState<Job[]>([]);
  const [containers, setContainers] = useState<Container[]>([]);
  const [selectedContainer, setSelectedContainer] = useState<Container | null>(null);
  const [loading, setLoading] = useState(true);
  const [restartingProject, setRestartingProject] = useState(false);

  const loadContainers = useCallback(async (projectName: string) => {
    try {
      const containerData = await fetchContainers(projectName);
      setContainers(containerData);
    } catch (e) {
      console.error("Failed to load containers:", e);
    }
  }, []);

  useEffect(() => {
    const load = async () => {
      if (!id) return;
      try {
        const [repoData, jobsData] = await Promise.all([
          fetchRepo(Number(id)),
          fetchRepoJobs(Number(id)),
        ]);
        setRepo(repoData);
        setJobs(jobsData);
        if (repoData.name) {
          loadContainers(repoData.name);
        }
      } catch (e) {
        console.error("Failed to load repo:", e);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, [id, loadContainers]);

  const handleRestartProject = async () => {
    if (!repo) return;
    setRestartingProject(true);
    try {
      await restartProject(repo.name);
      loadContainers(repo.name);
    } catch (e) {
      console.error("Failed to restart project:", e);
    } finally {
      setRestartingProject(false);
    }
  };

  if (loading) {
    return (
      <div style={{ display: "flex", alignItems: "center", justifyContent: "center", height: "16rem" }}>
        <Autorenew className="spin-animation" style={{ fontSize: 32, color: "#8B8F96" }} />
      </div>
    );
  }

  if (!repo) {
    return (
      <div style={{ textAlign: "center", padding: "3rem 0" }}>
        <h2 style={{ fontSize: "1.5rem", fontWeight: 700, color: "#8B8F96" }}>
          Repository not found
        </h2>
        <Link to="/repos" style={{ color: "#C65D00", marginTop: "1rem", display: "block" }}>
          Back to repositories
        </Link>
      </div>
    );
  }

  const successRate =
    repo.build_count > 0
      ? ((repo.success_count / repo.build_count) * 100).toFixed(1)
      : "0";

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
      {/* Back link */}
      <Link
        to="/repos"
        style={{ display: "inline-flex", alignItems: "center", gap: "0.5rem", color: "#8B8F96", textDecoration: "none" }}
      >
        <ArrowBack style={{ fontSize: 16 }} />
        Back to repositories
      </Link>

      {/* Header */}
      <div style={{ display: "flex", alignItems: "flex-start", justifyContent: "space-between" }}>
        <div>
          <div style={{ display: "flex", alignItems: "center", gap: "0.75rem" }}>
            <AccountTree style={{ fontSize: 32, color: "#C65D00" }} />
            <div>
              <h1 style={{ fontSize: "2rem", fontWeight: 700, letterSpacing: "-0.02em", fontVariantNumeric: "tabular-nums" }}>{repo.name}</h1>
              <p style={{ color: "#8B8F96" }}>{repo.owner}</p>
            </div>
            {repo.private ? (
              <Badge variant="secondary" style={{ marginLeft: "0.5rem" }}>
                <Lock style={{ fontSize: 12, marginRight: "0.25rem" }} />
                Private
              </Badge>
            ) : (
              <Badge variant="outline" style={{ marginLeft: "0.5rem" }}>
                <Public style={{ fontSize: 12, marginRight: "0.25rem" }} />
                Public
              </Badge>
            )}
          </div>
          {repo.description && (
            <p style={{ color: "#8B8F96", marginTop: "0.5rem", maxWidth: "42rem" }}>
              {repo.description}
            </p>
          )}
        </div>
        {repo.html_url && (
          <Button
            variant="outline"
            size="sm"
            style={{ display: "inline-flex", alignItems: "center", gap: "0.5rem", textDecoration: "none" }}
            onClick={() => window.open(repo.html_url, '_blank')}
          >
            View on GitHub
            <OpenInNew style={{ fontSize: 16 }} />
          </Button>
        )}
      </div>

      {/* Stats cards */}
      <Grid container spacing={2}>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card>
            <CardHeader style={{ paddingBottom: "0.5rem" }}>
              <CardTitle style={{ fontSize: "0.6875rem", fontWeight: 500, color: "#8B8F96", letterSpacing: "0.04em", textTransform: "uppercase" }}>
                Total Builds
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div style={{ fontSize: "2rem", fontWeight: 700, letterSpacing: "-0.02em", fontVariantNumeric: "tabular-nums" }}>{repo.build_count}</div>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card>
            <CardHeader style={{ paddingBottom: "0.5rem" }}>
              <CardTitle style={{ fontSize: "0.6875rem", fontWeight: 500, color: "#8B8F96", letterSpacing: "0.04em", textTransform: "uppercase" }}>
                Success Rate
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div style={{ fontSize: "2rem", fontWeight: 700, letterSpacing: "-0.02em", fontVariantNumeric: "tabular-nums", color: "#2D9D5E" }}>
                {successRate}%
              </div>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card>
            <CardHeader style={{ paddingBottom: "0.5rem" }}>
              <CardTitle style={{ fontSize: "0.6875rem", fontWeight: 500, color: "#8B8F96", letterSpacing: "0.04em", textTransform: "uppercase" }}>
                Passed
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div style={{ fontSize: "2rem", fontWeight: 700, letterSpacing: "-0.02em", fontVariantNumeric: "tabular-nums", color: "#2D9D5E" }}>
                {repo.success_count}
              </div>
            </CardContent>
          </Card>
        </Grid>
        <Grid size={{ xs: 12, sm: 6, md: 3 }}>
          <Card>
            <CardHeader style={{ paddingBottom: "0.5rem" }}>
              <CardTitle style={{ fontSize: "0.6875rem", fontWeight: 500, color: "#8B8F96", letterSpacing: "0.04em", textTransform: "uppercase" }}>
                Failed
              </CardTitle>
            </CardHeader>
            <CardContent>
              <div style={{ fontSize: "2rem", fontWeight: 700, letterSpacing: "-0.02em", fontVariantNumeric: "tabular-nums", color: "#D44B4B" }}>
                {repo.failure_count}
              </div>
            </CardContent>
          </Card>
        </Grid>
      </Grid>

      {/* Containers section */}
      <ErrorBoundary section="Containers">
        {containers.length > 0 && (
          <Card>
            <CardHeader style={{ display: "flex", flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}>
              <CardTitle style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                <Inventory2 style={{ fontSize: 20 }} />
                Containers
              </CardTitle>
              <Button
                variant="outline"
                size="sm"
                onClick={handleRestartProject}
                disabled={restartingProject}
              >
                {restartingProject ? (
                  <Autorenew className="spin-animation" style={{ fontSize: 16, marginRight: "0.5rem" }} />
                ) : (
                  <RestartAlt style={{ fontSize: 16, marginRight: "0.5rem" }} />
                )}
                Restart All
              </Button>
            </CardHeader>
            <CardContent>
              {selectedContainer ? (
                <LogViewer
                  container={selectedContainer}
                  onClose={() => setSelectedContainer(null)}
                />
              ) : (
                <ContainerList
                  containers={containers}
                  onViewLogs={setSelectedContainer}
                  onRefresh={() => repo && loadContainers(repo.name)}
                />
              )}
            </CardContent>
          </Card>
        )}
      </ErrorBoundary>

      {/* Repo info */}
      <Card>
        <CardHeader>
          <CardTitle>Repository Info</CardTitle>
        </CardHeader>
        <CardContent>
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: "1rem" }}>
            {repo.language && (
              <div>
                <dt style={{ fontSize: "0.875rem", color: "#8B8F96" }}>Language</dt>
                <dd style={{ fontWeight: 500 }}>{repo.language}</dd>
              </div>
            )}
            {repo.default_branch && (
              <div>
                <dt style={{ fontSize: "0.875rem", color: "#8B8F96" }}>Default Branch</dt>
                <dd style={{ fontWeight: 500 }}>{repo.default_branch}</dd>
              </div>
            )}
            {repo.last_build_at && (
              <div>
                <dt style={{ fontSize: "0.875rem", color: "#8B8F96" }}>Last Build</dt>
                <dd style={{ fontWeight: 500 }}>{formatRelativeTime(repo.last_build_at)}</dd>
              </div>
            )}
            <div>
              <dt style={{ fontSize: "0.875rem", color: "#8B8F96" }}>Created</dt>
              <dd style={{ fontWeight: 500 }}>{formatRelativeTime(repo.created_at)}</dd>
            </div>
          </div>
        </CardContent>
      </Card>

      {/* Recent builds */}
      <ErrorBoundary section="Recent Builds">
        <RepoJobsList jobs={jobs} />
      </ErrorBoundary>
    </div>
  );
}

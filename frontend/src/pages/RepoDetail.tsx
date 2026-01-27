import { useEffect, useState, useCallback } from "react";
import { useParams, Link } from "react-router-dom";
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
import { formatRelativeTime } from "@/lib/utils";
import {
  GitBranch,
  ExternalLink,
  Loader2,
  CheckCircle2,
  XCircle,
  Clock,
  GitCommit,
  ArrowLeft,
  Lock,
  Globe,
  RotateCw,
  Box,
} from "lucide-react";

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

        // Try to load containers for this project (using repo name as project name)
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
      // Refresh containers after restart
      loadContainers(repo.name);
    } catch (e) {
      console.error("Failed to restart project:", e);
    } finally {
      setRestartingProject(false);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (!repo) {
    return (
      <div className="text-center py-12">
        <h2 className="text-2xl font-bold text-muted-foreground">
          Repository not found
        </h2>
        <Link to="/repos" className="text-primary hover:underline mt-4 block">
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
    <div className="space-y-6">
      {/* Back link */}
      <Link
        to="/repos"
        className="inline-flex items-center gap-2 text-muted-foreground hover:text-foreground transition-colors"
      >
        <ArrowLeft className="h-4 w-4" />
        Back to repositories
      </Link>

      {/* Header */}
      <div className="flex items-start justify-between">
        <div>
          <div className="flex items-center gap-3">
            <GitBranch className="h-8 w-8 text-primary" />
            <div>
              <h1 className="text-3xl font-bold">{repo.name}</h1>
              <p className="text-muted-foreground">{repo.owner}</p>
            </div>
            {repo.private ? (
              <Badge variant="secondary" className="ml-2">
                <Lock className="h-3 w-3 mr-1" />
                Private
              </Badge>
            ) : (
              <Badge variant="outline" className="ml-2">
                <Globe className="h-3 w-3 mr-1" />
                Public
              </Badge>
            )}
          </div>
          {repo.description && (
            <p className="text-muted-foreground mt-2 max-w-2xl">
              {repo.description}
            </p>
          )}
        </div>
        {repo.html_url && (
          <a
            href={repo.html_url}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-2 px-4 py-2 bg-secondary hover:bg-secondary/80 rounded-md transition-colors"
          >
            View on GitHub
            <ExternalLink className="h-4 w-4" />
          </a>
        )}
      </div>

      {/* Stats cards */}
      <div className="grid gap-4 md:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Total Builds
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold">{repo.build_count}</div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Success Rate
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold text-green-500">
              {successRate}%
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Passed
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold text-green-500">
              {repo.success_count}
            </div>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground">
              Failed
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div className="text-3xl font-bold text-red-500">
              {repo.failure_count}
            </div>
          </CardContent>
        </Card>
      </div>

      {/* Containers section */}
      {containers.length > 0 && (
        <Card>
          <CardHeader className="flex flex-row items-center justify-between">
            <CardTitle className="flex items-center gap-2">
              <Box className="h-5 w-5" />
              Containers
            </CardTitle>
            <Button
              variant="outline"
              size="sm"
              onClick={handleRestartProject}
              disabled={restartingProject}
            >
              {restartingProject ? (
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
              ) : (
                <RotateCw className="h-4 w-4 mr-2" />
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

      {/* Repo info */}
      <Card>
        <CardHeader>
          <CardTitle>Repository Info</CardTitle>
        </CardHeader>
        <CardContent>
          <dl className="grid grid-cols-2 md:grid-cols-4 gap-4">
            {repo.language && (
              <div>
                <dt className="text-sm text-muted-foreground">Language</dt>
                <dd className="font-medium">{repo.language}</dd>
              </div>
            )}
            {repo.default_branch && (
              <div>
                <dt className="text-sm text-muted-foreground">Default Branch</dt>
                <dd className="font-medium">{repo.default_branch}</dd>
              </div>
            )}
            {repo.last_build_at && (
              <div>
                <dt className="text-sm text-muted-foreground">Last Build</dt>
                <dd className="font-medium">
                  {formatRelativeTime(repo.last_build_at)}
                </dd>
              </div>
            )}
            <div>
              <dt className="text-sm text-muted-foreground">Created</dt>
              <dd className="font-medium">
                {formatRelativeTime(repo.created_at)}
              </dd>
            </div>
          </dl>
        </CardContent>
      </Card>

      {/* Recent builds */}
      <Card>
        <CardHeader>
          <CardTitle>Recent Builds</CardTitle>
        </CardHeader>
        <CardContent>
          {jobs.length === 0 ? (
            <p className="text-muted-foreground text-center py-8">
              No builds yet
            </p>
          ) : (
            <div className="space-y-2">
              {jobs.map((job) => (
                <Link
                  key={job.id}
                  to={`/job/${job.id}`}
                  className="flex items-center justify-between p-3 rounded-lg border hover:bg-accent transition-colors"
                >
                  <div className="flex items-center gap-3">
                    <StatusIcon status={job.status} />
                    <div>
                      <div className="flex items-center gap-2">
                        <GitCommit className="h-4 w-4 text-muted-foreground" />
                        <span className="font-mono text-sm">
                          {job.git_sha.substring(0, 7)}
                        </span>
                      </div>
                      {job.commit_message && (
                        <p className="text-sm text-muted-foreground truncate max-w-md">
                          {job.commit_message}
                        </p>
                      )}
                    </div>
                  </div>
                  <div className="flex items-center gap-4 text-sm text-muted-foreground">
                    {job.duration_secs && (
                      <div className="flex items-center gap-1">
                        <Clock className="h-4 w-4" />
                        {formatDuration(job.duration_secs)}
                      </div>
                    )}
                    <span>{formatRelativeTime(job.created_at)}</span>
                  </div>
                </Link>
              ))}
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

function StatusIcon({ status }: Readonly<{ status: string }>) {
  switch (status) {
    case "success":
      return <CheckCircle2 className="h-5 w-5 text-green-500" />;
    case "failed":
      return <XCircle className="h-5 w-5 text-red-500" />;
    case "running":
      return <Loader2 className="h-5 w-5 text-blue-500 animate-spin" />;
    default:
      return <Clock className="h-5 w-5 text-muted-foreground" />;
  }
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  return `${mins}m ${secs}s`;
}

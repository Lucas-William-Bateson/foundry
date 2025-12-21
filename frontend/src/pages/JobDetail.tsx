import { useEffect, useState, useRef } from "react";
import { useParams, Link, useNavigate } from "react-router-dom";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { fetchJob, rerunJob, type JobDetail } from "@/lib/api";
import { formatDuration, cn } from "@/lib/utils";
import {
  ArrowLeft,
  GitCommit,
  GitBranch,
  User,
  Clock,
  ExternalLink,
  CheckCircle2,
  XCircle,
  Loader2,
  Timer,
  RotateCcw,
} from "lucide-react";

export function JobDetailPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [job, setJob] = useState<JobDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [rerunning, setRerunning] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);

  const handleRerun = async () => {
    if (!id || rerunning) return;
    setRerunning(true);
    try {
      const result = await rerunJob(parseInt(id));
      if (result.ok && result.job_id) {
        navigate(`/job/${result.job_id}`);
      }
    } catch (e) {
      console.error("Failed to rerun job:", e);
    } finally {
      setRerunning(false);
    }
  };

  useEffect(() => {
    if (!id) return;

    const load = async () => {
      try {
        const data = await fetchJob(parseInt(id));
        setJob(data);
      } catch (e) {
        console.error("Failed to load job:", e);
      } finally {
        setLoading(false);
      }
    };

    load();
    const interval = setInterval(() => {
      if (job?.status === "queued" || job?.status === "running") {
        load();
      }
    }, 2000);

    return () => clearInterval(interval);
  }, [id, job?.status]);

  useEffect(() => {
    if (autoScroll && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [job?.logs, autoScroll]);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (!job) {
    return (
      <div className="text-center py-12">
        <h2 className="text-2xl font-bold">Job not found</h2>
        <Link to="/" className="text-primary hover:underline mt-2 inline-block">
          Back to dashboard
        </Link>
      </div>
    );
  }

  const statusConfig = {
    success: {
      color: "text-green-500",
      bg: "bg-green-500/10",
      icon: CheckCircle2,
    },
    failed: { color: "text-red-500", bg: "bg-red-500/10", icon: XCircle },
    running: {
      color: "text-yellow-500",
      bg: "bg-yellow-500/10",
      icon: Loader2,
    },
    queued: { color: "text-muted-foreground", bg: "bg-muted", icon: Clock },
    cancelled: {
      color: "text-muted-foreground",
      bg: "bg-muted",
      icon: XCircle,
    },
  };

  const { color, bg, icon: StatusIcon } = statusConfig[job.status];

  return (
    <div className="space-y-6">
      <div className="flex items-center gap-4">
        <Button variant="ghost" size="icon" asChild>
          <Link to="/">
            <ArrowLeft className="h-5 w-5" />
          </Link>
        </Button>
        <div className="flex-1">
          <h1 className="text-2xl font-bold">Build #{job.id}</h1>
          <p className="text-muted-foreground">
            {job.repo_owner}/{job.repo_name}
          </p>
        </div>
        <Button
          variant="outline"
          size="sm"
          onClick={handleRerun}
          disabled={
            rerunning || job.status === "running" || job.status === "queued"
          }
          className="gap-2"
        >
          {rerunning ? (
            <Loader2 className="h-4 w-4 animate-spin" />
          ) : (
            <RotateCcw className="h-4 w-4" />
          )}
          Re-run
        </Button>
        <div className={cn("flex items-center gap-2 px-4 py-2 rounded-lg", bg)}>
          <StatusIcon
            className={cn(
              "h-5 w-5",
              color,
              job.status === "running" && "animate-spin"
            )}
          />
          <span className={cn("font-semibold capitalize", color)}>
            {job.status}
          </span>
        </div>
      </div>

      {/* Metadata Grid */}
      <div className="grid gap-4 md:grid-cols-2 lg:grid-cols-4">
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <GitCommit className="h-4 w-4" />
              Commit
            </CardTitle>
          </CardHeader>
          <CardContent>
            <code className="text-sm">{job.git_sha.substring(0, 7)}</code>
            {job.commit_url && (
              <a
                href={job.commit_url}
                target="_blank"
                rel="noopener noreferrer"
                className="ml-2 text-primary hover:underline inline-flex items-center gap-1"
              >
                <ExternalLink className="h-3 w-3" />
              </a>
            )}
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <GitBranch className="h-4 w-4" />
              Branch
            </CardTitle>
          </CardHeader>
          <CardContent>
            <span className="text-sm">
              {job.git_ref.replace("refs/heads/", "")}
            </span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <User className="h-4 w-4" />
              Author
            </CardTitle>
          </CardHeader>
          <CardContent>
            <span className="text-sm">
              {job.commit_author || job.pusher_name || "-"}
            </span>
          </CardContent>
        </Card>
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-medium text-muted-foreground flex items-center gap-2">
              <Timer className="h-4 w-4" />
              Duration
            </CardTitle>
          </CardHeader>
          <CardContent>
            <span className="text-sm">{formatDuration(job.duration_secs)}</span>
          </CardContent>
        </Card>
      </div>

      {/* Commit Message */}
      {job.commit_message && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm">Commit Message</CardTitle>
          </CardHeader>
          <CardContent>
            <p className="text-sm whitespace-pre-wrap">{job.commit_message}</p>
          </CardContent>
        </Card>
      )}

      {/* Build Logs */}
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle>Build Logs</CardTitle>
          <label className="flex items-center gap-2 text-sm text-muted-foreground cursor-pointer">
            <input
              type="checkbox"
              checked={autoScroll}
              onChange={(e) => setAutoScroll(e.target.checked)}
              className="rounded"
            />
            Auto-scroll
          </label>
        </CardHeader>
        <CardContent className="p-0">
          <ScrollArea className="h-[500px] w-full">
            <pre className="p-4 text-sm font-mono bg-black/50 rounded-b-lg">
              {job.logs.length === 0 ? (
                <span className="text-muted-foreground">
                  Waiting for logs...
                </span>
              ) : (
                job.logs.map((log, i) => (
                  <div key={i} className="flex gap-4 hover:bg-white/5">
                    <span className="text-muted-foreground select-none w-20 shrink-0">
                      {new Date(log.timestamp).toLocaleTimeString()}
                    </span>
                    <span
                      className={cn(
                        log.level === "error" && "text-red-400",
                        log.message.toLowerCase().includes("error") &&
                          "text-red-400",
                        log.message.includes("✓") && "text-green-400"
                      )}
                    >
                      {log.message}
                    </span>
                  </div>
                ))
              )}
              <div ref={logsEndRef} />
            </pre>
          </ScrollArea>
        </CardContent>
      </Card>
    </div>
  );
}

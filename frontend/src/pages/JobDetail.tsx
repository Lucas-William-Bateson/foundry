import { useEffect, useState, useRef, useCallback } from "react";
import { useParams, Link } from "react-router-dom";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { fetchJob, type JobDetail } from "@/lib/api";
import { usePolling } from "@/lib/hooks";
import { JobHeader } from "@/components/JobHeader";
import { JobMetrics } from "@/components/JobMetrics";
import Autorenew from "@mui/icons-material/Autorenew";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import MuiCheckbox from "@mui/material/Checkbox";
import FormControlLabel from "@mui/material/FormControlLabel";

export function JobDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [job, setJob] = useState<JobDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [autoScroll, setAutoScroll] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);

  const loadJob = useCallback(async () => {
    if (!id) return;
    try {
      const data = await fetchJob(parseInt(id));
      setJob(data);
    } catch (e) {
      console.error("Failed to load job:", e);
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    loadJob();
  }, [loadJob]);
  usePolling(
    loadJob,
    2000,
    job?.status === "queued" || job?.status === "running",
  );

  useEffect(() => {
    if (autoScroll && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [job?.logs, autoScroll]);

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

  if (!job) {
    return (
      <div style={{ textAlign: "center", padding: "3rem 0" }}>
        <h2 style={{ fontSize: "1.5rem", fontWeight: 700 }}>Job not found</h2>
        <Link
          to="/"
          style={{
            color: "#C65D00",
            marginTop: "0.5rem",
            display: "inline-block",
          }}
        >
          Back to dashboard
        </Link>
      </div>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
      <JobHeader job={job} />

      <ErrorBoundary section="Build Metrics">
        {job.metrics && <JobMetrics metrics={job.metrics} />}
      </ErrorBoundary>

      <ErrorBoundary section="Build Logs">
        <Card>
          <CardHeader
            style={{
              display: "flex",
              flexDirection: "row",
              alignItems: "center",
              justifyContent: "space-between",
            }}
          >
            <CardTitle>Build Logs</CardTitle>
            <FormControlLabel
              control={
                <MuiCheckbox
                  checked={autoScroll}
                  onChange={(
                    _e: React.ChangeEvent<HTMLInputElement>,
                    checked: boolean,
                  ) => setAutoScroll(checked)}
                  size="small"
                />
              }
              label="Auto-scroll"
            />
          </CardHeader>
          <CardContent style={{ padding: 0 }}>
            <ScrollArea style={{ height: "500px", width: "100%" }}>
              <pre
                style={{
                  padding: "0.5rem 0",
                  fontSize: "0.8125rem",
                  fontFamily: "'JetBrains Mono', 'Roboto Mono', monospace",
                  backgroundColor: "#0B0F14",
                  borderRadius: "0 0 6px 6px",
                  margin: 0,
                  lineHeight: 1.7,
                }}
              >
                {job.logs.length === 0 ? (
                  <span style={{ color: "#8B8F96", padding: "0 1rem" }}>
                    Waiting for logs...
                  </span>
                ) : (
                  job.logs.map((log, i) => (
                    <div
                      key={i}
                      className={`log-line ${i % 2 === 0 ? "log-line-alt" : ""}`}
                      style={{
                        display: "flex",
                        gap: "1rem",
                        padding: "0 1rem",
                      }}
                    >
                      <span
                        style={{
                          color: "#555",
                          userSelect: "none",
                          width: "3rem",
                          flexShrink: 0,
                          textAlign: "right",
                          fontSize: "0.75rem",
                        }}
                      >
                        {i + 1}
                      </span>
                      <span
                        style={{
                          color: "#8B8F96",
                          userSelect: "none",
                          width: "5rem",
                          flexShrink: 0,
                          fontSize: "0.75rem",
                        }}
                      >
                        {new Date(log.timestamp).toLocaleTimeString()}
                      </span>
                      <span style={{ color: "#9CA3AF" }}>
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
      </ErrorBoundary>
    </div>
  );
}

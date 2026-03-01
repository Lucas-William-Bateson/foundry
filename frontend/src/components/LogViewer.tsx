import { useState, useEffect, useRef } from "react";
import { type Container, fetchContainerLogs } from "@/lib/api";
import { useLogStream } from "@/lib/hooks";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import CloseIcon from "@mui/icons-material/Close";
import Download from "@mui/icons-material/Download";
import Delete from "@mui/icons-material/Delete";
import Pause from "@mui/icons-material/Pause";
import PlayArrow from "@mui/icons-material/PlayArrow";

interface LogViewerProps {
  readonly container: Container;
  readonly onClose: () => void;
}

export function LogViewer({ container, onClose }: LogViewerProps) {
  const {
    lines: logs,
    isConnected,
    isPaused,
    pause,
    resume,
    clear: handleClear,
    setLines: setLogs,
  } = useLogStream({
    url: `/api/containers/${container.id}/logs/stream?lines=100`,
  });

  const [autoScroll, setAutoScroll] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const loadLogs = async () => {
      try {
        const data = await fetchContainerLogs(container.id, 500);
        setLogs(data.logs);
      } catch (error) {
        console.error("Failed to fetch logs:", error);
      }
    };
    loadLogs();
  }, [container.id, setLogs]);

  useEffect(() => {
    if (autoScroll && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: "smooth" });
    }
  }, [logs, autoScroll]);

  const handleDownload = () => {
    const blob = new Blob([logs.join("\n")], { type: "text/plain" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${container.name}-logs.txt`;
    a.click();
    URL.revokeObjectURL(url);
  };

  return (
    <Card style={{ display: "flex", flexDirection: "column", height: "600px" }}>
      <CardHeader
        style={{
          padding: "0.625rem 1rem",
          borderBottom: "1px solid rgba(255, 255, 255, 0.06)",
          flexShrink: 0,
        }}
      >
        <div
          style={{
            display: "flex",
            alignItems: "center",
            justifyContent: "space-between",
          }}
        >
          <CardTitle
            style={{
              fontSize: "0.8125rem",
              fontWeight: 500,
              display: "flex",
              alignItems: "center",
              gap: "0.5rem",
            }}
          >
            <div
              style={{
                width: "6px",
                height: "6px",
                borderRadius: "50%",
                backgroundColor: isConnected ? "#2D9D5E" : "#555",
              }}
              className={isConnected ? "pulse-animation" : undefined}
            />
            Logs: {container.name}
          </CardTitle>
          <div style={{ display: "flex", alignItems: "center", gap: "1rem" }}>
            <div
              style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}
            >
              <Switch
                id="log-auto-scroll"
                checked={autoScroll}
                onCheckedChange={setAutoScroll}
              />
              <span style={{ fontSize: "0.6875rem", color: "#8B8F96" }}>
                Auto-scroll
              </span>
            </div>
            <div style={{ display: "flex", gap: "0.25rem" }}>
              <Button
                variant="ghost"
                size="icon"
                onClick={isPaused ? resume : pause}
                title={isPaused ? "Resume" : "Pause"}
              >
                {isPaused ? (
                  <PlayArrow style={{ fontSize: 14 }} />
                ) : (
                  <Pause style={{ fontSize: 14 }} />
                )}
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={handleClear}
                title="Clear logs"
              >
                <Delete style={{ fontSize: 14 }} />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={handleDownload}
                title="Download logs"
              >
                <Download style={{ fontSize: 14 }} />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                onClick={onClose}
                title="Close"
              >
                <CloseIcon style={{ fontSize: 14 }} />
              </Button>
            </div>
          </div>
        </div>
      </CardHeader>
      <CardContent
        ref={containerRef}
        style={{
          flex: 1,
          overflow: "auto",
          padding: 0,
          backgroundColor: "#0B0F14",
          fontFamily: "'JetBrains Mono', 'Roboto Mono', monospace",
          fontSize: "0.75rem",
        }}
      >
        <div style={{ padding: "0.5rem 0" }}>
          {logs.length === 0 ? (
            <div
              style={{ color: "#8B8F96", textAlign: "center", padding: "2rem" }}
            >
              No logs available
            </div>
          ) : (
            logs.map((line, index) => {
              const key = `${index}-${line.slice(0, 50)}`;
              return (
                <div
                  key={key}
                  className={`log-line ${index % 2 === 0 ? "log-line-alt" : ""}`}
                  style={{
                    whiteSpace: "pre-wrap",
                    wordBreak: "break-all",
                    lineHeight: 1.7,
                    padding: "0 0.75rem",
                    color: "#9CA3AF",
                  }}
                >
                  <span
                    style={{
                      color: "#555",
                      userSelect: "none",
                      display: "inline-block",
                      width: "3.5rem",
                      textAlign: "right",
                      marginRight: "1rem",
                      fontSize: "0.6875rem",
                    }}
                  >
                    {index + 1}
                  </span>
                  {line}
                </div>
              );
            })
          )}
          <div ref={logsEndRef} />
        </div>
      </CardContent>
    </Card>
  );
}

import { useState, useEffect, useRef } from "react";
import { type Container, fetchContainerLogs, streamContainerLogs } from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { X, Download, Trash2, Pause, Play } from "lucide-react";
import { cn } from "@/lib/utils";

interface LogViewerProps {
  readonly container: Container;
  readonly onClose: () => void;
}

export function LogViewer({ container, onClose }: LogViewerProps) {
  const [logs, setLogs] = useState<string[]>([]);
  const [isStreaming, setIsStreaming] = useState(true);
  const [isPaused, setIsPaused] = useState(false);
  const [autoScroll, setAutoScroll] = useState(true);
  const logsEndRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const cleanupRef = useRef<(() => void) | null>(null);

  // Initial fetch
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
  }, [container.id]);

  // Streaming
  useEffect(() => {
    if (!isStreaming || isPaused) return;

    cleanupRef.current = streamContainerLogs(
      container.id,
      (line) => {
        setLogs((prev) => [...prev.slice(-2000), line]); // Keep last 2000 lines
      },
      (error) => {
        console.error("Log stream error:", error);
        setIsStreaming(false);
      },
      100
    );

    return () => {
      if (cleanupRef.current) {
        cleanupRef.current();
      }
    };
  }, [container.id, isStreaming, isPaused]);

  // Auto-scroll
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

  const handleClear = () => {
    setLogs([]);
  };

  const getLogLevel = (line: string): "error" | "warn" | "info" => {
    const lower = line.toLowerCase();
    if (lower.includes("error") || lower.includes("fatal") || lower.includes("panic")) {
      return "error";
    }
    if (lower.includes("warn") || lower.includes("warning")) {
      return "warn";
    }
    return "info";
  };

  return (
    <Card className="flex flex-col h-[600px]">
      <CardHeader className="py-3 px-4 border-b flex-shrink-0">
        <div className="flex items-center justify-between">
          <CardTitle className="text-sm font-medium flex items-center gap-2">
            <div
              className={cn(
                "w-2 h-2 rounded-full",
                isStreaming && !isPaused ? "bg-green-500 animate-pulse" : "bg-gray-400"
              )}
            />
            Logs: {container.name}
          </CardTitle>
          <div className="flex items-center gap-4">
            <div className="flex items-center gap-2">
              <Switch
                id="auto-scroll"
                checked={autoScroll}
                onCheckedChange={setAutoScroll}
              />
              <span className="text-xs text-muted-foreground">
                Auto-scroll
              </span>
            </div>
            <div className="flex gap-1">
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => setIsPaused(!isPaused)}
                title={isPaused ? "Resume" : "Pause"}
              >
                {isPaused ? (
                  <Play className="h-3.5 w-3.5" />
                ) : (
                  <Pause className="h-3.5 w-3.5" />
                )}
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={handleClear}
                title="Clear logs"
              >
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={handleDownload}
                title="Download logs"
              >
                <Download className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={onClose}
                title="Close"
              >
                <X className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        </div>
      </CardHeader>
      <CardContent
        ref={containerRef}
        className="flex-1 overflow-auto p-0 bg-zinc-950 font-mono text-xs"
      >
        <div className="p-3 space-y-0.5">
          {logs.length === 0 ? (
            <div className="text-zinc-500 text-center py-8">
              No logs available
            </div>
          ) : (
            logs.map((line, index) => {
              const level = getLogLevel(line);
              // Use line content + index as key since log lines may repeat
              const key = `${index}-${line.slice(0, 50)}`;
              return (
                <div
                  key={key}
                  className={cn(
                    "whitespace-pre-wrap break-all leading-relaxed",
                    level === "error" && "text-red-400",
                    level === "warn" && "text-yellow-400",
                    level === "info" && "text-zinc-300"
                  )}
                >
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

import { useState } from "react";
import {
  type Container,
  restartContainer,
  stopContainer,
  startContainer,
} from "@/lib/api";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Box,
  Play,
  Square,
  RotateCw,
  Terminal,
  Loader2,
} from "lucide-react";
import { cn } from "@/lib/utils";

interface ContainerListProps {
  readonly containers: Container[];
  readonly onViewLogs: (container: Container) => void;
  readonly onRefresh: () => void;
}

export function ContainerList({
  containers,
  onViewLogs,
  onRefresh,
}: ContainerListProps) {
  const [loading, setLoading] = useState<string | null>(null);

  const handleRestart = async (container: Container) => {
    setLoading(container.id);
    try {
      await restartContainer(container.id);
      onRefresh();
    } catch (error) {
      console.error("Failed to restart container:", error);
    } finally {
      setLoading(null);
    }
  };

  const handleStop = async (container: Container) => {
    setLoading(container.id);
    try {
      await stopContainer(container.id);
      onRefresh();
    } catch (error) {
      console.error("Failed to stop container:", error);
    } finally {
      setLoading(null);
    }
  };

  const handleStart = async (container: Container) => {
    setLoading(container.id);
    try {
      await startContainer(container.id);
      onRefresh();
    } catch (error) {
      console.error("Failed to start container:", error);
    } finally {
      setLoading(null);
    }
  };

  const getStateColor = (state: string) => {
    switch (state.toLowerCase()) {
      case "running":
        return "bg-green-500";
      case "exited":
        return "bg-red-500";
      case "paused":
        return "bg-yellow-500";
      case "restarting":
        return "bg-blue-500";
      default:
        return "bg-gray-500";
    }
  };

  if (containers.length === 0) {
    return (
      <Card>
        <CardContent className="py-8 text-center text-muted-foreground">
          <Box className="h-12 w-12 mx-auto mb-4 opacity-50" />
          <p>No containers found for this project</p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div className="space-y-3">
      {containers.map((container) => {
        const isLoading = loading === container.id;
        const isRunning = container.state.toLowerCase() === "running";

        return (
          <Card key={container.id} className="hover:border-primary/30 transition-colors">
            <CardHeader className="py-3 px-4">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div
                    className={cn(
                      "w-2 h-2 rounded-full",
                      getStateColor(container.state)
                    )}
                  />
                  <div>
                    <CardTitle className="text-sm font-medium">
                      {container.name}
                    </CardTitle>
                    <p className="text-xs text-muted-foreground mt-0.5">
                      {container.image}
                    </p>
                  </div>
                </div>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="text-xs">
                    {container.state}
                  </Badge>
                  <div className="flex gap-1">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      onClick={() => onViewLogs(container)}
                      title="View Logs"
                    >
                      <Terminal className="h-3.5 w-3.5" />
                    </Button>
                    {isRunning ? (
                      <>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7"
                          onClick={() => handleRestart(container)}
                          disabled={isLoading}
                          title="Restart"
                        >
                          {isLoading ? (
                            <Loader2 className="h-3.5 w-3.5 animate-spin" />
                          ) : (
                            <RotateCw className="h-3.5 w-3.5" />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-7 w-7 text-red-500 hover:text-red-600"
                          onClick={() => handleStop(container)}
                          disabled={isLoading}
                          title="Stop"
                        >
                          <Square className="h-3.5 w-3.5" />
                        </Button>
                      </>
                    ) : (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 text-green-500 hover:text-green-600"
                        onClick={() => handleStart(container)}
                        disabled={isLoading}
                        title="Start"
                      >
                        {isLoading ? (
                          <Loader2 className="h-3.5 w-3.5 animate-spin" />
                        ) : (
                          <Play className="h-3.5 w-3.5" />
                        )}
                      </Button>
                    )}
                  </div>
                </div>
              </div>
            </CardHeader>
            {container.ports && (
              <CardContent className="py-2 px-4 pt-0 border-t">
                <p className="text-xs text-muted-foreground">
                  <span className="font-medium">Ports:</span> {container.ports}
                </p>
              </CardContent>
            )}
          </Card>
        );
      })}
    </div>
  );
}

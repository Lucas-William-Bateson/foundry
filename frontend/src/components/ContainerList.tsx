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
import ViewInAr from "@mui/icons-material/ViewInAr";
import PlayArrow from "@mui/icons-material/PlayArrow";
import Stop from "@mui/icons-material/Stop";
import Refresh from "@mui/icons-material/Refresh";
import TerminalIcon from "@mui/icons-material/Terminal";
import Autorenew from "@mui/icons-material/Autorenew";

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

  const getStateColor = (state: string): string => {
    switch (state.toLowerCase()) {
      case "running":
        return "#2D9D5E";
      case "exited":
        return "#D44B4B";
      case "paused":
        return "#C89520";
      case "restarting":
        return "#C65D00";
      default:
        return "#8B8F96";
    }
  };

  if (containers.length === 0) {
    return (
      <Card>
        <CardContent
          style={{ padding: "2rem", textAlign: "center", color: "#8B8F96" }}
        >
          <ViewInAr
            style={{
              fontSize: 48,
              margin: "0 auto 1rem",
              opacity: 0.5,
              display: "block",
            }}
          />
          <p>No containers found for this project</p>
        </CardContent>
      </Card>
    );
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "0.75rem" }}>
      {containers.map((container) => {
        const isLoading = loading === container.id;
        const isRunning = container.state.toLowerCase() === "running";

        return (
          <Card key={container.id}>
            <CardHeader style={{ padding: "0.75rem 1rem" }}>
              <div
                style={{
                  display: "flex",
                  alignItems: "center",
                  justifyContent: "space-between",
                }}
              >
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "0.75rem",
                  }}
                >
                  <div
                    style={{
                      width: "0.5rem",
                      height: "0.5rem",
                      borderRadius: "50%",
                      backgroundColor: getStateColor(container.state),
                    }}
                  />
                  <div>
                    <CardTitle
                      style={{ fontSize: "0.875rem", fontWeight: 500 }}
                    >
                      {container.name}
                    </CardTitle>
                    <p
                      style={{
                        fontSize: "0.75rem",
                        color: "#8B8F96",
                        marginTop: "0.125rem",
                      }}
                    >
                      {container.image}
                    </p>
                  </div>
                </div>
                <div
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: "0.5rem",
                  }}
                >
                  <Badge variant="outline">{container.state}</Badge>
                  <div style={{ display: "flex", gap: "0.25rem" }}>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => onViewLogs(container)}
                      title="View Logs"
                    >
                      <TerminalIcon style={{ fontSize: 14 }} />
                    </Button>
                    {isRunning ? (
                      <>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => handleRestart(container)}
                          disabled={isLoading}
                          title="Restart"
                        >
                          {isLoading ? (
                            <Autorenew
                              style={{ fontSize: 14 }}
                              className="spin-animation"
                            />
                          ) : (
                            <Refresh style={{ fontSize: 14 }} />
                          )}
                        </Button>
                        <Button
                          variant="ghost"
                          size="icon"
                          onClick={() => handleStop(container)}
                          disabled={isLoading}
                          title="Stop"
                          style={{ color: "#D44B4B" }}
                        >
                          <Stop style={{ fontSize: 14 }} />
                        </Button>
                      </>
                    ) : (
                      <Button
                        variant="ghost"
                        size="icon"
                        onClick={() => handleStart(container)}
                        disabled={isLoading}
                        title="Start"
                        style={{ color: "#2D9D5E" }}
                      >
                        {isLoading ? (
                          <Autorenew
                            style={{ fontSize: 14 }}
                            className="spin-animation"
                          />
                        ) : (
                          <PlayArrow style={{ fontSize: 14 }} />
                        )}
                      </Button>
                    )}
                  </div>
                </div>
              </div>
            </CardHeader>
            {container.ports && (
              <CardContent
                style={{
                  padding: "0.5rem 1rem 0.5rem",
                  borderTop: "1px solid rgba(255, 255, 255, 0.06)",
                }}
              >
                <p style={{ fontSize: "0.75rem", color: "#8B8F96" }}>
                  <span style={{ fontWeight: 500 }}>Ports:</span>{" "}
                  {container.ports}
                </p>
              </CardContent>
            )}
          </Card>
        );
      })}
    </div>
  );
}

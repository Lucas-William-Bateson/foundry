import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import type { JobMetrics as JobMetricsType } from "@/lib/api";
import CheckCircle from "@mui/icons-material/CheckCircle";
import ErrorIcon from "@mui/icons-material/Error";
import AccessTime from "@mui/icons-material/AccessTime";
import Autorenew from "@mui/icons-material/Autorenew";
import Speed from "@mui/icons-material/Speed";
import PlayArrow from "@mui/icons-material/PlayArrow";

interface JobMetricsProps {
  readonly metrics: JobMetricsType;
}

export function JobMetrics({ metrics }: JobMetricsProps) {
  return (
    <>
      <Card>
        <CardHeader style={{ paddingBottom: "0.25rem" }}>
          <CardTitle
            style={{
              fontSize: "0.8125rem",
              display: "flex",
              alignItems: "center",
              gap: "0.5rem",
            }}
          >
            <Speed style={{ fontSize: 14 }} />
            Build Metrics
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div
            style={{
              display: "flex",
              flexDirection: "column",
              gap: "0.375rem",
              fontSize: "0.8125rem",
            }}
          >
            <div style={{ display: "flex", justifyContent: "space-between" }}>
              <span style={{ color: "#8B8F96" }}>Clone</span>
              <span
                style={{
                  fontVariantNumeric: "tabular-nums",
                  fontFamily: "'JetBrains Mono', monospace",
                  fontSize: "0.75rem",
                }}
              >
                {metrics.clone_duration_ms}ms
              </span>
            </div>
            {metrics.build_duration_ms && (
              <div style={{ display: "flex", justifyContent: "space-between" }}>
                <span style={{ color: "#8B8F96" }}>Build</span>
                <span
                  style={{
                    fontVariantNumeric: "tabular-nums",
                    fontFamily: "'JetBrains Mono', monospace",
                    fontSize: "0.75rem",
                  }}
                >
                  {metrics.build_duration_ms}ms
                </span>
              </div>
            )}
            <div
              style={{
                display: "flex",
                justifyContent: "space-between",
                fontWeight: 500,
                borderTop: "1px solid rgba(255, 255, 255, 0.06)",
                paddingTop: "0.375rem",
                marginTop: "0.25rem",
              }}
            >
              <span>Total</span>
              <span
                style={{
                  fontVariantNumeric: "tabular-nums",
                  fontFamily: "'JetBrains Mono', monospace",
                  fontSize: "0.75rem",
                }}
              >
                {metrics.total_duration_ms}ms
              </span>
            </div>
          </div>
        </CardContent>
      </Card>

      {metrics.stages && metrics.stages.length > 0 && (
        <Card>
          <CardHeader style={{ paddingBottom: "0.25rem" }}>
            <CardTitle
              style={{
                fontSize: "0.875rem",
                display: "flex",
                alignItems: "center",
                gap: "0.5rem",
              }}
            >
              <PlayArrow style={{ fontSize: 14 }} />
              Pipeline Stages
            </CardTitle>
          </CardHeader>
          <CardContent>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                gap: "0.5rem",
              }}
            >
              {metrics.stages.map((stage, i) => (
                <div
                  key={i}
                  style={{
                    display: "flex",
                    alignItems: "center",
                    justifyContent: "space-between",
                    padding: "0.5rem",
                    borderRadius: "4px",
                    backgroundColor: "rgba(255, 255, 255, 0.03)",
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "0.5rem",
                    }}
                  >
                    {stage.status === "success" && (
                      <CheckCircle style={{ fontSize: 14, color: "#2D9D5E" }} />
                    )}
                    {stage.status === "failed" && (
                      <ErrorIcon style={{ fontSize: 14, color: "#D44B4B" }} />
                    )}
                    {stage.status === "skipped" && (
                      <AccessTime style={{ fontSize: 14, color: "#8B8F96" }} />
                    )}
                    {stage.status === "running" && (
                      <Autorenew
                        className="spin-animation"
                        style={{ fontSize: 14, color: "#C89520" }}
                      />
                    )}
                    <span style={{ fontWeight: 500, fontSize: "0.8125rem" }}>{stage.name}</span>
                  </div>
                  <span style={{ color: "#8B8F96", fontSize: "0.75rem", fontFamily: "'JetBrains Mono', monospace", fontVariantNumeric: "tabular-nums" }}>
                    {stage.duration_ms}ms
                  </span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}
    </>
  );
}

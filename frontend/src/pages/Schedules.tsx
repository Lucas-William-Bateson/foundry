import { useEffect, useState, useCallback } from "react";
import { fetchSchedules, type Schedule } from "@/lib/api";
import { formatRelativeTime } from "@/lib/utils";
import { usePolling } from "@/lib/hooks";
import CalendarMonth from "@mui/icons-material/CalendarMonth";
import AccessTime from "@mui/icons-material/AccessTime";
import Autorenew from "@mui/icons-material/Autorenew";

function cronToHuman(cron: string): string {
  const parts = cron.trim().split(/\s+/);
  if (parts.length < 6) return cron;

  const [, min, hour, day, , weekday] = parts;

  if (weekday !== "*" && day === "*") {
    const days = [
      "Sunday",
      "Monday",
      "Tuesday",
      "Wednesday",
      "Thursday",
      "Friday",
      "Saturday",
    ];
    const dayName = days[parseInt(weekday)] || weekday;
    return `Weekly on ${dayName} at ${hour}:${min.padStart(2, "0")}`;
  }

  if (day === "*" && weekday === "*") {
    return `Daily at ${hour}:${min.padStart(2, "0")}`;
  }

  if (day !== "*") {
    return `Monthly on day ${day} at ${hour}:${min.padStart(2, "0")}`;
  }

  return cron;
}

export function Schedules() {
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    try {
      const data = await fetchSchedules();
      setSchedules(data);
    } catch (e) {
      console.error("Failed to load schedules:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);
  usePolling(load, 30000);

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

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: "1.5rem" }}>
      <h1
        style={{
          fontSize: "1.5rem",
          fontWeight: 700,
          letterSpacing: "-0.02em",
        }}
      >
        Schedules
      </h1>

      {schedules.length === 0 ? (
        <div
          style={{ textAlign: "center", padding: "3rem 0", color: "#8B8F96" }}
        >
          <CalendarMonth
            style={{
              fontSize: 48,
              margin: "0 auto 1rem",
              opacity: 0.5,
              display: "block",
            }}
          />
          <p>No schedules configured</p>
          <p style={{ fontSize: "0.875rem", marginTop: "0.5rem" }}>
            Add a{" "}
            <code
              style={{
                backgroundColor: "rgba(255, 255, 255, 0.06)",
                padding: "0.125rem 0.25rem",
                borderRadius: "0.25rem",
              }}
            >
              [schedule]
            </code>{" "}
            section to foundry.toml
          </p>
        </div>
      ) : (
        <div
          style={{ display: "flex", flexDirection: "column", gap: "0.375rem" }}
        >
          {schedules.map((schedule) => (
            <div
              key={schedule.id}
              className="build-row"
              style={{
                display: "flex",
                alignItems: "center",
                justifyContent: "space-between",
                padding: "1rem 1rem 1rem 1.25rem",
                borderRadius: "4px",
                backgroundColor: "rgba(255, 255, 255, 0.03)",
                border: "1px solid rgba(255, 255, 255, 0.06)",
                ['--status-color' as string]: schedule.last_run_at ? '#2D9D5E' : '#8B8F96',
              }}
            >
              <div
                style={{ display: "flex", alignItems: "center", gap: "0.625rem" }}
              >
                <AccessTime style={{ fontSize: 16, color: "#C65D00" }} />
                <div>
                  <div style={{ fontWeight: 500, fontSize: "0.875rem" }}>{schedule.repo_name}</div>
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: "0.375rem",
                      color: "#8B8F96",
                      fontSize: "0.8125rem",
                      marginTop: "2px",
                    }}
                  >
                    {cronToHuman(schedule.cron_expression)}
                  </div>
                </div>
              </div>
              <div style={{ fontSize: "0.8125rem", color: "#A0A4AB", fontVariantNumeric: "tabular-nums" }}>
                {schedule.last_run_at ? (
                  <span>
                    Last run {formatRelativeTime(schedule.last_run_at)}
                  </span>
                ) : (
                  <span>Never run</span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

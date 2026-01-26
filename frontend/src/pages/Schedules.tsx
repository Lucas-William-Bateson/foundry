import { useEffect, useState } from "react";
import { fetchSchedules, type Schedule } from "@/lib/api";
import { formatRelativeTime } from "@/lib/utils";
import { Calendar, Clock, Loader2 } from "lucide-react";

// Convert 7-field cron to human-readable format
function cronToHuman(cron: string): string {
  const parts = cron.trim().split(/\s+/);
  // Format: sec min hour day month weekday year
  if (parts.length < 6) return cron;

  const [, min, hour, day, , weekday] = parts;

  // Check if it's weekly (specific weekday)
  if (weekday !== "*" && day === "*") {
    const days = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
    const dayName = days[parseInt(weekday)] || weekday;
    return `Weekly on ${dayName} at ${hour}:${min.padStart(2, "0")}`;
  }

  // Daily
  if (day === "*" && weekday === "*") {
    return `Daily at ${hour}:${min.padStart(2, "0")}`;
  }

  // Monthly
  if (day !== "*") {
    return `Monthly on day ${day} at ${hour}:${min.padStart(2, "0")}`;
  }

  return cron;
}

export function Schedules() {
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [loading, setLoading] = useState(true);

  const load = async () => {
    try {
      const data = await fetchSchedules();
      setSchedules(data);
    } catch (e) {
      console.error("Failed to load schedules:", e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    load();
    const interval = setInterval(load, 30000);
    return () => clearInterval(interval);
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <h1 className="text-2xl font-bold">Schedules</h1>

      {schedules.length === 0 ? (
        <div className="text-center py-12 text-muted-foreground">
          <Calendar className="h-12 w-12 mx-auto mb-4 opacity-50" />
          <p>No schedules configured</p>
          <p className="text-sm mt-2">
            Add a <code className="bg-muted px-1 rounded">[schedule]</code> section to foundry.toml
          </p>
        </div>
      ) : (
        <div className="space-y-2">
          {schedules.map((schedule) => (
            <div
              key={schedule.id}
              className="flex items-center justify-between py-3 px-4 rounded-lg bg-card border"
            >
              <div className="flex items-center gap-4">
                <div className="font-medium">
                  {schedule.repo_name}
                </div>
                <div className="flex items-center gap-1.5 text-muted-foreground text-sm">
                  <Clock className="h-3.5 w-3.5" />
                  {cronToHuman(schedule.cron_expression)}
                </div>
              </div>
              <div className="text-sm text-muted-foreground">
                {schedule.last_run_at ? (
                  <span>Last run {formatRelativeTime(schedule.last_run_at)}</span>
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

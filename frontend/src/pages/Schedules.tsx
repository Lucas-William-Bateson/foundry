import { useEffect, useState } from "react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import {
  fetchSchedules,
  toggleSchedule,
  deleteSchedule,
  type Schedule,
} from "@/lib/api";
import { formatRelativeTime } from "@/lib/utils";
import {
  Calendar,
  Clock,
  GitBranch,
  Loader2,
  Trash2,
  Play,
  Pause,
} from "lucide-react";

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
    const interval = setInterval(load, 30000); // Refresh every 30s
    return () => clearInterval(interval);
  }, []);

  const handleToggle = async (schedule: Schedule) => {
    try {
      await toggleSchedule(schedule.id, !schedule.enabled);
      setSchedules((prev) =>
        prev.map((s) =>
          s.id === schedule.id ? { ...s, enabled: !s.enabled } : s,
        ),
      );
    } catch (e) {
      console.error("Failed to toggle schedule:", e);
    }
  };

  const handleDelete = async (id: number) => {
    if (!confirm("Are you sure you want to delete this schedule?")) return;
    try {
      await deleteSchedule(id);
      setSchedules((prev) => prev.filter((s) => s.id !== id));
    } catch (e) {
      console.error("Failed to delete schedule:", e);
    }
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-3xl font-bold">Scheduled Builds</h1>
        <div className="text-sm text-muted-foreground">
          {schedules.filter((s) => s.enabled).length} active schedules
        </div>
      </div>

      {schedules.length === 0 ? (
        <Card>
          <CardContent className="py-12 text-center text-muted-foreground">
            <Calendar className="h-12 w-12 mx-auto mb-4 opacity-50" />
            <p className="text-lg font-medium mb-2">No scheduled builds</p>
            <p className="text-sm">
              Add a <code className="bg-muted px-1 rounded">[schedule]</code>{" "}
              section to your{" "}
              <code className="bg-muted px-1 rounded">foundry.toml</code> to
              enable cron builds.
            </p>
            <pre className="mt-4 text-left bg-muted p-4 rounded-md text-xs max-w-md mx-auto">
              {`[schedule]
cron = "0 0 * * *"  # Daily at midnight
branch = "main"
enabled = true`}
            </pre>
          </CardContent>
        </Card>
      ) : (
        <div className="grid gap-4">
          {schedules.map((schedule) => (
            <Card
              key={schedule.id}
              className={`transition-colors ${
                schedule.enabled
                  ? "hover:border-primary/50"
                  : "opacity-60 hover:opacity-80"
              }`}
            >
              <CardHeader className="pb-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-3">
                    <CardTitle className="flex items-center gap-2">
                      <GitBranch className="h-5 w-5" />
                      {schedule.repo_owner}/{schedule.repo_name}
                    </CardTitle>
                    <Badge variant="outline">{schedule.branch}</Badge>
                  </div>
                  <div className="flex items-center gap-4">
                    <div className="flex items-center gap-2">
                      {schedule.enabled ? (
                        <Play className="h-4 w-4 text-green-500" />
                      ) : (
                        <Pause className="h-4 w-4 text-muted-foreground" />
                      )}
                      <Switch
                        checked={schedule.enabled}
                        onCheckedChange={() => handleToggle(schedule)}
                      />
                    </div>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => handleDelete(schedule.id)}
                      className="text-destructive hover:text-destructive"
                    >
                      <Trash2 className="h-4 w-4" />
                    </Button>
                  </div>
                </div>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-1 md:grid-cols-3 gap-4 text-sm">
                  <div className="flex items-center gap-2">
                    <Clock className="h-4 w-4 text-muted-foreground" />
                    <div>
                      <span className="font-mono">
                        {schedule.cron_expression}
                      </span>
                      <span className="text-muted-foreground ml-2">
                        ({schedule.timezone})
                      </span>
                    </div>
                  </div>
                  <div>
                    <span className="text-muted-foreground">Last run: </span>
                    {schedule.last_run_at ? (
                      <span title={schedule.last_run_at}>
                        {formatRelativeTime(schedule.last_run_at)}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">Never</span>
                    )}
                  </div>
                  <div>
                    <span className="text-muted-foreground">Next run: </span>
                    {schedule.next_run_at && schedule.enabled ? (
                      <span
                        className="text-primary font-medium"
                        title={schedule.next_run_at}
                      >
                        {formatRelativeTime(schedule.next_run_at)}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">
                        {schedule.enabled ? "Calculating..." : "Paused"}
                      </span>
                    )}
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>
      )}
    </div>
  );
}

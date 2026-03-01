import { Badge } from "@/components/ui/badge";
import CheckCircleIcon from "@mui/icons-material/CheckCircle";
import ErrorIcon from "@mui/icons-material/Error";
import AccessTimeIcon from "@mui/icons-material/AccessTime";
import AutorenewIcon from "@mui/icons-material/Autorenew";
import CancelIcon from "@mui/icons-material/Cancel";
import { cn } from "@/lib/utils";

export type JobStatus = "success" | "failed" | "running" | "queued" | "cancelled";

type BadgeVariant = "default" | "secondary" | "destructive" | "outline" | "success" | "warning";

interface StatusConfigEntry {
  color: string;
  bg: string;
  icon: React.ComponentType<{ fontSize?: "small" | "inherit"; className?: string; style?: React.CSSProperties }>;
  variant: BadgeVariant;
}

export const statusConfig: Record<JobStatus, StatusConfigEntry> = {
  success: {
    color: "status-success",
    bg: "status-success-bg",
    icon: CheckCircleIcon,
    variant: "success",
  },
  failed: {
    color: "status-error",
    bg: "status-error-bg",
    icon: ErrorIcon,
    variant: "destructive",
  },
  running: {
    color: "status-warning",
    bg: "status-warning-bg",
    icon: AutorenewIcon,
    variant: "warning",
  },
  queued: {
    color: "status-muted",
    bg: "status-muted-bg",
    icon: AccessTimeIcon,
    variant: "secondary",
  },
  cancelled: {
    color: "status-muted",
    bg: "status-muted-bg",
    icon: CancelIcon,
    variant: "outline",
  },
};

interface StatusBadgeProps {
  status: string;
}

export function StatusBadge({ status }: StatusBadgeProps) {
  const config = statusConfig[status as JobStatus] ?? statusConfig.queued;
  const Icon = config.icon;
  return (
    <Badge variant={config.variant} className={cn("status-badge", config.color)}>
      <Icon fontSize="small" className={cn(status === "running" && "spin-animation")} style={{ fontSize: '0.875rem' }} />
      <span style={{ marginLeft: "4px" }}>{status}</span>
    </Badge>
  );
}

interface StatusIconProps {
  status: string;
  className?: string;
}

export function StatusIcon({ status, className }: StatusIconProps) {
  const config = statusConfig[status as JobStatus] ?? statusConfig.queued;
  const Icon = config.icon;
  return (
    <Icon
      fontSize="small"
      className={cn(
        config.color,
        status === "running" && "spin-animation",
        className,
      )}
    />
  );
}

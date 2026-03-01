import React from "react";
import Chip from "@mui/material/Chip";

export interface BadgeProps extends React.HTMLAttributes<HTMLDivElement> {
  variant?:
    | "default"
    | "secondary"
    | "destructive"
    | "outline"
    | "success"
    | "warning";
}

function getChipStyles(variant?: string) {
  switch (variant) {
    case "success":
      return { bgcolor: "rgba(45, 157, 94, 0.12)", color: "#2D9D5E" };
    case "destructive":
      return { bgcolor: "rgba(212, 75, 75, 0.12)", color: "#D44B4B" };
    case "warning":
      return { bgcolor: "rgba(200, 149, 32, 0.12)", color: "#C89520" };
    case "secondary":
      return { bgcolor: "rgba(255, 255, 255, 0.04)", color: "#8B8F96" };
    case "outline":
      return {
        bgcolor: "transparent",
        color: "#8B8F96",
        border: "1px solid rgba(255, 255, 255, 0.08)",
      };
    default:
      return { bgcolor: "rgba(198, 93, 0, 0.12)", color: "#C65D00" };
  }
}

function Badge({ className, variant, children }: BadgeProps) {
  const chipStyles = getChipStyles(variant);
  return (
    <Chip
      label={children}
      size="small"
      className={className}
      sx={{
        ...chipStyles,
        fontWeight: 500,
        fontSize: "0.6875rem",
        letterSpacing: "0.02em",
        height: "auto",
        "& .MuiChip-label": {
          display: "flex",
          alignItems: "center",
          gap: "0.25rem",
          px: 0.75,
          py: 0.25,
        },
      }}
    />
  );
}

export { Badge };

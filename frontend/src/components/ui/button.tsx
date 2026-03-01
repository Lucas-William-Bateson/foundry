import React from "react";
import MuiButton from "@mui/material/Button";
import IconButton from "@mui/material/IconButton";

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "default"
    | "destructive"
    | "outline"
    | "secondary"
    | "ghost"
    | "link";
  size?: "default" | "sm" | "lg" | "icon";
  asChild?: boolean;
  children?: React.ReactNode;
}

function mapVariant(variant?: string): "contained" | "outlined" | "text" {
  switch (variant) {
    case "outline":
      return "outlined";
    case "ghost":
    case "link":
    case "secondary":
      return "text";
    default:
      return "contained";
  }
}

function mapColor(
  variant?: string,
):
  | "primary"
  | "error"
  | "inherit"
  | "secondary"
  | "info"
  | "success"
  | "warning" {
  switch (variant) {
    case "destructive":
      return "error";
    case "ghost":
    case "link":
    case "secondary":
      return "inherit";
    default:
      return "primary";
  }
}

function mapSize(size?: string): "small" | "medium" | "large" {
  switch (size) {
    case "sm":
      return "small";
    case "lg":
      return "large";
    default:
      return "medium";
  }
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  (
    {
      variant,
      size,
      asChild,
      children,
      className,
      style,
      color: _color,
      onClick,
      disabled,
      type,
    },
    ref,
  ) => {
    const muiColor = mapColor(variant);
    if (size === "icon") {
      return (
        <IconButton
          ref={ref}
          color={muiColor}
          size="small"
          className={className}
          style={style}
          onClick={onClick}
          disabled={disabled}
          type={type}
        >
          {children}
        </IconButton>
      );
    }

    if (asChild && React.isValidElement(children)) {
      const child = children as React.ReactElement<Record<string, unknown>>;
      return React.cloneElement(child, {
        onClick,
        disabled,
        ref,
        className,
        style,
      } as Record<string, unknown>);
    }

    return (
      <MuiButton
        ref={ref}
        variant={mapVariant(variant)}
        color={muiColor}
        size={mapSize(size)}
        className={className}
        style={style}
        onClick={onClick}
        disabled={disabled}
        type={type}
      >
        {children}
      </MuiButton>
    );
  },
);
Button.displayName = "Button";

export { Button };

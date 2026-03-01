import React from "react";
import MuiCard from "@mui/material/Card";
import { cn } from "@/lib/utils";

const Card = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, children, ...props }, ref) => (
  <MuiCard
    ref={ref}
    variant="outlined"
    className={cn("card-hover", className)}
    {...props}
    sx={{ bgcolor: "background.paper" }}
  >
    {children}
  </MuiCard>
));
Card.displayName = "Card";

const CardHeader = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(className)}
    style={{
      padding: "0.875rem 1rem",
      display: "flex",
      flexDirection: "column",
      gap: "0.25rem",
      ...props.style,
    }}
    {...props}
  />
));
CardHeader.displayName = "CardHeader";

const CardTitle = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <h3
    ref={ref}
    className={cn(className)}
    style={{
      fontSize: "0.8125rem",
      fontWeight: 600,
      lineHeight: 1.4,
      margin: 0,
      color: "#E8EAED",
      letterSpacing: "-0.01em",
      ...props.style,
    }}
    {...props}
  />
));
CardTitle.displayName = "CardTitle";

const CardDescription = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <p
    ref={ref}
    className={cn(className)}
    style={{
      fontSize: "0.8125rem",
      color: "#8B8F96",
      margin: 0,
      ...props.style,
    }}
    {...props}
  />
));
CardDescription.displayName = "CardDescription";

const CardContentWrapper = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(className)}
    style={{ padding: "0 1rem 0.875rem", ...props.style }}
    {...props}
  />
));
CardContentWrapper.displayName = "CardContent";

const CardFooter = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(className)}
    style={{
      display: "flex",
      alignItems: "center",
      padding: "0 1rem 0.875rem",
      ...props.style,
    }}
    {...props}
  />
));
CardFooter.displayName = "CardFooter";

export {
  Card,
  CardHeader,
  CardFooter,
  CardTitle,
  CardDescription,
  CardContentWrapper as CardContent,
};

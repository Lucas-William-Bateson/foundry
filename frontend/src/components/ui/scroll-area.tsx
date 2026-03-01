import React from "react";
import { cn } from "@/lib/utils";

const ScrollArea = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, children, style, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(className)}
    style={{ overflow: "auto", position: "relative", ...style }}
    {...props}
  >
    {children}
  </div>
));
ScrollArea.displayName = "ScrollArea";

const ScrollBar = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>((_props, _ref) => null);
ScrollBar.displayName = "ScrollBar";

export { ScrollArea, ScrollBar };

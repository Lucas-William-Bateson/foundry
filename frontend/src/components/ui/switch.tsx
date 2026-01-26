import * as React from "react";
import { cn } from "@/lib/utils";

interface SwitchProps extends React.InputHTMLAttributes<HTMLInputElement> {
  onCheckedChange?: (checked: boolean) => void;
}

const Switch = React.forwardRef<HTMLInputElement, SwitchProps>(
  ({ className, checked, onCheckedChange, ...props }, ref) => {
    return (
      <label className="relative inline-flex items-center cursor-pointer">
        <input
          type="checkbox"
          className="sr-only peer"
          ref={ref}
          checked={checked}
          onChange={(e) => onCheckedChange?.(e.target.checked)}
          {...props}
        />
        <div
          className={cn(
            "w-11 h-6 bg-muted rounded-full peer",
            "peer-checked:after:translate-x-full peer-checked:bg-primary",
            "after:content-[''] after:absolute after:top-[2px] after:left-[2px]",
            "after:bg-white after:rounded-full after:h-5 after:w-5",
            "after:transition-all after:duration-200",
            "transition-colors duration-200",
            className,
          )}
        />
      </label>
    );
  },
);
Switch.displayName = "Switch";

export { Switch };

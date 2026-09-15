import * as React from "react";

import { cn } from "@/lib/utils";

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, ...props }, ref) => (
    <input
      ref={ref}
      type={type}
      data-slot="input"
      className={cn(
        "h-9 w-full min-w-0 rounded-md border border-[var(--border)] bg-[var(--surface-elevated)] px-3 py-1 text-sm text-[var(--ink)] shadow-sm outline-none transition-[border-color,box-shadow] placeholder:text-[var(--ink-muted)] focus-visible:border-[color-mix(in_srgb,var(--accent)_55%,var(--border))] focus-visible:ring-2 focus-visible:ring-[color-mix(in_srgb,var(--accent)_14%,transparent)] disabled:cursor-not-allowed disabled:opacity-50",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";

export { Input };

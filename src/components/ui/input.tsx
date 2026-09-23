import * as React from "react";

import { cn } from "@/lib/utils";

const Input = React.forwardRef<HTMLInputElement, React.ComponentProps<"input">>(
  ({ className, type, ...props }, ref) => (
    <input
      ref={ref}
      type={type}
      data-slot="input"
      className={cn(
        "h-8 w-full min-w-0 rounded-[5px] border border-line-strong bg-canvas px-2.5 text-[13px] text-ink outline-none transition-[border-color,box-shadow] duration-150 placeholder:text-ink-3 hover:border-[color-mix(in_srgb,var(--ink)_28%,transparent)] focus-visible:border-signal-line focus-visible:shadow-[0_0_0_3px_var(--signal-soft)] disabled:cursor-not-allowed disabled:opacity-50 read-only:text-ink-3",
        className,
      )}
      {...props}
    />
  ),
);
Input.displayName = "Input";

export { Input };

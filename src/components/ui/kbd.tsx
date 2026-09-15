import * as React from "react";

import { cn } from "@/lib/utils";

function Kbd({ className, ...props }: React.ComponentProps<"kbd">) {
  return <kbd data-slot="kbd" className={cn("inline-flex min-w-5 items-center justify-center rounded border border-[var(--border)] bg-[var(--surface-2)] px-1 py-0.5 font-mono text-[10px] font-medium text-[var(--ink-muted)] shadow-[inset_0_-1px_0_var(--border)]", className)} {...props} />;
}

export { Kbd };

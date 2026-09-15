import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

const buttonVariants = cva(
  "inline-flex shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-md text-xs font-semibold transition-[color,background-color,border-color,box-shadow,transform] outline-none focus-visible:ring-2 focus-visible:ring-[color-mix(in_srgb,var(--accent)_28%,transparent)] disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "border border-[var(--accent)] bg-[var(--accent)] text-white shadow-sm hover:bg-[color-mix(in_srgb,var(--accent)_88%,white)]",
        outline: "border border-[var(--border)] bg-[var(--surface-elevated)] text-[var(--ink)] hover:bg-[var(--surface-2)]",
        secondary: "border border-transparent bg-[var(--surface-2)] text-[var(--ink)] hover:bg-[color-mix(in_srgb,var(--surface-2)_82%,var(--ink)_8%)]",
        ghost: "border border-transparent bg-transparent text-[var(--ink-secondary)] hover:bg-[color-mix(in_srgb,var(--ink)_6%,transparent)] hover:text-[var(--ink)]",
        destructive: "border border-[color-mix(in_srgb,var(--danger)_40%,var(--border))] bg-[color-mix(in_srgb,var(--danger)_10%,transparent)] text-[var(--danger)] hover:bg-[color-mix(in_srgb,var(--danger)_16%,transparent)]",
        link: "h-auto border-0 bg-transparent p-0 text-[var(--accent)] underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 px-3 py-2",
        sm: "h-8 rounded-md px-2.5",
        lg: "h-10 rounded-lg px-4",
        icon: "size-9 p-0",
        "icon-sm": "size-8 p-0",
      },
    },
    compoundVariants: [
      { variant: "link", size: "default", className: "h-auto min-h-0 p-0" },
    ],
    defaultVariants: { variant: "outline", size: "default" },
  },
);

type ButtonProps = React.ComponentProps<"button"> & VariantProps<typeof buttonVariants>;

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, type = "button", ...props }, ref) => (
    <button
      ref={ref}
      type={type}
      data-slot="button"
      className={cn(buttonVariants({ variant, size }), className)}
      {...props}
    />
  ),
);
Button.displayName = "Button";

export { Button, buttonVariants };

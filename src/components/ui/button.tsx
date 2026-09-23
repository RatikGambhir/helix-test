import * as React from "react";
import { cva, type VariantProps } from "class-variance-authority";

import { cn } from "@/lib/utils";

/**
 * The one button in the system. `default` carries the signal colour and is
 * reserved for the single primary action on a surface (Run, Connect, Continue).
 */
const buttonVariants = cva(
  "inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-[5px] border text-[12.5px] font-medium leading-none outline-none transition-[color,background-color,border-color,transform] duration-150 active:translate-y-px focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-signal-line disabled:cursor-default disabled:opacity-45 disabled:active:translate-y-0 [&_svg]:pointer-events-none [&_svg]:size-[15px] [&_svg]:shrink-0",
  {
    variants: {
      variant: {
        default: "border-transparent bg-signal font-semibold text-signal-ink hover:brightness-[1.06] disabled:hover:brightness-100",
        outline: "border-line-strong bg-transparent text-ink hover:bg-hover disabled:hover:bg-transparent",
        ghost: "border-transparent bg-transparent text-ink-2 hover:bg-hover hover:text-ink disabled:hover:bg-transparent",
        destructive: "border-transparent bg-transparent text-danger hover:bg-[color-mix(in_srgb,var(--danger)_12%,transparent)]",
        link: "h-auto rounded-[3px] border-0 bg-transparent p-0 font-mono text-[12px] font-normal text-ink underline decoration-line-strong underline-offset-[3px] hover:decoration-current active:translate-y-0",
      },
      size: {
        default: "h-8 px-3",
        sm: "h-7 px-2.5 text-[12px]",
        lg: "h-10 px-4 text-[13px]",
        icon: "size-8 p-0",
        "icon-sm": "size-7 p-0",
      },
    },
    compoundVariants: [{ variant: "link", className: "h-auto px-0" }],
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

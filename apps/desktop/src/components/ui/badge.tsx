import { cva, type VariantProps } from "class-variance-authority";
import type { HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

const badgeVariants = cva(
  "inline-flex items-center gap-1 rounded-[6px] border px-1.5 py-0.5 text-xs font-medium",
  {
    variants: {
      variant: {
        neutral: "border-stroke bg-surface-raised text-muted",
        accent: "border-accent/40 bg-accent/10 text-accent",
        warning: "border-warning/40 bg-warning/10 text-warning",
        critical: "border-critical/40 bg-critical/10 text-critical",
      },
    },
    defaultVariants: { variant: "neutral" },
  }
);

export interface BadgeProps
  extends HTMLAttributes<HTMLSpanElement>,
    VariantProps<typeof badgeVariants> {}

export function Badge({ className, variant, ...props }: BadgeProps) {
  return <span className={cn(badgeVariants({ variant }), className)} {...props} />;
}

/**
 * Availability chip for every non-`available` state (design.md §4/§15).
 * States are never encoded by color alone: each chip pairs an icon shape
 * with a text label, and the tooltip carries the reason code plus
 * remediation. `available` renders nothing (the card shows the live value).
 */
import { AlertTriangle, Ban, History, Loader2, ShieldAlert } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useT } from "@/lib/i18n";
import type { MessageKey } from "@/lib/i18n/dictionaries";
import { cn } from "@/lib/utils";
import type { Availability } from "@/lib/ipc/types";

interface StateMeta {
  labelKey: MessageKey;
  icon: LucideIcon;
  variant: "neutral" | "warning" | "critical" | "accent";
}

const STATE_META: Partial<Record<Availability, StateMeta>> = {
  unsupported: { labelKey: "states.unsupported", icon: Ban, variant: "neutral" },
  permission_required: {
    labelKey: "states.permissionRequired",
    icon: ShieldAlert,
    variant: "warning",
  },
  temporarily_unavailable: {
    labelKey: "states.temporarilyUnavailable",
    icon: Loader2,
    variant: "accent",
  },
  stale: { labelKey: "states.stale", icon: History, variant: "warning" },
  error: { labelKey: "states.error", icon: AlertTriangle, variant: "critical" },
};

export interface MetricAvailabilityProps {
  availability: Availability;
  reasonCode?: string;
  remediation?: string;
  className?: string;
}

export function MetricAvailability({
  availability,
  reasonCode,
  remediation,
  className,
}: MetricAvailabilityProps) {
  const t = useT();
  const meta = STATE_META[availability];
  if (!meta) return null; // available → no chip

  const Icon = meta.icon;
  const label = t(meta.labelKey);
  const iconClassName =
    availability === "temporarily_unavailable" ? "h-3 w-3 animate-pulse" : "h-3 w-3";
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge
          variant={meta.variant}
          className={cn("cursor-help", className)}
          aria-label={label}
        >
          <Icon className={iconClassName} aria-hidden />
          {label}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>
        <p>{label}</p>
        {reasonCode ? (
          <p className="mt-1 text-muted numeric">{reasonCode}</p>
        ) : null}
        {remediation ? <p className="mt-1 text-muted">{remediation}</p> : null}
      </TooltipContent>
    </Tooltip>
  );
}

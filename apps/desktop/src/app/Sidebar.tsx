/**
 * Primary navigation (design.md §3). Buttons are naturally keyboard
 * focusable; global shortcuts map Ctrl/Cmd+1..7 to pages and Ctrl/Cmd+, to
 * Settings. Entries carry no fabricated status dots — status comes from
 * real telemetry only.
 */
import {
  Activity,
  ArrowLeftRight,
  Circle,
  HardDrive,
  ListTree,
  Settings as SettingsIcon,
  Wifi,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useT } from "@/lib/i18n";
import type { MessageKey } from "@/lib/i18n/dictionaries";
import { cn } from "@/lib/utils";

export const PAGE_IDS = [
  "overview",
  "applications",
  "storage",
  "network",
  "sensors",
  "compare",
  "settings",
] as const;

export type PageId = (typeof PAGE_IDS)[number];

const NAV_ITEMS: { id: PageId; labelKey: MessageKey; icon: LucideIcon }[] = [
  { id: "overview", labelKey: "nav.overview", icon: Activity },
  { id: "applications", labelKey: "nav.applications", icon: ListTree },
  { id: "storage", labelKey: "nav.storage", icon: HardDrive },
  { id: "network", labelKey: "nav.network", icon: Wifi },
  { id: "sensors", labelKey: "nav.sensors", icon: Circle },
  { id: "compare", labelKey: "nav.compare", icon: ArrowLeftRight },
  { id: "settings", labelKey: "nav.settings", icon: SettingsIcon },
];

export interface SidebarProps {
  active: PageId;
  onSelect: (page: PageId) => void;
}

export function Sidebar({ active, onSelect }: SidebarProps) {
  const t = useT();
  return (
    <nav
      aria-label={t("app.name")}
      className="flex h-full w-56 shrink-0 flex-col gap-1 border-r border-stroke bg-sidebar p-3"
    >
      <div className="px-2 pb-4 pt-2 text-sm font-semibold tracking-wide text-muted">
        {t("app.name")}
      </div>
      {NAV_ITEMS.map(({ id, labelKey, icon: Icon }, index) => (
        <button
          key={id}
          type="button"
          onClick={() => onSelect(id)}
          aria-current={active === id ? "page" : undefined}
          title={`${index + 1}. ${t(labelKey)}`}
          className={cn(
            "flex items-center gap-3 rounded-[8px] px-2 py-2 text-left text-sm transition-colors",
            active === id
              ? "bg-surface-raised font-medium text-text"
              : "text-muted hover:bg-surface-raised hover:text-text"
          )}
        >
          <Icon className="h-4 w-4 shrink-0" aria-hidden />
          <span className="truncate">{t(labelKey)}</span>
        </button>
      ))}
    </nav>
  );
}

/**
 * Status strip (design.md §3): connection state and last update time. Both
 * derive from real scheduler/IPC state — there is no simulated "live" badge.
 */
import { useT } from "@/lib/i18n";
import { cn } from "@/lib/utils";
import { useTelemetryStore } from "@/stores/telemetryStore";

export function Header({ pageTitle }: { pageTitle: string }) {
  const t = useT();
  const connection = useTelemetryStore((s) => s.connection);
  const snapshot = useTelemetryStore((s) => s.snapshot);
  const stale = useTelemetryStore((s) => s.stale);

  const statusLabel =
    connection === "live"
      ? stale
        ? t("states.stale")
        : t("status.live")
      : connection === "connecting"
        ? t("status.connecting")
        : t("status.error");

  return (
    <header className="flex h-12 shrink-0 items-center justify-between border-b border-stroke px-5">
      <h1 className="text-base font-semibold">{pageTitle}</h1>
      <div className="flex items-center gap-3 text-xs text-muted">
        <span
          className={cn(
            "numeric",
            connection === "error" && "text-critical",
            stale && connection === "live" && "text-warning"
          )}
        >
          {statusLabel}
        </span>
        {snapshot ? (
          <span className="numeric">
            {t("status.lastUpdate")}: {snapshot.collected_at_utc}
          </span>
        ) : (
          <span>{t("states.noSampleYet")}</span>
        )}
      </div>
    </header>
  );
}

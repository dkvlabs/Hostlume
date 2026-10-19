/**
 * Overview — Phase 01 is an honest empty state: the shell is live, metric
 * collectors do not exist yet, so no cards with numbers are rendered. No
 * simulated values, ever (AGENTS.md §5).
 */
import { useT } from "@/lib/i18n";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useTelemetryStore } from "@/stores/telemetryStore";

export function OverviewPage() {
  const t = useT();
  const connection = useTelemetryStore((s) => s.connection);
  const error = useTelemetryStore((s) => s.error);
  const snapshot = useTelemetryStore((s) => s.snapshot);

  return (
    <div className="grid gap-4 p-5">
      <Card data-testid="overview-state">
        <CardHeader>
          <CardTitle>{t("overview.title")}</CardTitle>
        </CardHeader>
        <CardContent className="text-sm">
          {connection === "error" ? (
            <p className="text-critical" role="alert">
              {t("status.error")}
              {error ? ` — ${error}` : ""}
            </p>
          ) : snapshot === null ? (
            <p className="text-muted" role="status">
              {t("overview.waiting")}
            </p>
          ) : (
            <p className="text-muted" role="status">
              {t("overview.emptyCollectors")}
            </p>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

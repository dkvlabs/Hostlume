/**
 * Applications page (FX-007): virtualized, sortable, filterable process
 * table; detail drawer with per-process CPU history; quit / force quit
 * behind an explicit confirmation dialog. All termination guards live in
 * Rust — this UI can only *ask*.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { ConfirmActionDialog } from "@/components/ui/ConfirmActionDialog";
import { ipc } from "@/lib/ipc/client";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { useT } from "@/lib/i18n";
import { formatByUnit } from "@/lib/formatting/units";
import {
  PROCESS_POLL_MS,
  processKey,
  sortRows,
  useProcessStore,
  type ProcessSortKey,
} from "./processStore";
import type { ProcessRow } from "@/lib/ipc/types";

const ROW_HEIGHT = 32;
const VIEWPORT_HEIGHT = 480;
const OVERSCAN = 8;

type SortDir = "desc" | "asc";

type MessageKey = Parameters<ReturnType<typeof useT>>[0];

const OUTCOME_KEYS: Record<string, MessageKey> = {
  ok: "applications.actionOk",
  identity_changed: "applications.identityChanged",
  process_exited: "applications.processExited",
  protected_target: "applications.protectedTarget",
  permission_denied: "applications.permissionDenied",
  unsupported_on_platform: "applications.unsupportedOnPlatform",
  failed: "applications.failed",
};

function outcomeMessage(outcome: string): MessageKey {
  return OUTCOME_KEYS[outcome] ?? "errors.unexpected";
}

function ProcessDrawer({
  row,
  onClose,
  onAction,
}: {
  row: ProcessRow;
  onClose: () => void;
  onAction: (row: ProcessRow, mode: "graceful" | "force") => void;
}) {
  const t = useT();
  const cpuHistory = useProcessStore((s) => s.cpuHistory[processKey(row)]);
  const latest = cpuHistory?.at(-1)?.v ?? null;
  return (
    <div
      className="fixed inset-y-0 right-0 z-40 w-96 overflow-auto border-l border-stroke bg-surface p-5"
      role="complementary"
      aria-label={t("applications.detail")}
    >
      <div className="flex items-center justify-between">
        <h2 className="truncate text-base font-semibold">{row.name}</h2>
        <Button variant="ghost" size="sm" onClick={onClose} aria-label={t("common.cancel")}>
          ✕
        </Button>
      </div>
      <dl className="mt-3 grid gap-1 text-xs">
        <div className="flex justify-between">
          <dt className="text-muted">{t("applications.col.pid")}</dt>
          <dd className="numeric">{row.pid}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-muted">{t("applications.col.cpu")}</dt>
          <dd className="numeric" title={t("applications.col.cpuHint")}>
            {latest === null ? "—" : formatByUnit(latest, "percent", 1)}
          </dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-muted">{t("applications.col.memory")}</dt>
          <dd className="numeric">{formatByUnit(row.rssBytes, "bytes")}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-muted">{t("applications.started")}</dt>
          <dd className="numeric">
            {new Date(row.startTimeSec * 1000).toISOString().replace("T", " ").slice(0, 16)} UTC
          </dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-muted">{t("applications.parent")}</dt>
          <dd className="numeric">{row.parentPid ?? "—"}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-muted">{t("applications.status")}</dt>
          <dd>{row.status}</dd>
        </div>
      </dl>
      <p className="mt-2 text-[10px] text-muted">{t("applications.col.cpuHint")}</p>
      <div className="mt-4 flex gap-2">
        <Button variant="secondary" onClick={() => onAction(row, "graceful")}>
          {t("applications.quit")}
        </Button>
        <Button variant="destructive" onClick={() => onAction(row, "force")}>
          {t("applications.forceQuit")}
        </Button>
      </div>
    </div>
  );
}

export function ApplicationsPage() {
  const t = useT();
  const page = useProcessStore((s) => s.page);
  const error = useProcessStore((s) => s.error);
  const refresh = useProcessStore((s) => s.refresh);
  const [filter, setFilter] = useState("");
  const [sortKey, setSortKey] = useState<ProcessSortKey>("memory");
  const [sortDir, setSortDir] = useState<SortDir>("desc");
  const [selected, setSelected] = useState<ProcessRow | null>(null);
  const [confirmMode, setConfirmMode] = useState<"graceful" | "force" | null>(null);
  const [lastOutcome, setLastOutcome] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);

  // Single poll source for the whole page — active only while mounted.
  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), PROCESS_POLL_MS);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const rows = useMemo(() => {
    const filtered = (page?.rows ?? []).filter((row) =>
      filter === "" ? true : row.name.toLowerCase().includes(filter.toLowerCase())
    );
    return sortRows(filtered, sortKey, sortDir === "asc");
  }, [page, filter, sortKey, sortDir]);

  // Virtualization: render only the visible window (+overscan).
  const totalHeight = rows.length * ROW_HEIGHT;
  const firstVisible = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  const visibleCount = Math.ceil(VIEWPORT_HEIGHT / ROW_HEIGHT) + OVERSCAN * 2;
  const windowed = rows.slice(firstVisible, firstVisible + visibleCount);

  const setSort = (key: ProcessSortKey) => {
    if (key === sortKey) {
      setSortDir((dir) => (dir === "desc" ? "asc" : "desc"));
    } else {
      setSortKey(key);
      setSortDir(key === "name" ? "asc" : "desc");
    }
  };

  const requestAction = async (row: ProcessRow, mode: "graceful" | "force") => {
    const result = await ipc.requestProcessAction({
      pid: row.pid,
      startTimeSec: row.startTimeSec,
      mode,
      confirmationId: `ui-${Date.now()}`,
    });
    setLastOutcome(result.outcome);
    void refresh();
  };

  const header = (key: ProcessSortKey, label: string, extraClass = "") => (
    <button
      type="button"
      className={`flex items-center gap-1 text-left text-xs font-medium text-muted hover:text-text ${extraClass}`}
      onClick={() => setSort(key)}
      aria-sort={sortKey === key ? (sortDir === "asc" ? "ascending" : "descending") : "none"}
    >
      {label}
      {sortKey === key ? <span aria-hidden>{sortDir === "asc" ? "↑" : "↓"}</span> : null}
    </button>
  );

  return (
    <div className="relative grid gap-3 p-5" style={{ height: "100%", gridTemplateRows: "auto 1fr" }}>
      <div className="flex items-center gap-3">
        <input
          type="search"
          value={filter}
          onChange={(event) => setFilter(event.target.value)}
          placeholder={t("applications.search")}
          aria-label={t("applications.search")}
          className="h-8 w-64 rounded-[8px] border border-stroke bg-surface px-3 text-sm focus-visible:outline-2 focus-visible:outline-accent"
        />
        {page?.cpuWarmup ? <Badge variant="accent">{t("applications.warmup")}</Badge> : null}
        {error ? (
          <span className="text-xs text-critical" role="alert">
            {error}
          </span>
        ) : null}
        {lastOutcome ? (
          <span className="text-xs text-muted" role="status">
            {t(outcomeMessage(lastOutcome))}
          </span>
        ) : null}
      </div>

      <Card className="overflow-hidden">
        <CardContent className="flex h-full flex-col p-0">
          <div className="grid border-b border-stroke px-3 py-2" style={{ gridTemplateColumns: "1fr 5rem 6rem 7rem" }}>
            {header("name", t("applications.col.name"))}
            {header("pid", t("applications.col.pid"))}
            {header("cpu", t("applications.col.cpu"))}
            {header("memory", t("applications.col.memory"))}
          </div>
          <div
            ref={scrollRef}
            className="min-h-0 flex-1 overflow-auto"
            style={{ maxHeight: VIEWPORT_HEIGHT }}
            onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
          >
            {rows.length === 0 ? (
              <p className="p-4 text-sm text-muted">
                {page ? t("applications.empty") : t("applications.warmup")}
              </p>
            ) : (
              <div style={{ height: totalHeight, position: "relative" }}>
                <div style={{ transform: `translateY(${firstVisible * ROW_HEIGHT}px)` }}>
                  {windowed.map((row) => (
                    <button
                      key={processKey(row)}
                      type="button"
                      onClick={() => setSelected(row)}
                      className="grid w-full items-center px-3 text-left text-xs hover:bg-surface-raised"
                      style={{ height: ROW_HEIGHT, gridTemplateColumns: "1fr 5rem 6rem 7rem" }}
                    >
                      <span className="truncate">{row.name}</span>
                      <span className="numeric text-muted">{row.pid}</span>
                      <span className="numeric">
                        {row.cpuUsageCores === null
                          ? "—"
                          : formatByUnit(row.cpuUsageCores, "percent", 1)}
                      </span>
                      <span className="numeric">{formatByUnit(row.rssBytes, "bytes")}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {selected ? (
        <ProcessDrawer
          row={selected}
          onClose={() => setSelected(null)}
          onAction={(_row, mode) => setConfirmMode(mode)}
        />
      ) : null}

      {selected && confirmMode ? (
        <ConfirmActionDialog
          open
          destructive={confirmMode === "force"}
          title={
            confirmMode === "force"
              ? t("applications.confirmForceTitle")
              : t("applications.confirmQuitTitle")
          }
          body={`${selected.name} (PID ${selected.pid}) — ${
            confirmMode === "force"
              ? t("applications.confirmForceBody")
              : t("applications.confirmQuitBody")
          }`}
          confirmLabel={
            confirmMode === "force" ? t("applications.forceQuit") : t("applications.quit")
          }
          onConfirm={() => {
            const row = selected;
            void requestAction(row, confirmMode);
          }}
          onCancel={() => setConfirmMode(null)}
        />
      ) : null}
    </div>
  );
}

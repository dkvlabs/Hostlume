/**
 * App shell (design.md §3): sidebar navigation, status strip, page surface.
 * Global shortcuts: Ctrl/Cmd+, opens Settings; Ctrl/Cmd+1..7 switch pages.
 * Reserved OS shortcuts are not intercepted.
 */
import { useCallback, useEffect, useState } from "react";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Header } from "@/app/Header";
import { PAGE_IDS, Sidebar, type PageId } from "@/app/Sidebar";
import { useTelemetry } from "@/app/useTelemetry";
import { ApplicationsPage } from "@/features/applications/ApplicationsPage";
import { NetworkPage } from "@/features/network/NetworkPage";
import { OverviewPage } from "@/features/overview/OverviewPage";
import { StoragePage } from "@/features/storage/StoragePage";
import { SensorsPage } from "@/features/sensors/SensorsPage";
import { PlaceholderPage } from "@/features/placeholder/PlaceholderPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { useT } from "@/lib/i18n";
import type { MessageKey } from "@/lib/i18n/dictionaries";
import { applyAppearance, resolveTheme, watchSystemTheme } from "@/lib/theme";
import { useSettingsStore } from "@/stores/settingsStore";

const PAGE_TITLE_KEYS: Record<PageId, MessageKey> = {
  overview: "nav.overview",
  applications: "nav.applications",
  storage: "nav.storage",
  network: "nav.network",
  sensors: "nav.sensors",
  compare: "nav.compare",
  settings: "nav.settings",
};

function renderPage(page: PageId, title: string) {
  switch (page) {
    case "overview":
      return <OverviewPage />;
    case "settings":
      return <SettingsPage />;
    case "applications":
      return <ApplicationsPage />;
    case "storage":
      return <StoragePage />;
    case "network":
      return <NetworkPage />;
    case "sensors":
      return <SensorsPage />;
    case "compare":
      return <PlaceholderPage title={title} />;
  }
}

export function App() {
  const [page, setPage] = useState<PageId>("overview");
  const settings = useSettingsStore((s) => s.settings);
  const loadSettings = useSettingsStore((s) => s.load);
  const t = useT();
  const title = t(PAGE_TITLE_KEYS[page]);

  useTelemetry();

  useEffect(() => {
    void loadSettings();
  }, [loadSettings]);

  // Theme + density + language side effects (persistence lives in SQLite;
  // this reflects the loaded settings onto the document).
  useEffect(() => {
    if (!settings) return;
    const prefersLight =
      typeof window.matchMedia === "function" &&
      window.matchMedia("(prefers-color-scheme: light)").matches;
    applyAppearance(settings.appearance, prefersLight, settings.locale);
    return watchSystemTheme(settings.appearance, (systemPrefersLight) => {
      document.documentElement.dataset.theme = resolveTheme(
        settings.appearance,
        systemPrefersLight
      );
    });
  }, [settings]);

  const selectPage = useCallback((next: PageId) => setPage(next), []);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey)) return;
      if (event.key === ",") {
        event.preventDefault();
        setPage("settings");
        return;
      }
      const index = Number.parseInt(event.key, 10);
      if (Number.isInteger(index) && index >= 1 && index <= PAGE_IDS.length) {
        const target = PAGE_IDS[index - 1];
        if (target) {
          event.preventDefault();
          setPage(target);
        }
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  return (
    <TooltipProvider delayDuration={200}>
      <div className="flex h-full min-h-0" data-compact={settings?.compact_mode ? "true" : undefined}>
        <Sidebar active={page} onSelect={selectPage} />
        <div className="flex min-w-0 flex-1 flex-col">
          <Header pageTitle={title} />
          <main className="min-h-0 flex-1 overflow-auto">{renderPage(page, title)}</main>
          <footer className="flex h-8 shrink-0 items-center gap-3 border-t border-stroke px-5 text-xs text-muted">
            <span>{t("app.name")}</span>
            <span aria-hidden>·</span>
            <span>{t("nav.settingsShortcut")}</span>
          </footer>
        </div>
      </div>
    </TooltipProvider>
  );
}

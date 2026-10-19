/**
 * Theme application: resolves dark/light/system onto `data-theme` and keeps
 * `lang` in sync. The attribute (not a class) is the single source consumed
 * by globals.css; dark is the canonical default.
 */
import type { Appearance } from "@/lib/ipc/types";

export function resolveTheme(appearance: Appearance, systemPrefersLight: boolean): "light" | "dark" {
  if (appearance === "system") return systemPrefersLight ? "light" : "dark";
  return appearance;
}

export function applyAppearance(appearance: Appearance, systemPrefersLight: boolean, lang?: string): void {
  const root = document.documentElement;
  root.dataset.theme = resolveTheme(appearance, systemPrefersLight);
  if (lang) {
    root.lang = lang;
  }
}

/** Subscribe to OS theme changes while in system mode. Returns unlisten. */
export function watchSystemTheme(
  appearance: Appearance,
  apply: (systemPrefersLight: boolean) => void
): () => void {
  if (appearance !== "system" || typeof window.matchMedia !== "function") {
    return () => {};
  }
  const query = window.matchMedia("(prefers-color-scheme: light)");
  const onChange = (event: MediaQueryListEvent) => apply(event.matches);
  query.addEventListener("change", onChange);
  return () => query.removeEventListener("change", onChange);
}

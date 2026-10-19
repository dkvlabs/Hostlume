import { useSettingsStore } from "@/stores/settingsStore";
import { en, ro, type Dictionary, type MessageKey } from "./dictionaries";

export const LOCALES = ["en", "ro"] as const;
export type Locale = (typeof LOCALES)[number];

const DICTIONARIES: Record<Locale, Dictionary> = { en, ro };

export function isSupportedLocale(value: string): value is Locale {
  return (LOCALES as readonly string[]).includes(value);
}

export function translate(locale: Locale, key: MessageKey): string {
  return DICTIONARIES[locale][key] ?? en[key] ?? key;
}

/** Hook: the translate function bound to the current settings locale. */
export function useT(): (key: MessageKey) => string {
  const locale = useSettingsStore((state) =>
    state.settings && isSupportedLocale(state.settings.locale)
      ? (state.settings.locale as Locale)
      : "en"
  );
  return (key: MessageKey) => translate(locale, key);
}

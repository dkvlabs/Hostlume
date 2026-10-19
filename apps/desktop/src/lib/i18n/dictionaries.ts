/**
 * i18n message catalogs. Keys are stable identifiers resolved at render
 * time; no concatenated translated strings (requirements.md). English is
 * canonical; Romanian is prepared from day one.
 */

export const en = {
  "app.name": "APP_NAME",
  "nav.overview": "Overview",
  "nav.applications": "Applications",
  "nav.storage": "Storage",
  "nav.network": "Network",
  "nav.sensors": "Sensors",
  "nav.compare": "Compare",
  "nav.settings": "Settings",
  "nav.settingsShortcut": "Settings (Ctrl/Cmd+,)",

  "status.connecting": "Connecting…",
  "status.live": "Live",
  "status.error": "Connection error",
  "status.lastUpdate": "Last update",

  "states.collecting": "Collecting…",
  "states.unsupported": "Not supported",
  "states.permissionRequired": "Permission required",
  "states.temporarilyUnavailable": "Temporarily unavailable",
  "states.stale": "Stale",
  "states.error": "Error",
  "states.noSampleYet": "No sample yet",

  "overview.title": "Overview",
  "overview.waiting": "Waiting for the first sample…",
  "overview.emptyCollectors":
    "The monitoring shell is ready. Metric collectors arrive in a later phase — no values are simulated.",

  "common.notImplemented": "Not implemented yet",
  "common.notImplementedHint":
    "This page ships with a later phase. It will never show simulated values in the meantime.",
  "common.retry": "Retry",

  "settings.title": "Settings",
  "settings.general": "General",
  "settings.closeToTray": "Close to tray",
  "settings.closeToTrayHint":
    "Stored now; takes effect when tray support arrives. Until then closing the window exits the app.",
  "settings.appearance": "Appearance",
  "settings.theme": "Theme",
  "settings.theme.system": "System",
  "settings.theme.light": "Light",
  "settings.theme.dark": "Dark",
  "settings.compactMode": "Compact spacing",
  "settings.compactModeHint": "Denser 12 px rhythm instead of 16 px card padding.",
  "settings.locale": "Language",
  "settings.monitoring": "Monitoring",
  "settings.polling": "Base polling interval",
  "settings.pollingHint": "Safe range 1–60 s. Default 3 s. Applies on the next tick.",
  "settings.saved": "Saved",
  "settings.saveFailed": "Change rejected",

  "capabilities.collectorNotImplemented":
    "Metric collection for this family is not implemented in this build yet.",
  "capabilities.wrongPlatform":
    "This adapter was built for a different operating system.",

  "errors.noSnapshotYet": "No sample has been collected yet.",
  "errors.settingsInvalid": "The change was rejected as invalid.",
  "errors.store": "The settings store reported an error.",
  "errors.unexpected": "Something went wrong.",
} as const;

export type MessageKey = keyof typeof en;
export type Dictionary = Record<MessageKey, string>;

export const ro: Dictionary = {
  "app.name": "APP_NAME",
  "nav.overview": "Prezentare generală",
  "nav.applications": "Aplicații",
  "nav.storage": "Stocare",
  "nav.network": "Rețea",
  "nav.sensors": "Senzori",
  "nav.compare": "Comparație",
  "nav.settings": "Setări",
  "nav.settingsShortcut": "Setări (Ctrl/Cmd+,)",

  "status.connecting": "Se conectează…",
  "status.live": "Activ",
  "status.error": "Eroare de conexiune",
  "status.lastUpdate": "Ultima actualizare",

  "states.collecting": "Se colectează…",
  "states.unsupported": "Neacceptat",
  "states.permissionRequired": "Necesită permisiune",
  "states.temporarilyUnavailable": "Temporar indisponibil",
  "states.stale": "Învechit",
  "states.error": "Eroare",
  "states.noSampleYet": "Încă niciun eșantion",

  "overview.title": "Prezentare generală",
  "overview.waiting": "Se așteaptă primul eșantion…",
  "overview.emptyCollectors":
    "Structura aplicației este gata. Colectorii de metrici sosesc într-o fază ulterioară — nu se simulează valori.",

  "common.notImplemented": "Neimplementat încă",
  "common.notImplementedHint":
    "Această pagină sosește într-o fază ulterioară. Între timp nu afișează valori simulate.",
  "common.retry": "Reîncearcă",

  "settings.title": "Setări",
  "settings.general": "Generale",
  "settings.closeToTray": "Închidere în bară",
  "settings.closeToTrayHint":
    "Salvat acum; devine activ odată cu suportul de bară. Până atunci, închiderea ferestrei închide aplicația.",
  "settings.appearance": "Aspect",
  "settings.theme": "Temă",
  "settings.theme.system": "Sistem",
  "settings.theme.light": "Deschisă",
  "settings.theme.dark": "Întunecată",
  "settings.compactMode": "Spațiere compactă",
  "settings.compactModeHint": "Ritm de 12 px în loc de 16 px pentru carduri.",
  "settings.locale": "Limbă",
  "settings.monitoring": "Monitorizare",
  "settings.polling": "Interval de bază de colectare",
  "settings.pollingHint": "Interval sigur 1–60 s. Implicit 3 s. Se aplică la următorul ciclu.",
  "settings.saved": "Salvat",
  "settings.saveFailed": "Modificare respinsă",

  "capabilities.collectorNotImplemented":
    "Colectarea acestei familii de metrici nu este încă implementată în această versiune.",
  "capabilities.wrongPlatform": "Acest adaptor a fost compilat pentru alt sistem de operare.",

  "errors.noSnapshotYet": "Nu s-a colectat încă niciun eșantion.",
  "errors.settingsInvalid": "Modificarea a fost respinsă ca fiind invalidă.",
  "errors.store": "Magazinul de setări a raportat o eroare.",
  "errors.unexpected": "Ceva nu a funcționat.",
};

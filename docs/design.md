# Design specification — original cross-platform monitor

**Design goal:** A compact, exceptionally readable performance workstation that feels native enough on each OS, while retaining one recognizable visual system. **Do not replicate iStatistica's screenshots, iconography or exact screen layouts.** This is an original design brief.

## 1. Product personality

- **Precise:** verified numbers, correct units, explicit hardware support.
- **Quiet:** low visual noise, no flashing gauges or endless animations.
- **Dense, not cramped:** useful monitoring information at a glance and accessible details on demand.
- **Honest:** unavailable metrics use designed, actionable states.
- **Responsive:** main windows, mini popup, and remote browser scale appropriately.

## 2. Visual language / tokens

Implement semantic CSS variables for both themes (starting suggestions; adjust with contrast tests):

| Token | Dark proposal | Light proposal | Usage |
|---|---|---|---|
| `--bg` | `#101318` | `#F5F7FA` | App canvas |
| `--sidebar` | `#151A21` | `#FFFFFF` | Sidebar |
| `--surface` | `#1A2028` | `#FFFFFF` | Panels/cards |
| `--surface-raised` | `#222A34` | `#F0F3F7` | Popovers/hover |
| `--stroke` | `#313B47` | `#D9E0E7` | Borders |
| `--text` | `#F2F5F8` | `#17212C` | Primary text |
| `--muted` | `#A6B0BD` | `#536171` | Secondary text |
| `--accent` | `#67D4D0` | `#167D84` | Active UI controls |
| `--warning` | `#F4BC6C` | `#9F5800` | Warm / caution |
| `--critical` | `#FB7C80` | `#B72F3D` | Destructive/critical |

Chart series suggestions: CPU teal, memory violet, GPU blue, network down cyan/up amber, disk read green/write pink, sensor temperature orange. **No color conveys availability or danger alone**: include labels, badges and icon shapes.

- Type: system UI font by OS (SF/Segoe UI/Noto-compatible); monospace tabular numerals for measurements.
- Scale: 12–14 px dense labels; 15–16 px body; 22–28 px key values; line-height 1.35–1.5.
- Radius: 10–14 px for cards, 7–8 px controls; minimal shadow and low blur by default.
- Grid baseline: 8 px spacing system. Card inner padding 16–20 px; compact mode 12 px.
- Themes: dark/light/system; accessible contrast and focus indicators; user-controlled density.

## 3. Desktop navigation and layout

```text
┌──────────────────┬──────────────────────────────────────────────────┐
│ APP_NAME         │ Machine / OS  • last update • status • actions   │
│                  ├──────────────────────────────────────────────────┤
│ ▣ Overview       │ PAGE TITLE      [time range] [filter] [export]   │
│ ◷ Applications   │                                                  │
│ ▦ Storage        │  ┌───────────────┐  ┌──────────────────────────┐ │
│ ⇅ Network        │  │ Metric card   │  │ Metric card              │ │
│ ◉ Sensors        │  │ 74%          │  │ 12.8 GiB                │ │
│ ≈ Compare        │  │ sparkline     │  │ sparkline                │ │
│                  │  └───────────────┘  └──────────────────────────┘ │
│ ⚙ Settings       │     additional cards / charts / tables          │
└──────────────────┴──────────────────────────────────────────────────┘
```

- Minimum main window proposed: 960×650 **logical** pixels; test platform minimums. Sidebar ~220 px, collapsible to icons ~68 px. Content uses 12-column grid; cards span 3/4/6/12 units according to screen width.
- At 960–1200 px use two-column cards; >1400 px may use three/four columns; narrow views stack, never overlap.
- Main app footer or discreet status strip: sampling paused/running, collection timestamp, database and optional remote server states.
- Menu labels: **Overview, Applications, Storage, Network, Sensors, Compare, Settings**. Battery, GPU and Bluetooth are context details or pinned cards, not obligatory sidebar entries if unsupported.
- Keyboard: `Cmd/Ctrl+K` quick navigation, `Cmd/Ctrl+,` settings, `Esc` close dialogs, native platform shortcuts where appropriate. Do not hijack reserved OS shortcuts.

## 4. Component inventory

- `MetricCard`: label, current value, unit, delta, availability indicator, last-sampled timestamp, sparkline, details affordance.
- `MetricAvailability`: compact state chip and tooltip/help action for `unsupported`, `permission_required`, `temporarily_unavailable`, `stale`, `error`; remediation info.
- `TimeRangePicker`: 1m/5m/15m/1h/24h/7d/custom according to retained history.
- `TimeSeriesChart`: UTC timestamps, units, zoom, legend, data-gap visualization, keyboard-accessible table option.
- `ProcessTable`: virtualized list with sortable columns and per-process detail drawer.
- `DeviceSelector`: stable device IDs with unplugged state handling.
- `StatusNotice`: inline, dismissible, direct actions where safe.
- `ConfirmActionDialog`: explicit target name/path, reversible effects, confirm with safe defaults.
- `SensorGroup`: sensor-type grouping, native name, optional user alias, min/max history.
- `ScanProgress`: discovered/visited count, approximate bytes, cancellation and access errors.
- `RecordingControl`: start/stop/annotate/export, visible capture status.
- `AlertRuleEditor`: metric picker, comparator, threshold, duration, cooldown, priority, actions.
- `TrayPopover`: 320–400 px wide compact status, 4–7 configurable rows, quick links.

Every component must have `loading`, `ready`, `empty`, `unsupported`, `permission_required`, `stale`, and `error` states as relevant. No UI component may transform `null` into `0%` or `0°C`.

## 5. Screen specification — Overview (FX-001–006, FX-005)

**Header:** hostname, OS/version, device uptime; refresh state; fast path to capability report. **Top strip:** CPU, memory, GPU, network down/up as primary values; cards for disk and battery below.

Cards:

1. **CPU:** total big number, per-core mini bars, 5 min sparkline, user/system breakdown when available, top CPU process link.
2. **Memory:** used/total, pressure if OS exposes it, available/cache/swap breakdown, 5 min history.
3. **Network:** active interface name, ↑ ↓ rates, transfer counters, 5 min history and switch interface.
4. **Storage:** volumes list with capacity bars, mount labels, I/O if available, Storage Analyzer deep link.
5. **GPU:** adapter switcher, utilization, dedicated VRAM or shared/unified memory indication, display details; source label.
6. **Sensors:** temperature/fan tiles with optional min/max, hidden behind capability notice if no supported sensors.
7. **Battery:** charge, power source, health/cycles/capacity only when device supports them.

Card layout can be customized (show/hide/reorder/size) and persisted. First-use default layout should prioritize verified universal measurements instead of unsupported temperature placeholders.

## 6. Applications screen (FX-007)

- Search process/app name, filter user/system, sort CPU/memory/threads/name, snapshot timestamp.
- Large table: icon, name, PID, CPU (display semantic toggle), RAM, optional disk/network, process start time. Virtualize rows; avoid re-sorting during keyboard interaction.
- Select row opens detail drawer: CPU/memory history, executable path if accessible, parent process and permission status.
- Primary action: **Quit** (graceful where possible). Secondary critical action: **Force Quit**. Both show clear confirmation if destruction of unsaved work is possible. Revalidate PID + creation time in Rust before action.
- Handle exited process between click and confirmation with non-destructive status; show protected/system processes with disabled action and reason.

## 7. Storage screen (FX-008, FX-009)

**Flow:** choose volume/folder → confirm scan boundary → observe progress → interact with proportional treemap → click node to drill down → select file(s) → view path/size → Move to Trash with confirmation.

Design:

- Breadcrumb with `Back`, selected root and `Rescan` control.
- Treemap of colored rectangles sized by **reported logical bytes**, an explicit legend and accessible fallback hierarchical table.
- Side detail pane: selected path, readable size, allocated size if available, last modified, open in file manager, Move to Trash.
- Scanning shows discovered/visited nodes, access-denied warnings, partial total, cancel button. For very large trees render top N children + “Other” grouping; never allocate one DOM node for each file.
- On a symlink, show badge `Symbolic link` and do not recursively follow by default. A mount boundary warning is explicit. On Trash failure offer a useful OS error, not permanent delete by fallback.
- I/O tab shows read/write throughput, device selection and rate history.

## 8. Network screen (FX-006, FX-011)

- Interface cards with address, status and receive/send rate; allow selecting any interface including VPN/virtual.
- Large synchronized in/out chart; current and cumulative counters separated.
- Details for IPv4/IPv6, gateway, hardware address (local UI only), driver/speed if available.
- External IP / geolocation button is initially **disabled** and states that an external request is required; show settings opt-in.
- Do not add physical and virtual counters to an “overall bandwidth” figure unless the aggregation is carefully defined to avoid double counting.

## 9. Sensors screen (FX-010, FX-012–014, FX-021)

Tabs/panels: **Temperatures, Fans, GPU, Battery, Bluetooth, Displays, Capabilities**. Only accessible/available data gets live graphs.

- Temperatures: CPU/GPU/drive/motherboard sensor group; current/min/max/chart; °C/°F setting and clear source.
- Fans: measured RPM and availability; no sliders or fan-speed controls.
- GPU: utilization chart and memory; distinguish dedicated VRAM and shared memory; multi-adapter switch.
- Battery: charge, power source, condition, reported cycles, current/designed capacity, time estimate (if available).
- Bluetooth: supported paired/connected devices; battery shown only for devices exposing it.
- Capabilities: one row per metric type with source, status, reason, permission guidance, option to re-probe. Avoid presenting “Install random tool” advice as universal.

## 10. Compare and recording (FX-018)

- Select up to 4 metric series (may choose independent axis only with clearly labeled units).
- Shared cursor/time grid; crosshair tooltip, zoom, brush range, table export, gap markers.
- `Start recording` starts an explicitly labeled session. `Stop` sets end timestamp. Sessions persist across app restarts. CSV includes units, UTC timestamps, source and missingness; optional import validates schema.
- Recording may require longer raw retention; warn if user's retention would delete sessions and provide explicit export option.

## 11. Alerts (FX-017)

Located under **Settings → Alerts** with active-rules summary on dashboard.

- Rule builder: metric + entity + comparator + threshold + sustained duration + hysteresis + cooldown + notification action; optional locally-approved workflow.
- Show a human sentence preview e.g. “Notify when CPU total stays above 90% for 60 seconds.”
- Test rule against **fixture data** without triggering real scripts; event log displays activation/resolution timestamps.
- Disable controls for unavailable metrics with explanation. Don't allow untrusted metric text to flow into a shell invocation.

## 12. Settings (FX-020)

**General:** launch at login, close-to-tray, base interval, energy saver.  
**Appearance:** theme, compact spacing, accent, chart motion, units.  
**Monitoring:** modules enabled, metric polling overrides within safe bounds, sensor aliases.  
**Dashboard & Tray:** card order, visible tray indicators, popover details.  
**Alerts:** rules, notification access, workflow consent.  
**History:** retention, recording quota, export and erase.  
**Remote:** disabled/loopback/LAN modes, bind/port, TLS, token reset, connections.  
**Advanced:** capabilities, privacy/export diagnostics, helper permission status, updates, logs.

Validation is immediate and reversible; sensitive settings require confirmation. Restart-required options are labeled.

## 13. Menu bar, tray and widgets (FX-015–016)

**macOS:** compact icon(s) or OS-approved item(s), dynamic text if available, click opens popover; independent rows for CPU/RAM/traffic/battery; size and item count configurable. Respect notch/menu bar space and macOS appearance changes.  
**Windows:** one or more bounded notification-area icons if supported; one main popup on click and context menu for pause/open/quit. Avoid promising permanent visibility when OS controls overflow.  
**Linux:** StatusNotifier/AppIndicator where supported; optional floating always-on-top compact UI when tray absent (e.g. some GNOME setups).  
**Widgets:** WidgetKit macOS extension is its own signed delivery item. Windows widget provider integration and Linux native widgets are separate feasibility gates; floating compact monitor is valid functional fallback.

**Popup blueprint:** top label + sampling status, four configurable measurement rows, small traffic chart, Open dashboard, Pause/Resume, Settings, Quit. Background collector does not restart when popover opens.

## 14. Remote browser dashboard (FX-019)

- Separate responsive read-only design, auth on first access, explicit host badge, last-updated stamp, overview charts and device lists.
- Phone 360 px: stack cards and simplify legends. Tablet 768 px: two columns. Desktop 1280 px: full grid.
- No quit/force-quit/Trash/workflow buttons, even visually; server does not expose such API routes.
- Token entry must never be stored in query strings, analytics, referrer logs or third-party scripts; use secure session handling with XSS/CSRF review appropriate to chosen auth mode.
- Clear warning to user before enabling LAN, with active listen interface and TLS state.

## 15. States, feedback and accessibility

| State | Visual treatment | Action |
|---|---|---|
| `available` | live value, normal graph | details |
| `temporarily_unavailable` | “Collecting…” and placeholder dash | retry automatically |
| `unsupported` | subdued placeholder, `Not supported` label | learn why |
| `permission_required` | permission-needed icon + short explanation | open settings/docs |
| `stale` | desaturated last-known number + timestamp | recheck source |
| `error` | descriptive text, diagnostic link | retry/report |
| `offline` remote host | offline/last seen state | reconnect/manual refresh |

All graphs provide a non-color table alternative, logical headings, chart descriptions, labeled buttons, keyboard focus, screen-reader status updates **without** announcing every 3-second metric refresh. Use `prefers-reduced-motion` and test zoom at 200%.

## 16. UI acceptance / design review checklist

- [ ] Consistent 8 px rhythm, semantic tokens, dark/light/system.
- [ ] No blank or fabricated GPU/sensor values on unsupported hardware.
- [ ] Charts display real gaps when readings are missing or after sleep.
- [ ] Table navigation and control activation work by keyboard.
- [ ] Large tree scans and 10k-row process tables don't block scrolling.
- [ ] Destructive actions use contextual confirmation and screen-reader labels.
- [ ] macOS menu bar, Windows tray and Linux fallback are validated separately.
- [ ] Remote dashboard does not expose local machine controls.
- [ ] High contrast, 200% zoom, reduced motion, 320–360 px remote layout verified.
- [ ] Screenshots created from app's own UI only; no copied reference assets.

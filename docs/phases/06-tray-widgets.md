# Phase 06 — Native menu bar, system tray and widgets

**Owns:** FX-015, FX-016.  
**Outcome:** Low-overhead monitoring even when the main window is closed, with native OS affordances or explicitly supported fallback.

## 1. Preconditions

- Phase 02 real snapshots and Phase 05 capability registry.
- Platform desktop-environment feasibility documented in `docs/platform-capabilities.md`.

## 2. Implementation sequence

1. Implement a tray controller that subscribes to the existing normalized snapshot bus. **No independent duplicate polling loop** per icon or popup.
2. Design one main tray icon + compact popover with CPU, RAM, download/upload and user-chosen sensors; add pause/resume, open Overview, Settings, Quit.
3. Persist visible indicators/order/units/update rate. Throttle expensive icon rendering and redraw only when displayed value changes (within sensible limits).
4. **macOS:** implement Tauri 2 tray and optional appropriate native menu bar item layout(s); test Retina, dark/light, notch space, menu bar overflow. Test close-to-tray and `Quit` as distinct actions.
5. **Windows:** use notification-area tray; test Explorer restart, monitor switching, DPI, hidden overflow and predictable context/left click behavior.
6. **Linux:** implement StatusNotifier/AppIndicator when supported; detect absent tray; offer a small, optional floating always-on-top monitor and clear explanation. Validate GNOME/KDE Wayland, X11 where available.
7. Native widgets: implement **macOS WidgetKit** read-only extension only after confirming toolchain, entitlement, signed bundle and safe shared snapshots. Widget updates must respect OS scheduler; not 3-second polling.
8. Investigate Windows widget provider and Linux desktop widget feasibility. If maintaining true native integrations is impractical, ship floating widget fallback, label it honestly and document the limitation.
9. Ensure background collector respects battery/thermal load settings and cleanly stops on actual Quit. Remote server also stops on Quit.

## 3. Tests / manual verification

- Close main window → tray remains → background CPU samples continue → re-open same app without duplicate sampler.
- `Pause` stops sampling (or switches into explicit paused state); `Resume` restarts with new counter baseline.
- Quit removes tray icon, shuts down collector + SQLite writer + optional remote server and exits process.
- Restart Windows Explorer; GNOME with no tray extension; KDE/Wayland; macOS dark/light and multiple displays.
- Widget displays last snapshot timestamp, handles OS refresh throttling and becomes stale gracefully if app not running.
- Tray popover keyboard accessibility and menu action focus.

## 4. Deliverables

- Cross-platform tray controller, native behavior adapters and popup UI.
- Persisted per-user configuration.
- macOS widget extension only if packaged and genuinely tested; otherwise explicit gated status.
- Floating compact monitor fallback for unsupported native widget/tray environments.
- Energy/battery impact and visual acceptance evidence.

## 5. Acceptance gate

- [ ] One background collector, no duplicate Tauri process after reopening.
- [ ] Native tray works on real macOS/Windows; Linux path verified by DE, fallback works without tray support.
- [ ] Start-at-login, close-to-tray, pause and quit have explicit safe semantics.
- [ ] Tray shows *real current values or unavailable/stale*, never mock values.
- [ ] macOS widgets either truly work as signed native extension or remain explicitly gated; no fake native widget claim.
- [ ] Logs + hardware/desktop state captured in `docs/progress/PHASE-06.md`.

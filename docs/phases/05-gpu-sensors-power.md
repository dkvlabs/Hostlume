# Phase 05 — Hardware-dependent GPU, temperature, fans, battery, Bluetooth

**Owns:** FX-010, FX-012, FX-013, FX-014, FX-021.  
**Outcome:** Honest native hardware visibility and a detailed user-facing capability report. **Success includes proving a reading is unavailable**, not faking it.

## 1. Preconditions

- Phase 02 collector traits and scheduler verified; Phase 03 history ready.
- Platform capability risks from Phase 00 investigated and licenses reviewed.

## 2. Implementation sequence

1. Build a user-facing `CapabilityReport` with `metric_id`, status, data source, OS/hardware notes, optional helper requirement, permission request, last probe timestamp and remediation action.
2. Enumerate graphics adapters and displays: adapter/vendor, active/inactive, driver/OS-reported name, display names/resolutions if exposed; avoid assuming dedicated VRAM on unified-memory architectures.
3. Investigate GPU usage from **supported** platform/vendor APIs. Implement narrowly per tested vendor/device; record GPU engine semantics. If unsupported, retain device identity but mark usage `unsupported`.
4. Implement read-only sensor enumeration and stable sensor IDs. On Linux, normalize hwmon temperature scaling and fan RPM; on Windows/macOS use reviewed interfaces only. No write endpoints, fan control, kernel drivers or privileged defaults.
5. Implement battery/power source readings with health, cycles, design/current capacity and estimated time **only where exposed**; do not invent battery health from arbitrary guesses.
6. Implement Bluetooth accessory enumeration and charge via OS-exposed profiles/characteristics where permitted. Missing battery characteristic → `unsupported`, not `0%`.
7. Wire sensors/GPU/power/Bluetooth into metric history with slower polling schedules; avoid waking/hammering inactive devices to refresh UI.
8. Build Sensors screen tabs and Overview cards as per design. For unavailable data, show a capability explanation and potential manually initiated re-probe.
9. If a native helper is truly essential, write its **separate** threat model, entitlement/license analysis, install/upgrade/uninstall documentation, then ask for architectural sign-off before packaging.

## 3. OS-specific evidence tasks

- **macOS:** Apple Silicon and Intel sensor read availability, public API coverage, thermal source licensing and signing; battery present/absent; integrated GPU/unified memory wording.
- **Windows:** WDDM/vendor driver GPU cases, GPU engines; sensor read feasibility without third-party privileged drivers; desktop vs laptop battery data.
- **Linux:** `/sys/class/hwmon` unit conversion, UPower/BlueZ D-Bus, DRM/vendor tools, missing hwmon drivers, Wayland session effects where relevant.

## 4. Tests

- Metric parser tests (millidegree → °C, W and RPM units, null handling, sensor disappearing).
- Vendor/GPU adapter normalization, no false unified VRAM number, multi-GPU enumeration, GPU engine vs overall definition.
- Battery absent, health missing, cycle count missing, AC↔battery state changes, full charge and physically stale estimates.
- Bluetooth supported device with charge and device without charge, disconnect and re-pair.
- Permissions revoked midstream, helper absent/crashed; UI display and history gracefully degrade.
- Real hardware reading corroborated by trusted native tool when possible; test source must be named.

## 5. Acceptance gate

- [ ] GPU and display metadata populate where OS exposes them; conditional live utilization correctly sourced.
- [ ] Temperature/fan readings use real OS/driver values and stable units, no fabricated defaults.
- [ ] Battery and Bluetooth screens distinguish absent, unsupported, permission-required and healthy cases.
- [ ] Capability details identify source and next steps; UI isn't empty or misleading on sensor-less hardware.
- [ ] At least one real hardware test per available platform with device/driver ID and notes.
- [ ] Actual unsupported hardware combinations recorded in `docs/progress/PHASE-05.md`; do not call them “fully supported”.

**Priority reminder:** hardware support coverage is conditional. Retain functional P0 monitor even when some sensors are blocked.

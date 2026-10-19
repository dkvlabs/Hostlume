# Reference audit — iStatistica Pro

**Reference URL:** https://www.imagetasks.com/istatistica/pro/  
**Secondary listing:** https://apps.apple.com/us/app/istatistica-pro/id1447778660  
**Audit date:** 18.10.2026  
**Method:** Public website and store descriptions; no binary decompilation, source access, interactive inspection, private API interception, or claims about internal architecture.

## Verified publicly described features

| Area | Evidence from public descriptions | Our mapping |
|---|---|---|
| Dashboard | Uptime; CPU, RAM, disks, network; macOS memory pressure | FX-001 through FX-006 |
| Applications | CPU usage history, memory/RAM map, quit and force quit; original plugin dependency noted | FX-007 |
| GPU | GPU utilization, VRAM, display name/resolution | FX-010 |
| Storage | Disk/folder size visualization, largest content, Move to Trash, drive statistics | FX-008, FX-009 |
| Network | All interface dashboard, LAN/gateway/WAN IP, traffic speed/history | FX-006, FX-011 |
| Sensors | Temperature, fans and disk I/O through optional sensors plugin | FX-009, FX-012 |
| Battery | Capacity, cycles, health, charge, remaining-time estimate | FX-013 |
| Bluetooth | Supported device battery readings (examples: AirPods, Magic accessories) | FX-014 |
| Widgets | macOS Notification Center widgets | FX-016 |
| Menu bar | CPU, memory, disk, network, battery, graphics, sensors; customization | FX-015 |
| Alerts | Conditions based on CPU/RAM/disk/network/GPU/sensors plus terminal/workflow actions | FX-017 |
| Comparison | Comparison charts, dashboard record control, CSV export | FX-018 |
| Web access | Mobile-friendly LAN dashboard, JSON REST API, IP/passkey restrictions | FX-019 |

Primary supporting pages:

- [Official iStatistica Pro feature page](https://www.imagetasks.com/istatistica/pro/)
- [Mac App Store feature list and release history](https://apps.apple.com/us/app/istatistica-pro/id1447778660)

## Important qualifications

1. The original is a **macOS app**; Windows and Linux equivalents are **our new implementation**, not existing reference behavior.
2. iStatistica's separate sensors plugin is a **product-specific** mechanism. We are not entitled to ship it or call its APIs. Our own collectors/helpers must be independently implemented and permissioned.
3. The reference site's REST command list documents the **original API**, not a required compatibility contract for our application. We implement a **new `/api/v1` API**, not ImageTasks protocol emulation. Any compatibility mode is a separately reviewed future decision.
4. The reference documents some data through charts/images but does **not** specify precise chart sampling, SQL schema, all event semantics, min OS versions for our app, or internal architectural choices. Treat those as new product decisions, not reversed facts.
5. Exact per-model GPU/fan/temperature support is **not established** by screenshots or a marketing description. Every target machine must detect its own capabilities.
6. External IP/country is described by the original, but our privacy-first app should require explicit opt-in and show its service provider.
7. The reference's macOS widgets and menu bar cannot be cloned 1:1 on Windows/Linux; platform-appropriate tray/floating widgets are the intended functional equivalents.

## Competitive differentiation (original, optional)

- Explicit metric availability and capability diagnostics (parity UX improvement).
- Cross-platform native monitoring (central product differentiator).
- Strong default-local remote-server security with token lifecycle and TLS for LAN.
- Better long-term history, measured overhead budgets, and clear CSV field semantics.
- Phase 10 **optional** multi-host aggregator — *not* a reference feature.

## What must be verified interactively (if app is available)

Create findings in `docs/evidence/REFERENCE_WALKTHROUGH.md` before claiming precise UX parity:

- App navigation hierarchy, keyboard accelerators and tray popup dimensions.
- Disk treemap hierarchy, hover behavior, sorting and cancellation UX.
- What specific apps/proc events and history durations are shown.
- Comparison graph behavior and CSV columns.
- Preferences/alert rule options and update-rate boundaries.
- Widget refresh and menu-bar customizations.
- Failure states when the sensors plugin is missing.

Collect observations in your own words and screenshots **only for internal research**, not redistribution. Do not turn screenshots into copied assets or trace them pixel-for-pixel.

## Platform/framework documentation consulted

- [Tauri 2 system tray](https://v2.tauri.app/learn/system-tray/)
- [Tauri 2 permissions and capabilities](https://v2.tauri.app/learn/security/using-plugin-permissions/)
- [Rust sysinfo crate](https://docs.rs/sysinfo/latest/sysinfo/)
- [Windows performance counters guidance](https://learn.microsoft.com/en-us/windows/win32/perfctrs/about-performance-counters)
- [Linux hwmon sysfs spec](https://docs.kernel.org/hwmon/sysfs-interface.html)

Review all sources again before implementation; documentation can change.

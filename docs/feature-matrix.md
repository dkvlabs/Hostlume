# Feature matrix — single source of truth for scope/status

**Status convention:** `planned` → `in_progress` → `implemented` → `verified`; or `platform_limited` with platform/hardware explanation. **Initial status: all planned.** Never silently mark as verified. `Parity` means publicly described reference functionality, not copied internals/UI.

| FX ID | Feature | Reference? | Priority | Owning phase | Initial status | Key acceptance evidence |
|---|---|---|---|---|---|---|
| FX-001 | Device identity, uptime and hardware summary | Yes | P0 | 02 | planned | Native values + unavailable states |
| FX-002 | CPU overall and per-core history | Yes | P0 | 02 | planned | Two samples, rates and per-core tests |
| FX-003 | Memory/pressure/swap | Yes | P0 | 02 | planned | OS-specific categories and chart |
| FX-004 | Disks/mounts/capacity/external drives | Yes | P0 | 02 | planned | Add/remove devices; eject guarded |
| FX-005 | Configurable overview dashboard | Yes | P0 | 02 | planned | Real snapshot subscription; responsive |
| FX-006 | Network interfaces, addresses and speeds | Yes | P0 | 02 | planned | Per-adapter bytes/sec, resets/gaps |
| FX-007 | Applications/process history and quit | Yes | P0 | 03 | planned | PID+start-time validation, permission handling |
| FX-008 | Disk visualizer, large files, Move to Trash | Yes | P0 | 04 | planned | Cancel, safe symlinks, native Trash |
| FX-009 | Disk I/O read/write counters and rates | Yes | P1 | 04 | planned | Real adapter-based source + test |
| FX-010 | GPU utilization, memory, displays | Yes | P1 | 05 | planned | Native drivers; honest unsupported states |
| FX-011 | Advanced network history/map/IP info | Yes | P1 | 04 | planned | Interface switching; external lookup opt-in |
| FX-012 | Temperatures and fan sensors | Yes | P1 | 05 | planned | Sensor names, temperature units, provenance |
| FX-013 | Battery cycles/health/charge/time | Yes | P1 | 05 | planned | Battery-less machine and live battery tests |
| FX-014 | Bluetooth accessory battery | Yes | P1 | 05 | planned | Supported device enumeration/availability |
| FX-015 | Menu bar / system tray monitors | Yes | P1 | 06 | planned | Background close; controls, native smoke |
| FX-016 | Widgets / native mini-dashboard | Yes (macOS) | P1 | 06 | planned | macOS extension + fallback check per OS |
| FX-017 | Notifications and local opt-in actions | Yes | P1 | 07 | planned | Hysteresis, cooldown, safe trigger tests |
| FX-018 | Comparison, recording, CSV export | Yes | P1 | 08 | planned | Series alignment; CSV import/export |
| FX-019 | Mobile web monitor / read-only REST API | Yes | P1 | 08 | planned | Auth/TLS/loopback tests, no write routes |
| FX-020 | Preferences, themes, retention, i18n | Yes (subset) | P0 | 01/09 | in_progress | Persistent schema migrations, UI states |
| FX-021 | User-facing capability diagnostics | Enhancement | P1 | 05 | planned | Per-metric OS driver status + remediation |
| FX-022 | Multi-computer monitoring | Enhancement | P3 | 10 (optional) | planned | Multi-agent secure pairing specification |

## Cross-cutting work (not standalone feature rows)

- **Phase 00:** reference audit; OS support research; license and entitlement checks; risk register.
- **Phase 01:** app shell, Rust workspace, platform traits, telemetry envelope, settings + IPC, tests.
- **Phase 03:** SQLite history, aggregation and retention; process identities.
- **Phase 08:** endpoint schemas and remote threat model.
- **Phase 09:** regression, accessibility, distro/machine validation, installers, crash handling, documentation.

## Evidence format

For each status change append an evidence record in `docs/progress/PHASE-XX.md`:

```md
| FX ID | OS / hardware | Build commit | Validation type | Result | Limitation | Log / test path |
| FX-002 | macOS / Apple Silicon | <sha> | real collector + unit | PASS | none | <path> |
```

Use a real commit and real log reference only when available. A verification performed on Windows does **not** verify Linux.

# Platform capability plan — macOS / Windows / Linux

**Key distinction:** “Potentially implementable” ≠ “available on every machine.” Probe runtime capabilities per metric and surface a real reason when unavailable. **Do not infer guaranteed support solely from an API name.**

## Legend

- **P** = plausible via standard OS interfaces, subject to implementation/validation.
- **C** = conditional (driver, hardware model, OS permissions, optional helper or desktop environment).
- **F** = different implementation/fallback expected.
- **—** = not a universal promise.

| Feature | macOS | Windows | Linux | Validation requirements |
|---|---|---|---|---|
| System identity/uptime | P | P | P | OS version, suspend/uptime semantics |
| Overall/per-core CPU | P | P | P | Two-sample warm-up, core count changes |
| RAM/swap breakdown | P | P | P | OS-specific semantics documented |
| Process list and termination | P/C | P/C | P/C | Protected processes, UID/admin permissions |
| Filesystems/capacity | P | P | P | APFS, NTFS, ext4/Btrfs and network mounts |
| File treemap + safe Trash | P | P | P/C | OS-native Trash/Recycle Bin, mount policy |
| Network interfaces/rates | P | P | P | VPN, virtual adapters, sleep/resume |
| Disk device I/O | P/C | P/C | P/C | Privileges, physical vs virtual counters |
| GPU utilization/VRAM | C | C | C | Model/driver-specific capabilities |
| Temperatures/fan RPM | C | C | C | Real hardware sensors only |
| Battery health/cycles | C | C | C | Driver exposes cycles/design capacity? |
| Bluetooth battery | C | C | C | Accessory profile and OS exposure |
| Tray/menubar | P | P | C/F | GNOME/KDE/Wayland/X11; Windows overflow |
| Native desktop widgets | P/C | C/F | C/F | macOS WidgetKit extension; alternatives |
| Local web/API | P | P | P | Local-only then secure LAN opt-in |
| CSV, history, alerts | P | P | P | Process/OS action permission gates |

## macOS implementation investigation

| Subsystem | Candidate technology | Notes/gate |
|---|---|---|
| Host, CPU, memory, processes | portable `sysinfo` plus public OS interfaces/sysctl where needed | Validate per-core load vs Activity Monitor; sampling semantics |
| Power/battery | IOKit power sources / public power APIs | Battery health/cycles might be absent; distinguish Intel/Apple Silicon |
| Temperature and fan RPM | vetted SMC/IOKit interfaces **where legally and technically supportable** | Apple Silicon exposure varies; security and signing gate; never ship unsupported private paths casually |
| GPU and displays | supported graphics/system APIs, possible Metal statistics where officially exposed | Avoid claiming Apple Silicon GPU busy% accessible universally; separate adapter identity from utilization |
| Storage I/O | system I/O counters through supported public interfaces | Investigate entitlements, system protections and per-process limitations |
| Network | system configuration, interface counters and public networking APIs | External-IP lookup separate opt-in feature |
| Tray | Tauri tray/native `NSStatusItem` integration as needed | Text layouts depend on macOS/window manager |
| Widgets | WidgetKit Swift extension + App Group/shared snapshot mechanism | Separate native target, signing, extension sandbox validation |

**Testing:** Intel Mac and Apple Silicon Mac (at minimum two hardware generations if possible), charging/non-charging laptop, sensors missing and sensors available; main window hidden; menu-bar notch; permissions and notarized build. **Do not promise Mac App Store distribution until entitlement and helper restrictions are reviewed.**

## Windows implementation investigation

| Subsystem | Candidate technology | Notes/gate |
|---|---|---|
| CPU/RAM/processes | Win32 process/global memory/time APIs; PDH where appropriate | PDH is diagnostic-oriented; do not poll >1Hz without justification |
| Disk/net throughput | native performance counters / Windows network APIs | Correct identity for adapters, devices and process instances |
| GPU | DXGI / documented Windows GPU engine counters / vendor SDKs as permitted | Hardware, WDDM/driver variation; normalized load depends on engine |
| Sensors/fans | driver/vendor-exposed interfaces or reviewed library/optional helper | Many motherboard sensors inaccessible to standard accounts |
| Battery | Windows power APIs / WMI where appropriate | Design capacity and cycle count may be absent |
| Bluetooth | Windows Bluetooth device property exposure | Accessory battery support varies by device |
| Process termination | Win32 with permissions and process creation-time validation | Never attempt silent elevation or terminate protected OS tasks |
| Tray | Tauri tray + Windows notification area | OS may collapse icons into overflow |
| Widgets | research current widget-provider distribution/runtime capabilities | If non-viable, ship a compact floating monitor; don't pretend it is an OS widget |

**Testing:** Windows 10/11 supported by current Tauri, two GPU configurations (integrated/graphics card), Defender/protected process, laptop battery and desktop battery absence, per-user install, DPI 125/150/200% and signing. Microsoft's [PDH guidance](https://learn.microsoft.com/en-us/windows/win32/perfctrs/about-performance-counters) cautions against high-frequency PDH polling.

## Linux implementation investigation

| Subsystem | Candidate technology | Notes/gate |
|---|---|---|
| CPU/memory/processes | `/proc/stat`, `/proc/meminfo`, `/proc/<pid>` or `sysinfo` | Kernel permission options and container constraints |
| Sensors/fans | `/sys/class/hwmon/`, device-specific drivers | Numeric scaling (temp often millidegrees), read-only |
| Power/battery | UPower, `/sys/class/power_supply/` | Depends on device and desktop session |
| Disks and I/O | `/proc/diskstats`, sysfs, filesystem APIs | Device-mapper, loop and RAID aggregation avoid duplicates |
| Network | `/proc/net/dev`, netlink, network APIs | Handle interface renaming, VPN, namespaces |
| GPU | supported DRM sysfs/driver APIs, vendor-specific tools where appropriate | Intel/AMD/NVIDIA differ; telemetry is conditional |
| Bluetooth | BlueZ D-Bus device battery properties | Not every BLE device exposes charge |
| Tray | StatusNotifier/AppIndicator where desktop supports | Some GNOME installations need extension; Wayland limits floating window positioning |
| Widgets | desktop-specific extensions only if maintainable | Default to floating mini-dashboard fallback |

**Testing:** Ubuntu GNOME Wayland, KDE Plasma Wayland, X11 session where supported, separate system with hwmon sensors, no sensors, NVIDIA/AMD/Intel if available, deb/rpm/AppImage packaging; rootless sandbox/Flatpak concerns reviewed. The [kernel hwmon docs](https://docs.kernel.org/hwmon/sysfs-interface.html) explain exposed sensor naming and units.

## Capability reporting example

```json
{
  "metricId": "sensor.cpu.package.temperature",
  "availability": "unsupported",
  "source": "macos-platform",
  "reasonCode": "NO_PUBLIC_READABLE_SENSOR",
  "explanation": "This hardware/OS combination does not expose a supported CPU package sensor.",
  "remediation": null,
  "helperAvailable": false
}
```

This is a **schema illustration**, not a claim about any particular Mac. Translate messages via i18n keys and do not expose implementation internals in public remote responses.

## Spike priorities before claiming parity

1. Prove 3-second native CPU/memory/network metric collection per OS; compare results against OS monitor using identical time windows.
2. Prove disk I/O without elevated access; document cases requiring privileges.
3. On Apple Silicon and Windows hardware, inventory *actual* accessible GPU/temperature/fan metrics and licenses.
4. Test Linux tray on GNOME Wayland and KDE; decide universal fallback strategy.
5. Package/provision signed macOS WidgetKit extension; confirm native Windows widgets feasibility with official docs.
6. Record dependency licensing of any vendor-specific graphics or sensor SDK; do not assume bundling permission.

## Documented degradation pattern

When a capability is unavailable: keep navigation usable; show a clear reason; allow manual retry after driver/permission change; hide irrelevant controls; preserve last-known history with timestamps; avoid broken graphs/empty 0 values. For a module that has **no** usable data on a host, display a well-designed explanation rather than a blank feature page.

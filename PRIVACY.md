# Privacy — what Hostlume collects, keeps and never sends

*Applies to version 0.1.0 (release candidate, named **Hostlume** on 20.10.2026 — earlier placeholder builds migrate their data directory into the named one on first start). This document describes the shipped behavior; where a feature is optional it is **off by default** and says so.*

## The short version

- Everything is measured **on this computer** and stays on this computer.
- There is **no account, no cloud service, no analytics, no crash upload and no update check**. The app makes **no network connection** unless you turn on one of the two opt-in features below, and even then it never contacts the internet.
- You can **export**, **clear** and **reset** your data from Settings at any time.

## What the app reads

To show you system metrics the app reads, every few seconds (3 s by default, adjustable 1–60 s):

- CPU usage (total and per core), memory and swap usage, disk capacity and disk I/O counters, network interface byte counters, GPU/display inventory, temperature sensors, battery state, Bluetooth accessory battery levels where the operating system exposes them.
- The list of running processes (name, process id, start time, CPU and memory use) — **only while the Applications page is open**.
- Only when you ask for a storage scan: the sizes of files and folders **under the folder you selected**. No scan starts on its own.

Where the operating system or hardware does not expose a value, the app shows `unsupported`, `permission required`, `temporarily unavailable` or `error` with a reason. It never substitutes an invented number.

## What is stored, and where

All data lives in the app's own data directory and nowhere else:

| OS | Location |
|---|---|
| macOS | `~/Library/Application Support/com.dkvlabs.hostlume/` |
| Windows | `%APPDATA%\com.dkvlabs.hostlume\` |
| Linux | `~/.local/share/com.dkvlabs.hostlume/` |

Inside it:

- `settings.sqlite3` — your preferences, plus an anonymous random **host id** (a UUID generated on first run; it identifies this installation to itself and is not derived from any hardware serial).
- `history/history.sqlite3` — numeric metric samples with UTC timestamps, retention-trimmed (defaults: raw samples 24 h, per-minute rollups 30 days, hourly rollups 365 days; all adjustable, `0` = keep all), plus:
  - your alert rules and a log of alert events (which rule fired, when, the measured value),
  - an audit log of **process quit/force-quit actions** you confirmed (timestamp, process id, outcome — not the process's command line),
  - recording session labels you created.
- `exports/` — CSV files you exported. Nothing is written there unless you click Export.
- `crash.log` — if the app ever hits an internal error, one line per occurrence (time, thread, source location, error text). It is for your own diagnosis and is **never transmitted**.
- `remote-tls/` — if you switch the remote monitor to LAN mode: this machine's own HTTPS certificate and private key (owner-only file). Consoles that pair with this machine pin its fingerprint.
- `remote-credentials.json` — if you pair this console with other machines: the access tokens those machines gave you (owner-only file; never in settings, exports or the app window). Removing a host deletes its token.

Metric history, storage-scan results and process lists are never written outside this directory. Storage scans are kept **in memory only** for the session.

## What never leaves the computer

By default: nothing. Specifically, the app does not transmit metrics, process names, file paths, hostnames, IP or MAC addresses, serial numbers or user names anywhere, and it does not phone home.

## Optional features that involve a network — both off by default

### 1. Remote read-only monitor (Settings → Remote)

Lets you open a live dashboard in a browser **on this same computer**.

- Off by default: no network port is opened until you turn it on.
- When enabled it listens **only on `127.0.0.1` (this computer)**, or — if you explicitly switch to **LAN mode** — on your local network over **HTTPS with this machine's own certificate** (the app refuses LAN without a certificate). Other computers can only pair by typing a **single-use pairing code** shown on this screen, and each paired computer gets its own token you can **revoke individually**; the app stores only SHA-256 hashes.
- Access requires a random token that is shown to you **once**; the app stores only its SHA-256 hash. You can rotate or revoke it; revoking stops the server.
- **Paired computers (Hosts page):** this app can also act as the *console* — reading numeric metrics from machines you paired. Every remote value is labelled with the host it came from and its timestamp; stale values are never shown as live. There is **no remote control** in either direction: no quit, no delete, no settings, no commands. Pairing codes and access tokens are never written to settings, exports or logs.
- The dashboard is **read-only**: it exposes numeric metrics and a sanitized process table (name, pid, CPU, memory). It deliberately does **not** expose IP/MAC addresses, interface or hostnames on metric routes, process command lines or file paths, and it has **no** controls — no quit, no delete, no settings.
- Rate-limited and connection-capped. No cookies except the dashboard session cookie (HttpOnly, SameSite=Strict, expires in 12 h, in memory only).

### 2. External IP lookup (Network page)

Would ask a third-party service for this computer's public IP address. In this version the switch exists but **no network transport is implemented**: turning it on produces an explicit `EXTERNAL_LOOKUP_TRANSPORT_UNAVAILABLE` message and **no request is made**. If a future version implements it, the provider will be named in the UI and the feature will remain opt-in.

## Notifications and local actions

- Alert notifications use the operating system's notification center and contain only the text you configured (with the measured value, entity and threshold substituted).
- Optional "run a program when an alert fires" actions are **off by default**, require a master switch **and** per-rule approval, run a program path you typed yourself with no arguments and no shell, are time-limited, and every run or refusal is written to the local audit log. They cannot be triggered remotely.

## Your controls

- **Export** selected metrics or a recording as CSV (Compare page). The exported file contains timestamps, metric ids, entity ids, values and units — review it before sharing.
- **Clear history** (Settings → History) permanently deletes stored samples and rollups.
- **Reset settings** (Settings → Reset) restores defaults, stops the remote monitor, erases its token and turns off launch-at-login; history and alert rules are kept.
- Deleting the data directory removes everything the app has ever stored.

## Diagnostics you may choose to share

The in-app diagnostics view and `crash.log` are local files. If you send them to the developers for support, they may contain metric names, interface names and your settings; they do not contain tokens (only hashes are stored) or process command lines. Nothing is sent automatically.

## Changes

Any future feature that transmits data off-device will be opt-in, disclosed here with the destination named, and will never be enabled by an update.

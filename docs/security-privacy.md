# Security and privacy specification

## 1. Threat model

Assets: host telemetry, process list/path, private network details, disk paths, exported history, access tokens, stored rules, optional helper permissions. Attackers: unauthenticated LAN user, malware running with lower privileges, hostile webpage/remote UI, misconfigured firewall, untrusted filesystem path, malicious command input, compromised update artifact.

**Default boundary:** the app runs locally; it has no cloud dependency, no mandatory telemetry upload, no open network port, no privileged native helper and no active workflow execution. Remote viewing, external IP lookups, local commands and native helpers are separately opt-in.

## 2. Tauri/WebView and IPC security

- Explicit Tauri 2 capability grants per window and operation; never enable global shell/filesystem permissions to simplify implementation.
- Central Rust authorization/validation for every `invoke` action; JavaScript dialog confirmation is not a security boundary.
- Strict Content Security Policy, no arbitrary remote web content loaded into the privileged desktop WebView, no dynamic scripts from user-supplied URLs.
- Separate untrusted data (process names, path labels, device names, Bluetooth names) from markup; React escapes by default, but never feed untrusted strings into `dangerouslySetInnerHTML`.
- Treat IPC command parameters as untrusted; validate lengths, paths, owner, enum allowlist, time bounds and object identity.
- Sanitize logs and crash reports; disable verbose payload dumps in production.

## 3. Process termination actions

- Require deliberate user interaction in the local desktop UI, explain potential unsaved-work loss.
- Re-read PID **and creation/start time** immediately before termination to prevent PID reuse.
- Reject protected/system targets if insufficient permission; do not auto-elevate.
- Graceful quit first when feasible; force-quit needs distinct confirmation.
- No process action exposed through any HTTP route or remote dashboard.
- Audit action locally with timestamp, target pseudonym/ID and result (avoid storing full command line by default).

## 4. Filesystem scan and cleanup

- Require user-selected scan path/volume; no automatic whole-home scan on startup.
- Canonicalize roots, check symlink/reparse loops and avoid following external mount paths unexpectedly.
- Cancellation and bounded recursion; protect against deeply nested directories and abnormal filenames.
- **Only OS-native Trash/Recycle Bin**, with explicit confirmation and paths displayed. If Trash fails, fail closed; never fallback to permanent deletion.
- Avoid claiming that selected logical bytes will equal actual reclaimed storage.
- Sensitive paths shown locally only, excluded from remote default schema and redacted in diagnostic exports.

## 5. Alert workflows

- Alerts may notify by default; shell commands/scripts remain disabled until a user explicitly enables, defines and approves each workflow locally.
- Prefer named commands/argument arrays over interpolation into shell strings. If shell integration is implemented for a supported workflow, no metric or process/path text may be automatically interpolated into shell code.
- Prevent command execution from remote API; disallow hidden script downloads and privileged actions.
- Implement concurrency limits, rate limits, per-rule cooldown and audit history.
- Make unsafe/unavailable actions visibly disabled on restricted platforms.

## 6. Remote server — disabled until opt-in

Mode 1 **OFF** (default): no socket open.  
Mode 2 **Loopback**: `127.0.0.1`/`::1`, token auth strongly preferred, may support local browser.  
Mode 3 **LAN**: explicit opt-in, selected bind address, **TLS + strong token authentication**, clear firewall guidance, network risk warning, device-scoped secrets and revocation.

> **Phase 10 status:** LAN mode is implemented over pinned TLS (self-signed agent certificate, fingerprint pinned by consoles at pairing, ADR-0009). Pairing uses single-use short-lived codes bound to the certificate fingerprint; paired consoles hold per-device revocable tokens (hash-only on the agent). TLS cert trust/onboarding is the explicit fingerprint display at pairing — no untrusted cert is silently distributed. No discovery, no relay, no WAN forwarding; a real second-device LAN validation is still pending (docs/progress/PHASE-10.md §9).

Implementation:

- Hardcoded read-only route registry: `/api/v1/capabilities`, `/system`, `/snapshot`, `/cpu`, `/memory`, `/disks`, `/network`, `/gpu`, `/sensors`, `/battery`, `/processes` (with privacy minimization), `/metrics/history`, and authenticated read-only stream.
- `GET`/`HEAD` on whitelisted paths only; reject mutating methods with 405/404. No generic Tauri IPC-to-HTTP bridge.
- Auth tokens generated from OS cryptographic RNG with sufficient entropy; store in OS credential storage or secure equivalent; hash in server state where appropriate; allow rotation and revocation.
- No credentials in URL/query, log, browser analytics, crash dump or exports.
- Apply rate limits, bounded query limits, connection caps, slow-client disconnection, maximum response sizes, read timeouts.
- CORS allowlist rather than `*`; origin checks complement auth. TLS certificate trust/onboarding must be understandable; do not silently distribute untrusted certs.
- Strong session handling for browser dashboards, CSRF protection if cookie/session authentication is chosen, CSP headers, no external tracking scripts.
- Restrict the remote projection: show only non-sensitive metrics and minimally needed process fields; never return raw command-line args, credential paths, complete file lists or local secrets.
- LAN IP allowlist can be a **secondary** safeguard; never a substitute for authentication.
- Do not ship WAN port forwarding, UPnP, dynamic DNS or public hosted relay by default.

## 7. Privacy controls

- All core measurements are obtained on-device; history stored locally in the app's data directory.
- First-run privacy panel: monitoring offline, history retention, opt-in external IP lookup, local network server off, optional updates with network usage disclosed.
- Local user can clear history, export selected metrics/CSV and reset settings.
- Hardware serials, MAC addresses, usernames, process executable paths and private interface addresses are excluded from remote by default. Explicit user choice is needed if a future feature exposes them.
- External IP/country lookup must name the destination/provider and offer a local-only disable switch.
- Do not enable crash/analytics reporting without clear opt-in and privacy policy if added later.

## 8. Native helper and distribution

Privileged helpers (sensor access, other restricted device access) are **not** assumed necessary. Before one is added: prove technical necessity, use public supportable interfaces, separate signed binary, least privileges, authenticated local IPC, clear installation/uninstallation and documented attack surface. Review macOS notarization/entitlements, Windows driver signing or service security and Linux polkit/DBus before implementation. Do not ship unsigned kernel-level drivers for v1.

## 9. Mandatory security tests

| Case | Expected result |
|---|---|
| Fresh launch before remote opt-in | No listener on port |
| LAN flag enabled without TLS/auth | Refused by validation |
| Missing/invalid token | 401/403, no metric disclosure |
| Non-GET remote action | Rejected; process/Trash unaffected |
| Oversized history range/limit | Validation error + bounded compute |
| Cross-origin browser request | Enforced CORS and auth |
| XSS through process name/path | Rendered as inert text |
| PID changed before force-quit | Action refused |
| Filesystem symlink escape | Scan/Trash follow configured policy, not arbitrary path |
| Unsafe command interpolation | Denied; no local shell execution |
| Secret in diagnostic export | Redacted or excluded |
| Token rotation | Old token invalid immediately |

## 10. Security release gate

All tests above pass on supported targets; dependency review/SBOM generated; remote server externally scanned in LAN mode; manual test includes failed auth, TLS trust, cancellation and token revocation; no high-severity unresolved issue. Document known limitations and secure defaults in release notes.

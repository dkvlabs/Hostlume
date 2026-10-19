# Phase progress report template

Save as `docs/progress/PHASE-NN.md`. Fill with **actual** evidence; examples are placeholders and must not be represented as work already done.

```md
# Phase NN — <name>
Date: DD.MM.YYYY
Commit: <actual SHA>
Feature IDs: FX-...
Status: in progress | blocked | accepted

## Changes implemented
- <source paths and behaviors>

## Verification
| Environment | Command / scenario | Result | Log file |
|---|---|---|---|
| macOS/Windows/Linux | <command> | PASS/FAIL/NOT_RUN | <artifact> |

## Platform capability findings
| Metric | OS + hardware | Source | Availability | Notes |
|---|---|---|---|---|

## Security and data integrity
- <risk tested / remaining concern>

## Blockers / untested
- <explicit reason>

## Feature matrix edits
- FX-...: status -> evidence

## Gate review
- [ ] every required acceptance condition achieved
- [ ] documented exceptions approved
- [ ] next phase safe to start
```

# Architecture

## Core rule

Business logic must not depend on the web framework, scheduler or storage representation.

## Modules

- `openrouter`: catalog, benchmark and real completion calls only.
- `ranking`: pure filtering/ranking. No network or database access.
- `tester`: bounded-concurrency candidate preflight.
- `newapi`: New API HTTP contract and channel ownership verification.
- `sync`: orchestration and rollback of fixed managed channel mappings.
- `db`: persistence boundary.
- `scheduler`: invokes `sync`; contains no selection logic.
- `api`: HTTP adapter only.

## Removed from v2

- Windmill Full-code App bindings.
- Manager action switchboard.
- Ten-minute heartbeat.
- Generation IDs.
- Repeated staging-channel creation for every sync.
- Multi-generation backup channel retention.
- Large set of user-editable compatibility fields on the main screen.

## Preserved from v2

- Exact managed channel IDs.
- Alias conflict detection.
- Zero-price checks.
- Artificial Analysis ranking.
- Real model preflight.
- Three production routes.
- No mutation of foreign New API resources.
- Secret-at-rest protection.
- Fail closed when validation is incomplete.

## Connection configuration boundary (v3.0.2)

Connection identity is a separate persistence concern from model-selection settings:

- `/api/connections` owns New API base URL, administrator user ID and non-empty secret updates.
- Connection settings plus newly entered encrypted secrets are committed in one SQLite transaction.
- `/api/settings` preserves the persisted New API base URL and administrator user ID instead of accepting them from the general settings payload.
- Connection checks are persisted separately from configuration as `unknown`, `connected` or `failed` state.
- The React UI keeps editable drafts separate from server-persisted settings; background status refreshes never replace in-progress edits.

This separation prevents UI refresh behavior from becoming a data-loss path and keeps connection verification aligned with the exact configuration used by scheduled synchronization.


## Scheduler boundary (v3.0.3)

The scheduler remains an orchestration adapter only; it never contains model-selection or New API mutation logic. Both schedule modes ultimately call the same `sync::run` path used by manual synchronization.

- `interval`: schedules the next run after the configured interval.
- `daily`: resolves an `HH:MM` wall-clock time in an explicit IANA timezone and schedules the next occurrence.
- fixed-time mode does not depend on the container/host timezone and therefore does not drift when the container restarts.
- DST gaps are shifted forward to the first valid local minute; ambiguous local times use the earlier occurrence.
- saving settings notifies the scheduler immediately so the active timer is rebuilt.

Manual **立即同步** is deliberately not a shortcut around validation: it always calls the normal sync pipeline, which performs a fresh OpenRouter scan, benchmark ranking, real preflight, Top-3 comparison and safe New API update/rollback.


## Observable sync boundary (v3.0.5)

Manual sync is split into **start** and **progress** operations. `POST /api/sync/start` acquires the global sync lock, persists a `running` SyncRun, and spawns the workflow. The UI polls `GET /api/sync/{id}`.

`sync_run_logs` is append-only per run and records `level`, `stage`, `category`, `message`, and optional `detail`. The same entries power the live console and history drill-down. This keeps UI diagnostics independent of process stdout while preserving the existing single-container architecture.

Routing-pool inspection allows manual channels to share the public alias. Only orphaned explicit AOM identity stops mutation; manual and related channels remain read-only diagnostics.


## Hybrid New API routing ownership (v3.0.5)

The public alias is deliberately decoupled from resource ownership. Multiple New API channels may expose the same alias. AOM classifies them into:

- **Manual Pool**: serves the alias but is not locally registered as AOM-owned. Read-only.
- **Managed Pool**: exact Channel IDs stored in SQLite and verified against AOM name/tag/remark/base URL identity.
- **Orphan AOM**: claims AOM identity but is not locally registered. Fail-closed until manually investigated.
- **Related**: shares group/tag/prefix metadata but neither serves the alias nor claims enough AOM identity to be adopted. Diagnostic only.

This keeps New API routing concerns (alias, priority, weight) separate from AOM ownership concerns (exact ID + identity verification).


## New API adapter schema boundary (v3.0.6)

Business settings remain typed for AOM semantics (`auto_ban: bool`). The New API adapter owns wire-format compatibility and serializes `auto_ban` as integer `1`/`0`, matching the current New API `Channel` schema. This prevents New API-specific transport details from leaking into the application settings model.


## Quality-first Model Health Engine (v3.0.7)

The health engine runs the entire candidate set in repeated rounds. Defaults are three rounds and a 60-second inter-round delay. Attempts are persisted individually. After the final round, candidates below the minimum success rate are removed. R1 is always the strongest qualified model; R2/R3 then prioritize health among the remaining qualified candidates, with capability rank as the tie-breaker. This deliberately protects top capability while making the backup slots more reliable.

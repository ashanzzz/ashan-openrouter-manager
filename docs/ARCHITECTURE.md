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

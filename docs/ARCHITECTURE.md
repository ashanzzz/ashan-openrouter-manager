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

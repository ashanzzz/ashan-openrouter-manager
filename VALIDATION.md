# Validation status

## Completed in the generation environment

- Project structure and module boundaries reviewed.
- Frontend TypeScript/TSX syntax and project type flow checked with TypeScript 5.8.3 using temporary React type shims.
- JSON project files parsed.
- Rust source delimiter balance checked.
- New API request shapes were migrated from the previous working v2 implementation:
  - `Authorization: Bearer ...`
  - `New-Api-User`
  - `POST /api/channel/` with `{mode:"single", channel}`
  - `PUT /api/channel/` with `{id, ...patch}`
  - `POST /api/channel/:id/status`
  - `GET /api/channel/test/:id?model=...`
  - exact-ID batch cleanup only for channels created by a failed initialization.
- OpenRouter v3 API assumptions were checked against the current official documentation for `/api/v1/models` and `/api/v1/benchmarks`.

## Not executable in this generation environment

The current sandbox does not provide a Rust toolchain or Docker daemon. Its npm registry mirror also does not expose public React/Vite packages. Therefore these commands must be run on the target machine or CI before production use:

```bash
cargo fmt --all -- --check
cargo test
cd frontend && npm install && npm run build
cd ..
docker compose build
docker compose up -d
./scripts/smoke.sh
```

The first live New API sync must still be tested against the exact New API build in use, because New API is an external project whose channel contract can change between releases.

## v3.0.1 deployment simplification

- Removed the user-configurable container `PORT`; the internal listener is fixed at 8080.
- Compose exposes only host-side `WEBUI_PORT`.
- Unraid template installer exposes one port field only.
- Removed unused QEMU and optional Docker Hub login from GHCR CI.
- Changed workflow concurrency from commit SHA to Git ref so main/tag builds do not cancel each other.
- Preserved the frontend `tsconfig.node.json` `noEmit` fix from current GitHub main.


## v3.0.2 connection-state refactor

- Root cause fixed: saving secrets no longer triggers a settings reload that wipes an unsaved New API base URL.
- Added a dedicated connection persistence API and separated connection settings from general settings mutations.
- Connection settings plus newly entered encrypted secrets are written in one SQLite transaction to avoid partial saves.
- Added persistent connection-check state (`unknown`, `connected`, `failed`) stored in SQLite KV state.
- Successful connection tests return service-specific messages, latency and resource counts.
- Frontend now keeps saved settings and editable drafts as separate state, so background refreshes do not destroy user edits.
- Frontend actions use specific success/error messages instead of the previous generic `操作成功`.
- UI visibly differentiates configured-but-untested from verified connected state.
- TypeScript application structure was checked with TypeScript 5.8.3 using temporary React type shims; temporary shims are excluded from the release package.
- Rust toolchain is still unavailable in this sandbox, so GitHub Actions remains the authoritative Rust/Docker build verification for this release.

## v3.0.5 observability checks

- `sync_run_logs` is created with `CREATE TABLE IF NOT EXISTS`, so existing AppData upgrades in place.
- Manual sync starts asynchronously and returns a Run ID before long-running network work begins.
- `GET /api/sync/{id}` returns the persisted run and ordered logs.
- `GET /api/status` exposes `active_sync_run_id` so browser refresh can resume polling.
- New API 401/403 is classified as a permission problem; OpenRouter 401/403 is classified separately.
- Foreign channels that only share the business group are warnings; alias/mapping occupation or explicit orphaned AOM v3 identity remain blocking safety conflicts.
- No secret plaintext is included in sync-log messages or details.
- Frontend TypeScript structure was checked with the sandbox TypeScript 5.8.3 compiler and temporary React type shims.
- The sandbox still has no Rust toolchain or Docker daemon; GitHub Actions remains the final `cargo build` / Docker build authority.

## v3.0.5 hybrid-routing checks

- Public alias equality is no longer used as an ownership signal.
- Manual channels exposing `ashan-ai-model` are classified read-only and do not block initialization or synchronization.
- AOM mutation paths still require exact local Channel IDs plus live identity verification.
- Orphaned explicit AOM identity channels remain fail-closed.
- `/api/routing` returns only non-secret routing metadata (ID, name, status, priority, weight, group/tag, model list and alias mapping target).
- Overview renders Manual Pool and AOM Managed Pool separately and explains the current priority relationship.
- Existing SQLite schema remains compatible; no AppData reset is required.


## v3.0.6 New API schema compatibility checks

- Verified against the current New API source model: `Channel.AutoBan` is `*int`, not a JSON boolean.
- AOM retains a boolean setting internally and converts it to `1`/`0` only in the New API adapter.
- Both channel creation and managed-channel update payloads use the integer representation.
- Sync errors containing Go JSON schema decode failures are classified as `newapi_schema` and rendered as `New API API Schema`.
- Added a Rust unit test for `true -> 1` and `false -> 0` encoding.
- No SQLite schema change is introduced by v3.0.6.


## v3.0.7 multi-round model-health checks

- Existing settings JSON remains backward compatible through serde defaults: attempts=3, interval=60 seconds, minimum success rate=0.30.
- SQLite creates `model_health_checks` and `model_health_summary` with `CREATE TABLE IF NOT EXISTS`; existing AppData upgrades in place.
- Candidate testing runs by rounds across the whole candidate set, not three sequential waits per individual model.
- R1 keeps the best original capability rank; R2/R3 prioritize health among the remaining qualified candidates and use capability rank as a tie-breaker.
- Default 1/3 success = 33.3%, which passes the 30% threshold; 0/3 fails.
- Manual scan, manual sync and scheduled sync call the same health engine.
- The sandbox still has no Rust toolchain/Docker daemon; GitHub Actions remains authoritative for `cargo test` and image build.


## v3.0.8 health-gate-only Top3 selection

- Every candidate still receives at least 3 real OpenRouter checks with at least 60 seconds between rounds.
- Default qualification threshold remains 30%; 1/3 success (33.3%) qualifies.
- Health rate is used only as an admission gate.
- After qualification, health rate is not used for R1, R2 or R3 ordering.
- Qualified candidates preserve the original capability/benchmark rank; the first three become R1/R2/R3.
- Regression test covers a case where capability rank #2 has 33.3% health while lower-ranked models have 100%/66.7%; rank #2 must remain R2.
- No SQLite schema or AppData reset is required from v3.0.7.


## v3.0.9 Tokio Send/lifetime CI fix

- Root cause from GitHub Actions: Rust rejected the manual sync `tokio::spawn` future and the spawned scheduler future with `implementation of Send is not general enough`.
- Sync execution boundaries now own cloned state/logger/settings/database handles.
- Health-check round futures own model IDs and request state before `.await`; no candidate iterator borrow is carried through the request loop.
- Scheduler uses owned Notify futures and is polled concurrently with Axum from `main` rather than being passed to `tokio::spawn`.
- Frontend/health-selection behavior is unchanged from the finalized v3.0.8 policy.

## v3.0.10 routing-group / failover redesign

Validation focus:

- Added `routing_groups` with a backward-compatible default of `default`.
- Kept `managed_group` as the AOM ownership marker; production channel groups are the normalized union of ownership + routing groups.
- Existing registered channels are reconciled before the Top-3 no-change shortcut, so a v3.0.9 R1/R2/R3 set can be repaired without changing models or channel IDs.
- Routing reconciliation reuses the existing exact-ID ownership checks and managed-channel update path; foreign channels remain read-only.
- AOM does not add a second provider/model fallback loop and does not mutate New API's global retry count. Failover remains in New API, where the same requested alias can advance across the distinct channel priorities.
- Added regression tests for ownership-group preservation, default routing-group fallback, normalization and deduplication.
- Frontend TypeScript/TSX syntax transpilation passed for all frontend entry/source files in the delivery environment.
- `frontend/package.json` parses successfully and `git diff --check` passes.

Build-environment limitation for this delivery session:

- The artifact runtime does not include Rust/Cargo.
- The recovered v3.0.9 Git archive intentionally does not contain `frontend/node_modules`, and the runtime cannot fetch missing npm packages from the public registry.
- Therefore full `cargo test` and `npm build` must be performed by the repository CI after publication. The source tree itself was checked without claiming those unavailable local builds passed.

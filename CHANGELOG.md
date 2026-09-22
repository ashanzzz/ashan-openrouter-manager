# Changelog

## v3.0.12

- New and updated AOM-managed New API channels now expose both the stable public alias (`ashan-ai-model`) and their selected real OpenRouter model ID (for example, `nvidia/nemotron-3-super-120b-a12b:free`).
- Existing alias-only channels are upgraded in place during the next synchronization, even when the selected Top 3 has not changed.
- Kept `ashan-ai-model` mapped to each channel's selected real model, so existing alias-based failover behavior is unchanged.
- No SQLite schema or AppData reset is required when upgrading from v3.0.11.

## v3.0.11

- Added Agent Harness attribution headers (`HTTP-Referer`, `X-Title`, `User-Agent`) dynamically configurable via `AppSettings` with default presets for **NousResearch / Hermes Agent**.
- Attached Hermes Agent attribution headers to all OpenRouter API requests (model directory fetching, benchmark lookups, and multi-round candidate health preflight checks), enabling access to models restricted to agentic harnesses (e.g. `thinkingmachines/inkling:free`).
- Injected custom attribution `headers` JSON payload into New API channel creation (`create_channel`) and channel update (`update_model`) requests, ensuring end-user chat traffic routed through New API carries the Hermes Agent identity to OpenRouter.
- Added Settings UI panel for Agent Harness request header customization and a one-click preset button to populate Hermes Agent configuration.
- Added unit tests for New API channel header serialization and validation.
- No database reset is required when upgrading from v3.0.10.

## v3.0.10

- Split AOM's ownership group from its production request-routing groups.
- Added backward-compatible `routing_groups`, defaulting to `default`.
- New and updated R1/R2/R3 channels now use the normalized union of the AOM ownership group and configured routing groups.
- Added an in-place routing-group reconciliation pass before the Top-3 no-change shortcut, so existing v3.0.9 channels are repaired even when the selected models are unchanged.
- Added an editable WebUI field for actual routing groups while keeping ownership identity fields locked after channel initialization.
- Kept failover inside New API rather than introducing a duplicate AOM retry layer. The managed channels already share `ashan-ai-model` and use priorities 10003/10002/10001, so New API's native failed-channel retry can walk R1 -> R2 -> R3 once the request group can see them.
- Added regression tests for ownership + routing group normalization and deduplication.
- No SQLite schema or AppData reset is required when upgrading from v3.0.9.

## v3.0.9

- Fixed the Rust CI failure introduced by the multi-round health engine: background sync execution now owns `AppState`, `SyncLogger`, settings and database handles across async boundaries instead of exposing borrowed lifetimes to `tokio::spawn`.
- Changed model health checking to take an owned database handle and materialize owned per-round request futures before awaiting them.
- Changed scheduler wakeups to `Notify::notified_owned()` and run the scheduler alongside Axum on the main Tokio task instead of spawning the scheduler future.
- Preserved the finalized selection policy: health is admission-only; every model at or above the configured threshold remains ranked strictly by benchmark/capability rank for R1/R2/R3.
- No SQLite schema change and no AppData reset are required.

## v3.0.8

- Changed model health from a partial ranking signal to a pure admission gate.
- R1, R2 and R3 now use exactly the same rule: a model that meets the configured health threshold is qualified, and qualified models are ordered strictly by their original capability/benchmark rank.
- Removed health-priority sorting from R2/R3; 33.3%, 66.7% and 100% health are equivalent for ranking once qualified.
- Kept the existing 3-round minimum, 60-second minimum interval, 30% default threshold, SQLite health history, last-check timestamps and real-time health logs.
- Updated UI copy so health is shown as diagnostic/qualification data rather than a backup ranking signal.
- Added regression coverage ensuring a higher-capability 33.3%-healthy model is not displaced by a lower-capability 100%-healthy model.
- No AppData/database reset is required when upgrading from v3.0.7.

## v3.0.7

- Replaced one-shot model preflight with a persistent multi-round Model Health Engine.
- Default health policy: 3 attempts per candidate, 60 seconds between rounds, minimum success rate 30%.
- R1 is always the strongest qualified candidate, even at 1/3 health; R2/R3 prioritize healthier remaining qualified candidates and use capability rank as the tie-breaker.
- Manual scan, manual sync and scheduled sync now share the exact same multi-round health engine.
- Added SQLite `model_health_checks` and `model_health_summary` tables with per-attempt timestamps, latency, success/failure and last error.
- Models UI now shows health success rate, recent attempt results, last checked time and R1/R2/R3 roles.
- Added configurable health attempts, interval and minimum success rate; attempts are validated to be at least 3.
- Sync logs now show each health round, the wait between rounds, every model attempt and the final qualification decision.
- Existing v3.0.6 AppData upgrades in place; no reset is required.

## v3.0.6

- Fixed New API channel creation against the current `Channel.auto_ban` schema: AOM now serializes the internal boolean as integer `1`/`0` at the New API adapter boundary.
- Applied the same `auto_ban` integer conversion to managed-channel update requests so forced/changed syncs keep the New API setting consistent.
- Added explicit `New API API Schema` sync-log classification for Go JSON decode errors such as `cannot unmarshal ... into Go struct field`.
- Added a unit test ensuring `true -> 1` and `false -> 0` encoding.
- Preserved v3.0.5 hybrid routing: manual `ashan-ai-model` channels remain read-only and may coexist with the three AOM-managed Top3 channels.
- No AppData or database reset is required when upgrading from v3.0.5.

## v3.0.5

- Added first-class hybrid routing: user-managed New API channels and AOM-managed OpenRouter Top3 channels may expose the same public alias at the same time.
- Removed the incorrect assumption that `ashan-ai-model` implies AOM ownership. Manual channels using the alias are classified as `manual` and remain read-only.
- AOM ownership is now based on the exact IDs stored in SQLite plus AOM identity verification; only those exact channels can be updated, enabled, disabled, tested as managed slots, or rolled back.
- Hard safety blocking is now reserved for orphaned AOM identity channels (explicit OWNER_ID, exact AOM slot name, or AOM tag + prefix) that are not registered locally.
- Added `GET /api/routing` to expose a safe New API routing-pool diagnostic view without returning channel secrets.
- Added a routing-pool dashboard showing manual channels, AOM managed channels, enabled counts, priority/weight information, mapping targets, and the current highest-priority relationship.
- Sync logs now record each manual alias channel as preserved/read-only instead of reporting it as an alias conflict.
- Shared group/tag/prefix channels that do not claim AOM ownership are diagnostic-only and never mutated.
- No database reset is required when upgrading from v3.0.4.

## v3.0.4

- Added persistent per-run synchronization logs stored in SQLite (`sync_run_logs`).
- Changed manual sync UI to start a background run and poll live progress instead of blocking one HTTP request.
- Added an inline real-time log console below **立即同步**, with stage, severity, category, timestamp, message and diagnostic detail.
- Added history log drill-down so completed/failed runs can be inspected later.
- Added explicit failure categories for configuration, OpenRouter permission/API, New API permission/API, network/internal errors and safety conflicts.
- New API HTTP 401/403 errors are reported as administrator permission/token/user-ID failures.
- OpenRouter HTTP 401/403 errors are reported as OpenRouter permission failures.
- Added detailed logs for model catalog fetch, benchmark fetch, ranking, each preflight result, New API connection, identity verification, channel creation/update/test, E2E testing and rollback.
- Existing databases upgrade in place; no AppData reset is required.

## v3.0.3

- Added two automatic synchronization modes: fixed interval and daily fixed local time.
- Added IANA timezone-aware scheduling with `Asia/Shanghai` as the default fixed-time timezone.
- Added persistent scheduler fields with backward-compatible defaults.
- Added next-run display for daily schedules and preserved one-container/one-port deployment.
- Added an immediate-sync action in the automation settings card.

## v3.0.2

- Fixed New API base URL disappearing after saving credentials.
- Separated connection persistence from general model/automation settings.
- Connection settings and newly entered encrypted secrets are committed in one SQLite transaction.
- Added `/api/connections` to save New API endpoint, administrator ID and non-empty encrypted secrets as one connection action.
- Prevented runtime refreshes from overwriting unsaved form drafts.
- Added persistent OpenRouter/New API connection-test state in SQLite.
- Added explicit `未配置`, `已配置 · 未测试`, green `已连接`, and red `连接失败` UI states.
- Connection success now reports the real result, e.g. model/channel count and latency, instead of generic `操作成功`.
- Added loading spinners, action-specific success/error notices, dirty-state indicators and disabled actions when required configuration is missing.
- General settings saves can no longer overwrite New API base URL or administrator ID.
- `configured` now requires OpenRouter key, New API token, New API base URL and administrator user ID.
- Locked protected identity fields in the UI once managed channels exist, matching backend safety rules.

## v3.0.1

- Simplified deployment to one user-facing WebUI port.
- Fixed the container HTTP listener to internal port 8080.
- Removed the `PORT` environment variable from Compose, examples and Unraid configuration.
- Added an Unraid template installer using `/mnt/cache/appdata/ashan-openrouter-manager` by default.
- Unraid now exposes only `WebUI Port`, `App Data`, `APP_MASTER_KEY` and optional `RUST_LOG`.
- Preserved the current GitHub `tsconfig.node.json` `noEmit` build fix.
- Changed GitHub Actions concurrency to use `github.ref` instead of `github.sha`.
- Removed unused QEMU and optional Docker Hub login; GHCR is the single release registry.
- Corrected GHCR image tagging for main, release tags, semantic versions and commit SHA.

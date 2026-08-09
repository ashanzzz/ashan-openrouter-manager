# Changelog

## v3.0.4

- Added persistent per-run synchronization logs stored in SQLite (`sync_run_logs`).
- Changed manual sync UI to start a background run and poll live progress instead of blocking one HTTP request.
- Added an inline real-time log console below **立即同步**, with stage, severity, category, timestamp, message and diagnostic detail.
- Added history log drill-down so completed/failed runs can be inspected later.
- Added explicit failure categories for configuration, OpenRouter permission/API, New API permission/API, network/internal errors and safety conflicts.
- New API HTTP 401/403 errors are now reported as administrator permission/token/user-ID failures.
- OpenRouter HTTP 401/403 errors are now reported as OpenRouter permission failures.
- Refined foreign-channel safety detection: sharing the managed group alone is a warning, not a hard conflict. Alias occupation, alias mapping occupation, and orphaned explicit AOM v3 channels remain hard conflicts.
- Foreign-channel diagnostics now record channel ID, name, reason and whether the finding blocks synchronization.
- Added detailed logs for model catalog fetch, benchmark fetch, ranking, each preflight result, New API connection, identity verification, channel creation/update/test, E2E testing and rollback.
- Existing v3.0.3 databases upgrade in place; no AppData reset is required.

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

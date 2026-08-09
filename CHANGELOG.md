# Changelog

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

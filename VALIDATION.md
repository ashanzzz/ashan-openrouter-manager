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

## v3.0.4 observability checks

- `sync_run_logs` is created with `CREATE TABLE IF NOT EXISTS`, so existing AppData upgrades in place.
- Manual sync starts asynchronously and returns a Run ID before long-running network work begins.
- `GET /api/sync/{id}` returns the persisted run and ordered logs.
- `GET /api/status` exposes `active_sync_run_id` so browser refresh can resume polling.
- New API 401/403 is classified as a permission problem; OpenRouter 401/403 is classified separately.
- Foreign channels that only share the business group are warnings; alias/mapping occupation or explicit orphaned AOM v3 identity remain blocking safety conflicts.
- No secret plaintext is included in sync-log messages or details.
- Frontend TypeScript structure was checked with the sandbox TypeScript 5.8.3 compiler and temporary React type shims.
- The sandbox still has no Rust toolchain or Docker daemon; GitHub Actions remains the final `cargo build` / Docker build authority.

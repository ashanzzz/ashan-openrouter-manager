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

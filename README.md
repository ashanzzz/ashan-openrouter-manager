# Ashan OpenRouter Manager v3

A clean-room refactor of the original Windmill-based OpenRouter free-model manager.

## What it does

1. Reads the current OpenRouter model catalog.
2. Keeps only zero-price text models that meet the configured context limit.
3. Joins Artificial Analysis intelligence/coding/agentic benchmark data.
4. Ranks candidates and performs real OpenRouter chat-completion preflight tests.
5. Requires three usable models before changing production state.
6. Maintains exactly three registered New API channels that all expose `ashan-ai-model`.
7. Updates only those exact channel IDs. Unknown channels are read-only and conflicts stop the sync.
8. Runs automatically on an internal scheduler, with manual scan/sync controls in the web UI.

## Architecture

```text
React SPA
   |
REST API
   |
Rust + Axum
   |-- OpenRouter client
   |-- Ranking engine
   |-- Model tester
   |-- Sync service
   |-- New API client
   |-- Scheduler
   `-- SQLite
```

The React build is served by the Rust process. One container runs the frontend, API, scheduler and SQLite-backed state.

## Quick start

```bash
cp .env.example .env
# edit APP_MASTER_KEY and keep it stable

docker compose up -d --build
```

Open `http://YOUR_SERVER_IP:8080`.

Then:

1. Configure the OpenRouter API key.
2. Configure New API base URL, administrator token and administrator user ID.
3. Test both connections.
4. Save settings.
5. Run **Scan only** and inspect the top three.
6. Run **Sync now**. On first sync, the service creates exactly three fixed managed channels.
7. Enable automatic sync after the first successful live sync.

## New API managed resources

Default identity:

```text
alias:          ashan-ai-model
group:          wm-ashan-openrouter-free
tag:            ashan-openrouter-manager-v3
channel prefix: [AOM3]
channels:       [AOM3] R1, [AOM3] R2, [AOM3] R3
priorities:     10003, 10002, 10001
```

The manager stores the exact three channel IDs in SQLite and verifies live ownership before every mutation. It does not delete foreign channels or change New API global retries, tokens, model ratios or group ratios.

## Data

Persistent state is under `/data` in the container. The default Compose file maps it to `./data`.

Secrets are encrypted before SQLite storage. `APP_MASTER_KEY` is used to derive the encryption key. **Do not change it after saving secrets**, or the existing encrypted secrets can no longer be decrypted.

## Environment

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `APP_MASTER_KEY` | yes | none | encryption root key |
| `PORT` | no | `8080` | HTTP port |
| `DATA_DIR` | no | `/data` | SQLite location |
| `WEB_DIR` | no | `/app/web` | compiled React assets |
| `RUST_LOG` | no | `info` | tracing filter |

## Safety model

The synchronization rule is fail-closed:

- fewer than three usable models: keep current channels unchanged;
- alias/ownership conflict: stop without mutation;
- New API update/test failure: attempt to restore the previous mappings;
- unchanged top three: no channel writes;
- only exact IDs saved in `managed_channels` are mutable after initialization.

## Development

Frontend:

```bash
cd frontend
npm install
npm run dev
```

Backend:

```bash
cargo run
```

The frontend dev server proxies `/api` to `http://127.0.0.1:8080`.

## Docker & CI/CD Auto-Build

This repository uses GitHub Actions for automated Docker image building and pushing to **GitHub Container Registry (GHCR)**.

### Running with GHCR Image

```bash
docker run -d \
  --name ashan-openrouter-manager \
  -p 8080:8080 \
  -e APP_MASTER_KEY="your-stable-encryption-key" \
  -v ./data:/data \
  ghcr.io/<your-github-username>/ashan-openrouter-manager:latest
```

## Version Management & Release Workflow

We use **Semantic Versioning** (`vX.Y.Z`).

To cut a new version release:

1. Update `version` in `Cargo.toml` (e.g., `version = "3.0.1"`).
2. Commit changes: `git commit -am "bump version to v3.0.1"`
3. Tag and push to trigger automated build & release:
   ```bash
   git tag -a v3.0.1 -m "Release v3.0.1"
   git push origin v3.0.1
   ```
   *(Or run `make release-tag VERSION=v3.0.1`)*

GitHub Actions will automatically:
- Build the multi-stage Docker image with layer caching.
- Tag and publish images to GHCR: `:latest`, `:3.0.1`, `:3.0`, `:3`.
- Create a new **GitHub Release** with auto-generated release notes.

## Project status

This repository is the v3 refactor baseline. The architecture intentionally removes Windmill heartbeat/runnables, generation-based blue-green channels and the monolithic manager action API while preserving the core model-selection and exact-channel safety rules.


# Ashan OpenRouter Manager v3

A clean-room refactor of the original Windmill-based OpenRouter free-model manager.

## What it does

1. Reads the current OpenRouter model catalog.
2. Keeps only zero-price text models that meet the configured context limit.
3. Joins Artificial Analysis intelligence/coding/agentic benchmark data.
4. Ranks candidates by capability, then performs multi-round OpenRouter health checks (default: 3 attempts, 60-second interval).
5. Health never demotes the strongest qualified model: default threshold is 30%, so 1/3 success remains eligible for R1; R2/R3 prioritize healthier qualified fallbacks.
6. Requires three qualified models before changing production state.
7. Maintains exactly three registered New API channels that all expose `ashan-ai-model`.
8. Updates only those exact channel IDs. Manual New API channels may expose the same alias and remain strictly read-only.
9. Runs automatically on an internal scheduler, with manual scan/sync controls in the web UI.


### Observable synchronization

Manual synchronization is a background run with a persistent Run ID. The web UI opens a live log console directly below **立即同步** and polls the backend for progress. Every log entry is also stored in SQLite so failures can be reviewed later from **历史**.

The log pipeline distinguishes:

- configuration validation;
- OpenRouter model catalog and benchmark retrieval;
- model filtering/ranking plus every round and attempt of the multi-round health check;
- New API connection and administrator permission failures (401/403);
- manual-channel coexistence, route priority diagnostics, and true AOM ownership conflicts;
- exact managed-channel identity verification;
- channel creation/update/testing;
- E2E testing and rollback.

No API key or administrator token plaintext is written to synchronization logs.

## Architecture

```text
Browser
   |
Host WebUI Port (configurable, default 8080)
   |
Container :8080 (fixed)
   |
Rust + Axum
   |-- serves React SPA
   |-- REST API
   |-- OpenRouter client
   |-- Ranking engine
   |-- Model tester
   |-- Sync service
   |-- New API client
   |-- Scheduler
   `-- SQLite
```

The React build and REST API are served by the same Rust process. **There is no separate frontend port and backend port.** The container always listens on internal port `8080`; only the host-side WebUI port is configurable.

## Quick start

```bash
cp .env.example .env
# edit APP_MASTER_KEY and keep it stable

docker compose up -d --build
```

Open `http://YOUR_SERVER_IP:8080`. If `WEBUI_PORT` is changed, use that host port instead.

Then:

1. Configure the OpenRouter API key.
2. Configure New API base URL, administrator token and administrator user ID.
3. Test both connections.
4. Click **保存连接配置**. Connection fields and secrets are saved together.
5. Test OpenRouter and New API separately. Successful tests display a persistent green **已连接** state with latency and result details.
6. Run **立即检查**. By default each candidate is tested three times, with 60 seconds between rounds; the Models page shows success rate and last check time.
7. Run **立即同步**. Synchronization always performs the same health check before selecting R1/R2/R3. On first sync, the service creates exactly three fixed managed channels.
8. Enable automatic sync after the first successful live sync.

## Unraid

The Unraid template intentionally exposes only **one port field**: `WebUI Port`. The container-side target remains fixed at `8080`.

Default persistent path:

```text
/mnt/cache/appdata/ashan-openrouter-manager
```

Install the template from an Unraid terminal after copying/cloning this project:

```bash
bash scripts/install-unraid-template.sh
```

To use another host port without changing the container port:

```bash
HOST_PORT=18080 bash scripts/install-unraid-template.sh
```

## Quality-first model health policy

AOM v3.0.7 is designed for a resilient three-channel pool rather than for choosing only the most stable model. The selection policy is:

```text
Capability ranking (Intelligence -> Coding -> Agentic)
        |
        v
Multi-round health gate (default 3 checks, 60 s apart)
        |
        +-- success rate < 30% -> reject this run
        `-- success rate >= 30% -> keep original capability rank
        |
        v
R1 = strongest qualified model
R2 = healthiest remaining qualified fallback (capability rank breaks ties)
R3 = next healthiest remaining qualified fallback
```

Health success rate does **not** demote R1. A 1/3 model can remain R1 if it is the strongest candidate. R2/R3 deliberately prefer healthier remaining qualified models, while manual channels provide additional fallback capacity. Every manual scan, manual synchronization and scheduled synchronization uses the same health engine.

Model health attempts are stored in SQLite (`model_health_checks`) and the latest summary is stored in `model_health_summary`. The Models page exposes the latest success ratio, per-attempt result, latency and last checked time. Existing databases upgrade in place.

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

### Hybrid routing with manual channels

`ashan-ai-model` is a public routing alias, **not an ownership marker**. You may keep any number of manually maintained New API channels that also expose `ashan-ai-model`; AOM will list them for diagnostics but never modify, delete, disable, reprioritize, reweight, or adopt them.

AOM owns only the three exact Channel IDs stored in SQLite and verifies their AOM identity before every mutation. The new `GET /api/routing` endpoint and Overview routing card show both pools side-by-side:

```text
ashan-ai-model
  |-- Manual Pool       (user-owned, read-only to AOM)
  `-- AOM Managed Pool  ([AOM3] R1/R2/R3, exact-ID managed)
```

New API selects channels using its own priority/weight rules. AOM displays the current highest-priority relationship but never changes manual channel priority or weight.

A hard block is used only when a channel claims explicit AOM identity but its ID is absent from the local managed-channel registry, because silently adopting or overwriting such an orphan would be unsafe.


### New API schema compatibility

AOM keeps `auto_ban` as a boolean business setting internally, but the current New API `Channel` schema represents `auto_ban` as an integer (`1` enabled, `0` disabled). The New API adapter converts the value at the API boundary for both channel creation and channel updates. Schema decode errors such as `cannot unmarshal ... into Go struct field` are classified separately in synchronization logs as **New API API Schema** errors.


## Synchronization API

The legacy blocking `POST /api/sync` endpoint remains available for compatibility. The v3.0.7 UI uses:

```text
POST /api/sync/start      -> returns run_id immediately
GET  /api/sync/{run_id}   -> returns run status + persistent stage logs
```

A running task is also exposed as `active_sync_run_id` in `GET /api/status`, allowing the UI to reconnect to an in-progress sync after a page refresh.

## Data and secrets

Persistent state is under `/data` in the container. The default Compose file maps it to `./data`; the Unraid template maps it to `/mnt/cache/appdata/ashan-openrouter-manager`.

Secrets are encrypted before SQLite storage. `APP_MASTER_KEY` is used to derive the encryption key. **Do not change it after saving secrets**, or existing encrypted secrets can no longer be decrypted.

## User-facing environment

| Variable | Required | Default | Purpose |
|---|---|---|---|
| `APP_MASTER_KEY` | yes | none | encryption root key |
| `WEBUI_PORT` | Compose only | `8080` | host-side published port; container remains `8080` |
| `RUST_LOG` | no | `info` | tracing filter |

`PORT` is intentionally **not** a user setting. `DATA_DIR=/data` and `WEB_DIR=/app/web` are internal container defaults and are not exposed in the Unraid template.


## Connection settings UX

Connection settings are intentionally separated from model-selection settings:

- `New API 地址` and administrator ID are saved through the dedicated connection endpoint.
- Non-empty OpenRouter/New API secrets are encrypted and saved in the same connection action.
- Runtime refreshes never overwrite unsaved form drafts.
- Connection tests always use the saved runtime configuration; if a credential or endpoint has unsaved changes, the UI asks the user to save first.
- Successful tests persist a green `已连接` state in SQLite with timestamp, latency and result details.
- Failed tests persist a red `连接失败` state and surface the concrete backend error.
- General model/automation settings cannot accidentally erase the saved New API address or administrator ID.

This fixes the previous state-flow bug where saving only secrets triggered a global refresh that reloaded an empty persisted `newapi_base_url` and cleared the user's input.

## Safety model

The synchronization rule is fail-closed:

- fewer than three usable models: keep current channels unchanged;
- manual channels sharing the alias are allowed and remain read-only;
- orphaned/unregistered AOM identity: stop without mutation;
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

The frontend development server proxies `/api` to `http://127.0.0.1:8080`. Production still uses one Rust HTTP listener.

## Docker & CI/CD Auto-Build

GitHub Actions builds and publishes the image to GitHub Container Registry (GHCR).

### Running with GHCR image

```bash
docker run -d \
  --name ashan-openrouter-manager \
  -p 8080:8080 \
  -e APP_MASTER_KEY="your-stable-encryption-key" \
  -v ./data:/data \
  ghcr.io/ashanzzz/ashan-openrouter-manager:latest
```

To publish on host port `18080`, change only the left side: `-p 18080:8080`.

## Version Management & Release Workflow

We use Semantic Versioning (`vX.Y.Z`). This package is prepared as **v3.0.7**.

To release:

```bash
git add .
git commit -m "release: v3.0.7 add quality-first multi-round model health"
git tag -a v3.0.7 -m "Release v3.0.7"
git push origin main --tags
```

The workflow publishes `latest`, `main` on main builds, version tags on releases, and a short SHA tag. Main and tag runs use separate concurrency groups so tagging a commit does not cancel the main-branch build for the same SHA.

## Project status

This repository is the v3 refactor baseline. It removes Windmill heartbeat/runnables, generation-based blue-green channels and the monolithic manager action API while preserving the core model-selection and exact-channel safety rules.

# Synchronization logging

v3.0.6 records every manual and scheduled synchronization as a `SyncRun` plus ordered entries in `sync_run_logs`.

## Stages

- `configuration` — saved settings and required secret presence
- `openrouter_models` — OpenRouter model catalog retrieval
- `openrouter_benchmarks` — Artificial Analysis benchmark retrieval
- `ranking` — free-model filtering and ranking
- `preflight` — real candidate chat-completion tests
- `newapi_connection` — New API endpoint and administrator read permission
- `newapi_routing` — manual/AOM routing-pool inspection and ownership safety checks
- `newapi_identity` — exact managed-channel ownership verification
- `newapi_create` — first-time creation
- `newapi_update` — production mapping update
- `newapi_test` — New API channel test endpoint
- `e2e` — optional end-to-end alias request
- `rollback` — restoration after a failed update
- `complete` — final result

## Diagnostic categories

- `configuration` — missing/invalid saved configuration
- `openrouter_permission` — OpenRouter HTTP 401/403
- `openrouter_api` — other OpenRouter API/model-test failures
- `newapi_permission` — New API HTTP 401/403; check administrator Token and `New-Api-User`
- `newapi_api` — other New API request failures
- `network_or_internal` — timeout, connection or internal runtime/database failure
- `safety_conflict` — a hard channel identity/alias safety conflict
- `legacy_channel` — non-blocking old/shared-group channel warning
- `rollback_failure` — a rollback request itself failed

## Foreign-channel rule

Sharing only `managed_group` is not proof of ownership and therefore does not block synchronization. A foreign channel blocks synchronization if it already exposes/mappings the configured alias, or if it carries explicit AOM v3 ownership identity but is not registered in local SQLite.

## Secret handling

Synchronization logs never intentionally contain OpenRouter API keys, New API administrator tokens, or New API test tokens. Connection URLs, channel IDs, model IDs, status codes and API error summaries may be recorded for diagnosis.


## API schema compatibility diagnostics

New API Go JSON decode failures such as `cannot unmarshal bool into Go struct field ... of type int` are categorized as `newapi_schema` and displayed as **New API API Schema**. These indicate an API payload/schema mismatch, not an administrator permission problem.

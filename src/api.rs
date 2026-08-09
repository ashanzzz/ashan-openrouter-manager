use std::time::Instant;

use axum::{
    extract::State,
    routing::{get, post, put},
    Json, Router,
};
use chrono::Utc;
use serde_json::{json, Value};

use crate::{
    error::AppError,
    models::{
        AppSettings, ConnectionCheck, ConnectionSaveResponse, ConnectionTestResult,
        ConnectionUpdate, SecretStatus, SecretUpdate, StatusResponse, SyncRequest,
    },
    newapi::NewApiClient,
    openrouter::OpenRouterClient,
    AppState,
};

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/status", get(status))
        .route("/api/models", get(models))
        .route("/api/history", get(history))
        .route("/api/settings", get(get_settings).put(save_settings))
        .route("/api/connections", put(save_connections))
        .route("/api/secrets", put(save_secrets))
        .route("/api/scan", post(scan))
        .route("/api/sync", post(sync_now))
        .route("/api/test/openrouter", post(test_openrouter))
        .route("/api/test/newapi", post(test_newapi))
        .with_state(state)
}

async fn secret_status(state: &AppState) -> Result<SecretStatus, AppError> {
    Ok(SecretStatus {
        openrouter_api_key: state.db.has_secret("openrouter_api_key").await?,
        newapi_admin_token: state.db.has_secret("newapi_admin_token").await?,
        newapi_test_token: state.db.has_secret("newapi_test_token").await?,
    })
}

async fn status(State(state): State<AppState>) -> Result<Json<StatusResponse>, AppError> {
    let settings = state.db.get_settings().await?;
    let secrets = secret_status(&state).await?;
    let current = state.db.list_managed_channels().await?;
    let last_scan = state.db.get_last_scan().await?;
    let last_run = state.db.last_run().await?;
    let scheduler = state.scheduler_status.read().await.clone();
    let connections = state.db.get_connection_checks().await?;
    let configured = secrets.openrouter_api_key
        && secrets.newapi_admin_token
        && !settings.newapi_base_url.trim().is_empty()
        && !settings.newapi_admin_user_id.trim().is_empty();

    Ok(Json(StatusResponse {
        healthy: true,
        configured,
        current_models: current,
        last_scan,
        last_run,
        scheduler,
        secrets,
        connections,
    }))
}

async fn models(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({"scan": state.db.get_last_scan().await?})))
}

async fn history(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({"runs": state.db.list_runs(100).await?})))
}

async fn get_settings(State(state): State<AppState>) -> Result<Json<AppSettings>, AppError> {
    Ok(Json(state.db.get_settings().await?))
}

// General settings deliberately cannot mutate connection identity. Connection fields are
// saved through /api/connections so UI refreshes cannot accidentally erase them.
async fn save_settings(
    State(state): State<AppState>,
    Json(mut settings): Json<AppSettings>,
) -> Result<Json<Value>, AppError> {
    let old = state.db.get_settings().await?;
    settings.newapi_base_url = old.newapi_base_url;
    settings.newapi_admin_user_id = old.newapi_admin_user_id;

    if !state.db.list_managed_channels().await?.is_empty() {
        let protected_changed = old.openrouter_upstream_base != settings.openrouter_upstream_base
            || old.alias_model != settings.alias_model
            || old.managed_group != settings.managed_group
            || old.managed_tag != settings.managed_tag
            || old.channel_name_prefix != settings.channel_name_prefix
            || old.channel_type != settings.channel_type
            || old.enabled_status != settings.enabled_status
            || old.disabled_status != settings.disabled_status;
        if protected_changed {
            return Err(AppError::conflict(
                "受管渠道已存在，受保护的渠道身份设置不能修改",
            ));
        }
    }

    state.db.save_settings(&settings).await?;
    if let Ok(next_status) = crate::scheduler::preview_status(&settings, Utc::now()) {
        *state.scheduler_status.write().await = next_status;
    }
    state.scheduler_notify.notify_one();
    Ok(Json(json!({"ok": true, "message": "设置已保存", "settings": settings})))
}

async fn save_connections(
    State(state): State<AppState>,
    Json(update): Json<ConnectionUpdate>,
) -> Result<Json<ConnectionSaveResponse>, AppError> {
    let mut settings = state.db.get_settings().await?;
    let old_base = settings.newapi_base_url.clone();
    let old_user = settings.newapi_admin_user_id.clone();
    let new_base = update.newapi_base_url.trim().trim_end_matches('/').to_string();
    let new_user = update.newapi_admin_user_id.trim().to_string();

    if new_user.is_empty() {
        return Err(AppError::bad("New API 管理员用户 ID 不能为空"));
    }

    if !state.db.list_managed_channels().await?.is_empty()
        && (old_base != new_base || old_user != new_user)
    {
        return Err(AppError::conflict(
            "受管渠道已存在，New API 地址和管理员用户 ID 已锁定；如需迁移实例，请先处理现有受管渠道",
        ));
    }

    settings.newapi_base_url = new_base;
    settings.newapi_admin_user_id = new_user;

    let mut encrypted_secrets = Vec::new();
    let openrouter_secret_changed = encrypt_secret_if_present(
        &state,
        "openrouter_api_key",
        update.openrouter_api_key,
        &mut encrypted_secrets,
    )?;
    let newapi_secret_changed = encrypt_secret_if_present(
        &state,
        "newapi_admin_token",
        update.newapi_admin_token,
        &mut encrypted_secrets,
    )?;
    let newapi_test_secret_changed = encrypt_secret_if_present(
        &state,
        "newapi_test_token",
        update.newapi_test_token,
        &mut encrypted_secrets,
    )?;

    state.db.save_connection_bundle(&settings, &encrypted_secrets).await?;

    let mut checks = state.db.get_connection_checks().await?;
    if openrouter_secret_changed {
        checks.openrouter = ConnectionCheck::default();
    }
    if old_base != settings.newapi_base_url
        || old_user != settings.newapi_admin_user_id
        || newapi_secret_changed
        || newapi_test_secret_changed
    {
        checks.newapi = ConnectionCheck::default();
    }
    state.db.save_connection_checks(&checks).await?;
    state.scheduler_notify.notify_one();

    Ok(Json(ConnectionSaveResponse {
        ok: true,
        message: "连接配置已保存".into(),
        settings,
        secrets: secret_status(&state).await?,
        connections: checks,
    }))
}

fn encrypt_secret_if_present(
    state: &AppState,
    name: &str,
    value: Option<String>,
    output: &mut Vec<(String, String)>,
) -> Result<bool, AppError> {
    if let Some(value) = value {
        let value = value.trim();
        if !value.is_empty() {
            output.push((name.to_string(), state.crypto.encrypt(value)?));
            return Ok(true);
        }
    }
    Ok(false)
}

async fn save_secret_if_present(
    state: &AppState,
    name: &str,
    value: Option<String>,
) -> Result<bool, AppError> {
    if let Some(value) = value {
        let value = value.trim();
        if !value.is_empty() {
            let encrypted = state.crypto.encrypt(value)?;
            state.db.set_secret_ciphertext(name, &encrypted).await?;
            return Ok(true);
        }
    }
    Ok(false)
}

// Kept for API compatibility. New UI uses /api/connections instead.
async fn save_secrets(
    State(state): State<AppState>,
    Json(update): Json<SecretUpdate>,
) -> Result<Json<Value>, AppError> {
    let openrouter_changed =
        save_secret_if_present(&state, "openrouter_api_key", update.openrouter_api_key).await?;
    let newapi_changed =
        save_secret_if_present(&state, "newapi_admin_token", update.newapi_admin_token).await?;
    let test_changed =
        save_secret_if_present(&state, "newapi_test_token", update.newapi_test_token).await?;

    if openrouter_changed || newapi_changed || test_changed {
        let mut checks = state.db.get_connection_checks().await?;
        if openrouter_changed {
            checks.openrouter = ConnectionCheck::default();
        }
        if newapi_changed || test_changed {
            checks.newapi = ConnectionCheck::default();
        }
        state.db.save_connection_checks(&checks).await?;
    }

    Ok(Json(json!({
        "ok": true,
        "message": "密钥已保存",
        "secrets": secret_status(&state).await?
    })))
}

async fn scan(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({"ok": true, "scan": crate::sync::scan(&state).await?})))
}

async fn sync_now(
    State(state): State<AppState>,
    Json(req): Json<SyncRequest>,
) -> Result<Json<Value>, AppError> {
    Ok(Json(json!({
        "ok": true,
        "run": crate::sync::run(&state, "manual", req.force).await?
    })))
}

async fn decrypt_secret(state: &AppState, name: &str) -> Result<String, AppError> {
    let c = state
        .db
        .get_secret_ciphertext(name)
        .await?
        .ok_or_else(|| AppError::bad(format!("{name} 尚未配置")))?;
    state.crypto.decrypt(&c)
}

async fn test_openrouter(
    State(state): State<AppState>,
) -> Result<Json<ConnectionTestResult>, AppError> {
    let settings = state.db.get_settings().await?;
    let key = decrypt_secret(&state, "openrouter_api_key").await?;
    let client = OpenRouterClient::new(state.http.clone(), &settings.openrouter_api_base);
    let started = Instant::now();
    let checked_at = Utc::now().to_rfc3339();

    match client.list_models(&key).await {
        Ok(models) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let detail = format!("已读取 {} 个模型", models.len());
            let check = ConnectionCheck {
                state: "connected".into(),
                checked_at: Some(checked_at.clone()),
                message: "OpenRouter 连接成功".into(),
                latency_ms: Some(latency_ms),
                detail: Some(detail.clone()),
            };
            let mut checks = state.db.get_connection_checks().await?;
            checks.openrouter = check;
            state.db.save_connection_checks(&checks).await?;
            Ok(Json(ConnectionTestResult {
                ok: true,
                connection: "openrouter".into(),
                state: "connected".into(),
                message: "OpenRouter 连接成功".into(),
                checked_at,
                latency_ms,
                detail,
            }))
        }
        Err(error) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let message = format!("OpenRouter 连接失败：{error}");
            let check = ConnectionCheck {
                state: "failed".into(),
                checked_at: Some(checked_at),
                message: message.clone(),
                latency_ms: Some(latency_ms),
                detail: None,
            };
            let mut checks = state.db.get_connection_checks().await?;
            checks.openrouter = check;
            state.db.save_connection_checks(&checks).await?;
            Err(AppError::bad(message))
        }
    }
}

async fn test_newapi(
    State(state): State<AppState>,
) -> Result<Json<ConnectionTestResult>, AppError> {
    let settings = state.db.get_settings().await?;
    if settings.newapi_base_url.trim().is_empty() {
        return Err(AppError::bad("请先填写并保存 New API 地址"));
    }
    let token = decrypt_secret(&state, "newapi_admin_token").await?;
    let client = NewApiClient::new(state.http.clone(), &settings, token)?;
    let started = Instant::now();
    let checked_at = Utc::now().to_rfc3339();

    match client.list_channels().await {
        Ok(channels) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let detail = format!("已读取 {} 条渠道", channels.len());
            let check = ConnectionCheck {
                state: "connected".into(),
                checked_at: Some(checked_at.clone()),
                message: "New API 连接成功".into(),
                latency_ms: Some(latency_ms),
                detail: Some(detail.clone()),
            };
            let mut checks = state.db.get_connection_checks().await?;
            checks.newapi = check;
            state.db.save_connection_checks(&checks).await?;
            Ok(Json(ConnectionTestResult {
                ok: true,
                connection: "newapi".into(),
                state: "connected".into(),
                message: "New API 连接成功".into(),
                checked_at,
                latency_ms,
                detail,
            }))
        }
        Err(error) => {
            let latency_ms = started.elapsed().as_millis() as u64;
            let message = format!("New API 连接失败：{error}");
            let check = ConnectionCheck {
                state: "failed".into(),
                checked_at: Some(checked_at),
                message: message.clone(),
                latency_ms: Some(latency_ms),
                detail: None,
            };
            let mut checks = state.db.get_connection_checks().await?;
            checks.newapi = check;
            state.db.save_connection_checks(&checks).await?;
            Err(AppError::bad(message))
        }
    }
}

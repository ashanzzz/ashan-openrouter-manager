use axum::{extract::State, routing::{get, post, put}, Json, Router};
use serde_json::{json, Value};

use crate::{AppState, error::AppError, models::{AppSettings, SecretStatus, SecretUpdate, StatusResponse, SyncRequest}, newapi::NewApiClient, openrouter::OpenRouterClient};

pub fn router(state:AppState)->Router{
    Router::new()
        .route("/api/status",get(status))
        .route("/api/models",get(models))
        .route("/api/history",get(history))
        .route("/api/settings",get(get_settings).put(save_settings))
        .route("/api/secrets",put(save_secrets))
        .route("/api/scan",post(scan))
        .route("/api/sync",post(sync_now))
        .route("/api/test/openrouter",post(test_openrouter))
        .route("/api/test/newapi",post(test_newapi))
        .with_state(state)
}

async fn secret_status(state:&AppState)->Result<SecretStatus,AppError>{Ok(SecretStatus{
    openrouter_api_key:state.db.has_secret("openrouter_api_key").await?,
    newapi_admin_token:state.db.has_secret("newapi_admin_token").await?,
    newapi_test_token:state.db.has_secret("newapi_test_token").await?,
})}

async fn status(State(state):State<AppState>)->Result<Json<StatusResponse>,AppError>{
    let secrets=secret_status(&state).await?; let current=state.db.list_managed_channels().await?; let last_scan=state.db.get_last_scan().await?; let last_run=state.db.last_run().await?; let scheduler=state.scheduler_status.read().await.clone();
    Ok(Json(StatusResponse{healthy:true,configured:secrets.openrouter_api_key&&secrets.newapi_admin_token,current_models:current,last_scan,last_run,scheduler,secrets}))
}
async fn models(State(state):State<AppState>)->Result<Json<Value>,AppError>{Ok(Json(json!({"scan":state.db.get_last_scan().await?}))) }
async fn history(State(state):State<AppState>)->Result<Json<Value>,AppError>{Ok(Json(json!({"runs":state.db.list_runs(100).await?}))) }
async fn get_settings(State(state):State<AppState>)->Result<Json<AppSettings>,AppError>{Ok(Json(state.db.get_settings().await?))}
async fn save_settings(State(state):State<AppState>,Json(settings):Json<AppSettings>)->Result<Json<Value>,AppError>{
    let old=state.db.get_settings().await?;
    if !state.db.list_managed_channels().await?.is_empty(){
        let protected_changed=old.newapi_base_url!=settings.newapi_base_url
            || old.newapi_admin_user_id!=settings.newapi_admin_user_id
            || old.openrouter_upstream_base!=settings.openrouter_upstream_base
            || old.alias_model!=settings.alias_model
            || old.managed_group!=settings.managed_group
            || old.managed_tag!=settings.managed_tag
            || old.channel_name_prefix!=settings.channel_name_prefix
            || old.channel_type!=settings.channel_type
            || old.enabled_status!=settings.enabled_status
            || old.disabled_status!=settings.disabled_status;
        if protected_changed{return Err(AppError::conflict("protected New API identity settings cannot be changed after managed channels exist"));}
    }
    state.db.save_settings(&settings).await?;state.scheduler_notify.notify_one();Ok(Json(json!({"ok":true})))
}

async fn save_secrets(State(state):State<AppState>,Json(update):Json<SecretUpdate>)->Result<Json<Value>,AppError>{
    for (name,value) in [("openrouter_api_key",update.openrouter_api_key),("newapi_admin_token",update.newapi_admin_token),("newapi_test_token",update.newapi_test_token)]{
        if let Some(value)=value { if !value.trim().is_empty(){ let encrypted=state.crypto.encrypt(value.trim())?;state.db.set_secret_ciphertext(name,&encrypted).await?; } }
    }
    Ok(Json(json!({"ok":true,"secrets":secret_status(&state).await?})))
}

async fn scan(State(state):State<AppState>)->Result<Json<Value>,AppError>{Ok(Json(json!({"ok":true,"scan":crate::sync::scan(&state).await?}))) }
async fn sync_now(State(state):State<AppState>,Json(req):Json<SyncRequest>)->Result<Json<Value>,AppError>{Ok(Json(json!({"ok":true,"run":crate::sync::run(&state,"manual",req.force).await?}))) }

async fn decrypt_secret(state:&AppState,name:&str)->Result<String,AppError>{let c=state.db.get_secret_ciphertext(name).await?.ok_or_else(||AppError::bad(format!("{name} is not configured")))?;state.crypto.decrypt(&c)}
async fn test_openrouter(State(state):State<AppState>)->Result<Json<Value>,AppError>{let settings=state.db.get_settings().await?;let key=decrypt_secret(&state,"openrouter_api_key").await?;let c=OpenRouterClient::new(state.http.clone(),&settings.openrouter_api_base);let models=c.list_models(&key).await?;Ok(Json(json!({"ok":true,"model_count":models.len()})))}
async fn test_newapi(State(state):State<AppState>)->Result<Json<Value>,AppError>{let settings=state.db.get_settings().await?;let token=decrypt_secret(&state,"newapi_admin_token").await?;let c=NewApiClient::new(state.http.clone(),&settings,token)?;let channels=c.list_channels().await?;Ok(Json(json!({"ok":true,"channel_count":channels.len()})))}

use std::{path::Path, str::FromStr};

use chrono::Utc;
use sqlx::{sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions}, Row, SqlitePool};

use crate::{error::AppError, models::{AppSettings, ConnectionChecks, ManagedChannel, ScanResult, SyncRun}};

#[derive(Clone)]
pub struct Database { pool: SqlitePool }

impl Database {
    pub async fn connect(path: &Path) -> anyhow::Result<Self> {
        let url = format!("sqlite://{}", path.to_string_lossy());
        let options = SqliteConnectOptions::from_str(&url)?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new().max_connections(5).connect_with(options).await?;
        Ok(Self { pool })
    }

    pub async fn init(&self) -> anyhow::Result<()> {
        sqlx::query("CREATE TABLE IF NOT EXISTS kv (key TEXT PRIMARY KEY, value TEXT NOT NULL, updated_at TEXT NOT NULL)").execute(&self.pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS secrets (name TEXT PRIMARY KEY, ciphertext TEXT NOT NULL, updated_at TEXT NOT NULL)").execute(&self.pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS managed_channels (rank INTEGER PRIMARY KEY, channel_id INTEGER UNIQUE NOT NULL, name TEXT NOT NULL, model_id TEXT NOT NULL, priority INTEGER NOT NULL, updated_at TEXT NOT NULL)").execute(&self.pool).await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS sync_runs (id TEXT PRIMARY KEY, started_at TEXT NOT NULL, ended_at TEXT NOT NULL, trigger TEXT NOT NULL, status TEXT NOT NULL, changed INTEGER NOT NULL, selected_models TEXT NOT NULL, error TEXT)").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn ensure_default_settings(&self) -> anyhow::Result<()> {
        if self.get_kv("settings").await?.is_none() {
            self.save_settings(&AppSettings::default()).await?;
        }
        Ok(())
    }

    async fn get_kv(&self, key: &str) -> anyhow::Result<Option<String>> {
        let row = sqlx::query("SELECT value FROM kv WHERE key = ?").bind(key).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    async fn set_kv(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query("INSERT INTO kv(key,value,updated_at) VALUES(?,?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
            .bind(key).bind(value).bind(Utc::now().to_rfc3339()).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_settings(&self) -> Result<AppSettings, AppError> {
        match self.get_kv("settings").await? {
            Some(v) => Ok(serde_json::from_str(&v)?),
            None => Ok(AppSettings::default()),
        }
    }

    fn validate_settings(settings: &AppSettings) -> Result<(), AppError> {
        if settings.candidate_pool < 3 { return Err(AppError::bad("candidate_pool must be at least 3")); }
        if settings.preflight_concurrency == 0 { return Err(AppError::bad("preflight_concurrency must be at least 1")); }
        if settings.sync_interval_minutes < 60 { return Err(AppError::bad("sync interval must be at least 60 minutes")); }
        if settings.alias_model.trim().is_empty() { return Err(AppError::bad("alias_model cannot be empty")); }
        Ok(())
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> Result<(), AppError> {
        Self::validate_settings(settings)?;
        self.set_kv("settings", &serde_json::to_string(settings)?).await?;
        Ok(())
    }

    pub async fn save_connection_bundle(&self, settings: &AppSettings, secrets: &[(String, String)]) -> Result<(), AppError> {
        Self::validate_settings(settings)?;
        let settings_json = serde_json::to_string(settings)?;
        let now = Utc::now().to_rfc3339();
        let mut tx = self.pool.begin().await?;

        sqlx::query("INSERT INTO kv(key,value,updated_at) VALUES(?,?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
            .bind("settings").bind(settings_json).bind(&now).execute(&mut *tx).await?;

        for (name, ciphertext) in secrets {
            sqlx::query("INSERT INTO secrets(name,ciphertext,updated_at) VALUES(?,?,?) ON CONFLICT(name) DO UPDATE SET ciphertext=excluded.ciphertext, updated_at=excluded.updated_at")
                .bind(name).bind(ciphertext).bind(&now).execute(&mut *tx).await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn set_secret_ciphertext(&self, name: &str, ciphertext: &str) -> Result<(), AppError> {
        sqlx::query("INSERT INTO secrets(name,ciphertext,updated_at) VALUES(?,?,?) ON CONFLICT(name) DO UPDATE SET ciphertext=excluded.ciphertext, updated_at=excluded.updated_at")
            .bind(name).bind(ciphertext).bind(Utc::now().to_rfc3339()).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_secret_ciphertext(&self, name: &str) -> Result<Option<String>, AppError> {
        let row = sqlx::query("SELECT ciphertext FROM secrets WHERE name = ?").bind(name).fetch_optional(&self.pool).await?;
        Ok(row.map(|r| r.get::<String,_>("ciphertext")))
    }

    pub async fn has_secret(&self, name: &str) -> Result<bool, AppError> {
        Ok(self.get_secret_ciphertext(name).await?.is_some())
    }

    pub async fn get_connection_checks(&self) -> Result<ConnectionChecks, AppError> {
        match self.get_kv("connection_checks").await? {
            Some(v) => Ok(serde_json::from_str(&v).unwrap_or_default()),
            None => Ok(ConnectionChecks::default()),
        }
    }

    pub async fn save_connection_checks(&self, checks: &ConnectionChecks) -> Result<(), AppError> {
        self.set_kv("connection_checks", &serde_json::to_string(checks)?).await?;
        Ok(())
    }

    pub async fn save_last_scan(&self, scan: &ScanResult) -> Result<(), AppError> {
        self.set_kv("last_scan", &serde_json::to_string(scan)?).await?;
        Ok(())
    }

    pub async fn get_last_scan(&self) -> Result<Option<ScanResult>, AppError> {
        match self.get_kv("last_scan").await? { Some(v) => Ok(Some(serde_json::from_str(&v)?)), None => Ok(None) }
    }

    pub async fn upsert_managed_channel(&self, c: &ManagedChannel) -> Result<(), AppError> {
        sqlx::query("INSERT INTO managed_channels(rank,channel_id,name,model_id,priority,updated_at) VALUES(?,?,?,?,?,?) ON CONFLICT(rank) DO UPDATE SET channel_id=excluded.channel_id,name=excluded.name,model_id=excluded.model_id,priority=excluded.priority,updated_at=excluded.updated_at")
            .bind(c.rank).bind(c.channel_id).bind(&c.name).bind(&c.model_id).bind(c.priority).bind(&c.updated_at).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_managed_channels(&self) -> Result<Vec<ManagedChannel>, AppError> {
        let rows = sqlx::query("SELECT rank,channel_id,name,model_id,priority,updated_at FROM managed_channels ORDER BY rank").fetch_all(&self.pool).await?;
        Ok(rows.into_iter().map(|r| ManagedChannel {
            rank: r.get("rank"), channel_id: r.get("channel_id"), name: r.get("name"), model_id: r.get("model_id"), priority: r.get("priority"), updated_at: r.get("updated_at")
        }).collect())
    }

    pub async fn record_run(&self, run: &SyncRun) -> Result<(), AppError> {
        sqlx::query("INSERT OR REPLACE INTO sync_runs(id,started_at,ended_at,trigger,status,changed,selected_models,error) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&run.id).bind(&run.started_at).bind(&run.ended_at).bind(&run.trigger).bind(&run.status).bind(if run.changed {1} else {0})
            .bind(serde_json::to_string(&run.selected_models)?).bind(&run.error).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_runs(&self, limit: i64) -> Result<Vec<SyncRun>, AppError> {
        let rows = sqlx::query("SELECT id,started_at,ended_at,trigger,status,changed,selected_models,error FROM sync_runs ORDER BY started_at DESC LIMIT ?")
            .bind(limit).fetch_all(&self.pool).await?;
        let mut out = Vec::new();
        for r in rows {
            let raw: String = r.get("selected_models");
            out.push(SyncRun {
                id: r.get("id"), started_at: r.get("started_at"), ended_at: r.get("ended_at"), trigger: r.get("trigger"), status: r.get("status"), changed: r.get::<i64,_>("changed") != 0,
                selected_models: serde_json::from_str(&raw).unwrap_or_default(), error: r.get("error")
            });
        }
        Ok(out)
    }

    pub async fn last_run(&self) -> Result<Option<SyncRun>, AppError> {
        Ok(self.list_runs(1).await?.into_iter().next())
    }
}

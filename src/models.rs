use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub openrouter_api_base: String,
    pub openrouter_upstream_base: String,
    pub newapi_base_url: String,
    pub newapi_admin_user_id: String,
    pub alias_model: String,
    pub managed_group: String,
    pub managed_tag: String,
    pub channel_name_prefix: String,
    pub min_context_length: i64,
    pub candidate_pool: usize,
    pub preflight_concurrency: usize,
    pub require_benchmark: bool,
    pub require_free_suffix: bool,
    pub include_models: Vec<String>,
    pub exclude_models: Vec<String>,
    pub ranking_mode: RankingMode,
    pub intelligence_weight: f64,
    pub coding_weight: f64,
    pub agentic_weight: f64,
    pub context_weight: f64,
    pub auto_sync: bool,
    pub sync_interval_minutes: u64,
    pub channel_type: i64,
    pub enabled_status: i64,
    pub disabled_status: i64,
    pub priority_base: i64,
    pub priority_step: i64,
    pub channel_weight: i64,
    pub auto_ban: bool,
    pub e2e_test_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            openrouter_api_base: "https://openrouter.ai/api/v1".into(),
            openrouter_upstream_base: "https://openrouter.ai/api".into(),
            newapi_base_url: String::new(),
            newapi_admin_user_id: "1".into(),
            alias_model: "ashan-ai-model".into(),
            managed_group: "wm-ashan-openrouter-free".into(),
            managed_tag: "ashan-openrouter-manager-v3".into(),
            channel_name_prefix: "[AOM3]".into(),
            min_context_length: 32768,
            candidate_pool: 12,
            preflight_concurrency: 3,
            require_benchmark: true,
            require_free_suffix: false,
            include_models: vec![],
            exclude_models: vec![],
            ranking_mode: RankingMode::IntelligenceFirst,
            intelligence_weight: 0.70,
            coding_weight: 0.20,
            agentic_weight: 0.10,
            context_weight: 0.0,
            auto_sync: false,
            sync_interval_minutes: 360,
            channel_type: 20,
            enabled_status: 1,
            disabled_status: 2,
            priority_base: 10003,
            priority_step: 1,
            channel_weight: 100,
            auto_ban: true,
            e2e_test_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RankingMode { IntelligenceFirst, Weighted }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    #[serde(default)] pub canonical_slug: Option<String>,
    #[serde(default)] pub context_length: i64,
    pub pricing: ModelPricing,
    #[serde(default)] pub architecture: ModelArchitecture,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelPricing {
    #[serde(default)] pub prompt: String,
    #[serde(default)] pub completion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ModelArchitecture {
    #[serde(default)] pub modality: String,
    #[serde(default)] pub output_modalities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkItem {
    pub model_permaslug: String,
    #[serde(default)] pub display_name: String,
    #[serde(default)] pub intelligence_index: Option<f64>,
    #[serde(default)] pub coding_index: Option<f64>,
    #[serde(default)] pub agentic_index: Option<f64>,
    #[serde(default)] pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedModel {
    pub rank: usize,
    pub id: String,
    pub name: String,
    pub context_length: i64,
    pub intelligence_index: Option<f64>,
    pub coding_index: Option<f64>,
    pub agentic_index: Option<f64>,
    pub score: f64,
    pub usable: Option<bool>,
    pub test_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub scanned_at: String,
    pub total_models: usize,
    pub free_models: usize,
    pub ranked_candidates: Vec<RankedModel>,
    pub selected: Vec<RankedModel>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedChannel {
    pub rank: i64,
    pub channel_id: i64,
    pub name: String,
    pub model_id: String,
    pub priority: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRun {
    pub id: String,
    pub started_at: String,
    pub ended_at: String,
    pub trigger: String,
    pub status: String,
    pub changed: bool,
    pub selected_models: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SchedulerStatus {
    pub next_run_at: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretStatus {
    pub openrouter_api_key: bool,
    pub newapi_admin_token: bool,
    pub newapi_test_token: bool,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionCheck {
    pub state: String,
    pub checked_at: Option<String>,
    pub message: String,
    pub latency_ms: Option<u64>,
    pub detail: Option<String>,
}

impl Default for ConnectionCheck {
    fn default() -> Self {
        Self { state: "unknown".into(), checked_at: None, message: "尚未测试".into(), latency_ms: None, detail: None }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ConnectionChecks {
    pub openrouter: ConnectionCheck,
    pub newapi: ConnectionCheck,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionUpdate {
    pub newapi_base_url: String,
    pub newapi_admin_user_id: String,
    pub openrouter_api_key: Option<String>,
    pub newapi_admin_token: Option<String>,
    pub newapi_test_token: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionSaveResponse {
    pub ok: bool,
    pub message: String,
    pub settings: AppSettings,
    pub secrets: SecretStatus,
    pub connections: ConnectionChecks,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConnectionTestResult {
    pub ok: bool,
    pub connection: String,
    pub state: String,
    pub message: String,
    pub checked_at: String,
    pub latency_ms: u64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub healthy: bool,
    pub configured: bool,
    pub current_models: Vec<ManagedChannel>,
    pub last_scan: Option<ScanResult>,
    pub last_run: Option<SyncRun>,
    pub scheduler: SchedulerStatus,
    pub secrets: SecretStatus,
    pub connections: ConnectionChecks,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SecretUpdate {
    pub openrouter_api_key: Option<String>,
    pub newapi_admin_token: Option<String>,
    pub newapi_test_token: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SyncRequest { #[serde(default)] pub force: bool }

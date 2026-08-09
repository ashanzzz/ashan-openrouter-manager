export type RankingMode = 'intelligence_first' | 'weighted'
export type ScheduleMode = 'interval' | 'daily'
export type ConnectionState = 'unknown' | 'connected' | 'failed'

export interface Settings {
  openrouter_api_base: string
  openrouter_upstream_base: string
  newapi_base_url: string
  newapi_admin_user_id: string
  alias_model: string
  managed_group: string
  managed_tag: string
  channel_name_prefix: string
  min_context_length: number
  candidate_pool: number
  preflight_concurrency: number
  require_benchmark: boolean
  require_free_suffix: boolean
  include_models: string[]
  exclude_models: string[]
  ranking_mode: RankingMode
  intelligence_weight: number
  coding_weight: number
  agentic_weight: number
  context_weight: number
  auto_sync: boolean
  schedule_mode: ScheduleMode
  sync_interval_minutes: number
  daily_sync_time: string
  schedule_timezone: string
  channel_type: number
  enabled_status: number
  disabled_status: number
  priority_base: number
  priority_step: number
  channel_weight: number
  auto_ban: boolean
  e2e_test_enabled: boolean
}

export interface RankedModel {
  rank: number
  id: string
  name: string
  context_length: number
  intelligence_index?: number | null
  coding_index?: number | null
  agentic_index?: number | null
  score: number
  usable?: boolean | null
  test_error?: string | null
}

export interface Scan {
  scanned_at: string
  total_models: number
  free_models: number
  ranked_candidates: RankedModel[]
  selected: RankedModel[]
  warning?: string | null
}

export interface Channel {
  rank: number
  channel_id: number
  name: string
  model_id: string
  priority: number
  updated_at: string
}

export interface Run {
  id: string
  started_at: string
  ended_at: string
  trigger: string
  status: string
  changed: boolean
  selected_models: string[]
  error?: string | null
}


export interface SyncLogEntry {
  run_id: string
  seq: number
  timestamp: string
  level: 'info' | 'success' | 'warning' | 'error' | string
  stage: string
  category: string
  message: string
  detail?: string | null
}

export interface SyncProgress {
  run: Run
  logs: SyncLogEntry[]
}

export interface SyncStartResponse {
  ok: boolean
  run_id: string
  status: string
}

export interface ConnectionCheck {
  state: ConnectionState
  checked_at?: string | null
  message: string
  latency_ms?: number | null
  detail?: string | null
}

export interface ConnectionChecks {
  openrouter: ConnectionCheck
  newapi: ConnectionCheck
}

export interface SecretStatus {
  openrouter_api_key: boolean
  newapi_admin_token: boolean
  newapi_test_token: boolean
}

export interface Status {
  healthy: boolean
  configured: boolean
  current_models: Channel[]
  last_scan?: Scan | null
  last_run?: Run | null
  scheduler: { next_run_at?: string | null; next_run_local?: string | null; enabled: boolean; mode: string }
  secrets: SecretStatus
  connections: ConnectionChecks
  active_sync_run_id?: string | null
}

export interface ConnectionUpdate {
  newapi_base_url: string
  newapi_admin_user_id: string
  openrouter_api_key?: string
  newapi_admin_token?: string
  newapi_test_token?: string
}

export interface ConnectionSaveResponse {
  ok: boolean
  message: string
  settings: Settings
  secrets: SecretStatus
  connections: ConnectionChecks
}

export interface ConnectionTestResult {
  ok: boolean
  connection: 'openrouter' | 'newapi'
  state: 'connected'
  message: string
  checked_at: string
  latency_ms: number
  detail: string
}

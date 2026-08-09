use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};

use chrono::Utc;
use futures_util::{stream, StreamExt};
use tokio::sync::OwnedMutexGuard;
use uuid::Uuid;

use crate::{
    error::AppError,
    models::{
        ManagedChannel, RankedModel, ScanResult, SyncLogEntry, SyncRun, SyncStartResponse,
    },
    newapi::NewApiClient,
    openrouter::OpenRouterClient,
    ranking, tester,
    AppState,
};

#[derive(Clone)]
struct SyncLogger {
    db: crate::db::Database,
    run_id: String,
    seq: Arc<AtomicI64>,
}

impl SyncLogger {
    fn new(db: crate::db::Database, run_id: String) -> Self {
        Self {
            db,
            run_id,
            seq: Arc::new(AtomicI64::new(0)),
        }
    }

    async fn log(
        &self,
        level: &str,
        stage: &str,
        category: &str,
        message: impl Into<String>,
        detail: Option<String>,
    ) {
        let entry = SyncLogEntry {
            run_id: self.run_id.clone(),
            seq: self.seq.fetch_add(1, Ordering::SeqCst) + 1,
            timestamp: Utc::now().to_rfc3339(),
            level: level.into(),
            stage: stage.into(),
            category: category.into(),
            message: message.into(),
            detail,
        };
        let _ = self.db.append_run_log(&entry).await;
    }

    async fn error(&self, stage: &str, error: &AppError) {
        self.log(
            "error",
            stage,
            classify_error(stage, error),
            format!("{}阶段失败", stage_label(stage)),
            Some(error.to_string()),
        )
        .await;
    }
}

fn stage_label(stage: &str) -> &'static str {
    match stage {
        "configuration" => "配置校验",
        "openrouter_models" => "OpenRouter 模型目录",
        "openrouter_benchmarks" => "OpenRouter Benchmark",
        "ranking" => "模型筛选与排名",
        "preflight" => "候选模型真实调用",
        "newapi_connection" => "New API 连接与权限",
        "newapi_conflicts" => "New API 渠道冲突检查",
        "newapi_identity" => "受管渠道身份校验",
        "newapi_create" => "New API 渠道初始化",
        "newapi_update" => "New API 渠道更新",
        "newapi_test" => "New API 渠道测试",
        "e2e" => "端到端测试",
        "rollback" => "回滚",
        "complete" => "完成",
        _ => "同步",
    }
}

fn classify_error(stage: &str, error: &AppError) -> &'static str {
    match error {
        AppError::Unauthorized(_) => {
            if stage.starts_with("newapi") || stage == "e2e" {
                "newapi_permission"
            } else {
                "openrouter_permission"
            }
        }
        AppError::Conflict(_) => "safety_conflict",
        AppError::BadRequest(message) => {
            if message.contains("尚未配置") || message.contains("地址为空") || message.contains("用户 ID 为空") || message.contains("不能为空") {
                return "configuration";
            }
            match stage {
            "configuration" => "configuration",
            "openrouter_models" | "openrouter_benchmarks" | "preflight" => "openrouter_api",
            "newapi_connection" | "newapi_create" | "newapi_update" | "newapi_test" | "e2e" => "newapi_api",
            _ => "validation",
            }
        }
        AppError::Internal(_) => "network_or_internal",
    }
}

async fn secret(state: &AppState, name: &str) -> Result<String, AppError> {
    let encrypted = state
        .db
        .get_secret_ciphertext(name)
        .await?
        .ok_or_else(|| AppError::bad(format!("secret {name} 尚未配置")))?;
    state.crypto.decrypt(&encrypted)
}

pub async fn scan(state: &AppState) -> Result<ScanResult, AppError> {
    let settings = state.db.get_settings().await?;
    let key = secret(state, "openrouter_api_key").await?;
    let client = OpenRouterClient::new(state.http.clone(), &settings.openrouter_api_base);
    let models = client.list_models(&key).await?;
    let total = models.len();
    let benchmarks = client.benchmarks(&key).await?;
    let (free_count, candidates) = ranking::rank(models, benchmarks, &settings);
    let tested = tester::preflight(client, key, candidates, settings.preflight_concurrency).await;
    let selected: Vec<_> = tested
        .iter()
        .filter(|m| m.usable == Some(true))
        .take(3)
        .cloned()
        .collect();
    let warning = if selected.len() < 3 {
        Some(format!(
            "only {} usable models found; production will not be changed",
            selected.len()
        ))
    } else {
        None
    };
    let result = ScanResult {
        scanned_at: Utc::now().to_rfc3339(),
        total_models: total,
        free_models: free_count,
        ranked_candidates: tested,
        selected,
        warning,
    };
    state.db.save_last_scan(&result).await?;
    Ok(result)
}

pub async fn start_manual(state: AppState, force: bool) -> Result<SyncStartResponse, AppError> {
    let guard = state
        .sync_guard
        .clone()
        .try_lock_owned()
        .map_err(|_| AppError::conflict("已有同步任务正在执行，请等待当前任务完成"))?;
    let (run, logger) = begin_run(&state, "manual").await?;
    let run_id = run.id.clone();

    tokio::spawn(async move {
        let _guard = guard;
        let _ = execute_run(&state, run, logger, force).await;
    });

    Ok(SyncStartResponse {
        ok: true,
        run_id,
        status: "running".into(),
    })
}

pub async fn run(state: &AppState, trigger: &str, force: bool) -> Result<SyncRun, AppError> {
    let guard = state
        .sync_guard
        .clone()
        .try_lock_owned()
        .map_err(|_| AppError::conflict("已有同步任务正在执行"))?;
    let (run, logger) = begin_run(state, trigger).await?;
    execute_run_with_guard(state, guard, run, logger, force).await
}

async fn execute_run_with_guard(
    state: &AppState,
    _guard: OwnedMutexGuard<()>,
    run: SyncRun,
    logger: SyncLogger,
    force: bool,
) -> Result<SyncRun, AppError> {
    execute_run(state, run, logger, force).await
}

async fn begin_run(state: &AppState, trigger: &str) -> Result<(SyncRun, SyncLogger), AppError> {
    let started = Utc::now().to_rfc3339();
    let run = SyncRun {
        id: Uuid::new_v4().to_string(),
        started_at: started.clone(),
        ended_at: started,
        trigger: trigger.into(),
        status: "running".into(),
        changed: false,
        selected_models: vec![],
        error: None,
    };
    state.db.record_run(&run).await?;
    let logger = SyncLogger::new(state.db.clone(), run.id.clone());
    logger
        .log(
            "info",
            "configuration",
            "lifecycle",
            if trigger == "manual" {
                "手动同步任务已启动"
            } else {
                "定时同步任务已启动"
            },
            Some(format!("Run ID: {}", run.id)),
        )
        .await;
    Ok((run, logger))
}

async fn execute_run(
    state: &AppState,
    mut run: SyncRun,
    logger: SyncLogger,
    force: bool,
) -> Result<SyncRun, AppError> {
    let result = run_inner(state, &logger, force).await;
    run.ended_at = Utc::now().to_rfc3339();

    match result {
        Ok((changed, models, status)) => {
            run.changed = changed;
            run.selected_models = models.clone();
            run.status = status.clone();
            logger
                .log(
                    "success",
                    "complete",
                    "result",
                    if changed { "同步完成，New API 已更新" } else { "同步完成，无需更新" },
                    Some(format!("Top 3: {}", models.join(" | "))),
                )
                .await;
            state.db.record_run(&run).await?;
            Ok(run)
        }
        Err(error) => {
            logger.error("complete", &error).await;
            run.status = "failed".into();
            run.error = Some(error.to_string());
            state.db.record_run(&run).await?;
            Err(AppError::bad(run.error.clone().unwrap_or_else(|| "sync failed".into())))
        }
    }
}

async fn scan_with_logger(
    state: &AppState,
    logger: &SyncLogger,
    settings: &crate::models::AppSettings,
) -> Result<ScanResult, AppError> {
    logger
        .log(
            "info",
            "configuration",
            "configuration",
            "正在校验同步配置",
            Some(format!(
                "OpenRouter API: {}；New API: {}；管理员用户 ID: {}；别名: {}",
                settings.openrouter_api_base,
                if settings.newapi_base_url.trim().is_empty() { "未配置" } else { settings.newapi_base_url.as_str() },
                settings.newapi_admin_user_id,
                settings.alias_model
            )),
        )
        .await;

    let key = match secret(state, "openrouter_api_key").await {
        Ok(v) => v,
        Err(e) => {
            logger.error("configuration", &e).await;
            return Err(e);
        }
    };
    logger
        .log(
            "success",
            "configuration",
            "configuration",
            "基础配置校验通过",
            Some("OpenRouter Key 已存在；密钥明文不会写入日志".into()),
        )
        .await;

    let client = OpenRouterClient::new(state.http.clone(), &settings.openrouter_api_base);

    logger
        .log(
            "info",
            "openrouter_models",
            "openrouter_api",
            "正在读取 OpenRouter 模型目录",
            None,
        )
        .await;
    let models = match client.list_models(&key).await {
        Ok(v) => v,
        Err(e) => {
            logger.error("openrouter_models", &e).await;
            return Err(e);
        }
    };
    let total = models.len();
    logger
        .log(
            "success",
            "openrouter_models",
            "openrouter_api",
            format!("已读取 {total} 个 OpenRouter 模型"),
            None,
        )
        .await;

    logger
        .log(
            "info",
            "openrouter_benchmarks",
            "openrouter_api",
            "正在读取 Artificial Analysis Benchmark",
            None,
        )
        .await;
    let benchmarks = match client.benchmarks(&key).await {
        Ok(v) => v,
        Err(e) => {
            logger.error("openrouter_benchmarks", &e).await;
            return Err(e);
        }
    };
    let benchmark_count = benchmarks.len();
    logger
        .log(
            "success",
            "openrouter_benchmarks",
            "openrouter_api",
            format!("已读取 {benchmark_count} 条 Benchmark 数据"),
            None,
        )
        .await;

    let (free_count, candidates) = ranking::rank(models, benchmarks, settings);
    logger
        .log(
            "success",
            "ranking",
            "selection",
            format!("筛选完成：{free_count} 个符合基础免费规则，{} 个进入候选池", candidates.len()),
            Some(
                candidates
                    .iter()
                    .take(8)
                    .map(|m| format!("#{} {}", m.rank, m.id))
                    .collect::<Vec<_>>()
                    .join("；"),
            ),
        )
        .await;

    logger
        .log(
            "info",
            "preflight",
            "openrouter_api",
            format!("开始真实调用候选模型，并发数 {}", settings.preflight_concurrency.max(1)),
            None,
        )
        .await;

    let mut pending = stream::iter(candidates.into_iter().map(|mut model| {
        let client = client.clone();
        let key = key.clone();
        async move {
            match client.test_model(&key, &model.id).await {
                Ok(_) => model.usable = Some(true),
                Err(e) => {
                    model.usable = Some(false);
                    model.test_error = Some(e.to_string());
                }
            }
            model
        }
    }))
    .buffer_unordered(settings.preflight_concurrency.max(1));

    let mut tested: Vec<RankedModel> = Vec::new();
    while let Some(model) = pending.next().await {
        if model.usable == Some(true) {
            logger
                .log(
                    "success",
                    "preflight",
                    "openrouter_api",
                    format!("候选 #{} 可用：{}", model.rank, model.id),
                    None,
                )
                .await;
        } else {
            logger
                .log(
                    "warning",
                    "preflight",
                    "openrouter_api",
                    format!("候选 #{} 不可用：{}", model.rank, model.id),
                    model.test_error.clone(),
                )
                .await;
        }
        tested.push(model);
    }
    tested.sort_by_key(|m| m.rank);

    let selected: Vec<_> = tested
        .iter()
        .filter(|m| m.usable == Some(true))
        .take(3)
        .cloned()
        .collect();
    let warning = if selected.len() < 3 {
        Some(format!("只找到 {} 个可用模型；不会修改生产渠道", selected.len()))
    } else {
        None
    };

    if selected.len() == 3 {
        logger
            .log(
                "success",
                "preflight",
                "selection",
                "已选出 3 个可用模型",
                Some(
                    selected
                        .iter()
                        .enumerate()
                        .map(|(i, m)| format!("#{} {}", i + 1, m.id))
                        .collect::<Vec<_>>()
                        .join("；"),
                ),
            )
            .await;
    } else {
        logger
            .log(
                "error",
                "preflight",
                "selection",
                "可用模型不足 3 个，安全停止",
                warning.clone(),
            )
            .await;
    }

    let result = ScanResult {
        scanned_at: Utc::now().to_rfc3339(),
        total_models: total,
        free_models: free_count,
        ranked_candidates: tested,
        selected,
        warning,
    };
    state.db.save_last_scan(&result).await?;
    Ok(result)
}

async fn run_inner(
    state: &AppState,
    logger: &SyncLogger,
    force: bool,
) -> Result<(bool, Vec<String>, String), AppError> {
    let settings = state.db.get_settings().await?;
    let scan = scan_with_logger(state, logger, &settings).await?;
    if scan.selected.len() != 3 {
        return Err(AppError::bad(
            scan.warning
                .unwrap_or_else(|| "three usable models are required".into()),
        ));
    }
    let selected_ids: Vec<String> = scan.selected.iter().map(|m| m.id.clone()).collect();

    let openrouter_key = match secret(state, "openrouter_api_key").await {
        Ok(v) => v,
        Err(e) => {
            logger.error("configuration", &e).await;
            return Err(e);
        }
    };
    let admin_token = match secret(state, "newapi_admin_token").await {
        Ok(v) => v,
        Err(e) => {
            logger.error("configuration", &e).await;
            return Err(e);
        }
    };

    logger
        .log(
            "info",
            "newapi_connection",
            "newapi_api",
            "正在连接 New API 并验证管理员读取权限",
            Some(format!(
                "地址: {}；管理员用户 ID: {}",
                settings.newapi_base_url, settings.newapi_admin_user_id
            )),
        )
        .await;
    let client = match NewApiClient::new(state.http.clone(), &settings, admin_token) {
        Ok(v) => v,
        Err(e) => {
            logger.error("newapi_connection", &e).await;
            return Err(e);
        }
    };
    if let Err(e) = client.test_connection().await {
        logger.error("newapi_connection", &e).await;
        return Err(e);
    }
    logger
        .log(
            "success",
            "newapi_connection",
            "newapi_api",
            "New API 连接成功，管理员渠道读取权限正常",
            None,
        )
        .await;

    let mut managed = state.db.list_managed_channels().await?;
    logger
        .log(
            "info",
            "newapi_conflicts",
            "safety",
            format!("开始检查外部渠道；本地登记受管渠道 {} 条", managed.len()),
            None,
        )
        .await;
    let report = match client.inspect_foreign_channels(&settings, &managed).await {
        Ok(v) => v,
        Err(e) => {
            logger.error("newapi_conflicts", &e).await;
            return Err(e);
        }
    };

    for finding in report.warnings() {
        logger
            .log(
                "warning",
                "newapi_conflicts",
                "legacy_channel",
                format!("外部渠道 ID {} [{}] 仅警告，不阻止同步", finding.id, finding.name),
                Some(finding.reason.clone()),
            )
            .await;
    }
    let hard = report.hard_conflicts();
    for finding in &hard {
        logger
            .log(
                "error",
                "newapi_conflicts",
                "safety_conflict",
                format!("外部渠道 ID {} [{}] 构成硬冲突", finding.id, finding.name),
                Some(finding.reason.clone()),
            )
            .await;
    }
    if !hard.is_empty() {
        let summary = hard
            .iter()
            .map(|f| format!("{}:{}", f.id, f.reason))
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(AppError::conflict(format!(
            "发现 {} 个 New API 硬冲突渠道；未修改任何渠道。{}",
            hard.len(), summary
        )));
    }
    logger
        .log(
            "success",
            "newapi_conflicts",
            "safety",
            "渠道冲突检查通过",
            Some("同分组/相似名称仅作为历史渠道警告；只有别名占用、映射占用或明确的孤儿 v3 渠道会阻断同步".into()),
        )
        .await;

    if managed.is_empty() {
        logger
            .log(
                "info",
                "newapi_create",
                "newapi_api",
                "尚未登记受管渠道，开始首次初始化 3 条渠道",
                None,
            )
            .await;
        let mut created = Vec::new();
        for (idx, model) in scan.selected.iter().enumerate() {
            let rank = (idx + 1) as i64;
            logger
                .log(
                    "info",
                    "newapi_create",
                    "newapi_api",
                    format!("正在创建 Rank {rank} 渠道"),
                    Some(model.id.clone()),
                )
                .await;
            match client
                .create_channel(&settings, &openrouter_key, model, rank)
                .await
            {
                Ok(c) => {
                    logger
                        .log(
                            "success",
                            "newapi_create",
                            "newapi_api",
                            format!("已创建渠道 ID {}", c.channel_id),
                            Some(format!("{} -> {}", settings.alias_model, model.id)),
                        )
                        .await;
                    created.push(c)
                }
                Err(e) => {
                    logger.error("newapi_create", &e).await;
                    logger
                        .log(
                            "warning",
                            "rollback",
                            "safety",
                            "初始化失败，尝试清理本次已创建渠道",
                            None,
                        )
                        .await;
                    let _ = client.delete_exact(&settings, &created).await;
                    return Err(e);
                }
            }
        }

        for c in &created {
            logger
                .log(
                    "info",
                    "newapi_test",
                    "newapi_api",
                    format!("正在测试新渠道 ID {}", c.channel_id),
                    None,
                )
                .await;
            if let Err(e) = client.test_channel(c.channel_id, &settings.alias_model).await {
                logger.error("newapi_test", &e).await;
                let _ = client.delete_exact(&settings, &created).await;
                return Err(e);
            }
            logger
                .log(
                    "success",
                    "newapi_test",
                    "newapi_api",
                    format!("渠道 ID {} 测试通过", c.channel_id),
                    None,
                )
                .await;
        }
        for c in &created {
            if let Err(e) = client.set_status(c.channel_id, settings.enabled_status).await {
                logger.error("newapi_create", &e).await;
                let _ = client.delete_exact(&settings, &created).await;
                return Err(e);
            }
        }
        if let Err(e) = maybe_e2e(state, &client, &settings, Some(logger)).await {
            let _ = client.delete_exact(&settings, &created).await;
            return Err(e);
        }
        for c in &created {
            state.db.upsert_managed_channel(c).await?;
        }
        return Ok((true, selected_ids, "initialized".into()));
    }

    if managed.len() != 3 {
        let e = AppError::conflict(format!(
            "本地数据库应登记 3 条受管渠道，实际找到 {} 条",
            managed.len()
        ));
        logger.error("newapi_identity", &e).await;
        return Err(e);
    }

    managed.sort_by_key(|c| c.rank);
    for c in &managed {
        logger
            .log(
                "info",
                "newapi_identity",
                "safety",
                format!("校验受管渠道 ID {} / Rank {}", c.channel_id, c.rank),
                Some(c.name.clone()),
            )
            .await;
        if let Err(e) = client.assert_owned(&settings, c).await {
            logger.error("newapi_identity", &e).await;
            return Err(e);
        }
    }
    logger
        .log(
            "success",
            "newapi_identity",
            "safety",
            "3 条受管渠道身份校验通过",
            None,
        )
        .await;

    let current: Vec<String> = managed.iter().map(|c| c.model_id.clone()).collect();
    if current == selected_ids && !force {
        logger
            .log(
                "success",
                "complete",
                "no_change",
                "当前 Top 3 与 New API 已登记模型完全一致，无需写入",
                Some(current.join(" | ")),
            )
            .await;
        return Ok((false, selected_ids, "no_change".into()));
    }

    let previous = managed.clone();
    for (c, new_model) in managed.iter().zip(scan.selected.iter()) {
        logger
            .log(
                "info",
                "newapi_update",
                "newapi_api",
                format!("正在更新渠道 ID {} / Rank {}", c.channel_id, c.rank),
                Some(format!("{} -> {}", c.model_id, new_model.id)),
            )
            .await;
        if let Err(error) = client
            .update_model(&settings, c, &openrouter_key, &new_model.id)
            .await
        {
            logger.error("newapi_update", &error).await;
            rollback(&client, &settings, &openrouter_key, &previous, logger).await;
            return Err(error);
        }
        logger
            .log(
                "success",
                "newapi_update",
                "newapi_api",
                format!("渠道 ID {} 映射更新成功", c.channel_id),
                Some(new_model.id.clone()),
            )
            .await;
    }

    for c in &managed {
        logger
            .log(
                "info",
                "newapi_test",
                "newapi_api",
                format!("正在通过 New API 测试渠道 ID {}", c.channel_id),
                Some(settings.alias_model.clone()),
            )
            .await;
        if let Err(error) = client.test_channel(c.channel_id, &settings.alias_model).await {
            logger.error("newapi_test", &error).await;
            rollback(&client, &settings, &openrouter_key, &previous, logger).await;
            return Err(error);
        }
        logger
            .log(
                "success",
                "newapi_test",
                "newapi_api",
                format!("渠道 ID {} 测试通过", c.channel_id),
                None,
            )
            .await;
    }

    if let Err(error) = maybe_e2e(state, &client, &settings, Some(logger)).await {
        rollback(&client, &settings, &openrouter_key, &previous, logger).await;
        return Err(error);
    }

    for (c, new_model) in managed.iter().zip(scan.selected.iter()) {
        let updated = ManagedChannel {
            model_id: new_model.id.clone(),
            updated_at: Utc::now().to_rfc3339(),
            ..c.clone()
        };
        state.db.upsert_managed_channel(&updated).await?;
    }
    Ok((true, selected_ids, "success".into()))
}

async fn rollback(
    client: &NewApiClient,
    settings: &crate::models::AppSettings,
    key: &str,
    previous: &[ManagedChannel],
    logger: &SyncLogger,
) {
    logger
        .log(
            "warning",
            "rollback",
            "safety",
            "开始恢复同步前的模型映射",
            None,
        )
        .await;
    let mut failures = Vec::new();
    for c in previous {
        match client.update_model(settings, c, key, &c.model_id).await {
            Ok(_) => {
                logger
                    .log(
                        "success",
                        "rollback",
                        "safety",
                        format!("渠道 ID {} 已恢复", c.channel_id),
                        Some(c.model_id.clone()),
                    )
                    .await;
            }
            Err(e) => {
                failures.push(format!("{}: {}", c.channel_id, e));
                logger
                    .log(
                        "error",
                        "rollback",
                        "rollback_failure",
                        format!("渠道 ID {} 回滚失败", c.channel_id),
                        Some(e.to_string()),
                    )
                    .await;
            }
        }
    }
    if failures.is_empty() {
        logger
            .log(
                "success",
                "rollback",
                "safety",
                "回滚完成",
                None,
            )
            .await;
    }
}

async fn maybe_e2e(
    state: &AppState,
    client: &NewApiClient,
    settings: &crate::models::AppSettings,
    logger: Option<&SyncLogger>,
) -> Result<(), AppError> {
    if !settings.e2e_test_enabled {
        if let Some(logger) = logger {
            logger
                .log(
                    "info",
                    "e2e",
                    "configuration",
                    "端到端测试未启用，跳过",
                    None,
                )
                .await;
        }
        return Ok(());
    }
    let token = match secret(state, "newapi_test_token").await {
        Ok(v) => v,
        Err(e) => {
            if let Some(logger) = logger {
                logger.error("e2e", &e).await;
            }
            return Err(e);
        }
    };
    if let Some(logger) = logger {
        logger
            .log(
                "info",
                "e2e",
                "newapi_api",
                "正在通过统一别名执行 New API 端到端测试",
                Some(settings.alias_model.clone()),
            )
            .await;
    }
    match client.e2e_test(&token, &settings.alias_model).await {
        Ok(_) => {
            if let Some(logger) = logger {
                logger
                    .log(
                        "success",
                        "e2e",
                        "newapi_api",
                        "端到端测试通过",
                        None,
                    )
                    .await;
            }
            Ok(())
        }
        Err(e) => {
            if let Some(logger) = logger {
                logger.error("e2e", &e).await;
            }
            Err(e)
        }
    }
}

use std::collections::HashSet;

use chrono::Utc;
use reqwest::{
    header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE},
    Client,
};
use serde_json::{json, Value};

use crate::{
    error::AppError,
    models::{AppSettings, ManagedChannel, RankedModel, RoutingChannel, RoutingPoolStatus},
};

const OWNER_ID: &str = "ashan-openrouter-manager-v3";

fn newapi_auto_ban(value: bool) -> i32 {
    if value { 1 } else { 0 }
}

#[derive(Clone)]
pub struct NewApiClient {
    http: Client,
    base: String,
    token: String,
    user_id: String,
}

impl NewApiClient {
    pub fn new(http: Client, settings: &AppSettings, token: String) -> Result<Self, AppError> {
        if settings.newapi_base_url.trim().is_empty() {
            return Err(AppError::bad("New API 地址为空"));
        }
        if settings.newapi_admin_user_id.trim().is_empty() {
            return Err(AppError::bad("New API 管理员用户 ID 为空"));
        }
        Ok(Self {
            http,
            base: settings.newapi_base_url.trim_end_matches('/').to_string(),
            token,
            user_id: settings.newapi_admin_user_id.clone(),
        })
    }

    fn headers(&self) -> Result<HeaderMap, AppError> {
        let mut h = HeaderMap::new();
        h.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.token))
                .map_err(|e| anyhow::anyhow!(e))?,
        );
        h.insert(
            "new-api-user",
            HeaderValue::from_str(&self.user_id).map_err(|e| anyhow::anyhow!(e))?,
        );
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(h)
    }

    async fn json_request(
        &self,
        request: reqwest::RequestBuilder,
        context: &str,
    ) -> Result<Value, AppError> {
        let response = request.headers(self.headers()?).send().await?;
        let status = response.status();
        let text = response.text().await?;
        let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw":text}));

        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!(
                "New API 权限不足或管理员 Token/User ID 无效：{context}: HTTP {status}: {value}"
            )));
        }
        if !status.is_success() {
            return Err(AppError::bad(format!(
                "New API 请求失败：{context}: HTTP {status}: {value}"
            )));
        }
        if value.get("success").and_then(|x| x.as_bool()) == Some(false) {
            let message = value
                .get("message")
                .map(|v| v.to_string())
                .unwrap_or_else(|| value.to_string());
            if message.contains("cannot unmarshal") || message.contains("Go struct field") {
                return Err(AppError::bad(format!(
                    "New API API Schema 兼容性错误：{context}: {message}"
                )));
            }
            return Err(AppError::bad(format!(
                "New API 返回失败：{context}: {message}"
            )));
        }
        Ok(value)
    }

    pub async fn test_connection(&self) -> Result<(), AppError> {
        self.list_channels().await.map(|_| ())
    }

    pub async fn list_channels(&self) -> Result<Vec<Value>, AppError> {
        let mut all = Vec::new();
        for page in 1..=20 {
            let value = self
                .json_request(
                    self.http
                        .get(format!("{}/api/channel/", self.base))
                        .query(&[
                            ("p", page.to_string()),
                            ("page_size", "500".into()),
                            ("id_sort", "true".into()),
                            ("tag_mode", "false".into()),
                            ("status", "all".into()),
                        ]),
                    "读取渠道列表",
                )
                .await?;
            let rows = unwrap_rows(&value);
            let count = rows.len();
            all.extend(rows);
            let total = value
                .pointer("/data/total")
                .or_else(|| value.get("total"))
                .and_then(|v| v.as_u64())
                .unwrap_or(all.len() as u64) as usize;
            if count == 0 || count < 500 || all.len() >= total {
                break;
            }
        }
        Ok(all)
    }

    pub async fn get_channel(&self, id: i64) -> Result<Value, AppError> {
        let value = self
            .json_request(
                self.http.get(format!("{}/api/channel/{id}", self.base)),
                &format!("读取渠道 {id}"),
            )
            .await?;
        Ok(value
            .get("data")
            .or_else(|| value.get("channel"))
            .cloned()
            .unwrap_or(value))
    }

    /// Build a read-only view of every New API channel related to the public alias.
    ///
    /// Ownership is intentionally NOT inferred from the public alias. Manual channels are
    /// allowed to expose the same alias and remain completely read-only. A hard safety block
    /// is reserved for channels that explicitly claim AOM ownership but are not registered in
    /// the local managed_channels table.
    pub async fn inspect_routing_pool(
        &self,
        settings: &AppSettings,
        managed: &[ManagedChannel],
    ) -> Result<RoutingPoolStatus, AppError> {
        let channels = self.list_channels().await?;
        let total_channels = channels.len();
        Ok(classify_routing_pool(settings, managed, channels, total_channels))
    }

    pub async fn assert_owned(
        &self,
        settings: &AppSettings,
        registered: &ManagedChannel,
    ) -> Result<Value, AppError> {
        let live = self.get_channel(registered.channel_id).await?;
        let mapping = parse_mapping(&live);
        let models = channel_models(&live);
        let ok = live.get("id").and_then(|v| v.as_i64()) == Some(registered.channel_id)
            && str_field(&live, "name") == registered.name
            && live.get("type").and_then(|v| v.as_i64()) == Some(settings.channel_type)
            && str_field(&live, "base_url").trim_end_matches('/')
                == settings.openrouter_upstream_base.trim_end_matches('/')
            && channel_groups(&live)
                .iter()
                .any(|g| g == &settings.managed_group)
            && models.len() == 1
            && models[0] == settings.alias_model
            && mapping
                .get(&settings.alias_model)
                .map(|v| v == &registered.model_id)
                .unwrap_or(false)
            && str_field(&live, "tag") == settings.managed_tag
            && str_field(&live, "remark").contains(OWNER_ID)
            && str_field(&live, "remark").contains(&format!("rank={}", registered.rank));
        if !ok {
            return Err(AppError::conflict(format!(
                "渠道 {} 所有权校验失败；程序拒绝修改该渠道",
                registered.channel_id
            )));
        }
        Ok(live)
    }

    pub async fn create_channel(
        &self,
        settings: &AppSettings,
        key: &str,
        model: &RankedModel,
        rank: i64,
    ) -> Result<ManagedChannel, AppError> {
        let name = format!("{} R{}", settings.channel_name_prefix, rank);
        let priority = settings.priority_base - (rank - 1) * settings.priority_step;
        let groups = desired_channel_groups(settings);
        let group = groups.join(",");
        let headers = desired_channel_headers(settings);
        let mut channel = json!({
            "name": name,
            "type": settings.channel_type,
            "key": key,
            "base_url": settings.openrouter_upstream_base.trim_end_matches('/'),
            "models": settings.alias_model,
            "groups": groups,
            "group": group,
            "priority": priority,
            "weight": settings.channel_weight,
            "status": settings.disabled_status,
            "auto_ban": newapi_auto_ban(settings.auto_ban),
            "tag": settings.managed_tag,
            "model_mapping": mapping_string(&settings.alias_model, &model.id),
            "remark": format!("{};rank={}", OWNER_ID, rank),
        });
        if let Some(h) = headers {
            channel["headers"] = json!(h);
        }
        self.json_request(
            self.http
                .post(format!("{}/api/channel/", self.base))
                .json(&json!({"mode":"single","channel":channel})),
            "创建受管渠道",
        )
        .await?;
        let created = self.find_exact_name(&name).await?;
        let id = created
            .get("id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| AppError::bad("New API 创建渠道后未返回 id"))?;
        self.set_status(id, settings.disabled_status).await?;
        let managed = ManagedChannel {
            rank,
            channel_id: id,
            name,
            model_id: model.id.clone(),
            priority,
            updated_at: Utc::now().to_rfc3339(),
        };
        self.assert_owned_allow_status(settings, &managed).await?;
        Ok(managed)
    }

    async fn assert_owned_allow_status(
        &self,
        settings: &AppSettings,
        registered: &ManagedChannel,
    ) -> Result<(), AppError> {
        let live = self.get_channel(registered.channel_id).await?;
        let mapping = parse_mapping(&live);
        let models = channel_models(&live);
        let ok = str_field(&live, "name") == registered.name
            && live.get("type").and_then(|v| v.as_i64()) == Some(settings.channel_type)
            && channel_groups(&live)
                .iter()
                .any(|g| g == &settings.managed_group)
            && models.len() == 1
            && models[0] == settings.alias_model
            && mapping
                .get(&settings.alias_model)
                .map(|v| v == &registered.model_id)
                .unwrap_or(false)
            && str_field(&live, "tag") == settings.managed_tag
            && str_field(&live, "remark").contains(OWNER_ID);
        if !ok {
            return Err(AppError::conflict(format!(
                "新建渠道 {} 所有权校验失败",
                registered.channel_id
            )));
        }
        Ok(())
    }

    pub async fn assert_identity(
        &self,
        settings: &AppSettings,
        registered: &ManagedChannel,
    ) -> Result<Value, AppError> {
        let live = self.get_channel(registered.channel_id).await?;
        let models = channel_models(&live);
        let ok = live.get("id").and_then(|v| v.as_i64()) == Some(registered.channel_id)
            && str_field(&live, "name") == registered.name
            && live.get("type").and_then(|v| v.as_i64()) == Some(settings.channel_type)
            && str_field(&live, "base_url").trim_end_matches('/')
                == settings.openrouter_upstream_base.trim_end_matches('/')
            && channel_groups(&live)
                .iter()
                .any(|g| g == &settings.managed_group)
            && models.len() == 1
            && models[0] == settings.alias_model
            && str_field(&live, "tag") == settings.managed_tag
            && str_field(&live, "remark").contains(OWNER_ID)
            && str_field(&live, "remark").contains(&format!("rank={}", registered.rank));
        if !ok {
            return Err(AppError::conflict(format!(
                "渠道 {} 身份校验失败",
                registered.channel_id
            )));
        }
        Ok(live)
    }

    pub async fn assert_model(
        &self,
        settings: &AppSettings,
        registered: &ManagedChannel,
        expected_model: &str,
    ) -> Result<(), AppError> {
        let live = self.assert_identity(settings, registered).await?;
        let mapping = parse_mapping(&live);
        if mapping
            .get(&settings.alias_model)
            .map(|v| v.as_str())
            != Some(expected_model)
        {
            return Err(AppError::conflict(format!(
                "渠道 {} 模型映射验证失败",
                registered.channel_id
            )));
        }
        Ok(())
    }

    pub async fn update_model(
        &self,
        settings: &AppSettings,
        registered: &ManagedChannel,
        key: &str,
        new_model: &str,
    ) -> Result<(), AppError> {
        self.assert_identity(settings, registered).await?;
        let groups = desired_channel_groups(settings);
        let group = groups.join(",");
        let headers = desired_channel_headers(settings);
        let mut patch = json!({
            "id": registered.channel_id,
            "key": key,
            "models": settings.alias_model,
            "model_mapping": mapping_string(&settings.alias_model,new_model),
            "priority": registered.priority,
            "weight": settings.channel_weight,
            "auto_ban": newapi_auto_ban(settings.auto_ban),
            "tag": settings.managed_tag,
            "group": group,
            "groups": groups,
            "remark": format!("{};rank={}",OWNER_ID,registered.rank),
        });
        if let Some(h) = headers {
            patch["headers"] = json!(h);
        }
        self.json_request(
            self.http
                .put(format!("{}/api/channel/", self.base))
                .json(&patch),
            &format!("更新渠道 {}", registered.channel_id),
        )
        .await?;
        self.assert_model(settings, registered, new_model).await?;
        Ok(())
    }

    /// Keep the AOM ownership group and the actual request-routing groups separate.
    /// Existing v3.0.9 channels only had `managed_group`, which meant a `default`
    /// token could not see them. This reconciliation is intentionally independent
    /// from model changes so an already-correct Top 3 is migrated on the next sync.
    pub async fn ensure_routing_groups(
        &self,
        settings: &AppSettings,
        registered: &ManagedChannel,
        key: &str,
    ) -> Result<bool, AppError> {
        let live = self.assert_identity(settings, registered).await?;
        let current = normalized_groups(channel_groups(&live));
        let desired = desired_channel_groups(settings);
        if current == desired {
            return Ok(false);
        }

        // Reuse the fully-specified managed-channel update path instead of relying
        // on partial-PUT semantics. The model mapping is intentionally unchanged.
        self.update_model(settings, registered, key, &registered.model_id)
            .await?;

        let updated = self.assert_identity(settings, registered).await?;
        let actual = normalized_groups(channel_groups(&updated));
        let expected = desired_channel_groups(settings);
        if actual != expected {
            return Err(AppError::conflict(format!(
                "渠道 {} 路由分组验证失败；期望 {}，实际 {}",
                registered.channel_id,
                expected.join(","),
                actual.join(",")
            )));
        }
        Ok(true)
    }

    pub async fn delete_exact(
        &self,
        settings: &AppSettings,
        channels: &[ManagedChannel],
    ) -> Result<(), AppError> {
        if channels.is_empty() {
            return Ok(());
        }
        let mut ids = Vec::new();
        for c in channels {
            self.assert_identity(settings, c).await?;
            ids.push(c.channel_id);
        }
        self.json_request(
            self.http
                .post(format!("{}/api/channel/batch", self.base))
                .json(&json!({"ids":ids})),
            "删除本次初始化创建的渠道",
        )
        .await?;
        Ok(())
    }

    pub async fn test_channel(&self, id: i64, model: &str) -> Result<(), AppError> {
        self.json_request(
            self.http
                .get(format!("{}/api/channel/test/{id}", self.base))
                .query(&[("model", model)]),
            &format!("测试渠道 {id}"),
        )
        .await?;
        Ok(())
    }

    pub async fn set_status(&self, id: i64, status: i64) -> Result<(), AppError> {
        let direct = self
            .json_request(
                self.http
                    .post(format!("{}/api/channel/{id}/status", self.base))
                    .json(&json!({"status":status})),
                &format!("设置渠道 {id} 状态"),
            )
            .await;
        if direct.is_ok() {
            return Ok(());
        }
        self.json_request(
            self.http
                .put(format!("{}/api/channel/", self.base))
                .json(&json!({"id":id,"status":status})),
            &format!("设置渠道 {id} 状态（兼容模式）"),
        )
        .await?;
        Ok(())
    }

    pub async fn e2e_test(&self, user_token: &str, alias: &str) -> Result<(), AppError> {
        let response = self
            .http
            .post(format!("{}/v1/chat/completions", self.base))
            .bearer_auth(user_token)
            .json(&json!({"model":alias,"messages":[{"role":"user","content":"Reply with OK only."}],"max_tokens":4,"temperature":0}))
            .send()
            .await?;
        let status = response.status();
        let value: Value = response.json().await.unwrap_or(json!({}));
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!(
                "New API 端到端测试权限失败: HTTP {status}: {value}"
            )));
        }
        if !status.is_success()
            || value
                .get("choices")
                .and_then(|v| v.as_array())
                .map(|v| v.is_empty())
                .unwrap_or(true)
        {
            return Err(AppError::bad(format!(
                "New API 端到端测试失败: {status} {value}"
            )));
        }
        Ok(())
    }

    async fn find_exact_name(&self, name: &str) -> Result<Value, AppError> {
        for _ in 0..8 {
            let matches: Vec<Value> = self
                .list_channels()
                .await?
                .into_iter()
                .filter(|c| str_field(c, "name") == name)
                .collect();
            if matches.len() == 1 {
                return Ok(matches[0].clone());
            }
            if matches.len() > 1 {
                return Err(AppError::conflict(format!(
                    "存在多个同名渠道 {name}"
                )));
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(AppError::bad(format!("创建后未找到渠道 {name}")))
    }
}

fn classify_routing_pool(
    settings: &AppSettings,
    managed: &[ManagedChannel],
    channels: Vec<Value>,
    total_channels: usize,
) -> RoutingPoolStatus {
    let managed_ids: HashSet<i64> = managed.iter().map(|c| c.channel_id).collect();
    let mut manual_channels = Vec::new();
    let mut managed_channels = Vec::new();
    let mut orphan_channels = Vec::new();
    let mut related_channels = Vec::new();

    for channel in channels {
        let id = channel.get("id").and_then(|v| v.as_i64()).unwrap_or_default();
        let name = str_field(&channel, "name");
        let status = channel.get("status").and_then(|v| v.as_i64()).unwrap_or_default();
        let priority = channel.get("priority").and_then(|v| v.as_i64()).unwrap_or_default();
        let weight = channel.get("weight").and_then(|v| v.as_i64()).unwrap_or_default();
        let group = str_field(&channel, "group");
        let tag = str_field(&channel, "tag");
        let remark = str_field(&channel, "remark");
        let models = channel_models(&channel);
        let mapping = parse_mapping(&channel);
        let mapping_target = mapping.get(&settings.alias_model).cloned();
        let serves_alias = models.iter().any(|m| m == &settings.alias_model) || mapping_target.is_some();
        let explicit_owner = remark.contains(OWNER_ID);
        let exact_manager_name = (1..=3).any(|rank| name == format!("{} R{}", settings.channel_name_prefix, rank));
        let manager_identity = explicit_owner
            || exact_manager_name
            || (tag == settings.managed_tag && name.starts_with(&settings.channel_name_prefix));
        let is_managed = managed_ids.contains(&id);

        let mut entry = RoutingChannel {
            id,
            name: name.clone(),
            status,
            priority,
            weight,
            group: group.clone(),
            tag: tag.clone(),
            models: models.clone(),
            mapping_target,
            classification: String::new(),
            reason: String::new(),
        };

        if is_managed {
            entry.classification = "managed".into();
            entry.reason = "Channel ID 已登记在本地 managed_channels；AOM 可对该渠道执行精确校验与更新".into();
            managed_channels.push(entry);
            continue;
        }

        if manager_identity {
            entry.classification = "orphan".into();
            entry.reason = if explicit_owner {
                "remark 明确包含 AOM OWNER_ID，但本地数据库没有登记该 Channel ID".into()
            } else if exact_manager_name {
                "渠道名称与 AOM 固定槽位完全相同，但本地数据库没有登记该 Channel ID".into()
            } else {
                "渠道同时使用 AOM 标签与名称前缀，但本地数据库没有登记该 Channel ID".into()
            };
            orphan_channels.push(entry);
            continue;
        }

        if serves_alias {
            entry.classification = "manual".into();
            entry.reason = format!(
                "手动/外部渠道合法提供统一别名 {}；AOM 只读，不修改、不删除、不调整优先级或权重",
                settings.alias_model
            );
            manual_channels.push(entry);
            continue;
        }

        let same_group = group == settings.managed_group
            || channel_groups(&channel).iter().any(|g| g == &settings.managed_group);
        let same_tag = tag == settings.managed_tag;
        let same_prefix = name.starts_with(&settings.channel_name_prefix);
        if same_group || same_tag || same_prefix {
            let mut reasons = Vec::new();
            if same_group { reasons.push(format!("相同业务分组 {}", settings.managed_group)); }
            if same_tag { reasons.push(format!("相同标签 {}", settings.managed_tag)); }
            if same_prefix { reasons.push(format!("相同名称前缀 {}", settings.channel_name_prefix)); }
            entry.classification = "related".into();
            entry.reason = format!("相关但不提供统一别名：{}；仅用于诊断，不参与所有权判定", reasons.join("；"));
            related_channels.push(entry);
        }
    }

    manual_channels.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    managed_channels.sort_by(|a, b| b.priority.cmp(&a.priority).then_with(|| a.id.cmp(&b.id)));
    orphan_channels.sort_by_key(|c| c.id);
    related_channels.sort_by_key(|c| c.id);

    let manual_enabled = manual_channels
        .iter()
        .filter(|c| c.status == settings.enabled_status)
        .count();
    let managed_enabled = managed_channels
        .iter()
        .filter(|c| c.status == settings.enabled_status)
        .count();
    let highest_manual_priority = manual_channels
        .iter()
        .filter(|c| c.status == settings.enabled_status)
        .map(|c| c.priority)
        .max();
    let highest_managed_priority = managed_channels
        .iter()
        .filter(|c| c.status == settings.enabled_status)
        .map(|c| c.priority)
        .max();
    let required_routing_groups = configured_routing_groups(settings);
    let managed_group_mismatch = !managed_channels.is_empty()
        && managed_channels.iter().any(|channel| {
            let actual = normalized_groups(vec![channel.group.clone()]);
            required_routing_groups
                .iter()
                .any(|required| !actual.iter().any(|group| group == required))
        });
    let route_mode = if managed_group_mismatch {
        "managed_group_mismatch".to_string()
    } else {
        match (highest_manual_priority, highest_managed_priority) {
            (Some(m), Some(a)) if m > a => "manual_first",
            (Some(m), Some(a)) if a > m => "managed_first",
            (Some(_), Some(_)) => "mixed_same_priority",
            (Some(_), None) => "manual_only",
            (None, Some(_)) => "managed_only",
            (None, None) => "no_enabled_channels",
        }
        .to_string()
    };
    let message = match route_mode.as_str() {
        "managed_group_mismatch" => "AOM 受管渠道尚未加入全部实际请求分组；下一次同步会先补齐分组，再判断 Top 3 是否需要更新",
        "manual_first" => "当前最高优先级来自手动渠道；AOM 自动池作为较低优先级补充/备用",
        "managed_first" => "当前最高优先级来自 AOM 自动池；手动渠道仍保留并可作为较低优先级补充/备用",
        "mixed_same_priority" => "手动池与 AOM 自动池存在相同最高优先级；New API 将在同优先级渠道中结合 weight 进行分配",
        "manual_only" => "当前仅手动渠道池可用；AOM 自动池尚未初始化或未启用",
        "managed_only" => "当前仅 AOM 自动池可用；未检测到启用的同别名手动渠道",
        _ => "当前没有检测到启用的同别名渠道",
    }
    .to_string();

    RoutingPoolStatus {
        available: true,
        alias_model: settings.alias_model.clone(),
        total_channels,
        manual_enabled,
        managed_enabled,
        manual_channels,
        managed_channels,
        orphan_channels,
        related_channels,
        highest_manual_priority,
        highest_managed_priority,
        route_mode,
        message,
        error: None,
    }
}

fn mapping_string(alias: &str, actual: &str) -> String {
    let mut map = std::collections::HashMap::new();
    map.insert(alias, actual);
    serde_json::to_string(&map).unwrap()
}

fn str_field(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(|x| x.as_str())
        .unwrap_or_default()
        .to_string()
}

fn channel_models(v: &Value) -> Vec<String> {
    match v.get("models") {
        Some(Value::String(s)) => s
            .split(',')
            .map(|x| x.trim().to_string())
            .filter(|x| !x.is_empty())
            .collect(),
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        _ => vec![],
    }
}

fn channel_groups(v: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(g) = v.get("group").and_then(|x| x.as_str()) {
        out.extend(g.split(',').map(|x| x.trim().to_string()));
    }
    if let Some(a) = v.get("groups").and_then(|x| x.as_array()) {
        out.extend(a.iter().filter_map(|x| x.as_str().map(str::to_string)));
    }
    out.sort();
    out.dedup();
    out
}

fn normalized_groups(groups: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for raw in groups {
        for item in raw.split(',') {
            let item = item.trim();
            if !item.is_empty() && !out.iter().any(|existing| existing == item) {
                out.push(item.to_string());
            }
        }
    }
    out.sort();
    out
}

fn configured_routing_groups(settings: &AppSettings) -> Vec<String> {
    let groups = normalized_groups(settings.routing_groups.clone());
    if groups.is_empty() {
        vec!["default".into()]
    } else {
        groups
    }
}

fn desired_channel_groups(settings: &AppSettings) -> Vec<String> {
    let mut groups = vec![settings.managed_group.clone()];
    groups.extend(configured_routing_groups(settings));
    normalized_groups(groups)
}

pub fn desired_channel_headers(settings: &AppSettings) -> Option<String> {
    let mut map = serde_json::Map::new();
    if !settings.openrouter_http_referer.trim().is_empty() {
        map.insert(
            "HTTP-Referer".into(),
            json!(settings.openrouter_http_referer.trim()),
        );
    }
    if !settings.openrouter_x_title.trim().is_empty() {
        map.insert("X-Title".into(), json!(settings.openrouter_x_title.trim()));
    }
    if !settings.openrouter_user_agent.trim().is_empty() {
        map.insert(
            "User-Agent".into(),
            json!(settings.openrouter_user_agent.trim()),
        );
    }
    if map.is_empty() {
        None
    } else {
        Some(Value::Object(map).to_string())
    }
}

fn parse_mapping(v: &Value) -> std::collections::HashMap<String, String> {
    let raw = v.get("model_mapping");
    match raw {
        Some(Value::Object(m)) => m
            .iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
            .collect(),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_default(),
        _ => Default::default(),
    }
}

fn unwrap_rows(v: &Value) -> Vec<Value> {
    for ptr in ["/data/items", "/data/data", "/data", "/items", "/channels"] {
        if let Some(a) = v.pointer(ptr).and_then(|x| x.as_array()) {
            return a.clone();
        }
    }
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_alias_channel_is_allowed_and_read_only() {
        let settings = AppSettings::default();
        let channels = vec![json!({
            "id": 49,
            "name": "My Manual Tunnel",
            "status": settings.enabled_status,
            "priority": 20000,
            "weight": 100,
            "group": "default",
            "tag": "manual",
            "remark": "user managed",
            "models": settings.alias_model.clone(),
            "model_mapping": "{}",
        })];

        let report = classify_routing_pool(&settings, &[], channels, 1);
        assert_eq!(report.manual_channels.len(), 1);
        assert!(report.orphan_channels.is_empty());
        assert_eq!(report.route_mode, "manual_only");
    }

    #[test]
    fn auto_ban_is_encoded_as_newapi_integer() {
        assert_eq!(newapi_auto_ban(true), 1);
        assert_eq!(newapi_auto_ban(false), 0);
    }

    #[test]
    fn desired_groups_keep_owner_group_and_add_default_routing_group() {
        let mut settings = AppSettings::default();
        settings.managed_group = "ashan-openrouter-free".into();
        settings.routing_groups = vec!["default".into()];
        assert_eq!(
            desired_channel_groups(&settings),
            vec!["ashan-openrouter-free".to_string(), "default".to_string()]
        );
    }

    #[test]
    fn empty_routing_groups_still_use_default() {
        let mut settings = AppSettings::default();
        settings.managed_group = "ashan-openrouter-free".into();
        settings.routing_groups = vec![];
        assert_eq!(
            desired_channel_groups(&settings),
            vec!["ashan-openrouter-free".to_string(), "default".to_string()]
        );
    }

    #[test]
    fn desired_groups_are_trimmed_and_deduplicated() {
        let mut settings = AppSettings::default();
        settings.managed_group = "ashan-openrouter-free".into();
        settings.routing_groups = vec![
            "default".into(),
            " default,ashan-openrouter-free ".into(),
        ];
        assert_eq!(
            desired_channel_groups(&settings),
            vec!["ashan-openrouter-free".to_string(), "default".to_string()]
        );
    }

    #[test]
    fn unregistered_explicit_aom_identity_is_orphaned() {
        let settings = AppSettings::default();
        let channels = vec![json!({
            "id": 88,
            "name": format!("{} R1", settings.channel_name_prefix),
            "status": settings.enabled_status,
            "priority": settings.priority_base,
            "weight": settings.channel_weight,
            "group": settings.managed_group.clone(),
            "tag": settings.managed_tag.clone(),
            "remark": format!("{};rank=1", OWNER_ID),
            "models": settings.alias_model.clone(),
            "model_mapping": "{}",
        })];

        let report = classify_routing_pool(&settings, &[], channels, 1);
        assert!(report.manual_channels.is_empty());
        assert_eq!(report.orphan_channels.len(), 1);
    }

    #[test]
    fn desired_channel_headers_builds_valid_json() {
        let settings = AppSettings::default();
        let headers_str = desired_channel_headers(&settings).expect("headers should exist");
        let parsed: Value = serde_json::from_str(&headers_str).expect("valid json");
        assert_eq!(
            parsed.get("HTTP-Referer").and_then(|v| v.as_str()),
            Some("https://github.com/NousResearch/hermes-agent")
        );
        assert_eq!(
            parsed.get("X-Title").and_then(|v| v.as_str()),
            Some("Hermes Agent")
        );
        assert_eq!(
            parsed.get("User-Agent").and_then(|v| v.as_str()),
            Some("HermesAgent/0.1.0 (NousResearch; +https://github.com/NousResearch/hermes-agent)")
        );
    }

    #[test]
    fn empty_channel_headers_returns_none() {
        let mut settings = AppSettings::default();
        settings.openrouter_http_referer = "   ".into();
        settings.openrouter_x_title = "".into();
        settings.openrouter_user_agent = "".into();
        assert_eq!(desired_channel_headers(&settings), None);
    }
}


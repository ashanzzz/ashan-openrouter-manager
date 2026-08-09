use std::collections::HashSet;

use chrono::Utc;
use reqwest::{header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE}, Client};
use serde_json::{json, Value};

use crate::{error::AppError, models::{AppSettings, ManagedChannel, RankedModel}};

const OWNER_ID: &str = "ashan-openrouter-manager-v3";

#[derive(Clone)]
pub struct NewApiClient { http: Client, base: String, token: String, user_id: String }

impl NewApiClient {
    pub fn new(http: Client, settings: &AppSettings, token: String) -> Result<Self, AppError> {
        if settings.newapi_base_url.trim().is_empty() { return Err(AppError::bad("New API base URL is empty")); }
        Ok(Self { http, base: settings.newapi_base_url.trim_end_matches('/').to_string(), token, user_id: settings.newapi_admin_user_id.clone() })
    }

    fn headers(&self) -> Result<HeaderMap, AppError> {
        let mut h = HeaderMap::new();
        h.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", self.token)).map_err(|e| anyhow::anyhow!(e))?);
        h.insert("new-api-user", HeaderValue::from_str(&self.user_id).map_err(|e| anyhow::anyhow!(e))?);
        h.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        Ok(h)
    }

    async fn json_request(&self, request: reqwest::RequestBuilder, context: &str) -> Result<Value, AppError> {
        let response = request.headers(self.headers()?).send().await?;
        let status = response.status();
        let text = response.text().await?;
        let value: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({"raw":text}));
        if !status.is_success() { return Err(AppError::bad(format!("{context}: HTTP {status}: {value}"))); }
        if value.get("success").and_then(|x| x.as_bool()) == Some(false) {
            return Err(AppError::bad(format!("{context}: {}", value.get("message").unwrap_or(&value))));
        }
        Ok(value)
    }

    pub async fn test_connection(&self) -> Result<(), AppError> { self.list_channels().await.map(|_| ()) }

    pub async fn list_channels(&self) -> Result<Vec<Value>, AppError> {
        let mut all = Vec::new();
        for page in 1..=20 {
            let value = self.json_request(self.http.get(format!("{}/api/channel/",self.base)).query(&[
                ("p",page.to_string()),("page_size","500".into()),("id_sort","true".into()),("tag_mode","false".into()),("status","all".into())
            ]), "List New API channels").await?;
            let rows = unwrap_rows(&value);
            let count = rows.len();
            all.extend(rows);
            let total = value.pointer("/data/total").or_else(|| value.get("total")).and_then(|v| v.as_u64()).unwrap_or(all.len() as u64) as usize;
            if count == 0 || count < 500 || all.len() >= total { break; }
        }
        Ok(all)
    }

    pub async fn get_channel(&self, id: i64) -> Result<Value, AppError> {
        let value = self.json_request(self.http.get(format!("{}/api/channel/{id}",self.base)), &format!("Get channel {id}")).await?;
        Ok(value.get("data").or_else(|| value.get("channel")).cloned().unwrap_or(value))
    }

    pub async fn assert_no_foreign_conflicts(&self, settings: &AppSettings, managed: &[ManagedChannel]) -> Result<(), AppError> {
        let ids: HashSet<i64> = managed.iter().map(|c| c.channel_id).collect();
        let mut conflicts = Vec::new();
        for channel in self.list_channels().await? {
            let id = channel.get("id").and_then(|v| v.as_i64()).unwrap_or_default();
            if ids.contains(&id) { continue; }
            let name = str_field(&channel,"name");
            let tag = str_field(&channel,"tag");
            let group = str_field(&channel,"group");
            let models = channel_models(&channel);
            let mapping = parse_mapping(&channel);
            if models.iter().any(|m| m == &settings.alias_model)
                || mapping.get(&settings.alias_model).is_some()
                || tag == settings.managed_tag
                || group == settings.managed_group
                || name.starts_with(&settings.channel_name_prefix) {
                conflicts.push(id);
            }
        }
        if !conflicts.is_empty() { return Err(AppError::conflict(format!("foreign New API channels conflict with managed identity: {conflicts:?}"))); }
        Ok(())
    }

    pub async fn assert_owned(&self, settings: &AppSettings, registered: &ManagedChannel) -> Result<Value, AppError> {
        let live = self.get_channel(registered.channel_id).await?;
        let mapping = parse_mapping(&live);
        let models = channel_models(&live);
        let ok = live.get("id").and_then(|v| v.as_i64()) == Some(registered.channel_id)
            && str_field(&live,"name") == registered.name
            && live.get("type").and_then(|v| v.as_i64()) == Some(settings.channel_type)
            && str_field(&live,"base_url").trim_end_matches('/') == settings.openrouter_upstream_base.trim_end_matches('/')
            && channel_groups(&live).iter().any(|g| g == &settings.managed_group)
            && models.len() == 1 && models[0] == settings.alias_model
            && mapping.get(&settings.alias_model).map(|v| v == &registered.model_id).unwrap_or(false)
            && str_field(&live,"tag") == settings.managed_tag
            && str_field(&live,"remark").contains(OWNER_ID)
            && str_field(&live,"remark").contains(&format!("rank={}",registered.rank));
        if !ok { return Err(AppError::conflict(format!("channel {} failed ownership verification", registered.channel_id))); }
        Ok(live)
    }

    pub async fn create_channel(&self, settings: &AppSettings, key: &str, model: &RankedModel, rank: i64) -> Result<ManagedChannel, AppError> {
        let name = format!("{} R{}", settings.channel_name_prefix, rank);
        let priority = settings.priority_base - (rank - 1) * settings.priority_step;
        let channel = json!({
            "name": name,
            "type": settings.channel_type,
            "key": key,
            "base_url": settings.openrouter_upstream_base.trim_end_matches('/'),
            "models": settings.alias_model,
            "groups": [settings.managed_group],
            "group": settings.managed_group,
            "priority": priority,
            "weight": settings.channel_weight,
            "status": settings.disabled_status,
            "auto_ban": settings.auto_ban,
            "tag": settings.managed_tag,
            "model_mapping": mapping_string(&settings.alias_model, &model.id),
            "remark": format!("{};rank={}", OWNER_ID, rank),
        });
        self.json_request(self.http.post(format!("{}/api/channel/",self.base)).json(&json!({"mode":"single","channel":channel})), "Create managed channel").await?;
        let created = self.find_exact_name(&name).await?;
        let id = created.get("id").and_then(|v| v.as_i64()).ok_or_else(|| AppError::bad("New API created channel has no id"))?;
        self.set_status(id, settings.disabled_status).await?;
        let managed = ManagedChannel { rank, channel_id:id, name, model_id:model.id.clone(), priority, updated_at:Utc::now().to_rfc3339() };
        self.assert_owned_allow_status(settings,&managed).await?;
        Ok(managed)
    }

    async fn assert_owned_allow_status(&self, settings:&AppSettings, registered:&ManagedChannel) -> Result<(),AppError> {
        let live=self.get_channel(registered.channel_id).await?;
        let mapping=parse_mapping(&live); let models=channel_models(&live);
        let ok=str_field(&live,"name")==registered.name && live.get("type").and_then(|v|v.as_i64())==Some(settings.channel_type)
            && channel_groups(&live).iter().any(|g|g==&settings.managed_group) && models.len()==1 && models[0]==settings.alias_model
            && mapping.get(&settings.alias_model).map(|v|v==&registered.model_id).unwrap_or(false)
            && str_field(&live,"tag")==settings.managed_tag && str_field(&live,"remark").contains(OWNER_ID);
        if !ok { return Err(AppError::conflict(format!("created channel {} failed ownership verification",registered.channel_id))); }
        Ok(())
    }

    pub async fn assert_identity(&self, settings:&AppSettings, registered:&ManagedChannel) -> Result<Value,AppError> {
        let live=self.get_channel(registered.channel_id).await?;
        let models=channel_models(&live);
        let ok=live.get("id").and_then(|v|v.as_i64())==Some(registered.channel_id)
            && str_field(&live,"name")==registered.name
            && live.get("type").and_then(|v|v.as_i64())==Some(settings.channel_type)
            && str_field(&live,"base_url").trim_end_matches('/')==settings.openrouter_upstream_base.trim_end_matches('/')
            && channel_groups(&live).iter().any(|g|g==&settings.managed_group)
            && models.len()==1 && models[0]==settings.alias_model
            && str_field(&live,"tag")==settings.managed_tag
            && str_field(&live,"remark").contains(OWNER_ID)
            && str_field(&live,"remark").contains(&format!("rank={}",registered.rank));
        if !ok{return Err(AppError::conflict(format!("channel {} failed identity verification",registered.channel_id)));}
        Ok(live)
    }

    pub async fn assert_model(&self, settings:&AppSettings, registered:&ManagedChannel, expected_model:&str)->Result<(),AppError>{
        let live=self.assert_identity(settings,registered).await?;
        let mapping=parse_mapping(&live);
        if mapping.get(&settings.alias_model).map(|v|v.as_str())!=Some(expected_model){return Err(AppError::conflict(format!("channel {} mapping verification failed",registered.channel_id)));}
        Ok(())
    }

    pub async fn update_model(&self, settings:&AppSettings, registered:&ManagedChannel, key:&str, new_model:&str) -> Result<(),AppError> {
        self.assert_identity(settings,registered).await?;
        let patch=json!({
            "id": registered.channel_id,
            "key": key,
            "models": settings.alias_model,
            "model_mapping": mapping_string(&settings.alias_model,new_model),
            "priority": registered.priority,
            "weight": settings.channel_weight,
            "tag": settings.managed_tag,
            "group": settings.managed_group,
            "groups": [settings.managed_group],
            "remark": format!("{};rank={}",OWNER_ID,registered.rank),
        });
        self.json_request(self.http.put(format!("{}/api/channel/",self.base)).json(&patch), &format!("Update channel {}",registered.channel_id)).await?;
        self.assert_model(settings,registered,new_model).await?;
        Ok(())
    }

    pub async fn delete_exact(&self, settings:&AppSettings, channels:&[ManagedChannel])->Result<(),AppError>{
        if channels.is_empty(){return Ok(());}
        let mut ids=Vec::new();
        for c in channels{self.assert_identity(settings,c).await?;ids.push(c.channel_id);}
        self.json_request(self.http.post(format!("{}/api/channel/batch",self.base)).json(&json!({"ids":ids})),"Delete newly created managed channels").await?;
        Ok(())
    }

    pub async fn test_channel(&self, id:i64, model:&str) -> Result<(),AppError> {
        self.json_request(self.http.get(format!("{}/api/channel/test/{id}",self.base)).query(&[("model",model)]), &format!("Test channel {id}")).await?;
        Ok(())
    }

    pub async fn set_status(&self,id:i64,status:i64)->Result<(),AppError>{
        let direct=self.json_request(self.http.post(format!("{}/api/channel/{id}/status",self.base)).json(&json!({"status":status})), &format!("Set channel {id} status")).await;
        if direct.is_ok(){return Ok(());}
        self.json_request(self.http.put(format!("{}/api/channel/",self.base)).json(&json!({"id":id,"status":status})), &format!("Set channel {id} status fallback")).await?;
        Ok(())
    }

    pub async fn e2e_test(&self, user_token:&str, alias:&str)->Result<(),AppError>{
        let response=self.http.post(format!("{}/v1/chat/completions",self.base)).bearer_auth(user_token).json(&json!({"model":alias,"messages":[{"role":"user","content":"Reply with OK only."}],"max_tokens":4,"temperature":0})).send().await?;
        let status=response.status(); let value:Value=response.json().await.unwrap_or(json!({}));
        if !status.is_success() || value.get("choices").and_then(|v|v.as_array()).map(|v|v.is_empty()).unwrap_or(true){return Err(AppError::bad(format!("New API end-to-end test failed: {status} {value}")));}
        Ok(())
    }

    async fn find_exact_name(&self,name:&str)->Result<Value,AppError>{
        for _ in 0..8 {
            let matches:Vec<Value>=self.list_channels().await?.into_iter().filter(|c|str_field(c,"name")==name).collect();
            if matches.len()==1{return Ok(matches[0].clone());}
            if matches.len()>1{return Err(AppError::conflict(format!("multiple channels have name {name}")));}
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(AppError::bad(format!("created channel not found by name {name}")))
    }
}

fn mapping_string(alias:&str,actual:&str)->String{
    let mut map=std::collections::HashMap::new();
    map.insert(alias,actual);
    serde_json::to_string(&map).unwrap()
}
fn str_field(v:&Value,key:&str)->String{v.get(key).and_then(|x|x.as_str()).unwrap_or_default().to_string()}
fn channel_models(v:&Value)->Vec<String>{
    match v.get("models") {
        Some(Value::String(s))=>s.split(',').map(|x|x.trim().to_string()).filter(|x|!x.is_empty()).collect(),
        Some(Value::Array(a))=>a.iter().filter_map(|x|x.as_str().map(str::to_string)).collect(),
        _=>vec![]
    }
}
fn channel_groups(v:&Value)->Vec<String>{
    let mut out=Vec::new();
    if let Some(g)=v.get("group").and_then(|x|x.as_str()){out.extend(g.split(',').map(|x|x.trim().to_string()));}
    if let Some(a)=v.get("groups").and_then(|x|x.as_array()){out.extend(a.iter().filter_map(|x|x.as_str().map(str::to_string)));}
    out.sort(); out.dedup(); out
}
fn parse_mapping(v:&Value)->std::collections::HashMap<String,String>{
    let raw=v.get("model_mapping");
    match raw {
        Some(Value::Object(m))=>m.iter().filter_map(|(k,v)|v.as_str().map(|s|(k.clone(),s.to_string()))).collect(),
        Some(Value::String(s))=>serde_json::from_str(s).unwrap_or_default(),
        _=>Default::default()
    }
}
fn unwrap_rows(v:&Value)->Vec<Value>{
    for ptr in ["/data/items","/data/data","/data","/items","/channels"] {
        if let Some(a)=v.pointer(ptr).and_then(|x|x.as_array()){return a.clone();}
    }
    vec![]
}

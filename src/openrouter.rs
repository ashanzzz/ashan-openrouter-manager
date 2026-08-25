use reqwest::{header::USER_AGENT, Client};
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, models::{AppSettings, BenchmarkItem, OpenRouterModel}};

#[derive(Deserialize)] struct ModelsResponse { data: Vec<OpenRouterModel> }
#[derive(Deserialize)] struct BenchmarksResponse { data: Vec<BenchmarkItem> }

#[derive(Clone)]
pub struct OpenRouterClient {
    http: Client,
    base: String,
    referer: String,
    title: String,
    user_agent: String,
}

impl OpenRouterClient {
    pub fn new(http: Client, settings: &AppSettings) -> Self {
        Self {
            http,
            base: settings.openrouter_api_base.trim_end_matches('/').to_string(),
            referer: settings.openrouter_http_referer.clone(),
            title: settings.openrouter_x_title.clone(),
            user_agent: settings.openrouter_user_agent.clone(),
        }
    }

    fn apply_headers(&self, mut request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if !self.referer.trim().is_empty() {
            request = request.header("HTTP-Referer", &self.referer);
        }
        if !self.title.trim().is_empty() {
            request = request.header("X-Title", &self.title);
        }
        if !self.user_agent.trim().is_empty() {
            request = request.header(USER_AGENT, &self.user_agent);
        }
        request
    }

    pub async fn list_models(&self, key: &str) -> Result<Vec<OpenRouterModel>, AppError> {
        let request = self.http.get(format!("{}/models", self.base)).bearer_auth(key);
        let response = self.apply_headers(request).send().await?;
        let status = response.status();
        let text = response.text().await?;
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!("OpenRouter 权限失败: HTTP {status}: {text}")));
        }
        if !status.is_success() {
            return Err(AppError::bad(format!("OpenRouter models failed: {status} {text}")));
        }
        Ok(serde_json::from_str::<ModelsResponse>(&text)?.data)
    }

    pub async fn benchmarks(&self, key: &str) -> Result<Vec<BenchmarkItem>, AppError> {
        let request = self.http.get(format!("{}/benchmarks", self.base))
            .query(&[("source", "artificial-analysis"), ("max_results", "500")])
            .bearer_auth(key);
        let response = self.apply_headers(request).send().await?;
        let status = response.status();
        let text = response.text().await?;
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!("OpenRouter Benchmark 权限失败: HTTP {status}: {text}")));
        }
        if !status.is_success() {
            return Err(AppError::bad(format!("OpenRouter benchmarks failed: {status} {text}")));
        }
        Ok(serde_json::from_str::<BenchmarksResponse>(&text)?.data)
    }

    pub async fn test_model(&self, key: &str, model: &str) -> Result<(), AppError> {
        let request = self.http.post(format!("{}/chat/completions", self.base))
            .bearer_auth(key)
            .json(&json!({
                "model": model,
                "messages": [{"role":"user","content":"Reply with OK only."}],
                "max_tokens": 4,
                "temperature": 0
            }));
        let response = self.apply_headers(request).send().await?;
        let status = response.status();
        let value: serde_json::Value = response.json().await.unwrap_or_else(|_| json!({}));
        if status.as_u16() == 401 || status.as_u16() == 403 {
            return Err(AppError::unauthorized(format!("{model}: OpenRouter 权限失败 HTTP {status}: {value}")));
        }
        if !status.is_success() {
            return Err(AppError::bad(format!("{model}: OpenRouter preflight HTTP {status}: {value}")));
        }
        if value.get("choices").and_then(|v| v.as_array()).map(|v| v.is_empty()).unwrap_or(true) {
            return Err(AppError::bad(format!("{model}: preflight returned no choices")));
        }
        Ok(())
    }
}


use reqwest::Client;
use serde::Deserialize;
use serde_json::json;

use crate::{error::AppError, models::{BenchmarkItem, OpenRouterModel}};

#[derive(Deserialize)] struct ModelsResponse { data: Vec<OpenRouterModel> }
#[derive(Deserialize)] struct BenchmarksResponse { data: Vec<BenchmarkItem> }

#[derive(Clone)]
pub struct OpenRouterClient { http: Client, base: String }

impl OpenRouterClient {
    pub fn new(http: Client, base: &str) -> Self { Self { http, base: base.trim_end_matches('/').to_string() } }

    pub async fn list_models(&self, key: &str) -> Result<Vec<OpenRouterModel>, AppError> {
        let response = self.http.get(format!("{}/models", self.base)).bearer_auth(key).send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() { return Err(AppError::bad(format!("OpenRouter models failed: {status} {text}"))); }
        Ok(serde_json::from_str::<ModelsResponse>(&text)?.data)
    }

    pub async fn benchmarks(&self, key: &str) -> Result<Vec<BenchmarkItem>, AppError> {
        let response = self.http.get(format!("{}/benchmarks", self.base))
            .query(&[("source", "artificial-analysis"), ("max_results", "500")])
            .bearer_auth(key).send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() { return Err(AppError::bad(format!("OpenRouter benchmarks failed: {status} {text}"))); }
        Ok(serde_json::from_str::<BenchmarksResponse>(&text)?.data)
    }

    pub async fn test_model(&self, key: &str, model: &str) -> Result<(), AppError> {
        let response = self.http.post(format!("{}/chat/completions", self.base))
            .bearer_auth(key)
            .json(&json!({
                "model": model,
                "messages": [{"role":"user","content":"Reply with OK only."}],
                "max_tokens": 4,
                "temperature": 0
            }))
            .send().await?;
        let status = response.status();
        let value: serde_json::Value = response.json().await.unwrap_or_else(|_| json!({}));
        if !status.is_success() { return Err(AppError::bad(format!("{model}: OpenRouter preflight HTTP {status}: {value}"))); }
        if value.get("choices").and_then(|v| v.as_array()).map(|v| v.is_empty()).unwrap_or(true) {
            return Err(AppError::bad(format!("{model}: preflight returned no choices")));
        }
        Ok(())
    }
}

use futures_util::{stream, StreamExt};

use crate::{models::RankedModel, openrouter::OpenRouterClient};

pub async fn preflight(client: OpenRouterClient, key: String, candidates: Vec<RankedModel>, concurrency: usize) -> Vec<RankedModel> {
    let mut tested: Vec<RankedModel> = stream::iter(candidates.into_iter().map(|mut model| {
        let client = client.clone();
        let key = key.clone();
        async move {
            match client.test_model(&key, &model.id).await {
                Ok(_) => { model.usable = Some(true); }
                Err(e) => { model.usable = Some(false); model.test_error = Some(e.to_string()); }
            }
            model
        }
    })).buffer_unordered(concurrency.max(1)).collect().await;
    tested.sort_by_key(|m| m.rank);
    tested
}

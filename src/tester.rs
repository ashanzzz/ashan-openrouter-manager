use std::{collections::HashMap, time::Instant};

use chrono::Utc;
use futures_util::{stream, StreamExt};
use tokio::{sync::mpsc::UnboundedSender, time::{sleep, Duration}};
use uuid::Uuid;

use crate::{
    db::Database,
    error::AppError,
    models::{ModelHealthAttempt, RankedModel},
    openrouter::OpenRouterClient,
};

#[derive(Debug, Clone)]
pub enum HealthEvent {
    RoundStarted { round: usize, total_rounds: usize, candidate_count: usize },
    Attempt(ModelHealthAttempt),
    RoundCompleted { round: usize, total_rounds: usize },
    Waiting { seconds: u64, next_round: usize, total_rounds: usize },
}

async fn run_attempt(
    client: OpenRouterClient,
    key: String,
    batch_id: String,
    model_id: String,
    attempt: usize,
) -> ModelHealthAttempt {
    let started = Instant::now();
    let result = client.test_model(&key, &model_id).await;
    ModelHealthAttempt {
        batch_id,
        model_id,
        attempt,
        checked_at: Utc::now().to_rfc3339(),
        success: result.is_ok(),
        latency_ms: started.elapsed().as_millis() as u64,
        error: result.err().map(|e| e.to_string()),
    }
}

pub async fn health_check(
    db: &Database,
    client: OpenRouterClient,
    key: String,
    mut candidates: Vec<RankedModel>,
    attempts: usize,
    interval_seconds: u64,
    min_success_rate: f64,
    concurrency: usize,
    events: Option<UnboundedSender<HealthEvent>>,
) -> Result<Vec<RankedModel>, AppError> {
    let attempts = attempts.max(3);
    let interval_seconds = interval_seconds.max(60);
    let min_success_rate = min_success_rate.clamp(0.01, 1.0);
    let batch_id = Uuid::new_v4().to_string();
    let mut history: HashMap<String, Vec<ModelHealthAttempt>> = HashMap::new();

    for round in 1..=attempts {
        if let Some(tx) = &events {
            let _ = tx.send(HealthEvent::RoundStarted {
                round,
                total_rounds: attempts,
                candidate_count: candidates.len(),
            });
        }

        let mut pending = stream::iter(candidates.iter().map(|model| {
            run_attempt(
                client.clone(),
                key.clone(),
                batch_id.clone(),
                model.id.clone(),
                round,
            )
        }))
        .buffer_unordered(concurrency.max(1));

        while let Some(attempt) = pending.next().await {
            db.record_model_health_attempt(&attempt).await?;
            history
                .entry(attempt.model_id.clone())
                .or_default()
                .push(attempt.clone());
            if let Some(tx) = &events {
                let _ = tx.send(HealthEvent::Attempt(attempt));
            }
        }

        if let Some(tx) = &events {
            let _ = tx.send(HealthEvent::RoundCompleted {
                round,
                total_rounds: attempts,
            });
        }

        if round < attempts {
            if let Some(tx) = &events {
                let _ = tx.send(HealthEvent::Waiting {
                    seconds: interval_seconds,
                    next_round: round + 1,
                    total_rounds: attempts,
                });
            }
            sleep(Duration::from_secs(interval_seconds)).await;
        }
    }

    for model in &mut candidates {
        let mut checks = history.remove(&model.id).unwrap_or_default();
        checks.sort_by_key(|x| x.attempt);
        let successes = checks.iter().filter(|x| x.success).count();
        let success_rate = if checks.is_empty() {
            0.0
        } else {
            successes as f64 / checks.len() as f64
        };
        let successful_latencies: Vec<u64> = checks
            .iter()
            .filter(|x| x.success)
            .map(|x| x.latency_ms)
            .collect();
        let average_latency_ms = if successful_latencies.is_empty() {
            None
        } else {
            Some(successful_latencies.iter().sum::<u64>() / successful_latencies.len() as u64)
        };
        let last_checked_at = checks.last().map(|x| x.checked_at.clone());
        let last_success_at = checks.iter().rev().find(|x| x.success).map(|x| x.checked_at.clone());
        let last_failure = checks.iter().rev().find(|x| !x.success);
        let last_failure_at = last_failure.map(|x| x.checked_at.clone());
        let last_error = last_failure.and_then(|x| x.error.clone());
        let qualified = success_rate >= min_success_rate && successes > 0;

        model.usable = Some(qualified);
        model.test_error = last_error;
        model.health_attempts = checks.len();
        model.health_successes = successes;
        model.health_success_rate = success_rate;
        model.last_checked_at = last_checked_at;
        model.last_success_at = last_success_at;
        model.last_failure_at = last_failure_at;
        model.average_latency_ms = average_latency_ms;
        model.health_status = if successes == checks.len() && !checks.is_empty() {
            "healthy".into()
        } else if qualified {
            "qualified".into()
        } else {
            "unavailable".into()
        };
        model.health_batch_id = Some(batch_id.clone());
        model.health_checks = checks;
        db.upsert_model_health_summary(model).await?;
    }

    // Health is a gate, not a quality ranking signal. Preserve the benchmark rank.
    candidates.sort_by_key(|m| m.rank);
    Ok(candidates)
}

pub fn select_qualified_top3(tested: &[RankedModel]) -> Vec<RankedModel> {
    let mut qualified: Vec<RankedModel> = tested
        .iter()
        .filter(|m| m.usable == Some(true))
        .cloned()
        .collect();

    // Health is a binary admission gate only. Once a model reaches the configured
    // minimum success rate, R1/R2/R3 are selected strictly by the original
    // capability/benchmark rank. 33.3%, 66.7% and 100% are equal for ranking.
    qualified.sort_by_key(|m| m.rank);
    qualified.into_iter().take(3).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_of_three_passes_default_threshold() {
        let successes = 1usize;
        let attempts = 3usize;
        let rate = successes as f64 / attempts as f64;
        assert!(rate >= 0.30);
    }

    #[test]
    fn zero_of_three_fails_default_threshold() {
        let successes = 0usize;
        let attempts = 3usize;
        let rate = successes as f64 / attempts as f64;
        assert!(rate < 0.30);
    }

    fn sample(rank: usize, rate: f64) -> RankedModel {
        RankedModel {
            rank, id: format!("m{rank}"), name: format!("M{rank}"), context_length: 65536,
            intelligence_index: Some(100.0 - rank as f64), coding_index: None, agentic_index: None,
            score: 0.0, usable: Some(true), test_error: None, health_attempts: 3,
            health_successes: (rate * 3.0).round() as usize, health_success_rate: rate,
            last_checked_at: None, last_success_at: None, last_failure_at: None, average_latency_ms: None,
            health_status: "qualified".into(), health_batch_id: None, health_checks: vec![],
        }
    }

    #[test]
    fn health_does_not_reorder_qualified_models() {
        let tested = vec![sample(1, 1.0 / 3.0), sample(2, 1.0 / 3.0), sample(3, 1.0), sample(4, 2.0 / 3.0)];
        let selected = select_qualified_top3(&tested);
        assert_eq!(selected[0].rank, 1);
        assert_eq!(selected[1].rank, 2);
        assert_eq!(selected[2].rank, 3);
    }

    #[test]
    fn unavailable_higher_rank_is_skipped_without_health_resorting() {
        let mut first = sample(1, 0.0);
        first.usable = Some(false);
        let tested = vec![first, sample(2, 1.0 / 3.0), sample(3, 1.0), sample(4, 2.0 / 3.0)];
        let selected = select_qualified_top3(&tested);
        assert_eq!(selected.iter().map(|m| m.rank).collect::<Vec<_>>(), vec![2, 3, 4]);
    }
}

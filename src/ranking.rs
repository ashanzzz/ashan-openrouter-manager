use std::{cmp::Ordering, collections::{HashMap, HashSet}};

use crate::models::{AppSettings, BenchmarkItem, OpenRouterModel, RankedModel, RankingMode};

fn zero_price(v: &str) -> bool { v.parse::<f64>().map(|x| x == 0.0).unwrap_or(false) }
fn normalized(id: &str) -> String { id.strip_suffix(":free").unwrap_or(id).to_string() }
fn score(v: Option<f64>) -> f64 { v.unwrap_or(-1.0) }

pub fn rank(models: Vec<OpenRouterModel>, benchmarks: Vec<BenchmarkItem>, settings: &AppSettings) -> (usize, Vec<RankedModel>) {
    let includes: HashSet<String> = settings.include_models.iter().cloned().collect();
    let excludes: HashSet<String> = settings.exclude_models.iter().cloned().collect();
    let mut benchmark_map: HashMap<String, BenchmarkItem> = HashMap::new();
    for b in benchmarks { benchmark_map.insert(normalized(&b.model_permaslug), b); }

    let mut free_count = 0usize;
    let mut ranked = Vec::new();
    for m in models {
        if excludes.contains(&m.id) { continue; }
        let included = includes.contains(&m.id);
        if !zero_price(&m.pricing.prompt) || !zero_price(&m.pricing.completion) { continue; }
        if settings.require_free_suffix && !m.id.ends_with(":free") && !included { continue; }
        if m.context_length < settings.min_context_length { continue; }
        let text_output = m.architecture.output_modalities.iter().any(|x| x == "text") || m.architecture.modality.contains("text");
        if !text_output { continue; }
        free_count += 1;
        let lookup = m.canonical_slug.clone().unwrap_or_else(|| normalized(&m.id));
        let bm = benchmark_map.get(&normalized(&lookup));
        if settings.require_benchmark && bm.and_then(|b| b.intelligence_index).is_none() && !included { continue; }
        let intelligence = bm.and_then(|b| b.intelligence_index);
        let coding = bm.and_then(|b| b.coding_index);
        let agentic = bm.and_then(|b| b.agentic_index);
        let weighted = score(intelligence).max(0.0) * settings.intelligence_weight
            + score(coding).max(0.0) * settings.coding_weight
            + score(agentic).max(0.0) * settings.agentic_weight
            + (m.context_length as f64 / 1_000_000.0) * settings.context_weight;
        ranked.push(RankedModel {
            rank: 0, id: m.id, name: m.name, context_length: m.context_length,
            intelligence_index: intelligence, coding_index: coding, agentic_index: agentic,
            score: weighted, usable: None, test_error: None,
        });
    }

    ranked.sort_by(|a,b| match settings.ranking_mode {
        RankingMode::Weighted => b.score.partial_cmp(&a.score).unwrap_or(Ordering::Equal).then_with(|| a.id.cmp(&b.id)),
        RankingMode::IntelligenceFirst => score(b.intelligence_index).partial_cmp(&score(a.intelligence_index)).unwrap_or(Ordering::Equal)
            .then_with(|| score(b.coding_index).partial_cmp(&score(a.coding_index)).unwrap_or(Ordering::Equal))
            .then_with(|| score(b.agentic_index).partial_cmp(&score(a.agentic_index)).unwrap_or(Ordering::Equal))
            .then_with(|| b.context_length.cmp(&a.context_length))
            .then_with(|| a.id.cmp(&b.id)),
    });
    ranked.truncate(settings.candidate_pool);
    for (i, item) in ranked.iter_mut().enumerate() { item.rank = i + 1; }
    (free_count, ranked)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ModelArchitecture, ModelPricing};

    #[test]
    fn paid_models_are_filtered() {
        let models = vec![OpenRouterModel { id:"a:free".into(), name:"A".into(), canonical_slug:None, context_length:65536, pricing:ModelPricing{prompt:"0".into(),completion:"0".into()}, architecture:ModelArchitecture{modality:"text->text".into(),output_modalities:vec!["text".into()]} },
            OpenRouterModel { id:"b".into(), name:"B".into(), canonical_slug:None, context_length:65536, pricing:ModelPricing{prompt:"0.1".into(),completion:"0".into()}, architecture:ModelArchitecture{modality:"text->text".into(),output_modalities:vec!["text".into()]} }];
        let bm = vec![BenchmarkItem{model_permaslug:"a".into(),display_name:"A".into(),intelligence_index:Some(10.0),coding_index:Some(9.0),agentic_index:Some(8.0),source:"artificial-analysis".into()}];
        let (free, ranked) = rank(models,bm,&AppSettings::default());
        assert_eq!(free,1); assert_eq!(ranked.len(),1); assert_eq!(ranked[0].id,"a:free");
    }
}

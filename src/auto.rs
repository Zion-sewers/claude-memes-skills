//! Auto mode — the semantic long tail.
//!
//! Curated events stay the fast path; anything unmapped becomes a *concept*
//! that gets embedded and matched against the emotion hubs in
//! `config/hubs.toml` (hub-and-spoke: the hub carries the joke). Firing is
//! gated hard: cheap interestingness prefilter → hourly budget → per-hub
//! novelty → similarity threshold. Ranking is pure and unit-tested; model IO
//! lives in `embed`.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::Deserialize;

const DEFAULT_HUBS: &str = include_str!("../config/hubs.toml");

#[derive(Debug, Deserialize)]
pub struct HubFile {
    pub hubs: BTreeMap<String, Hub>,
}

#[derive(Debug, Deserialize)]
pub struct Hub {
    pub description: String,
    pub queries: Vec<String>,
}

pub fn hubs() -> Result<HubFile> {
    toml::from_str(DEFAULT_HUBS).context("parsing embedded hubs.toml")
}

/// Pure ranking: cosine of the concept vector against every hub vector,
/// descending. Separated from model IO so it's testable with synthetic vectors.
pub fn rank<'a>(concept: &[f32], hub_vecs: &'a [(String, Vec<f32>)]) -> Vec<(&'a str, f32)> {
    let mut scored: Vec<(&str, f32)> = hub_vecs
        .iter()
        .map(|(name, vec)| (name.as_str(), crate::embed::cosine(concept, vec)))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored
}

/// Pick a query from the hub's pool, rotating over time rather than randomly
/// (no rand dependency; per-firing variety is all that matters).
pub fn pick_query(hub: &Hub) -> &str {
    if hub.queries.is_empty() {
        return "";
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    &hub.queries[nanos % hub.queries.len()]
}

/// Loaded model + pre-embedded hub vectors. Load once, suggest many times
/// (the watch daemon holds one for its lifetime; hooks build one per firing).
pub struct Engine {
    model: model2vec_rs::model::StaticModel,
    pub hubs: HubFile,
    hub_vecs: Vec<(String, Vec<f32>)>,
}

impl Engine {
    pub fn load(allow_download: bool) -> Result<Engine> {
        let model = crate::embed::load(allow_download)?;
        let hubs = hubs()?;
        let descriptions: Vec<String> = hubs.hubs.values().map(|h| h.description.clone()).collect();
        let vectors = model.encode(&descriptions);
        let hub_vecs = hubs.hubs.keys().cloned().zip(vectors).collect();
        Ok(Engine {
            model,
            hubs,
            hub_vecs,
        })
    }

    /// All hubs ranked against the concept, best first.
    pub fn top(&self, concept: &str) -> Vec<(&str, f32)> {
        let vectors = self.model.encode(&[concept.to_string()]);
        rank(&vectors[0], &self.hub_vecs)
    }

    /// Best hub if it clears the threshold: (hub name, score, chosen query).
    pub fn suggest(&self, concept: &str, threshold: f32) -> Option<(String, f32, String)> {
        let ranked = self.top(concept);
        let (name, score) = ranked.first()?;
        if *score < threshold {
            return None;
        }
        let hub = self.hubs.hubs.get(*name)?;
        Some((name.to_string(), *score, pick_query(hub).to_string()))
    }
}

/// Commands whose invocation is never a meme-worthy moment on its own.
const TRIVIAL_COMMANDS: &[&str] = &[
    "ls", "cd", "pwd", "cat", "echo", "which", "env", "head", "tail", "wc", "date", "true",
    "mkdir", "touch", "cp", "mv", "sed", "awk", "grep", "find", "sleep",
];

/// Extract an embeddable concept from a Bash hook payload, or None when the
/// moment is too boring to consider (cheap prefilter — runs before any model
/// load, so the common path costs microseconds).
pub fn bash_concept(command: &str, response: &str) -> Option<String> {
    let cmd = command.trim();
    if cmd.is_empty() || response.len() < 40 {
        return None;
    }
    let first_word = cmd
        .split_whitespace()
        .next()?
        .rsplit('/')
        .next()
        .unwrap_or_default();
    if TRIVIAL_COMMANDS.contains(&first_word) {
        return None;
    }
    let head: String = response.chars().take(240).collect();
    Some(format!("running `{cmd}` resulted in: {head}"))
}

/// Concept for a skill invocation that has no curated mapping.
pub fn skill_concept(skill: &str) -> Option<String> {
    let skill = skill.trim();
    if skill.is_empty() {
        return None;
    }
    Some(format!("invoked the {skill} skill to work on a task"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_hubs_parse_and_are_complete() {
        let file = hubs().expect("hubs.toml must parse");
        assert!(file.hubs.len() >= 35, "expected a rich hub set");
        for (name, hub) in &file.hubs {
            assert!(
                !hub.description.trim().is_empty(),
                "{name} lacks description"
            );
            assert!(!hub.queries.is_empty(), "{name} lacks queries");
            assert!(
                hub.queries.iter().all(|q| !q.trim().is_empty()),
                "{name} has an empty query"
            );
        }
    }

    #[test]
    fn rank_orders_by_similarity() {
        let hub_vecs = vec![
            ("far".to_string(), vec![0.0, 1.0]),
            ("near".to_string(), vec![1.0, 0.1]),
        ];
        let ranked = rank(&[1.0, 0.0], &hub_vecs);
        assert_eq!(ranked[0].0, "near");
        assert!(ranked[0].1 > ranked[1].1);
    }

    #[test]
    fn bash_concept_filters_boring_moments() {
        assert!(bash_concept("ls -la", &"x".repeat(100)).is_none());
        assert!(bash_concept("/bin/cat foo", &"x".repeat(100)).is_none());
        assert!(bash_concept("cargo build", "short").is_none());
        assert!(bash_concept("", &"x".repeat(100)).is_none());
        let c = bash_concept(
            "cargo build --release",
            &"error[E0308]: mismatched types ".repeat(3),
        );
        assert!(c.unwrap().contains("cargo build --release"));
    }

    #[test]
    fn concept_text_is_bounded() {
        let c = bash_concept("cargo test", &"y".repeat(10_000)).unwrap();
        assert!(c.len() < 400);
    }

    #[test]
    fn pick_query_never_panics() {
        let hub = Hub {
            description: "d".into(),
            queries: vec!["a".into(), "b".into()],
        };
        let q = pick_query(&hub);
        assert!(q == "a" || q == "b");
    }
}

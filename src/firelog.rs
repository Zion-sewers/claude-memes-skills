//! Firing log + dream replay.
//!
//! Every delivered meme — curated event or auto-mode hub — appends one line to
//! `firings.jsonl`. `claude-memes dream` replays the log the way the org
//! harnesses replay their week: what fired, what the auto long-tail keeps
//! reaching for, and which hubs have earned a promotion into the curated
//! registry. No content is logged — just kind, label, and timestamp.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config::cache_dir;

const MAX_LOG_BYTES: u64 = 1_000_000;
/// A hub that fired this often is a recurring moment worth curating.
const PROMOTION_THRESHOLD: u64 = 3;

fn log_path() -> PathBuf {
    cache_dir().join("firings.jsonl")
}

/// Record a delivered meme. `kind` is "curated" or "auto"; `label` is the
/// event or hub name. Best-effort, size-capped, never fails the caller.
pub fn record(kind: &str, label: &str) {
    let path = log_path();
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > MAX_LOG_BYTES {
            return;
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::json!({"ts": ts, "kind": kind, "label": label}).to_string();
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// (kind, label) → firing count, from the on-disk log.
pub fn counts() -> BTreeMap<(String, String), u64> {
    let Ok(contents) = std::fs::read_to_string(log_path()) else {
        return BTreeMap::new();
    };
    aggregate(&contents)
}

/// Pure aggregation over jsonl content — unit-testable without the filesystem.
pub fn aggregate(jsonl: &str) -> BTreeMap<(String, String), u64> {
    let mut out = BTreeMap::new();
    for line in jsonl.lines() {
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let (Some(kind), Some(label)) = (
            v.get("kind").and_then(|k| k.as_str()),
            v.get("label").and_then(|l| l.as_str()),
        ) else {
            continue;
        };
        *out.entry((kind.to_string(), label.to_string()))
            .or_insert(0) += 1;
    }
    out
}

/// The dream report: replay the firing log, surface recurring auto hubs, and
/// print ready-to-paste registry snippets for the ones worth promoting.
pub fn dream(hubs: &crate::auto::HubFile) {
    let counts = counts();
    if counts.is_empty() {
        println!(
            "Nothing to dream about yet — the firing log at {} is empty.",
            log_path().display()
        );
        return;
    }

    let mut curated: Vec<(&str, u64)> = Vec::new();
    let mut auto: Vec<(&str, u64)> = Vec::new();
    for ((kind, label), n) in &counts {
        match kind.as_str() {
            "curated" => curated.push((label, *n)),
            "auto" => auto.push((label, *n)),
            _ => {}
        }
    }
    curated.sort_by_key(|c| std::cmp::Reverse(c.1));
    auto.sort_by_key(|a| std::cmp::Reverse(a.1));

    println!("dream report — what actually fired\n");
    println!(
        "curated events ({} total firings):",
        curated.iter().map(|c| c.1).sum::<u64>()
    );
    for (label, n) in curated.iter().take(10) {
        println!("  {n:>4}× {label}");
    }
    println!(
        "\nauto-mode hubs ({} total firings):",
        auto.iter().map(|a| a.1).sum::<u64>()
    );
    for (label, n) in auto.iter().take(10) {
        println!("  {n:>4}× {label}");
    }

    let promotable: Vec<&(&str, u64)> = auto
        .iter()
        .filter(|(_, n)| *n >= PROMOTION_THRESHOLD)
        .collect();
    if promotable.is_empty() {
        println!("\nNo auto hub has fired {PROMOTION_THRESHOLD}+ times yet — nothing to promote.");
        return;
    }

    println!(
        "\nThese hubs keep recurring — consider pinning them as curated events\n(paste into events.toml and wire a trigger, or leave them to auto mode):\n"
    );
    for (label, n) in promotable {
        let queries = hubs
            .hubs
            .get(*label)
            .map(|h| h.queries.clone())
            .unwrap_or_default();
        let query = queries.first().cloned().unwrap_or_default();
        println!("# fired {n}× via auto mode");
        println!("[events.{label}]");
        println!("query = \"{query}\"");
        println!("trigger = \"Promoted from auto mode — add a detect rule in src/detect.rs\"\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_counts_by_kind_and_label() {
        let jsonl = r#"{"ts":1,"kind":"auto","label":"dependency_hell"}
{"ts":2,"kind":"auto","label":"dependency_hell"}
{"ts":3,"kind":"curated","label":"tests_all_pass"}
not json at all
{"ts":4,"kind":"auto","label":"waiting"}"#;
        let counts = aggregate(jsonl);
        assert_eq!(counts[&("auto".into(), "dependency_hell".into())], 2);
        assert_eq!(counts[&("curated".into(), "tests_all_pass".into())], 1);
        assert_eq!(counts[&("auto".into(), "waiting".into())], 1);
        assert_eq!(counts.len(), 3);
    }

    #[test]
    fn aggregate_ignores_malformed_entries() {
        let counts = aggregate("{\"ts\":1}\n{\"kind\":\"auto\"}\n");
        assert!(counts.is_empty());
    }
}

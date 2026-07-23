//! Empirical hook-payload shape logging.
//!
//! The exact per-event stdin schema is not reliably documented (and varies
//! across harness versions), so we record what actually arrives — but only the
//! *structure*: event name + sorted field paths. Values are never written, so
//! prompts, commands, and tool output stay out of the log by construction.

use std::collections::BTreeSet;
use std::path::PathBuf;

use crate::config::cache_dir;

const MAX_LOG_BYTES: u64 = 1_000_000;

fn shapes_path() -> PathBuf {
    cache_dir().join("hook-shapes.jsonl")
}

/// Collect sorted dot-paths of every field in a JSON value. Arrays collapse to
/// `[]` so payload size doesn't explode the shape.
pub fn field_paths(value: &serde_json::Value) -> Vec<String> {
    let mut out = BTreeSet::new();
    walk(value, String::new(), &mut out);
    out.into_iter().collect()
}

fn walk(value: &serde_json::Value, prefix: String, out: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.insert(path.clone());
                walk(child, path, out);
            }
        }
        serde_json::Value::Array(items) => {
            let path = format!("{prefix}[]");
            for child in items.iter().take(3) {
                walk(child, path.clone(), out);
            }
        }
        _ => {}
    }
}

/// Append this payload's shape unless an identical shape was already recorded.
/// Best-effort: never fails the hook.
pub fn record(kind: &str, payload: &serde_json::Value) {
    let event_name = payload
        .get("hook_event_name")
        .and_then(|v| v.as_str())
        .unwrap_or(kind);
    let fields = field_paths(payload);
    let line = serde_json::json!({"event": event_name, "fields": fields}).to_string();

    let path = shapes_path();
    crate::config::rotate_if_large(&path, MAX_LOG_BYTES);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        if existing.lines().any(|l| l == line) {
            return;
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(f, "{line}");
    }
}

/// Print every recorded shape (for `claude-memes shapes`).
pub fn print_all() {
    match std::fs::read_to_string(shapes_path()) {
        Ok(contents) if !contents.trim().is_empty() => {
            for line in contents.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    let event = v.get("event").and_then(|e| e.as_str()).unwrap_or("?");
                    println!("{event}:");
                    if let Some(fields) = v.get("fields").and_then(|f| f.as_array()) {
                        for f in fields {
                            if let Some(s) = f.as_str() {
                                println!("  {s}");
                            }
                        }
                    }
                    println!();
                }
            }
        }
        _ => println!(
            "No hook payload shapes recorded yet. Shapes accumulate at {} as hooks fire.",
            shapes_path().display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_sorted_and_nested() {
        let v = serde_json::json!({
            "tool_input": {"command": "ls", "timeout": 5},
            "session_id": "abc",
        });
        let paths = field_paths(&v);
        assert_eq!(
            paths,
            vec![
                "session_id",
                "tool_input",
                "tool_input.command",
                "tool_input.timeout"
            ]
        );
    }

    #[test]
    fn arrays_collapse() {
        let v = serde_json::json!({"items": [{"a": 1}, {"a": 2}, {"b": 3}]});
        let paths = field_paths(&v);
        assert!(paths.contains(&"items[].a".to_string()));
        assert!(paths.contains(&"items[].b".to_string()));
    }

    #[test]
    fn no_values_ever_appear() {
        let v = serde_json::json!({"secret_prompt": "the nuclear codes are 1234"});
        let paths = field_paths(&v);
        assert_eq!(paths, vec!["secret_prompt"]);
    }
}

//! Hook orchestration: stdin payload → detection → tiered delivery.
//!
//! Everything a `claude-memes hook <kind>` invocation does lives here: shape
//! logging, curated-event detection, the auto-mode concept path, and the
//! delivery tiers (watch daemon → legacy tty → terminalSequence notification).
//! Hooks must never break a session: `run` always prints valid JSON and
//! returns Ok.

use std::io::Read;
use std::process::{Command, Stdio};

use anyhow::Result;
use clap::Subcommand;

use crate::config::Registry;
use crate::{auto, daemon, detect, embed, firelog, notify, shapes, state};

/// Daemon-less auto attempts pay the ~80ms model load even when nothing fires,
/// so they are throttled independently of the firing budget.
const AUTO_ATTEMPT_MIN_SECS: u64 = 30;

#[derive(Subcommand)]
pub enum HookKind {
    /// PostToolUse[Bash]
    Bash,
    /// PostToolUse[Skill]
    Skill,
    /// Stop
    Stop,
    /// Any other event: record its payload shape, take no action.
    Observe,
}

pub fn run(kind: HookKind) -> Result<()> {
    let output = (|| -> Result<serde_json::Value> {
        let empty = serde_json::json!({});
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).ok();
        let payload: serde_json::Value =
            serde_json::from_str(&input).unwrap_or(serde_json::Value::Null);

        // Record what actually arrives (field names only, never values) —
        // the per-event payload schema is undocumented and shifts.
        if !payload.is_null() {
            shapes::record(kind_label(&kind), &payload);
        }

        let reg = Registry::load()?;
        if !state::is_enabled(reg.settings.enabled) {
            return Ok(empty);
        }

        let mut sequence = None;
        let mut events = events_for_hook(&kind, &payload);

        // Auto mode: an unmapped moment (no curated event, or only the generic
        // skill clip) becomes a concept for semantic hub matching.
        if reg.settings.auto && (events.is_empty() || events == ["skill_loaded"]) {
            if let Some(concept) = concept_for(&kind, &payload) {
                events.clear();
                if !daemon::try_send_concept(reg.settings.watch_port, &concept) {
                    sequence = local_auto_suggest(&reg, &concept);
                }
            }
        }

        for event in events {
            let delivered = deliver(&reg, &event);
            if delivered {
                firelog::record("curated", &event);
            }
            // First undeliverable event falls back to a terminalSequence
            // notification (one per hook response).
            if !delivered && sequence.is_none() {
                let body = match reg.event(&event) {
                    Some(ev) if !ev.trigger.is_empty() => {
                        format!("{event} — {}", ev.trigger)
                    }
                    _ => event.clone(),
                };
                sequence = notify::sequence(&reg.settings.notify, "claude-memes", &body);
                if sequence.is_some() {
                    firelog::record("curated", &event);
                }
            }
        }
        Ok(match sequence {
            Some(seq) => serde_json::json!({ "terminalSequence": seq }),
            None => empty,
        })
    })();

    // Claude Code reads this stdout as the hook result.
    match output {
        Ok(value) => println!("{value}"),
        Err(e) => {
            eprintln!("claude-memes hook: {e:#}");
            println!("{{}}");
        }
    }
    Ok(())
}

fn kind_label(kind: &HookKind) -> &'static str {
    match kind {
        HookKind::Bash => "PostToolUse[Bash]",
        HookKind::Skill => "PostToolUse[Skill]",
        HookKind::Stop => "Stop",
        HookKind::Observe => "Observe",
    }
}

/// Build a concept for auto mode from an unmapped hook payload.
fn concept_for(kind: &HookKind, payload: &serde_json::Value) -> Option<String> {
    match kind {
        HookKind::Bash => {
            let command = payload
                .pointer("/tool_input/command")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            auto::bash_concept(command, &response_text(payload))
        }
        HookKind::Skill => {
            let skill = payload
                .pointer("/tool_input/skill")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            auto::skill_concept(skill)
        }
        HookKind::Stop | HookKind::Observe => None,
    }
}

/// Daemon-less auto path: suggest locally and emit a notification. Never
/// downloads the model (hooks must stay fast), throttles attempts so busy
/// sessions don't pay the model load on every command, and consumes firing
/// state only once a deliverable suggestion exists.
fn local_auto_suggest(reg: &Registry, concept: &str) -> Option<String> {
    if !embed::model_cached() {
        return None;
    }
    if !state::cooldown_ok("auto__attempt", AUTO_ATTEMPT_MIN_SECS) {
        return None;
    }
    let engine = auto::Engine::load(false).ok()?;
    let (hub, _score, query) = engine.suggest(concept, reg.settings.auto_threshold)?;
    let seq = notify::sequence(
        &reg.settings.notify,
        "claude-memes",
        &format!("{hub}: {query}"),
    )?;
    if !state::cooldown_ok(&format!("auto__{hub}"), 3600) {
        return None;
    }
    if !state::auto_budget_try_consume(reg.settings.auto_per_hour) {
        return None;
    }
    firelog::record("auto", &hub);
    Some(seq)
}

/// Delivery tiers: watch daemon (inline GIF in the user's own terminal) →
/// legacy /dev/tty spawn (pre-2.1.141 Claude Code) → not delivered (caller
/// falls back to a terminalSequence notification).
fn deliver(reg: &Registry, event: &str) -> bool {
    if daemon::try_send(reg.settings.watch_port, event) {
        return true;
    }
    background_play(event)
}

fn events_for_hook(kind: &HookKind, payload: &serde_json::Value) -> Vec<String> {
    match kind {
        HookKind::Bash => {
            let command = payload
                .pointer("/tool_input/command")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let response = response_text(payload);
            if command.is_empty() {
                return vec![];
            }
            match detect::detect_bash(command, &response, state::last_test().as_deref()) {
                Some(d) => {
                    if let Some(s) = d.set_last_test {
                        state::set_last_test(s);
                    }
                    // A deploy landing on Friday evening earns the warning clip instead.
                    if d.event == "ci_deploy_success" && is_friday_evening() {
                        vec!["friday_deploy".to_string()]
                    } else {
                        vec![d.event.to_string()]
                    }
                }
                None => vec![],
            }
        }
        HookKind::Skill => {
            let skill = payload
                .pointer("/tool_input/skill")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if skill.is_empty() {
                return vec![];
            }
            vec![detect::detect_skill(skill).to_string()]
        }
        HookKind::Observe => vec![],
        HookKind::Stop => match payload.get("session_id").and_then(|v| v.as_str()) {
            Some(session) => stop_events(session),
            // Session signals are meaningless without a session identity; a
            // shared fallback bucket would leak fire-once state across sessions.
            None => vec![],
        },
    }
}

/// Stop-event signals. Each fires at most once per session (keyed by the
/// Claude Code session id from the hook payload).
fn stop_events(session: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut push_once = |event: &str| {
        if state::fire_once(session, event) {
            out.push(event.to_string());
        }
    };

    // Every Stop event is one completed turn; count them ourselves.
    let turns = state::bump_turns(session);
    if turns == 1 {
        push_once("session_start");
    }
    if turns >= 20 {
        push_once("desperation");
    }
    if state::last_test().as_deref() == Some("fail") {
        push_once("not_prepared");
    }
    if git_diff_total() >= 500 {
        push_once("scope_too_big");
    }
    if let Some(hour) = local_hour() {
        if !(5..22).contains(&hour) {
            push_once("end_of_day");
        }
    }
    out
}

/// Normalize `tool_response` (string or object) into searchable text.
fn response_text(payload: &serde_json::Value) -> String {
    match payload.get("tool_response") {
        Some(serde_json::Value::String(s)) => s.chars().take(4000).collect(),
        Some(other) => other.to_string().chars().take(4000).collect(),
        None => String::new(),
    }
}

/// Spawn a detached `play` that renders to the controlling terminal
/// (`/dev/tty`). Only works on harness versions where hooks still have a
/// controlling terminal; current Claude Code does not, in which case the open
/// fails and we report undelivered so the caller can fall back.
fn background_play(event: &str) -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Ok(tty) = std::fs::OpenOptions::new().write(true).open("/dev/tty") else {
        return false;
    };
    Command::new(exe)
        .arg("play")
        .arg(event)
        .stdin(Stdio::null())
        .stdout(Stdio::from(tty))
        .stderr(Stdio::null())
        .spawn()
        .is_ok()
}

/// Local (ISO weekday 1–7, hour 0–23) from a single `date` invocation.
fn local_weekday_hour() -> Option<(u32, u32)> {
    let out = Command::new("date").arg("+%u %H").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split_whitespace();
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

/// Friday (ISO weekday 5) at or after 16:00 local time.
fn is_friday_evening() -> bool {
    matches!(local_weekday_hour(), Some((5, h)) if h >= 16)
}

/// Local hour [0,23] (system timezone). None on failure.
fn local_hour() -> Option<u32> {
    local_weekday_hour().map(|(_, h)| h)
}

/// Sum of added+deleted lines vs HEAD, via `git diff --numstat`. 0 on any error.
fn git_diff_total() -> i64 {
    let Ok(out) = Command::new("git")
        .args(["diff", "--numstat", "HEAD"])
        .output()
    else {
        return 0;
    };
    if !out.status.success() {
        return 0;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let add: i64 = cols.next()?.parse().ok()?;
            let del: i64 = cols.next()?.parse().ok()?;
            Some(add + del)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_text_handles_string_and_object() {
        let p = serde_json::json!({"tool_response": "hello"});
        assert_eq!(response_text(&p), "hello");
        let p = serde_json::json!({"tool_response": {"stdout": "x"}});
        assert!(response_text(&p).contains("stdout"));
    }
}

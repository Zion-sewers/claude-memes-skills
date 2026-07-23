//! claude-memes — event-driven meme GIFs for Claude Code.
//!
//! A single static binary that Claude Code's hook system invokes on workflow
//! events. It resolves the event to a Giphy search, fetches+caches a GIF, and
//! renders it inline in your terminal. No yt-dlp, no ffmpeg, no popups.

mod auto;
mod config;
mod daemon;
mod detect;
mod embed;
mod firelog;
mod giphy;
mod mcp;
mod notify;
mod render;
mod shapes;
mod state;

use std::io::Read;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use config::Registry;

#[derive(Parser)]
#[command(
    name = "claude-memes",
    version,
    about = "Event-driven meme GIFs for Claude Code, rendered inline in your terminal."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Fetch and render the GIF for a single event (respects enabled + cooldown).
    Play {
        /// Event name from the registry, e.g. `tests_all_pass`.
        event: String,
        /// Render even if the event is within its cooldown window.
        #[arg(long)]
        force: bool,
        /// Fetch + decode only (no rendering); prints the frame count. For CI/healthchecks.
        #[arg(long)]
        check: bool,
    },
    /// Hook entry point: read a Claude Code tool payload on stdin and fire clips.
    Hook {
        #[command(subcommand)]
        kind: HookKind,
    },
    /// List all registered events and their Giphy queries.
    List,
    /// Diagnose terminal support, API key, config, and cache.
    Doctor,
    /// Install hooks into ~/.claude/settings.json and seed the on-disk registry.
    Install {
        /// Print the settings.json patch instead of applying it.
        #[arg(long)]
        print: bool,
    },
    /// Run the render daemon: hook-triggered memes play inline in this terminal.
    Watch {
        /// Override the listen port (default from settings: 42741).
        #[arg(long)]
        port: Option<u16>,
    },
    /// Print the hook payload shapes observed so far (field names only, no values).
    Shapes,
    /// Rank the emotion hubs for a concept (downloads the ~30MB model on first use).
    Find {
        /// A workflow moment in plain words, e.g. "cargo build exploded with 47 errors".
        concept: String,
    },
    /// Serve find_meme/play_meme over MCP (stdio) for org agents and gateways.
    Mcp,
    /// Replay the firing log: what fired, and which auto hubs earned promotion.
    Dream,
}

#[derive(Subcommand)]
enum HookKind {
    /// PostToolUse[Bash]
    Bash,
    /// PostToolUse[Skill]
    Skill,
    /// Stop
    Stop,
    /// Any other event: record its payload shape, take no action.
    Observe,
}

fn main() {
    let cli = Cli::parse();
    let code = match run(cli) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("claude-memes: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Cmd::Play {
            event,
            force,
            check,
        } => cmd_play(&event, force, check),
        Cmd::Hook { kind } => cmd_hook(kind),
        Cmd::List => cmd_list(),
        Cmd::Doctor => cmd_doctor(),
        Cmd::Install { print } => cmd_install(print),
        Cmd::Watch { port } => cmd_watch(port),
        Cmd::Shapes => {
            shapes::print_all();
            Ok(())
        }
        Cmd::Find { concept } => cmd_find(&concept),
        Cmd::Mcp => mcp::serve(),
        Cmd::Dream => {
            firelog::dream(&auto::hubs()?);
            Ok(())
        }
    }
}

// ----- watch ---------------------------------------------------------------

fn cmd_watch(port_override: Option<u16>) -> Result<()> {
    let reg = Registry::load()?;
    let port = port_override.unwrap_or(reg.settings.watch_port);
    // The engine loads lazily on the first concept and lives for the daemon's
    // lifetime, amortizing model load to zero for every later suggestion.
    let engine: std::cell::OnceCell<Option<auto::Engine>> = std::cell::OnceCell::new();
    daemon::run(port, |msg| match msg {
        daemon::Msg::Event(event) => {
            if let Err(e) = cmd_play(&event, false, false) {
                eprintln!("claude-memes watch: {e:#}");
            }
        }
        daemon::Msg::Concept(concept) => {
            if !reg.settings.auto {
                return;
            }
            let engine = engine.get_or_init(|| match auto::Engine::load(true) {
                Ok(e) => Some(e),
                Err(e) => {
                    eprintln!("claude-memes watch: auto mode unavailable: {e:#}");
                    None
                }
            });
            let Some(engine) = engine else { return };
            if let Some((hub, score, query)) = engine.suggest(&concept, reg.settings.auto_threshold)
            {
                // Gates consume state only when we actually intend to render.
                if state::cooldown_ok(&format!("auto__{hub}"), 3600)
                    && state::auto_budget_try_consume(reg.settings.auto_per_hour)
                {
                    println!("auto: {hub} ({score:.2}) → \"{query}\"");
                    match giphy::fetch_gif(&format!("auto_{hub}"), &query, &reg.settings) {
                        Ok(bytes) => {
                            firelog::record("auto", &hub);
                            if let Err(e) = render::play(&bytes, &reg.settings) {
                                eprintln!("claude-memes watch: {e:#}");
                            }
                        }
                        Err(e) => eprintln!("claude-memes watch: {e:#}"),
                    }
                }
            }
        }
    })
}

// ----- find ----------------------------------------------------------------

fn cmd_find(concept: &str) -> Result<()> {
    let reg = Registry::load()?;
    if !embed::model_cached() {
        println!(
            "downloading embedding model {} (~30MB, one-time)…",
            embed::MODEL_ID
        );
    }
    let engine = auto::Engine::load(true)?;
    println!("concept: {concept}\n");
    for (name, score) in engine.top(concept).iter().take(5) {
        let fires = if *score >= reg.settings.auto_threshold {
            "→"
        } else {
            " "
        };
        let query = engine
            .hubs
            .hubs
            .get(*name)
            .map(|h| h.queries.join(" | "))
            .unwrap_or_default();
        println!("{fires} {score:>6.3}  {name:<22} {query}");
    }
    println!(
        "\nthreshold: {} (auto mode fires on '→' rows; tune auto_threshold in events.toml)",
        reg.settings.auto_threshold
    );
    Ok(())
}

// ----- play ----------------------------------------------------------------

fn cmd_play(event: &str, force: bool, check: bool) -> Result<()> {
    let reg = Registry::load()?;
    let Some(ev) = reg.event(event) else {
        anyhow::bail!("unknown event '{event}' (try `claude-memes list`)");
    };

    // --check is a healthcheck: always fetch + decode, never gated by enabled/cooldown.
    if check {
        let bytes = giphy::fetch_gif(event, &ev.query, &reg.settings)?;
        let frames = render::probe(&bytes, &reg.settings)?;
        println!("ok: '{event}' → \"{}\" ({frames} frames)", ev.query);
        return Ok(());
    }

    if !state::is_enabled(reg.settings.enabled) {
        return Ok(());
    }
    if !force && !state::cooldown_ok(event, reg.settings.cooldown_secs) {
        return Ok(());
    }
    let bytes = giphy::fetch_gif(event, &ev.query, &reg.settings)?;
    render::play(&bytes, &reg.settings)?;
    Ok(())
}

// ----- hooks ---------------------------------------------------------------

fn cmd_hook(kind: HookKind) -> Result<()> {
    // Hooks must never break a session: emit valid JSON and exit 0 regardless.
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
/// downloads the model (hooks must stay fast) and consumes gate state only
/// after a deliverable suggestion exists.
fn local_auto_suggest(reg: &Registry, concept: &str) -> Option<String> {
    if !embed::model_cached() {
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

// ----- list / doctor -------------------------------------------------------

fn cmd_list() -> Result<()> {
    let reg = Registry::load()?;
    println!("{} events ({}):\n", reg.events.len(), source_label());
    for (name, ev) in &reg.events {
        println!("  {name:<20} → {}", ev.query);
        if !ev.trigger.is_empty() {
            println!("  {:<20}   {}", "", ev.trigger);
        }
    }
    Ok(())
}

fn cmd_doctor() -> Result<()> {
    let reg = Registry::load().ok();
    println!("claude-memes doctor\n");

    let enabled = reg.as_ref().map(|r| state::is_enabled(r.settings.enabled));
    print_check(
        "master switch",
        enabled == Some(true),
        match enabled {
            Some(true) => "enabled".into(),
            Some(false) => "disabled (config or session override)".into(),
            None => "registry failed to load".into(),
        },
    );

    print_check(
        "GIPHY_API_KEY",
        giphy::has_api_key(),
        if giphy::has_api_key() {
            "set".into()
        } else {
            "missing — export GIPHY_API_KEY=...".into()
        },
    );

    print_check(
        "registry",
        reg.is_some(),
        match &reg {
            Some(r) => format!("{} events ({})", r.events.len(), source_label()),
            None => "could not parse".into(),
        },
    );

    let has_tty = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .is_ok();
    print_check(
        "terminal (/dev/tty)",
        has_tty,
        if has_tty {
            format!("ok — rendering via {}", render::detected_protocol())
        } else {
            "no controlling terminal (inline GIFs will be skipped in hooks)".into()
        },
    );

    match giphy::newest_cache_age() {
        Some(age) => print_check(
            "gif cache",
            true,
            format!("freshest entry {}s old", age.as_secs()),
        ),
        None => print_check(
            "gif cache",
            true,
            "empty (will populate on first event)".into(),
        ),
    }

    let auto_on = reg.as_ref().map(|r| r.settings.auto).unwrap_or(false);
    let model_ready = embed::model_cached();
    print_check(
        "auto mode",
        !auto_on || model_ready,
        match (auto_on, model_ready) {
            (false, _) => "disabled".into(),
            (true, true) => format!("on — model cached ({})", embed::MODEL_ID),
            (true, false) => {
                "on, but model not downloaded — run `claude-memes find \"test\"` once".into()
            }
        },
    );

    println!(
        "\nConfig:   {}",
        Registry::config_path()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    );
    println!("Cache:    {}", config::cache_dir().display());
    Ok(())
}

fn print_check(label: &str, ok: bool, detail: String) {
    let mark = if ok { "✓" } else { "✗" };
    println!("  {mark} {label:<22} {detail}");
}

fn source_label() -> &'static str {
    match Registry::config_path() {
        Some(p) if p.exists() => "installed config",
        _ => "embedded defaults",
    }
}

// ----- install -------------------------------------------------------------

fn cmd_install(print_only: bool) -> Result<()> {
    let exe = std::env::current_exe().context("locating own binary path")?;
    let exe = exe.to_string_lossy().to_string();

    let settings_path = home_dir()
        .context("HOME not set")?
        .join(".claude")
        .join("settings.json");

    let mut settings: serde_json::Value = if settings_path.exists() {
        let raw = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("reading {}", settings_path.display()))?;
        serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    patch_settings(&mut settings, &exe);

    if print_only {
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }

    // Seed the on-disk registry so users can edit queries without rebuilding.
    if let Some(cfg_path) = Registry::config_path() {
        if !cfg_path.exists() {
            if let Some(parent) = cfg_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&cfg_path, Registry::default_toml())
                .with_context(|| format!("writing registry to {}", cfg_path.display()))?;
            println!("seeded registry → {}", cfg_path.display());
        } else {
            println!("kept existing registry → {}", cfg_path.display());
        }
    }

    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    if settings_path.exists() {
        // Timestamped so repeated installs never clobber an earlier backup.
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let backup = settings_path.with_extension(format!("json.bak-{ts}"));
        std::fs::copy(&settings_path, &backup).ok();
    }
    std::fs::write(
        &settings_path,
        serde_json::to_string_pretty(&settings)? + "\n",
    )
    .with_context(|| format!("writing {}", settings_path.display()))?;

    println!("patched hooks → {}", settings_path.display());
    println!("\nDone. Set your key and try it:\n    export GIPHY_API_KEY=...\n    claude-memes play tests_all_pass");
    Ok(())
}

/// Idempotently insert our hook entries, removing any prior `claude-memes` ones.
fn patch_settings(settings: &mut serde_json::Value, exe: &str) {
    use serde_json::json;

    if !settings.is_object() {
        *settings = json!({});
    }
    let root = settings.as_object_mut().unwrap();
    let hooks = root.entry("hooks").or_insert_with(|| json!({}));
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let hooks = hooks.as_object_mut().unwrap();

    // Remove only our own hook entries (command shaped like "…/claude-memes hook …"),
    // never a whole group: foreign hooks sharing a group must survive a re-install.
    let is_ours = |h: &serde_json::Value| {
        h.get("command")
            .and_then(|c| c.as_str())
            .map(|c| c.contains("claude-memes hook"))
            .unwrap_or(false)
    };
    let strip = |arr: &mut serde_json::Value| {
        if let Some(list) = arr.as_array_mut() {
            for group in list.iter_mut() {
                if let Some(hs) = group.get_mut("hooks").and_then(|h| h.as_array_mut()) {
                    hs.retain(|h| !is_ours(h));
                }
            }
            list.retain(|group| {
                group
                    .get("hooks")
                    .and_then(|h| h.as_array())
                    .map(|hs| !hs.is_empty())
                    .unwrap_or(true)
            });
        }
    };

    let post = hooks.entry("PostToolUse").or_insert_with(|| json!([]));
    strip(post);
    if let Some(list) = post.as_array_mut() {
        list.push(json!({"matcher":"Bash","hooks":[{"type":"command","command":format!("{exe} hook bash"),"timeout":5000}]}));
        list.push(json!({"matcher":"Skill","hooks":[{"type":"command","command":format!("{exe} hook skill"),"timeout":5000}]}));
    }

    let stop = hooks.entry("Stop").or_insert_with(|| json!([]));
    strip(stop);
    if let Some(list) = stop.as_array_mut() {
        list.push(json!({"hooks":[{"type":"command","command":format!("{exe} hook stop"),"timeout":5000}]}));
    }
}

// ----- small helpers -------------------------------------------------------

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
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

/// Local hour [0,23] (system timezone). None on failure.
fn local_hour() -> Option<u32> {
    local_weekday_hour().map(|(_, h)| h)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_is_idempotent() {
        let mut s = serde_json::json!({});
        patch_settings(&mut s, "/usr/bin/claude-memes");
        patch_settings(&mut s, "/usr/bin/claude-memes");
        let post = s["hooks"]["PostToolUse"].as_array().unwrap();
        // Exactly Bash + Skill, not doubled.
        assert_eq!(post.len(), 2);
        let stop = s["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
    }

    #[test]
    fn patch_preserves_foreign_hooks() {
        let mut s = serde_json::json!({
            "hooks": {"PostToolUse": [
                {"matcher":"Bash","hooks":[{"type":"command","command":"/other/tool"}]}
            ]}
        });
        patch_settings(&mut s, "/usr/bin/claude-memes");
        let post = s["hooks"]["PostToolUse"].as_array().unwrap();
        // foreign + Bash + Skill
        assert_eq!(post.len(), 3);
        assert!(post
            .iter()
            .any(|g| g["hooks"][0]["command"] == "/other/tool"));
    }

    #[test]
    fn patch_preserves_foreign_hooks_sharing_a_group() {
        // A group mixing a foreign hook with an old claude-memes entry must keep
        // the foreign hook; only our entry is removed on re-install.
        let mut s = serde_json::json!({
            "hooks": {"PostToolUse": [
                {"matcher":"Bash","hooks":[
                    {"type":"command","command":"/other/tool"},
                    {"type":"command","command":"/old/path/claude-memes hook bash"}
                ]}
            ]}
        });
        patch_settings(&mut s, "/usr/bin/claude-memes");
        let post = s["hooks"]["PostToolUse"].as_array().unwrap();
        // mixed group (now foreign-only) + our Bash + our Skill
        assert_eq!(post.len(), 3);
        let mixed = &post[0]["hooks"].as_array().unwrap();
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0]["command"], "/other/tool");
    }

    #[test]
    fn patch_does_not_strip_lookalike_commands() {
        // A command that merely contains "claude-memes" as a path fragment is
        // not ours and must survive.
        let mut s = serde_json::json!({
            "hooks": {"PostToolUse": [
                {"matcher":"Bash","hooks":[
                    {"type":"command","command":"/opt/claude-memes-utils/helper"}
                ]}
            ]}
        });
        patch_settings(&mut s, "/usr/bin/claude-memes");
        let post = s["hooks"]["PostToolUse"].as_array().unwrap();
        assert_eq!(post.len(), 3);
        assert_eq!(
            post[0]["hooks"][0]["command"],
            "/opt/claude-memes-utils/helper"
        );
    }

    #[test]
    fn plugin_hooks_json_is_valid_and_calls_real_subcommands() {
        let raw = include_str!("../hooks/hooks.json");
        let v: serde_json::Value = serde_json::from_str(raw).expect("hooks.json must parse");
        let hooks = v.get("hooks").and_then(|h| h.as_object()).unwrap();
        assert!(hooks.contains_key("PostToolUse"));
        assert!(hooks.contains_key("Stop"));
        // Every command must be a claude-memes hook subcommand we actually have.
        for group_list in hooks.values() {
            for group in group_list.as_array().unwrap() {
                for hook in group["hooks"].as_array().unwrap() {
                    let cmd = hook["command"].as_str().unwrap();
                    assert!(cmd.starts_with("claude-memes hook "), "unexpected: {cmd}");
                    let sub = cmd.strip_prefix("claude-memes hook ").unwrap();
                    assert!(
                        ["bash", "skill", "stop", "observe"].contains(&sub),
                        "unknown hook subcommand in hooks.json: {sub}"
                    );
                }
            }
        }
    }

    #[test]
    fn response_text_handles_string_and_object() {
        let p = serde_json::json!({"tool_response": "hello"});
        assert_eq!(response_text(&p), "hello");
        let p = serde_json::json!({"tool_response": {"stdout": "x"}});
        assert!(response_text(&p).contains("stdout"));
    }
}

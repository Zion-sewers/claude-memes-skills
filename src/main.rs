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
mod hook;
mod install;
mod mcp;
mod notify;
mod render;
mod shapes;
mod state;

use anyhow::Result;
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
        kind: hook::HookKind,
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
        Cmd::Hook { kind } => hook::run(kind),
        Cmd::List => cmd_list(),
        Cmd::Doctor => cmd_doctor(),
        Cmd::Install { print } => install::run(print),
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

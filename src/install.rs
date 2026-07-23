//! Classic (non-plugin) installation: patch `~/.claude/settings.json`
//! idempotently and seed the on-disk registry.

use anyhow::{Context, Result};

use crate::config::Registry;

pub fn run(print_only: bool) -> Result<()> {
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

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
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
}

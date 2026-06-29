//! Event registry + settings.
//!
//! The default registry is embedded at compile time, so the binary works the
//! moment it is installed. An optional on-disk copy at
//! `~/.config/claude-memes/events.toml` takes precedence when present, letting
//! users tune queries without rebuilding.

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

/// Embedded default registry — the single source of truth that ships in the binary.
const DEFAULT_REGISTRY: &str = include_str!("../config/events.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Registry {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub events: BTreeMap<String, Event>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Settings {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_rating")]
    pub rating: String,
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u64,
    #[serde(default = "default_width")]
    pub width: u32,
    #[serde(default = "default_loops")]
    pub loops: u32,
    #[serde(default = "default_max_frames")]
    pub max_frames: usize,
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub query: String,
    #[serde(default)]
    pub trigger: String,
}

impl Default for Settings {
    fn default() -> Self {
        // Parse the embedded defaults so there is exactly one place that defines them.
        toml::from_str::<Registry>(DEFAULT_REGISTRY)
            .map(|r| r.settings)
            .unwrap_or(Settings {
                enabled: true,
                rating: default_rating(),
                cooldown_secs: default_cooldown(),
                width: default_width(),
                loops: default_loops(),
                max_frames: default_max_frames(),
                cache_ttl_secs: default_cache_ttl(),
            })
    }
}

fn default_true() -> bool {
    true
}
fn default_rating() -> String {
    "pg-13".to_string()
}
fn default_cooldown() -> u64 {
    60
}
fn default_width() -> u32 {
    38
}
fn default_loops() -> u32 {
    2
}
fn default_max_frames() -> usize {
    48
}
fn default_cache_ttl() -> u64 {
    86_400
}

impl Registry {
    /// Load the on-disk registry if present, otherwise fall back to the embedded default.
    pub fn load() -> Result<Self> {
        match Self::config_path() {
            Some(path) if path.exists() => {
                let raw = std::fs::read_to_string(&path)
                    .with_context(|| format!("reading registry at {}", path.display()))?;
                toml::from_str(&raw)
                    .with_context(|| format!("parsing registry at {}", path.display()))
            }
            _ => Self::embedded(),
        }
    }

    /// The embedded default registry — also used by `install` to seed the on-disk copy.
    pub fn embedded() -> Result<Self> {
        toml::from_str(DEFAULT_REGISTRY).context("parsing embedded registry")
    }

    pub fn default_toml() -> &'static str {
        DEFAULT_REGISTRY
    }

    pub fn event(&self, name: &str) -> Option<&Event> {
        self.events.get(name)
    }

    /// `~/.config/claude-memes/events.toml`, honoring `$XDG_CONFIG_HOME`.
    pub fn config_path() -> Option<PathBuf> {
        config_dir().map(|d| d.join("events.toml"))
    }
}

/// `$XDG_CONFIG_HOME/claude-memes` or `~/.config/claude-memes`.
pub fn config_dir() -> Option<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        if !xdg.is_empty() {
            return Some(PathBuf::from(xdg).join("claude-memes"));
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("claude-memes"))
}

/// `$XDG_CACHE_HOME/claude-memes` or `~/.cache/claude-memes`.
pub fn cache_dir() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        if !xdg.is_empty() {
            return PathBuf::from(xdg).join("claude-memes");
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".cache").join("claude-memes")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_parses() {
        let reg = Registry::embedded().expect("embedded registry must parse");
        assert!(reg.settings.enabled);
        assert!(reg.events.contains_key("tests_all_pass"));
        assert!(reg.events.contains_key("red_phase"));
    }

    #[test]
    fn every_event_has_a_nonempty_query() {
        let reg = Registry::embedded().unwrap();
        for (name, ev) in &reg.events {
            assert!(
                !ev.query.trim().is_empty(),
                "event {name} has an empty query"
            );
        }
    }

    #[test]
    fn settings_defaults_are_sane() {
        let s = Settings::default();
        assert!(s.cooldown_secs > 0);
        assert!(s.width > 0);
        assert!(s.max_frames > 0);
    }
}

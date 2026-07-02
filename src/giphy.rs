//! Giphy API client + on-disk cache.
//!
//! One network dependency, one endpoint: `/v1/gifs/random?tag=<query>`. The GIF
//! bytes are cached per event under the cache dir and reused until the TTL
//! lapses, so a busy session hits the network at most once per event per day.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context, Result};

use crate::config::{cache_dir, Settings};

const API_BASE: &str = "https://api.giphy.com/v1/gifs/random";
const ENV_KEY: &str = "GIPHY_API_KEY";

/// Read the API key from the environment. Kept in one place so `doctor` and the
/// client agree on the contract.
pub fn api_key() -> Result<String> {
    let key = std::env::var(ENV_KEY).unwrap_or_default();
    let key = key.trim().to_string();
    if key.is_empty() {
        return Err(anyhow!(
            "{ENV_KEY} is not set. Get a key at https://developers.giphy.com and export it:\n    export {ENV_KEY}=your_key_here"
        ));
    }
    Ok(key)
}

pub fn has_api_key() -> bool {
    std::env::var(ENV_KEY)
        .map(|k| !k.trim().is_empty())
        .unwrap_or(false)
}

fn gif_cache_path(event: &str) -> PathBuf {
    cache_dir()
        .join("gifs")
        .join(format!("{}.gif", crate::state::sanitize(event)))
}

fn cache_is_fresh(path: &PathBuf, ttl_secs: u64) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if meta.len() == 0 {
        return false;
    }
    if ttl_secs == 0 {
        return true; // 0 == cache forever
    }
    match meta
        .modified()
        .and_then(|m| m.elapsed().map_err(std::io::Error::other))
    {
        Ok(age) => age < Duration::from_secs(ttl_secs),
        Err(_) => true, // unknown mtime: trust the cache rather than refetch
    }
}

/// Return GIF bytes for an event, fetching from Giphy on a cache miss.
pub fn fetch_gif(event: &str, query: &str, settings: &Settings) -> Result<Vec<u8>> {
    let cache_path = gif_cache_path(event);

    if cache_is_fresh(&cache_path, settings.cache_ttl_secs) {
        if let Ok(bytes) = std::fs::read(&cache_path) {
            if !bytes.is_empty() {
                return Ok(bytes);
            }
        }
    }

    let key = api_key()?;
    let url = resolve_gif_url(&key, query, &settings.rating)?;
    let bytes = download(&url).with_context(|| format!("downloading GIF for '{event}'"))?;

    if let Some(parent) = cache_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Write atomically-ish via a temp file so a killed process never leaves a
    // truncated GIF that the cache would later treat as valid.
    let tmp = cache_path.with_extension("gif.part");
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &cache_path);
    }
    Ok(bytes)
}

fn http_client() -> Result<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(concat!("claude-memes/", env!("CARGO_PKG_VERSION")))
        .build()
        .context("building HTTP client")
}

/// Hit `/random` and pull a renderable GIF URL out of the response, preferring
/// smaller renditions for fast terminal rendering.
fn resolve_gif_url(key: &str, query: &str, rating: &str) -> Result<String> {
    let client = http_client()?;
    let resp = client
        .get(API_BASE)
        .query(&[("api_key", key), ("tag", query), ("rating", rating)])
        .send()
        .context("calling Giphy /random")?;

    if !resp.status().is_success() {
        return Err(anyhow!("Giphy returned HTTP {}", resp.status()));
    }

    let body: serde_json::Value = resp.json().context("parsing Giphy JSON")?;
    let data = body
        .get("data")
        .ok_or_else(|| anyhow!("Giphy response has no `data` (empty result for '{query}'?)"))?;

    // /random sometimes nests under `images`, sometimes exposes flat fields.
    let images = data.get("images");
    let url = ["downsized", "fixed_height", "fixed_width", "original"]
        .iter()
        .filter_map(|size| images?.get(size)?.get("url")?.as_str())
        .next()
        .or_else(|| data.get("image_url").and_then(|v| v.as_str()))
        .or_else(|| data.get("image_original_url").and_then(|v| v.as_str()))
        .ok_or_else(|| anyhow!("no GIF URL in Giphy response for '{query}'"))?;

    Ok(url.to_string())
}

fn download(url: &str) -> Result<Vec<u8>> {
    let client = http_client()?;
    let resp = client.get(url).send().context("GET gif bytes")?;
    if !resp.status().is_success() {
        return Err(anyhow!("GIF download returned HTTP {}", resp.status()));
    }
    let bytes = resp.bytes().context("reading gif body")?;
    if bytes.is_empty() {
        return Err(anyhow!("Giphy returned an empty GIF body"));
    }
    Ok(bytes.to_vec())
}

/// Best-effort age of the freshest cached GIF — surfaced by `doctor`.
pub fn newest_cache_age() -> Option<Duration> {
    let dir = cache_dir().join("gifs");
    let entries = std::fs::read_dir(dir).ok()?;
    let mut newest: Option<SystemTime> = None;
    for entry in entries.flatten() {
        if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
            newest = Some(newest.map_or(modified, |n| n.max(modified)));
        }
    }
    newest.and_then(|t| t.elapsed().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_path_cannot_escape_gifs_dir() {
        let path = gif_cache_path("../../tmp/evil");
        let s = path.to_string_lossy();
        assert!(!s.contains(".."), "traversal survived sanitization: {s}");
        assert!(s.ends_with(".gif"));
        // Still anchored under the gifs cache dir.
        assert!(path.starts_with(cache_dir().join("gifs")));
    }
}

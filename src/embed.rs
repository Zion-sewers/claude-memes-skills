//! Local static embeddings via Model2Vec — pure Rust, no ONNX runtime.
//!
//! Measured on potion-base-8M: ~30MB weights (Hugging Face cache), ~80ms warm
//! load, ~50µs per embedding, dim 256. Hooks must never trigger the 30MB
//! download — `load(false)` fails fast when the model isn't cached; only
//! explicit user actions (`find`, the watch daemon) download.

use anyhow::{anyhow, Result};
use model2vec_rs::model::StaticModel;
use std::path::PathBuf;

pub const MODEL_ID: &str = "minishlab/potion-base-8M";

/// Hugging Face hub cache root (mirrors hf-hub's default resolution).
fn hf_cache_root() -> PathBuf {
    if let Ok(home) = std::env::var("HF_HOME") {
        if !home.is_empty() {
            return PathBuf::from(home).join("hub");
        }
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".cache").join("huggingface").join("hub")
}

/// True when the model weights are already on disk.
pub fn model_cached() -> bool {
    let snapshots = hf_cache_root()
        .join(format!("models--{}", MODEL_ID.replace('/', "--")))
        .join("snapshots");
    std::fs::read_dir(snapshots)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false)
}

/// Load the model. With `allow_download = false` (hook context) this fails
/// fast when the weights aren't cached instead of pulling 30MB mid-hook.
pub fn load(allow_download: bool) -> Result<StaticModel> {
    if !allow_download && !model_cached() {
        return Err(anyhow!(
            "embedding model not downloaded yet — run `claude-memes find \"some concept\"` once to set it up"
        ));
    }
    StaticModel::from_pretrained(MODEL_ID, None, None, None)
        .map_err(|e| anyhow!("loading embedding model {MODEL_ID}: {e}"))
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na * nb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_basics() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    }
}

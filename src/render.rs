//! Inline terminal rendering of an animated GIF.
//!
//! Decodes frames with the `image` crate and animates them via `viuer`, which
//! auto-selects the best available protocol: Kitty graphics, iTerm2 inline
//! images, or a truecolor Unicode half-block fallback that works in any modern
//! terminal. No system libraries, no popup window — it draws where Claude Code
//! already is.

use std::io::Cursor;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use image::codecs::gif::GifDecoder;
use image::{AnimationDecoder, DynamicImage};
use viuer::Config;

use crate::config::Settings;

struct Frame {
    image: DynamicImage,
    delay: Duration,
}

/// Per-frame delay is clamped so pathological GIFs (0ms or multi-second frames)
/// still play as a brisk, bounded animation.
const MIN_DELAY: Duration = Duration::from_millis(20);
const MAX_DELAY: Duration = Duration::from_millis(200);

fn decode_frames(bytes: &[u8], max_frames: usize) -> Result<Vec<Frame>> {
    let decoder = GifDecoder::new(Cursor::new(bytes)).context("decoding GIF")?;
    let mut out = Vec::new();
    for frame in decoder.into_frames() {
        let frame = frame.context("reading GIF frame")?;
        let (num, den) = frame.delay().numer_denom_ms();
        let ms = if den == 0 { 100 } else { num / den.max(1) };
        let delay = Duration::from_millis(ms as u64).clamp(MIN_DELAY, MAX_DELAY);
        out.push(Frame {
            image: DynamicImage::ImageRgba8(frame.into_buffer()),
            delay,
        });
        if out.len() >= max_frames {
            break;
        }
    }
    if out.is_empty() {
        return Err(anyhow!("GIF contained no frames"));
    }
    Ok(out)
}

fn config(width: u32, restore_cursor: bool) -> Config {
    Config {
        width: Some(width),
        absolute_offset: false,
        restore_cursor,
        ..Default::default()
    }
}

/// Render the GIF inline. With `loops == 0` (or a single-frame GIF) it draws one
/// still frame; otherwise it animates `loops` times, redrawing in place, then
/// leaves the final frame on screen with the cursor below it.
pub fn play(bytes: &[u8], settings: &Settings) -> Result<()> {
    let frames = decode_frames(bytes, settings.max_frames)?;

    if settings.loops == 0 || frames.len() == 1 {
        viuer::print(&frames[0].image, &config(settings.width, false))
            .context("rendering frame")?;
        return Ok(());
    }

    let anim = config(settings.width, true);
    for _ in 0..settings.loops {
        for frame in &frames {
            viuer::print(&frame.image, &anim).context("rendering frame")?;
            std::thread::sleep(frame.delay);
        }
    }

    // Redraw the last frame without restoring the cursor so following output
    // lands below the image instead of overwriting it.
    let last = &frames[frames.len() - 1];
    viuer::print(&last.image, &config(settings.width, false)).context("rendering final frame")?;
    Ok(())
}

/// Decode-only path: validate that bytes are a renderable GIF and report the
/// frame count, without touching the terminal. Used by `play --check` and CI.
pub fn probe(bytes: &[u8], settings: &Settings) -> Result<usize> {
    Ok(decode_frames(bytes, settings.max_frames)?.len())
}

/// Best-effort label of the graphics protocol viuer will likely use — for `doctor`.
pub fn detected_protocol() -> &'static str {
    if std::env::var_os("KITTY_WINDOW_ID").is_some()
        || std::env::var("TERM")
            .map(|t| t.contains("kitty"))
            .unwrap_or(false)
    {
        "Kitty graphics"
    } else if matches!(
        std::env::var("TERM_PROGRAM").as_deref(),
        Ok("iTerm.app") | Ok("WezTerm")
    ) {
        "iTerm2 inline images"
    } else {
        "Unicode half-blocks (truecolor)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::gif::GifEncoder;
    use image::{Frame, RgbaImage};

    /// Encode a tiny N-frame GIF in memory so decode can be tested headlessly.
    fn synth_gif(frames: u8) -> Vec<u8> {
        let mut buf = Vec::new();
        {
            let mut enc = GifEncoder::new(&mut buf);
            for i in 0..frames {
                let img = RgbaImage::from_pixel(4, 4, image::Rgba([i.wrapping_mul(40), 0, 0, 255]));
                enc.encode_frame(Frame::new(img)).unwrap();
            }
        }
        buf
    }

    #[test]
    fn decodes_all_frames() {
        let gif = synth_gif(3);
        let frames = decode_frames(&gif, 48).expect("should decode synthetic gif");
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn respects_max_frames_cap() {
        let gif = synth_gif(5);
        let frames = decode_frames(&gif, 2).unwrap();
        assert_eq!(frames.len(), 2);
    }

    #[test]
    fn rejects_garbage_bytes() {
        assert!(decode_frames(b"not a gif", 48).is_err());
    }

    #[test]
    fn frame_delays_are_clamped() {
        let frames = decode_frames(&synth_gif(2), 48).unwrap();
        for f in &frames {
            assert!(f.delay >= MIN_DELAY && f.delay <= MAX_DELAY);
        }
    }
}

#!/usr/bin/env bash
# download-memes.sh — Batch download all clips defined in config/memes.json
#
# Reads event URLs and durations from memes.json, downloads each as a short .mp4.
# Skips clips that already exist (safe to re-run after adding new events).
#
# Prerequisites:
#   python3 -m venv ~/.claude/assets/memes/.venv
#   ~/.claude/assets/memes/.venv/bin/pip install yt-dlp

set -euo pipefail

CLAUDE_DIR="${CLAUDE_DIR:-$HOME/.claude}"
MEME_DIR="$CLAUDE_DIR/assets/memes"
CONFIG="$CLAUDE_DIR/config/memes.json"
# yt-dlp: prefer local venv, fall back to system-wide
YTDLP="${YTDLP_BIN:-$MEME_DIR/.venv/bin/yt-dlp}"
command -v "$YTDLP" &>/dev/null || YTDLP=$(command -v yt-dlp 2>/dev/null || echo "")

mkdir -p "$MEME_DIR"

if [ -z "$YTDLP" ] || ! command -v "$YTDLP" &>/dev/null; then
  echo "yt-dlp not found. Run one of:"
  echo "  pip install yt-dlp                                        # system-wide"
  echo "  python3 -m venv $MEME_DIR/.venv && $MEME_DIR/.venv/bin/pip install yt-dlp  # local venv"
  exit 1
fi

if [ ! -f "$CONFIG" ]; then
  echo "Config not found: $CONFIG"
  exit 1
fi

# Parse event → url + duration from JSON config (placeholders flagged separately)
python3 -c "
import json
cfg = json.load(open('$CONFIG'))
for event, data in cfg.get('events', {}).items():
    url = (data.get('url') or '').strip()
    dur = data.get('duration', 5)
    start = data.get('start', 0)
    # Mark placeholders explicitly so the shell side can warn instead of silently failing.
    if not url or 'YOUR_URL_HERE' in url:
        print(f'{event}|__PLACEHOLDER__|{dur}|{start}')
    else:
        print(f'{event}|{url}|{dur}|{start}')
" | while IFS='|' read -r event url duration start; do
  outfile="$MEME_DIR/${event}.mp4"

  if [ "$url" = "__PLACEHOLDER__" ]; then
    echo "NEEDS_URL $event (placeholder in memes.json — fill in via README's clip-sourcing flow)"
    continue
  fi

  if [ -f "$outfile" ]; then
    echo "SKIP $event (already exists)"
    continue
  fi

  echo "DOWNLOADING $event ..."

  # Build time range: start=0 means from beginning, start>0 means mid-video clip
  if [ "${start:-0}" -gt 0 ]; then
    END=$((start + duration))
    SECTIONS="*${start}-${END}"
  else
    SECTIONS="*0-${duration}"
  fi

  # Try best quality ≤480p first, fall back to any format
  "$YTDLP" \
    --format 'bestvideo[height<=480][ext=mp4]+bestaudio[ext=m4a]/best[height<=480][ext=mp4]/best' \
    --merge-output-format mp4 \
    --download-sections "$SECTIONS" \
    --force-keyframes-at-cuts \
    --no-playlist \
    --quiet \
    --output "$outfile" \
    "$url" 2>&1 || {
      echo "  Retrying with simpler format..."
      "$YTDLP" \
        --format 'best[height<=480]' \
        --download-sections "$SECTIONS" \
        --no-playlist \
        --quiet \
        --output "$outfile" \
        "$url" 2>&1 || echo "  SKIP: $event (download failed)"
    }

  [ -f "$outfile" ] && echo "  OK: $(du -h "$outfile" | cut -f1) $event"
done

echo ""
echo "=== Downloaded clips ==="
du -sh "$MEME_DIR"/*.mp4 2>/dev/null | sort -h || echo "No clips found"
echo ""
echo "Total: $(du -sh "$MEME_DIR" 2>/dev/null | cut -f1)"

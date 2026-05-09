#!/usr/bin/env bash
# play-meme.sh — Core playback engine
# Usage: play-meme.sh <event-name>
#
# Plays ~/.claude/assets/memes/<event>.mp4 via ffplay popup.
# - 60-second per-clip cooldown (anti-spam)
# - Reads enabled flag from ~/.claude/config/memes.json
# - Fire-and-forget (disowned background job)

EVENT="${1:-test}"
MEME_DIR="${CLAUDE_DIR:-$HOME/.claude}/assets/memes"
CONFIG="${CLAUDE_DIR:-$HOME/.claude}/config/memes.json"
CLIP="$MEME_DIR/${EVENT}.mp4"
COOLDOWN_DIR="/tmp/.claude-meme-cooldown"

# Check enabled (config file, then session-level override)
ENABLED=0
if [ -f "$CONFIG" ]; then
  ENABLED=$(python3 -c "import json; print(1 if json.load(open('$CONFIG')).get('enabled',False) else 0)" 2>/dev/null || echo 0)
fi
[ -f /tmp/.claude-meme-disabled ] && ENABLED=0
[ "$ENABLED" = "1" ] || exit 0

# Clip must exist
[ -f "$CLIP" ] || exit 0

# 60-second cooldown per clip
mkdir -p "$COOLDOWN_DIR" 2>/dev/null
LAST_FILE="$COOLDOWN_DIR/$EVENT"
if [ -f "$LAST_FILE" ]; then
  LAST=$(cat "$LAST_FILE" 2>/dev/null || echo 0)
  NOW=$(date +%s)
  DIFF=$((NOW - LAST))
  [ "$DIFF" -lt 60 ] && exit 0
fi
date +%s > "$LAST_FILE"

# Increment session play counter (used by status line integrations)
COUNTER_FILE="/tmp/.claude-meme-count"
COUNT=$(cat "$COUNTER_FILE" 2>/dev/null || echo 0)
[[ "$COUNT" =~ ^[0-9]+$ ]] || COUNT=0
echo $((COUNT + 1)) > "$COUNTER_FILE"

# Log: timestamp + event name
LOG_FILE="/tmp/.claude-meme-log"
echo "$(date +%s) ${EVENT}" >> "$LOG_FILE"

# Play: 400×260 window, auto-close when done, quiet logging
ffplay -autoexit \
  -window_title "Meme" \
  -x 400 -y 260 \
  -loglevel quiet \
  "$CLIP" &

disown 2>/dev/null
exit 0

#!/usr/bin/env bash
# play-random-meme.sh — Pick and play a random clip from a set
# Usage: play-random-meme.sh clip1 clip2 clip3
#
# Randomly selects one event name from the arguments and delegates to play-meme.sh.
# Used for events with multiple valid responses (deploy success, no-op diffs, etc.)

PLAY="${CLAUDE_DIR:-$HOME/.claude}/bin/play-meme.sh"
CLIPS=("$@")
[ ${#CLIPS[@]} -eq 0 ] && exit 0
PICK="${CLIPS[$((RANDOM % ${#CLIPS[@]}))]}"
"$PLAY" "$PICK"

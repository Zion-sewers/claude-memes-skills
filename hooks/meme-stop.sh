#!/usr/bin/env bash
# meme-stop.sh — Stop event hook
#
# Fires on every Claude Code Stop event (when the model finishes a turn).
# Detects session-level signals: workflow phase, time of day, desperation, scope.
# Registered in ~/.claude/settings.json under hooks.Stop.

PLAY="${CLAUDE_DIR:-$HOME/.claude}/bin/play-meme.sh"
RANDOM_PLAY="${CLAUDE_DIR:-$HOME/.claude}/bin/play-random-meme.sh"
ONCE_DIR="/tmp/.claude-meme-once"

CONFIG="${CLAUDE_DIR:-$HOME/.claude}/config/memes.json"
ENABLED=$(python3 -c "import json; print(1 if json.load(open('$CONFIG')).get('enabled',False) else 0)" 2>/dev/null || echo 0)
[ -f /tmp/.claude-meme-disabled ] && ENABLED=0
[ "$ENABLED" = "1" ] || { echo '{}'; exit 0; }
mkdir -p "$ONCE_DIR" 2>/dev/null

# Helper: play a clip at most once per session (prevents spam on every turn)
fire_once() {
  local event="$1"
  [ -f "$ONCE_DIR/$event" ] && return 1
  touch "$ONCE_DIR/$event"
  shift
  if [ $# -gt 0 ]; then
    "$RANDOM_PLAY" "$@" &
  else
    "$PLAY" "$event" &
  fi
  return 0
}

# --- Session start (first message) ---
MSG_COUNT=$(cat /tmp/.claude-msg-count 2>/dev/null || echo "0")
if [ "${MSG_COUNT:-0}" -le 1 ] 2>/dev/null; then
  fire_once warcraft_work
fi

# --- Workflow phase detection ---
# Write to /tmp/.claude-workflow-phase from your TDD scripts to drive these:
#   echo "RED" > /tmp/.claude-workflow-phase
#   echo "GREEN" > /tmp/.claude-workflow-phase
#   echo "REFACTOR" > /tmp/.claude-workflow-phase
PHASE=$(cat /tmp/.claude-workflow-phase 2>/dev/null || echo "")
PREV_PHASE=$(cat /tmp/.claude-workflow-phase-prev 2>/dev/null || echo "")

case "$PHASE" in
  REFACTOR) "$PLAY" refactor_loop & ;;
esac

# Transition out of REFACTOR → refactor complete
if [ "$PREV_PHASE" = "REFACTOR" ] && [ "$PHASE" != "REFACTOR" ] && [ -n "$PHASE" ]; then
  "$PLAY" refactor_complete &
fi

[ -n "$PHASE" ] && echo "$PHASE" > /tmp/.claude-workflow-phase-prev

# --- Session ending with failing tests ---
# Set by meme-post-bash.sh when tests fail
LAST_TEST=$(cat /tmp/.claude-meme-last-test 2>/dev/null || echo "")
if [ "$LAST_TEST" = "fail" ]; then
  fire_once not_prepared
fi

# --- Time-of-day signals ---
HOUR=$(date +%-H)
DOW=$(date +%u)  # 1=Mon ... 5=Fri

if [ "$DOW" = "5" ] && [ "$HOUR" -ge 16 ]; then
  fire_once friday_deploy           # Friday afternoon deploy warning
fi

if [ "$HOUR" -ge 22 ] || [ "$HOUR" -le 5 ]; then
  fire_once end_of_day              # Late night / early morning session
fi

# --- Desperation (too many turns without resolution) ---
# MSG_COUNT written by a session-msg-counter hook (see docs/settings-snippet.json)
if [ "${MSG_COUNT:-0}" -ge 20 ] 2>/dev/null; then
  fire_once desperation_cooldown    # "This is fine"
fi

# --- Scope too big (large diff = likely scope creep) ---
DIFF_LINES=$(git diff --stat HEAD 2>/dev/null | tail -1 | grep -oE '[0-9]+ insertion' | grep -oE '[0-9]+' || echo "0")
if [ "${DIFF_LINES:-0}" -ge 500 ] 2>/dev/null; then
  fire_once scope_too_big scope_too_big erlich_bachman
fi

# --- Overloaded (too many active tasks) ---
# Requires a task tracking system that writes /tmp/.claude-tasks-* files
TASK_COUNT=$(ls /tmp/.claude-tasks-* 2>/dev/null | wc -l)
if [ "${TASK_COUNT:-0}" -ge 4 ] 2>/dev/null; then
  fire_once fly_you_fools
fi

echo '{}'
exit 0

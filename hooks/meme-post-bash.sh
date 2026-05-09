#!/usr/bin/env bash
# meme-post-bash.sh — PostToolUse hook for Bash tool
#
# Reads Claude Code tool JSON from stdin, detects workflow events, fires clips.
# Registered in ~/.claude/settings.json under hooks.PostToolUse[matcher=Bash].
#
# Detects: test results (with transition tracking), CI polling, deploy, PR, lint, git ops.

INPUT=$(cat)
PLAY="${CLAUDE_DIR:-$HOME/.claude}/bin/play-meme.sh"
RANDOM_PLAY="${CLAUDE_DIR:-$HOME/.claude}/bin/play-random-meme.sh"
LAST_TEST="/tmp/.claude-meme-last-test"

# Quick exit if memes disabled (check config OR session-level temp file)
CONFIG="${CLAUDE_DIR:-$HOME/.claude}/config/memes.json"
ENABLED=$(python3 -c "import json; print(1 if json.load(open('$CONFIG')).get('enabled',False) else 0)" 2>/dev/null || echo 0)
[ -f /tmp/.claude-meme-disabled ] && ENABLED=0
[ "$ENABLED" = "1" ] || { echo '{}'; exit 0; }

COMMAND=$(printf '%s' "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tool_input',{}).get('command',''))" 2>/dev/null || echo "")
RESPONSE=$(printf '%s' "$INPUT" | python3 -c "import sys,json; r=json.load(sys.stdin).get('tool_response',''); print(r[:2000] if isinstance(r,str) else '')" 2>/dev/null || echo "")

[ -z "$COMMAND" ] && { echo '{}'; exit 0; }

# --- Test results ---
# Tracks pass/fail transitions for hydra (consecutive fail) and matrix_dodge (recovery)
if printf '%s' "$COMMAND" | grep -qiE '(pytest|vitest|bun test|bun run test)'; then
  PREV=$(cat "$LAST_TEST" 2>/dev/null || echo "none")
  if printf '%s' "$RESPONSE" | grep -qiE '(FAILED|FAIL |failed)'; then
    if [ "$PREV" = "fail" ]; then
      "$PLAY" hydra &         # Consecutive failure: cut one head, two grow back
    else
      "$PLAY" red_phase &     # First failure: Thanos "back to me"
    fi
    echo "fail" > "$LAST_TEST"
  elif printf '%s' "$RESPONSE" | grep -qiE '(passed|tests? passed|PASS)'; then
    if [ "$PREV" = "fail" ]; then
      "$PLAY" matrix_dodge &  # Recovery from failure: the narrow save
    else
      "$PLAY" tests_all_pass & # Clean pass: LeBron "too easy"
    fi
    echo "pass" > "$LAST_TEST"
  fi

# --- CI pipeline polling ---
elif printf '%s' "$RESPONSE" | grep -qiE 'completed.*succeeded'; then
  "$PLAY" full_pipeline_green &

elif printf '%s' "$RESPONSE" | grep -qiE 'completed.*failed'; then
  "$PLAY" ci_build_fail &

# --- Deploy success ---
elif printf '%s' "$RESPONSE" | grep -qiE 'deploy.*succe'; then
  "$PLAY" ci_deploy_success &

# --- PR creation ---
elif printf '%s' "$COMMAND" | grep -qiE 'gh pr create'; then
  "$PLAY" ship_pr_created &

# --- Codex / AI review tool invoked ---
elif printf '%s' "$COMMAND" | grep -qiE '\bcodex\b'; then
  "$PLAY" codex_invoke &

# --- Coverage run ---
elif printf '%s' "$COMMAND" | grep -qiE '(--cov\b|coverage)'; then
  "$PLAY" coverage_check &

# --- Lint clean (no errors in output) ---
elif printf '%s' "$COMMAND" | grep -qiE '(ruff check|eslint|bun run lint)'; then
  if ! printf '%s' "$RESPONSE" | grep -qiE '(error|warning|problem|FAIL)'; then
    "$PLAY" noice &
  fi

# --- Git push success ---
elif printf '%s' "$COMMAND" | grep -qiE 'git push'; then
  if ! printf '%s' "$RESPONSE" | grep -qiE '(rejected|error|fatal)'; then
    "$PLAY" noice &
  fi

# --- No-op diff / working tree clean ---
elif printf '%s' "$COMMAND" | grep -qiE '(git commit|git diff|git status)'; then
  if printf '%s' "$RESPONSE" | grep -qiE '(nothing to commit.*working tree clean|no changes added|working tree clean)'; then
    "$RANDOM_PLAY" jian_yang jian_yang_v3 &
  fi

# --- Long-running commands (install, build, start) ---
elif printf '%s' "$COMMAND" | grep -qiE '(docker build|docker compose up|uv sync|bun install|func start)'; then
  "$RANDOM_PLAY" warcraft_zagzag zug_test1 zug_test2 zug_test3 &
fi

echo '{}'
exit 0

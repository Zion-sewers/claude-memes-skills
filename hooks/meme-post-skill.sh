#!/usr/bin/env bash
# meme-post-skill.sh — PostToolUse hook for Skill tool
#
# Maps Claude Code skill invocations to meme clips.
# Registered in ~/.claude/settings.json under hooks.PostToolUse[matcher=Skill].
#
# Add your own skills to the case statement below.

INPUT=$(cat)
PLAY="${CLAUDE_DIR:-$HOME/.claude}/bin/play-meme.sh"
RANDOM_PLAY="${CLAUDE_DIR:-$HOME/.claude}/bin/play-random-meme.sh"

CONFIG="${CLAUDE_DIR:-$HOME/.claude}/config/memes.json"
ENABLED=$(python3 -c "import json; print(1 if json.load(open('$CONFIG')).get('enabled',False) else 0)" 2>/dev/null || echo 0)
[ -f /tmp/.claude-meme-disabled ] && ENABLED=0
[ "$ENABLED" = "1" ] || { echo '{}'; exit 0; }

SKILL=$(printf '%s' "$INPUT" | python3 -c "import sys,json; print(json.load(sys.stdin).get('tool_input',{}).get('skill',''))" 2>/dev/null || echo "")

[ -z "$SKILL" ] && { echo '{}'; exit 0; }

case "$SKILL" in
  brainstorming)      "$PLAY" brainstorm_start & ;;    # Planning phase begins
  deep-research)      "$PLAY" deep_research & ;;       # Research mode
  end-session)        "$PLAY" end_session & ;;          # Session cleanup
  heidegger-reflect)  "$PLAY" reflect & ;;             # Philosophical reflection
  review)             "$PLAY" review_perfect & ;;      # Code review pass
  commit-push-pr)     "$PLAY" ship_pr_created & ;;     # PR shipped
  reground)           "$PLAY" naruto_sage & ;;         # Context reload
  agent-team)         "$PLAY" avengers_assemble & ;;   # Multi-agent spawn
  red-team)           "$PLAY" red_phase & ;;           # Red-team audit
  pre-ship-clean)     "$PLAY" verify & ;;              # Pre-ship verification
  simplify)           "$PLAY" matrix_there_is_no_spoon & ;; # Refactor
  deploy-prod)        "$RANDOM_PLAY" ci_deploy_success jian_yang_ceo & ;; # Deploy
  loop|schedule)      "$PLAY" squidward_future & ;;    # Recurring task scheduled
  *)                  "$PLAY" skill_loaded & ;;         # Any other skill
esac

echo '{}'
exit 0

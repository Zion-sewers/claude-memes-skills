# claude-meme-hooks

> Event-driven meme clips for Claude Code. Short video popups fire on test results, CI events, skill invocations, and workflow phases — making the RED/GREEN/REFACTOR cycle something you look forward to.

![License](https://img.shields.io/badge/license-MIT-blue)
![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20WSL2%20%7C%20macOS-lightgrey)
![Shell](https://img.shields.io/badge/shell-bash%204%2B-green)

---

## What It Does

Hooks into Claude Code's lifecycle events and plays a matching 4–8 second `ffplay` popup:

| Workflow event | Clip |
|----------------|------|
| First test failure | Thanos "back to me" |
| Consecutive failures (fail → fail) | Hydra "cut one head, two grow back" |
| Recovery (fail → pass) | Matrix bullet dodge |
| All tests pass | LeBron "that's too easy" |
| CI deploy succeeds | "Smells like a promotion" |
| Git diff ≥ 500 lines | HIMYM slap bet |
| Turn count ≥ 20 (stuck) | This Is Fine dog |
| Friday after 16:00 + deploy | Warning clip |
| `/agent-team` skill invoked | Avengers assemble |

Each clip plays in a 400×260 popup, fire-and-forget, with a 60-second per-clip cooldown. Zero workflow interruption.

---

## Architecture

```
Claude Code lifecycle
  │
  ├─ PostToolUse[Bash]   → hooks/meme-post-bash.sh   ─┐
  ├─ PostToolUse[Skill]  → hooks/meme-post-skill.sh  ─┤→ bin/play-meme.sh → ffplay popup
  └─ Stop                → hooks/meme-stop.sh         ─┘
                                                           ↓
                                                 ~/.claude/assets/memes/<event>.mp4
                                                 ~/.claude/config/memes.json  (event registry)
```

Three hooks feed into one playback engine driven by a single JSON config.

---

## Requirements

| Tool | Min version | Install |
|------|-------------|---------|
| `ffplay` | ffmpeg 4.0+ | `apt install ffmpeg` / `brew install ffmpeg` |
| `yt-dlp` | 2024.1+ | `pip install yt-dlp` |
| `python3` | 3.8+ | pre-installed on most systems |
| `bash` | 4.0+ | pre-installed on Linux/macOS |

**WSL2:** `ffplay` requires a display server. Set `DISPLAY=:0` (tested with X410 and VcXsrv).

---

## Installation

### 1. Clone and copy files

```bash
git clone https://github.com/shovalbe/claude-meme-hooks
cd claude-meme-hooks

# Create required directories
mkdir -p ~/.claude/bin ~/.claude/hooks ~/.claude/config ~/.claude/assets/memes

# Copy scripts
cp bin/play-meme.sh bin/play-random-meme.sh ~/.claude/bin/
cp hooks/meme-*.sh ~/.claude/hooks/
cp config/memes.json ~/.claude/config/

chmod +x ~/.claude/bin/play-meme.sh ~/.claude/bin/play-random-meme.sh
chmod +x ~/.claude/hooks/meme-*.sh
```

### 2. Register hooks in `~/.claude/settings.json`

Add to the `hooks` key (see `docs/settings-snippet.json` for a full example):

```json
{
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Bash",
        "hooks": [{ "type": "command", "command": "/home/YOUR_USER/.claude/hooks/meme-post-bash.sh", "timeout": 1000 }]
      },
      {
        "matcher": "Skill",
        "hooks": [{ "type": "command", "command": "/home/YOUR_USER/.claude/hooks/meme-post-skill.sh", "timeout": 1000 }]
      }
    ],
    "Stop": [
      { "hooks": [{ "type": "command", "command": "/home/YOUR_USER/.claude/hooks/meme-stop.sh", "timeout": 1000 }] }
    ]
  }
}
```

> Use absolute paths. Claude Code does not expand `~` in hook command strings.

### 3. Download clips

```bash
# Install yt-dlp into a local venv
python3 -m venv ~/.claude/assets/memes/.venv
~/.claude/assets/memes/.venv/bin/pip install yt-dlp

# Batch-download all clips defined in memes.json
~/.claude/bin/download-memes.sh
```

### 4. Enable

```bash
# Enable for all sessions (edit config)
# Open ~/.claude/config/memes.json and set "enabled": true

# OR enable just for the current shell session
touch /tmp/.claude-meme-enabled

# Disable for current session without touching config
touch /tmp/.claude-meme-disabled
```

### 5. Verify

```bash
~/.claude/bin/play-meme.sh tests_all_pass
```

A 4-second LeBron clip should pop up. If nothing happens, check that `/tmp/.claude-meme-enabled` exists and the clip downloaded successfully.

---

## How It Works

### Event detection (`meme-post-bash.sh`)

The hook reads the tool JSON from stdin and pattern-matches on `tool_input.command` and `tool_response`:

```bash
# Transition tracking: writes "pass" or "fail" to /tmp/.claude-meme-last-test
PREV=$(cat /tmp/.claude-meme-last-test 2>/dev/null || echo "none")

if grep -qiE '(FAILED|FAIL )' <<< "$RESPONSE"; then
    [ "$PREV" = "fail" ] && "$PLAY" hydra || "$PLAY" red_phase
    echo "fail" > /tmp/.claude-meme-last-test
elif grep -qiE '(passed|PASS)' <<< "$RESPONSE"; then
    [ "$PREV" = "fail" ] && "$PLAY" matrix_dodge || "$PLAY" tests_all_pass
    echo "pass" > /tmp/.claude-meme-last-test
fi
```

The fail → pass transition is what triggers `matrix_dodge` instead of the standard `tests_all_pass`. State is cheap: one temp file per signal.

### Skill mapping (`meme-post-skill.sh`)

Reads `tool_input.skill` from the hook JSON and dispatches via `case`:

```bash
case "$SKILL" in
  brainstorming)     "$PLAY" brainstorm_start ;;
  deep-research)     "$PLAY" deep_research ;;
  heidegger-reflect) "$PLAY" reflect ;;
  review)            "$PLAY" review_perfect ;;
  agent-team)        "$PLAY" avengers_assemble ;;
  deploy-prod)       "$RANDOM_PLAY" ci_deploy_success jian_yang_ceo ;;
  *)                 "$PLAY" skill_loaded ;;
esac
```

### Session signals (`meme-stop.sh`)

Runs on every `Stop` event. Checks multiple independent signals, each firing at most once per session via `/tmp/.claude-meme-once/<event>`:

| Signal | Source | Clip |
|--------|--------|------|
| First turn | `/tmp/.claude-msg-count` ≤ 1 | `warcraft_work` |
| Phase = REFACTOR | `/tmp/.claude-workflow-phase` | `refactor_loop` |
| REFACTOR → other | phase transition | `refactor_complete` |
| Last test = FAIL at Stop | `/tmp/.claude-meme-last-test` | `not_prepared` |
| Friday ≥ 16:00 | `date` | `friday_deploy` |
| After 22:00 / before 05:00 | `date` | `end_of_day` |
| Turn count ≥ 20 | `/tmp/.claude-msg-count` | `desperation_cooldown` |
| Diff ≥ 500 lines | `git diff --stat HEAD` | `scope_too_big` |
| 4+ active tasks | `/tmp/.claude-tasks-*` count | `fly_you_fools` |

### Cooldown & state files

| Path | Written by | Read by | Purpose |
|------|-----------|---------|---------|
| `/tmp/.claude-meme-enabled` | user / session hook | all hooks | master enable switch |
| `/tmp/.claude-meme-disabled` | user | `play-meme.sh` | session-level override |
| `/tmp/.claude-meme-cooldown/<event>` | `play-meme.sh` | `play-meme.sh` | 60s per-clip throttle |
| `/tmp/.claude-meme-once/<event>` | `meme-stop.sh` | `meme-stop.sh` | fire-once-per-session |
| `/tmp/.claude-meme-last-test` | `meme-post-bash.sh` | `meme-post-bash.sh` | fail→pass transition |
| `/tmp/.claude-workflow-phase` | external TDD scripts | `meme-stop.sh` | current phase (RED/GREEN/REFACTOR) |
| `/tmp/.claude-msg-count` | session-msg-counter hook | `meme-stop.sh` | turn counter |

> The `session-msg-counter` hook (not included) must write an integer to `/tmp/.claude-msg-count` on each Stop event for desperation/session-start detection to work.

---

## Adding Your Own Clips

### 1. Find the right YouTube video

Describe the emotion to Claude:

> "I need a 4–5 second clip that captures the feeling of 'every fix creates a new problem'. What's the most recognizable pop culture moment for that?"

Claude will suggest specific scenes with timestamps (e.g., MCU Hydra scene, Whack-a-Mole). For clips inside longer videos, note the exact timestamp range.

### 2. Register in `config/memes.json`

For a YouTube Short (clip starts at 0:00):
```json
"my_event": {
  "name": "Ron Burgundy - Kind Of A Big Deal",
  "url": "https://www.youtube.com/watch?v=XXXX",
  "duration": 5,
  "trigger": "When X happens",
  "quote": "I'm kind of a big deal"
}
```

For a clip that starts mid-video, add a `start` offset in seconds:
```json
"matrix_dodge": {
  "url": "https://www.youtube.com/watch?v=aHWriz1hBIY",
  "start": 312,
  "duration": 5,
  "trigger": "Test recovery (fail → pass)"
}
```

`download-memes.sh` computes `--download-sections "*{start}-{start+duration}"` automatically.

### 3. Add a trigger

In `hooks/meme-post-bash.sh`:
```bash
elif printf '%s' "$COMMAND" | grep -qiE 'your-command-pattern'; then
  "$PLAY" my_event &
fi
```

In `hooks/meme-post-skill.sh`:
```bash
my-skill-name) "$PLAY" my_event & ;;
```

### 4. Download and test

```bash
~/.claude/bin/download-memes.sh   # skips existing clips
~/.claude/bin/play-meme.sh my_event
```

See `docs/clip-sourcing.md` for the full Claude-assisted sourcing workflow.

---

## Generalizing to Other Tools

The playback engine and JSON registry are tool-agnostic. Only the hook scripts are Claude Code-specific.

**To adapt to a git post-commit hook:**

```bash
#!/usr/bin/env bash
# .git/hooks/post-commit
PLAY="$HOME/.claude/bin/play-meme.sh"
MSG=$(git log -1 --pretty=%s)
printf '%s' "$MSG" | grep -qiE '^feat' && "$PLAY" ship_pr_created &
printf '%s' "$MSG" | grep -qiE '^fix'  && "$PLAY" noice &
```

**To adapt to any CI/CD script:**

```bash
# Write workflow phase from your pipeline
echo "REFACTOR" > /tmp/.claude-workflow-phase

# Or play directly
~/.claude/bin/play-meme.sh ci_deploy_success
```

The only contract is: `~/.claude/assets/memes/<event>.mp4` exists + `~/.claude/config/memes.json` has `"enabled": true`.

---

## Clip Sourcing

See `docs/clip-sourcing.md` for the full workflow. The short version:

1. Describe the emotion to Claude → get a specific scene + timestamp suggestion
2. Add the YouTube URL and timestamp to `memes.json`
3. Run `download-memes.sh` — it trims to `start`/`duration` via yt-dlp `--download-sections`
4. Test with `play-meme.sh <event>`

Most clips are YouTube Shorts (start at 0:00, no trimming needed). About 25% of clips need a `start` offset for mid-video moments.

> Clips are not redistributed in this repo. The `memes.json` ships with YouTube URLs; you download locally. Ensure compliance with YouTube ToS for personal use.

---

## License

[MIT](LICENSE)

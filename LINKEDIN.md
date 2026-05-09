# LinkedIn Post Draft

---

I made TDD fun by wiring Claude Code to play meme clips.

Tests fail → Thanos appears.
Tests pass after 3 failures → Matrix bullet dodge.
CI goes green → "Smells like a promotion in here."
Large diff on a Friday → slap bet fires immediately.

49 clips total. Each one plays in a 400×260 popup, disappears, and gets out of your way.

---

**How it works:**

Claude Code has a hooks system — shell scripts that fire on lifecycle events like `PostToolUse` and `Stop`. I wrote three hook scripts that read the tool JSON from stdin, pattern-match on commands and outputs, and call `ffplay` with the right clip.

The event registry is a single `memes.json` — event name, YouTube URL, duration in seconds. A `download-memes.sh` script batch-downloads everything via yt-dlp.

For clips that needed exact trimming (e.g., "the 4 seconds where Gandalf says 'fly you fools'"), I described what I wanted to Claude: *"I need a 4-5 second clip that captures the feeling of a session becoming completely unmanageable."* Claude suggested specific scenes and timestamps. yt-dlp's `--download-sections "*1:23-1:28"` handled the actual cut.

Whole sourcing pass for 49 clips took ~3 hours.

---

**Why this matters beyond the memes:**

The hooks system is the interesting part. The same pattern works for:
- Alerting when a diff exceeds 500 lines (scope creep signal)
- Detecting turn count ≥ 20 (the AI is stuck — different signal from "working hard")
- Firing on Friday afternoon deploys (behavioral nudge, not a block)
- Tracking workflow phase transitions (RED → GREEN → REFACTOR) from CI/CD pipelines

It's a feedback layer that makes invisible workflow states visible without requiring any change to how you actually work.

---

The full system is open-sourced: [github.com/shovalbe/claude-meme-hooks]

Includes the hook scripts, download automation, clip sourcing workflow, and a memes.json template with event metadata (you supply the URLs).

---

**Tags:** #claudecode #developertools #tdd #devex #aitools

---

## Alternative shorter version (280 chars for X/Twitter)

I wired Claude Code's hook system to play meme clips:
- Test fails → Thanos
- Recovery → Matrix dodge  
- Large diff → slap bet
- Friday deploy → warning clip

3 hooks + 1 JSON config + yt-dlp for trimming. 49 clips, fire-and-forget, 60s cooldown.

Open source:

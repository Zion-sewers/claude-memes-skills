# LinkedIn Post Draft

---

I rewrote my "Claude Code plays memes when your tests fail" project from scratch. It's now a single Rust binary that renders GIFs **inline in your terminal**.

Tests fail → Thanos.
Pass after 3 failures → Matrix bullet dodge.
CI goes green → "Smells like a promotion in here."
500-line diff → slap bet.

---

**Why the rewrite?**

The first version worked but wasn't something I'd call production. It downloaded YouTube clips with yt-dlp (fragile, ToS-gray, half the URLs were unfilled placeholders), played them in an `ffplay` popup (desktop-only — invisible the moment you run Claude Code on web or in the cloud), and leaned on ffmpeg + a Python venv + a 150 MB embedding model for what was really just a lookup table.

So I tore it down to the studs.

**The new stack:**
- **Giphy API** instead of YouTube. One key, one endpoint, GIFs resolved on demand and cached. No downloads, no placeholders, legally clean.
- **One Rust binary** instead of seven bash scripts. Small, fully tested.
- **Inline terminal rendering** instead of a popup. It auto-detects Kitty graphics, iTerm2 inline images, or falls back to truecolor Unicode half-blocks — so it shows up in whatever terminal Claude Code is living in.

The neat trick: Claude Code captures a hook's stdout to parse it as JSON, so you can't just print escape codes from a hook and expect a GIF. The binary spawns its renderer pointed at `/dev/tty` — the actual controlling terminal — bypassing the harness. No tty (a cloud session)? It silently skips. Hooks never break your workflow.

---

**Why this matters beyond the memes:**

The interesting part is still the hooks layer. Pure, unit-tested event detection turns invisible workflow states into signals:
- diff > 500 lines → scope-creep nudge
- turn count ≥ 20 → the agent is stuck (a different signal from "working hard")
- fail → pass transition → you just recovered, here's your dopamine

It's a feedback layer on top of an agentic coding loop, and it ships with CI that runs a live Giphy smoke test against a repo secret on every push.

Open source, MIT: github.com/ShovalBenjer/claude-memes-skills

# CLAUDE OS — claude-memes

Status: ACTIVE (living document, the spine)
Date: 2026-07-31
Owner: Shoval Benjer. Maintainer: the claude-memes session.
Repo: github.com/ShovalBenjer/claude-memes-skills — canonical home.

This file MERGES and SUPERSEDES every prior setup plan for this repo.
Predecessors remain as history/reference only; none is authoritative.

---

## 1. Mission

A single Rust binary that renders event-driven meme GIFs inline in the Claude
Code terminal. Every hook event maps to a curated Giphy query; the binary fetches
the GIF, decodes it, and renders it to `/dev/tty` so it lands in the real terminal
even when Claude Code captures the hook's stdout.

Three extension targets beyond Claude Code:
1. **VS Code Copilot agent mode** — auto-detects this repo's plugin format.
2. **OpenAI Codex CLI** — copies `adapters/codex/hooks.json` to `~/.codex/`.
3. **Cursor** — copies `adapters/cursor/hooks.json` to `.cursor/` (observe-only).
4. **Org agents / MCP** — `claude-memes mcp` serves `find_meme` + `play_meme` over stdio.

---

## 2. The Layers

### L0 — SDLC Kernel
- **Intent contract first.** Substantive task → acceptance checklist.
- **Loop, don't hand back.** Full time/token budget; speed is only a virtue on
  explicit quick fixes.
- **Done has a format.** Verification evidence (command + output) + intent-coverage
  statement (covered / uncovered + why). No coverage table = not done.
- **Confidence gates.** ≥0.90 autonomous; 0.70–0.90 work + explicit proof gate;
  <0.70 ask or spawn reviewer.
- **Privacy by design.** Hooks record field names only — never prompts, commands,
  or tool output. Auto embedding runs locally (model2vec-rs, 30MB); concepts never
  leave the machine.

### L1 — Governance & Identity
- One repository, one plugin manifest (`.claude-plugin/plugin.json`).
- Secrets: `GIPHY_API_KEY` never read/echoed/committed; live via `secrets.GIPHY_API_KEY`.
- PII: hook payload fields are logged as names only; no embedding of session text.
- License: MIT. Codex = independent reviewer only (executor role dead).

### L2 — Memory, Continuity & Comms Copilot
- **Event registry as spine:** `config/events.toml` + `config/hubs.toml` are the
  sole source of truth for meme mappings. The binary ships an embedded copy; the
  installed copy at `~/.config/claude-memes/events.toml` takes precedence.
- **Auto-memory:** `claude-memes dream` replays the delivery log and prints
  ready-to-paste registry snippets for recurring auto hubs.
- **Session signals:** the Stop hook counts its own turns per Claude Code session
  id (first turn → `session_start`, ≥ 20 turns → `desperation`), checks git diff
  size, and knows the local time. No external counter hooks needed.

### L3 — Orchestration
- One binary, one config directory. No daemon unless `claude-memes watch` is
  explicitly started.
- Adapters are additive: install by copying JSON, no rebuild needed.
- MCP server is stdio-bound; any MCP gateway can plug into it.

### L4 — I/O
- Inbound: Claude Code hooks (`PostToolUse`, `Stop`, `Notification`, `PostToolUseFailure`).
- Outbound: `/dev/tty` inline GIF + desktop notification (OSC 9 / OSC 777).
- Watch daemon: hooks hand events over localhost to the rendering pane.

### L5 — Quality Fabric
- **a2a ⇄ GitHub PR review sync**: two independent reviews (Claude /code-review
  + Codex via a2a-call) → agreement gate → `gh pr review` comment with
  `_provenance` + audit.jsonl traceback; disagree → digest for Shoval.
- Eval gates (promptfoo/deepeval) non-blocking until trusted; deterministic checks
  ALWAYS before LLM judges.
- Skills estate: every skill owned by a persona or archived; unwired = defect.

### L6 — Project Portfolio
- Registry: every repo → owner persona, standards score, deploy story, docs
  compliance. Active: claude-setup, claude-memes-skills. Dormant with wake
  triggers: seekapa legacy (archived), Kith (Apple fee → OneSignal Expo path ready).

### L7 — Platform & Machine
- Rust 1.82+, edition 2021.
- Release matrix: Linux x86_64/arm64, macOS Intel/Apple Silicon.
- SQLite everywhere with WAL; survives reboots.
- Always-on-PC health for the watch daemon (power settings, wake).

### L8 — Frontier
- **Mojuco** (adversarial sim-verify + sim-to-real calibration) and **artifixer**
  (opacity-masked generation: high=byte-faithful, low=marked interpolation,
  zero=kill) generalized as OS-wide patterns.
- **Latent/vector path:** every inter-agent edge declares transport=text_json today;
  swappable to embedding/latent later without topology redesign.
- **Learning-card emitter:** any session that ships work using a concept files a
  card (concept, where used, mastery dimensions) to the learning platform queue.

---

## 3. Build Sequence

- **P0 (now):** repo structure enforced (CODEOWNERS, TODO.md, this file).
- **P1:** integration tests for watch daemon + coverage report.
- **P2:** adapter maturity (Cursor schema, Codex compatibility, MCP protocol docs).
- **P3:** crates.io publish + Sigstore signing.
- **P4:** auto-mode hub expansion + offline fallback.

## 4. Acceptance (PRD table)

| # | Criterion | Status |
|---|---|---|
| 1 | Single Rust binary, no ffmpeg/venv/LanceDB | DONE |
| 2 | Inline GIF in Kitty / iTerm2 / Unicode half-blocks | DONE |
| 3 | Auto mode with local embedding (30MB static) | DONE |
| 4 | Watch daemon for full inline delivery | DONE |
| 5 | Plugin manifest for Claude Code + VS Code Copilot | DONE |
| 6 | Adapters for Codex CLI + Cursor | PARTIAL (Cursor observe-only) |
| 7 | MCP server (find_meme + play_meme) | DONE |
| 8 | Coverage + benchmarks | TODO |
| 9 | crates.io + Sigstore release signing | TODO |

## 5. Supersession table

| Predecessor | Disposition |
|---|---|
| bash + yt-dlp + ffmpeg project | Superseded. Goals preserved: production-clean, dependency-light, visible everywhere Claude Code runs. |

# claude-memes

> Event-driven meme GIFs for Claude Code. Tests fail → Thanos. All green → LeBron. Big diff → slap bet. One small Rust binary that fetches the right GIF from Giphy and renders it **inline in your terminal** — no popups, no yt-dlp, no ffmpeg.

![CI](https://github.com/ShovalBenjer/claude-memes-skills/actions/workflows/ci.yml/badge.svg)
![Rust](https://img.shields.io/badge/rust-1.82%2B-orange)
![License](https://img.shields.io/badge/license-MIT-blue)

---

## What it does

Claude Code fires shell hooks on lifecycle events (`PostToolUse`, `Stop`). `claude-memes` registers itself as those hooks, reads the tool payload, matches it to an event, and pops the matching GIF straight into your terminal.

| Workflow event | Clip |
|----------------|------|
| First test failure | Thanos — "this does put a smile on my face" |
| Consecutive failures (fail → fail) | Hydra — "cut one head, two grow back" |
| Recovery (fail → pass) | Matrix bullet dodge |
| All tests pass | LeBron — "that's too easy" |
| CI deploy succeeds | "Smells like a promotion" |
| Git diff ≥ 500 lines | HIMYM slap bet |
| Turn count ≥ 20 (stuck) | This Is Fine dog |
| `/deep-research`, `/agent-team`, … | per-skill clips |

Full list: `claude-memes list`.

## Auto mode

Beyond the 22 curated events, **auto mode** (on by default) handles the long
tail: any unmapped workflow moment — a weird build failure, an exotic tool, an
unlisted skill — is embedded locally and matched against ~40 *emotion hubs*
(`config/hubs.toml`), each carrying a curated Giphy query pool:

```
$ claude-memes find "npm ERR! ERESOLVE unable to resolve dependency tree"
→  0.536  dependency_hell   tangled in christmas lights | domino chain reaction fail
```

- **Local + private**: a 30MB static-embedding model ([Model2Vec
  potion-base-8M](https://huggingface.co/minishlab/potion-base-8M)), downloaded
  once on first `claude-memes find` — ~50µs per match, no API calls, concepts
  never leave your machine. Hooks never trigger the download.
- **Gated hard**: similarity threshold (calibrated 0.28) → per-hub novelty
  (1/hour) → hourly budget (`auto_per_hour = 6`). A boring `ls` never reaches
  the model at all.
- **Off switch**: `auto = false` in the registry.

---

## Why it's built this way

This is a ground-up rewrite of the original bash + yt-dlp + ffmpeg project. The goals were **production-clean, dependency-light, and visible everywhere Claude Code runs.**

| Old | New |
|-----|-----|
| YouTube clips via `yt-dlp` (fragile, ToS-gray, half the URLs were placeholders) | **Giphy API** — one key, on-demand search, legally clean |
| `ffplay` popup window (desktop-only, invisible on web/cloud) | **Inline terminal GIF** via Kitty / iTerm2 / Unicode half-blocks |
| ffmpeg + a venv + LanceDB + a 150 MB embedding model | **One static Rust binary**, ~3 crates of real surface area |
| 7 bash scripts + JSON config | `claude-memes` + one TOML registry |

Rendering goes to `/dev/tty`, so the GIF lands in your real terminal even though Claude Code captures the hook's stdout. No controlling terminal (a cloud session)? It silently skips — hooks never break your session.

---

## Install

### 1. Get the binary

Grab a prebuilt binary from [Releases](https://github.com/ShovalBenjer/claude-memes-skills/releases) (Linux x86_64/arm64, macOS Intel/Apple Silicon) and drop it on your `PATH`, or:

```bash
cargo binstall claude-memes     # prebuilt, via cargo-binstall

# or build from source
git clone https://github.com/ShovalBenjer/claude-memes-skills
cd claude-memes-skills
cargo install --path .          # installs `claude-memes` to ~/.cargo/bin
```

### 2. Wire up the hooks

**As a Claude Code plugin (recommended):** this repo is a plugin
(`.claude-plugin/plugin.json` + `hooks/hooks.json`) — add it via your plugin
marketplace flow and the hooks register themselves, merging non-invasively
with any hooks you already have. The same plugin format is auto-detected by
VS Code Copilot agent mode.

**Or classic install:**

```bash
claude-memes install            # patches ~/.claude/settings.json + seeds the registry
```

This is idempotent and backs up your settings first. Preview without writing:

```bash
claude-memes install --print
```

### 2½. How memes reach your eyes

Modern Claude Code runs hooks with **no controlling terminal**, so delivery is
tiered by capability:

1. **`claude-memes watch`** running in any terminal pane → full inline GIFs
   land there (hooks hand events to it over localhost).
2. No daemon → a **desktop notification** via the hook `terminalSequence`
   field (OSC 9 by default; set `notify = "osc777"` for Ghostty/Warp, or
   `"off"`). Requires Claude Code ≥ 2.1.141.
3. Older Claude Code with a hook tty → legacy direct inline rendering still works.

`claude-memes play <event>` always renders inline — it runs in your own terminal.

### 3. Set your Giphy key

Grab a free key at [developers.giphy.com](https://developers.giphy.com), then:

```bash
export GIPHY_API_KEY=your_key_here     # add to ~/.bashrc / ~/.zshrc
```

### 4. Verify

```bash
claude-memes doctor                    # checks key, terminal, registry, cache
claude-memes play tests_all_pass       # should pop a LeBron GIF in your terminal
```

---

## Terminal support

Rendering auto-selects the best available protocol — you don't configure anything:

| Terminal | Protocol |
|----------|----------|
| Kitty, WezTerm | Kitty graphics (crisp) |
| iTerm2 | iTerm2 inline images |
| anything else (truecolor) | Unicode half-blocks |

`claude-memes doctor` tells you which one you'll get.

---

## Configuration

The event registry ships embedded in the binary and is seeded to
`~/.config/claude-memes/events.toml` on install. Edit it to retune any query —
no rebuild needed.

```toml
[settings]
enabled = true
rating = "pg-13"        # giphy content rating: g | pg | pg-13 | r
cooldown_secs = 60      # per-event throttle
width = 38              # render width in terminal cells
loops = 2               # animation loops (0 = single still frame)

[events.tests_all_pass]
query = "lebron james thats too easy"
trigger = "Full test suite passes (all green)"
```

Add your own event by copying a block and pointing a hook at it.

### Muting

```bash
touch  "${TMPDIR:-/tmp}/claude-memes/disabled"   # off for this machine/session
rm     "${TMPDIR:-/tmp}/claude-memes/disabled"    # back on
```

Or set `enabled = false` in the registry for a permanent off.

---

## Commands

```
claude-memes play <event> [--force] [--check]   render an event (--check = fetch+decode only)
claude-memes hook <bash|skill|stop|observe>     hook entry point (reads tool JSON on stdin)
claude-memes watch [--port N]                   render daemon: hook memes play in this terminal
claude-memes shapes                             observed hook payload shapes (field names only)
claude-memes list                               list events + queries
claude-memes doctor                             diagnostics
claude-memes install [--print]                  patch settings.json + seed registry
```

> Privacy note: `hook observe` and shape logging record **field names only** —
> never prompts, commands, or tool output.

---

## How it works

```
Claude Code lifecycle
  ├─ PostToolUse[Bash]   → claude-memes hook bash  ┐
  ├─ PostToolUse[Skill]  → claude-memes hook skill ┤→ detect event → spawn `play` → /dev/tty
  └─ Stop                → claude-memes hook stop  ┘                      │
                                                       Giphy /random (cached) → decode → render
```

- **Detection** (`src/detect.rs`) — pure functions map a tool payload to an event, including fail→fail / fail→pass transition tracking. Fully unit-tested.
- **Session signals** — the Stop hook counts its own turns per Claude Code session id (first turn → `session_start`, ≥ 20 turns → `desperation`), checks the git diff size, and knows what time it is. No external counter hooks needed.
- **Giphy** (`src/giphy.rs`) — one `/random` call per event, cached to `~/.cache/claude-memes/gifs/` with a TTL so a tight loop hits the network at most once a day.
- **Render** (`src/render.rs`) — decodes GIF frames with the `image` crate and animates them via `viuer`.
- **State** (`src/state.rs`) — per-event cooldown + fire-once-per-session, all under a temp dir.

---

## Development

```bash
cargo test            # unit + integration tests
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

CI runs fmt + clippy + tests on every push, plus a live Giphy smoke test
(`play --check`) using the `GIPHY_API_KEY` repository secret.

---

## License

[MIT](LICENSE)

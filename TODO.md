# TODO — claude-memes

One TODO, grouped by concern. Fresh session? Read CLAUDE-OS.md first.

## Core
- [ ] Cargo.lock should be committed (currently tracked but verify CI produces no drift)
- [ ] Add integration tests for `watch` daemon startup/shutdown
- [ ] Add benchmark for render latency (currently unmeasured per README)
- [ ] Coverage report: 60 tests over 14 files is a count, not a percentage — add `cargo tarpaulin`

## Auto mode
- [ ] Expand `config/hubs.toml` emotion hub coverage beyond the current ~40
- [ ] Document auto-mode calibration procedure (threshold + novelty tuning)
- [ ] Add offline fallback for `claude-memes find` when network is unavailable

## Adapters
- [ ] Cursor adapter: wire detection against Cursor's actual hook schema (currently observe-only)
- [ ] OpenAI Codex CLI adapter: verify schema compatibility beyond the current mirror assumption
- [ ] MCP: document `find_meme` and `play_meme` stdio protocol for external MCP gateways

## Packaging & release
- [ ] Add `aarch64-apple-darwin` and `x86_64-unknown-linux-gnu` to `release.yml` matrix (already present, verify)
- [ ] Sign release tarballs with Sigstore/cosign
- [ ] Publish `claude-memes` to crates.io

## CI / workflow
- [ ] Add `actions/checkout@v4` to all workflows — verified in ci.yml, claude-code-review.yml, release.yml
- [ ] Add `claude-nightly.yml` for scheduled dependency freshness checks
- [ ] Gate `cargo fmt --check` + `cargo clippy -- -D warnings` as required checks on PRs

## P0 — Truth & hygiene
- [ ] Verify no secrets/API keys in any committed file
- [ ] Rotate `GIPHY_API_KEY` if it ever appears in config or hooks
- [ ] Ensure `hooks/hooks.json` matchers stay current with Claude Code hook schema changes

## DONE
- [x] Plugin manifest `.claude-plugin/plugin.json` added
- [x] VS Code Copilot agent mode compatibility documented
- [x] Inline terminal GIF rendering via Kitty / iTerm2 / Unicode half-blocks
- [x] Auto mode with local embedding (model2vec potion-base-8M)
- [x] Watch daemon for full inline GIF delivery
- [x] Desktop notification fallback (OSC 9 / OSC 777)

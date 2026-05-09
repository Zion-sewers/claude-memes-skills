#!/usr/bin/env bash
# startup.sh — End-to-end installer for claude-meme-hooks.
#
# Idempotent: safe to re-run. Patches ~/.claude/settings.json without clobbering
# existing hooks. Backs up any file it overwrites.
#
# Usage:
#   ./startup.sh                       # full install (deps check, copy, hooks, download, vectordb)
#   ./startup.sh --no-download         # skip yt-dlp clip downloads
#   ./startup.sh --no-vectordb         # skip Imgflip → LanceDB seed
#   ./startup.sh --no-hooks            # don't patch ~/.claude/settings.json
#   ./startup.sh --bundle              # also build claude-meme-hooks-<date>.zip
#   ./startup.sh --prefix /custom/dir  # install under /custom/dir instead of ~/.claude
#   ./startup.sh --dry-run             # print actions without executing
#
# Exit codes: 0 ok, 1 hard failure, 2 partial (some optional steps skipped).

set -euo pipefail

# ---------- constants & flags ----------

PROJECT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
CLAUDE_DIR="${CLAUDE_DIR:-$HOME/.claude}"
DRY_RUN=0
DO_DOWNLOAD=1
DO_VECTORDB=1
DO_HOOKS=1
DO_BUNDLE=0
PARTIAL=0
INSTALL_MODE=""   # "copy" or "link"; auto-detected if empty

log()  { printf '\033[1;36m[meme-install]\033[0m %s\n' "$*" >&2; }
warn() { printf '\033[1;33m[meme-install][warn]\033[0m %s\n' "$*" >&2; PARTIAL=1; }
die()  { printf '\033[1;31m[meme-install][error]\033[0m %s\n' "$*" >&2; exit 1; }
run()  { if [ "$DRY_RUN" = "1" ]; then printf '  + %s\n' "$*" >&2; else eval "$@"; fi; }

usage() {
  sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
  exit 0
}

while [ $# -gt 0 ]; do
  case "$1" in
    --no-download) DO_DOWNLOAD=0 ;;
    --no-vectordb) DO_VECTORDB=0 ;;
    --no-hooks)    DO_HOOKS=0 ;;
    --bundle)      DO_BUNDLE=1 ;;
    --copy)        INSTALL_MODE="copy" ;;
    --link)        INSTALL_MODE="link" ;;
    --prefix)      shift; CLAUDE_DIR="$1" ;;
    --dry-run)     DRY_RUN=1 ;;
    -h|--help)     usage ;;
    *) die "unknown flag: $1 (try --help)" ;;
  esac
  shift
done

# ---------- 1. dependency check ----------

check_deps() {
  log "checking dependencies"
  local missing=()
  for bin in bash python3 jq ffplay; do
    command -v "$bin" >/dev/null 2>&1 || missing+=("$bin")
  done
  if [ ${#missing[@]} -gt 0 ]; then
    die "missing required tools: ${missing[*]}.  apt install jq ffmpeg python3"
  fi

  local bash_major
  bash_major=$(bash -c 'echo "${BASH_VERSINFO[0]}"')
  [ "$bash_major" -ge 4 ] || die "bash 4+ required (got $bash_major)"

  if [ -n "${WSL_DISTRO_NAME:-}" ] && [ -z "${DISPLAY:-}" ]; then
    warn "running under WSL with no DISPLAY set — ffplay popups won't render"
  fi

  log "deps ok"
}

# ---------- 2. directory layout ----------

create_dirs() {
  log "creating directory layout under $CLAUDE_DIR"
  for d in bin hooks config assets/memes cache/lancedb cache/memes; do
    run "mkdir -p \"$CLAUDE_DIR/$d\""
  done
}

# ---------- 3. file install ----------

install_file() {
  local src="$1" dst="$2"
  run "chmod +x \"$src\""

  # Already a symlink back to src → user is in link mode, leave it alone.
  if [ -L "$dst" ]; then
    local target
    target=$(readlink -f "$dst" 2>/dev/null || echo "")
    if [ "$target" = "$(readlink -f "$src" 2>/dev/null)" ]; then
      log "  symlinked already: $dst -> $target"
      return 0
    fi
  fi

  if [ "$INSTALL_MODE" = "link" ]; then
    [ -e "$dst" ] && run "rm -f \"$dst\""
    run "ln -s \"$src\" \"$dst\""
    log "  linked: $dst -> $src"
    return 0
  fi

  # copy mode (default for portability)
  if [ -f "$dst" ] && ! cmp -s "$src" "$dst"; then
    run "cp \"$dst\" \"$dst.bak-$(date +%Y%m%d-%H%M%S)\""
  fi
  run "cp \"$src\" \"$dst\""
  run "chmod +x \"$dst\""
}

# Auto-detect install mode from existing files (one symlink → user wants link mode).
detect_mode() {
  if [ -n "$INSTALL_MODE" ]; then return 0; fi
  for probe in \
      "$CLAUDE_DIR/bin/play-meme.sh" \
      "$CLAUDE_DIR/hooks/meme-post-bash.sh" \
      "$CLAUDE_DIR/hooks/meme-stop.sh"; do
    if [ -L "$probe" ]; then
      local t; t=$(readlink -f "$probe" 2>/dev/null || echo "")
      case "$t" in
        "$PROJECT_DIR"/*) INSTALL_MODE="link"; return 0 ;;
      esac
    fi
  done
  INSTALL_MODE="copy"
}

install_files() {
  detect_mode
  log "installing scripts + hooks (mode=$INSTALL_MODE)"

  install_file "$PROJECT_DIR/bin/play-meme.sh"          "$CLAUDE_DIR/bin/play-meme.sh"
  install_file "$PROJECT_DIR/bin/play-random-meme.sh"   "$CLAUDE_DIR/bin/play-random-meme.sh"
  install_file "$PROJECT_DIR/scripts/download-memes.sh" "$CLAUDE_DIR/bin/download-memes.sh"
  install_file "$PROJECT_DIR/scripts/seed-meme-vectordb.py" "$CLAUDE_DIR/bin/seed-meme-vectordb.py"

  install_file "$PROJECT_DIR/hooks/meme-post-bash.sh"   "$CLAUDE_DIR/hooks/meme-post-bash.sh"
  install_file "$PROJECT_DIR/hooks/meme-post-skill.sh"  "$CLAUDE_DIR/hooks/meme-post-skill.sh"
  install_file "$PROJECT_DIR/hooks/meme-stop.sh"        "$CLAUDE_DIR/hooks/meme-stop.sh"

  # Don't clobber a user-edited memes.json — only install if missing.
  if [ ! -f "$CLAUDE_DIR/config/memes.json" ]; then
    run "cp \"$PROJECT_DIR/config/memes.json\" \"$CLAUDE_DIR/config/memes.json\""
    log "installed default memes.json"
  else
    log "preserved existing $CLAUDE_DIR/config/memes.json (not overwritten)"
  fi
}

# ---------- 4. python venv + yt-dlp ----------

setup_venv() {
  local venv="$CLAUDE_DIR/assets/memes/.venv"
  local pip="$venv/bin/pip"

  if [ ! -x "$venv/bin/python3" ]; then
    log "creating venv at $venv"
    run "python3 -m venv \"$venv\""
  else
    log "venv already exists at $venv"
  fi

  log "installing/upgrading yt-dlp"
  run "\"$pip\" install --quiet --upgrade pip"
  run "\"$pip\" install --quiet --upgrade yt-dlp"

  if [ "$DO_VECTORDB" = "1" ]; then
    log "installing lancedb + sentence-transformers (~150MB, one-time)"
    if ! run "\"$pip\" install --quiet lancedb sentence-transformers"; then
      warn "lancedb/sentence-transformers install failed — will skip vectordb seed"
      DO_VECTORDB=0
    fi
  fi
}

# ---------- 5. settings.json hook patch ----------

patch_settings() {
  local settings="$HOME/.claude/settings.json"
  local hookbase="$CLAUDE_DIR/hooks"

  log "patching $settings (idempotent: removes any prior meme-* entries first)"

  if [ ! -f "$settings" ]; then
    run "echo '{}' > \"$settings\""
  fi

  run "cp \"$settings\" \"$settings.bak-$(date +%Y%m%d-%H%M%S)\""

  if [ "$DRY_RUN" = "1" ]; then
    printf '  + jq patch on %s\n' "$settings" >&2
    return 0
  fi

  local tmp
  tmp=$(mktemp)
  jq \
    --arg postbash  "$hookbase/meme-post-bash.sh" \
    --arg postskill "$hookbase/meme-post-skill.sh" \
    --arg stop      "$hookbase/meme-stop.sh" \
    '
    # Ensure the containers exist with their expected types.
    .hooks //= {}
    | .hooks.PostToolUse //= []
    | .hooks.Stop //= []

    # Drop any previously-registered meme hooks (match by command-path substring "meme-").
    | .hooks.PostToolUse |= (
        map(
          if (.hooks // []) | any(.command? // "" | tostring | test("meme-"))
          then empty else .
          end
        )
      )
    | .hooks.Stop |= (
        map(
          if (.hooks // []) | any(.command? // "" | tostring | test("meme-"))
          then empty else .
          end
        )
      )

    # Append our entries.
    | .hooks.PostToolUse += [
        {"matcher":"Bash","hooks":[{"type":"command","command":$postbash,"timeout":1000}]},
        {"matcher":"Skill","hooks":[{"type":"command","command":$postskill,"timeout":1000}]}
      ]
    | .hooks.Stop += [
        {"hooks":[{"type":"command","command":$stop,"timeout":1000}]}
      ]
    ' "$settings" > "$tmp"

  mv "$tmp" "$settings"
  log "settings.json patched"
}

# ---------- 6. download clips ----------

download_clips() {
  log "downloading clips via yt-dlp (placeholders flagged as NEEDS_URL)"
  run "YTDLP_BIN=\"$CLAUDE_DIR/assets/memes/.venv/bin/yt-dlp\" \"$CLAUDE_DIR/bin/download-memes.sh\"" || {
    warn "download-memes.sh exited non-zero — some clips may be missing"
  }
}

# ---------- 7. seed vectordb ----------

seed_vectordb() {
  local py="$CLAUDE_DIR/assets/memes/.venv/bin/python3"
  log "seeding meme-template vectordb (Imgflip → LanceDB)"
  if ! run "\"$py\" \"$CLAUDE_DIR/bin/seed-meme-vectordb.py\""; then
    warn "vectordb seed failed — JSON fallback may still exist at ~/.claude/cache/memes/templates.json"
  fi
}

# ---------- 8. bundle zip ----------

bundle_zip() {
  command -v zip >/dev/null 2>&1 || die "zip not installed (apt install zip)"
  local out="$PROJECT_DIR/dist/claude-meme-hooks-$(date +%Y%m%d).zip"
  run "mkdir -p \"$PROJECT_DIR/dist\""
  log "building bundle: $out"

  local stage
  stage=$(mktemp -d)
  run "mkdir -p \"$stage/claude-meme-hooks\""

  # Project source (excludes per .claudeignore + dist/)
  for item in bin hooks config scripts docs README.md LICENSE LINKEDIN.md startup.sh .gitignore .claudeignore; do
    [ -e "$PROJECT_DIR/$item" ] && run "cp -r \"$PROJECT_DIR/$item\" \"$stage/claude-meme-hooks/\""
  done

  # Downloaded mp4 clips, if any
  if compgen -G "$CLAUDE_DIR/assets/memes/*.mp4" > /dev/null; then
    run "mkdir -p \"$stage/claude-meme-hooks/assets/memes\""
    run "cp \"$CLAUDE_DIR/assets/memes\"/*.mp4 \"$stage/claude-meme-hooks/assets/memes/\""
  else
    warn "no .mp4 clips at $CLAUDE_DIR/assets/memes/ — bundle will contain source only"
  fi

  ( cd "$stage" && zip -qr "$out" claude-meme-hooks )
  run "rm -rf \"$stage\""

  log "bundle ready: $out  ($(du -h "$out" | cut -f1))"
}

# ---------- 9. verify ----------

verify() {
  log "verification:"
  local ok=1
  for f in \
    "$CLAUDE_DIR/bin/play-meme.sh" \
    "$CLAUDE_DIR/bin/play-random-meme.sh" \
    "$CLAUDE_DIR/bin/download-memes.sh" \
    "$CLAUDE_DIR/bin/seed-meme-vectordb.py" \
    "$CLAUDE_DIR/hooks/meme-post-bash.sh" \
    "$CLAUDE_DIR/hooks/meme-post-skill.sh" \
    "$CLAUDE_DIR/hooks/meme-stop.sh" \
    "$CLAUDE_DIR/config/memes.json"; do
    if [ -e "$f" ]; then printf '  ✓ %s\n' "$f" >&2
    else                 printf '  ✗ %s\n' "$f" >&2; ok=0
    fi
  done

  local clip_count
  clip_count=$(find "$CLAUDE_DIR/assets/memes" -maxdepth 1 -name '*.mp4' 2>/dev/null | wc -l)
  printf '  ⚑ clips downloaded: %s\n' "$clip_count" >&2

  if [ -f "$CLAUDE_DIR/cache/memes/templates.json" ]; then
    local n
    n=$(python3 -c 'import json,sys;print(len(json.load(open(sys.argv[1]))))' "$CLAUDE_DIR/cache/memes/templates.json" 2>/dev/null || echo "?")
    printf '  ⚑ vectordb seed records: %s\n' "$n" >&2
  fi

  [ "$ok" = "1" ] || die "missing files — install incomplete"
}

# ---------- main ----------

log "claude-meme-hooks installer  (project=$PROJECT_DIR  prefix=$CLAUDE_DIR  dry_run=$DRY_RUN)"

check_deps
create_dirs
install_files
setup_venv
[ "$DO_HOOKS"     = "1" ] && patch_settings  || log "skipping hook patch (--no-hooks)"
[ "$DO_DOWNLOAD" = "1" ] && download_clips  || log "skipping downloads (--no-download)"
[ "$DO_VECTORDB" = "1" ] && seed_vectordb   || log "skipping vectordb seed (--no-vectordb)"
verify
[ "$DO_BUNDLE"   = "1" ] && bundle_zip      || true

log "done"
exit $([ "$PARTIAL" = "1" ] && echo 2 || echo 0)

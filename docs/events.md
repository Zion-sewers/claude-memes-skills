# Event Reference

All event names that `play-meme.sh` recognizes. Each maps to `~/.claude/assets/memes/<event>.mp4`.

## Test & CI Events

| Event | Trigger | Suggested Clip |
|-------|---------|---------------|
| `red_phase` | First test failure in a run | Thanos "fun is not something one considers..." |
| `hydra` | Consecutive test failures (fail → fail) | MCU Hydra "cut one head, two grow back" |
| `matrix_dodge` | Test recovery (fail → pass transition) | Matrix bullet dodge |
| `tests_all_pass` | All tests pass (clean run) | LeBron "that's too easy" |
| `coverage_check` | `pytest --cov` or coverage command | Thanos "perfectly balanced" |
| `ci_build_fail` | CI pipeline: `completed.failed` | WoW Leeroy Jenkins |
| `full_pipeline_green` | CI pipeline: `completed.succeeded` | Any celebration |
| `ci_deploy_success` | Deploy stage succeeds | "Smells like promotion" |
| `not_prepared` | Session ends with last test = FAIL | WoW "You are not prepared" |

## Git & PR Events

| Event | Trigger | Suggested Clip |
|-------|---------|---------------|
| `ship_pr_created` | `gh pr create` succeeds | Silicon Valley "this guy fucks" |
| `noice` | Lint clean / `git push` success | Brooklyn Nine-Nine "noice" |
| `jian_yang` | `git status` = nothing to commit | Silicon Valley Jian Yang |
| `jian_yang_v3` | Same (random pool with `jian_yang`) | Same |

## Skill Invocation Events

| Event | Skill | Suggested Clip |
|-------|-------|---------------|
| `brainstorm_start` | `/brainstorming` | Any "beginning of a plan" scene |
| `deep_research` | `/deep-research` | Watchmen "locked in here with me" |
| `reflect` | `/heidegger-reflect` | Contemplative scene |
| `review_perfect` | `/review` | Ronaldo SIUUU |
| `avengers_assemble` | `/agent-team` | Avengers Endgame assemble |
| `naruto_sage` | `/reground` | Naruto sage mode activation |
| `verify` | `/pre-ship-clean` | Breaking Bad "say my name" |
| `matrix_there_is_no_spoon` | `/simplify` | Matrix "there is no spoon" |
| `squidward_future` | `/loop`, `/schedule` | Squidward future meme |
| `end_session` | `/end-session` | Any "goodbye/shutdown" scene |
| `codex_invoke` | `codex` command in bash | Pokemon "I choose you" |
| `skill_loaded` | Any other skill | Generic reaction |

## Session-Level Events (Stop hook)

| Event | Trigger | Suggested Clip |
|-------|---------|---------------|
| `warcraft_work` | First message in session | WoW Peon "work work" |
| `refactor_loop` | Phase = REFACTOR (ongoing) | Doctor Strange Dormammu |
| `refactor_complete` | REFACTOR → other phase | Suits "get it done" |
| `desperation_cooldown` | Turn count ≥ 20 | This Is Fine dog |
| `scope_too_big` | Git diff ≥ 500 lines | HIMYM slap bet |
| `erlich_bachman` | Same (random pool) | Silicon Valley Erlich grand plans |
| `fly_you_fools` | 4+ active tasks | LOTR Gandalf "fly you fools" |
| `friday_deploy` | Deploy on Friday ≥ 16:00 | Any warning/panic clip |
| `end_of_day` | Session after 22:00 or before 05:00 | Any "go to sleep" clip |

## Background / Async Events

| Event | Trigger | Suggested Clip |
|-------|---------|---------------|
| `warcraft_zagzag` | `docker build`, `bun install`, etc. | WoW Orc "zug zug" |
| `zug_test1`–`zug_test3` | Same (random pool) | WoW variants |
| `spawn_agents` | Subagent spawn | Any "sending troops" scene |
| `multi_agent_run` | 3+ parallel agents | Yu-Gi-Oh Exodia "obliterate" |

## Security Events

| Event | Trigger | Suggested Clip |
|-------|---------|---------------|
| `security_block` | Hook blocks dangerous command | Any "blocked/rejected" scene |

---

## Adding an Event

1. Add an entry to `config/memes.json` with `url`, `duration`, and optionally `start`.
2. Run `~/.claude/bin/download-memes.sh` — it skips existing clips.
3. Add detection logic in the relevant hook script (Bash/Skill/Stop).
4. Test with: `~/.claude/bin/play-meme.sh <your_event_name>`

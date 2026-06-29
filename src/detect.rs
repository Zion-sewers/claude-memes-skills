//! Workflow-event detection.
//!
//! Pure functions map a hook's payload (a Bash command + its output, a skill
//! name, or a Stop signal sweep) to event names. Keeping detection pure makes it
//! unit-testable without spawning Claude Code — the IO lives in `cli`.

/// Outcome of inspecting a Bash tool call.
#[derive(Debug, PartialEq, Eq)]
pub struct BashDecision {
    pub event: &'static str,
    /// New value to persist for fail->fail / fail->pass transition tracking.
    pub set_last_test: Option<&'static str>,
}

/// True if all needles appear in `haystack` in the given order (cheap regex stand-in).
fn contains_in_order(haystack: &str, needles: &[&str]) -> bool {
    let mut idx = 0usize;
    for needle in needles {
        match haystack[idx..].find(needle) {
            Some(pos) => idx += pos + needle.len(),
            None => return false,
        }
    }
    true
}

fn any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn is_test_command(cmd: &str) -> bool {
    any(
        cmd,
        &[
            "pytest",
            "vitest",
            "bun test",
            "bun run test",
            "cargo test",
            "go test",
            "npm test",
            "jest",
            "pnpm test",
            "yarn test",
        ],
    )
}

/// Detect events from a Bash `PostToolUse` payload. `prev_test` is the last
/// persisted test outcome ("pass"/"fail"), enabling transition-aware clips.
pub fn detect_bash(command: &str, response: &str, prev_test: Option<&str>) -> Option<BashDecision> {
    let cmd = command.to_lowercase();
    let resp = response.to_lowercase();

    if is_test_command(&cmd) {
        let failed = any(
            &resp,
            &["failed", "fail ", "error[", "panicked", " failures"],
        );
        let passed = any(
            &resp,
            &[
                "passed",
                "test result: ok",
                "tests passed",
                " ok\n",
                "0 failed",
                "all tests pass",
            ],
        );
        if failed {
            let event = if prev_test == Some("fail") {
                "hydra"
            } else {
                "red_phase"
            };
            return Some(BashDecision {
                event,
                set_last_test: Some("fail"),
            });
        }
        if passed {
            let event = if prev_test == Some("fail") {
                "matrix_dodge"
            } else {
                "tests_all_pass"
            };
            return Some(BashDecision {
                event,
                set_last_test: Some("pass"),
            });
        }
        return None;
    }

    // CI pipeline polling (e.g. `gh run watch` output).
    if contains_in_order(&resp, &["completed", "success"]) {
        return Some(BashDecision {
            event: "full_pipeline_green",
            set_last_test: None,
        });
    }
    if contains_in_order(&resp, &["completed", "fail"]) {
        return Some(BashDecision {
            event: "ci_build_fail",
            set_last_test: None,
        });
    }
    if contains_in_order(&resp, &["deploy", "succe"]) {
        return Some(BashDecision {
            event: "ci_deploy_success",
            set_last_test: None,
        });
    }

    if any(&cmd, &["gh pr create"]) {
        return Some(BashDecision {
            event: "ship_pr_created",
            set_last_test: None,
        });
    }
    if any(&cmd, &["--cov", "coverage"]) {
        return Some(BashDecision {
            event: "coverage_check",
            set_last_test: None,
        });
    }
    if any(
        &cmd,
        &[
            "ruff check",
            "eslint",
            "bun run lint",
            "cargo clippy",
            "npm run lint",
        ],
    ) && !any(&resp, &["error", "warning", "problem", "failed"])
    {
        return Some(BashDecision {
            event: "noice",
            set_last_test: None,
        });
    }
    if any(&cmd, &["git push"]) && !any(&resp, &["rejected", "error", "fatal"]) {
        return Some(BashDecision {
            event: "noice",
            set_last_test: None,
        });
    }
    if any(&cmd, &["git commit", "git status", "git diff"])
        && any(
            &resp,
            &[
                "nothing to commit",
                "working tree clean",
                "no changes added",
            ],
        )
    {
        return Some(BashDecision {
            event: "nothing_to_commit",
            set_last_test: None,
        });
    }

    None
}

/// Map a skill name to an event. Unknown skills get the generic clip.
pub fn detect_skill(skill: &str) -> &'static str {
    match skill.trim().to_lowercase().as_str() {
        "brainstorming" => "brainstorm_start",
        "deep-research" => "deep_research",
        "review" | "code-review" | "security-review" => "review_perfect",
        "agent-team" => "agent_team",
        _ => "skill_loaded",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_failure_is_red_phase() {
        let d = detect_bash("pytest tests/", "1 failed, 3 passed", None).unwrap();
        assert_eq!(d.event, "red_phase");
        assert_eq!(d.set_last_test, Some("fail"));
    }

    #[test]
    fn consecutive_failure_is_hydra() {
        let d = detect_bash("pytest", "2 failed", Some("fail")).unwrap();
        assert_eq!(d.event, "hydra");
    }

    #[test]
    fn recovery_is_matrix_dodge() {
        let d = detect_bash("cargo test", "test result: ok. 12 passed", Some("fail")).unwrap();
        assert_eq!(d.event, "matrix_dodge");
        assert_eq!(d.set_last_test, Some("pass"));
    }

    #[test]
    fn clean_pass_is_lebron() {
        let d = detect_bash("npm test", "All tests passed", Some("pass")).unwrap();
        assert_eq!(d.event, "tests_all_pass");
    }

    #[test]
    fn non_test_pass_text_is_ignored() {
        assert!(detect_bash("echo passed", "passed", None).is_none());
    }

    #[test]
    fn pr_create_detected() {
        let d = detect_bash("gh pr create --fill", "https://github.com/...", None).unwrap();
        assert_eq!(d.event, "ship_pr_created");
    }

    #[test]
    fn lint_clean_is_noice_but_dirty_is_not() {
        assert_eq!(
            detect_bash("cargo clippy", "Finished", None).unwrap().event,
            "noice"
        );
        assert!(detect_bash("cargo clippy", "warning: unused", None).is_none());
    }

    #[test]
    fn git_push_success_is_noice() {
        assert_eq!(
            detect_bash("git push origin main", "main -> main", None)
                .unwrap()
                .event,
            "noice"
        );
        assert!(detect_bash("git push", "rejected: non-fast-forward", None).is_none());
    }

    #[test]
    fn nothing_to_commit_detected() {
        let d = detect_bash(
            "git commit -am x",
            "nothing to commit, working tree clean",
            None,
        )
        .unwrap();
        assert_eq!(d.event, "nothing_to_commit");
    }

    #[test]
    fn ci_pipeline_states() {
        assert_eq!(
            detect_bash("gh run watch", "completed  success", None)
                .unwrap()
                .event,
            "full_pipeline_green"
        );
        assert_eq!(
            detect_bash("gh run watch", "completed  failure", None)
                .unwrap()
                .event,
            "ci_build_fail"
        );
    }

    #[test]
    fn skills_map_correctly() {
        assert_eq!(detect_skill("deep-research"), "deep_research");
        assert_eq!(detect_skill("agent-team"), "agent_team");
        assert_eq!(detect_skill("totally-unknown"), "skill_loaded");
    }

    #[test]
    fn contains_in_order_respects_order() {
        assert!(contains_in_order(
            "completed then success",
            &["completed", "success"]
        ));
        assert!(!contains_in_order(
            "success before completed",
            &["completed", "success"]
        ));
    }
}

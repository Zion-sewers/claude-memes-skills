//! Ephemeral per-session state: master switch, per-event cooldown, fire-once.
//!
//! All state lives under a single temp dir so it is wiped between reboots and
//! never pollutes the user's home. State is intentionally best-effort — a failed
//! read or write degrades to "go ahead and play", never to a crash.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn state_dir() -> PathBuf {
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("claude-memes")
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Session-level override files let a user mute/unmute without editing config:
///   touch $TMPDIR/claude-memes/disabled   -> off for this machine/session
///   touch $TMPDIR/claude-memes/enabled    -> force on (overrides config `enabled=false`)
pub fn session_override() -> Option<bool> {
    let dir = state_dir();
    if dir.join("disabled").exists() {
        return Some(false);
    }
    if dir.join("enabled").exists() {
        return Some(true);
    }
    None
}

/// Resolve the effective on/off state from config + session override.
pub fn is_enabled(config_enabled: bool) -> bool {
    match session_override() {
        Some(v) => v,
        None => config_enabled,
    }
}

/// Returns true and records "now" if the event is outside its cooldown window.
/// Returns false (skip) if it fired within `cooldown_secs`.
pub fn cooldown_ok(event: &str, cooldown_secs: u64) -> bool {
    let dir = state_dir().join("cooldown");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(sanitize(event));

    if let Ok(contents) = std::fs::read_to_string(&file) {
        if let Ok(last) = contents.trim().parse::<u64>() {
            if now_secs().saturating_sub(last) < cooldown_secs {
                return false;
            }
        }
    }
    let _ = std::fs::write(&file, now_secs().to_string());
    true
}

/// Returns true the first time it is called for `event` within `session`, false
/// after. Keyed by the Claude Code session id so "once per session" survives
/// long-lived machines where /tmp persists across many sessions.
pub fn fire_once(session: &str, event: &str) -> bool {
    let dir = state_dir().join("once");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("{}--{}", sanitize(session), sanitize(event)));
    if file.exists() {
        return false;
    }
    let _ = std::fs::write(&file, "1");
    true
}

/// Increment and return this session's turn counter. Each Stop event is one
/// completed turn, so claude-memes counts turns itself — no external
/// msg-counter hook required.
pub fn bump_turns(session: &str) -> u64 {
    let dir = state_dir().join("turns");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(sanitize(session));
    let n = std::fs::read_to_string(&file)
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    let _ = std::fs::write(&file, n.to_string());
    n
}

/// Last-seen test outcome, used for fail->fail and fail->pass transitions.
pub fn last_test() -> Option<String> {
    std::fs::read_to_string(state_dir().join("last-test"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn set_last_test(value: &str) {
    let dir = state_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(dir.join("last-test"), value);
}

/// Collapse an event name to a filesystem-safe token. Shared by every module
/// that derives a file name from an event (state files, GIF cache), so an
/// adversarial registry key like `../../evil` can never escape its directory.
pub fn sanitize(event: &str) -> String {
    event
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolate() -> tempdir_guard::Guard {
        tempdir_guard::Guard::new()
    }

    #[test]
    fn cooldown_blocks_second_immediate_call() {
        let _g = isolate();
        assert!(cooldown_ok("evt_cd", 60));
        assert!(!cooldown_ok("evt_cd", 60));
    }

    #[test]
    fn cooldown_zero_always_ok() {
        let _g = isolate();
        assert!(cooldown_ok("evt_zero", 0));
        assert!(cooldown_ok("evt_zero", 0));
    }

    #[test]
    fn fire_once_only_first_time() {
        let _g = isolate();
        assert!(fire_once("sess_a", "evt_once"));
        assert!(!fire_once("sess_a", "evt_once"));
    }

    #[test]
    fn fire_once_is_scoped_per_session() {
        let _g = isolate();
        assert!(fire_once("sess_1", "evt"));
        // A different session gets its own fresh once-slot.
        assert!(fire_once("sess_2", "evt"));
        assert!(!fire_once("sess_1", "evt"));
    }

    #[test]
    fn turns_count_up_per_session() {
        let _g = isolate();
        assert_eq!(bump_turns("sess_t"), 1);
        assert_eq!(bump_turns("sess_t"), 2);
        assert_eq!(bump_turns("other"), 1);
    }

    #[test]
    fn override_beats_config() {
        let _g = isolate();
        let dir = state_dir();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("disabled"), "1").unwrap();
        assert!(!is_enabled(true));
        std::fs::remove_file(dir.join("disabled")).unwrap();
        std::fs::write(dir.join("enabled"), "1").unwrap();
        assert!(is_enabled(false));
    }

    /// Point TMPDIR at a throwaway dir for the duration of a test so state files
    /// never collide with a real session (or other tests sharing the process).
    mod tempdir_guard {
        use std::path::PathBuf;
        use std::sync::Mutex;

        // Serialize tests that mutate the shared TMPDIR env var.
        static LOCK: Mutex<()> = Mutex::new(());

        pub struct Guard {
            _lock: std::sync::MutexGuard<'static, ()>,
            prev: Option<std::ffi::OsString>,
            dir: PathBuf,
        }

        impl Guard {
            pub fn new() -> Self {
                let lock = LOCK.lock().unwrap_or_else(|e| e.into_inner());
                let prev = std::env::var_os("TMPDIR");
                let dir = std::env::temp_dir().join(format!(
                    "claude-memes-test-{}-{}",
                    std::process::id(),
                    fastish_id()
                ));
                std::fs::create_dir_all(&dir).unwrap();
                std::env::set_var("TMPDIR", &dir);
                Guard {
                    _lock: lock,
                    prev,
                    dir,
                }
            }
        }

        impl Drop for Guard {
            fn drop(&mut self) {
                match &self.prev {
                    Some(v) => std::env::set_var("TMPDIR", v),
                    None => std::env::remove_var("TMPDIR"),
                }
                let _ = std::fs::remove_dir_all(&self.dir);
            }
        }

        // Monotonic-ish unique suffix without pulling in rand.
        fn fastish_id() -> u64 {
            use std::sync::atomic::{AtomicU64, Ordering};
            static N: AtomicU64 = AtomicU64::new(0);
            N.fetch_add(1, Ordering::Relaxed)
        }
    }
}

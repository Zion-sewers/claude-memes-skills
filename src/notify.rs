//! terminalSequence notification building.
//!
//! Hooks in current Claude Code run with no controlling terminal, so the
//! sanctioned ambient output channel is the `terminalSequence` hook-output
//! field (v2.1.141+), restricted to OSC 0/1/2/9/99/777 + BEL. We build desktop
//! notifications: OSC 9 (iTerm2/WezTerm/Windows Terminal/ConEmu) by default,
//! OSC 777 (urxvt/Ghostty/Warp) as an option.

/// Strip anything that could terminate or corrupt the escape sequence.
fn sanitize(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).take(160).collect()
}

/// Build the notification escape sequence for the configured style.
/// Returns None when notifications are disabled.
pub fn sequence(style: &str, title: &str, body: &str) -> Option<String> {
    let title = sanitize(title);
    let body = sanitize(body);
    match style {
        "osc9" => Some(format!("\x1b]9;{title}: {body}\x07")),
        "osc777" => Some(format!("\x1b]777;notify;{title};{body}\x07")),
        "off" => None,
        // Unknown style: fail open with the broadest option rather than silence.
        _ => Some(format!("\x1b]9;{title}: {body}\x07")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc9_shape() {
        let s = sequence("osc9", "claude-memes", "tests_all_pass").unwrap();
        assert!(s.starts_with("\x1b]9;"));
        assert!(s.ends_with('\x07'));
        assert!(s.contains("tests_all_pass"));
    }

    #[test]
    fn osc777_shape() {
        let s = sequence("osc777", "claude-memes", "hydra").unwrap();
        assert!(s.starts_with("\x1b]777;notify;"));
    }

    #[test]
    fn off_disables() {
        assert!(sequence("off", "t", "b").is_none());
    }

    #[test]
    fn control_chars_are_stripped() {
        let s = sequence("osc9", "t", "evil\x07\x1b]0;pwn\x07body").unwrap();
        // Exactly one BEL — the terminator we appended ourselves.
        assert_eq!(s.matches('\x07').count(), 1);
        assert!(!s.contains("\x1b]0;"));
    }
}

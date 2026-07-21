//! `claude-memes watch` — a tiny localhost daemon that renders GIFs in a
//! terminal the user actually owns.
//!
//! Hooks have no controlling terminal, so inline rendering from a hook is
//! impossible; instead the hook POSTs the event name here over plain TCP
//! (deliberately not reqwest: localhost traffic must never route through an
//! HTTPS proxy), and the daemon — running in the user's own pane — plays it.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use anyhow::{Context, Result};

/// How long a hook is willing to wait on the daemon before falling back.
const CLIENT_TIMEOUT: Duration = Duration::from_millis(250);

/// A message a hook can hand to the daemon.
#[derive(Debug, PartialEq, Eq)]
pub enum Msg {
    /// A curated registry event to play.
    Event(String),
    /// An unmapped workflow concept for the auto-mode pipeline.
    Concept(String),
}

/// Try to hand a curated event to a running watch daemon.
pub fn try_send(port: u16, event: &str) -> bool {
    send_json(port, serde_json::json!({ "event": event }))
}

/// Try to hand an auto-mode concept to a running watch daemon.
pub fn try_send_concept(port: u16, concept: &str) -> bool {
    send_json(port, serde_json::json!({ "concept": concept }))
}

/// Returns true only when the daemon acknowledged with a 200.
fn send_json(port: u16, payload: serde_json::Value) -> bool {
    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let Ok(mut stream) = TcpStream::connect_timeout(&addr, CLIENT_TIMEOUT) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(CLIENT_TIMEOUT));
    let _ = stream.set_write_timeout(Some(CLIENT_TIMEOUT));

    let body = payload.to_string();
    let request = format!(
        "POST /play HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    if stream.write_all(request.as_bytes()).is_err() {
        return false;
    }
    let mut response = [0u8; 64];
    match stream.read(&mut response) {
        Ok(n) if n > 0 => String::from_utf8_lossy(&response[..n]).contains(" 200 "),
        _ => false,
    }
}

/// Run the watch daemon in the foreground until interrupted.
pub fn run(port: u16, play: impl Fn(Msg)) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .with_context(|| format!("binding 127.0.0.1:{port} (is another watch running?)"))?;
    println!("claude-memes watch — listening on 127.0.0.1:{port}");
    println!("Hook-triggered memes will render in this terminal. Ctrl-C to stop.");

    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
        if let Some(msg) = read_event(&mut stream) {
            // Ack first so the hook never waits on rendering.
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            let _ = stream.flush();
            drop(stream);
            play(msg);
        } else {
            let _ = stream.write_all(
                b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    }
    Ok(())
}

/// Minimal HTTP parse: enough for our own well-formed client, nothing more.
fn read_event(stream: &mut TcpStream) -> Option<Msg> {
    let mut buf = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > 64 * 1024 {
                    return None;
                }
                if let Some(event) = parse_event(&buf) {
                    return Some(event);
                }
                // Keep reading until the body is complete or the peer stops.
            }
            Err(_) => break,
        }
    }
    parse_event(&buf)
}

/// Extract `{"event": "..."}` or `{"concept": "..."}` from a raw HTTP request.
pub fn parse_event(raw: &[u8]) -> Option<Msg> {
    let text = String::from_utf8_lossy(raw);
    let body_start = text.find("\r\n\r\n")? + 4;
    let body = text.get(body_start..)?.trim();
    if body.is_empty() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if let Some(event) = value.get("event").and_then(|e| e.as_str()) {
        return Some(Msg::Event(event.to_string()));
    }
    value
        .get("concept")
        .and_then(|c| c.as_str())
        .map(|c| Msg::Concept(c.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_wellformed_request() {
        let raw =
            b"POST /play HTTP/1.1\r\nContent-Length: 26\r\n\r\n{\"event\":\"tests_all_pass\"}";
        assert_eq!(
            parse_event(raw),
            Some(Msg::Event("tests_all_pass".to_string()))
        );
    }

    #[test]
    fn parses_concept_request() {
        let raw = b"POST /play HTTP/1.1\r\n\r\n{\"concept\":\"cargo build exploded\"}";
        assert_eq!(
            parse_event(raw),
            Some(Msg::Concept("cargo build exploded".to_string()))
        );
    }

    #[test]
    fn rejects_missing_body_or_garbage() {
        assert!(parse_event(b"POST /play HTTP/1.1\r\n\r\n").is_none());
        assert!(parse_event(b"POST /play HTTP/1.1\r\n\r\nnot json").is_none());
        assert!(parse_event(b"nonsense").is_none());
    }

    #[test]
    fn client_fails_fast_when_no_daemon() {
        // Port 1 is essentially never listening; must return false quickly.
        let start = std::time::Instant::now();
        assert!(!try_send(1, "tests_all_pass"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn end_to_end_over_localhost() {
        use std::sync::mpsc;
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let event = read_event(&mut stream).unwrap();
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
            tx.send(event).unwrap();
        });
        assert!(try_send(port, "hydra"));
        assert_eq!(rx.recv().unwrap(), Msg::Event("hydra".to_string()));
    }
}

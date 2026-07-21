//! `claude-memes mcp` — a minimal MCP (Model Context Protocol) server over
//! stdio, exposing the meme engine to any MCP-speaking harness: org agents
//! behind tool brokers/gateways, Slack-routed assistants, other CLIs.
//!
//! Tools:
//! - `find_meme(concept)` — rank emotion hubs for a workflow concept; returns
//!   the best hub, its score, the chosen Giphy query, and (when a
//!   GIPHY_API_KEY is available) a ready-to-post GIF URL.
//! - `play_meme(event)` — fire a curated registry event: hands it to a running
//!   `watch` daemon when present, otherwise returns a GIF URL. Never renders
//!   to stdout — that channel belongs to the protocol.
//!
//! Hand-rolled JSON-RPC 2.0 over newline-delimited stdio: the protocol subset
//! MCP clients need (initialize / tools/list / tools/call / ping) is small
//! enough that a dependency would cost more than these ~150 lines.

use std::io::{BufRead, Write};

use anyhow::Result;
use serde_json::{json, Value};

use crate::config::Registry;

pub fn serve() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if let Some(response) = handle(&request) {
            writeln!(stdout, "{response}")?;
            stdout.flush()?;
        }
    }
    Ok(())
}

/// Route one JSON-RPC message. Notifications (no `id`) get no response.
pub fn handle(request: &Value) -> Option<Value> {
    let method = request.get("method")?.as_str()?;
    let id = request.get("id").filter(|v| !v.is_null()).cloned();
    match method {
        "initialize" => {
            let requested = request
                .pointer("/params/protocolVersion")
                .and_then(|v| v.as_str())
                .unwrap_or("2025-06-18");
            Some(result(
                id?,
                json!({
                    "protocolVersion": requested,
                    "capabilities": { "tools": {} },
                    "serverInfo": {
                        "name": "claude-memes",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            ))
        }
        "ping" => Some(result(id?, json!({}))),
        "tools/list" => Some(result(id?, json!({ "tools": tool_definitions() }))),
        "tools/call" => {
            let id = id?;
            let name = request
                .pointer("/params/name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let args = request
                .pointer("/params/arguments")
                .cloned()
                .unwrap_or_else(|| json!({}));
            Some(match call_tool(name, &args) {
                Ok(text) => result(id, json!({ "content": [{ "type": "text", "text": text }] })),
                Err(e) => result(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": format!("{e:#}") }],
                        "isError": true
                    }),
                ),
            })
        }
        // Notifications (initialized, cancelled, …) need no reply; unknown
        // *requests* get a proper error.
        _ => id.map(|id| error(id, -32601, &format!("method not found: {method}"))),
    }
}

fn tool_definitions() -> Value {
    json!([
        {
            "name": "find_meme",
            "description": "Match a developer-workflow moment to an emotion-hub meme. Returns the best hub, similarity score, Giphy search query, and a GIF URL when a GIPHY_API_KEY is configured.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "concept": {
                        "type": "string",
                        "description": "The workflow moment in plain words, e.g. 'npm install failed with conflicting peer dependencies'"
                    }
                },
                "required": ["concept"]
            }
        },
        {
            "name": "play_meme",
            "description": "Fire a curated claude-memes registry event (see the events registry). Renders via a running `claude-memes watch` daemon when present; otherwise returns a GIF URL to post.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "event": {
                        "type": "string",
                        "description": "Registry event name, e.g. 'tests_all_pass'"
                    }
                },
                "required": ["event"]
            }
        }
    ])
}

fn call_tool(name: &str, args: &Value) -> Result<String> {
    match name {
        "find_meme" => {
            let concept = args
                .get("concept")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required argument: concept"))?;
            let reg = Registry::load()?;
            let engine = crate::auto::Engine::load(true)?;
            let ranked = engine.top(concept);
            let (hub, score) = ranked
                .first()
                .ok_or_else(|| anyhow::anyhow!("no hubs available"))?;
            let query = engine
                .hubs
                .hubs
                .get(*hub)
                .map(crate::auto::pick_query)
                .unwrap_or_default()
                .to_string();
            // URL resolution is best-effort: without an API key the query alone
            // is still useful to the caller.
            let gif_url = crate::giphy::resolve_url(&query, &reg.settings.rating).ok();
            Ok(json!({
                "hub": hub,
                "score": score,
                "query": query,
                "fires": *score >= reg.settings.auto_threshold,
                "gif_url": gif_url
            })
            .to_string())
        }
        "play_meme" => {
            let event = args
                .get("event")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("missing required argument: event"))?;
            let reg = Registry::load()?;
            let ev = reg
                .event(event)
                .ok_or_else(|| anyhow::anyhow!("unknown event '{event}'"))?;
            if crate::daemon::try_send(reg.settings.watch_port, event) {
                return Ok(json!({ "delivered": "watch-daemon", "event": event }).to_string());
            }
            let gif_url = crate::giphy::resolve_url(&ev.query, &reg.settings.rating)?;
            Ok(json!({ "delivered": "url", "event": event, "gif_url": gif_url }).to_string())
        }
        other => Err(anyhow::anyhow!("unknown tool: {other}")),
    }
}

fn result(id: Value, payload: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": payload })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initialize_handshake() {
        let req = json!({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2025-06-18","capabilities":{}}});
        let resp = handle(&req).unwrap();
        assert_eq!(resp["id"], 1);
        assert_eq!(resp["result"]["serverInfo"]["name"], "claude-memes");
        assert_eq!(resp["result"]["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn tools_list_exposes_both_tools() {
        let req = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
        let resp = handle(&req).unwrap();
        let tools = resp["result"]["tools"].as_array().unwrap();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert_eq!(names, vec!["find_meme", "play_meme"]);
        for t in tools {
            assert!(t["inputSchema"]["required"].is_array());
        }
    }

    #[test]
    fn notifications_get_no_response() {
        let req = json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        assert!(handle(&req).is_none());
    }

    #[test]
    fn unknown_request_gets_error() {
        let req = json!({"jsonrpc":"2.0","id":3,"method":"resources/list"});
        let resp = handle(&req).unwrap();
        assert_eq!(resp["error"]["code"], -32601);
    }

    #[test]
    fn tool_call_with_missing_args_is_error_content() {
        let req = json!({"jsonrpc":"2.0","id":4,"method":"tools/call",
            "params":{"name":"find_meme","arguments":{}}});
        let resp = handle(&req).unwrap();
        assert_eq!(resp["result"]["isError"], true);
    }
}

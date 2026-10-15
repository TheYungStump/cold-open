//! The handshake (docs/specs/mcp.md §5.3).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::VERSION;

/// The client's program, from MCP's client information.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Client {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// The client's first line.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hello {
    pub v: u32,
    pub token: String,
    pub client: Client,
    pub doc: PathBuf,
}

/// Why a handshake failed, as its error code says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelloError {
    BadToken,
    WrongDoc,
    AgentAccessOff,
    TooManyLinks,
    BadHello,
}

impl HelloError {
    pub fn code(self) -> &'static str {
        match self {
            HelloError::BadToken => "bad_token",
            HelloError::WrongDoc => "wrong_doc",
            HelloError::AgentAccessOff => "agent_access_off",
            HelloError::TooManyLinks => "too_many_links",
            HelloError::BadHello => "bad_hello",
        }
    }

    pub fn message(self) -> &'static str {
        match self {
            HelloError::BadToken => "the token does not match this app's session",
            HelloError::WrongDoc => "that script is not open in the app",
            HelloError::AgentAccessOff => "agent access is turned off in the app",
            HelloError::TooManyLinks => "the app already has the most agents it accepts",
            HelloError::BadHello => "the handshake was not understood",
        }
    }
}

/// The handshake line a client sends.
pub fn hello_line(token: &str, client: &Client, doc: &std::path::Path) -> String {
    json!({ "hello": Hello {
        v: VERSION,
        token: token.to_string(),
        client: client.clone(),
        doc: doc.to_path_buf(),
    } })
    .to_string()
}

/// Reads the client's handshake line.
pub fn parse_hello(line: &str) -> Result<Hello, HelloError> {
    let v: Value = serde_json::from_str(line).map_err(|_| HelloError::BadHello)?;
    let obj = v.as_object().ok_or(HelloError::BadHello)?;
    if obj.len() != 1 {
        return Err(HelloError::BadHello);
    }
    let h: Hello = serde_json::from_value(obj.get("hello").cloned().ok_or(HelloError::BadHello)?)
        .map_err(|_| HelloError::BadHello)?;
    if h.v != VERSION {
        return Err(HelloError::BadHello);
    }
    Ok(h)
}

/// The app's answer when the handshake succeeds.
pub fn reply_ok(app: &str, doc: &std::path::Path, rev: u64) -> String {
    json!({ "hello": {
        "v": VERSION,
        "ok": true,
        "app": app,
        "doc": { "path": doc, "rev": rev },
    } })
    .to_string()
}

/// The app's answer when the handshake fails.
pub fn reply_err(e: HelloError) -> String {
    json!({ "hello": {
        "v": VERSION,
        "ok": false,
        "error": { "code": e.code(), "message": e.message() },
    } })
    .to_string()
}

/// What a successful handshake tells the client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelloOk {
    pub app: String,
    pub doc: PathBuf,
    pub rev: u64,
}

/// Reads the app's answer: what it said, or its error code and message.
pub fn parse_reply(line: &str) -> Result<HelloOk, (String, String)> {
    let bad = || {
        (
            "bad_hello".to_string(),
            "the app's answer was not understood".to_string(),
        )
    };
    let v: Value = serde_json::from_str(line).map_err(|_| bad())?;
    let h = &v["hello"];
    if h["ok"] == true {
        Ok(HelloOk {
            app: h["app"].as_str().unwrap_or("").to_string(),
            doc: PathBuf::from(h["doc"]["path"].as_str().ok_or_else(bad)?),
            rev: h["doc"]["rev"].as_u64().unwrap_or(0),
        })
    } else {
        let e = &h["error"];
        Err((
            e["code"].as_str().ok_or_else(bad)?.to_string(),
            e["message"].as_str().unwrap_or("").to_string(),
        ))
    }
}

/// Compares two tokens in time that depends only on their length, which
/// is public, never on where they first differ.
pub fn tokens_equal(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    std::hint::black_box(diff) == 0
}

/// An agent's name as the app shows it: at most 64 characters, no control
/// characters, "agent" when nothing is left.
pub fn clean_name(s: &str) -> String {
    let t: String = s
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect::<String>()
        .trim()
        .to_string();
    if t.is_empty() { "agent".into() } else { t }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn hello_round_trips_and_rejects_junk() {
        let c = Client {
            name: "claude-code".into(),
            version: Some("2.1".into()),
        };
        let line = hello_line("ab", &c, Path::new("/s/x.fountain"));
        let h = parse_hello(&line).unwrap();
        assert_eq!(h.client, c);
        assert_eq!(h.token, "ab");
        for junk in [
            "",
            "{}",
            "[1]",
            r#"{"hello": {"v": 2, "token": "a", "client": {"name": "x"}, "doc": "d"}}"#,
            r#"{"hello": {"v": 1, "token": "a", "client": {"name": "x"}, "doc": "d", "x": 1}}"#,
            r#"{"hello": {"v": 1, "token": "a", "client": {"name": "x"}, "doc": "d"}, "x": 1}"#,
        ] {
            assert_eq!(parse_hello(junk), Err(HelloError::BadHello), "{junk}");
        }
    }

    #[test]
    fn replies_round_trip() {
        let ok = parse_reply(&reply_ok("0.1.0", Path::new("/s/x.fountain"), 7)).unwrap();
        assert_eq!(ok.rev, 7);
        assert_eq!(ok.doc, Path::new("/s/x.fountain"));
        let e = parse_reply(&reply_err(HelloError::AgentAccessOff)).unwrap_err();
        assert_eq!(e.0, "agent_access_off");
        assert!(parse_reply("nope").is_err());
    }

    #[test]
    fn tokens_and_names() {
        assert!(tokens_equal("abc", "abc"));
        assert!(!tokens_equal("abc", "abd"));
        assert!(!tokens_equal("abc", "abcd"));
        assert_eq!(clean_name("  claude\u{7}-code \n"), "claude-code");
        assert_eq!(clean_name("\u{1b}"), "agent");
        assert_eq!(clean_name(&"x".repeat(100)).len(), 64);
    }
}

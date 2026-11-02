//! Local endpoint settings + auth (FABLE B1/B2): the OpenAI-compatible
//! /v1 routes and the /handoff route share one opt-in toggle and one
//! stable bearer token. Off by default; when on, external tools (Claude
//! Code, Aider, the browser extension) authenticate with
//! `Authorization: Bearer <token>` or `x-api-key: <token>`.

use serde::{Deserialize, Serialize};

const FILE: &str = "endpoint.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EndpointSettings {
    #[serde(default)]
    pub enabled: bool,
    /// Stable token shown once in Settings; rotates only when the user
    /// asks. Never the 30s pairing token - external tools store it.
    #[serde(default)]
    pub token: String,
}

impl EndpointSettings {
    pub fn load() -> Self {
        crate::storage::load_json(FILE).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        crate::storage::save_json(FILE, self)
    }

    pub fn ensure_token(&mut self) {
        if self.token.is_empty() {
            self.token = format!("orbx-{}", uuid::Uuid::new_v4().simple());
        }
    }

    /// The base URL external tools paste next to the token.
    pub fn base_url() -> String {
        "http://127.0.0.1:51231/v1".to_string()
    }
}

/// Authorize a request against the current settings. Reads the settings
/// file on every call so a toggle flip applies immediately; the file is
/// a few hundred bytes on local disk.
pub fn authorize(authorization: Option<&str>, x_api_key: Option<&str>) -> Result<(), &'static str> {
    let settings = EndpointSettings::load();
    if !settings.enabled {
        return Err("the local endpoint is off. Turn it on in Orbix Settings > Models.");
    }
    if settings.token.is_empty() {
        return Err("the local endpoint has no token yet. Toggle it in Orbix Settings > Models.");
    }
    let bearer = authorization
        .map(str::trim)
        .and_then(|a| a.strip_prefix("Bearer ").or_else(|| a.strip_prefix("bearer ")))
        .map(str::trim);
    let presented = bearer.or(x_api_key.map(str::trim));
    match presented {
        Some(t) if constant_time_eq(t.as_bytes(), settings.token.as_bytes()) => Ok(()),
        Some(_) => Err("invalid token."),
        None => Err("missing token. Send Authorization: Bearer <token>."),
    }
}

/// Constant-time comparison so the token cannot be guessed byte by byte
/// from timing, even on localhost.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_minted_once_and_stable() {
        let mut s = EndpointSettings::default();
        s.ensure_token();
        assert!(s.token.starts_with("orbx-"));
        let first = s.token.clone();
        s.ensure_token();
        assert_eq!(s.token, first);
    }

    #[test]
    fn constant_time_eq_basics() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(!constant_time_eq(b"", b"x"));
    }

    #[test]
    fn bearer_parsing_accepts_both_headers() {
        // authorize() reads from disk, so exercise the header parsing
        // shape only: the strip logic mirrors these cases.
        let strip = |a: &str| -> Option<String> {
            a.trim()
                .strip_prefix("Bearer ")
                .or_else(|| a.trim().strip_prefix("bearer "))
                .map(|s| s.trim().to_string())
        };
        assert_eq!(strip("Bearer tok-1").as_deref(), Some("tok-1"));
        assert_eq!(strip("bearer tok-1").as_deref(), Some("tok-1"));
        assert_eq!(strip("Basic xyz"), None);
    }
}

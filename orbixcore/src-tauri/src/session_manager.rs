use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;
use chrono::{Utc, Duration};

// ─────────────────────────────────────────────
//  Session State
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Created,
    Active,
    Idle,
    Expired,
    Revoked,
}

// ─────────────────────────────────────────────
//  Principal
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Principal {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
    pub attributes: HashMap<String, String>,
}

// ─────────────────────────────────────────────
//  Session
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub token: String,
    pub principal: Principal,
    pub state: SessionState,
    pub metadata: HashMap<String, String>,
    pub created_at: String,
    pub last_activity: String,
    pub expires_at: String,
    pub idle_deadline: String,
}

// ─────────────────────────────────────────────
//  Session Manager
// ─────────────────────────────────────────────

pub struct SessionManager {
    sessions: HashMap<String, Session>,  // token -> session
    ttl_seconds: i64,
    idle_timeout_seconds: i64,
}

impl SessionManager {
    pub fn new(ttl_seconds: i64, idle_timeout_seconds: i64) -> Self {
        Self {
            sessions: HashMap::new(),
            ttl_seconds,
            idle_timeout_seconds,
        }
    }

    /// Create a new session for a principal.
    pub fn create_session(&mut self, principal: Principal, metadata: HashMap<String, String>) -> Session {
        let now = Utc::now();
        let token = format!("sess_{}", Uuid::new_v4().to_string().replace('-', ""));
        let session = Session {
            token: token.clone(),
            principal,
            state: SessionState::Active,
            metadata,
            created_at: now.to_rfc3339(),
            last_activity: now.to_rfc3339(),
            expires_at: (now + Duration::seconds(self.ttl_seconds)).to_rfc3339(),
            idle_deadline: (now + Duration::seconds(self.idle_timeout_seconds)).to_rfc3339(),
        };
        self.sessions.insert(token, session.clone());
        session
    }

    /// Validate a session token. Returns the session if valid, None otherwise.
    /// Automatically transitions idle/expired sessions.
    pub fn validate_session(&mut self, token: &str) -> Option<Session> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let session = self.sessions.get_mut(token)?;

        // Check if revoked
        if session.state == SessionState::Revoked {
            return None;
        }

        // Check absolute expiry
        if now_str > session.expires_at {
            session.state = SessionState::Expired;
            return None;
        }

        // Check idle timeout
        if now_str > session.idle_deadline && session.state == SessionState::Active {
            session.state = SessionState::Idle;
        }

        if session.state == SessionState::Expired {
            return None;
        }

        // Touch the session (reset idle deadline, mark active)
        session.last_activity = now_str;
        session.idle_deadline = (now + Duration::seconds(self.idle_timeout_seconds)).to_rfc3339();
        if session.state == SessionState::Idle || session.state == SessionState::Created {
            session.state = SessionState::Active;
        }

        Some(session.clone())
    }

    /// Revoke a session immediately.
    pub fn revoke_session(&mut self, token: &str) -> bool {
        if let Some(session) = self.sessions.get_mut(token) {
            session.state = SessionState::Revoked;
            true
        } else {
            false
        }
    }

    /// Extend a session's idle timeout.
    pub fn extend_session(&mut self, token: &str) -> Option<Session> {
        let now = Utc::now();
        let session = self.sessions.get_mut(token)?;

        if session.state == SessionState::Revoked || session.state == SessionState::Expired {
            return None;
        }

        session.last_activity = now.to_rfc3339();
        session.idle_deadline = (now + Duration::seconds(self.idle_timeout_seconds)).to_rfc3339();
        if session.state == SessionState::Idle {
            session.state = SessionState::Active;
        }
        Some(session.clone())
    }

    /// Get session without touching it.
    pub fn get_session(&self, token: &str) -> Option<&Session> {
        self.sessions.get(token)
    }

    /// List all active sessions.
    pub fn active_sessions(&self) -> Vec<&Session> {
        self.sessions.values()
            .filter(|s| s.state == SessionState::Active || s.state == SessionState::Idle || s.state == SessionState::Created)
            .collect()
    }

    /// Clean up expired and revoked sessions.
    pub fn cleanup(&mut self) -> usize {
        let now_str = Utc::now().to_rfc3339();
        let before = self.sessions.len();

        // First mark expired
        for session in self.sessions.values_mut() {
            if now_str > session.expires_at && session.state != SessionState::Revoked {
                session.state = SessionState::Expired;
            }
        }

        // Remove expired and revoked
        self.sessions.retain(|_, s| {
            s.state != SessionState::Expired && s.state != SessionState::Revoked
        });

        before - self.sessions.len()
    }

    /// Total session count.
    pub fn count(&self) -> usize {
        self.sessions.len()
    }

    /// Get session stats.
    pub fn stats(&self) -> SessionStats {
        let mut stats = SessionStats::default();
        for s in self.sessions.values() {
            match s.state {
                SessionState::Active => stats.active += 1,
                SessionState::Idle => stats.idle += 1,
                SessionState::Created => stats.created += 1,
                SessionState::Expired => stats.expired += 1,
                SessionState::Revoked => stats.revoked += 1,
            }
        }
        stats.total = self.sessions.len();
        stats
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SessionStats {
    pub total: usize,
    pub active: usize,
    pub idle: usize,
    pub created: usize,
    pub expired: usize,
    pub revoked: usize,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(3600, 900)
    }
}

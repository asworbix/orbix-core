use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::Utc;
use uuid::Uuid;

// ─────────────────────────────────────────────
//  Route Definition
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub path: String,
    pub method: HttpMethod,
    pub handler: String,
    pub middleware: Vec<String>,
    pub priority: u32,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "POST" => Self::Post,
            "PUT" => Self::Put,
            "DELETE" => Self::Delete,
            "PATCH" => Self::Patch,
            _ => Self::Get,
        }
    }
}

// ─────────────────────────────────────────────
//  Route Match Result
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RouteMatchResult {
    pub route: Route,
    pub params: HashMap<String, String>,
    pub matched: bool,
}

// ─────────────────────────────────────────────
//  Rate Limiter
// ─────────────────────────────────────────────

#[derive(Clone, Debug)]
struct RateLimitEntry {
    count: u32,
    window_start: String,
}

pub struct RateLimiter {
    limits: HashMap<String, RateLimitEntry>,  // client_id -> entry
    max_rps: u32,
    window_seconds: u64,
}

impl RateLimiter {
    pub fn new(max_rps: u32) -> Self {
        Self {
            limits: HashMap::new(),
            max_rps,
            window_seconds: 1,
        }
    }

    /// Check if a request is allowed for a given client.
    pub fn check(&mut self, client_id: &str) -> bool {
        let now = Utc::now();
        let now_str = now.to_rfc3339();

        let entry = self.limits.entry(client_id.to_string()).or_insert(RateLimitEntry {
            count: 0,
            window_start: now_str.clone(),
        });

        // Check if window has expired
        if let Ok(start) = entry.window_start.parse::<chrono::DateTime<Utc>>() {
            if (now - start).num_seconds() >= self.window_seconds as i64 {
                entry.count = 0;
                entry.window_start = now_str;
            }
        }

        entry.count += 1;
        entry.count <= self.max_rps
    }

    /// Get retry-after seconds for a rate-limited client.
    pub fn retry_after(&self, client_id: &str) -> u64 {
        if let Some(entry) = self.limits.get(client_id) {
            if let Ok(start) = entry.window_start.parse::<chrono::DateTime<Utc>>() {
                let elapsed = (Utc::now() - start).num_seconds();
                let remaining = self.window_seconds as i64 - elapsed;
                return remaining.max(1) as u64;
            }
        }
        1
    }

    pub fn cleanup(&mut self) {
        let cutoff = (Utc::now() - chrono::Duration::seconds(self.window_seconds as i64 * 2)).to_rfc3339();
        self.limits.retain(|_, e| e.window_start > cutoff);
    }
}

// ─────────────────────────────────────────────
//  API Error Response
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    pub retry_after: Option<u64>,
    pub trace_id: String,
}

impl ApiError {
    pub fn unauthenticated(msg: &str) -> Self {
        Self { code: "UNAUTHENTICATED".into(), message: msg.into(), retry_after: None, trace_id: Uuid::new_v4().to_string() }
    }

    pub fn forbidden(msg: &str) -> Self {
        Self { code: "FORBIDDEN".into(), message: msg.into(), retry_after: None, trace_id: Uuid::new_v4().to_string() }
    }

    pub fn not_found(msg: &str) -> Self {
        Self { code: "NOT_FOUND".into(), message: msg.into(), retry_after: None, trace_id: Uuid::new_v4().to_string() }
    }

    pub fn rate_limited(retry_after: u64) -> Self {
        Self {
            code: "RATE_LIMIT_EXCEEDED".into(),
            message: format!("Too many requests. Retry after {} seconds.", retry_after),
            retry_after: Some(retry_after),
            trace_id: Uuid::new_v4().to_string(),
        }
    }

    pub fn validation(msg: &str) -> Self {
        Self { code: "VALIDATION_ERROR".into(), message: msg.into(), retry_after: None, trace_id: Uuid::new_v4().to_string() }
    }

    pub fn internal(msg: &str) -> Self {
        Self { code: "INTERNAL_ERROR".into(), message: msg.into(), retry_after: None, trace_id: Uuid::new_v4().to_string() }
    }

    pub fn unavailable(msg: &str) -> Self {
        Self { code: "SERVICE_UNAVAILABLE".into(), message: msg.into(), retry_after: None, trace_id: Uuid::new_v4().to_string() }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "error": {
                "code": self.code,
                "message": self.message,
                "retryAfter": self.retry_after,
                "traceId": self.trace_id,
            }
        })
    }

    pub fn http_status(&self) -> u16 {
        match self.code.as_str() {
            "UNAUTHENTICATED" => 401,
            "FORBIDDEN" => 403,
            "NOT_FOUND" => 404,
            "RATE_LIMIT_EXCEEDED" => 429,
            "VALIDATION_ERROR" => 422,
            "SERVICE_UNAVAILABLE" => 503,
            _ => 500,
        }
    }
}

// ─────────────────────────────────────────────
//  Request Router
// ─────────────────────────────────────────────

pub struct RequestRouter {
    routes: Vec<Route>,
    rate_limiter: RateLimiter,
}

impl RequestRouter {
    pub fn new(rate_limit_rps: u32) -> Self {
        let mut router = Self {
            routes: Vec::new(),
            rate_limiter: RateLimiter::new(rate_limit_rps),
        };
        router.register_default_routes();
        router
    }

    /// Add a route.
    pub fn add_route(&mut self, route: Route) {
        self.routes.push(route);
        self.routes.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Match an incoming request path and method to a route.
    pub fn match_route(&self, method: &HttpMethod, path: &str) -> Option<RouteMatchResult> {
        for route in &self.routes {
            if &route.method != method {
                continue;
            }
            if let Some(params) = match_path_pattern(&route.path, path) {
                return Some(RouteMatchResult {
                    route: route.clone(),
                    params,
                    matched: true,
                });
            }
        }
        None
    }

    /// Check rate limit for a client.
    pub fn check_rate_limit(&mut self, client_id: &str) -> Result<(), ApiError> {
        if self.rate_limiter.check(client_id) {
            Ok(())
        } else {
            Err(ApiError::rate_limited(self.rate_limiter.retry_after(client_id)))
        }
    }

    /// List all registered routes.
    pub fn list_routes(&self) -> Vec<&Route> {
        self.routes.iter().collect()
    }

    /// Register OrbixCore's default API routes.
    fn register_default_routes(&mut self) {
        let routes = vec![
            // Health
            Route { path: "/health".into(), method: HttpMethod::Get, handler: "health.check".into(), middleware: vec![], priority: 100, description: Some("Basic health check".into()) },
            Route { path: "/health/ready".into(), method: HttpMethod::Get, handler: "health.ready".into(), middleware: vec![], priority: 100, description: Some("Readiness probe".into()) },
            Route { path: "/health/live".into(), method: HttpMethod::Get, handler: "health.live".into(), middleware: vec![], priority: 100, description: Some("Liveness probe".into()) },
            Route { path: "/metrics".into(), method: HttpMethod::Get, handler: "metrics.export".into(), middleware: vec![], priority: 90, description: Some("Prometheus metrics".into()) },

            // Jobs
            Route { path: "/api/v1/jobs".into(), method: HttpMethod::Post, handler: "job.create".into(), middleware: vec!["auth".into(), "rate_limit".into()], priority: 50, description: Some("Submit a new job".into()) },
            Route { path: "/api/v1/jobs/:id".into(), method: HttpMethod::Get, handler: "job.get".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Get job status".into()) },
            Route { path: "/api/v1/jobs/:id".into(), method: HttpMethod::Delete, handler: "job.cancel".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Cancel a job".into()) },
            Route { path: "/api/v1/jobs".into(), method: HttpMethod::Get, handler: "job.list".into(), middleware: vec!["auth".into()], priority: 50, description: Some("List jobs".into()) },

            // Sessions
            Route { path: "/api/v1/sessions".into(), method: HttpMethod::Post, handler: "session.create".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Create session".into()) },
            Route { path: "/api/v1/sessions/:token".into(), method: HttpMethod::Get, handler: "session.validate".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Validate session".into()) },
            Route { path: "/api/v1/sessions/:token".into(), method: HttpMethod::Delete, handler: "session.revoke".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Revoke session".into()) },

            // Plugins
            Route { path: "/api/v1/plugins".into(), method: HttpMethod::Get, handler: "plugin.list".into(), middleware: vec!["auth".into()], priority: 50, description: Some("List plugins".into()) },
            Route { path: "/api/v1/plugins/:name/activate".into(), method: HttpMethod::Post, handler: "plugin.activate".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Activate plugin".into()) },
            Route { path: "/api/v1/plugins/:name/deactivate".into(), method: HttpMethod::Post, handler: "plugin.deactivate".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Deactivate plugin".into()) },
            Route { path: "/api/v1/plugins/:name/status".into(), method: HttpMethod::Get, handler: "plugin.status".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Plugin health".into()) },

            // Events
            Route { path: "/api/v1/events".into(), method: HttpMethod::Post, handler: "event.publish".into(), middleware: vec!["auth".into(), "rate_limit".into()], priority: 50, description: Some("Publish event".into()) },
            Route { path: "/api/v1/events/stats".into(), method: HttpMethod::Get, handler: "event.stats".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Event bus stats".into()) },

            // Config
            Route { path: "/api/v1/config".into(), method: HttpMethod::Get, handler: "config.get".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Get configuration".into()) },
            Route { path: "/api/v1/config/:key".into(), method: HttpMethod::Put, handler: "config.set".into(), middleware: vec!["auth".into()], priority: 50, description: Some("Set config value".into()) },
        ];

        for route in routes {
            self.routes.push(route);
        }
        self.routes.sort_by(|a, b| b.priority.cmp(&a.priority));
    }
}

impl Default for RequestRouter {
    fn default() -> Self {
        Self::new(1000)
    }
}

/// Match a route pattern (e.g., "/api/v1/jobs/:id") against a request path.
/// Returns extracted parameters if matched.
fn match_path_pattern(pattern: &str, path: &str) -> Option<HashMap<String, String>> {
    let pattern_parts: Vec<&str> = pattern.split('/').filter(|s| !s.is_empty()).collect();
    let path_parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();

    // Check for wildcard
    if pattern.ends_with('*') {
        let base_parts: Vec<&str> = pattern.trim_end_matches('*').split('/').filter(|s| !s.is_empty()).collect();
        if path_parts.len() < base_parts.len() {
            return None;
        }
        for (i, pp) in base_parts.iter().enumerate() {
            if !pp.starts_with(':') && *pp != path_parts[i] {
                return None;
            }
        }
        let mut params = HashMap::new();
        for (i, pp) in base_parts.iter().enumerate() {
            if let Some(name) = pp.strip_prefix(':') {
                params.insert(name.to_string(), path_parts[i].to_string());
            }
        }
        return Some(params);
    }

    if pattern_parts.len() != path_parts.len() {
        return None;
    }

    let mut params = HashMap::new();
    for (i, pp) in pattern_parts.iter().enumerate() {
        if let Some(name) = pp.strip_prefix(':') {
            params.insert(name.to_string(), path_parts[i].to_string());
        } else if *pp != path_parts[i] {
            return None;
        }
    }
    Some(params)
}

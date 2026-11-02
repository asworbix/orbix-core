use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
//  Agent Capabilities
// ─────────────────────────────────────────────

/// Skills/capabilities that an agent can declare.
/// Used for dynamic routing instead of hardcoded URL patterns.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentCapabilities {
    /// Free-form skill tags, e.g. ["crm", "sales", "lead-management"]
    #[serde(default)]
    pub skills: Vec<String>,
    /// URL path patterns the agent handles, e.g. ["/api/crm", "/api/sales"]
    #[serde(default)]
    pub route_patterns: Vec<String>,
    /// Keywords that trigger routing to this agent
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Maximum priority (higher = preferred when multiple agents match)
    #[serde(default)]
    pub priority: i32,
}

impl Default for AgentCapabilities {
    fn default() -> Self {
        Self {
            skills: Vec::new(),
            route_patterns: Vec::new(),
            keywords: Vec::new(),
            priority: 0,
        }
    }
}

// ─────────────────────────────────────────────
//  Route Match Result
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RouteMatch {
    pub agent_id: String,
    pub agent_name: String,
    pub score: i32,
    pub reason: String,
}

// ─────────────────────────────────────────────
//  Generic Router
// ─────────────────────────────────────────────

/// Represents a routable agent with its ID, name, capabilities, and metadata.
pub struct RoutableAgent<'a> {
    pub id: &'a str,
    pub name: &'a str,
    pub enabled: bool,
    pub is_default: bool,
    pub capabilities: &'a AgentCapabilities,
    pub connector_ids: &'a [String],
}

/// Route a workflow step to the best agent.
/// Priority order:
///   1. Explicit connector binding (agent owns the connector)
///   2. Route pattern match (agent's route_patterns match the command URL)
///   3. Keyword match (agent's keywords appear in step label or prompt)
///   4. Skill match (agent's skills match step context)
///   5. Default agent fallback
pub fn route_step(
    agents: &[RoutableAgent],
    connector_id: Option<&str>,
    command_url: Option<&str>,
    step_label: &str,
    step_prompt: Option<&str>,
) -> Option<RouteMatch> {
    let mut candidates: Vec<RouteMatch> = Vec::new();

    for agent in agents.iter().filter(|a| a.enabled) {
        let mut score: i32 = 0;
        let mut reasons: Vec<&str> = Vec::new();

        // 1. Connector binding (highest priority: +100)
        if let Some(cid) = connector_id {
            if agent.connector_ids.contains(&cid.to_string()) {
                score += 100;
                reasons.push("connector-bound");
            }
        }

        // 2. Route pattern match (+50 per match)
        if let Some(url) = command_url {
            let url_lower = url.to_lowercase();
            for pattern in &agent.capabilities.route_patterns {
                if url_lower.contains(&pattern.to_lowercase()) {
                    score += 50;
                    reasons.push("route-pattern");
                    break;
                }
            }
        }

        // 3. Keyword match (+10 per keyword)
        let text = format!("{} {}", step_label, step_prompt.unwrap_or("")).to_lowercase();
        for kw in &agent.capabilities.keywords {
            if text.contains(&kw.to_lowercase()) {
                score += 10;
                reasons.push("keyword");
            }
        }

        // 4. Skill match (+5 per skill found in text)
        for skill in &agent.capabilities.skills {
            if text.contains(&skill.to_lowercase()) {
                score += 5;
                reasons.push("skill");
            }
        }

        // Add agent priority bonus
        score += agent.capabilities.priority;

        if score > 0 {
            candidates.push(RouteMatch {
                agent_id: agent.id.to_string(),
                agent_name: agent.name.to_string(),
                score,
                reason: reasons.iter().collect::<std::collections::HashSet<_>>()
                    .into_iter().copied().collect::<Vec<_>>().join(", "),
            });
        }
    }

    // Sort by score descending
    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    if let Some(best) = candidates.into_iter().next() {
        return Some(best);
    }

    // Fallback: default agent
    agents.iter().find(|a| a.is_default && a.enabled).map(|a| RouteMatch {
        agent_id: a.id.to_string(),
        agent_name: a.name.to_string(),
        score: 0,
        reason: "default".into(),
    })
}

/// Route a chat message to the best agent based on content analysis.
/// Used for auto-routing in send_message when no explicit agent_id is provided.
pub fn route_message(
    agents: &[RoutableAgent],
    message_content: &str,
) -> Option<RouteMatch> {
    let mut candidates: Vec<RouteMatch> = Vec::new();
    let text_lower = message_content.to_lowercase();

    for agent in agents.iter().filter(|a| a.enabled) {
        let mut score: i32 = 0;
        let mut reasons: Vec<&str> = Vec::new();

        // Keyword match (+10 per keyword)
        for kw in &agent.capabilities.keywords {
            if text_lower.contains(&kw.to_lowercase()) {
                score += 10;
                reasons.push("keyword");
            }
        }

        // Skill match (+5 per skill)
        for skill in &agent.capabilities.skills {
            if text_lower.contains(&skill.to_lowercase()) {
                score += 5;
                reasons.push("skill");
            }
        }

        // Priority bonus
        score += agent.capabilities.priority;

        if score > 0 {
            candidates.push(RouteMatch {
                agent_id: agent.id.to_string(),
                agent_name: agent.name.to_string(),
                score,
                reason: reasons.iter().collect::<std::collections::HashSet<_>>()
                    .into_iter().copied().collect::<Vec<_>>().join(", "),
            });
        }
    }

    candidates.sort_by(|a, b| b.score.cmp(&a.score));

    if let Some(best) = candidates.into_iter().next() {
        return Some(best);
    }

    // Fallback: default agent
    agents.iter().find(|a| a.is_default && a.enabled).map(|a| RouteMatch {
        agent_id: a.id.to_string(),
        agent_name: a.name.to_string(),
        score: 0,
        reason: "default".into(),
    })
}

/// Pre-built capability sets for common agent roles
pub fn preset_capabilities(role: &str) -> AgentCapabilities {
    match role {
        "orchestrator" => AgentCapabilities {
            skills: vec!["coordination".into(), "planning".into(), "analysis".into(), "summarization".into()],
            route_patterns: vec!["/api/dashboard".into()],
            keywords: vec!["overview".into(), "summary".into(), "report".into(), "plan".into(), "coordinate".into()],
            priority: 1,
        },
        "crm" => AgentCapabilities {
            skills: vec!["crm".into(), "sales".into(), "customer".into(), "lead".into()],
            route_patterns: vec!["/api/crm".into()],
            keywords: vec!["customer".into(), "lead".into(), "opportunity".into(), "deal".into(), "pipeline".into(), "contact".into(), "account".into(), "crm".into()],
            priority: 0,
        },
        "finance" => AgentCapabilities {
            skills: vec!["finance".into(), "accounting".into(), "budget".into(), "invoice".into()],
            route_patterns: vec!["/api/finance".into()],
            keywords: vec!["revenue".into(), "budget".into(), "invoice".into(), "expense".into(), "payment".into(), "profit".into(), "financial".into(), "finance".into()],
            priority: 0,
        },
        "erp" => AgentCapabilities {
            skills: vec!["erp".into(), "inventory".into(), "supply-chain".into(), "procurement".into()],
            route_patterns: vec!["/api/erp".into()],
            keywords: vec!["inventory".into(), "order".into(), "supply".into(), "procurement".into(), "warehouse".into(), "stock".into(), "erp".into()],
            priority: 0,
        },
        "ppm" => AgentCapabilities {
            skills: vec!["project".into(), "portfolio".into(), "task".into(), "milestone".into()],
            route_patterns: vec!["/api/ppm".into()],
            keywords: vec!["project".into(), "task".into(), "milestone".into(), "sprint".into(), "deadline".into(), "resource".into(), "ppm".into()],
            priority: 0,
        },
        "support" => AgentCapabilities {
            skills: vec!["support".into(), "helpdesk".into(), "ticket".into(), "issue".into()],
            route_patterns: vec!["/api/support".into()],
            keywords: vec!["ticket".into(), "issue".into(), "support".into(), "bug".into(), "incident".into(), "help".into(), "resolve".into()],
            priority: 0,
        },
        // ── Logistics demo presets ──
        "logi-orchestrator" => AgentCapabilities {
            skills: vec!["coordination".into(), "planning".into(), "sql".into(), "logistics".into(), "analytics".into()],
            route_patterns: vec!["/api/logi/dashboard".into(), "/api/logi/analytics".into()],
            keywords: vec!["overview".into(), "summary".into(), "logistics".into(), "fleet".into(), "kpi".into(), "coordinate".into()],
            priority: 2,
        },
        "warehouse" => AgentCapabilities {
            skills: vec!["warehouse".into(), "inventory".into(), "sql".into(), "wms".into(), "picking".into()],
            route_patterns: vec!["/api/logi/warehouse".into(), "/api/logi/inventory".into()],
            keywords: vec!["warehouse".into(), "bin".into(), "pick".into(), "pack".into(), "stock".into(), "storage".into(), "inbound".into(), "outbound".into()],
            priority: 0,
        },
        "routing" => AgentCapabilities {
            skills: vec!["routing".into(), "optimization".into(), "sql".into(), "geospatial".into(), "tsp".into()],
            route_patterns: vec!["/api/logi/routes".into(), "/api/logi/optimization".into()],
            keywords: vec!["route".into(), "optimize".into(), "distance".into(), "eta".into(), "delivery".into(), "miles".into(), "stops".into()],
            priority: 0,
        },
        "freight" => AgentCapabilities {
            skills: vec!["freight".into(), "carrier".into(), "sql".into(), "rate-shopping".into(), "bol".into()],
            route_patterns: vec!["/api/logi/freight".into(), "/api/logi/carriers".into()],
            keywords: vec!["freight".into(), "carrier".into(), "shipment".into(), "ltl".into(), "ftl".into(), "rate".into(), "bol".into(), "tender".into()],
            priority: 0,
        },
        "logi-inventory" => AgentCapabilities {
            skills: vec!["inventory".into(), "demand-planning".into(), "sql".into(), "forecasting".into()],
            route_patterns: vec!["/api/logi/inventory".into(), "/api/logi/demand".into()],
            keywords: vec!["inventory".into(), "sku".into(), "reorder".into(), "safety-stock".into(), "abc-analysis".into(), "turnover".into()],
            priority: 0,
        },
        "customs" => AgentCapabilities {
            skills: vec!["customs".into(), "compliance".into(), "sql".into(), "hs-codes".into(), "trade".into()],
            route_patterns: vec!["/api/logi/customs".into(), "/api/logi/compliance".into()],
            keywords: vec!["customs".into(), "tariff".into(), "hs-code".into(), "import".into(), "export".into(), "duty".into(), "compliance".into()],
            priority: 0,
        },
        "fleet-tracker" => AgentCapabilities {
            skills: vec!["fleet".into(), "telematics".into(), "sql".into(), "gps".into(), "maintenance".into()],
            route_patterns: vec!["/api/logi/fleet".into(), "/api/logi/telematics".into()],
            keywords: vec!["truck".into(), "vehicle".into(), "driver".into(), "gps".into(), "fuel".into(), "maintenance".into(), "eld".into()],
            priority: 0,
        },
        "demand-forecast" => AgentCapabilities {
            skills: vec!["forecasting".into(), "ml".into(), "sql".into(), "time-series".into(), "demand".into()],
            route_patterns: vec!["/api/logi/forecast".into(), "/api/logi/demand".into()],
            keywords: vec!["forecast".into(), "demand".into(), "predict".into(), "seasonal".into(), "trend".into(), "volume".into()],
            priority: 0,
        },
        "order-mgmt" => AgentCapabilities {
            skills: vec!["orders".into(), "fulfillment".into(), "sql".into(), "oms".into()],
            route_patterns: vec!["/api/logi/orders".into(), "/api/logi/fulfillment".into()],
            keywords: vec!["order".into(), "fulfill".into(), "backorder".into(), "allocation".into(), "promise-date".into(), "sla".into()],
            priority: 0,
        },
        "returns" => AgentCapabilities {
            skills: vec!["returns".into(), "reverse-logistics".into(), "sql".into(), "rma".into()],
            route_patterns: vec!["/api/logi/returns".into(), "/api/logi/rma".into()],
            keywords: vec!["return".into(), "rma".into(), "refund".into(), "damage".into(), "reverse".into(), "disposition".into()],
            priority: 0,
        },
        "yard" => AgentCapabilities {
            skills: vec!["yard".into(), "dock".into(), "sql".into(), "scheduling".into()],
            route_patterns: vec!["/api/logi/yard".into(), "/api/logi/dock".into()],
            keywords: vec!["yard".into(), "dock".into(), "trailer".into(), "gate".into(), "appointment".into(), "dwell".into()],
            priority: 0,
        },
        "cost-analyst" => AgentCapabilities {
            skills: vec!["cost".into(), "analytics".into(), "sql".into(), "margin".into(), "tcl".into()],
            route_patterns: vec!["/api/logi/cost".into(), "/api/logi/analytics".into()],
            keywords: vec!["cost".into(), "margin".into(), "spend".into(), "per-mile".into(), "freight-cost".into(), "budget".into()],
            priority: 0,
        },
        _ => AgentCapabilities::default(),
    }
}

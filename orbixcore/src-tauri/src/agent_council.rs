use serde::{Deserialize, Serialize};
use chrono::Utc;
use uuid::Uuid;

// ─────────────────────────────────────────────
//  Council Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Approve,
    Flag,
    Reject,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CouncilScope {
    Always,      // All outputs pass through council
    HighStakes,  // External comms, financial data, write ops
    Sampling,    // Configurable % of outputs reviewed
    Never,       // Low-risk, internal-only
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ValidatorResult {
    pub validator: String,
    pub verdict: Verdict,
    pub reason: String,
    pub details: Option<String>,
    pub duration_ms: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CouncilDecision {
    pub id: String,
    pub output_id: String,
    pub agent_name: String,
    pub final_verdict: Verdict,
    pub validator_results: Vec<ValidatorResult>,
    pub original_output: String,
    pub revised_output: Option<String>,
    pub total_duration_ms: u64,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CouncilConfig {
    pub scope: CouncilScope,
    pub sampling_rate: f32,   // 0.0-1.0, used when scope = Sampling
    pub validators_enabled: Vec<String>, // which validators to run
    /// Token budget the cost auditor compares usage against. Defaults to
    /// the streaming max_tokens (16384). Was previously a hardcoded 10000
    /// at each call site (W2.7).
    #[serde(default = "default_token_budget")]
    pub token_budget: u64,
}

fn default_token_budget() -> u64 { 16384 }

/// Rough token estimate from text length (~4 chars/token) so the cost
/// auditor has a real number to judge instead of a hardcoded 0 (W2.7).
pub fn estimate_tokens(text: &str) -> u64 {
    (text.chars().count() as u64 / 4).max(1)
}

impl Default for CouncilConfig {
    fn default() -> Self {
        Self {
            scope: CouncilScope::HighStakes,
            sampling_rate: 0.2,
            validators_enabled: vec![
                "schema".into(),
                "fact_checker".into(),
                "policy".into(),
                "consistency".into(),
                "cost_auditor".into(),
            ],
            token_budget: default_token_budget(),
        }
    }
}

// ─────────────────────────────────────────────
//  Council Store
// ─────────────────────────────────────────────

pub struct CouncilStore {
    pub config: CouncilConfig,
    pub decisions: Vec<CouncilDecision>,
    pub policies: Vec<PolicyRule>,
}

impl Default for CouncilStore {
    fn default() -> Self {
        Self {
            config: CouncilConfig::default(),
            decisions: Vec::new(),
            policies: vec![
                PolicyRule { id: "no_pii".into(), name: "No PII in outputs".into(), pattern: r"(?i)\b\d{3}-\d{2}-\d{4}\b|\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b".into(), action: "flag".into() },
                PolicyRule { id: "no_profanity".into(), name: "No profanity".into(), pattern: r"(?i)\b(damn|hell|shit|fuck|crap)\b".into(), action: "reject".into() },
                PolicyRule { id: "no_promises".into(), name: "No financial promises".into(), pattern: r"(?i)\b(guarantee|guaranteed|promise|assured)\b.{0,30}\b(return|profit|income|revenue)\b".into(), action: "flag".into() },
            ],
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PolicyRule {
    pub id: String,
    pub name: String,
    pub pattern: String, // regex
    pub action: String,  // "flag" or "reject"
}

// ─────────────────────────────────────────────
//  Validator implementations
// ─────────────────────────────────────────────

/// Schema Validator - checks if output matches expected structure
pub fn validate_schema(output: &str, expected_format: Option<&str>) -> ValidatorResult {
    let start = std::time::Instant::now();

    let (verdict, reason) = if let Some(fmt) = expected_format {
        match fmt {
            "json" => {
                if serde_json::from_str::<serde_json::Value>(output).is_ok() {
                    (Verdict::Approve, "Valid JSON structure".into())
                } else {
                    (Verdict::Flag, "Output is not valid JSON".into())
                }
            }
            "markdown" => {
                if output.contains('#') || output.contains('*') || output.contains('-') {
                    (Verdict::Approve, "Contains markdown formatting".into())
                } else {
                    (Verdict::Approve, "Plain text output (no formatting required)".into())
                }
            }
            _ => (Verdict::Approve, "No specific schema expected".into()),
        }
    } else {
        // Basic sanity: non-empty, not just whitespace
        if output.trim().is_empty() {
            (Verdict::Reject, "Output is empty".into())
        } else if output.len() < 5 {
            (Verdict::Flag, "Output is suspiciously short".into())
        } else {
            (Verdict::Approve, "Output has content".into())
        }
    };

    ValidatorResult {
        validator: "schema".into(),
        verdict,
        reason,
        details: None,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Fact Checker - cross-references claims against input context
pub fn validate_facts(output: &str, input_context: &str) -> ValidatorResult {
    let start = std::time::Instant::now();

    // Extract numbers from output and check if they appear in input
    let output_numbers: Vec<&str> = output.split(|c: char| !c.is_ascii_digit() && c != '.' && c != ',')
        .filter(|s| s.len() > 1 && s.chars().any(|c| c.is_ascii_digit()))
        .collect();

    let mut fabricated = Vec::new();
    for num in &output_numbers {
        // Skip common non-data numbers
        let clean = num.replace(',', "");
        if clean.parse::<f64>().ok().map(|n| n > 100.0).unwrap_or(false) {
            // Large number - check if it appears in input
            if !input_context.contains(*num) && !input_context.contains(&clean) {
                fabricated.push(num.to_string());
            }
        }
    }

    let (verdict, reason) = if fabricated.len() > 3 {
        (Verdict::Reject, format!("Multiple numbers not found in source data: {:?}", &fabricated[..3]))
    } else if !fabricated.is_empty() {
        (Verdict::Flag, format!("Numbers not found in source: {:?}. May be calculated or hallucinated.", fabricated))
    } else {
        (Verdict::Approve, "Claims appear consistent with input data".into())
    };

    ValidatorResult {
        validator: "fact_checker".into(),
        verdict,
        reason,
        details: if fabricated.is_empty() { None } else { Some(format!("Unverified: {:?}", fabricated)) },
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Policy Validator - checks organisational rules
pub fn validate_policy(output: &str, policies: &[PolicyRule]) -> ValidatorResult {
    let start = std::time::Instant::now();

    let mut violations = Vec::new();
    let mut worst_verdict = Verdict::Approve;

    for rule in policies {
        if let Ok(re) = regex::Regex::new(&rule.pattern) {
            if re.is_match(output) {
                let v = if rule.action == "reject" { Verdict::Reject } else { Verdict::Flag };
                if v == Verdict::Reject { worst_verdict = Verdict::Reject; }
                else if worst_verdict != Verdict::Reject { worst_verdict = Verdict::Flag; }
                violations.push(rule.name.clone());
            }
        }
    }

    let reason = if violations.is_empty() {
        "No policy violations detected".into()
    } else {
        format!("Policy violations: {}", violations.join(", "))
    };

    ValidatorResult {
        validator: "policy".into(),
        verdict: worst_verdict,
        reason,
        details: if violations.is_empty() { None } else { Some(violations.join(", ")) },
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Consistency Checker - compares with prior outputs in the workflow
pub fn validate_consistency(output: &str, prior_outputs: &[String]) -> ValidatorResult {
    let start = std::time::Instant::now();

    if prior_outputs.is_empty() {
        return ValidatorResult {
            validator: "consistency".into(),
            verdict: Verdict::Approve,
            reason: "No prior outputs to compare against".into(),
            details: None,
            duration_ms: start.elapsed().as_millis() as u64,
        };
    }

    // Check for contradictions: simple heuristic - if output says "no" or "none"
    // but prior output listed items, flag it
    let output_lower = output.to_lowercase();
    let has_negation = output_lower.contains("no results") || output_lower.contains("none found")
        || output_lower.contains("not found") || output_lower.contains("empty");

    let prior_had_data = prior_outputs.iter().any(|p| {
        let pl = p.to_lowercase();
        pl.contains("found") || pl.contains("total") || pl.contains("result")
    });

    let (verdict, reason) = if has_negation && prior_had_data {
        (Verdict::Flag, "Output claims no data but prior steps found results - possible inconsistency".into())
    } else {
        (Verdict::Approve, "Output is consistent with workflow context".into())
    };

    ValidatorResult {
        validator: "consistency".into(),
        verdict,
        reason,
        details: None,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

/// Cost Auditor - checks token/budget limits
pub fn validate_cost(tokens_used: u64, max_tokens: u64) -> ValidatorResult {
    let start = std::time::Instant::now();

    let ratio = tokens_used as f64 / max_tokens.max(1) as f64;
    let (verdict, reason) = if ratio > 1.0 {
        (Verdict::Reject, format!("Token budget exceeded: {} / {} ({:.0}%)", tokens_used, max_tokens, ratio * 100.0))
    } else if ratio > 0.8 {
        (Verdict::Flag, format!("Token usage high: {} / {} ({:.0}%)", tokens_used, max_tokens, ratio * 100.0))
    } else {
        (Verdict::Approve, format!("Token usage within budget: {} / {} ({:.0}%)", tokens_used, max_tokens, ratio * 100.0))
    };

    ValidatorResult {
        validator: "cost_auditor".into(),
        verdict,
        reason,
        details: None,
        duration_ms: start.elapsed().as_millis() as u64,
    }
}

// ─────────────────────────────────────────────
//  Council Runner
// ─────────────────────────────────────────────

/// Determine if an output should go through council review
pub fn should_review(config: &CouncilConfig, is_high_stakes: bool) -> bool {
    match config.scope {
        CouncilScope::Always => true,
        CouncilScope::HighStakes => is_high_stakes,
        CouncilScope::Sampling => {
            let r: f32 = (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap_or_default()
                .subsec_nanos() as f32) / 1_000_000_000.0;
            r < config.sampling_rate
        }
        CouncilScope::Never => false,
    }
}

/// Run the full council on an output. Returns the consensus decision.
pub fn run_council(
    output: &str,
    agent_name: &str,
    input_context: &str,
    prior_outputs: &[String],
    tokens_used: u64,
    max_tokens: u64,
    expected_format: Option<&str>,
    config: &CouncilConfig,
    policies: &[PolicyRule],
) -> CouncilDecision {
    let start = std::time::Instant::now();
    let mut results = Vec::new();

    if config.validators_enabled.contains(&"schema".to_string()) {
        results.push(validate_schema(output, expected_format));
    }
    if config.validators_enabled.contains(&"fact_checker".to_string()) {
        results.push(validate_facts(output, input_context));
    }
    if config.validators_enabled.contains(&"policy".to_string()) {
        results.push(validate_policy(output, policies));
    }
    if config.validators_enabled.contains(&"consistency".to_string()) {
        results.push(validate_consistency(output, prior_outputs));
    }
    if config.validators_enabled.contains(&"cost_auditor".to_string()) {
        results.push(validate_cost(tokens_used, max_tokens));
    }

    // Consensus: any reject = reject, any flag = flag, else approve
    let final_verdict = if results.iter().any(|r| r.verdict == Verdict::Reject) {
        Verdict::Reject
    } else if results.iter().any(|r| r.verdict == Verdict::Flag) {
        Verdict::Flag
    } else {
        Verdict::Approve
    };

    CouncilDecision {
        id: Uuid::new_v4().to_string(),
        output_id: Uuid::new_v4().to_string(),
        agent_name: agent_name.to_string(),
        final_verdict,
        validator_results: results,
        original_output: output.to_string(),
        revised_output: None,
        total_duration_ms: start.elapsed().as_millis() as u64,
        timestamp: Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod cost_tests {
    use super::*;

    #[test]
    fn estimate_tokens_scales_with_length() {
        assert_eq!(estimate_tokens(""), 1); // floor at 1
        assert!(estimate_tokens(&"x".repeat(400)) >= 90);
    }

    #[test]
    fn cost_auditor_flags_and_rejects_by_ratio() {
        let budget = 1000;
        assert_eq!(validate_cost(100, budget).verdict, Verdict::Approve);
        assert_eq!(validate_cost(850, budget).verdict, Verdict::Flag);
        assert_eq!(validate_cost(1200, budget).verdict, Verdict::Reject);
    }
}

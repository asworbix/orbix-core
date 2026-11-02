// ─────────────────────────────────────────────
//  Secure Relay - Privacy bridge between local and cloud agents
//
//  Flow:
//    1. Local agent analyses confidential document
//    2. Local agent generates a sanitised query (stripped of PII/secrets)
//    3. Relay validates the query against redaction policies
//    4. Cloud agent receives ONLY the sanitised query
//    5. Cloud agent returns results
//    6. Local agent compares results with the original document
//
//  Design principles:
//    - Original document content NEVER leaves the device
//    - No logs of raw content are stored in cloud-bound messages
//    - Redacted terms are replaced with opaque tokens ([REDACTED-xxxx])
//    - Full local audit trail of what was redacted and why
// ─────────────────────────────────────────────

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use regex::Regex;

// ─────────────────────────────────────────────
//  Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RelaySession {
    pub id: String,
    pub name: String,
    pub status: RelayStatus,
    pub local_agent_id: String,
    pub cloud_agent_id: String,
    pub policy: RedactionPolicy,
    pub steps: Vec<RelayStep>,
    pub redaction_log: Vec<RedactionEntry>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RelayStatus {
    Idle,
    LocalAnalysing,
    Sanitising,
    CloudQuerying,
    LocalComparing,
    Completed,
    Failed,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RelayStep {
    pub id: String,
    pub step_type: RelayStepType,
    pub status: RelayStepStatus,
    pub input: Option<String>,
    pub output: Option<String>,
    pub redaction_count: u32,
    pub timestamp: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RelayStepType {
    LocalAnalyse,
    Sanitise,
    CloudQuery,
    LocalCompare,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum RelayStepStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

// ─────────────────────────────────────────────
//  Redaction Policy
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RedactionPolicy {
    /// Strip email addresses
    pub redact_emails: bool,
    /// Strip phone numbers
    pub redact_phones: bool,
    /// Strip monetary amounts
    pub redact_amounts: bool,
    /// Strip dates (specific dates, not relative references)
    pub redact_dates: bool,
    /// Strip proper names (capitalised multi-word sequences)
    pub redact_names: bool,
    /// Strip anything matching these custom regex patterns
    pub custom_patterns: Vec<CustomPattern>,
    /// Specific terms to always redact (company names, project codenames, etc.)
    pub blocked_terms: Vec<String>,
    /// Maximum allowed output length for cloud query (truncate if longer)
    pub max_query_length: usize,
    /// If true, the sanitiser also strips any quoted text ("> ..." lines)
    pub strip_quotes: bool,
}

impl Default for RedactionPolicy {
    fn default() -> Self {
        Self {
            redact_emails: true,
            redact_phones: true,
            redact_amounts: true,
            redact_dates: false,
            redact_names: true,
            custom_patterns: vec![],
            blocked_terms: vec![],
            max_query_length: 2000,
            strip_quotes: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CustomPattern {
    pub name: String,
    pub regex: String,
}

// ─────────────────────────────────────────────
//  Redaction log - local-only audit trail
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RedactionEntry {
    pub token: String,           // e.g. "[REDACTED-a3f2]"
    pub category: String,        // "email", "phone", "name", "custom", "blocked_term"
    pub original_length: usize,  // length of redacted text (NOT the text itself for safety)
    pub timestamp: String,
}

// ─────────────────────────────────────────────
//  Relay Store
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct RelayStore {
    pub sessions: Vec<RelaySession>,
}

impl RelayStore {
    pub fn save(&self) {
        let path = crate::local_llm::data_dir().join("relay_sessions.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            crate::storage::atomic_write(path, json).ok();
        }
    }

    pub fn load() -> Self {
        let path = crate::local_llm::data_dir().join("relay_sessions.json");
        if let Ok(data) = std::fs::read_to_string(&path) {
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Self::default()
        }
    }
}

// ─────────────────────────────────────────────
//  Sanitisation engine
// ─────────────────────────────────────────────

pub struct SanitiseResult {
    pub sanitised_text: String,
    pub redactions: Vec<RedactionEntry>,
    pub redaction_count: u32,
}

/// Apply redaction policy to text, replacing sensitive content with opaque tokens.
/// Returns the sanitised text and a log of what was redacted (without storing the originals).
pub fn sanitise(text: &str, policy: &RedactionPolicy) -> SanitiseResult {
    let mut result = text.to_string();
    let mut redactions: Vec<RedactionEntry> = vec![];
    let now = Utc::now().to_rfc3339();

    // Helper: replace all matches of a regex with redaction tokens
    fn apply_regex(result: &mut String, redactions: &mut Vec<RedactionEntry>, now: &str, pattern: &str, category: &str) {
        if let Ok(re) = Regex::new(pattern) {
            let snap = result.clone();
            let matches: Vec<_> = re.find_iter(&snap).map(|m| (m.start(), m.end(), m.as_str().len())).collect();
            let mut new_result = snap;

            for &(start, end, original_len) in matches.iter().rev() {
                let token_id = &Uuid::new_v4().to_string()[..4];
                let token = format!("[REDACTED-{}]", token_id);

                redactions.push(RedactionEntry {
                    token: token.clone(),
                    category: category.to_string(),
                    original_length: original_len,
                    timestamp: now.to_string(),
                });

                new_result.replace_range(start..end, &token);
            }
            *result = new_result;
        }
    }

    // ── Built-in patterns ──

    if policy.redact_emails {
        apply_regex(&mut result, &mut redactions, &now, r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}", "email");
    }

    if policy.redact_phones {
        // International prefix + 2-digit groups (common EU format like
        // +45 12 34 56 78). Checked first so the general pattern below
        // does not leave a partial match behind.
        apply_regex(&mut result, &mut redactions, &now, r"\+\d{1,3}(?:[\s\-]\d{2}){3,5}", "phone");
        apply_regex(&mut result, &mut redactions, &now, r"(?:\+?\d{1,3}[\s\-]?)?\(?\d{2,4}\)?[\s\-]?\d{3,4}[\s\-]?\d{3,4}", "phone");
    }

    if policy.redact_amounts {
        apply_regex(&mut result, &mut redactions, &now, r"[\$\€\£]\s?\d[\d,]*\.?\d*\s?(?:million|billion|[MBKmk])?", "amount");
        apply_regex(&mut result, &mut redactions, &now, r"\d[\d,]*\.?\d*\s?(?:USD|EUR|GBP|AUD|CAD|CHF)", "amount");
    }

    if policy.redact_dates {
        apply_regex(&mut result, &mut redactions, &now, r"\d{4}[\-/]\d{1,2}[\-/]\d{1,2}", "date");
        apply_regex(&mut result, &mut redactions, &now,
            r"(?:January|February|March|April|May|June|July|August|September|October|November|December)\s+\d{1,2},?\s*\d{4}",
            "date",
        );
    }

    if policy.redact_names {
        apply_regex(&mut result, &mut redactions, &now, r"\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+)+\b", "name");
    }

    // ── Blocked terms (exact match, case-insensitive) ──
    for term in &policy.blocked_terms {
        if term.is_empty() { continue; }
        let escaped = regex::escape(term);
        apply_regex(&mut result, &mut redactions, &now, &format!(r"(?i)\b{}\b", escaped), "blocked_term");
    }

    // ── Custom regex patterns ──
    for pat in &policy.custom_patterns {
        apply_regex(&mut result, &mut redactions, &now, &pat.regex, &format!("custom:{}", pat.name));
    }

    // ── Strip quoted lines ──
    if policy.strip_quotes {
        if let Ok(re) = Regex::new(r"(?m)^>.*$") {
            result = re.replace_all(&result, "[quoted content removed]").to_string();
        }
    }

    // ── Truncate if too long ──
    if policy.max_query_length > 0 && result.len() > policy.max_query_length {
        result.truncate(policy.max_query_length);
        result.push_str("... [truncated for privacy]");
    }

    let count = redactions.len() as u32;
    SanitiseResult {
        sanitised_text: result,
        redactions,
        redaction_count: count,
    }
}

// ─────────────────────────────────────────────
//  Session management
// ─────────────────────────────────────────────

pub fn create_session(
    name: &str,
    local_agent_id: &str,
    cloud_agent_id: &str,
    policy: Option<RedactionPolicy>,
) -> RelaySession {
    let now = Utc::now().to_rfc3339();
    RelaySession {
        id: Uuid::new_v4().to_string(),
        name: name.to_string(),
        status: RelayStatus::Idle,
        local_agent_id: local_agent_id.to_string(),
        cloud_agent_id: cloud_agent_id.to_string(),
        policy: policy.unwrap_or_default(),
        steps: vec![
            RelayStep {
                id: Uuid::new_v4().to_string(),
                step_type: RelayStepType::LocalAnalyse,
                status: RelayStepStatus::Pending,
                input: None,
                output: None,
                redaction_count: 0,
                timestamp: now.clone(),
            },
            RelayStep {
                id: Uuid::new_v4().to_string(),
                step_type: RelayStepType::Sanitise,
                status: RelayStepStatus::Pending,
                input: None,
                output: None,
                redaction_count: 0,
                timestamp: now.clone(),
            },
            RelayStep {
                id: Uuid::new_v4().to_string(),
                step_type: RelayStepType::CloudQuery,
                status: RelayStepStatus::Pending,
                input: None,
                output: None,
                redaction_count: 0,
                timestamp: now.clone(),
            },
            RelayStep {
                id: Uuid::new_v4().to_string(),
                step_type: RelayStepType::LocalCompare,
                status: RelayStepStatus::Pending,
                input: None,
                output: None,
                redaction_count: 0,
                timestamp: now.clone(),
            },
        ],
        redaction_log: vec![],
        created_at: now.clone(),
        updated_at: now,
    }
}

/// Build the prompt that tells the local agent to analyse the document and produce a safe query.
pub fn local_analyse_prompt(document_content: &str) -> String {
    format!(
        r#"You are analysing a CONFIDENTIAL document. Your task is to produce a short, generic research query that captures the TOPIC and DOMAIN of this document WITHOUT revealing any specific:
- Names of people, companies, or organisations
- Financial figures, valuations, or deal terms
- Dates, deadlines, or timelines
- Internal project codenames or proprietary terms
- Any personally identifiable information

The query should be useful for finding related PUBLIC research, papers, patents, or news.

CONFIDENTIAL DOCUMENT:
---
{}
---

Produce ONLY the sanitised research query (2-4 sentences max). Do not reference the document. Do not include disclaimers."#,
        document_content
    )
}

/// Build the prompt that tells the cloud agent to search with the sanitised query.
pub fn cloud_search_prompt(sanitised_query: &str) -> String {
    format!(
        r#"Research the following topic and provide a comprehensive summary of publicly available information, including:
- Recent academic papers or research
- Industry trends and developments
- Key players and companies in this space
- Relevant patents or technical advances
- Market data and projections (if applicable)

Be thorough but concise. Cite sources where possible.

RESEARCH QUERY:
{}

Provide your findings in a structured format."#,
        sanitised_query
    )
}

/// Build the prompt that tells the local agent to compare cloud results with the original document.
pub fn local_compare_prompt(document_content: &str, cloud_results: &str) -> String {
    format!(
        r#"You have the original CONFIDENTIAL document and research results from public sources.

Compare and analyse:
1. How does the document's content align with or differ from public research?
2. Are there novel findings in the document not covered publicly?
3. What public information validates or challenges the document's claims?
4. What gaps exist - what does public research cover that the document doesn't, and vice versa?

CONFIDENTIAL DOCUMENT:
---
{}
---

PUBLIC RESEARCH RESULTS:
---
{}
---

Provide a detailed comparison. This analysis stays LOCAL and will not be shared externally."#,
        document_content, cloud_results
    )
}

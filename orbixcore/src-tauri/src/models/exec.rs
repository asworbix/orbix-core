//! Execution and workflow I/O models.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use crate::workflow::StepType;

/// Stats about an execute_actions pass, so callers (e.g. bob_send) can tell
/// whether a turn that was *supposed* to act actually did.
#[derive(Default, Debug, Clone)]
pub struct ExecStats {
    /// `<action>` blocks found in the text.
    pub attempted: usize,
    /// Actions that parsed and ran (produced a confirmation).
    pub executed: usize,
    /// `<action>` blocks whose JSON could not be parsed even after repair.
    pub failed: usize,
    /// Actions that only parsed after the JSON-repair pass.
    pub repaired: usize,
    /// Human-readable label per executed action (the bob_step wording),
    /// so the chat can show WHAT Bob did as a step trail instead of
    /// either raw machinery or nothing.
    pub labels: Vec<String>,
}

/// Result of a workflow step with an explicit success flag, so the run loop
/// can stop/fail instead of treating an error string as a successful step.
pub struct StepOutcome {
    pub success: bool,
    pub display: String,
    pub raw: Option<serde_json::Value>,
}

#[derive(Serialize, Clone)]
pub struct SystemScan {
    pub ram_gb: u64,
    pub chip: String,
    pub os_version: String,
    pub recommended_model: String,
    pub recommended_model_name: String,
    pub recommended_reason: String,
    pub all_recommendations: Vec<ModelRecommendation>,
}

#[derive(Serialize, Clone)]
pub struct ModelRecommendation {
    pub model_id: String,
    pub model_name: String,
    pub size_gb: f64,
    pub fits: bool,
    pub reason: String,
    pub category: String, // "text" or "vision"
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AddStepInput {
    pub step_type: StepType,
    pub label: String,
    pub config: HashMap<String, serde_json::Value>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SaveAsTemplateInput {
    pub name: String,
    pub description: String,
    pub category: String,
}

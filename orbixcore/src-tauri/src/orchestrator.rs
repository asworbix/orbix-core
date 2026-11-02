use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Agent Chain Models
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChainMode {
    Sequential,   // A → B → C
    FanOut,       // Split across agents, merge results
    Fallback,     // Try A, if fail try B, then C
    Specialist,   // Route subtasks to best agent by keyword
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentChainNode {
    pub id: String,
    pub agent_id: String,
    pub agent_name: String,     // Snapshot for display
    pub order: usize,
    pub role_hint: String,      // e.g. "summarizer", "coder", "reviewer"
    pub route_keywords: Vec<String>,  // For specialist routing
    pub timeout_secs: u64,
    pub retry_on_fail: bool,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentChain {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mode: ChainMode,
    pub nodes: Vec<AgentChainNode>,
    pub created_at: String,
    pub updated_at: String,
}

// ─────────────────────────────────────────────
//  Chain Execution
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChainRunStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum NodeRunStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct NodeRunResult {
    pub node_id: String,
    pub agent_name: String,
    pub status: NodeRunStatus,
    pub output: String,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub tokens_used: u64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChainRunState {
    pub chain_id: String,
    pub chain_name: String,
    pub status: ChainRunStatus,
    pub mode: ChainMode,
    pub input: String,
    pub final_output: Option<String>,
    pub node_results: Vec<NodeRunResult>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub total_tokens: u64,
}

// ─────────────────────────────────────────────
//  Chain Templates
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ChainTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub mode: ChainMode,
    pub node_hints: Vec<TemplateNodeHint>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TemplateNodeHint {
    pub role_hint: String,
    pub description: String,
    pub route_keywords: Vec<String>,
}

pub fn builtin_templates() -> Vec<ChainTemplate> {
    vec![
        ChainTemplate {
            id: "draft_review".into(),
            name: "Draft → Review".into(),
            description: "One agent drafts, another reviews and refines".into(),
            mode: ChainMode::Sequential,
            node_hints: vec![
                TemplateNodeHint {
                    role_hint: "drafter".into(),
                    description: "Generates initial draft".into(),
                    route_keywords: vec![],
                },
                TemplateNodeHint {
                    role_hint: "reviewer".into(),
                    description: "Reviews and improves the draft".into(),
                    route_keywords: vec![],
                },
            ],
        },
        ChainTemplate {
            id: "research_synthesize".into(),
            name: "Research → Synthesize".into(),
            description: "Multiple agents research in parallel, one synthesizes".into(),
            mode: ChainMode::FanOut,
            node_hints: vec![
                TemplateNodeHint {
                    role_hint: "researcher_a".into(),
                    description: "Researches from perspective A".into(),
                    route_keywords: vec![],
                },
                TemplateNodeHint {
                    role_hint: "researcher_b".into(),
                    description: "Researches from perspective B".into(),
                    route_keywords: vec![],
                },
                TemplateNodeHint {
                    role_hint: "synthesizer".into(),
                    description: "Combines all research into final output".into(),
                    route_keywords: vec![],
                },
            ],
        },
        ChainTemplate {
            id: "code_pipeline".into(),
            name: "Code → Test → Review".into(),
            description: "Coder writes, tester validates, reviewer polishes".into(),
            mode: ChainMode::Sequential,
            node_hints: vec![
                TemplateNodeHint {
                    role_hint: "coder".into(),
                    description: "Writes the implementation".into(),
                    route_keywords: vec![],
                },
                TemplateNodeHint {
                    role_hint: "tester".into(),
                    description: "Generates tests and validates".into(),
                    route_keywords: vec![],
                },
                TemplateNodeHint {
                    role_hint: "reviewer".into(),
                    description: "Code review and refinement".into(),
                    route_keywords: vec![],
                },
            ],
        },
        ChainTemplate {
            id: "fallback_chain".into(),
            name: "Primary → Fallback".into(),
            description: "Try primary agent first, fall back to secondary on failure".into(),
            mode: ChainMode::Fallback,
            node_hints: vec![
                TemplateNodeHint {
                    role_hint: "primary".into(),
                    description: "Primary agent (fastest/cheapest)".into(),
                    route_keywords: vec![],
                },
                TemplateNodeHint {
                    role_hint: "fallback".into(),
                    description: "Fallback agent (more capable)".into(),
                    route_keywords: vec![],
                },
            ],
        },
        ChainTemplate {
            id: "specialist_router".into(),
            name: "Specialist Router".into(),
            description: "Route tasks to specialized agents by topic".into(),
            mode: ChainMode::Specialist,
            node_hints: vec![
                TemplateNodeHint {
                    role_hint: "code_specialist".into(),
                    description: "Handles code-related tasks".into(),
                    route_keywords: vec!["code".into(), "function".into(), "bug".into(), "implement".into()],
                },
                TemplateNodeHint {
                    role_hint: "writing_specialist".into(),
                    description: "Handles writing & content tasks".into(),
                    route_keywords: vec!["write".into(), "email".into(), "draft".into(), "article".into()],
                },
                TemplateNodeHint {
                    role_hint: "analysis_specialist".into(),
                    description: "Handles data & analysis tasks".into(),
                    route_keywords: vec!["analyze".into(), "data".into(), "report".into(), "metrics".into()],
                },
            ],
        },
    ]
}

// ─────────────────────────────────────────────
//  Orchestrator Store
// ─────────────────────────────────────────────

pub struct OrchestratorStore {
    pub chains: Vec<AgentChain>,
    pub run_history: Vec<ChainRunState>,
    pub active_run: Option<ChainRunState>,
}

impl Default for OrchestratorStore {
    fn default() -> Self {
        Self {
            chains: Vec::new(),
            run_history: Vec::new(),
            active_run: None,
        }
    }
}

impl OrchestratorStore {
    // ── Chain CRUD ──

    pub fn list_chains(&self) -> Vec<AgentChain> {
        self.chains.clone()
    }

    pub fn get_chain(&self, id: &str) -> Option<AgentChain> {
        self.chains.iter().find(|c| c.id == id).cloned()
    }

    pub fn create_chain(
        &mut self,
        name: String,
        description: String,
        mode: ChainMode,
        nodes: Vec<AgentChainNode>,
    ) -> AgentChain {
        let now = Utc::now().to_rfc3339();
        let chain = AgentChain {
            id: Uuid::new_v4().to_string(),
            name,
            description,
            mode,
            nodes,
            created_at: now.clone(),
            updated_at: now,
        };
        self.chains.push(chain.clone());
        chain
    }

    pub fn update_chain(
        &mut self,
        id: &str,
        name: Option<String>,
        description: Option<String>,
        mode: Option<ChainMode>,
        nodes: Option<Vec<AgentChainNode>>,
    ) -> Option<AgentChain> {
        if let Some(c) = self.chains.iter_mut().find(|c| c.id == id) {
            if let Some(n) = name { c.name = n; }
            if let Some(d) = description { c.description = d; }
            if let Some(m) = mode { c.mode = m; }
            if let Some(ns) = nodes { c.nodes = ns; }
            c.updated_at = Utc::now().to_rfc3339();
            Some(c.clone())
        } else {
            None
        }
    }

    pub fn delete_chain(&mut self, id: &str) -> bool {
        let len = self.chains.len();
        self.chains.retain(|c| c.id != id);
        self.chains.len() < len
    }

    // ── Execution ──

    pub fn start_run(&mut self, chain: &AgentChain, input: String) -> ChainRunState {
        let state = ChainRunState {
            chain_id: chain.id.clone(),
            chain_name: chain.name.clone(),
            status: ChainRunStatus::Running,
            mode: chain.mode.clone(),
            input,
            final_output: None,
            node_results: Vec::new(),
            started_at: Utc::now().to_rfc3339(),
            finished_at: None,
            total_tokens: 0,
        };
        self.active_run = Some(state.clone());
        state
    }

    pub fn record_node_result(&mut self, result: NodeRunResult) {
        if let Some(ref mut run) = self.active_run {
            run.total_tokens += result.tokens_used;
            run.node_results.push(result);
        }
    }

    pub fn complete_run(&mut self, final_output: String) -> Option<ChainRunState> {
        if let Some(ref mut run) = self.active_run {
            run.status = ChainRunStatus::Completed;
            run.final_output = Some(final_output);
            run.finished_at = Some(Utc::now().to_rfc3339());
            let completed = run.clone();
            self.run_history.insert(0, completed.clone());
            if self.run_history.len() > 100 {
                self.run_history.truncate(100);
            }
            self.active_run = None;
            Some(completed)
        } else {
            None
        }
    }

    pub fn fail_run(&mut self, error: String) -> Option<ChainRunState> {
        if let Some(ref mut run) = self.active_run {
            run.status = ChainRunStatus::Failed;
            run.final_output = Some(error);
            run.finished_at = Some(Utc::now().to_rfc3339());
            let failed = run.clone();
            self.run_history.insert(0, failed.clone());
            self.active_run = None;
            Some(failed)
        } else {
            None
        }
    }

    pub fn stop_run(&mut self) -> Option<ChainRunState> {
        if let Some(ref mut run) = self.active_run {
            run.status = ChainRunStatus::Stopped;
            run.finished_at = Some(Utc::now().to_rfc3339());
            let stopped = run.clone();
            self.run_history.insert(0, stopped.clone());
            self.active_run = None;
            Some(stopped)
        } else {
            None
        }
    }

    pub fn get_active_run(&self) -> Option<ChainRunState> {
        self.active_run.clone()
    }

    pub fn get_run_history(&self, limit: usize) -> Vec<ChainRunState> {
        self.run_history.iter().take(limit).cloned().collect()
    }

    // ── Specialist Routing ──

    /// Pick the best node for a given input based on keyword matching.
    pub fn route_specialist(chain: &AgentChain, input: &str) -> Option<AgentChainNode> {
        let input_lower = input.to_lowercase();
        let mut best: Option<(&AgentChainNode, usize)> = None;

        for node in &chain.nodes {
            let score = node.route_keywords.iter()
                .filter(|kw| input_lower.contains(&kw.to_lowercase()))
                .count();
            if score > 0 {
                if best.is_none() || score > best.unwrap().1 {
                    best = Some((node, score));
                }
            }
        }

        best.map(|(n, _)| n.clone())
            .or_else(|| chain.nodes.first().cloned()) // fallback to first
    }
}

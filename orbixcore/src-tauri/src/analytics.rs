use serde::{Deserialize, Serialize};
use chrono::{Utc, DateTime};

use crate::pipeline::{BoardStore, ItemStatus};
use crate::triggers::{TriggerStore, EventStatus};
use crate::workflow::WorkflowStore;
use crate::scheduler::SchedulerStore;

// ─────────────────────────────────────────────
//  Time Range
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TimeRange {
    Today,
    Week,
    Month,
}

impl TimeRange {
    pub fn hours(&self) -> i64 {
        match self {
            TimeRange::Today => 24,
            TimeRange::Week => 168,
            TimeRange::Month => 720,
        }
    }

    pub fn cutoff(&self) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::hours(self.hours())
    }
}

// ─────────────────────────────────────────────
//  Dashboard Metrics
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DashboardMetrics {
    pub summary: SummaryCards,
    pub pipeline: PipelineMetrics,
    pub triggers: TriggerMetrics,
    pub workflows: WorkflowMetrics,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SummaryCards {
    pub total_items: usize,
    pub active_workflows: usize,
    pub events_today: usize,
    pub stale_items: usize,
    pub active_schedules: usize,
}

// ── Pipeline Metrics ──

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct PipelineMetrics {
    pub items_per_stage: Vec<StageCount>,
    pub items_by_source: Vec<SourceCount>,
    pub throughput: Vec<DayCount>,
    pub stale_count: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct StageCount {
    pub stage_name: String,
    pub color: String,
    pub count: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SourceCount {
    pub source: String,
    pub count: usize,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DayCount {
    pub label: String,
    pub count: usize,
}

// ── Trigger Metrics ──

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TriggerMetrics {
    pub events_by_source: Vec<SourceCount>,
    pub events_by_day: Vec<DayCount>,
    pub top_rules: Vec<RuleHitCount>,
    pub failure_rate: f64,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct RuleHitCount {
    pub rule_name: String,
    pub count: usize,
}

// ── Workflow Metrics ──

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkflowMetrics {
    pub total_runs: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub runs_by_day: Vec<DayCount>,
    pub most_used: Vec<WorkflowUsage>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct WorkflowUsage {
    pub name: String,
    pub runs: usize,
}

// ─────────────────────────────────────────────
//  Compute Functions
// ─────────────────────────────────────────────

fn is_within(timestamp: &str, cutoff: &DateTime<Utc>) -> bool {
    timestamp.parse::<DateTime<Utc>>().map(|dt| dt >= *cutoff).unwrap_or(false)
}

fn day_label(timestamp: &str) -> String {
    if let Ok(dt) = timestamp.parse::<DateTime<Utc>>() {
        dt.format("%m/%d").to_string()
    } else {
        "?".into()
    }
}

pub fn compute_dashboard(
    pipeline: &BoardStore,
    triggers: &TriggerStore,
    workflows: &WorkflowStore,
    scheduler: &SchedulerStore,
    range: &TimeRange,
) -> DashboardMetrics {
    let cutoff = range.cutoff();

    // ── Summary cards ──
    let total_items = pipeline.items.iter()
        .filter(|i| i.status != ItemStatus::Archived)
        .count();
    let stale_items = pipeline.stale_items().len();
    let active_workflows = workflows.list().len();
    let events_today = {
        let today_cutoff = TimeRange::Today.cutoff();
        triggers.event_log.iter()
            .filter(|e| is_within(&e.timestamp, &today_cutoff))
            .count()
    };
    let active_schedules = scheduler.schedules.iter()
        .filter(|s| s.enabled)
        .count();

    // ── Pipeline metrics ──
    let mut stage_counts: Vec<StageCount> = Vec::new();
    for p in &pipeline.pipelines {
        for stage in &p.stages {
            let count = pipeline.items.iter()
                .filter(|i| i.pipeline_id.as_deref() == Some(&p.id)
                    && i.stage_id.as_deref() == Some(&stage.id)
                    && i.status != ItemStatus::Archived)
                .count();
            if count > 0 {
                stage_counts.push(StageCount {
                    stage_name: stage.name.clone(),
                    color: stage.color.clone(),
                    count,
                });
            }
        }
    }

    let mut source_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in &pipeline.items {
        if item.status == ItemStatus::Archived { continue; }
        let src = format!("{:?}", item.source).to_lowercase();
        *source_map.entry(src).or_default() += 1;
    }
    let items_by_source: Vec<SourceCount> = source_map.into_iter()
        .map(|(source, count)| SourceCount { source, count })
        .collect();

    // Throughput: items processed per day in range
    let mut throughput_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in &pipeline.items {
        if item.status == ItemStatus::Processed && is_within(&item.updated_at, &cutoff) {
            let label = day_label(&item.updated_at);
            *throughput_map.entry(label).or_default() += 1;
        }
    }
    let mut throughput: Vec<DayCount> = throughput_map.into_iter()
        .map(|(label, count)| DayCount { label, count })
        .collect();
    throughput.sort_by(|a, b| a.label.cmp(&b.label));

    // ── Trigger metrics ──
    let filtered_events: Vec<_> = triggers.event_log.iter()
        .filter(|e| is_within(&e.timestamp, &cutoff))
        .collect();

    let mut events_src_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &filtered_events {
        let src = format!("{:?}", e.source).to_lowercase();
        *events_src_map.entry(src).or_default() += 1;
    }
    let events_by_source: Vec<SourceCount> = events_src_map.into_iter()
        .map(|(source, count)| SourceCount { source, count })
        .collect();

    let mut events_day_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &filtered_events {
        let label = day_label(&e.timestamp);
        *events_day_map.entry(label).or_default() += 1;
    }
    let mut events_by_day: Vec<DayCount> = events_day_map.into_iter()
        .map(|(label, count)| DayCount { label, count })
        .collect();
    events_by_day.sort_by(|a, b| a.label.cmp(&b.label));

    let mut rule_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for e in &filtered_events {
        if let Some(name) = &e.rule_name {
            *rule_map.entry(name.clone()).or_default() += 1;
        }
    }
    let mut top_rules: Vec<RuleHitCount> = rule_map.into_iter()
        .map(|(rule_name, count)| RuleHitCount { rule_name, count })
        .collect();
    top_rules.sort_by(|a, b| b.count.cmp(&a.count));
    top_rules.truncate(5);

    let total_events = filtered_events.len();
    let failed_events = filtered_events.iter()
        .filter(|e| e.status == EventStatus::Failed)
        .count();
    let failure_rate = if total_events > 0 {
        (failed_events as f64) / (total_events as f64)
    } else {
        0.0
    };

    // ── Workflow metrics ──
    let run_logs: Vec<_> = scheduler.run_history.iter()
        .filter(|r| is_within(&r.started_at, &cutoff))
        .collect();

    let total_runs = run_logs.len();
    let success_count = run_logs.iter()
        .filter(|r| format!("{:?}", r.result).to_lowercase() == "success")
        .count();
    let failure_count = run_logs.iter()
        .filter(|r| format!("{:?}", r.result).to_lowercase() == "failed")
        .count();

    let mut runs_day_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &run_logs {
        let label = day_label(&r.started_at);
        *runs_day_map.entry(label).or_default() += 1;
    }
    let mut runs_by_day: Vec<DayCount> = runs_day_map.into_iter()
        .map(|(label, count)| DayCount { label, count })
        .collect();
    runs_by_day.sort_by(|a, b| a.label.cmp(&b.label));

    let mut wf_usage_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in &run_logs {
        *wf_usage_map.entry(r.workflow_name.clone()).or_default() += 1;
    }
    let mut most_used: Vec<WorkflowUsage> = wf_usage_map.into_iter()
        .map(|(name, runs)| WorkflowUsage { name, runs })
        .collect();
    most_used.sort_by(|a, b| b.runs.cmp(&a.runs));
    most_used.truncate(5);

    DashboardMetrics {
        summary: SummaryCards {
            total_items,
            active_workflows,
            events_today,
            stale_items,
            active_schedules,
        },
        pipeline: PipelineMetrics {
            items_per_stage: stage_counts,
            items_by_source,
            throughput,
            stale_count: stale_items,
        },
        triggers: TriggerMetrics {
            events_by_source,
            events_by_day,
            top_rules,
            failure_rate,
        },
        workflows: WorkflowMetrics {
            total_runs,
            success_count,
            failure_count,
            runs_by_day,
            most_used,
        },
    }
}

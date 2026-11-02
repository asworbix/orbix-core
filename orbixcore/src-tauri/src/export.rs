use serde::{Deserialize, Serialize};
use chrono::Utc;
use std::path::PathBuf;

use crate::pipeline::{BoardStore, BoardItem};
use crate::triggers::{TriggerStore, EventLogEntry};
use crate::scheduler::{SchedulerStore, ScheduleRunLog};

// ─────────────────────────────────────────────
//  Export Types
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    Csv,
    Json,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    Pipeline,
    EventLog,
    WorkflowRuns,
    FullBackup,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ExportOptions {
    pub report_type: ReportType,
    pub format: ExportFormat,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub source_filter: Option<String>,
    pub status_filter: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ExportResult {
    pub path: String,
    pub filename: String,
    pub size_bytes: u64,
    pub row_count: usize,
}

// ─────────────────────────────────────────────
//  Reports Directory
// ─────────────────────────────────────────────

pub fn reports_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = home.join("Documents").join("OrbixCore").join("Reports");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn generate_filename(report_type: &ReportType, format: &ExportFormat) -> String {
    let type_str = match report_type {
        ReportType::Pipeline => "pipeline",
        ReportType::EventLog => "event-log",
        ReportType::WorkflowRuns => "workflow-runs",
        ReportType::FullBackup => "full-backup",
    };
    let ext = match format {
        ExportFormat::Csv => "csv",
        ExportFormat::Json => "json",
    };
    let ts = Utc::now().format("%Y%m%d-%H%M%S");
    format!("report-{}-{}.{}", type_str, ts, ext)
}

fn is_within_range(timestamp: &str, from: &Option<String>, to: &Option<String>) -> bool {
    if let Ok(dt) = timestamp.parse::<chrono::DateTime<Utc>>() {
        if let Some(f) = from {
            if let Ok(fd) = f.parse::<chrono::DateTime<Utc>>() {
                if dt < fd { return false; }
            }
        }
        if let Some(t) = to {
            if let Ok(td) = t.parse::<chrono::DateTime<Utc>>() {
                if dt > td { return false; }
            }
        }
        true
    } else {
        true // include if timestamp can't be parsed
    }
}

// ─────────────────────────────────────────────
//  Pipeline Export
// ─────────────────────────────────────────────

fn pipeline_csv(store: &BoardStore, opts: &ExportOptions) -> (String, usize) {
    let mut csv = String::from("id,title,source,status,pipeline_id,stage_id,created_at,updated_at\n");
    let mut count = 0;

    for item in &store.items {
        if !is_within_range(&item.created_at, &opts.date_from, &opts.date_to) { continue; }
        if let Some(sf) = &opts.status_filter {
            let status_str = format!("{:?}", item.status).to_lowercase();
            if &status_str != sf { continue; }
        }
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            escape_csv(&item.id),
            escape_csv(&item.title),
            format!("{:?}", item.source).to_lowercase(),
            format!("{:?}", item.status).to_lowercase(),
            item.pipeline_id.as_deref().unwrap_or(""),
            item.stage_id.as_deref().unwrap_or(""),
            &item.created_at,
            &item.updated_at,
        ));
        count += 1;
    }
    (csv, count)
}

fn pipeline_json(store: &BoardStore, opts: &ExportOptions) -> (String, usize) {
    let items: Vec<&BoardItem> = store.items.iter()
        .filter(|i| is_within_range(&i.created_at, &opts.date_from, &opts.date_to))
        .filter(|i| {
            if let Some(sf) = &opts.status_filter {
                format!("{:?}", i.status).to_lowercase() == *sf
            } else { true }
        })
        .collect();
    let count = items.len();
    let json = serde_json::to_string_pretty(&items).unwrap_or_default();
    (json, count)
}

// ─────────────────────────────────────────────
//  Event Log Export
// ─────────────────────────────────────────────

fn event_log_csv(store: &TriggerStore, opts: &ExportOptions) -> (String, usize) {
    let mut csv = String::from("id,source,source_detail,rule_name,workflow_name,status,message,timestamp\n");
    let mut count = 0;

    for e in &store.event_log {
        if !is_within_range(&e.timestamp, &opts.date_from, &opts.date_to) { continue; }
        if let Some(sf) = &opts.source_filter {
            let src = format!("{:?}", e.source).to_lowercase();
            if &src != sf { continue; }
        }
        if let Some(sf) = &opts.status_filter {
            let st = format!("{:?}", e.status).to_lowercase();
            if &st != sf { continue; }
        }
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            escape_csv(&e.id),
            format!("{:?}", e.source).to_lowercase(),
            escape_csv(&e.source_detail),
            escape_csv(e.rule_name.as_deref().unwrap_or("")),
            escape_csv(e.workflow_name.as_deref().unwrap_or("")),
            format!("{:?}", e.status).to_lowercase(),
            escape_csv(&e.message),
            &e.timestamp,
        ));
        count += 1;
    }
    (csv, count)
}

fn event_log_json(store: &TriggerStore, opts: &ExportOptions) -> (String, usize) {
    let entries: Vec<&EventLogEntry> = store.event_log.iter()
        .filter(|e| is_within_range(&e.timestamp, &opts.date_from, &opts.date_to))
        .filter(|e| {
            if let Some(sf) = &opts.source_filter {
                format!("{:?}", e.source).to_lowercase() == *sf
            } else { true }
        })
        .filter(|e| {
            if let Some(sf) = &opts.status_filter {
                format!("{:?}", e.status).to_lowercase() == *sf
            } else { true }
        })
        .collect();
    let count = entries.len();
    let json = serde_json::to_string_pretty(&entries).unwrap_or_default();
    (json, count)
}

// ─────────────────────────────────────────────
//  Workflow Runs Export
// ─────────────────────────────────────────────

fn workflow_runs_csv(store: &SchedulerStore, opts: &ExportOptions) -> (String, usize) {
    let mut csv = String::from("id,schedule_id,workflow_id,workflow_name,result,started_at,finished_at,duration_ms\n");
    let mut count = 0;

    for r in &store.run_history {
        if !is_within_range(&r.started_at, &opts.date_from, &opts.date_to) { continue; }
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{}\n",
            escape_csv(&r.id),
            escape_csv(&r.schedule_id),
            escape_csv(&r.workflow_id),
            escape_csv(&r.workflow_name),
            format!("{:?}", r.result).to_lowercase(),
            &r.started_at,
            &r.finished_at,
            r.duration_ms,
        ));
        count += 1;
    }
    (csv, count)
}

fn workflow_runs_json(store: &SchedulerStore, opts: &ExportOptions) -> (String, usize) {
    let runs: Vec<&ScheduleRunLog> = store.run_history.iter()
        .filter(|r| is_within_range(&r.started_at, &opts.date_from, &opts.date_to))
        .collect();
    let count = runs.len();
    let json = serde_json::to_string_pretty(&runs).unwrap_or_default();
    (json, count)
}

// ─────────────────────────────────────────────
//  Full Backup
// ─────────────────────────────────────────────

#[derive(Serialize)]
struct FullBackup<'a> {
    pipeline: &'a BoardStore,
    event_log: &'a [EventLogEntry],
    run_history: &'a [ScheduleRunLog],
    exported_at: String,
}

fn full_backup_json(
    pipeline: &BoardStore,
    triggers: &TriggerStore,
    scheduler: &SchedulerStore,
) -> (String, usize) {
    let backup = FullBackup {
        pipeline,
        event_log: &triggers.event_log,
        run_history: &scheduler.run_history,
        exported_at: Utc::now().to_rfc3339(),
    };
    let count = pipeline.items.len() + triggers.event_log.len() + scheduler.run_history.len();
    let json = serde_json::to_string_pretty(&backup).unwrap_or_default();
    (json, count)
}

// ─────────────────────────────────────────────
//  Main Export Function
// ─────────────────────────────────────────────

pub fn run_export(
    pipeline: &BoardStore,
    triggers: &TriggerStore,
    scheduler: &SchedulerStore,
    opts: &ExportOptions,
) -> Result<ExportResult, String> {
    let (content, row_count) = match (&opts.report_type, &opts.format) {
        (ReportType::Pipeline, ExportFormat::Csv) => pipeline_csv(pipeline, opts),
        (ReportType::Pipeline, ExportFormat::Json) => pipeline_json(pipeline, opts),
        (ReportType::EventLog, ExportFormat::Csv) => event_log_csv(triggers, opts),
        (ReportType::EventLog, ExportFormat::Json) => event_log_json(triggers, opts),
        (ReportType::WorkflowRuns, ExportFormat::Csv) => workflow_runs_csv(scheduler, opts),
        (ReportType::WorkflowRuns, ExportFormat::Json) => workflow_runs_json(scheduler, opts),
        (ReportType::FullBackup, ExportFormat::Json) => full_backup_json(pipeline, triggers, scheduler),
        (ReportType::FullBackup, ExportFormat::Csv) => {
            return Err("Full backup is only available in JSON format".into());
        }
    };

    let filename = generate_filename(&opts.report_type, &opts.format);
    let dir = reports_dir();
    let path = dir.join(&filename);

    crate::storage::atomic_write(&path, &content)
        .map_err(|e| format!("Failed to write report: {}", e))?;

    let size_bytes = content.len() as u64;

    Ok(ExportResult {
        path: path.to_string_lossy().to_string(),
        filename,
        size_bytes,
        row_count,
    })
}

pub fn list_reports() -> Vec<ExportResult> {
    let dir = reports_dir();
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("report-") {
                    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                    results.push(ExportResult {
                        path: path.to_string_lossy().to_string(),
                        filename: name.to_string(),
                        size_bytes: size,
                        row_count: 0,
                    });
                }
            }
        }
    }
    results.sort_by(|a, b| b.filename.cmp(&a.filename));
    results
}

pub fn delete_report(filename: &str) -> Result<(), String> {
    let path = reports_dir().join(filename);
    std::fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete report: {}", e))
}

// ─────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────

fn escape_csv(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Structured Log Entry
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub module: String,
    pub message: String,
    pub fields: HashMap<String, serde_json::Value>,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, PartialOrd)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "trace" => Self::Trace,
            "debug" => Self::Debug,
            "warn" | "warning" => Self::Warn,
            "error" => Self::Error,
            _ => Self::Info,
        }
    }
}

// ─────────────────────────────────────────────
//  Metric Types
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MetricSnapshot {
    pub name: String,
    pub metric_type: MetricType,
    pub value: f64,
    pub labels: HashMap<String, String>,
    pub timestamp: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetricType {
    Counter,
    Gauge,
    Histogram,
}

struct MetricEntry {
    metric_type: MetricType,
    value: f64,
    labels: HashMap<String, String>,
    // For histograms
    observations: Vec<f64>,
}

// ─────────────────────────────────────────────
//  Trace Context
// ─────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TraceContext {
    pub trace_id: String,
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub operation: String,
    pub start_time: String,
    pub end_time: Option<String>,
    pub duration_ms: Option<u64>,
    pub status: SpanStatus,
    pub attributes: HashMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SpanStatus {
    Ok,
    Error,
    InProgress,
}

// ─────────────────────────────────────────────
//  Observability Store
// ─────────────────────────────────────────────

pub struct ObservabilityStore {
    logs: RwLock<Vec<LogEntry>>,
    metrics: RwLock<HashMap<String, MetricEntry>>,
    traces: RwLock<Vec<TraceContext>>,
    log_level: RwLock<LogLevel>,
    max_logs: usize,
    max_traces: usize,
}

impl ObservabilityStore {
    pub fn new(max_logs: usize, max_traces: usize) -> Self {
        Self {
            logs: RwLock::new(Vec::new()),
            metrics: RwLock::new(HashMap::new()),
            traces: RwLock::new(Vec::new()),
            log_level: RwLock::new(LogLevel::Info),
            max_logs,
            max_traces,
        }
    }

    // ── Logging ─────────────────────────────

    pub fn set_log_level(&self, level: LogLevel) {
        *self.log_level.write().unwrap() = level;
    }

    pub fn log(&self, level: LogLevel, module: &str, message: &str, fields: HashMap<String, serde_json::Value>) {
        let min_level = self.log_level.read().unwrap().clone();
        if level < min_level {
            return;
        }

        let entry = LogEntry {
            timestamp: Utc::now().to_rfc3339(),
            level,
            module: module.to_string(),
            message: message.to_string(),
            fields,
            trace_id: None,
            span_id: None,
        };

        let mut logs = self.logs.write().unwrap();
        logs.push(entry);
        let max = self.max_logs;
        if logs.len() > max {
            let excess = logs.len() - max;
            logs.drain(0..excess);
        }
    }

    pub fn info(&self, module: &str, message: &str) {
        self.log(LogLevel::Info, module, message, HashMap::new());
    }

    pub fn warn(&self, module: &str, message: &str) {
        self.log(LogLevel::Warn, module, message, HashMap::new());
    }

    pub fn error(&self, module: &str, message: &str) {
        self.log(LogLevel::Error, module, message, HashMap::new());
    }

    pub fn recent_logs(&self, limit: usize, level_filter: Option<&LogLevel>) -> Vec<LogEntry> {
        let logs = self.logs.read().unwrap();
        logs.iter().rev()
            .filter(|l| level_filter.map_or(true, |f| &l.level >= f))
            .take(limit)
            .cloned()
            .collect()
    }

    // ── Metrics ─────────────────────────────

    pub fn increment_counter(&self, name: &str, labels: HashMap<String, String>) {
        let mut metrics = self.metrics.write().unwrap();
        let entry = metrics.entry(name.to_string()).or_insert(MetricEntry {
            metric_type: MetricType::Counter,
            value: 0.0,
            labels,
            observations: Vec::new(),
        });
        entry.value += 1.0;
    }

    pub fn counter_add(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        let mut metrics = self.metrics.write().unwrap();
        let entry = metrics.entry(name.to_string()).or_insert(MetricEntry {
            metric_type: MetricType::Counter,
            value: 0.0,
            labels,
            observations: Vec::new(),
        });
        entry.value += value;
    }

    pub fn set_gauge(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        let mut metrics = self.metrics.write().unwrap();
        metrics.insert(name.to_string(), MetricEntry {
            metric_type: MetricType::Gauge,
            value,
            labels,
            observations: Vec::new(),
        });
    }

    pub fn observe_histogram(&self, name: &str, value: f64, labels: HashMap<String, String>) {
        let mut metrics = self.metrics.write().unwrap();
        let entry = metrics.entry(name.to_string()).or_insert(MetricEntry {
            metric_type: MetricType::Histogram,
            value: 0.0,
            labels,
            observations: Vec::new(),
        });
        entry.observations.push(value);
        entry.value = entry.observations.len() as f64;
    }

    pub fn snapshot_metrics(&self) -> Vec<MetricSnapshot> {
        let metrics = self.metrics.read().unwrap();
        let now = Utc::now().to_rfc3339();
        metrics.iter().map(|(name, entry)| MetricSnapshot {
            name: name.clone(),
            metric_type: entry.metric_type.clone(),
            value: entry.value,
            labels: entry.labels.clone(),
            timestamp: now.clone(),
        }).collect()
    }

    /// Export metrics in Prometheus text format.
    pub fn prometheus_export(&self) -> String {
        let metrics = self.metrics.read().unwrap();
        let mut output = String::new();
        for (name, entry) in metrics.iter() {
            let safe_name = name.replace('.', "_").replace('-', "_");
            let type_str = match entry.metric_type {
                MetricType::Counter => "counter",
                MetricType::Gauge => "gauge",
                MetricType::Histogram => "histogram",
            };
            output.push_str(&format!("# TYPE {} {}\n", safe_name, type_str));
            if entry.labels.is_empty() {
                output.push_str(&format!("{} {}\n", safe_name, entry.value));
            } else {
                let labels: Vec<String> = entry.labels.iter()
                    .map(|(k, v)| format!("{}=\"{}\"", k, v))
                    .collect();
                output.push_str(&format!("{}{{{}}} {}\n", safe_name, labels.join(","), entry.value));
            }
        }
        output
    }

    // ── Tracing ─────────────────────────────

    pub fn start_span(&self, operation: &str, parent_span_id: Option<String>) -> TraceContext {
        let ctx = TraceContext {
            trace_id: uuid::Uuid::new_v4().to_string(),
            span_id: uuid::Uuid::new_v4().to_string(),
            parent_span_id,
            operation: operation.to_string(),
            start_time: Utc::now().to_rfc3339(),
            end_time: None,
            duration_ms: None,
            status: SpanStatus::InProgress,
            attributes: HashMap::new(),
        };
        let mut traces = self.traces.write().unwrap();
        traces.push(ctx.clone());
        let max = self.max_traces;
        if traces.len() > max {
            let excess = traces.len() - max;
            traces.drain(0..excess);
        }
        ctx
    }

    pub fn end_span(&self, span_id: &str, status: SpanStatus) {
        let mut traces = self.traces.write().unwrap();
        if let Some(span) = traces.iter_mut().find(|t| t.span_id == span_id) {
            let now = Utc::now();
            span.end_time = Some(now.to_rfc3339());
            span.status = status;
            if let Ok(start) = span.start_time.parse::<chrono::DateTime<Utc>>() {
                span.duration_ms = Some((now - start).num_milliseconds().max(0) as u64);
            }
        }
    }

    pub fn recent_traces(&self, limit: usize) -> Vec<TraceContext> {
        let traces = self.traces.read().unwrap();
        traces.iter().rev().take(limit).cloned().collect()
    }
}

impl Default for ObservabilityStore {
    fn default() -> Self {
        Self::new(5000, 1000)
    }
}

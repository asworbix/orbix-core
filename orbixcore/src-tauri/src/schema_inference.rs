//! Dynamic Data Analysis Engine (Epic 2)
//!
//! Replaces hardcoded shape detection with auto schema inference.
//! Analyzes any JSON data, detects field types, money fields, dates,
//! enums, and renders dynamic markdown tables and insights.

use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

// ─────────────────────────────────────────────
//  Field type inference
// ─────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum FieldType {
    Money,
    Number,
    Percentage,
    Date,
    Id,
    Enum(Vec<String>),
    Text,
    Boolean,
    Nested,
    Array,
    Null,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub field_type: FieldType,
    pub non_null_count: usize,
    pub sample_count: usize,
    /// Numeric stats (for Money/Number fields)
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    /// Unique string values (for enum detection)
    pub unique_values: HashSet<String>,
    /// Is this a likely "label" field (name, title, subject, etc.)
    pub is_label: bool,
    /// Is this a likely "status" field
    pub is_status: bool,
}

impl FieldInfo {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            field_type: FieldType::Null,
            non_null_count: 0,
            sample_count: 0,
            sum: 0.0,
            min: f64::MAX,
            max: f64::MIN,
            unique_values: HashSet::new(),
            is_label: is_label_field(name),
            is_status: is_status_field(name),
        }
    }

    fn observe(&mut self, val: &Value) {
        self.sample_count += 1;
        match val {
            Value::Null => {}
            Value::Bool(_) => {
                self.non_null_count += 1;
                self.field_type = FieldType::Boolean;
            }
            Value::Number(n) => {
                self.non_null_count += 1;
                let f = n.as_f64().unwrap_or(0.0);
                self.sum += f;
                if f < self.min { self.min = f; }
                if f > self.max { self.max = f; }
                // Type inference: Money vs Percentage vs Number
                if self.field_type == FieldType::Null || self.field_type == FieldType::Number
                    || self.field_type == FieldType::Money || self.field_type == FieldType::Percentage
                {
                    self.field_type = infer_numeric_type(&self.name, f, &self.field_type);
                }
            }
            Value::String(s) => {
                self.non_null_count += 1;
                if self.unique_values.len() < 50 {
                    self.unique_values.insert(s.clone());
                }
                let inferred = infer_string_type(&self.name, s);
                // Promote type: keep the most specific
                match (&self.field_type, &inferred) {
                    (FieldType::Null, _) => self.field_type = inferred,
                    (FieldType::Date, FieldType::Date) => {}
                    (FieldType::Id, FieldType::Id) => {}
                    (FieldType::Text, _) => {} // Text is the fallback
                    _ => {
                        if inferred == FieldType::Text {
                            self.field_type = FieldType::Text;
                        }
                    }
                }
            }
            Value::Object(_) => {
                self.non_null_count += 1;
                self.field_type = FieldType::Nested;
            }
            Value::Array(_) => {
                self.non_null_count += 1;
                self.field_type = FieldType::Array;
            }
        }
    }

    /// Finalize: convert Text with few unique values to Enum
    fn finalize(&mut self) {
        if let FieldType::Text = &self.field_type {
            if self.unique_values.len() <= 10 && self.non_null_count > 2
                && self.unique_values.len() < self.non_null_count
            {
                let mut vals: Vec<String> = self.unique_values.iter().cloned().collect();
                vals.sort();
                self.field_type = FieldType::Enum(vals);
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Schema
// ─────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct InferredSchema {
    pub fields: Vec<FieldInfo>,
    pub row_count: usize,
    /// Best field to use as label/title column
    pub label_field: Option<String>,
    /// Detected category/domain name (e.g. "Invoices", "Projects")
    pub domain_hint: Option<String>,
}

/// Infer schema from an array of JSON objects.
pub fn infer_schema(data: &[Value]) -> InferredSchema {
    let mut field_map: HashMap<String, FieldInfo> = HashMap::new();
    let mut field_order: Vec<String> = Vec::new();

    let sample = if data.len() > 100 { &data[..100] } else { data };

    for val in sample {
        if let Value::Object(map) = val {
            for (key, v) in map {
                if !field_map.contains_key(key) {
                    field_order.push(key.clone());
                    field_map.insert(key.clone(), FieldInfo::new(key));
                }
                field_map.get_mut(key).unwrap().observe(v);
            }
        }
    }

    // Finalize all fields (enum detection, etc.)
    for fi in field_map.values_mut() {
        fi.finalize();
    }

    // Build ordered field list
    let fields: Vec<FieldInfo> = field_order.iter()
        .filter_map(|k| field_map.remove(k))
        .collect();

    // Detect label field
    let label_field = fields.iter()
        .find(|f| f.is_label)
        .map(|f| f.name.clone());

    // Detect domain hint from field names
    let domain_hint = detect_domain(&fields);

    InferredSchema {
        fields,
        row_count: data.len(),
        label_field,
        domain_hint,
    }
}

// ─────────────────────────────────────────────
//  Dynamic rendering
// ─────────────────────────────────────────────

/// Analyze data dynamically and produce markdown report.
pub fn analyze_dynamic(data: &[Value], _prompt: &str) -> String {
    let mut sections: Vec<String> = Vec::new();
    let mut flags: Vec<String> = Vec::new();

    for val in data {
        match val {
            Value::Object(map) => {
                // Check if this is a nested dashboard (multiple domain objects)
                let nested_domains: Vec<&String> = map.keys()
                    .filter(|k| map.get(*k).map(|v| v.is_object()).unwrap_or(false))
                    .collect();

                if nested_domains.len() >= 2 {
                    // Multi-domain dashboard: render each sub-object as a section
                    for key in map.keys() {
                        if let Some(Value::Object(sub)) = map.get(key) {
                            let label = humanize_key(key);
                            let sub_section = render_object_as_table(&label, sub, &mut flags);
                            sections.push(sub_section);
                        }
                    }
                } else if let Some(arr) = extract_array(map) {
                    // Object wrapping an array
                    let schema = infer_schema(arr);
                    sections.push(render_table(arr, &schema, &mut flags));
                } else {
                    // Single flat object
                    let label = detect_object_domain(map).unwrap_or_else(|| "Summary".to_string());
                    sections.push(render_object_as_table(&label, map, &mut flags));
                }
            }
            Value::Array(arr) if !arr.is_empty() => {
                let schema = infer_schema(arr);
                sections.push(render_table(arr, &schema, &mut flags));
            }
            _ => {
                sections.push(format_value_generic(val, 0));
            }
        }
    }

    if sections.is_empty() {
        return data.iter()
            .map(|v| format_value_generic(v, 0))
            .collect::<Vec<_>>()
            .join("\n\n");
    }

    let mut report = sections.join("\n\n");
    if !flags.is_empty() {
        report.push_str("\n\n---\n\n**Flags & Recommendations**\n");
        for flag in &flags {
            report.push_str(&format!("\n- {}", flag));
        }
    }
    report
}

/// Render an array of objects as a markdown table using inferred schema.
fn render_table(data: &[Value], schema: &InferredSchema, flags: &mut Vec<String>) -> String {
    // Pick columns: label first, then status, then money/number, then others
    // Skip: Id, Nested, Array fields. Max 6 columns.
    let columns = pick_columns(schema);
    if columns.is_empty() {
        return format_value_generic(&Value::Array(data.to_vec()), 0);
    }

    let domain = schema.domain_hint.as_deref().unwrap_or("Data");

    // Summary stats
    let mut summary_parts: Vec<String> = vec![format!("{} items", schema.row_count)];
    for col in &columns {
        if matches!(col.field_type, FieldType::Money) && col.sum != 0.0 {
            summary_parts.push(format!("{}: {}", humanize_key(&col.name), fmt_money(col.sum)));
        }
    }

    // Header
    let header: Vec<String> = columns.iter().map(|f| humanize_key(&f.name)).collect();
    let sep: Vec<&str> = columns.iter().map(|_| "---").collect();

    // Rows
    let display_limit = if schema.row_count > 20 { 15 } else { schema.row_count };
    let mut rows: Vec<String> = Vec::new();
    for item in data.iter().take(display_limit) {
        if let Value::Object(map) = item {
            let cells: Vec<String> = columns.iter().map(|col| {
                format_cell(map.get(&col.name).unwrap_or(&Value::Null), &col.field_type, &col.name)
            }).collect();
            rows.push(format!("| {} |", cells.join(" | ")));
        }
    }

    // Generate flags from data patterns
    generate_flags(data, schema, &columns, flags);

    let mut output = format!("**{}** ({})\n\n| {} |\n| {} |\n{}",
        domain, summary_parts.join(", "),
        header.join(" | "), sep.join(" | "), rows.join("\n"));

    if schema.row_count > display_limit {
        output.push_str(&format!("\n\n*...and {} more*", schema.row_count - display_limit));
    }

    // Totals row for money columns
    let has_money = columns.iter().any(|c| matches!(c.field_type, FieldType::Money));
    if has_money && schema.row_count > 1 {
        let total_cells: Vec<String> = columns.iter().map(|col| {
            if matches!(col.field_type, FieldType::Money) {
                format!("**{}**", fmt_money(col.sum))
            } else if col.is_label {
                "**Total**".to_string()
            } else {
                String::new()
            }
        }).collect();
        output.push_str(&format!("\n| {} |", total_cells.join(" | ")));
    }

    output
}

/// Render a flat object as a key-value table.
fn render_object_as_table(label: &str, map: &Map<String, Value>, flags: &mut Vec<String>) -> String {
    let mut lines = Vec::new();
    let mut money_fields: Vec<(String, f64)> = Vec::new();

    for (key, val) in map {
        let display_key = humanize_key(key);
        match val {
            Value::Object(sub) => {
                lines.push(format!("| **{}** | |", display_key));
                for (sk, sv) in sub {
                    let sk_display = humanize_key(sk);
                    let sv_display = format_scalar(sv, sk);
                    lines.push(format!("| {} | {} |", sk_display, sv_display));
                }
            }
            Value::Array(arr) if arr.iter().all(|v| v.is_object()) && !arr.is_empty() => {
                // Nested array of objects: render as sub-table
                let schema = infer_schema(arr);
                let sub_table = render_table(arr, &schema, flags);
                lines.push(format!("| **{}** | ({} items) |", display_key, arr.len()));
                // We'll append the sub-table after this table
                lines.push(format!("\n{}", sub_table));
            }
            Value::Number(n) if is_money_field(key) => {
                let f = n.as_f64().unwrap_or(0.0);
                money_fields.push((key.clone(), f));
                lines.push(format!("| {} | **{}** |", display_key, fmt_money(f)));
            }
            _ => {
                lines.push(format!("| {} | {} |", display_key, format_scalar(val, key)));
            }
        }
    }

    // Auto-flag: if there's an "overdue" or "critical" count > 0
    for (k, val) in map {
        let key_lower = k.to_lowercase();
        if let Some(n) = val.as_i64() {
            if n > 0 && (key_lower.contains("overdue") || key_lower.contains("critical") || key_lower.contains("low_stock")) {
                flags.push(format!("{} {} - needs attention", n, humanize_key(k).to_lowercase()));
            }
        }
    }

    // Auto-flag: budget utilization
    if let (Some(budget), Some(spent)) = (
        map.get("totalBudget").or(map.get("total_budget")).or(map.get("budget")).and_then(|v| v.as_f64()),
        map.get("totalSpent").or(map.get("total_spent")).or(map.get("spent")).and_then(|v| v.as_f64()),
    ) {
        if budget > 0.0 {
            let util = spent / budget * 100.0;
            if util > 80.0 {
                flags.push(format!("Budget utilization at {:.0}% - approaching limit", util));
            }
        }
    }

    // Auto-flag: outstanding > revenue
    for (key1, val1) in &money_fields {
        for (key2, val2) in &money_fields {
            let k1 = key1.to_lowercase();
            let k2 = key2.to_lowercase();
            if k1.contains("outstanding") && k2.contains("revenue") && *val1 > *val2 && *val2 > 0.0 {
                flags.push(format!("Outstanding ({}) exceeds revenue ({}) - prioritize collections", fmt_money(*val1), fmt_money(*val2)));
            }
        }
    }

    format!("**{}**\n| Metric | Value |\n|---|---|\n{}", label, lines.join("\n"))
}

// ─────────────────────────────────────────────
//  Column selection
// ─────────────────────────────────────────────

fn pick_columns(schema: &InferredSchema) -> Vec<FieldInfo> {
    let mut cols: Vec<FieldInfo> = Vec::new();
    let mut remaining: Vec<&FieldInfo> = Vec::new();

    // Priority 1: label field
    if let Some(label) = schema.fields.iter().find(|f| f.is_label) {
        cols.push(label.clone());
    }

    // Priority 2: status/enum fields
    for f in &schema.fields {
        if f.is_label { continue; }
        if f.is_status || matches!(f.field_type, FieldType::Enum(_)) {
            if cols.len() < 6 { cols.push(f.clone()); }
        } else {
            remaining.push(f);
        }
    }

    // Priority 3: money fields
    for f in &remaining {
        if matches!(f.field_type, FieldType::Money) && cols.len() < 6 {
            cols.push((*f).clone());
        }
    }

    // Priority 4: number, percentage, date, text (up to 6 total)
    for f in &remaining {
        if cols.len() >= 6 { break; }
        if matches!(f.field_type, FieldType::Money) { continue; } // already added
        if matches!(f.field_type, FieldType::Id | FieldType::Nested | FieldType::Array | FieldType::Null) { continue; }
        cols.push((*f).clone());
    }

    cols
}

// ─────────────────────────────────────────────
//  Formatting helpers
// ─────────────────────────────────────────────

fn format_cell(val: &Value, field_type: &FieldType, _key: &str) -> String {
    match val {
        Value::Null => "-".to_string(),
        Value::Bool(b) => if *b { "Yes" } else { "No" }.to_string(),
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            match field_type {
                FieldType::Money => fmt_money(f),
                FieldType::Percentage => format!("{:.1}%", f),
                _ => fmt_number(f),
            }
        }
        Value::String(s) => {
            if s.len() > 40 { format!("{}...", &s[..40]) } else { s.clone() }
        }
        _ => "...".to_string(),
    }
}

fn format_scalar(val: &Value, key: &str) -> String {
    match val {
        Value::Null => "-".to_string(),
        Value::Bool(b) => if *b { "Yes" } else { "No" }.to_string(),
        Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if is_money_field(key) { fmt_money(f) }
            else if key.to_lowercase().contains("percent") || key.to_lowercase().contains("rate") || key.to_lowercase().contains("utilization") {
                format!("{:.1}%", f)
            } else { fmt_number(f) }
        }
        Value::String(s) => {
            if s.len() > 60 { format!("{}...", &s[..60]) } else { s.clone() }
        }
        Value::Array(arr) => format!("{} items", arr.len()),
        Value::Object(_) => "(object)".to_string(),
    }
}

fn format_value_generic(val: &Value, depth: usize) -> String {
    match val {
        Value::Object(map) => {
            let mut lines = Vec::new();
            for (key, v) in map {
                let label = humanize_key(key);
                match v {
                    Value::Object(_) => {
                        lines.push(format!("{}**{}**", if depth > 0 { "  " } else { "\n" }, label));
                        lines.push(format_value_generic(v, depth + 1));
                    }
                    Value::Array(arr) if arr.is_empty() => {
                        lines.push(format!("- {}: *none*", label));
                    }
                    Value::Array(arr) if arr.len() <= 5 && arr.iter().all(|v| v.is_object()) => {
                        lines.push(format!("**{}** ({})", label, arr.len()));
                        for item in arr {
                            if let Value::Object(obj) = item {
                                let summary: Vec<String> = obj.iter().take(4)
                                    .filter_map(|(k, v)| {
                                        let val_str = match v {
                                            Value::String(s) => if s.len() > 40 { format!("{}...", &s[..40]) } else { s.clone() },
                                            Value::Number(n) => fmt_number(n.as_f64().unwrap_or(0.0)),
                                            Value::Bool(b) => b.to_string(),
                                            _ => return None,
                                        };
                                        Some(format!("{}: {}", humanize_key(k), val_str))
                                    }).collect();
                                lines.push(format!("  - {}", summary.join(" · ")));
                            }
                        }
                    }
                    Value::Array(arr) => {
                        lines.push(format!("- {}: {} items", label, arr.len()));
                    }
                    Value::Number(n) => {
                        let f = n.as_f64().unwrap_or(0.0);
                        if is_money_field(key) {
                            lines.push(format!("- {}: **{}**", label, fmt_money(f)));
                        } else {
                            lines.push(format!("- {}: **{}**", label, fmt_number(f)));
                        }
                    }
                    Value::String(s) => {
                        let display = if s.len() > 80 { format!("{}...", &s[..80]) } else { s.clone() };
                        lines.push(format!("- {}: {}", label, display));
                    }
                    Value::Bool(b) => {
                        lines.push(format!("- {}: {}", label, if *b { "Yes" } else { "No" }));
                    }
                    Value::Null => {
                        lines.push(format!("- {}: -", label));
                    }
                }
            }
            lines.join("\n")
        }
        Value::Array(arr) if arr.is_empty() => "*No data*".to_string(),
        Value::Array(arr) => {
            arr.iter().take(10)
                .map(|v| format!("- {}", format_value_generic(v, depth + 1)))
                .collect::<Vec<_>>().join("\n")
        }
        Value::String(s) => s.clone(),
        Value::Number(n) => fmt_number(n.as_f64().unwrap_or(0.0)),
        Value::Bool(b) => b.to_string(),
        Value::Null => "-".to_string(),
    }
}

pub fn format_json_as_markdown(raw: &str) -> String {
    match serde_json::from_str::<Value>(raw) {
        Ok(val) => format_value_generic(&val, 0),
        Err(_) => {
            let trimmed = raw.trim();
            if trimmed.len() > 2000 {
                format!("{} ...", &trimmed[..2000])
            } else {
                trimmed.to_string()
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Auto-flag generation
// ─────────────────────────────────────────────

fn generate_flags(data: &[Value], schema: &InferredSchema, columns: &[FieldInfo], flags: &mut Vec<String>) {
    // Find status/priority enum fields and count values
    for col in columns {
        let key = &col.name;
        let key_lower = key.to_lowercase();

        if key_lower.contains("priority") || key_lower.contains("severity") {
            let critical_count = data.iter().filter(|v| {
                v.get(key).and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase() == "critical")
                    .unwrap_or(false)
            }).count();
            if critical_count > 0 {
                flags.push(format!("{} critical items require immediate attention", critical_count));
            }
        }

        if key_lower.contains("status") {
            let overdue_count = data.iter().filter(|v| {
                v.get(key).and_then(|v| v.as_str())
                    .map(|s| s.to_lowercase() == "overdue")
                    .unwrap_or(false)
            }).count();
            if overdue_count > 0 {
                flags.push(format!("{} overdue items need attention", overdue_count));
            }
        }
    }

    // Budget utilization across items
    let budget_field = schema.fields.iter().find(|f| f.name.to_lowercase().contains("budget") && matches!(f.field_type, FieldType::Money | FieldType::Number));
    let spent_field = schema.fields.iter().find(|f| f.name.to_lowercase().contains("spent") && matches!(f.field_type, FieldType::Money | FieldType::Number));

    if let (Some(bf), Some(sf)) = (budget_field, spent_field) {
        if bf.sum > 0.0 {
            let util = sf.sum / bf.sum * 100.0;
            if util > 80.0 {
                flags.push(format!("Overall budget utilization at {:.0}% - approaching limit", util));
            }
        }
    }

    // Low satisfaction
    for col in &schema.fields {
        let name_lower = col.name.to_lowercase();
        if (name_lower.contains("satisfaction") || name_lower.contains("rating")) && matches!(col.field_type, FieldType::Number | FieldType::Percentage) {
            if col.non_null_count > 0 {
                let avg = col.sum / col.non_null_count as f64;
                if avg < 4.0 && col.max <= 5.0 {
                    flags.push(format!("{} average ({:.1}) is below target - review quality", humanize_key(&col.name), avg));
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Heuristics
// ─────────────────────────────────────────────

fn is_money_field(key: &str) -> bool {
    let k = key.to_lowercase();
    const MONEY_WORDS: &[&str] = &[
        "value", "revenue", "income", "expense", "budget", "spent",
        "total", "amount", "price", "cost", "outstanding", "subtotal",
        "tax", "balance", "payment", "salary", "wage", "fee",
        "profit", "loss", "margin", "deal_value", "invoice",
    ];
    MONEY_WORDS.iter().any(|w| k.contains(w))
}

fn is_label_field(key: &str) -> bool {
    let k = key.to_lowercase();
    const LABEL_WORDS: &[&str] = &[
        "name", "title", "subject", "label", "description", "account_name",
    ];
    LABEL_WORDS.iter().any(|w| k == *w || k.ends_with(&format!("_{}", w)))
}

fn is_status_field(key: &str) -> bool {
    let k = key.to_lowercase();
    k == "status" || k == "state" || k == "priority" || k == "severity" || k == "stage"
}

fn is_id_field(key: &str) -> bool {
    let k = key.to_lowercase();
    k == "id" || k.ends_with("_id") || k == "uuid" || k == "key"
}

fn is_date_string(s: &str) -> bool {
    // ISO 8601 or common date patterns
    if s.len() >= 10 && s.len() <= 30 {
        // yyyy-mm-dd or yyyy-mm-ddThh:mm:ss
        if s.chars().nth(4) == Some('-') && s.chars().nth(7) == Some('-') {
            return true;
        }
    }
    false
}

fn infer_numeric_type(key: &str, _val: f64, _current: &FieldType) -> FieldType {
    if is_money_field(key) {
        FieldType::Money
    } else {
        let k = key.to_lowercase();
        if k.contains("percent") || k.contains("rate") || k.ends_with("_pct") || k.contains("utilization") {
            FieldType::Percentage
        } else {
            FieldType::Number
        }
    }
}

fn infer_string_type(key: &str, val: &str) -> FieldType {
    if is_id_field(key) {
        return FieldType::Id;
    }
    if is_date_string(val) || key.to_lowercase().contains("date") || key.to_lowercase().contains("_at") || key.to_lowercase() == "created" || key.to_lowercase() == "updated" {
        return FieldType::Date;
    }
    FieldType::Text
}

fn detect_domain(fields: &[FieldInfo]) -> Option<String> {
    let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();

    if names.contains(&"invoice_number") || names.contains(&"invoiceNumber") { return Some("Invoices".into()); }
    if names.contains(&"deal_value") || names.contains(&"dealValue") { return Some("Deals".into()); }
    if names.contains(&"subject") && names.contains(&"priority") { return Some("Tickets".into()); }
    if names.contains(&"budget") && names.contains(&"spent") { return Some("Projects".into()); }
    if names.contains(&"account_name") { return Some("Accounts".into()); }
    if names.contains(&"industry") && names.contains(&"name") { return Some("Companies".into()); }
    if names.contains(&"expense_date") { return Some("Expenses".into()); }
    if names.contains(&"response_time_hours") { return Some("SLA Policies".into()); }
    if names.contains(&"due_date") && names.contains(&"title") { return Some("Items".into()); }
    if names.contains(&"category") && names.contains(&"title") { return Some("Articles".into()); }

    // Generic: use first label field's parent
    None
}

fn detect_object_domain(map: &Map<String, Value>) -> Option<String> {
    let keys: Vec<&String> = map.keys().collect();
    // Check for known sub-domains
    if keys.iter().any(|k| k.to_lowercase().contains("revenue")) { return Some("Financial Overview".into()); }
    if keys.iter().any(|k| k.to_lowercase().contains("pipeline")) { return Some("Pipeline".into()); }
    if keys.iter().any(|k| k.to_lowercase().contains("ticket")) { return Some("Support".into()); }
    if keys.iter().any(|k| k.to_lowercase().contains("project")) { return Some("Projects".into()); }
    None
}

fn extract_array<'a>(map: &'a Map<String, Value>) -> Option<&'a Vec<Value>> {
    // Look for the first array field (prefer "data", "items", "results", etc.)
    for key in &["data", "items", "results", "records", "list", "entries"] {
        if let Some(Value::Array(arr)) = map.get(*key) {
            if !arr.is_empty() { return Some(arr); }
        }
    }
    // Fallback: any array field
    for (_key, val) in map {
        if let Value::Array(arr) = val {
            if !arr.is_empty() && arr.iter().any(|v| v.is_object()) {
                return Some(arr);
            }
        }
    }
    None
}

fn humanize_key(key: &str) -> String {
    let label = key.replace('_', " ");
    let mut chars = label.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().to_string() + chars.as_str(),
    }
}

pub fn fmt_money(v: f64) -> String {
    if v.abs() >= 1_000_000.0 { format!("${:.2}M", v / 1_000_000.0) }
    else if v.abs() >= 1_000.0 { format!("${:.0}K", v / 1_000.0) }
    else { format!("${:.0}", v) }
}

fn fmt_number(v: f64) -> String {
    if v == v.floor() {
        let i = v as i64;
        if i.abs() >= 1_000_000 { format!("{:.1}M", v / 1_000_000.0) }
        else if i.abs() >= 10_000 { format!("{:.0}K", v / 1_000.0) }
        else { format!("{}", i) }
    } else {
        format!("{:.2}", v)
    }
}

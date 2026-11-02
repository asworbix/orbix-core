use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{Utc, DateTime, Datelike, Timelike, Weekday, NaiveDate, NaiveTime, TimeZone};

// ─────────────────────────────────────────────
//  Cron Expression
// ─────────────────────────────────────────────

/// Simplified cron: minute hour day_of_month month day_of_week
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct CronExpr {
    pub raw: String,
    pub minutes: Vec<u32>,
    pub hours: Vec<u32>,
    pub days_of_month: Vec<u32>,   // 1–31
    pub months: Vec<u32>,           // 1–12
    pub days_of_week: Vec<u32>,     // 0=Sun … 6=Sat
}

impl CronExpr {
    pub fn parse(expr: &str) -> Result<Self, String> {
        let parts: Vec<&str> = expr.trim().split_whitespace().collect();
        if parts.len() != 5 {
            return Err("Cron expression must have 5 fields: min hour dom month dow".into());
        }
        Ok(Self {
            raw: expr.to_string(),
            minutes: parse_field(parts[0], 0, 59)?,
            hours: parse_field(parts[1], 0, 23)?,
            days_of_month: parse_field(parts[2], 1, 31)?,
            months: parse_field(parts[3], 1, 12)?,
            days_of_week: parse_dow_field(parts[4])?,
        })
    }

    /// Compute the next fire time after `after`. The cron fields are read
    /// in the user's local time ("daily at 9" means 09:00 on this
    /// computer's clock); the result is returned in UTC for the store.
    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        // Start one minute after, at second 0
        let naive = after.with_timezone(&chrono::Local).naive_local() + chrono::Duration::minutes(1);
        let mut year = naive.year();
        let mut month = naive.month();
        let mut day = naive.day();
        let mut hour = naive.hour();
        let mut minute = naive.minute();

        let max_iter = 525_960; // ~1 year of minutes
        for _ in 0..max_iter {
            if self.months.contains(&month)
                && self.days_of_month.contains(&day)
                && self.hours.contains(&hour)
                && self.minutes.contains(&minute)
            {
                // Check day of week
                if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                    let dow = match date.weekday() {
                        Weekday::Sun => 0, Weekday::Mon => 1, Weekday::Tue => 2,
                        Weekday::Wed => 3, Weekday::Thu => 4, Weekday::Fri => 5,
                        Weekday::Sat => 6,
                    };
                    if self.days_of_week.contains(&dow) {
                        let time = NaiveTime::from_hms_opt(hour, minute, 0)?;
                        let dt = NaiveDate::from_ymd_opt(year, month, day)?.and_time(time);
                        // A DST gap has no such local time: take the first valid instant after it.
                        let local = chrono::Local
                            .from_local_datetime(&dt)
                            .earliest()
                            .or_else(|| chrono::Local.from_local_datetime(&(dt + chrono::Duration::hours(1))).earliest());
                        if let Some(l) = local {
                            return Some(l.with_timezone(&Utc));
                        }
                    }
                }
            }

            // Advance one tick
            if !self.months.contains(&month) {
                // Skip to next month
                if month == 12 { year += 1; month = 1; } else { month += 1; }
                day = 1; hour = 0; minute = 0;
                continue;
            }
            if !self.days_of_month.contains(&day) || NaiveDate::from_ymd_opt(year, month, day).is_none() {
                // Skip to next day
                day += 1;
                if day > 31 {
                    if month == 12 { year += 1; month = 1; } else { month += 1; }
                    day = 1;
                }
                hour = 0; minute = 0;
                continue;
            }
            if !self.hours.contains(&hour) {
                hour += 1;
                if hour > 23 {
                    day += 1; hour = 0;
                    if day > 31 {
                        if month == 12 { year += 1; month = 1; } else { month += 1; }
                        day = 1;
                    }
                }
                minute = 0;
                continue;
            }
            // Advance minute
            minute += 1;
            if minute > 59 {
                minute = 0; hour += 1;
                if hour > 23 {
                    hour = 0; day += 1;
                    if day > 31 {
                        if month == 12 { year += 1; month = 1; } else { month += 1; }
                        day = 1;
                    }
                }
            }
        }
        None
    }
}

/// Parse a single cron field like "0", "*/5", "1-5", "1,3,5", or "*"
fn parse_field(field: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let mut vals = Vec::new();

    for part in field.split(',') {
        if part == "*" {
            return Ok((min..=max).collect());
        }
        if let Some(step_str) = part.strip_prefix("*/") {
            let step: u32 = step_str.parse().map_err(|_| format!("Invalid step: {}", part))?;
            if step == 0 { return Err("Step cannot be zero".into()); }
            let mut v = min;
            while v <= max {
                vals.push(v);
                v += step;
            }
        } else if part.contains('-') {
            let range: Vec<&str> = part.split('-').collect();
            if range.len() != 2 { return Err(format!("Invalid range: {}", part)); }
            let start: u32 = range[0].parse().map_err(|_| format!("Invalid: {}", part))?;
            let end: u32 = range[1].parse().map_err(|_| format!("Invalid: {}", part))?;
            if start > end || start < min || end > max {
                return Err(format!("Range out of bounds: {}", part));
            }
            vals.extend(start..=end);
        } else {
            let v: u32 = part.parse().map_err(|_| format!("Invalid value: {}", part))?;
            if v < min || v > max {
                return Err(format!("Value {} out of range {}-{}", v, min, max));
            }
            vals.push(v);
        }
    }

    vals.sort();
    vals.dedup();
    if vals.is_empty() {
        return Err("Empty cron field".into());
    }
    Ok(vals)
}

/// Parse day-of-week field with named days support (MON-FRI, etc.)
fn parse_dow_field(field: &str) -> Result<Vec<u32>, String> {
    let normalized = field
        .replace("SUN", "0").replace("MON", "1").replace("TUE", "2")
        .replace("WED", "3").replace("THU", "4").replace("FRI", "5")
        .replace("SAT", "6")
        .replace("Sun", "0").replace("Mon", "1").replace("Tue", "2")
        .replace("Wed", "3").replace("Thu", "4").replace("Fri", "5")
        .replace("Sat", "6");
    parse_field(&normalized, 0, 6)
}

// ─────────────────────────────────────────────
//  Schedule Config
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SchedulePreset {
    EveryMinute,
    Every5Minutes,
    Every15Minutes,
    EveryHour,
    Daily,
    WeekdayMornings,
    Weekly,
    Monthly,
    Custom,
}

impl SchedulePreset {
    pub fn to_cron(&self) -> Option<&str> {
        match self {
            Self::EveryMinute => Some("* * * * *"),
            Self::Every5Minutes => Some("*/5 * * * *"),
            Self::Every15Minutes => Some("*/15 * * * *"),
            Self::EveryHour => Some("0 * * * *"),
            Self::Daily => Some("0 9 * * *"),
            Self::WeekdayMornings => Some("0 9 * * 1-5"),
            Self::Weekly => Some("0 9 * * 1"),
            Self::Monthly => Some("0 9 1 * *"),
            Self::Custom => None,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::EveryMinute => "Every minute",
            Self::Every5Minutes => "Every 5 minutes",
            Self::Every15Minutes => "Every 15 minutes",
            Self::EveryHour => "Every hour",
            Self::Daily => "Daily at 9 AM",
            Self::WeekdayMornings => "Weekdays at 9 AM",
            Self::Weekly => "Weekly (Mon 9 AM)",
            Self::Monthly => "Monthly (1st at 9 AM)",
            Self::Custom => "Custom cron",
        }
    }
}

// ─────────────────────────────────────────────
//  Schedule Entry
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ScheduleEntry {
    pub id: String,
    pub workflow_id: String,
    pub preset: SchedulePreset,
    pub cron_expr: String,
    pub enabled: bool,
    pub timezone: String,           // e.g. "UTC", "America/New_York"
    pub next_run: Option<String>,   // ISO 8601
    pub last_run: Option<String>,
    pub run_count: u64,
    pub fail_count: u64,
    pub created_at: String,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleRunResult {
    Success,
    Failed,
    Skipped,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct ScheduleRunLog {
    pub id: String,
    pub schedule_id: String,
    pub workflow_id: String,
    pub workflow_name: String,
    pub result: ScheduleRunResult,
    pub output: Vec<String>,
    pub error: Option<String>,
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
}

// ─────────────────────────────────────────────
//  Scheduler Store
// ─────────────────────────────────────────────

pub struct SchedulerStore {
    pub schedules: Vec<ScheduleEntry>,
    pub run_history: Vec<ScheduleRunLog>,
}

impl Default for SchedulerStore {
    fn default() -> Self {
        Self {
            schedules: Vec::new(),
            run_history: Vec::new(),
        }
    }
}

/// On-disk snapshot of the scheduler. Without this, every schedule and its
/// run history is lost on app restart - recurring jobs silently never resume.
#[derive(Serialize, Deserialize, Default)]
struct SchedulerSnapshot {
    #[serde(default)]
    schedules: Vec<ScheduleEntry>,
    #[serde(default)]
    run_history: Vec<ScheduleRunLog>,
}

const SCHEDULER_FILE: &str = "scheduler.json";

impl SchedulerStore {
    /// Load persisted schedules, recomputing `next_run` so a schedule that
    /// was due while the app was closed fires promptly on next tick.
    pub fn load() -> Self {
        match crate::storage::load_json::<SchedulerSnapshot>(SCHEDULER_FILE) {
            Ok(snap) => {
                let mut store = Self {
                    schedules: snap.schedules,
                    run_history: snap.run_history,
                };
                let now = Utc::now();
                for entry in &mut store.schedules {
                    if !entry.enabled {
                        entry.next_run = None;
                        continue;
                    }
                    let stale = entry
                        .next_run
                        .as_ref()
                        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
                        .map(|dt| dt <= now)
                        .unwrap_or(true);
                    if stale {
                        if let Ok(parsed) = CronExpr::parse(&entry.cron_expr) {
                            entry.next_run = parsed.next_after(now).map(|t| t.to_rfc3339());
                        }
                    }
                }
                store
            }
            Err(_) => Self::default(),
        }
    }

    /// Persist schedules + run history. Best-effort: a failed write must not
    /// crash the scheduler loop.
    pub fn save(&self) {
        let snap = SchedulerSnapshot {
            schedules: self.schedules.clone(),
            run_history: self.run_history.clone(),
        };
        let _ = crate::storage::save_json(SCHEDULER_FILE, &snap);
    }
}

impl SchedulerStore {
    // ── CRUD ──

    pub fn list_schedules(&self) -> Vec<ScheduleEntry> {
        self.schedules.clone()
    }

    pub fn get_schedule(&self, id: &str) -> Option<ScheduleEntry> {
        self.schedules.iter().find(|s| s.id == id).cloned()
    }

    pub fn create_schedule(
        &mut self,
        workflow_id: String,
        preset: SchedulePreset,
        custom_cron: Option<String>,
        timezone: Option<String>,
    ) -> Result<ScheduleEntry, String> {
        let cron_str = if preset == SchedulePreset::Custom {
            custom_cron.ok_or("Custom preset requires a cron expression")?
        } else {
            preset.to_cron().unwrap().to_string()
        };

        // Validate cron
        let parsed = CronExpr::parse(&cron_str)?;
        let next = parsed.next_after(Utc::now());

        let entry = ScheduleEntry {
            id: Uuid::new_v4().to_string(),
            workflow_id,
            preset,
            cron_expr: cron_str,
            enabled: true,
            timezone: timezone.unwrap_or_else(|| "local".to_string()),
            next_run: next.map(|t| t.to_rfc3339()),
            last_run: None,
            run_count: 0,
            fail_count: 0,
            created_at: Utc::now().to_rfc3339(),
        };
        self.schedules.push(entry.clone());
        Ok(entry)
    }

    pub fn update_schedule(
        &mut self,
        id: &str,
        preset: Option<SchedulePreset>,
        custom_cron: Option<String>,
        enabled: Option<bool>,
    ) -> Result<ScheduleEntry, String> {
        let entry = self.schedules.iter_mut().find(|s| s.id == id)
            .ok_or("Schedule not found")?;

        if let Some(en) = enabled {
            entry.enabled = en;
        }

        if let Some(p) = preset {
            let cron_str = if p == SchedulePreset::Custom {
                custom_cron.ok_or("Custom preset requires a cron expression")?
            } else {
                p.to_cron().unwrap().to_string()
            };
            CronExpr::parse(&cron_str)?; // validate
            entry.preset = p;
            entry.cron_expr = cron_str;
        }

        // Recalculate next run
        if entry.enabled {
            if let Ok(parsed) = CronExpr::parse(&entry.cron_expr) {
                entry.next_run = parsed.next_after(Utc::now()).map(|t| t.to_rfc3339());
            }
        } else {
            entry.next_run = None;
        }

        Ok(entry.clone())
    }

    pub fn delete_schedule(&mut self, id: &str) -> bool {
        let len = self.schedules.len();
        self.schedules.retain(|s| s.id != id);
        self.schedules.len() < len
    }

    pub fn toggle_schedule(&mut self, id: &str) -> Result<ScheduleEntry, String> {
        let entry = self.schedules.iter_mut().find(|s| s.id == id)
            .ok_or("Schedule not found")?;
        entry.enabled = !entry.enabled;

        if entry.enabled {
            if let Ok(parsed) = CronExpr::parse(&entry.cron_expr) {
                entry.next_run = parsed.next_after(Utc::now()).map(|t| t.to_rfc3339());
            }
        } else {
            entry.next_run = None;
        }

        Ok(entry.clone())
    }

    /// Get schedules that are due to fire now.
    pub fn get_due_schedules(&mut self) -> Vec<ScheduleEntry> {
        let now = Utc::now();
        let mut due = Vec::new();

        for entry in &mut self.schedules {
            if !entry.enabled { continue; }
            if let Some(ref next_str) = entry.next_run {
                if let Ok(next_dt) = next_str.parse::<DateTime<Utc>>() {
                    if now >= next_dt {
                        due.push(entry.clone());
                        // Advance to next run
                        if let Ok(parsed) = CronExpr::parse(&entry.cron_expr) {
                            entry.next_run = parsed.next_after(now).map(|t| t.to_rfc3339());
                        }
                    }
                }
            }
        }

        due
    }

    /// Record a schedule execution result.
    pub fn record_run(
        &mut self,
        schedule_id: &str,
        workflow_id: &str,
        workflow_name: &str,
        result: ScheduleRunResult,
        output: Vec<String>,
        error: Option<String>,
        started_at: String,
        duration_ms: u64,
    ) {
        // Update schedule stats
        if let Some(entry) = self.schedules.iter_mut().find(|s| s.id == schedule_id) {
            entry.run_count += 1;
            entry.last_run = Some(Utc::now().to_rfc3339());
            if result == ScheduleRunResult::Failed {
                entry.fail_count += 1;
            }
        }

        let log = ScheduleRunLog {
            id: Uuid::new_v4().to_string(),
            schedule_id: schedule_id.to_string(),
            workflow_id: workflow_id.to_string(),
            workflow_name: workflow_name.to_string(),
            result,
            output,
            error,
            started_at,
            finished_at: Utc::now().to_rfc3339(),
            duration_ms,
        };

        self.run_history.insert(0, log);
        // Keep last 200 entries
        if self.run_history.len() > 200 {
            self.run_history.truncate(200);
        }
    }

    // ── History ──

    pub fn get_run_history(&self, schedule_id: Option<&str>, limit: usize) -> Vec<ScheduleRunLog> {
        let iter = self.run_history.iter();
        match schedule_id {
            Some(sid) => iter.filter(|r| r.schedule_id == sid).take(limit).cloned().collect(),
            None => iter.take(limit).cloned().collect(),
        }
    }

    /// Preview next N fire times for a cron expression.
    pub fn preview_next_runs(cron_str: &str, count: usize) -> Result<Vec<String>, String> {
        let parsed = CronExpr::parse(cron_str)?;
        let mut results = Vec::new();
        let mut cursor = Utc::now();
        for _ in 0..count {
            match parsed.next_after(cursor) {
                Some(next) => {
                    results.push(next.to_rfc3339());
                    cursor = next;
                }
                None => break,
            }
        }
        Ok(results)
    }
}

#[cfg(test)]
mod local_time_tests {
    use super::*;
    use chrono::Timelike;

    #[test]
    fn daily_at_nine_means_nine_on_this_clock() {
        let cron = CronExpr::parse("0 9 * * *").unwrap();
        let next = cron.next_after(Utc::now()).unwrap();
        let local = next.with_timezone(&chrono::Local);
        assert_eq!(local.hour(), 9, "next run should be 09:00 local, got {}", local);
        assert_eq!(local.minute(), 0);
        assert!(next > Utc::now());
    }
}

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;
use std::collections::HashMap;

// ─────────────────────────────────────────────
//  KPI Snapshots
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiSnapshot {
    pub kpi_name: String,
    pub target_value: f32,
    pub actual_value: f32,
    pub unit: String,
    pub pct_of_target: f32,        // actual/target * 100
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevicePerformance {
    pub device_id: String,
    pub device_name: String,
    pub role: String,
    pub playbook_name: Option<String>,
    pub score: f32,                // 0-100 composite score
    pub kpis: Vec<KpiSnapshot>,
    pub items_processed: u32,
    pub items_failed: u32,
    pub items_escalated: u32,
    pub avg_processing_mins: f32,
    pub uptime_pct: f32,
    pub trend: Vec<f32>,           // last 7 days scores
    pub rank: u32,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RolePerformance {
    pub role: String,
    pub device_count: u32,
    pub avg_score: f32,
    pub best_device: Option<String>,
    pub worst_device: Option<String>,
    pub total_processed: u32,
    pub total_failed: u32,
    pub avg_processing_mins: f32,
    pub kpi_summary: Vec<KpiSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceSummary {
    pub fleet_score: f32,          // overall fleet score
    pub total_devices: u32,
    pub top_performers: Vec<String>,    // device names
    pub underperformers: Vec<String>,
    pub total_processed_today: u32,
    pub total_failed_today: u32,
    pub improvement_suggestions: Vec<String>,
}

// ─────────────────────────────────────────────
//  Daily Record
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyRecord {
    pub date: String,              // "2026-03-09"
    pub device_id: String,
    pub role: String,
    pub score: f32,
    pub items_processed: u32,
    pub items_failed: u32,
    pub avg_processing_mins: f32,
}

// ─────────────────────────────────────────────
//  Performance Report
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceReport {
    pub id: String,
    pub title: String,
    pub period: String,            // "daily", "weekly"
    pub date_range: String,
    pub devices: Vec<DevicePerformance>,
    pub roles: Vec<RolePerformance>,
    pub summary: PerformanceSummary,
    pub generated_at: String,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

const MAX_DAILY: usize = 500;
const MAX_REPORTS: usize = 50;

pub struct FleetPerfStore {
    pub daily_records: Vec<DailyRecord>,
    pub reports: Vec<PerformanceReport>,
}

impl Default for FleetPerfStore {
    fn default() -> Self {
        Self {
            daily_records: Vec::new(),
            reports: Vec::new(),
        }
    }
}

impl FleetPerfStore {
    pub fn record_daily(&mut self, record: DailyRecord) {
        self.daily_records.push(record);
        if self.daily_records.len() > MAX_DAILY {
            self.daily_records.drain(0..self.daily_records.len() - MAX_DAILY);
        }
    }

    pub fn get_device_trend(&self, device_id: &str, days: usize) -> Vec<f32> {
        let mut scores: Vec<(String, f32)> = self.daily_records.iter()
            .filter(|r| r.device_id == device_id)
            .map(|r| (r.date.clone(), r.score))
            .collect();
        scores.sort_by(|a, b| a.0.cmp(&b.0));
        scores.iter().rev().take(days).rev().map(|s| s.1).collect()
    }

    pub fn compute_device_performance(
        &self,
        device_id: &str,
        device_name: &str,
        role: &str,
        playbook_name: Option<&str>,
        kpi_targets: &[(String, f32, String)],  // (name, target, unit)
        items_processed: u32,
        items_failed: u32,
        items_escalated: u32,
        avg_processing_mins: f32,
        uptime_pct: f32,
    ) -> DevicePerformance {
        let mut kpis = Vec::new();
        let mut score_sum = 0.0_f32;

        for (name, target, unit) in kpi_targets {
            let actual = match name.as_str() {
                "Avg response time" | "Classification time" | "Review time" => avg_processing_mins,
                "Leads per hour" | "Throughput" | "Drafts per day" => items_processed as f32 / 8.0,
                "Completion rate" => if items_processed + items_failed > 0 {
                    items_processed as f32 / (items_processed + items_failed) as f32 * 100.0
                } else { 100.0 },
                "Quality score" => if items_failed == 0 { 95.0 } else {
                    (1.0 - items_failed as f32 / (items_processed + items_failed).max(1) as f32) * 100.0
                },
                _ => items_processed as f32 / 8.0,
            };
            let pct = if *target > 0.0 {
                // For time-based KPIs, lower is better
                if unit.contains("minute") || unit.contains("min") {
                    if actual > 0.0 { (*target / actual * 100.0).min(150.0) } else { 100.0 }
                } else {
                    (actual / *target * 100.0).min(150.0)
                }
            } else { 100.0 };
            score_sum += pct.min(100.0);
            kpis.push(KpiSnapshot {
                kpi_name: name.clone(),
                target_value: *target,
                actual_value: actual,
                unit: unit.clone(),
                pct_of_target: pct,
            });
        }

        let score = if kpis.is_empty() {
            // Default scoring from uptime + completion
            uptime_pct * 0.5 + if items_processed > 0 {
                (items_processed as f32 / (items_processed + items_failed).max(1) as f32) * 50.0
            } else { 25.0 }
        } else {
            score_sum / kpis.len() as f32
        };

        let trend = self.get_device_trend(device_id, 7);

        DevicePerformance {
            device_id: device_id.to_string(),
            device_name: device_name.to_string(),
            role: role.to_string(),
            playbook_name: playbook_name.map(|s| s.to_string()),
            score,
            kpis,
            items_processed,
            items_failed,
            items_escalated,
            avg_processing_mins,
            uptime_pct,
            trend,
            rank: 0,
            recorded_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn compute_role_performance(&self, devices: &[DevicePerformance]) -> Vec<RolePerformance> {
        let mut by_role: HashMap<String, Vec<&DevicePerformance>> = HashMap::new();
        for d in devices {
            by_role.entry(d.role.clone()).or_default().push(d);
        }

        by_role.into_iter().map(|(role, devs)| {
            let avg_score = devs.iter().map(|d| d.score).sum::<f32>() / devs.len() as f32;
            let best = devs.iter().max_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
            let worst = devs.iter().min_by(|a, b| a.score.partial_cmp(&b.score).unwrap_or(std::cmp::Ordering::Equal));
            let total_proc: u32 = devs.iter().map(|d| d.items_processed).sum();
            let total_fail: u32 = devs.iter().map(|d| d.items_failed).sum();
            let avg_mins = devs.iter().map(|d| d.avg_processing_mins).sum::<f32>() / devs.len() as f32;

            // Aggregate KPIs
            let mut kpi_map: HashMap<String, (f32, f32, String, u32)> = HashMap::new(); // name -> (sum_actual, target, unit, count)
            for d in &devs {
                for kpi in &d.kpis {
                    let entry = kpi_map.entry(kpi.kpi_name.clone()).or_insert((0.0, kpi.target_value, kpi.unit.clone(), 0));
                    entry.0 += kpi.actual_value;
                    entry.3 += 1;
                }
            }
            let kpi_summary: Vec<KpiSnapshot> = kpi_map.into_iter().map(|(name, (sum, target, unit, count))| {
                let avg = sum / count as f32;
                let pct = if target > 0.0 {
                    if unit.contains("min") { (target / avg * 100.0).min(150.0) }
                    else { (avg / target * 100.0).min(150.0) }
                } else { 100.0 };
                KpiSnapshot { kpi_name: name, target_value: target, actual_value: avg, unit, pct_of_target: pct }
            }).collect();

            RolePerformance {
                role,
                device_count: devs.len() as u32,
                avg_score,
                best_device: best.map(|d| d.device_name.clone()),
                worst_device: worst.map(|d| d.device_name.clone()),
                total_processed: total_proc,
                total_failed: total_fail,
                avg_processing_mins: avg_mins,
                kpi_summary,
            }
        }).collect()
    }

    pub fn compute_summary(&self, devices: &[DevicePerformance]) -> PerformanceSummary {
        let fleet_score = if devices.is_empty() { 0.0 } else {
            devices.iter().map(|d| d.score).sum::<f32>() / devices.len() as f32
        };
        let total_proc: u32 = devices.iter().map(|d| d.items_processed).sum();
        let total_fail: u32 = devices.iter().map(|d| d.items_failed).sum();

        let mut sorted: Vec<&DevicePerformance> = devices.iter().collect();
        sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

        let top: Vec<String> = sorted.iter().take(3).map(|d| d.device_name.clone()).collect();
        let under: Vec<String> = sorted.iter().rev().take(3)
            .filter(|d| d.score < 70.0)
            .map(|d| d.device_name.clone()).collect();

        let mut suggestions = Vec::new();
        for d in devices {
            if d.score < 50.0 {
                suggestions.push(format!("{} ({}) is significantly underperforming - review playbook and workload", d.device_name, d.role));
            }
            if d.items_failed > d.items_processed / 4 && d.items_processed > 0 {
                suggestions.push(format!("{} has a high failure rate ({}/{}) - check decision rules", d.device_name, d.items_failed, d.items_processed));
            }
            if d.uptime_pct < 90.0 {
                suggestions.push(format!("{} uptime is low ({:.0}%) - investigate connectivity", d.device_name, d.uptime_pct));
            }
        }

        PerformanceSummary {
            fleet_score,
            total_devices: devices.len() as u32,
            top_performers: top,
            underperformers: under,
            total_processed_today: total_proc,
            total_failed_today: total_fail,
            improvement_suggestions: suggestions,
        }
    }

    pub fn generate_report(&mut self, title: &str, period: &str, devices: Vec<DevicePerformance>) -> PerformanceReport {
        let roles = self.compute_role_performance(&devices);
        let summary = self.compute_summary(&devices);
        let report = PerformanceReport {
            id: Uuid::new_v4().to_string(),
            title: title.to_string(),
            period: period.to_string(),
            date_range: Utc::now().format("%Y-%m-%d").to_string(),
            devices,
            roles,
            summary,
            generated_at: Utc::now().to_rfc3339(),
        };
        self.reports.push(report.clone());
        if self.reports.len() > MAX_REPORTS {
            self.reports.drain(0..self.reports.len() - MAX_REPORTS);
        }
        report
    }

    pub fn list_reports(&self) -> Vec<PerformanceReport> {
        self.reports.clone()
    }

    pub fn get_report(&self, id: &str) -> Option<PerformanceReport> {
        self.reports.iter().find(|r| r.id == id).cloned()
    }

    pub fn delete_report(&mut self, id: &str) -> bool {
        let len = self.reports.len();
        self.reports.retain(|r| r.id != id);
        self.reports.len() < len
    }
}

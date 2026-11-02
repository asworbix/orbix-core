use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::Utc;

// ─────────────────────────────────────────────
//  Report Types
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FleetReportType {
    DailyDigest,
    WeeklySummary,
    ComplianceAudit,
    PerformanceReview,
    RoutingAnalysis,
    Custom(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportRequest {
    pub report_type: FleetReportType,
    pub title: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub device_filter: Option<Vec<String>>,  // specific device IDs
    pub role_filter: Option<Vec<String>>,    // specific roles
}

// ─────────────────────────────────────────────
//  Report Structure
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetReport {
    pub id: String,
    pub report_type: FleetReportType,
    pub title: String,
    pub date_range: String,
    pub sections: Vec<ReportSection>,
    pub recommendations: Vec<String>,
    pub generated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportSection {
    pub title: String,
    pub section_type: SectionType,
    pub entries: Vec<ReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SectionType {
    Summary,
    DeviceRanking,
    ComplianceStatus,
    RoutingEfficiency,
    WorkItemBreakdown,
    Alerts,
    Trends,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportEntry {
    pub label: String,
    pub value: String,
    pub change: Option<String>,       // "+12%", "-3", etc.
    pub status: Option<String>,       // "good", "warning", "critical"
}

// ─────────────────────────────────────────────
//  Report Templates
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub report_type: FleetReportType,
    pub is_builtin: bool,
}

// ─────────────────────────────────────────────
//  Store
// ─────────────────────────────────────────────

pub struct FleetReportStore {
    pub reports: Vec<FleetReport>,
    pub templates: Vec<ReportTemplate>,
}

impl Default for FleetReportStore {
    fn default() -> Self {
        Self {
            reports: Vec::new(),
            templates: vec![
                ReportTemplate {
                    id: "rpt-daily".into(),
                    name: "Daily Digest".into(),
                    description: "Quick overview of fleet activity, work items processed, and any issues from today".into(),
                    report_type: FleetReportType::DailyDigest,
                    is_builtin: true,
                },
                ReportTemplate {
                    id: "rpt-weekly".into(),
                    name: "Weekly Summary".into(),
                    description: "Comprehensive weekly fleet performance, trends, and top/bottom performers".into(),
                    report_type: FleetReportType::WeeklySummary,
                    is_builtin: true,
                },
                ReportTemplate {
                    id: "rpt-compliance".into(),
                    name: "Compliance Audit".into(),
                    description: "Full compliance status across all devices with violation details and remediation steps".into(),
                    report_type: FleetReportType::ComplianceAudit,
                    is_builtin: true,
                },
                ReportTemplate {
                    id: "rpt-performance".into(),
                    name: "Performance Review".into(),
                    description: "Device-by-device performance scores, KPI achievement, and improvement suggestions".into(),
                    report_type: FleetReportType::PerformanceReview,
                    is_builtin: true,
                },
                ReportTemplate {
                    id: "rpt-routing".into(),
                    name: "Routing Analysis".into(),
                    description: "Routing rule effectiveness, message flow patterns, and bottleneck identification".into(),
                    report_type: FleetReportType::RoutingAnalysis,
                    is_builtin: true,
                },
            ],
        }
    }
}

impl FleetReportStore {
    pub fn generate_report(
        &mut self,
        req: ReportRequest,
        fleet_data: FleetSnapshot,
    ) -> FleetReport {
        let now = Utc::now().to_rfc3339();
        let date_range = format!(
            "{} to {}",
            req.date_from.as_deref().unwrap_or("start"),
            req.date_to.as_deref().unwrap_or("now"),
        );

        let title = req.title.unwrap_or_else(|| match &req.report_type {
            FleetReportType::DailyDigest => "Daily Digest".into(),
            FleetReportType::WeeklySummary => "Weekly Summary".into(),
            FleetReportType::ComplianceAudit => "Compliance Audit".into(),
            FleetReportType::PerformanceReview => "Performance Review".into(),
            FleetReportType::RoutingAnalysis => "Routing Analysis".into(),
            FleetReportType::Custom(name) => name.clone(),
        });

        let sections = self.build_sections(&req.report_type, &fleet_data);
        let recommendations = self.build_recommendations(&req.report_type, &fleet_data);

        let report = FleetReport {
            id: Uuid::new_v4().to_string(),
            report_type: req.report_type,
            title,
            date_range,
            sections,
            recommendations,
            generated_at: now,
        };

        self.reports.push(report.clone());
        report
    }

    fn build_sections(&self, report_type: &FleetReportType, data: &FleetSnapshot) -> Vec<ReportSection> {
        let mut sections = Vec::new();

        // Summary section (always included)
        sections.push(ReportSection {
            title: "Fleet Overview".into(),
            section_type: SectionType::Summary,
            entries: vec![
                ReportEntry {
                    label: "Total Devices".into(),
                    value: data.total_devices.to_string(),
                    change: None,
                    status: None,
                },
                ReportEntry {
                    label: "Online Devices".into(),
                    value: data.online_devices.to_string(),
                    change: None,
                    status: if data.online_devices == data.total_devices { Some("good".into()) } else { Some("warning".into()) },
                },
                ReportEntry {
                    label: "Total Work Items".into(),
                    value: data.total_work_items.to_string(),
                    change: None,
                    status: None,
                },
                ReportEntry {
                    label: "Items Completed".into(),
                    value: data.items_completed.to_string(),
                    change: None,
                    status: Some("good".into()),
                },
                ReportEntry {
                    label: "Items Failed".into(),
                    value: data.items_failed.to_string(),
                    change: None,
                    status: if data.items_failed > 0 { Some("warning".into()) } else { Some("good".into()) },
                },
                ReportEntry {
                    label: "Active Routing Rules".into(),
                    value: data.active_routing_rules.to_string(),
                    change: None,
                    status: None,
                },
            ],
        });

        match report_type {
            FleetReportType::DailyDigest | FleetReportType::WeeklySummary => {
                // Device ranking
                if !data.device_scores.is_empty() {
                    sections.push(ReportSection {
                        title: "Device Rankings".into(),
                        section_type: SectionType::DeviceRanking,
                        entries: data.device_scores.iter().map(|(name, role, score)| {
                            ReportEntry {
                                label: name.clone(),
                                value: format!("{:.0}", score),
                                change: Some(role.clone()),
                                status: Some(if *score >= 90.0 { "good" } else if *score >= 70.0 { "warning" } else { "critical" }.into()),
                            }
                        }).collect(),
                    });
                }

                // Work item breakdown
                sections.push(ReportSection {
                    title: "Work Items".into(),
                    section_type: SectionType::WorkItemBreakdown,
                    entries: vec![
                        ReportEntry { label: "Queued".into(), value: data.items_queued.to_string(), change: None, status: None },
                        ReportEntry { label: "In Progress".into(), value: data.items_in_progress.to_string(), change: None, status: None },
                        ReportEntry { label: "Completed".into(), value: data.items_completed.to_string(), change: None, status: Some("good".into()) },
                        ReportEntry { label: "Failed".into(), value: data.items_failed.to_string(), change: None, status: if data.items_failed > 0 { Some("critical".into()) } else { Some("good".into()) } },
                        ReportEntry { label: "Blocked".into(), value: data.items_blocked.to_string(), change: None, status: if data.items_blocked > 0 { Some("warning".into()) } else { None } },
                    ],
                });
            }

            FleetReportType::ComplianceAudit => {
                sections.push(ReportSection {
                    title: "Compliance Status".into(),
                    section_type: SectionType::ComplianceStatus,
                    entries: vec![
                        ReportEntry { label: "Compliance Score".into(), value: format!("{:.0}%", data.compliance_score), change: None, status: Some(if data.compliance_score >= 90.0 { "good" } else if data.compliance_score >= 70.0 { "warning" } else { "critical" }.into()) },
                        ReportEntry { label: "Rules Enabled".into(), value: data.compliance_rules_enabled.to_string(), change: None, status: None },
                        ReportEntry { label: "Active Violations".into(), value: data.active_violations.to_string(), change: None, status: if data.active_violations > 0 { Some("critical".into()) } else { Some("good".into()) } },
                        ReportEntry { label: "Critical Violations".into(), value: data.critical_violations.to_string(), change: None, status: if data.critical_violations > 0 { Some("critical".into()) } else { Some("good".into()) } },
                    ],
                });
            }

            FleetReportType::PerformanceReview => {
                if !data.device_scores.is_empty() {
                    sections.push(ReportSection {
                        title: "Performance Scores".into(),
                        section_type: SectionType::DeviceRanking,
                        entries: data.device_scores.iter().map(|(name, role, score)| {
                            ReportEntry {
                                label: name.clone(),
                                value: format!("{:.0}/100", score),
                                change: Some(role.clone()),
                                status: Some(if *score >= 90.0 { "good" } else if *score >= 70.0 { "warning" } else { "critical" }.into()),
                            }
                        }).collect(),
                    });
                }

                sections.push(ReportSection {
                    title: "Fleet Performance".into(),
                    section_type: SectionType::Summary,
                    entries: vec![
                        ReportEntry { label: "Fleet Score".into(), value: format!("{:.0}", data.fleet_score), change: None, status: Some(if data.fleet_score >= 90.0 { "good" } else if data.fleet_score >= 70.0 { "warning" } else { "critical" }.into()) },
                        ReportEntry { label: "Automation Rules Active".into(), value: data.automation_rules_active.to_string(), change: None, status: None },
                        ReportEntry { label: "Automations Executed".into(), value: data.automation_executions.to_string(), change: None, status: None },
                    ],
                });
            }

            FleetReportType::RoutingAnalysis => {
                sections.push(ReportSection {
                    title: "Routing Efficiency".into(),
                    section_type: SectionType::RoutingEfficiency,
                    entries: vec![
                        ReportEntry { label: "Active Rules".into(), value: data.active_routing_rules.to_string(), change: None, status: None },
                        ReportEntry { label: "Messages Sent".into(), value: data.messages_sent.to_string(), change: None, status: None },
                        ReportEntry { label: "Active Channels".into(), value: data.active_channels.to_string(), change: None, status: None },
                        ReportEntry { label: "Unresolved Alerts".into(), value: data.unresolved_alerts.to_string(), change: None, status: if data.unresolved_alerts > 0 { Some("warning".into()) } else { Some("good".into()) } },
                    ],
                });
            }

            FleetReportType::Custom(_) => {}
        }

        sections
    }

    fn build_recommendations(&self, report_type: &FleetReportType, data: &FleetSnapshot) -> Vec<String> {
        let mut recs = Vec::new();

        if data.online_devices < data.total_devices {
            let offline = data.total_devices - data.online_devices;
            recs.push(format!("{} device(s) are offline - investigate connectivity or restart", offline));
        }

        if data.items_blocked > 0 {
            recs.push(format!("{} work item(s) are blocked - review dependencies and unblock", data.items_blocked));
        }

        if data.items_failed > 0 {
            recs.push(format!("{} work item(s) failed - check device logs and playbook configurations", data.items_failed));
        }

        if data.compliance_score < 80.0 && matches!(report_type, FleetReportType::ComplianceAudit | FleetReportType::WeeklySummary) {
            recs.push("Fleet compliance score is below 80% - review and resolve violations".into());
        }

        if data.active_violations > 0 {
            recs.push(format!("{} active compliance violation(s) need resolution", data.active_violations));
        }

        if data.fleet_score < 70.0 && matches!(report_type, FleetReportType::PerformanceReview | FleetReportType::WeeklySummary) {
            recs.push("Fleet performance score is below 70 - review underperforming devices and KPI targets".into());
        }

        if data.unresolved_alerts > 0 {
            recs.push(format!("{} unresolved fleet alert(s) - acknowledge and take action", data.unresolved_alerts));
        }

        if data.automation_rules_active == 0 {
            recs.push("No automation rules are active - consider enabling auto-healing for better reliability".into());
        }

        if recs.is_empty() {
            recs.push("Fleet is operating well - no immediate actions required".into());
        }

        recs
    }

    pub fn list_reports(&self) -> Vec<FleetReport> {
        let mut reports = self.reports.clone();
        reports.sort_by(|a, b| b.generated_at.cmp(&a.generated_at));
        reports
    }

    pub fn get_report(&self, id: &str) -> Option<FleetReport> {
        self.reports.iter().find(|r| r.id == id).cloned()
    }

    pub fn delete_report(&mut self, id: &str) -> bool {
        let len = self.reports.len();
        self.reports.retain(|r| r.id != id);
        self.reports.len() < len
    }

    pub fn list_templates(&self) -> Vec<ReportTemplate> {
        self.templates.clone()
    }
}

// ─────────────────────────────────────────────
//  Snapshot (populated from other stores)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FleetSnapshot {
    pub total_devices: u32,
    pub online_devices: u32,
    pub total_work_items: u32,
    pub items_queued: u32,
    pub items_in_progress: u32,
    pub items_completed: u32,
    pub items_failed: u32,
    pub items_blocked: u32,
    pub active_routing_rules: u32,
    pub messages_sent: u32,
    pub active_channels: u32,
    pub unresolved_alerts: u32,
    pub compliance_score: f32,
    pub compliance_rules_enabled: u32,
    pub active_violations: u32,
    pub critical_violations: u32,
    pub fleet_score: f32,
    pub device_scores: Vec<(String, String, f32)>,  // (name, role, score)
    pub automation_rules_active: u32,
    pub automation_executions: u32,
}

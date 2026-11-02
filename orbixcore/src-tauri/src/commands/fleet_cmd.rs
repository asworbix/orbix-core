//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Device Fleet & Hierarchy (Epic AJ)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn list_fleet_devices(state: State<'_, AppState>) -> Vec<fleet::FleetDevice> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).list_devices()
}

#[tauri::command]
pub(crate) fn get_fleet_device(state: State<'_, AppState>, id: String) -> Option<fleet::FleetDevice> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).get_device(&id)
}

#[tauri::command]
pub(crate) fn register_fleet_device(state: State<'_, AppState>, device: fleet::FleetDevice) -> fleet::FleetDevice {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).register_device(device)
}

#[tauri::command]
pub(crate) fn remove_fleet_device(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).remove_device(&id)
}

#[tauri::command]
pub(crate) fn update_fleet_device_status(state: State<'_, AppState>, id: String, status: fleet::DeviceStatus) -> Option<fleet::FleetDevice> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).update_device_status(&id, status)
}

#[tauri::command]
pub(crate) fn fleet_heartbeat(state: State<'_, AppState>, id: String, workload: u32, chains: Vec<String>, workflows: Vec<String>) -> Option<fleet::FleetDevice> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).heartbeat(&id, workload, chains, workflows)
}

#[tauri::command]
pub(crate) fn set_fleet_device_parent(state: State<'_, AppState>, id: String, parent_id: Option<String>) -> Option<fleet::FleetDevice> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).set_device_parent(&id, parent_id.as_deref())
}

#[tauri::command]
pub(crate) fn get_fleet_hierarchy(state: State<'_, AppState>) -> Vec<fleet::HierarchyNode> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).get_hierarchy()
}

#[tauri::command]
pub(crate) fn list_work_items(state: State<'_, AppState>, status: Option<String>, device_id: Option<String>, role: Option<String>) -> Vec<fleet::WorkItem> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).list_work_items(status.as_deref(), device_id.as_deref(), role.as_deref())
}

#[tauri::command]
pub(crate) fn get_work_item(state: State<'_, AppState>, id: String) -> Option<fleet::WorkItem> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).get_work_item(&id)
}

#[tauri::command]
pub(crate) fn create_work_item(state: State<'_, AppState>, item: fleet::WorkItem) -> fleet::WorkItem {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).create_work_item(item)
}

#[tauri::command]
pub(crate) fn assign_work_item(state: State<'_, AppState>, item_id: String, device_id: String) -> Option<fleet::WorkItem> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).assign_work_item(&item_id, &device_id)
}

#[tauri::command]
pub(crate) fn update_work_item_status(state: State<'_, AppState>, item_id: String, status: fleet::WorkItemStatus, details: Option<String>) -> Option<fleet::WorkItem> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).update_work_item_status(&item_id, status, details.as_deref())
}

#[tauri::command]
pub(crate) fn handoff_work_item(state: State<'_, AppState>, item_id: String, from_device: String, to_device: String) -> Option<fleet::WorkItem> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).handoff_work_item(&item_id, &from_device, &to_device)
}

#[tauri::command]
pub(crate) fn complete_work_item(state: State<'_, AppState>, item_id: String, result: Option<String>) -> Option<fleet::WorkItem> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).complete_work_item(&item_id, result.as_deref())
}

#[tauri::command]
pub(crate) fn get_fleet_overview(state: State<'_, AppState>) -> fleet::FleetOverview {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).get_overview()
}

#[tauri::command]
pub(crate) fn get_fleet_activity(state: State<'_, AppState>) -> Vec<fleet::DeviceActivity> {
    state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner()).get_device_activity()
}


// ─────────────────────────────────────────────
//  Fleet Routing & Communication (Epic AK)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn list_routing_rules(state: State<'_, AppState>) -> Vec<fleet_routing::RoutingRule> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).list_routing_rules()
}

#[tauri::command]
pub(crate) fn create_routing_rule(state: State<'_, AppState>, rule: fleet_routing::RoutingRule) -> fleet_routing::RoutingRule {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).create_routing_rule(rule)
}

#[tauri::command]
pub(crate) fn update_routing_rule(state: State<'_, AppState>, rule: fleet_routing::RoutingRule) -> Option<fleet_routing::RoutingRule> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).update_routing_rule(rule)
}

#[tauri::command]
pub(crate) fn delete_routing_rule(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).delete_routing_rule(&id)
}

#[tauri::command]
pub(crate) fn toggle_routing_rule(state: State<'_, AppState>, id: String) -> Option<fleet_routing::RoutingRule> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_routing_rule(&id)
}

#[tauri::command]
pub(crate) fn list_escalation_rules(state: State<'_, AppState>) -> Vec<fleet_routing::EscalationRule> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).list_escalation_rules()
}

#[tauri::command]
pub(crate) fn create_escalation_rule(state: State<'_, AppState>, rule: fleet_routing::EscalationRule) -> fleet_routing::EscalationRule {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).create_escalation_rule(rule)
}

#[tauri::command]
pub(crate) fn delete_escalation_rule(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).delete_escalation_rule(&id)
}

#[tauri::command]
pub(crate) fn toggle_escalation_rule(state: State<'_, AppState>, id: String) -> Option<fleet_routing::EscalationRule> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_escalation_rule(&id)
}

#[tauri::command]
pub(crate) fn send_fleet_message(state: State<'_, AppState>, msg: fleet_routing::FleetMessage) -> fleet_routing::FleetMessage {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).send_message(msg)
}

#[tauri::command]
pub(crate) fn list_fleet_messages(state: State<'_, AppState>, channel: Option<String>, limit: Option<usize>) -> Vec<fleet_routing::FleetMessage> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).list_messages(channel.as_deref(), limit.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn list_fleet_channels(state: State<'_, AppState>) -> Vec<fleet_routing::FleetChannel> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).list_channels()
}

#[tauri::command]
pub(crate) fn mark_fleet_messages_read(state: State<'_, AppState>, channel: String) {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).mark_messages_read(&channel);
}

#[tauri::command]
pub(crate) fn fleet_unread_count(state: State<'_, AppState>) -> usize {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).unread_count()
}

#[tauri::command]
pub(crate) fn list_fleet_alerts(state: State<'_, AppState>, unacked_only: Option<bool>) -> Vec<fleet_routing::FleetAlert> {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).list_alerts(unacked_only.unwrap_or(false))
}

#[tauri::command]
pub(crate) fn acknowledge_fleet_alert(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).acknowledge_alert(&id)
}

#[tauri::command]
pub(crate) fn acknowledge_all_fleet_alerts(state: State<'_, AppState>) {
    state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner()).acknowledge_all_alerts();
}


// ─────────────────────────────────────────────
//  Fleet Live Monitor (Epic AL)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn log_fleet_event(state: State<'_, AppState>, event: fleet_monitor::ActivityEvent) -> fleet_monitor::ActivityEvent {
    state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner()).log_event(event)
}

#[tauri::command]
pub(crate) fn list_fleet_events(
    state: State<'_, AppState>,
    device_id: Option<String>,
    role: Option<String>,
    event_type: Option<String>,
    severity: Option<String>,
    limit: Option<usize>,
) -> Vec<fleet_monitor::ActivityEvent> {
    state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner()).list_events(
        device_id.as_deref(), role.as_deref(), event_type.as_deref(),
        severity.as_deref(), limit.unwrap_or(50),
    )
}

#[tauri::command]
pub(crate) fn clear_fleet_events(state: State<'_, AppState>) {
    state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner()).clear_events();
}

#[tauri::command]
pub(crate) fn get_fleet_map(state: State<'_, AppState>) -> fleet_monitor::FleetMap {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let monitor = state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner());
    let routing = state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner());

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let devices: Vec<_> = fleet.devices.iter().map(|d| {
        let active = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id)
                && matches!(w.status, fleet::WorkItemStatus::InProgress | fleet::WorkItemStatus::Assigned))
            .count() as u32;
        let completed = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id)
                && w.status == fleet::WorkItemStatus::Completed
                && w.updated_at.starts_with(&today))
            .count() as u32;
        (d.id.clone(), d.name.clone(), d.role.clone(), format!("{:?}", d.status), active, completed)
    }).collect();

    let routes: Vec<_> = routing.routing_rules.iter().map(|r| {
        (r.from_role.clone(), r.to_role.clone(), r.name.clone(), r.trigger_count, r.enabled)
    }).collect();

    monitor.build_map(&devices, &routes)
}

#[tauri::command]
pub(crate) fn get_fleet_health(state: State<'_, AppState>) -> fleet_monitor::FleetHealthSummary {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let monitor = state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner());
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let devices: Vec<_> = fleet.devices.iter().map(|d| {
        let active = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id)
                && matches!(w.status, fleet::WorkItemStatus::InProgress | fleet::WorkItemStatus::Assigned))
            .count() as u32;
        let completed = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id)
                && w.status == fleet::WorkItemStatus::Completed
                && w.updated_at.starts_with(&today))
            .count() as u32;
        let failed = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id)
                && w.status == fleet::WorkItemStatus::Failed)
            .count() as u32;
        // Rough avg processing time from timeline
        let avg_mins = 5.0_f32; // placeholder
        (d.id.clone(), d.name.clone(), d.role.clone(), active, completed, failed, avg_mins)
    }).collect();

    monitor.compute_health(&devices)
}

#[tauri::command]
pub(crate) fn list_fleet_presets(state: State<'_, AppState>) -> Vec<fleet_monitor::FleetPreset> {
    state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner()).list_presets()
}

#[tauri::command]
pub(crate) fn get_fleet_preset(state: State<'_, AppState>, id: String) -> Option<fleet_monitor::FleetPreset> {
    state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner()).get_preset(&id)
}

#[tauri::command]
pub(crate) fn create_fleet_preset(state: State<'_, AppState>, preset: fleet_monitor::FleetPreset) -> fleet_monitor::FleetPreset {
    state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner()).create_preset(preset)
}

#[tauri::command]
pub(crate) fn delete_fleet_preset(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_monitor_store.lock().unwrap_or_else(|e| e.into_inner()).delete_preset(&id)
}


// ─────────────────────────────────────────────
//  Role Playbooks (Epic AM)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn list_playbooks(state: State<'_, AppState>) -> Vec<playbook::Playbook> {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).list_playbooks()
}

#[tauri::command]
pub(crate) fn get_playbook(state: State<'_, AppState>, id: String) -> Option<playbook::Playbook> {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).get_playbook(&id)
}

#[tauri::command]
pub(crate) fn create_playbook(state: State<'_, AppState>, playbook: playbook::Playbook) -> playbook::Playbook {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).create_playbook(playbook)
}

#[tauri::command]
pub(crate) fn update_playbook(state: State<'_, AppState>, playbook: playbook::Playbook) -> Option<playbook::Playbook> {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).update_playbook(playbook)
}

#[tauri::command]
pub(crate) fn delete_playbook(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).delete_playbook(&id)
}

#[tauri::command]
pub(crate) fn duplicate_playbook(state: State<'_, AppState>, id: String) -> Option<playbook::Playbook> {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).duplicate_playbook(&id)
}

#[tauri::command]
pub(crate) fn assign_playbook(state: State<'_, AppState>, device_id: String, playbook_id: String) {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).assign_playbook(&device_id, &playbook_id);
}

#[tauri::command]
pub(crate) fn unassign_playbook(state: State<'_, AppState>, device_id: String) {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).unassign_playbook(&device_id);
}

#[tauri::command]
pub(crate) fn get_device_playbook(state: State<'_, AppState>, device_id: String) -> Option<playbook::Playbook> {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).get_device_playbook(&device_id)
}

#[tauri::command]
pub(crate) fn list_playbook_assignments(state: State<'_, AppState>) -> Vec<(String, String)> {
    state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner()).list_assignments()
}


// ─────────────────────────────────────────────
//  Fleet Performance (Epic AN)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn get_fleet_performance(state: State<'_, AppState>) -> Vec<fleet_perf::DevicePerformance> {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let playbook_store = state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner());
    let perf = state.fleet.fleet_perf_store.lock().unwrap_or_else(|e| e.into_inner());
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let mut perfs: Vec<fleet_perf::DevicePerformance> = fleet.devices.iter().map(|d| {
        let pb = playbook_store.get_device_playbook(&d.id);
        let kpi_targets: Vec<(String, f32, String)> = pb.as_ref()
            .map(|p| p.kpi_targets.iter().map(|k| (k.name.clone(), k.target_value, k.unit.clone())).collect())
            .unwrap_or_default();

        let processed = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::Completed && w.updated_at.starts_with(&today))
            .count() as u32;
        let failed = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::Failed)
            .count() as u32;
        let escalated = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::HandedOff)
            .count() as u32;

        perf.compute_device_performance(
            &d.id, &d.name, &d.role,
            pb.as_ref().map(|p| p.name.as_str()),
            &kpi_targets,
            processed, failed, escalated,
            5.0, // placeholder avg processing
            100.0, // placeholder uptime
        )
    }).collect();

    // Assign ranks
    perfs.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    for (i, p) in perfs.iter_mut().enumerate() {
        p.rank = (i + 1) as u32;
    }
    perfs
}

#[tauri::command]
pub(crate) fn get_role_performance(state: State<'_, AppState>) -> Vec<fleet_perf::RolePerformance> {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let playbook_store = state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner());
    let perf = state.fleet.fleet_perf_store.lock().unwrap_or_else(|e| e.into_inner());
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let device_perfs: Vec<fleet_perf::DevicePerformance> = fleet.devices.iter().map(|d| {
        let pb = playbook_store.get_device_playbook(&d.id);
        let kpi_targets: Vec<(String, f32, String)> = pb.as_ref()
            .map(|p| p.kpi_targets.iter().map(|k| (k.name.clone(), k.target_value, k.unit.clone())).collect())
            .unwrap_or_default();
        let processed = fleet.work_items.iter().filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::Completed && w.updated_at.starts_with(&today)).count() as u32;
        let failed = fleet.work_items.iter().filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::Failed).count() as u32;
        let escalated = fleet.work_items.iter().filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::HandedOff).count() as u32;
        perf.compute_device_performance(&d.id, &d.name, &d.role, pb.as_ref().map(|p| p.name.as_str()), &kpi_targets, processed, failed, escalated, 5.0, 100.0)
    }).collect();

    perf.compute_role_performance(&device_perfs)
}

#[tauri::command]
pub(crate) fn get_performance_summary(state: State<'_, AppState>) -> fleet_perf::PerformanceSummary {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let playbook_store = state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner());
    let perf = state.fleet.fleet_perf_store.lock().unwrap_or_else(|e| e.into_inner());
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let device_perfs: Vec<fleet_perf::DevicePerformance> = fleet.devices.iter().map(|d| {
        let pb = playbook_store.get_device_playbook(&d.id);
        let kpi_targets: Vec<(String, f32, String)> = pb.as_ref()
            .map(|p| p.kpi_targets.iter().map(|k| (k.name.clone(), k.target_value, k.unit.clone())).collect())
            .unwrap_or_default();
        let processed = fleet.work_items.iter().filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::Completed && w.updated_at.starts_with(&today)).count() as u32;
        let failed = fleet.work_items.iter().filter(|w| w.assigned_device.as_deref() == Some(&d.id) && w.status == fleet::WorkItemStatus::Failed).count() as u32;
        perf.compute_device_performance(&d.id, &d.name, &d.role, pb.as_ref().map(|p| p.name.as_str()), &kpi_targets, processed, failed, 0, 5.0, 100.0)
    }).collect();

    perf.compute_summary(&device_perfs)
}

#[tauri::command]
pub(crate) fn list_perf_reports(state: State<'_, AppState>) -> Vec<fleet_perf::PerformanceReport> {
    state.fleet.fleet_perf_store.lock().unwrap_or_else(|e| e.into_inner()).list_reports()
}

#[tauri::command]
pub(crate) fn get_perf_report(state: State<'_, AppState>, id: String) -> Option<fleet_perf::PerformanceReport> {
    state.fleet.fleet_perf_store.lock().unwrap_or_else(|e| e.into_inner()).get_report(&id)
}

#[tauri::command]
pub(crate) fn delete_perf_report(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_perf_store.lock().unwrap_or_else(|e| e.into_inner()).delete_report(&id)
}

// ── Fleet Audit Trail & Compliance (Epic AO) ─────

#[tauri::command]
pub(crate) fn search_audit_log(
    state: State<'_, AppState>,
    query: Option<String>,
    category: Option<String>,
    severity: Option<String>,
    device_id: Option<String>,
    action: Option<String>,
    limit: Option<usize>,
) -> Vec<audit::AuditEntry> {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).search(
        query.as_deref(),
        category.as_deref(),
        severity.as_deref(),
        device_id.as_deref(),
        action.as_deref(),
        limit.unwrap_or(100),
    )
}

#[tauri::command]
pub(crate) fn log_audit_entry(state: State<'_, AppState>, entry: audit::AuditEntry) -> audit::AuditEntry {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).log(entry)
}

#[tauri::command]
pub(crate) fn audit_count(state: State<'_, AppState>) -> usize {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).count()
}

#[tauri::command]
pub(crate) fn clear_audit_log(state: State<'_, AppState>) {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

#[tauri::command]
pub(crate) fn get_audit_retention(state: State<'_, AppState>) -> audit::RetentionPolicy {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).get_retention()
}

#[tauri::command]
pub(crate) fn update_audit_retention(state: State<'_, AppState>, policy: audit::RetentionPolicy) -> audit::RetentionPolicy {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).update_retention(policy)
}

#[tauri::command]
pub(crate) fn list_compliance_rules(state: State<'_, AppState>) -> Vec<audit::ComplianceRule> {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).list_compliance_rules()
}

#[tauri::command]
pub(crate) fn toggle_compliance_rule(state: State<'_, AppState>, id: String) -> Option<audit::ComplianceRule> {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_compliance_rule(&id)
}

#[tauri::command]
pub(crate) fn create_compliance_rule(state: State<'_, AppState>, rule: audit::ComplianceRule) -> audit::ComplianceRule {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).create_compliance_rule(rule)
}

#[tauri::command]
pub(crate) fn delete_compliance_rule(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).delete_compliance_rule(&id)
}

#[tauri::command]
pub(crate) fn list_compliance_violations(state: State<'_, AppState>, active_only: Option<bool>) -> Vec<audit::ComplianceViolation> {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).list_violations(active_only.unwrap_or(true))
}

#[tauri::command]
pub(crate) fn resolve_compliance_violation(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner()).resolve_violation(&id)
}

#[tauri::command]
pub(crate) fn get_compliance_summary(state: State<'_, AppState>) -> audit::ComplianceSummary {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let playbook_store = state.fleet.playbook_store.lock().unwrap_or_else(|e| e.into_inner());
    let _perf = state.fleet.fleet_perf_store.lock().unwrap_or_else(|e| e.into_inner());
    let audit = state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner());

    let devices: Vec<(String, String, String, bool, f32, f32)> = fleet.devices.iter().map(|d| {
        let has_pb = playbook_store.assignments.iter().any(|(did, _)| did == &d.id);
        let failure_rate = if d.current_workload > 0 { 0.0 } else { 0.0 }; // simplified
        let uptime = 99.0; // simplified
        (d.id.clone(), d.name.clone(), d.role.clone(), has_pb, failure_rate, uptime)
    }).collect();

    audit.compute_compliance(&devices)
}

// ── Fleet Automation & Auto-Healing (Epic AP) ────

#[tauri::command]
pub(crate) fn list_auto_rules(state: State<'_, AppState>) -> Vec<fleet_auto::AutomationRule> {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).list_rules()
}

#[tauri::command]
pub(crate) fn create_auto_rule(state: State<'_, AppState>, rule: fleet_auto::AutomationRule) -> fleet_auto::AutomationRule {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).create_rule(rule)
}

#[tauri::command]
pub(crate) fn update_auto_rule(state: State<'_, AppState>, rule: fleet_auto::AutomationRule) -> Option<fleet_auto::AutomationRule> {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).update_rule(rule)
}

#[tauri::command]
pub(crate) fn delete_auto_rule(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).delete_rule(&id)
}

#[tauri::command]
pub(crate) fn toggle_auto_rule(state: State<'_, AppState>, id: String) -> Option<fleet_auto::AutomationRule> {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_rule(&id)
}

#[tauri::command]
pub(crate) fn create_rule_from_template(state: State<'_, AppState>, template_id: String, name: Option<String>) -> Option<fleet_auto::AutomationRule> {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).create_rule_from_template(&template_id, name)
}

#[tauri::command]
pub(crate) fn log_auto_execution(state: State<'_, AppState>, exec: fleet_auto::AutoExecution) -> fleet_auto::AutoExecution {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).log_execution(exec)
}

#[tauri::command]
pub(crate) fn list_auto_executions(state: State<'_, AppState>, rule_id: Option<String>, success_only: Option<bool>, limit: Option<usize>) -> Vec<fleet_auto::AutoExecution> {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).list_executions(rule_id.as_deref(), success_only, limit.unwrap_or(100))
}

#[tauri::command]
pub(crate) fn clear_auto_executions(state: State<'_, AppState>) {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).clear_executions();
}

#[tauri::command]
pub(crate) fn get_auto_stats(state: State<'_, AppState>) -> fleet_auto::AutoStats {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).execution_stats()
}

#[tauri::command]
pub(crate) fn list_auto_templates(state: State<'_, AppState>) -> Vec<fleet_auto::AutoTemplate> {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).list_templates()
}

#[tauri::command]
pub(crate) fn get_auto_template(state: State<'_, AppState>, id: String) -> Option<fleet_auto::AutoTemplate> {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).get_template(&id)
}

#[tauri::command]
pub(crate) fn delete_auto_template(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner()).delete_template(&id)
}

// ── Fleet Reports & Insights (Epic AQ) ───────────

#[tauri::command]
pub(crate) fn generate_fleet_report(state: State<'_, AppState>, req: fleet_reports::ReportRequest) -> fleet_reports::FleetReport {
    // Build snapshot from all stores
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let routing = state.fleet.fleet_routing_store.lock().unwrap_or_else(|e| e.into_inner());
    let audit = state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner());
    let auto = state.fleet.fleet_auto_store.lock().unwrap_or_else(|e| e.into_inner());

    let online = fleet.devices.iter().filter(|d| format!("{:?}", d.status) == "Online").count() as u32;
    let items_queued = fleet.work_items.iter().filter(|i| format!("{:?}", i.status) == "Queued").count() as u32;
    let items_in_progress = fleet.work_items.iter().filter(|i| format!("{:?}", i.status) == "InProgress").count() as u32;
    let items_completed = fleet.work_items.iter().filter(|i| format!("{:?}", i.status) == "Completed").count() as u32;
    let items_failed = fleet.work_items.iter().filter(|i| format!("{:?}", i.status) == "Failed").count() as u32;
    let items_blocked = fleet.work_items.iter().filter(|i| format!("{:?}", i.status) == "Blocked").count() as u32;

    let active_routing = routing.routing_rules.iter().filter(|r| r.enabled).count() as u32;
    let messages_sent = routing.messages.len() as u32;
    let active_channels = routing.list_channels().len() as u32;
    let unresolved_alerts = routing.alerts.iter().filter(|a| !a.acknowledged).count() as u32;

    let compliance_rules_enabled = audit.compliance_rules.iter().filter(|r| r.enabled).count() as u32;
    let active_violations = audit.violations.iter().filter(|v| !v.resolved).count() as u32;
    let critical_violations = audit.violations.iter().filter(|v| !v.resolved && (v.severity == "high" || v.severity == "critical")).count() as u32;

    let auto_active = auto.rules.iter().filter(|r| r.enabled).count() as u32;
    let auto_execs = auto.executions.len() as u32;

    let snapshot = fleet_reports::FleetSnapshot {
        total_devices: fleet.devices.len() as u32,
        online_devices: online,
        total_work_items: fleet.work_items.len() as u32,
        items_queued,
        items_in_progress,
        items_completed,
        items_failed,
        items_blocked,
        active_routing_rules: active_routing,
        messages_sent,
        active_channels,
        unresolved_alerts,
        compliance_score: if compliance_rules_enabled > 0 { 100.0 } else { 100.0 },
        compliance_rules_enabled,
        active_violations,
        critical_violations,
        fleet_score: 0.0,
        device_scores: Vec::new(),
        automation_rules_active: auto_active,
        automation_executions: auto_execs,
    };

    drop(fleet);
    drop(routing);
    drop(audit);
    drop(auto);

    state.fleet.fleet_report_store.lock().unwrap_or_else(|e| e.into_inner()).generate_report(req, snapshot)
}

#[tauri::command]
pub(crate) fn list_fleet_reports(state: State<'_, AppState>) -> Vec<fleet_reports::FleetReport> {
    state.fleet.fleet_report_store.lock().unwrap_or_else(|e| e.into_inner()).list_reports()
}

#[tauri::command]
pub(crate) fn get_fleet_report(state: State<'_, AppState>, id: String) -> Option<fleet_reports::FleetReport> {
    state.fleet.fleet_report_store.lock().unwrap_or_else(|e| e.into_inner()).get_report(&id)
}

#[tauri::command]
pub(crate) fn delete_fleet_report(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_report_store.lock().unwrap_or_else(|e| e.into_inner()).delete_report(&id)
}

#[tauri::command]
pub(crate) fn list_report_templates(state: State<'_, AppState>) -> Vec<fleet_reports::ReportTemplate> {
    state.fleet.fleet_report_store.lock().unwrap_or_else(|e| e.into_inner()).list_templates()
}

// ── Fleet Tags & Filtering (Epic AR) ─────────────

#[tauri::command]
pub(crate) fn list_fleet_tags(state: State<'_, AppState>) -> Vec<fleet_tags::FleetTag> {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).list_tags()
}

#[tauri::command]
pub(crate) fn create_fleet_tag(state: State<'_, AppState>, tag: fleet_tags::FleetTag) -> fleet_tags::FleetTag {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).create_tag(tag)
}

#[tauri::command]
pub(crate) fn update_fleet_tag(state: State<'_, AppState>, tag: fleet_tags::FleetTag) -> Option<fleet_tags::FleetTag> {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).update_tag(tag)
}

#[tauri::command]
pub(crate) fn delete_fleet_tag(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).delete_tag(&id)
}

#[tauri::command]
pub(crate) fn assign_fleet_tag(state: State<'_, AppState>, tag_id: String, resource_type: fleet_tags::ResourceType, resource_id: String) -> bool {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).assign_tag(&tag_id, resource_type, &resource_id)
}

#[tauri::command]
pub(crate) fn remove_fleet_tag_assignment(state: State<'_, AppState>, tag_id: String, resource_type: fleet_tags::ResourceType, resource_id: String) -> bool {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).remove_tag_assignment(&tag_id, resource_type, &resource_id)
}

#[tauri::command]
pub(crate) fn get_resource_tags(state: State<'_, AppState>, resource_type: fleet_tags::ResourceType, resource_id: String) -> Vec<fleet_tags::FleetTag> {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).get_resource_tags(resource_type, &resource_id)
}

#[tauri::command]
pub(crate) fn get_resources_by_tag(state: State<'_, AppState>, tag_id: String, resource_type: Option<fleet_tags::ResourceType>) -> Vec<String> {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).get_resources_by_tag(&tag_id, resource_type)
}

#[tauri::command]
pub(crate) fn bulk_assign_fleet_tag(state: State<'_, AppState>, tag_id: String, resource_type: fleet_tags::ResourceType, resource_ids: Vec<String>) -> u32 {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).bulk_assign_tag(&tag_id, resource_type, &resource_ids)
}

#[tauri::command]
pub(crate) fn list_saved_filters(state: State<'_, AppState>) -> Vec<fleet_tags::SavedFilter> {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).list_saved_filters()
}

#[tauri::command]
pub(crate) fn create_saved_filter(state: State<'_, AppState>, filter: fleet_tags::SavedFilter) -> fleet_tags::SavedFilter {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).create_saved_filter(filter)
}

#[tauri::command]
pub(crate) fn delete_saved_filter(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).delete_saved_filter(&id)
}

#[tauri::command]
pub(crate) fn get_tag_analytics(state: State<'_, AppState>) -> fleet_tags::TagAnalytics {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    let total_devices = fleet.devices.len() as u32;
    drop(fleet);
    state.fleet.fleet_tag_store.lock().unwrap_or_else(|e| e.into_inner()).get_analytics(total_devices)
}

// ── Fleet Activity Timeline (Epic AT) ────────────

#[tauri::command]
pub(crate) fn push_timeline_event(state: State<'_, AppState>, event: fleet_timeline::TimelineEvent) -> fleet_timeline::TimelineEvent {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).push_event(event)
}

#[tauri::command]
pub(crate) fn list_timeline_events(
    state: State<'_, AppState>,
    category: Option<String>,
    severity: Option<String>,
    device_id: Option<String>,
    source_system: Option<String>,
    bookmarked_only: Option<bool>,
    limit: Option<usize>,
) -> Vec<fleet_timeline::TimelineEvent> {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).list_events(
        category.as_deref(), severity.as_deref(), device_id.as_deref(),
        source_system.as_deref(), bookmarked_only.unwrap_or(false), limit.unwrap_or(100),
    )
}

#[tauri::command]
pub(crate) fn search_timeline(state: State<'_, AppState>, query: String, limit: Option<usize>) -> Vec<fleet_timeline::TimelineEvent> {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).search_events(&query, limit.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn toggle_timeline_bookmark(state: State<'_, AppState>, id: String) -> Option<fleet_timeline::TimelineEvent> {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_bookmark(&id)
}

#[tauri::command]
pub(crate) fn list_timeline_bookmarks(state: State<'_, AppState>) -> Vec<fleet_timeline::TimelineEvent> {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).list_bookmarks()
}

#[tauri::command]
pub(crate) fn correlate_timeline_events(state: State<'_, AppState>, event_id: String, related_id: String) -> bool {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).correlate_events(&event_id, &related_id)
}

#[tauri::command]
pub(crate) fn get_related_timeline_events(state: State<'_, AppState>, event_id: String) -> Vec<fleet_timeline::TimelineEvent> {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).get_related_events(&event_id)
}

#[tauri::command]
pub(crate) fn get_timeline_stats(state: State<'_, AppState>) -> fleet_timeline::TimelineStats {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).get_stats()
}

#[tauri::command]
pub(crate) fn clear_timeline(state: State<'_, AppState>) {
    state.fleet.fleet_timeline_store.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

// ── Fleet Device Groups (Epic AU) ────────────────

#[tauri::command]
pub(crate) fn list_groups(state: State<'_, AppState>) -> Vec<fleet_groups::DeviceGroup> {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).list_groups()
}

#[tauri::command]
pub(crate) fn get_group(state: State<'_, AppState>, id: String) -> Option<fleet_groups::DeviceGroup> {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).get_group(&id)
}

#[tauri::command]
pub(crate) fn create_group(state: State<'_, AppState>, group: fleet_groups::DeviceGroup) -> fleet_groups::DeviceGroup {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).create_group(group)
}

#[tauri::command]
pub(crate) fn update_group(state: State<'_, AppState>, group: fleet_groups::DeviceGroup) -> Option<fleet_groups::DeviceGroup> {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).update_group(group)
}

#[tauri::command]
pub(crate) fn delete_group(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).delete_group(&id)
}

#[tauri::command]
pub(crate) fn add_device_to_group(state: State<'_, AppState>, group_id: String, device_id: String) -> bool {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).add_device_to_group(&group_id, &device_id)
}

#[tauri::command]
pub(crate) fn remove_device_from_group(state: State<'_, AppState>, group_id: String, device_id: String) -> bool {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).remove_device_from_group(&group_id, &device_id)
}

#[tauri::command]
pub(crate) fn get_device_groups(state: State<'_, AppState>, device_id: String) -> Vec<fleet_groups::DeviceGroup> {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).get_device_groups(&device_id)
}

#[tauri::command]
pub(crate) fn get_group_members(state: State<'_, AppState>, group_id: String) -> Vec<String> {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).get_group_members(&group_id)
}

#[tauri::command]
pub(crate) fn add_devices_to_group(state: State<'_, AppState>, group_id: String, device_ids: Vec<String>) -> u32 {
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).add_devices_to_group(&group_id, &device_ids)
}

#[tauri::command]
pub(crate) fn get_all_group_metrics(state: State<'_, AppState>) -> Vec<fleet_groups::GroupMetrics> {
    let fleet = state.fleet.fleet_store.lock().unwrap_or_else(|e| e.into_inner());
    // Count completed/failed work items per device
    let devices: Vec<(String, String, u32, u32, u32, u32, u32)> = fleet.devices.iter().map(|d| {
        let completed = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id) && matches!(w.status, fleet::WorkItemStatus::Completed))
            .count() as u32;
        let failed = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id) && matches!(w.status, fleet::WorkItemStatus::Failed))
            .count() as u32;
        let total_items = fleet.work_items.iter()
            .filter(|w| w.assigned_device.as_deref() == Some(&d.id))
            .count() as u32;
        (
            d.id.clone(),
            format!("{:?}", d.status),
            d.current_workload,
            total_items,
            completed,
            failed,
            d.max_workload,
        )
    }).collect();
    drop(fleet);
    state.fleet.fleet_group_store.lock().unwrap_or_else(|e| e.into_inner()).compute_all_metrics(&devices)
}

// ── Fleet Incident Management (Epic AV) ──────────

#[tauri::command]
pub(crate) fn list_incidents(state: State<'_, AppState>, status_filter: Option<String>, severity_filter: Option<String>) -> Vec<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).list_incidents(
        status_filter.as_deref(), severity_filter.as_deref(),
    )
}

#[tauri::command]
pub(crate) fn list_active_incidents(state: State<'_, AppState>) -> Vec<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).list_active_incidents()
}

#[tauri::command]
pub(crate) fn list_resolved_incidents(state: State<'_, AppState>, limit: Option<usize>) -> Vec<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).list_resolved_incidents(limit.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn get_incident(state: State<'_, AppState>, id: String) -> Option<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).get_incident(&id)
}

#[tauri::command]
pub(crate) fn create_incident(state: State<'_, AppState>, incident: fleet_incidents::Incident) -> fleet_incidents::Incident {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).create_incident(incident)
}

#[tauri::command]
pub(crate) fn update_incident_status(state: State<'_, AppState>, id: String, status: fleet_incidents::IncidentStatus, message: String, author: String) -> Option<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).update_incident_status(&id, status, message, author)
}

#[tauri::command]
pub(crate) fn assign_incident(state: State<'_, AppState>, id: String, device_id: Option<String>, group_id: Option<String>) -> Option<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).assign_incident(&id, device_id, group_id)
}

#[tauri::command]
pub(crate) fn resolve_incident(state: State<'_, AppState>, id: String, notes: String) -> Option<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).resolve_incident(&id, notes)
}

#[tauri::command]
pub(crate) fn close_incident(state: State<'_, AppState>, id: String) -> Option<fleet_incidents::Incident> {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).close_incident(&id)
}

#[tauri::command]
pub(crate) fn delete_incident(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).delete_incident(&id)
}

#[tauri::command]
pub(crate) fn get_incident_metrics(state: State<'_, AppState>) -> fleet_incidents::IncidentMetrics {
    state.fleet.fleet_incident_store.lock().unwrap_or_else(|e| e.into_inner()).get_metrics()
}

// ── Fleet Maintenance Windows (Epic AW) ──────────

#[tauri::command]
pub(crate) fn list_maintenance_windows(state: State<'_, AppState>, status_filter: Option<String>) -> Vec<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).list_windows(status_filter.as_deref())
}

#[tauri::command]
pub(crate) fn list_upcoming_maintenance(state: State<'_, AppState>) -> Vec<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).list_upcoming()
}

#[tauri::command]
pub(crate) fn list_active_maintenance(state: State<'_, AppState>) -> Vec<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).list_active()
}

#[tauri::command]
pub(crate) fn list_completed_maintenance(state: State<'_, AppState>, limit: Option<usize>) -> Vec<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).list_completed(limit.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn get_maintenance_window(state: State<'_, AppState>, id: String) -> Option<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).get_window(&id)
}

#[tauri::command]
pub(crate) fn create_maintenance_window(state: State<'_, AppState>, window: fleet_maintenance::MaintenanceWindow) -> fleet_maintenance::MaintenanceWindow {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).create_window(window)
}

#[tauri::command]
pub(crate) fn update_maintenance_window(state: State<'_, AppState>, window: fleet_maintenance::MaintenanceWindow) -> Option<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).update_window(window)
}

#[tauri::command]
pub(crate) fn delete_maintenance_window(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).delete_window(&id)
}

#[tauri::command]
pub(crate) fn activate_maintenance_window(state: State<'_, AppState>, id: String) -> Option<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).activate_window(&id)
}

#[tauri::command]
pub(crate) fn complete_maintenance_window(state: State<'_, AppState>, id: String) -> Option<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).complete_window(&id)
}

#[tauri::command]
pub(crate) fn cancel_maintenance_window(state: State<'_, AppState>, id: String) -> Option<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).cancel_window(&id)
}

#[tauri::command]
pub(crate) fn add_maintenance_note(state: State<'_, AppState>, id: String, message: String, author: String) -> Option<fleet_maintenance::MaintenanceWindow> {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).add_note(&id, message, author)
}

#[tauri::command]
pub(crate) fn get_maintenance_summary(state: State<'_, AppState>) -> fleet_maintenance::MaintenanceSummary {
    state.fleet.fleet_maintenance_store.lock().unwrap_or_else(|e| e.into_inner()).get_summary()
}

// ── Fleet Command Center (Epic AX) ───────────────

#[tauri::command]
pub(crate) fn issue_fleet_command(state: State<'_, AppState>, cmd: fleet_commands::FleetCommand) -> fleet_commands::FleetCommand {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).issue_command(cmd)
}

#[tauri::command]
pub(crate) fn start_fleet_command(state: State<'_, AppState>, id: String) -> Option<fleet_commands::FleetCommand> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).start_command(&id)
}

#[tauri::command]
pub(crate) fn complete_fleet_command(state: State<'_, AppState>, id: String, result: String) -> Option<fleet_commands::FleetCommand> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).complete_command(&id, result)
}

#[tauri::command]
pub(crate) fn fail_fleet_command(state: State<'_, AppState>, id: String, error: String) -> Option<fleet_commands::FleetCommand> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).fail_command(&id, error)
}

#[tauri::command]
pub(crate) fn cancel_fleet_command(state: State<'_, AppState>, id: String) -> Option<fleet_commands::FleetCommand> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).cancel_command(&id)
}

#[tauri::command]
pub(crate) fn list_fleet_commands(state: State<'_, AppState>, status_filter: Option<String>, limit: Option<usize>) -> Vec<fleet_commands::FleetCommand> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).list_commands(status_filter.as_deref(), limit.unwrap_or(100))
}

#[tauri::command]
pub(crate) fn list_active_fleet_commands(state: State<'_, AppState>) -> Vec<fleet_commands::FleetCommand> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).list_active_commands()
}

#[tauri::command]
pub(crate) fn get_fleet_command(state: State<'_, AppState>, id: String) -> Option<fleet_commands::FleetCommand> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).get_command(&id)
}

#[tauri::command]
pub(crate) fn delete_fleet_command(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).delete_command(&id)
}

#[tauri::command]
pub(crate) fn list_command_templates(state: State<'_, AppState>) -> Vec<fleet_commands::CommandTemplate> {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).list_templates()
}

#[tauri::command]
pub(crate) fn create_command_template(state: State<'_, AppState>, tpl: fleet_commands::CommandTemplate) -> fleet_commands::CommandTemplate {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).create_template(tpl)
}

#[tauri::command]
pub(crate) fn delete_command_template(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).delete_template(&id)
}

#[tauri::command]
pub(crate) fn get_command_stats(state: State<'_, AppState>) -> fleet_commands::CommandStats {
    state.fleet.fleet_command_store.lock().unwrap_or_else(|e| e.into_inner()).get_stats()
}

// ── Fleet Capacity Planning (Epic AY) ────────────

#[tauri::command]
pub(crate) fn record_capacity_snapshot(state: State<'_, AppState>, snap: fleet_capacity::CapacitySnapshot) -> fleet_capacity::CapacitySnapshot {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).record_snapshot(snap)
}

#[tauri::command]
pub(crate) fn list_capacity_snapshots(state: State<'_, AppState>) -> Vec<fleet_capacity::CapacitySnapshot> {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).list_snapshots()
}

#[tauri::command]
pub(crate) fn get_device_capacity(state: State<'_, AppState>, device_id: String) -> Option<fleet_capacity::CapacitySnapshot> {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).get_device_snapshot(&device_id)
}

#[tauri::command]
pub(crate) fn list_capacity_thresholds(state: State<'_, AppState>) -> Vec<fleet_capacity::CapacityThreshold> {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).list_thresholds()
}

#[tauri::command]
pub(crate) fn create_capacity_threshold(state: State<'_, AppState>, thr: fleet_capacity::CapacityThreshold) -> fleet_capacity::CapacityThreshold {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).create_threshold(thr)
}

#[tauri::command]
pub(crate) fn update_capacity_threshold(state: State<'_, AppState>, thr: fleet_capacity::CapacityThreshold) -> Option<fleet_capacity::CapacityThreshold> {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).update_threshold(thr)
}

#[tauri::command]
pub(crate) fn toggle_capacity_threshold(state: State<'_, AppState>, id: String) -> Option<fleet_capacity::CapacityThreshold> {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_threshold(&id)
}

#[tauri::command]
pub(crate) fn delete_capacity_threshold(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).delete_threshold(&id)
}

#[tauri::command]
pub(crate) fn list_capacity_alerts(state: State<'_, AppState>, unacked_only: Option<bool>) -> Vec<fleet_capacity::CapacityAlert> {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).list_alerts(unacked_only.unwrap_or(false))
}

#[tauri::command]
pub(crate) fn acknowledge_capacity_alert(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).acknowledge_alert(&id)
}

#[tauri::command]
pub(crate) fn acknowledge_all_capacity_alerts(state: State<'_, AppState>) {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).acknowledge_all_alerts();
}

#[tauri::command]
pub(crate) fn get_capacity_summary(state: State<'_, AppState>) -> fleet_capacity::FleetCapacitySummary {
    state.fleet.fleet_capacity_store.lock().unwrap_or_else(|e| e.into_inner()).get_summary()
}

// ── Fleet Webhooks & Integrations (Epic AZ) ──────

#[tauri::command]
pub(crate) fn list_fleet_webhooks(state: State<'_, AppState>) -> Vec<fleet_webhooks::Webhook> {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).list_webhooks()
}

#[tauri::command]
pub(crate) fn get_fleet_webhook(state: State<'_, AppState>, id: String) -> Option<fleet_webhooks::Webhook> {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).get_webhook(&id)
}

#[tauri::command]
pub(crate) fn create_fleet_webhook(state: State<'_, AppState>, wh: fleet_webhooks::Webhook) -> fleet_webhooks::Webhook {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).create_webhook(wh)
}

#[tauri::command]
pub(crate) fn update_fleet_webhook(state: State<'_, AppState>, wh: fleet_webhooks::Webhook) -> Option<fleet_webhooks::Webhook> {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).update_webhook(wh)
}

#[tauri::command]
pub(crate) fn toggle_fleet_webhook(state: State<'_, AppState>, id: String) -> Option<fleet_webhooks::Webhook> {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_webhook(&id)
}

#[tauri::command]
pub(crate) fn delete_fleet_webhook(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).delete_webhook(&id)
}

#[tauri::command]
pub(crate) async fn test_fleet_webhook(state: State<'_, AppState>, id: String) -> Result<Option<fleet_webhooks::WebhookDelivery>, String> {
    // Get the webhook config without holding the lock across the HTTP call
    let wh = {
        let store = state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner());
        store.get_webhook(&id)
    };
    let wh = match wh {
        Some(w) => w,
        None => return Ok(None),
    };

    let test_payload = serde_json::json!({
        "type": "test",
        "source": "orbixcore",
        "message": "Webhook test from OrbixCore",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    let result = client
        .post(&wh.url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "OrbixCore/1.0")
        .json(&test_payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await;
    let duration = start.elapsed().as_millis() as u64;

    let (status, code, body, error) = match result {
        Ok(resp) => {
            let code = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            if code >= 200 && code < 300 {
                (fleet_webhooks::DeliveryStatus::Success, Some(code), Some(body), None)
            } else {
                (fleet_webhooks::DeliveryStatus::Failed, Some(code), Some(body), Some(format!("HTTP {}", code)))
            }
        }
        Err(e) => (fleet_webhooks::DeliveryStatus::Failed, None, None, Some(e.to_string())),
    };

    let delivery = fleet_webhooks::WebhookDelivery {
        id: format!("del-{}", uuid::Uuid::new_v4()),
        webhook_id: wh.id.clone(),
        webhook_name: wh.name.clone(),
        event_type: fleet_webhooks::WebhookEventType::All,
        status,
        response_code: code,
        response_body: body,
        attempt: 1,
        max_attempts: 1,
        error,
        payload_preview: serde_json::to_string(&test_payload).unwrap_or_default(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        duration_ms: Some(duration as u32),
    };

    // Record the result back in the store
    {
        let mut store = state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner());
        store.record_test_result(&id, delivery.clone());
    }

    Ok(Some(delivery))
}

#[tauri::command]
pub(crate) fn list_webhook_deliveries(state: State<'_, AppState>, webhook_id: Option<String>, limit: Option<usize>) -> Vec<fleet_webhooks::WebhookDelivery> {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).list_deliveries(webhook_id.as_deref(), limit.unwrap_or(100))
}

#[tauri::command]
pub(crate) fn clear_webhook_deliveries(state: State<'_, AppState>, webhook_id: Option<String>) {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).clear_deliveries(webhook_id.as_deref());
}

#[tauri::command]
pub(crate) fn get_webhook_stats(state: State<'_, AppState>) -> fleet_webhooks::WebhookStats {
    state.fleet.fleet_webhook_store.lock().unwrap_or_else(|e| e.into_inner()).get_stats()
}

// ── Fleet Runbooks (Epic BA) ─────────────────────

#[tauri::command]
pub(crate) fn list_runbooks(state: State<'_, AppState>, category: Option<String>) -> Vec<fleet_runbooks::Runbook> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).list_runbooks(category.as_deref())
}

#[tauri::command]
pub(crate) fn get_runbook(state: State<'_, AppState>, id: String) -> Option<fleet_runbooks::Runbook> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).get_runbook(&id)
}

#[tauri::command]
pub(crate) fn create_runbook(state: State<'_, AppState>, rb: fleet_runbooks::Runbook) -> fleet_runbooks::Runbook {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).create_runbook(rb)
}

#[tauri::command]
pub(crate) fn delete_runbook(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).delete_runbook(&id)
}

#[tauri::command]
pub(crate) fn list_runbook_categories(state: State<'_, AppState>) -> Vec<String> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).list_categories()
}

#[tauri::command]
pub(crate) fn start_runbook_execution(state: State<'_, AppState>, runbook_id: String, device_id: Option<String>, device_name: Option<String>) -> Option<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).start_execution(&runbook_id, device_id, device_name, "user".into())
}

#[tauri::command]
pub(crate) fn advance_runbook_step(state: State<'_, AppState>, execution_id: String, notes: Option<String>) -> Option<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).advance_step(&execution_id, notes)
}

#[tauri::command]
pub(crate) fn fail_runbook_step(state: State<'_, AppState>, execution_id: String, notes: Option<String>) -> Option<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).fail_step(&execution_id, notes)
}

#[tauri::command]
pub(crate) fn skip_runbook_step(state: State<'_, AppState>, execution_id: String) -> Option<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).skip_step(&execution_id)
}

#[tauri::command]
pub(crate) fn cancel_runbook_execution(state: State<'_, AppState>, execution_id: String) -> Option<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).cancel_execution(&execution_id)
}

#[tauri::command]
pub(crate) fn list_active_runbook_executions(state: State<'_, AppState>) -> Vec<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).list_active_executions()
}

#[tauri::command]
pub(crate) fn list_runbook_executions(state: State<'_, AppState>, limit: Option<usize>) -> Vec<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).list_executions(limit.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn get_runbook_execution(state: State<'_, AppState>, id: String) -> Option<fleet_runbooks::RunbookExecution> {
    state.fleet.fleet_runbook_store.lock().unwrap_or_else(|e| e.into_inner()).get_execution(&id)
}

// ── Fleet Environments (Epic BB) ───────────────

#[tauri::command]
pub(crate) fn list_environments(state: State<'_, AppState>) -> Vec<fleet_environments::FleetEnvironment> {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).list_environments()
}

#[tauri::command]
pub(crate) fn create_environment(state: State<'_, AppState>, env: fleet_environments::FleetEnvironment) -> fleet_environments::FleetEnvironment {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).create_environment(env)
}

#[tauri::command]
pub(crate) fn update_environment(state: State<'_, AppState>, env: fleet_environments::FleetEnvironment) -> Option<fleet_environments::FleetEnvironment> {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).update_environment(env)
}

#[tauri::command]
pub(crate) fn delete_environment(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).delete_environment(&id)
}

#[tauri::command]
pub(crate) fn assign_device_to_env(state: State<'_, AppState>, env_id: String, device_id: String) -> Option<fleet_environments::FleetEnvironment> {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).assign_device(&env_id, &device_id)
}

#[tauri::command]
pub(crate) fn remove_device_from_env(state: State<'_, AppState>, env_id: String, device_id: String) -> Option<fleet_environments::FleetEnvironment> {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).remove_device(&env_id, &device_id)
}

#[tauri::command]
pub(crate) fn list_env_variables(state: State<'_, AppState>, env_id: String) -> Vec<fleet_environments::EnvVariable> {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).list_variables(&env_id)
}

#[tauri::command]
pub(crate) fn set_env_variable(state: State<'_, AppState>, env_id: String, variable: fleet_environments::EnvVariable) -> fleet_environments::EnvVariable {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).set_variable(&env_id, variable)
}

#[tauri::command]
pub(crate) fn delete_env_variable(state: State<'_, AppState>, env_id: String, key: String) -> bool {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).delete_variable(&env_id, &key)
}

#[tauri::command]
pub(crate) fn promote_to_env(state: State<'_, AppState>, from_env_id: String, to_env_id: String, notes: String) -> Option<fleet_environments::EnvPromotion> {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).promote(&from_env_id, &to_env_id, &notes)
}

#[tauri::command]
pub(crate) fn list_promotions(state: State<'_, AppState>, limit: Option<usize>) -> Vec<fleet_environments::EnvPromotion> {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).list_promotions(limit.unwrap_or(50))
}

#[tauri::command]
pub(crate) fn get_env_summary(state: State<'_, AppState>) -> fleet_environments::EnvSummary {
    state.fleet.fleet_environment_store.lock().unwrap_or_else(|e| e.into_inner()).get_summary()
}

// ── Fleet SLA Management (Epic AS) ───────────────

#[tauri::command]
pub(crate) fn list_sla_policies(state: State<'_, AppState>) -> Vec<fleet_sla::SlaPolicy> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).list_policies()
}

#[tauri::command]
pub(crate) fn create_sla_policy(state: State<'_, AppState>, policy: fleet_sla::SlaPolicy) -> fleet_sla::SlaPolicy {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).create_policy(policy)
}

#[tauri::command]
pub(crate) fn update_sla_policy(state: State<'_, AppState>, policy: fleet_sla::SlaPolicy) -> Option<fleet_sla::SlaPolicy> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).update_policy(policy)
}

#[tauri::command]
pub(crate) fn delete_sla_policy(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).delete_policy(&id)
}

#[tauri::command]
pub(crate) fn toggle_sla_policy(state: State<'_, AppState>, id: String) -> Option<fleet_sla::SlaPolicy> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).toggle_policy(&id)
}

#[tauri::command]
pub(crate) fn start_sla_tracking(state: State<'_, AppState>, tracker: fleet_sla::SlaTracker) -> fleet_sla::SlaTracker {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).start_tracking(tracker)
}

#[tauri::command]
pub(crate) fn list_active_sla_trackers(state: State<'_, AppState>) -> Vec<fleet_sla::SlaTracker> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).list_active_trackers()
}

#[tauri::command]
pub(crate) fn list_all_sla_trackers(state: State<'_, AppState>, limit: Option<usize>) -> Vec<fleet_sla::SlaTracker> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).list_all_trackers(limit.unwrap_or(100))
}

#[tauri::command]
pub(crate) fn mark_sla_responded(state: State<'_, AppState>, tracker_id: String) -> Option<fleet_sla::SlaTracker> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).mark_responded(&tracker_id)
}

#[tauri::command]
pub(crate) fn mark_sla_resolved(state: State<'_, AppState>, tracker_id: String) -> Option<fleet_sla::SlaTracker> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).mark_resolved(&tracker_id)
}

#[tauri::command]
pub(crate) fn list_sla_breaches(state: State<'_, AppState>, unacked_only: Option<bool>) -> Vec<fleet_sla::SlaBreach> {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).list_breaches(unacked_only.unwrap_or(false))
}

#[tauri::command]
pub(crate) fn acknowledge_sla_breach(state: State<'_, AppState>, id: String) -> bool {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).acknowledge_breach(&id)
}

#[tauri::command]
pub(crate) fn acknowledge_all_sla_breaches(state: State<'_, AppState>) {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).acknowledge_all_breaches();
}

#[tauri::command]
pub(crate) fn get_sla_dashboard(state: State<'_, AppState>) -> fleet_sla::SlaDashboard {
    state.fleet.fleet_sla_store.lock().unwrap_or_else(|e| e.into_inner()).get_dashboard()
}

// ── Quick-Launch Presets (Epic 6) ──────────────

#[tauri::command]
pub(crate) fn list_presets(state: State<'_, AppState>) -> Vec<presets::WorkspacePreset> {
    state.automation.preset_store.lock().unwrap_or_else(|e| e.into_inner()).list()
}

#[tauri::command]
pub(crate) fn get_preset(state: State<'_, AppState>, id: String) -> Option<presets::WorkspacePreset> {
    state.automation.preset_store.lock().unwrap_or_else(|e| e.into_inner()).get(&id)
}


#[tauri::command]
pub(crate) fn save_preset(state: State<'_, AppState>, input: SavePresetInput) -> presets::WorkspacePreset {
    let snapshot = presets::WorkspacePreset {
        id: String::new(),
        name: String::new(),
        description: String::new(),
        icon: String::new(),
        active_tab: input.active_tab,
        active_panel: input.active_panel,
        role_id: input.role_id,
        voice_settings: input.voice_settings,
        enabled_agent_ids: input.enabled_agent_ids,
        default_agent_id: input.default_agent_id,
        selected_local_model: input.selected_local_model,
        connector_instance_ids: input.connector_instance_ids,
        is_builtin: false,
        created_at: String::new(),
        updated_at: String::new(),
    };
    state.automation.preset_store.lock().unwrap_or_else(|e| e.into_inner()).create(input.name, input.description, input.icon, snapshot)
}

#[tauri::command]
pub(crate) fn update_preset(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
    icon: Option<String>,
) -> Option<presets::WorkspacePreset> {
    state.automation.preset_store.lock().unwrap_or_else(|e| e.into_inner()).update(&id, name, description, icon)
}

#[tauri::command]
pub(crate) fn delete_preset(state: State<'_, AppState>, id: String) -> bool {
    state.automation.preset_store.lock().unwrap_or_else(|e| e.into_inner()).delete(&id)
}

#[tauri::command]
pub(crate) fn apply_preset(state: State<'_, AppState>, id: String) -> Result<presets::WorkspacePreset, String> {
    let preset = state.automation.preset_store.lock().unwrap_or_else(|e| e.into_inner()).get(&id)
        .ok_or_else(|| "Preset not found".to_string())?;

    // Apply voice settings
    {
        let mut vs = state.engine.voice_settings.lock().unwrap_or_else(|e| e.into_inner());
        vs.tts_voice = preset.voice_settings.tts_voice.clone();
        vs.tts_rate = preset.voice_settings.tts_rate as i32;
        vs.auto_speak = preset.voice_settings.auto_speak;
        vs.stt_language = preset.voice_settings.stt_language.clone();
    }

    // Apply selected local model
    *state.engine.selected_local_model.lock().unwrap_or_else(|e| e.into_inner()) = preset.selected_local_model.clone();

    // Enable/disable agents per preset
    if !preset.enabled_agent_ids.is_empty() {
        let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
        for agent in agents.iter_mut() {
            agent.enabled = preset.enabled_agent_ids.contains(&agent.id);
            agent.is_default = preset.default_agent_id.as_ref() == Some(&agent.id);
        }
        storage::save_agents(&agents).ok();
    }

    Ok(preset)
}


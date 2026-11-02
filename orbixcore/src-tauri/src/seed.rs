#![allow(unused_imports, dead_code)]

use std::collections::HashMap;
use uuid::Uuid;
use chrono::Utc;
use crate::{Agent, AgentProvider, agent_router};
use crate::workflow::{Workflow, WorkflowStep, StepType, WorkflowTrigger};
use crate::fleet::{FleetDevice, DeviceStatus};
use crate::pipeline::{ProcessBoard, BoardStage, BoardItem, ItemSource, ItemStatus};
use crate::semantic_twin::*;
use crate::event_ledger::{LedgerEvent, EventType as LedgerEventType};


fn uid() -> String { Uuid::new_v4().to_string() }
fn now() -> String { Utc::now().to_rfc3339() }

pub fn seed_agents() -> Vec<Agent> {
    Vec::new()
}

pub fn seed_workflows() -> Vec<Workflow> {
    Vec::new()
}

pub fn seed_logi_agents() -> Vec<Agent> {
    Vec::new()
}

pub fn seed_logi_workflows() -> Vec<Workflow> {
    Vec::new()
}

pub fn seed_logi_fleet_devices() -> Vec<FleetDevice> {
    Vec::new()
}

pub fn seed_logi_pipelines() -> (Vec<ProcessBoard>, Vec<BoardItem>) {
    (Vec::new(), Vec::new())
}

pub fn seed_populate_agents() -> Vec<Agent> {
    Vec::new()
}

pub fn seed_populate_workflows() -> Vec<Workflow> {
    Vec::new()
}

pub fn seed_resolve_agents() -> Vec<Agent> {
    Vec::new()
}

pub fn seed_resolve_workflows() -> Vec<Workflow> {
    Vec::new()
}

pub fn seed_twin_graph() -> (Vec<TwinEntity>, Vec<TwinRelation>) {
    (Vec::new(), Vec::new())
}

pub fn seed_twin_events(entities: &[TwinEntity]) -> Vec<LedgerEvent> {
    let _ = entities;
    Vec::new()
}

pub fn seed_twin_agents() -> Vec<Agent> {
    Vec::new()
}

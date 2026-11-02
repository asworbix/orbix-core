//! Destination commands: introspect a write target's schema and run a
//! test write. The Build UI uses describe to populate a field-picker and
//! test to prove a destination is reachable before it is wired into a
//! workflow.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

/// Read the target's column schema so the UI (and the AI mapping step)
/// know what shape it expects.
#[tauri::command]
pub(crate) async fn destination_describe(
    spec: serde_json::Value,
) -> Result<destinations::SchemaInfo, String> {
    let dest: destinations::Destination =
        serde_json::from_value(spec).map_err(|e| format!("invalid destination: {}", e))?;
    dest.describe().await
}

/// Write a small batch of rows to a destination to confirm it works.
#[tauri::command]
pub(crate) async fn destination_test_write(
    spec: serde_json::Value,
    rows: Vec<serde_json::Value>,
) -> Result<destinations::WriteOutcome, String> {
    let dest: destinations::Destination =
        serde_json::from_value(spec).map_err(|e| format!("invalid destination: {}", e))?;
    dest.write(&rows).await
}

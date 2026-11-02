//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Connector modules (Phase 1)
// ─────────────────────────────────────────────


#[tauri::command]
pub(crate) fn connector_modules_summary() -> Result<ConnectorModulesSummary, String> {
    let slugs = connector_modules::list_module_slugs();
    Ok(ConnectorModulesSummary {
        count: slugs.len(),
        slugs,
        dir: connector_modules::modules_dir().to_string_lossy().into_owned(),
    })
}


// ─────────────────────────────────────────────
//  Commands - Connector vault (Phase 4)
//
//  Module connectors authenticate by writing the OAuth result here,
//  keyed by slug. Tokens flow into the connector-agent's env at spawn
//  time and never return across the bus. The frontend gets an
//  ExpiryHint with no token bytes.
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) fn connector_vault_list_keys(slug: String) -> Result<Vec<String>, String> {
    Ok(connector_vault::list_keys(&slug))
}

#[tauri::command]
pub(crate) fn connector_vault_delete_slug(slug: String) -> Result<(), String> {
    connector_vault::delete_slug(&slug);
    Ok(())
}

#[tauri::command]
pub(crate) fn connector_scan_staging(staging_dir: String) -> Result<connector_scanner::ScanReport, String> {
    use std::path::Path;
    let staging = Path::new(&staging_dir);
    let manifest = connector_manifest::load_manifest(staging)?;
    connector_scanner::scan_module(&staging.join("module.ts"), &manifest)
}

#[tauri::command]
pub(crate) fn connector_activate_module(staging_dir: String) -> Result<connector_activate::ActivationReport, String> {
    use std::path::Path;
    let report = connector_activate::activate_from_staging(Path::new(&staging_dir))?;
    Ok(report)
}

#[tauri::command]
pub(crate) fn connector_signer_key_id() -> Result<String, String> {
    connector_signer::key_id()
}


// ─────────────────────────────────────────────
//  Commands - Bob's authoring loop (CONNECTORS.md §8)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn bob_fetch_spec(
    vendor: String,
    url: String,
) -> Result<bob_connector_author::SpecFetch, String> {
    bob_connector_author::fetch_spec(&vendor, &url).await
}

#[tauri::command]
pub(crate) async fn bob_author_connector(
    draft: bob_connector_author::DraftInputs,
) -> Result<bob_connector_author::AuthorReport, String> {
    bob_connector_author::author_connector(draft).await
}

#[tauri::command]
pub(crate) async fn bob_smoke_test(slug: String) -> Result<bob_connector_author::SmokeReport, String> {
    Ok(bob_connector_author::smoke_test(&slug).await)
}

// Phase 6 - browser-attach session probe. Returns matching tabs only;
// the user's other tabs never surface to the caller.
#[tauri::command]
pub(crate) async fn connector_browser_session_probe(
    staging_dir: String,
) -> Result<browser_attach::SessionProbe, String> {
    use std::path::Path;
    let manifest = connector_manifest::load_manifest(Path::new(&staging_dir))?;
    browser_attach::session_probe(&manifest).await
}

/// Run the full OAuth dance and land the token in the slug vault.
/// Same UX as start_oauth_login but writes via the connector vault,
/// not the legacy instance-keyed credential store. The returned hint
/// carries an expiry timestamp and whether a refresh_token was issued
/// - never the token bytes.
#[tauri::command]
pub(crate) async fn start_oauth_for_module(
    slug: String,
    app: tauri::AppHandle,
) -> Result<connector_vault::ExpiryHint, String> {
    crate::event_bus::emit_ui(&app, "oauth-status", serde_json::json!({
        "connector_id": &slug,
        "status": "starting",
    })).ok();

    // The existing oauth::start_oauth_flow expects the slug to match
    // one of its built-in OAuthConfig entries (linkedin, google, slack,
    // github, microsoft). Module slugs follow the same naming so this
    // works for the bundled reference modules. Arbitrary slugs without
    // a built-in config get a clear "OAuth not supported" error.
    let tokens = oauth::start_oauth_flow(&slug).await?;

    let hint = connector_vault::put_oauth_bundle(
        &slug,
        &tokens.access_token,
        tokens.refresh_token.as_deref(),
        tokens.expires_in,
    )?;

    crate::event_bus::emit_ui(&app, "oauth-status", serde_json::json!({
        "connector_id": &slug,
        "status": "complete",
        "has_refresh_token": hint.has_refresh_token,
        "expires_at_unix": hint.expires_at_unix,
    })).ok();

    Ok(hint)
}

#[tauri::command]
pub(crate) fn connector_complete_oauth_for_module(
    slug: String,
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
) -> Result<connector_vault::ExpiryHint, String> {
    // Tokens enter the vault here and never leave. Returning the
    // ExpiryHint only - never the values - is part of the trust-line
    // contract from CONNECTORS.md §2.
    connector_vault::put_oauth_bundle(
        &slug,
        &access_token,
        refresh_token.as_deref(),
        expires_in,
    )
}


// ─────────────────────────────────────────────
//  Commands - Connector bus (Phase 3)
// ─────────────────────────────────────────────

#[tauri::command]
pub(crate) async fn connector_invoke(
    connector_slug: String,
    action_id: String,
    inputs: Option<serde_json::Value>,
    idempotency_key: Option<String>,
    caller: Option<String>,
    state: State<'_, AppState>,
) -> Result<connector_bus::ConnectorResult, String> {
    let job = connector_bus::ConnectorJob {
        connector_slug,
        action_id,
        inputs: inputs.unwrap_or(serde_json::Value::Null),
        // If the caller does not supply a key, mint one. They lose
        // idempotency guarantees but the bus still validates and audits.
        idempotency_key: idempotency_key.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
        caller: caller.unwrap_or_else(|| "frontend".to_string()),
    };
    Ok(state.connect.connector_bus.dispatch(job).await)
}


/// Resolve the bundled connector-module staging root (resources/modules)
/// via the Tauri resource resolver (W5.7). Replaces the wizard's hardcoded
/// developer path. In dev this lands in the project tree; in a packaged app
/// it resolves to Contents/Resources/resources/modules. Returns an empty
/// string if resolution fails, so the wizard falls back to a manual path.
#[tauri::command]
pub(crate) fn connector_staging_root(app: tauri::AppHandle) -> String {
    use tauri::Manager;
    let resolver = app.path();
    resolver
        .resolve("resources/modules", tauri::path::BaseDirectory::Resource)
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Generate a connector from a natural-language request with the local
/// model, then scan/sign/activate it (W5.3). Closes the self-building loop.
#[tauri::command]
pub(crate) async fn bob_generate_connector(
    request: String,
    vendor: Option<String>,
    spec: Option<String>,
) -> Result<bob_connector_author::AuthorReport, String> {
    bob_connector_author::generate_and_author(
        &request,
        vendor.as_deref().unwrap_or("Unknown"),
        spec.as_deref(),
    ).await
}

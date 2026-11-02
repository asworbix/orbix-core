//! Local endpoint commands (FABLE B1): the Settings toggle that exposes
//! the OpenAI-compatible /v1 routes (and /handoff) with a stable token.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

#[derive(serde::Serialize)]
pub(crate) struct EndpointInfo {
    pub enabled: bool,
    pub token: String,
    pub base_url: String,
    /// Same endpoint on this machine's LAN address, for OTHER devices on
    /// the network to connect to (None if the IP can't be determined).
    /// The server already binds 0.0.0.0, so this is reachable as-is.
    pub lan_url: Option<String>,
}

fn info(s: &endpoint::EndpointSettings) -> EndpointInfo {
    EndpointInfo {
        enabled: s.enabled,
        token: s.token.clone(),
        base_url: endpoint::EndpointSettings::base_url(),
        lan_url: crate::discovery::get_local_ip().map(|ip| format!("http://{}:51231/v1", ip)),
    }
}

#[tauri::command]
pub(crate) fn endpoint_get() -> Result<EndpointInfo, String> {
    Ok(info(&endpoint::EndpointSettings::load()))
}

#[tauri::command]
pub(crate) fn endpoint_set(enabled: bool) -> Result<EndpointInfo, String> {
    let mut s = endpoint::EndpointSettings::load();
    s.enabled = enabled;
    if enabled {
        s.ensure_token();
    }
    s.save()?;
    // Advertise (or stop advertising) this host on the LAN so other devices
    // can find it with `orbix connect`. Metadata only - never the token.
    let model = crate::storage::load_json::<String>("selected_model.json").ok();
    crate::discovery::set_advertising(enabled, model.as_deref());
    Ok(info(&s))
}

/// Mint a fresh token; every external tool must be updated with it.
#[tauri::command]
pub(crate) fn endpoint_rotate_token() -> Result<EndpointInfo, String> {
    let mut s = endpoint::EndpointSettings::load();
    s.token = String::new();
    s.ensure_token();
    s.save()?;
    Ok(info(&s))
}

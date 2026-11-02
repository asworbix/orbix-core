//! Commands for pair mode (peer.rs + pair_drag.rs).

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

#[tauri::command]
pub(crate) fn peer_list() -> Result<Vec<peer::Peer>, String> {
    Ok(peer::list())
}

/// Pair by name (the by-voice / by-chat path; the drag handle uses
/// peer_begin_drag). Runs the ladder and announces like the drag does.
#[tauri::command]
pub(crate) async fn peer_pair_app(name: String, app: tauri::AppHandle) -> Result<peer::Peer, String> {
    let identity = peer::identify_name(&name).await?;
    let for_learning = identity.clone();
    let app2 = app.clone();
    let progress = move |t: &str| peer::emit_event(&app2, "connecting", t, None);
    let p = peer::pair(identity, Some(&app), peer::PairProgress(&progress)).await?;
    peer::emit_event(&app, "connected", &format!("{} connected.", p.app), Some(&p));
    peer::learn_in_background(&app, &p, &for_learning);
    Ok(p)
}

#[tauri::command]
pub(crate) async fn peer_unpair(name: String, app: tauri::AppHandle) -> Result<(), String> {
    let p = peer::unpair(&name)?;
    peer::emit_event(&app, "unpaired", &format!("{} unpaired.", p.app), Some(&p));
    Ok(())
}

#[tauri::command]
pub(crate) async fn peer_send(name: String, text: String) -> Result<peer::SendReport, String> {
    peer::send(&name, &text).await
}

#[tauri::command]
pub(crate) async fn peer_read(name: String) -> Result<peer::ReadReport, String> {
    peer::read(&name).await
}

#[tauri::command]
pub(crate) async fn peer_follow_up(name: String, app: tauri::AppHandle) -> Result<peer::Peer, String> {
    let p = peer::follow_up(&name, Some(&app)).await?;
    peer::emit_event(&app, "updated", &format!("{} is ready.", p.app), Some(&p));
    Ok(p)
}

#[tauri::command]
pub(crate) async fn peer_refresh(name: String, app: tauri::AppHandle) -> Result<peer::Peer, String> {
    let p = peer::refresh(&name, Some(&app)).await?;
    peer::emit_event(&app, "updated", &format!("{} checked.", p.app), Some(&p));
    Ok(p)
}

#[tauri::command]
pub(crate) fn peer_begin_drag(app: tauri::AppHandle, origin_x: Option<f64>, origin_y: Option<f64>) -> Result<(), String> {
    let origin = match (origin_x, origin_y) {
        (Some(x), Some(y)) => Some((x, y)),
        _ => None,
    };
    pair_drag::begin(app, origin)
}

#[tauri::command]
pub(crate) fn peer_cancel_drag() -> Result<(), String> {
    pair_drag::cancel();
    Ok(())
}

// ── Steps waiting on the user (user_actions.rs) ──

#[tauri::command]
pub(crate) fn user_actions_list() -> Result<Vec<user_actions::UserAction>, String> {
    Ok(user_actions::list())
}

#[tauri::command]
pub(crate) async fn user_action_run(id: String, app: tauri::AppHandle) -> Result<String, String> {
    user_actions::run(&id, &app).await
}

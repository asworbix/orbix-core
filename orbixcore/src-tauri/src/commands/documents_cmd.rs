//! Generated-document commands: list, preview (Quick Look), open,
//! reveal, regenerate from an edited spec, delete. All file actions are
//! restricted to the Orbix Documents output directory.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

fn guard_generated(path: &str) -> Result<(), String> {
    if docgen::is_generated_path(path) {
        Ok(())
    } else {
        Err("Not a generated document path".into())
    }
}

#[tauri::command]
pub(crate) async fn list_generated_documents() -> Result<Vec<docgen::GeneratedDoc>, String> {
    Ok(docgen::registry_list())
}

/// Native macOS Quick Look - the nicest zero-dependency preview for
/// docx/pptx/html alike.
#[tauri::command]
pub(crate) async fn preview_generated_document(path: String) -> Result<(), String> {
    guard_generated(&path)?;
    std::process::Command::new("qlmanage")
        .arg("-p")
        .arg(&path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Quick Look failed: {}", e))?;
    Ok(())
}

#[tauri::command]
pub(crate) async fn open_generated_document(path: String) -> Result<(), String> {
    guard_generated(&path)?;
    open::that(&path).map_err(|e| format!("Open failed: {}", e))
}

#[tauri::command]
pub(crate) async fn reveal_generated_document(path: String) -> Result<(), String> {
    guard_generated(&path)?;
    std::process::Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Reveal failed: {}", e))?;
    Ok(())
}

/// Regenerate from an edited spec - same registry entry, fresh file,
/// any format (edit once, export as Word AND slides). The previous file
/// stays on disk so manual edits made in Office are never clobbered.
#[tauri::command]
pub(crate) async fn regenerate_document(
    id: String,
    kind: String,
    spec: docgen::DocSpec,
    state: State<'_, AppState>,
) -> Result<docgen::GeneratedDoc, String> {
    let kit = brand::active_kit(&state);
    let path = docgen::generate(&kind, &spec, &kit)?;
    // generate() inserted a fresh record; fold it into the edited entry so
    // the list keeps one row per logical document.
    let fresh = docgen::registry_list()
        .into_iter()
        .find(|d| d.path == path.display().to_string());
    if let Some(f) = fresh {
        docgen::registry_remove(&f.id);
    }
    let updated = docgen::GeneratedDoc {
        id: id.clone(),
        kind,
        title: spec.title.clone(),
        path: path.display().to_string(),
        spec,
        kit_name: kit.name,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    docgen::registry_upsert(updated.clone());
    Ok(updated)
}

#[tauri::command]
pub(crate) async fn delete_generated_document(id: String, delete_file: bool) -> Result<(), String> {
    if let Some(doc) = docgen::registry_remove(&id) {
        if delete_file && docgen::is_generated_path(&doc.path) {
            // A website's path is its index.html; the document is the whole
            // site folder. Remove the folder, guarded to a strict child of
            // the output dir so a registry entry can never nuke the root.
            let parent = std::path::Path::new(&doc.path).parent();
            if doc.kind == "website"
                && parent.map(|p| p.starts_with(docgen::output_dir()) && p != docgen::output_dir()).unwrap_or(false)
            {
                std::fs::remove_dir_all(parent.unwrap()).ok();
            } else {
                std::fs::remove_file(&doc.path).ok();
            }
        }
    }
    Ok(())
}

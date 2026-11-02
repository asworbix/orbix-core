// ─────────────────────────────────────────────
//  Turn scope - who asked this Bob turn, and from where
//
//  A Bob turn started outside the window (the `bob` CLI in a terminal,
//  later the hotkey bar or voice) carries two things the tools need
//  without threading them through every executor signature:
//
//  - workdir: the caller's folder. Shell commands run there and relative
//    file paths resolve there, ahead of the Workspace setting.
//  - steps: a sink for the same progress labels the chat ribbon shows,
//    so the caller can stream them.
//  - origin: where the turn came from ("Terminal", "Quick bar", a paired
//    app). The window only learns of a user message it did not type
//    itself through this, so the chat shows every turn, whoever asked.
//
//  Same task_local pattern as shift_monitor::STEP_SCOPE. The Bob turn
//  awaits its tools in-task (nothing in bob_send spawns), so the scope
//  reaches every executor.
// ─────────────────────────────────────────────

use std::path::PathBuf;

#[derive(Clone, Default)]
pub struct TurnScope {
    pub workdir: Option<PathBuf>,
    pub steps: Option<tokio::sync::mpsc::UnboundedSender<String>>,
    pub origin: Option<String>,
}

tokio::task_local! {
    static TURN_SCOPE: TurnScope;
}

/// Run `fut` with `scope` visible to every tool it awaits.
pub async fn scoped<F: std::future::Future>(scope: TurnScope, fut: F) -> F::Output {
    TURN_SCOPE.scope(scope, fut).await
}

/// The caller's folder for this turn, when it names an existing directory.
pub fn workdir() -> Option<PathBuf> {
    TURN_SCOPE
        .try_with(|s| s.workdir.clone())
        .ok()
        .flatten()
        .filter(|p| p.is_dir())
}

/// Where this turn came from, when it did not start in the window.
pub fn origin() -> Option<String> {
    TURN_SCOPE.try_with(|s| s.origin.clone()).ok().flatten()
}

/// Forward a progress label to the caller, if one is listening. A caller
/// that went away is not an error: the turn finishes regardless.
pub fn step(label: &str) {
    let _ = TURN_SCOPE.try_with(|s| {
        if let Some(tx) = &s.steps {
            let _ = tx.send(label.to_string());
        }
    });
}

/// Resolve a tool's path argument: `~` expands to home, an absolute path
/// stays as is, and a relative one resolves against this turn's folder
/// (then the Workspace setting). Without either it is left relative, as
/// before.
pub fn resolve_path(raw: &str) -> PathBuf {
    let expanded = if let Some(rest) = raw.strip_prefix("~/") {
        dirs::home_dir().map(|h| h.join(rest)).unwrap_or_else(|| PathBuf::from(raw))
    } else if raw == "~" {
        dirs::home_dir().unwrap_or_else(|| PathBuf::from(raw))
    } else {
        PathBuf::from(raw)
    };
    if expanded.is_absolute() {
        return expanded;
    }
    match base_dir() {
        Some(base) => base.join(expanded),
        None => expanded,
    }
}

fn base_dir() -> Option<PathBuf> {
    workdir().or_else(|| crate::workspace::WorkspaceSettings::load().usable_path())
}

/// The per-turn prompt line naming the caller's folder, or empty. The
/// last sentence is measured (2026-10-21, Gemma 4 E4B, fresh history,
/// n=6): "what's in this folder?" was answered without any tool call 3/6
/// times without it, 1/6 with it.
pub fn prompt_line() -> String {
    match workdir() {
        Some(dir) => format!(
            "\nWORKING FOLDER FOR THIS TURN: {} (the user's terminal is here). Shell commands run here and relative file paths resolve here. You have not looked inside it yet: answer anything about it (files, contents, git) from a file or shell tool call, never from memory.\n",
            dir.display()
        ),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn relative_paths_resolve_against_the_turn_folder() {
        let dir = std::env::temp_dir();
        let scope = TurnScope { workdir: Some(dir.clone()), steps: None, origin: None };
        let got = scoped(scope, async { resolve_path("notes/a.txt") }).await;
        assert_eq!(got, dir.join("notes/a.txt"));
    }

    #[tokio::test]
    async fn absolute_and_home_paths_ignore_the_turn_folder() {
        let scope = TurnScope { workdir: Some(std::env::temp_dir()), steps: None, origin: None };
        let (abs, home) = scoped(scope, async { (resolve_path("/etc/hosts"), resolve_path("~/x")) }).await;
        assert_eq!(abs, PathBuf::from("/etc/hosts"));
        assert_eq!(home, dirs::home_dir().unwrap().join("x"));
    }

    #[tokio::test]
    async fn a_missing_folder_is_not_a_workdir() {
        let scope = TurnScope { workdir: Some(PathBuf::from("/no/such/dir/orbix")), steps: None, origin: None };
        assert!(scoped(scope, async { workdir() }).await.is_none());
    }

    #[tokio::test]
    async fn steps_reach_the_caller() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let scope = TurnScope { workdir: None, steps: Some(tx), origin: None };
        scoped(scope, async { step("Reading main.rs…") }).await;
        assert_eq!(rx.recv().await.as_deref(), Some("Reading main.rs…"));
    }
}

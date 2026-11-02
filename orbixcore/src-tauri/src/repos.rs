// ─────────────────────────────────────────────
//  Git repo discovery - the inventory behind /repo
//
//  Scans the user's workspace plus the usual project roots for git
//  repositories so the chat can list them instantly and bind actions
//  (hire a code agent, direct Bob's file/shell tools) to one.
//
//  Lean by design: no `git` subprocess per repo. The branch comes from
//  reading .git/HEAD, recency from the HEAD file's mtime - both one
//  cheap stat/read. Depth-capped walk, hidden and dependency dirs
//  skipped, result capped.
// ─────────────────────────────────────────────

use serde::Serialize;
use std::path::{Path, PathBuf};

const MAX_REPOS: usize = 200;
const SCAN_DEPTH: usize = 2;

#[derive(Clone, Serialize, Debug)]
pub struct RepoInfo {
    /// Directory name, the human handle in the palette.
    pub name: String,
    pub path: String,
    /// Current branch, or a short commit hash when detached.
    pub branch: String,
    /// RFC3339 of the last HEAD change - cheap "last worked on".
    pub last_activity: Option<String>,
}

/// Roots worth scanning: the Orbix workspace first, then the common
/// project directories that actually exist under $HOME.
fn scan_roots(workspace: Option<PathBuf>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(ws) = workspace {
        roots.push(ws);
    }
    if let Some(home) = dirs::home_dir() {
        for candidate in [
            "Documents/GitHub",
            "GitHub",
            "Projects",
            "projects",
            "Developer",
            "Code",
            "code",
            "dev",
            "src",
            "repos",
        ] {
            let p = home.join(candidate);
            if p.is_dir() {
                roots.push(p);
            }
        }
    }
    roots.dedup();
    roots
}

fn head_branch(git_dir: &Path) -> (String, Option<String>) {
    let head = git_dir.join("HEAD");
    let last_activity = std::fs::metadata(&head)
        .and_then(|m| m.modified())
        .ok()
        .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
    let branch = std::fs::read_to_string(&head)
        .ok()
        .map(|s| {
            let s = s.trim();
            match s.strip_prefix("ref: refs/heads/") {
                Some(b) => b.to_string(),
                None => format!("detached @ {}", s.chars().take(8).collect::<String>()),
            }
        })
        .unwrap_or_else(|| "unknown".to_string());
    (branch, last_activity)
}

fn visit(dir: &Path, depth: usize, out: &mut Vec<RepoInfo>) {
    if out.len() >= MAX_REPOS {
        return;
    }
    let git_dir = dir.join(".git");
    if git_dir.is_dir() {
        let (branch, last_activity) = head_branch(&git_dir);
        out.push(RepoInfo {
            name: dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("repo")
                .to_string(),
            path: dir.display().to_string(),
            branch,
            last_activity,
        });
        // A repo's subdirectories are its working tree, not more repos
        // (submodules are the parent's business).
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }
        visit(&path, depth - 1, out);
        if out.len() >= MAX_REPOS {
            return;
        }
    }
}

/// Scan for git repos, most recently active first.
pub fn scan(workspace: Option<PathBuf>) -> Vec<RepoInfo> {
    let mut out: Vec<RepoInfo> = Vec::new();
    for root in scan_roots(workspace) {
        visit(&root, SCAN_DEPTH, &mut out);
    }
    // A root nested inside another (workspace under ~/Projects) can
    // yield duplicates - keep the first sighting of each path.
    let mut seen = std::collections::HashSet::new();
    out.retain(|r| seen.insert(r.path.clone()));
    out.sort_by(|a, b| b.last_activity.cmp(&a.last_activity));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_repo(dir: &Path, branch: &str) {
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), format!("ref: refs/heads/{}\n", branch)).unwrap();
    }

    #[test]
    fn finds_repos_at_depth_and_reads_branch() {
        let tmp = tempfile::tempdir().unwrap();
        fake_repo(&tmp.path().join("alpha"), "main");
        fake_repo(&tmp.path().join("org/beta"), "develop");
        std::fs::create_dir_all(tmp.path().join("node_modules/ignored/.git")).unwrap();
        let mut out = Vec::new();
        visit(tmp.path(), SCAN_DEPTH, &mut out);
        let names: Vec<&str> = out.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        assert_eq!(out.len(), 2, "node_modules must be skipped: {:?}", names);
        let alpha = out.iter().find(|r| r.name == "alpha").unwrap();
        assert_eq!(alpha.branch, "main");
        assert!(alpha.last_activity.is_some());
    }

    #[test]
    fn detached_head_reads_as_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("detached");
        let git = dir.join(".git");
        std::fs::create_dir_all(&git).unwrap();
        std::fs::write(git.join("HEAD"), "0123456789abcdef\n").unwrap();
        let mut out = Vec::new();
        visit(tmp.path(), SCAN_DEPTH, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].branch, "detached @ 01234567");
    }

    #[test]
    fn repo_working_tree_is_not_rescanned() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("outer");
        fake_repo(&repo, "main");
        // A vendored repo inside the working tree must not be listed.
        fake_repo(&repo.join("vendor/inner"), "main");
        let mut out = Vec::new();
        visit(tmp.path(), SCAN_DEPTH, &mut out);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "outer");
    }
}

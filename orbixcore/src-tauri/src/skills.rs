// ─────────────────────────────────────────────
//  Skills - user-extensible instructions for Bob
//
//  A skill is a markdown file (.md or .skill) in the workspace's
//  skills/ folder: frontmatter with a name and description, then the
//  instructions Bob follows when the skill is used. The system prompt
//  carries only the compact index (name + description); the body loads
//  on demand through the use_skill action, so fifty skills cost a few
//  hundred prompt tokens, not fifty pages.
//
//  Skills live under the active workspace profile, so a Work workspace
//  and a Private one naturally carry different skill sets. This is how
//  users turn Orbix into whatever assistant they want.
// ─────────────────────────────────────────────

use std::path::PathBuf;

const MAX_SKILLS: usize = 50;
const MAX_BODY_CHARS: usize = 12_000;

#[derive(Clone, Debug, serde::Serialize)]
pub struct SkillMeta {
    pub name: String,
    pub description: String,
    pub path: String,
}

pub fn skills_dir() -> PathBuf {
    crate::storage::data_dir().join("skills")
}

/// Parse "---\nname: x\ndescription: y\n---\nbody" loosely. Files
/// without frontmatter still work: filename stem becomes the name, the
/// first non-empty line the description.
pub(crate) fn parse_skill(file_stem: &str, content: &str) -> (String, String, String) {
    let trimmed = content.trim_start();
    if let Some(rest) = trimmed.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let front = &rest[..end];
            let body = rest[end + 4..].trim_start_matches(['-', '\n']).trim().to_string();
            let field = |key: &str| -> Option<String> {
                front.lines().find_map(|l| {
                    let l = l.trim();
                    l.strip_prefix(key)
                        .and_then(|r| r.strip_prefix(':'))
                        .map(|v| v.trim().trim_matches('"').to_string())
                        .filter(|v| !v.is_empty())
                })
            };
            let name = field("name").unwrap_or_else(|| file_stem.to_string());
            let description = field("description").unwrap_or_else(|| {
                body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string()
            });
            return (name, description, body);
        }
    }
    let body = trimmed.trim().to_string();
    let description = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
    (file_stem.to_string(), description, body)
}

/// All installed skills, alphabetical, capped.
pub fn list() -> Vec<SkillMeta> {
    let dir = skills_dir();
    let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut out: Vec<SkillMeta> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "md" | "skill") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("skill").to_string();
        if stem.starts_with('_') {
            continue; // _example.md and friends stay out of the index
        }
        let Ok(content) = std::fs::read_to_string(&path) else { continue };
        let (name, description, _) = parse_skill(&stem, &content);
        out.push(SkillMeta { name, description, path: path.display().to_string() });
        if out.len() >= MAX_SKILLS {
            break;
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// A skill's instruction body by (case-insensitive) name.
pub fn load(name: &str) -> Option<String> {
    let dir = skills_dir();
    let entries = std::fs::read_dir(&dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext, "md" | "skill") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let content = std::fs::read_to_string(&path).ok()?;
        let (skill_name, _, body) = parse_skill(stem, &content);
        if skill_name.eq_ignore_ascii_case(name.trim()) || stem.eq_ignore_ascii_case(name.trim()) {
            let mut body = body;
            if body.len() > MAX_BODY_CHARS {
                let mut end = MAX_BODY_CHARS;
                while !body.is_char_boundary(end) {
                    end -= 1;
                }
                body.truncate(end);
                body.push_str("\n[skill truncated]");
            }
            return Some(body);
        }
    }
    None
}

/// The compact index for the system prompt; empty string when the user
/// has no skills, so the prompt costs nothing by default. Ensures the
/// folder exists with a format example on first touch.
pub fn prompt_index() -> String {
    let dir = skills_dir();
    if !dir.exists() {
        if std::fs::create_dir_all(&dir).is_ok() {
            let example = "---\nname: example-skill\ndescription: Rename this file (drop the underscore) and describe when Bob should use it\n---\n\nInstructions Bob follows when this skill is used. Write them like you\nwould brief a sharp colleague: what to do, in what order, what good\noutput looks like. Files starting with _ are ignored.\n";
            std::fs::write(dir.join("_example.md"), example).ok();
        }
    }
    let skills = list();
    if skills.is_empty() {
        return format!(
            "=== SKILLS ===\nNo skills installed yet. The user can extend you by dropping .md or .skill files (frontmatter name + description, then instructions) into {} - an _example.md there shows the format. Mention this if they ask about skills or extending Orbix.\n\n",
            dir.display()
        );
    }
    let mut out = String::from("=== SKILLS (user-installed) ===\n");
    for s in &skills {
        out.push_str(&format!("- {}: {}\n", s.name, s.description));
    }
    out.push_str(
        "When a request matches a skill, FIRST call use_skill with that skill's name, then follow its instructions for the rest of the task. The user can reference one directly as [skill: name].\n\n",
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_parses_and_body_survives() {
        let (name, desc, body) = parse_skill(
            "meeting-notes",
            "---\nname: minutes\ndescription: Raw notes to structured minutes\n---\n\nAlways produce: attendees, decisions, actions.",
        );
        assert_eq!(name, "minutes");
        assert_eq!(desc, "Raw notes to structured minutes");
        assert!(body.starts_with("Always produce"));
    }

    #[test]
    fn bare_markdown_still_works() {
        let (name, desc, body) = parse_skill("sales-tone", "Write like a calm senior seller.\nNo hype.");
        assert_eq!(name, "sales-tone");
        assert_eq!(desc, "Write like a calm senior seller.");
        assert!(body.contains("No hype."));
    }
}

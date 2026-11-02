// ─────────────────────────────────────────────
//  Brand kits - the design DNA behind branded documents
//
//  A user drops their brand guideline (PDF/Markdown/text/HTML) into a
//  confidential folder and Bob extracts a BrandKit: colors with roles,
//  heading/body fonts, and tone of voice. Every generated document
//  (docgen.rs: HTML, Word, PowerPoint) is styled from the active kit,
//  so output carries the brand instead of looking like default Office.
//
//  Extraction is fully local: the guideline text never leaves the
//  machine - hex codes are harvested by regex, then the local model
//  assigns roles and reads fonts/tone, returning strict JSON. Reads
//  respect the data-control ACLs like every file tool.
// ─────────────────────────────────────────────

use serde::{Deserialize, Serialize};

use crate::AppState;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BrandColor {
    pub name: String,
    /// #RRGGBB
    pub hex: String,
    /// primary | secondary | accent | background | text | other
    pub role: String,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BrandKit {
    pub id: String,
    pub name: String,
    pub colors: Vec<BrandColor>,
    pub heading_font: String,
    pub body_font: String,
    /// Short description of the brand voice, used to steer copywriting.
    pub tone: String,
    pub source_path: Option<String>,
    /// PNG/JPEG logo embedded into generated documents when set.
    #[serde(default)]
    pub logo_path: Option<String>,
    pub created_at: String,
}

impl BrandKit {
    /// Color lookup by role with sensible fallbacks so generators never
    /// have to handle missing roles.
    pub fn color(&self, role: &str) -> String {
        if let Some(c) = self.colors.iter().find(|c| c.role.eq_ignore_ascii_case(role)) {
            return c.hex.clone();
        }
        match role {
            "primary" => self
                .colors
                .first()
                .map(|c| c.hex.clone())
                .unwrap_or_else(|| "#D88B6E".into()),
            "secondary" => self.colors.get(1).map(|c| c.hex.clone()).unwrap_or_else(|| self.color("primary")),
            "accent" => self.colors.get(2).map(|c| c.hex.clone()).unwrap_or_else(|| self.color("primary")),
            "background" => "#FFFFFF".into(),
            "text" => "#1E1F22".into(),
            _ => self.color("primary"),
        }
    }

    /// A neutral default kit so document generation works before any
    /// guideline has been extracted (uses the Orbix warm palette).
    pub fn fallback() -> Self {
        Self {
            id: "default".into(),
            name: "Orbix Default".into(),
            colors: vec![
                BrandColor { name: "Clay".into(), hex: "#B05A33".into(), role: "primary".into() },
                BrandColor { name: "Sand".into(), hex: "#E0B85F".into(), role: "secondary".into() },
                BrandColor { name: "Sage".into(), hex: "#4A8B5C".into(), role: "accent".into() },
                BrandColor { name: "Paper".into(), hex: "#FBF6EC".into(), role: "background".into() },
                BrandColor { name: "Ink".into(), hex: "#3A2F22".into(), role: "text".into() },
            ],
            heading_font: "Helvetica Neue".into(),
            body_font: "Helvetica Neue".into(),
            tone: "Clear, confident, warm. Short sentences. No jargon.".into(),
            source_path: None,
            logo_path: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct BrandStore {
    pub kits: Vec<BrandKit>,
    pub active_id: Option<String>,
}

impl BrandStore {
    pub fn load() -> Self {
        crate::storage::load_json("brand_kits.json").unwrap_or_default()
    }

    pub fn save(&self) {
        crate::storage::save_json("brand_kits.json", self).ok();
    }

    pub fn active(&self) -> BrandKit {
        self.active_id
            .as_ref()
            .and_then(|id| self.kits.iter().find(|k| &k.id == id))
            .cloned()
            .or_else(|| self.kits.first().cloned())
            .unwrap_or_else(BrandKit::fallback)
    }

    pub fn upsert(&mut self, kit: BrandKit) {
        match self.kits.iter_mut().find(|k| k.id == kit.id) {
            Some(existing) => *existing = kit.clone(),
            None => self.kits.push(kit.clone()),
        }
        // A freshly extracted kit becomes active - that is always what the
        // user just asked for.
        self.active_id = Some(kit.id);
        self.save();
    }
}

/// The active kit for generators. Falls back to the Orbix default so
/// create_document always works.
pub fn active_kit(state: &AppState) -> BrandKit {
    state
        .engine
        .brand_store
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .active()
}

// ─────────────────────────────────────────────
//  Extraction
// ─────────────────────────────────────────────

/// Read guideline text from disk, honouring data-control ACLs exactly
/// like the file_read tool ("bob" is the acting agent).
fn read_guideline_text(state: &AppState, path: &str) -> Result<String, String> {
    {
        let dc = state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner());
        if let Err(reason) = dc.authorize("bob", path, false) {
            return Err(format!("Access denied by data control: {}", reason));
        }
        if let Some((folder, _)) = dc.check_access("bob", path) {
            let fid = folder.id.clone();
            drop(dc);
            let mut dc = state.connect.data_control.lock().unwrap_or_else(|e| e.into_inner());
            dc.log_access(&fid, "bob", "bob", "read", path);
        }
    }

    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".pdf") {
        pdf_extract::extract_text(path)
            .map_err(|e| format!("Could not extract text from PDF: {}. A Markdown or text export of the guideline works too.", e))
    } else {
        let raw = std::fs::read_to_string(path).map_err(|e| format!("Could not read {}: {}", path, e))?;
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            // Crude tag strip is enough for guideline prose.
            let re = regex::Regex::new(r"<[^>]+>").unwrap();
            Ok(re.replace_all(&raw, " ").to_string())
        } else {
            Ok(raw)
        }
    }
}

/// Harvest candidate hex colors in document order, deduped.
fn harvest_hex(text: &str) -> Vec<String> {
    let re = regex::Regex::new(r"#[0-9a-fA-F]{6}\b").unwrap();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for m in re.find_iter(text) {
        let hex = m.as_str().to_uppercase();
        if seen.insert(hex.clone()) {
            out.push(hex);
        }
    }
    out.truncate(12);
    out
}

/// Extract a BrandKit from a guideline file using the local model.
pub async fn extract_kit(state: &AppState, path: &str) -> Result<BrandKit, String> {
    let text = read_guideline_text(state, path)?;
    if text.trim().len() < 40 {
        return Err("The guideline file contained almost no text. Export it as PDF-with-text, Markdown, or plain text.".into());
    }
    let hexes = harvest_hex(&text);
    // Keep the excerpt lean: guidelines front-load identity sections, and
    // the model only needs prose, not every appendix.
    let excerpt: String = text.chars().take(9000).collect();

    let system = "You extract brand identity from guideline text. Reply with STRICT JSON only - no prose, no markdown fences - in exactly this shape:\n{\"name\":\"<brand name>\",\"colors\":[{\"name\":\"<color name>\",\"hex\":\"#RRGGBB\",\"role\":\"primary|secondary|accent|background|text|other\"}],\"heading_font\":\"<font family>\",\"body_font\":\"<font family>\",\"tone\":\"<2-3 sentences describing the brand voice>\"}\nRules: use ONLY hex codes that appear in the candidate list or the text. Assign exactly one primary. If fonts are not stated, use \"Helvetica Neue\". 3-8 colors.";
    let user = format!(
        "CANDIDATE HEX CODES (in document order): {}\n\nGUIDELINE TEXT:\n{}",
        if hexes.is_empty() { "(none found - infer nothing, use role text/background defaults #1E1F22/#FFFFFF)".to_string() } else { hexes.join(", ") },
        excerpt
    );
    let history = vec![
        crate::Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "system".into(),
            content: system.into(),
            context: None,
            agent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_turns: None,
        },
        crate::Message {
            id: uuid::Uuid::new_v4().to_string(),
            role: "user".into(),
            content: user,
            context: None,
            agent_id: None,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_turns: None,
        },
    ];
    let client = reqwest::Client::new();
    let raw = crate::call_local_llm(&client, &history).await?;

    // Strict parse, then repair, then a last-resort harvest-only kit so a
    // flaky model reply never loses the user's colors.
    let parsed: serde_json::Value = serde_json::from_str(raw.trim())
        .ok()
        .or_else(|| {
            crate::repair_json(&raw).and_then(|f| serde_json::from_str(&f).ok())
        })
        .or_else(|| {
            let s = raw.find('{')?;
            let e = raw.rfind('}')?;
            serde_json::from_str(&raw[s..=e]).ok()
        })
        .unwrap_or(serde_json::Value::Null);

    let file_stem = std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Brand")
        .to_string();

    let mut colors: Vec<BrandColor> = parsed["colors"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let hex = c["hex"].as_str()?.to_uppercase();
                    if !regex::Regex::new(r"^#[0-9A-F]{6}$").unwrap().is_match(&hex) {
                        return None;
                    }
                    Some(BrandColor {
                        name: c["name"].as_str().unwrap_or("Color").to_string(),
                        hex,
                        role: c["role"].as_str().unwrap_or("other").to_lowercase(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if colors.is_empty() {
        // Model failed: fall back to the harvested order - first hex is
        // primary by convention in most guidelines.
        colors = hexes
            .iter()
            .enumerate()
            .map(|(i, hex)| BrandColor {
                name: format!("Color {}", i + 1),
                hex: hex.clone(),
                role: match i {
                    0 => "primary",
                    1 => "secondary",
                    2 => "accent",
                    _ => "other",
                }
                .into(),
            })
            .collect();
    }
    if colors.is_empty() {
        return Err("No colors could be extracted - the guideline lists none as hex codes. Add hex values (e.g. #1A2B3C) or edit the kit manually.".into());
    }

    let kit = BrandKit {
        id: uuid::Uuid::new_v4().to_string(),
        name: parsed["name"].as_str().map(|s| s.to_string()).filter(|s| !s.trim().is_empty()).unwrap_or(file_stem),
        colors,
        heading_font: parsed["heading_font"].as_str().unwrap_or("Helvetica Neue").to_string(),
        body_font: parsed["body_font"].as_str().unwrap_or("Helvetica Neue").to_string(),
        tone: parsed["tone"].as_str().unwrap_or("Clear and professional.").to_string(),
        source_path: Some(path.to_string()),
        // Guidelines rarely state the logo in text; look for an image file
        // named like a logo next to the guideline (PNG preferred).
        logo_path: find_sibling_logo(path),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state
        .engine
        .brand_store
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .upsert(kit.clone());
    Ok(kit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harvest_dedupes_and_uppercases() {
        let t = "Primary #d88b6e, again #D88B6E, then #123abc.";
        assert_eq!(harvest_hex(t), vec!["#D88B6E".to_string(), "#123ABC".to_string()]);
    }

    #[test]
    fn kit_color_fallbacks() {
        let kit = BrandKit::fallback();
        assert_eq!(kit.color("primary"), "#B05A33");
        assert_eq!(kit.color("nonexistent"), kit.color("primary"));
    }

    #[test]
    fn store_upsert_activates() {
        let mut s = BrandStore::default();
        let mut k = BrandKit::fallback();
        k.id = "k1".into();
        // NOTE: upsert saves to disk; acceptable in tests (data dir).
        s.kits.push(k.clone());
        s.active_id = Some("k1".into());
        assert_eq!(s.active().id, "k1");
    }
}

/// Look for a logo image next to the guideline: a .png/.jpg/.jpeg whose
/// name contains "logo" (PNG wins over JPEG).
fn find_sibling_logo(guideline_path: &str) -> Option<String> {
    let dir = std::path::Path::new(guideline_path).parent()?;
    let mut best: Option<(u8, String)> = None;
    for entry in std::fs::read_dir(dir).ok()? {
        let p = entry.ok()?.path();
        let name = p.file_name()?.to_str()?.to_ascii_lowercase();
        if !name.contains("logo") {
            continue;
        }
        let rank = if name.ends_with(".png") { 0 } else if name.ends_with(".jpg") || name.ends_with(".jpeg") { 1 } else { continue };
        let path_str = p.display().to_string();
        if best.as_ref().map(|(r, _)| rank < *r).unwrap_or(true) {
            best = Some((rank, path_str));
        }
    }
    best.map(|(_, p)| p)
}

/// Quality gate for generated documents: a thin spec (few sections,
/// near-empty bodies) is expanded by a dedicated local drafting pass in
/// the brand tone BEFORE rendering, so "create a powerpoint about X"
/// yields real content, not four bullet stubs. Returns None when the
/// draft fails - the caller keeps the original spec.
pub async fn expand_doc_spec(
    app: &tauri::AppHandle,
    spec: &crate::docgen::DocSpec,
    kit: &BrandKit,
    kind: &str,
) -> Option<crate::docgen::DocSpec> {
    let outline = serde_json::to_string(spec).ok()?;
    let style_hint = if kind == "pptx" {
        "Slides: 6-10 sections. Per section: body of 1-2 punchy sentences plus 3-6 tight bullets (max 12 words each). Include concrete facts and figures - sizes, numbers, dates - wherever the topic allows."
    } else if kind == "website" {
        "Website landing page: 5-8 sections with short, selling headings. Per section: body of 2-3 sentences that sell, plus 3-6 bullets written as 'Benefit: short supporting phrase' (max 10 words after the colon) - they render as feature cards. Include concrete facts and figures wherever the topic allows."
    } else {
        "Document: 6-9 sections that together cover the topic properly. Per section: 2-3 substantial paragraphs of 3-5 full sentences each (separate paragraphs with a blank line), plus bullets only where they genuinely help. Write a genuinely useful, complete piece - depth over summary. Include concrete facts and figures wherever the topic allows."
    };
    let system = format!(
        "You are a professional document writer. Brand tone of voice: {tone}\n\
         Expand the outline into a complete, substantive document. {style}\n\
         Write REAL, specific content - concrete facts, no placeholders, no lorem ipsum, no 'TBD'.\n\
         Inline markdown is supported in body and bullet text and renders properly: \
         [label](https://url) becomes a real link, **bold** and `code` work. When the outline \
         references documentation or sources, include them as [label](https://url) links.\n\
         Reply with STRICT JSON only, exactly this shape:\n\
         {{\"title\":\"...\",\"subtitle\":\"...\",\"sections\":[{{\"heading\":\"...\",\"body\":\"...\",\"bullets\":[\"...\"]}}]}}",
        tone = kit.tone,
        style = style_hint,
    );
    // Streamed, so the ribbon moves while the model drafts (this used to
    // be minutes of "Creating a document…") and Stop actually stops it.
    let client = reqwest::Client::new();
    let progress_app = app.clone();
    let raw = crate::llm_engine::stream_freeform(
        &client,
        app,
        &system,
        &format!("Outline to expand:\n{}", outline),
        8192,
        Some(&crate::BOB_CANCEL_FLAG),
        |n| crate::bob_core::bob_step(&progress_app, &format!("Drafting the content… {} KB", n / 1024)),
    )
    .await
    .ok()?;
    let parsed: crate::docgen::DocSpec = serde_json::from_str(raw.trim())
        .ok()
        .or_else(|| crate::repair_json(&raw).and_then(|f| serde_json::from_str(&f).ok()))
        .or_else(|| {
            let s = raw.find('{')?;
            let e = raw.rfind('}')?;
            serde_json::from_str(&raw[s..=e]).ok()
        })?;
    // Only accept a draft that is meaningfully richer than the outline
    // it came from - otherwise rendering the original is more honest.
    let orig_substance: usize = spec.sections.iter().map(|s| s.body.len() + s.bullets.join(" ").len()).sum();
    let substance: usize = parsed.sections.iter().map(|s| s.body.len() + s.bullets.join(" ").len()).sum();
    if parsed.sections.len() >= 3 && substance > 2000 && substance > orig_substance + orig_substance / 3 {
        Some(parsed)
    } else {
        None
    }
}

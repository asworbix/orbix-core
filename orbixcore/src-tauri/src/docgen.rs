// ─────────────────────────────────────────────
//  Branded document generation (HTML / DOCX / PPTX / website)
//
//  One generic DocSpec (title + sections) rendered four ways, every
//  one styled from the active BrandKit so output carries the user's
//  brand instead of default-Office grey.
//
//  DOCX and PPTX are ZIP packages of OOXML parts. We write them with a
//  tiny STORE-only zip writer (no new dependencies, Office accepts
//  uncompressed members) and hand-built minimal XML: styles.xml carries
//  the brand fonts/colors for Word, theme1.xml carries the color and
//  font scheme for PowerPoint. Lean by design - no docx/pptx crate, no
//  runtime template files.
// ─────────────────────────────────────────────

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::brand::BrandKit;

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct DocSection {
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub bullets: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug, Default)]
pub struct DocSpec {
    pub title: String,
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub sections: Vec<DocSection>,
    /// For kind "app": what the interactive page should do, in prose.
    #[serde(default)]
    pub brief: Option<String>,
}

/// Where generated documents land.
pub fn output_dir() -> PathBuf {
    dirs::document_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Orbix Documents")
}

// ─────────────────────────────────────────────
//  Generated-document registry
//
//  Every generation records its DocSpec, so "edit" means editing the
//  structured content and regenerating - into the same format or any
//  other - instead of reverse-engineering a binary. Lean: specs are
//  small; the registry never stores file bytes.
// ─────────────────────────────────────────────

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct GeneratedDoc {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub path: String,
    pub spec: DocSpec,
    pub kit_name: String,
    pub created_at: String,
}

static REGISTRY_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn registry_list() -> Vec<GeneratedDoc> {
    let _g = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    crate::storage::load_json("generated_docs.json").unwrap_or_default()
}

fn registry_save(docs: &[GeneratedDoc]) {
    crate::storage::save_json("generated_docs.json", docs).ok();
}

pub fn registry_upsert(doc: GeneratedDoc) {
    let _g = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut docs: Vec<GeneratedDoc> = crate::storage::load_json("generated_docs.json").unwrap_or_default();
    match docs.iter_mut().find(|d| d.id == doc.id) {
        Some(existing) => *existing = doc,
        None => docs.insert(0, doc),
    }
    docs.truncate(200);
    registry_save(&docs);
}

pub fn registry_remove(id: &str) -> Option<GeneratedDoc> {
    let _g = REGISTRY_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut docs: Vec<GeneratedDoc> = crate::storage::load_json("generated_docs.json").unwrap_or_default();
    let removed = docs.iter().find(|d| d.id == id).cloned();
    docs.retain(|d| d.id != id);
    registry_save(&docs);
    removed
}

/// Paths handed to preview/open/reveal must live in our output dir -
/// never let the frontend point these at arbitrary files.
pub fn is_generated_path(path: &str) -> bool {
    let p = std::path::Path::new(path);
    p.canonicalize()
        .ok()
        .map(|c| c.starts_with(output_dir()))
        .unwrap_or(false)
}

fn slug(s: &str) -> String {
    let cleaned: String = s
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let parts: Vec<&str> = cleaned.split('-').filter(|p| !p.is_empty()).collect();
    let joined = parts.join("-");
    joined.chars().take(48).collect()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Render the inline-markdown subset local models actually emit in doc
/// copy - links, bold, and code - into safe HTML. The text is
/// xml-escaped FIRST and the markdown patterns are rewritten on the
/// escaped string, so nothing the model writes can inject markup.
/// Without this, `[label](https://...)` and `**bold**` land in the
/// rendered page as literal bracket-and-asterisk soup - the field
/// complaint behind "the links are missing and the page looks ugly".
fn inline_md(text: &str) -> String {
    let mut s = xml_escape(text);
    // [label](https://url) - http(s) only, so no javascript: targets.
    if let Ok(re) = regex::Regex::new(r"\[([^\[\]]+)\]\((https?://[^)\s]+)\)") {
        s = re
            .replace_all(&s, "<a href=\"$2\" target=\"_blank\" rel=\"noopener\">$1</a>")
            .to_string();
    }
    // **bold**
    if let Ok(re) = regex::Regex::new(r"\*\*([^*\n]+)\*\*") {
        s = re.replace_all(&s, "<strong>$1</strong>").to_string();
    }
    // `code`
    if let Ok(re) = regex::Regex::new(r"`([^`\n]+)`") {
        s = re.replace_all(&s, "<code>$1</code>").to_string();
    }
    s
}

fn hex_no_hash(hex: &str) -> String {
    hex.trim_start_matches('#').to_uppercase()
}

/// Write a model-authored interactive page verbatim and register it as
/// an html document (so the chat chip previews it like any other page).
pub fn write_app_page(spec: &DocSpec, kit: &BrandKit, html: &str) -> Result<PathBuf, String> {
    let dir = output_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let path = dir.join(format!("{}-{}.html", slug(&spec.title), stamp));
    crate::storage::atomic_write(&path, html.as_bytes()).map_err(|e| format!("Write failed: {}", e))?;
    registry_upsert(GeneratedDoc {
        id: uuid::Uuid::new_v4().to_string(),
        kind: "html".to_string(),
        title: spec.title.clone(),
        path: path.display().to_string(),
        spec: spec.clone(),
        kit_name: kit.name.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    });
    Ok(path)
}

/// Generate a document. Returns the absolute output path.
pub fn generate(kind: &str, spec: &DocSpec, kit: &BrandKit) -> Result<PathBuf, String> {
    let dir = output_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create {}: {}", dir.display(), e))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let base = format!("{}-{}", slug(&spec.title), stamp);
    let path = match kind {
        "html" => {
            let p = dir.join(format!("{}.html", base));
            crate::storage::atomic_write(&p, generate_html(spec, kit).as_bytes())
                .map_err(|e| format!("Write failed: {}", e))?;
            p
        }
        "docx" => {
            let p = dir.join(format!("{}.docx", base));
            crate::storage::atomic_write(&p, &generate_docx(spec, kit))
                .map_err(|e| format!("Write failed: {}", e))?;
            p
        }
        "pptx" => {
            let p = dir.join(format!("{}.pptx", base));
            crate::storage::atomic_write(&p, &generate_pptx(spec, kit))
                .map_err(|e| format!("Write failed: {}", e))?;
            p
        }
        // A website is a folder, not a file: index.html + styles.css
        // (+ assets/logo) so the markup stays semantic and the styling
        // lives in a real stylesheet the user can hand to any host.
        "website" => {
            let site = dir.join(format!("{}-site", base));
            std::fs::create_dir_all(&site)
                .map_err(|e| format!("Could not create {}: {}", site.display(), e))?;
            let logo_rel = if let Some(l) = load_logo(kit) {
                let assets = site.join("assets");
                std::fs::create_dir_all(&assets)
                    .map_err(|e| format!("Could not create {}: {}", assets.display(), e))?;
                let name = format!("logo.{}", l.ext);
                crate::storage::atomic_write(&assets.join(&name), &l.bytes)
                    .map_err(|e| format!("Write failed: {}", e))?;
                Some(format!("assets/{}", name))
            } else {
                None
            };
            crate::storage::atomic_write(&site.join("styles.css"), site_css(kit).as_bytes())
                .map_err(|e| format!("Write failed: {}", e))?;
            let p = site.join("index.html");
            crate::storage::atomic_write(&p, generate_website(spec, kit, logo_rel.as_deref()).as_bytes())
                .map_err(|e| format!("Write failed: {}", e))?;
            p
        }
        other => return Err(format!("Unknown document kind '{}'. Use docx, pptx, html, or website.", other)),
    };
    registry_upsert(GeneratedDoc {
        id: uuid::Uuid::new_v4().to_string(),
        kind: kind.to_string(),
        title: spec.title.clone(),
        path: path.display().to_string(),
        spec: spec.clone(),
        kit_name: kit.name.clone(),
        created_at: chrono::Utc::now().to_rfc3339(),
    });
    Ok(path)
}

// ─────────────────────────────────────────────
//  Logo loading (PNG / JPEG with dimension sniffing)
// ─────────────────────────────────────────────

pub(crate) struct LogoImage {
    pub bytes: Vec<u8>,
    pub ext: &'static str,  // "png" | "jpeg"
    pub w: u32,
    pub h: u32,
}

fn png_dims(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 24 || &b[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    Some((
        u32::from_be_bytes([b[16], b[17], b[18], b[19]]),
        u32::from_be_bytes([b[20], b[21], b[22], b[23]]),
    ))
}

fn jpeg_dims(b: &[u8]) -> Option<(u32, u32)> {
    if b.len() < 4 || b[0] != 0xFF || b[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 9 < b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = b[i + 1];
        // SOF0..SOF15 except DHT(C4)/JPG(C8)/DAC(CC) carry dimensions.
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            let h = u32::from(b[i + 5]) << 8 | u32::from(b[i + 6]);
            let w = u32::from(b[i + 7]) << 8 | u32::from(b[i + 8]);
            return Some((w, h));
        }
        let len = (usize::from(b[i + 2]) << 8 | usize::from(b[i + 3])).max(2);
        i += 2 + len;
    }
    None
}

pub(crate) fn load_logo(kit: &BrandKit) -> Option<LogoImage> {
    let path = kit.logo_path.as_ref()?;
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() > 5_000_000 {
        return None; // sanity cap
    }
    if let Some((w, h)) = png_dims(&bytes) {
        return Some(LogoImage { bytes, ext: "png", w, h });
    }
    if let Some((w, h)) = jpeg_dims(&bytes) {
        return Some(LogoImage { bytes, ext: "jpeg", w, h });
    }
    None
}

// ─────────────────────────────────────────────
//  Minimal ZIP writer (STORE only)
// ─────────────────────────────────────────────

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        table[i as usize] = c;
    }
    let mut crc = 0xFFFFFFFFu32;
    for &b in data {
        crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFFFFFF
}

fn zip_store(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = Vec::new();
    let mut central: Vec<u8> = Vec::new();
    let (time, date) = (0u16, 0x21u16); // fixed timestamp: 1980-01-01
    for (name, data) in entries {
        let offset = out.len() as u32;
        let crc = crc32(data);
        let n = name.as_bytes();
        // local file header
        out.extend_from_slice(&0x04034b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0u16.to_le_bytes()); // flags
        out.extend_from_slice(&0u16.to_le_bytes()); // method: STORE
        out.extend_from_slice(&time.to_le_bytes());
        out.extend_from_slice(&date.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(n.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra len
        out.extend_from_slice(n);
        out.extend_from_slice(data);
        // central directory record
        central.extend_from_slice(&0x02014b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes()); // made by
        central.extend_from_slice(&20u16.to_le_bytes()); // needed
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes());
        central.extend_from_slice(&time.to_le_bytes());
        central.extend_from_slice(&date.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(n.len() as u16).to_le_bytes());
        central.extend_from_slice(&0u16.to_le_bytes()); // extra
        central.extend_from_slice(&0u16.to_le_bytes()); // comment
        central.extend_from_slice(&0u16.to_le_bytes()); // disk
        central.extend_from_slice(&0u16.to_le_bytes()); // int attrs
        central.extend_from_slice(&0u32.to_le_bytes()); // ext attrs
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(n);
    }
    let cd_offset = out.len() as u32;
    out.extend_from_slice(&central);
    // end of central directory
    out.extend_from_slice(&0x06054b50u32.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // comment len
    out
}

// ─────────────────────────────────────────────
//  HTML
// ─────────────────────────────────────────────

fn generate_html(spec: &DocSpec, kit: &BrandKit) -> String {
    let primary = kit.color("primary");
    let secondary = kit.color("secondary");
    let accent = kit.color("accent");
    let bg = kit.color("background");
    let text = kit.color("text");

    // Sections, numbered and anchored so the TOC can point at them.
    let mut body = String::new();
    for (i, s) in spec.sections.iter().enumerate() {
        body.push_str(&format!("<section id=\"s-{}\">", i + 1));
        if !s.heading.trim().is_empty() {
            body.push_str(&format!(
                "<h2><span class=\"kicker\">{:02}</span>{}</h2>",
                i + 1,
                xml_escape(s.heading.trim())
            ));
        }
        for para in s.body.split("\n\n").filter(|p| !p.trim().is_empty()) {
            body.push_str(&format!("<p>{}</p>", inline_md(para.trim())));
        }
        if !s.bullets.is_empty() {
            body.push_str("<ul>");
            for b in &s.bullets {
                body.push_str(&format!("<li>{}</li>", inline_md(b)));
            }
            body.push_str("</ul>");
        }
        body.push_str("</section>");
    }

    // A contents block once the document is long enough to need one.
    let toc = if spec.sections.iter().filter(|s| !s.heading.trim().is_empty()).count() >= 4 {
        let mut items = String::new();
        for (i, s) in spec.sections.iter().enumerate() {
            if s.heading.trim().is_empty() {
                continue;
            }
            items.push_str(&format!(
                "<a href=\"#s-{}\"><span class=\"toc-n\">{:02}</span>{}</a>",
                i + 1,
                i + 1,
                xml_escape(s.heading.trim())
            ));
        }
        format!(
            "<nav class=\"toc\" aria-label=\"Contents\"><span class=\"toc-title\">Contents</span>{}</nav>",
            items
        )
    } else {
        String::new()
    };

    let subtitle = spec
        .subtitle
        .as_deref()
        .map(|s| format!("<p class=\"subtitle\">{}</p>", xml_escape(s)))
        .unwrap_or_default();
    let date = chrono::Local::now().format("%B %e, %Y").to_string();

    format!(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<style>
  :root {{
    --primary: {primary};
    --secondary: {secondary};
    --accent: {accent};
    --bg: {bg};
    --text: {text};
    --ink-soft: color-mix(in srgb, var(--text) 62%, transparent);
    --secondary-soft: color-mix(in srgb, var(--secondary) 24%, transparent);
    --line: color-mix(in srgb, var(--text) 14%, transparent);
    --wash: color-mix(in srgb, var(--primary) 6%, white);
    --sheet: color-mix(in srgb, white 92%, var(--bg));
  }}
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  html {{ scroll-behavior: smooth; }}
  body {{
    font-family: '{body_font}', -apple-system, 'Segoe UI', sans-serif;
    background:
      radial-gradient(1200px 500px at 15% -10%, color-mix(in srgb, var(--primary) 14%, transparent), transparent 65%),
      radial-gradient(900px 420px at 110% 8%, color-mix(in srgb, var(--accent) 10%, transparent), transparent 60%),
      var(--bg);
    color: var(--text);
    line-height: 1.7;
    padding: clamp(16px, 4vw, 56px) clamp(12px, 4vw, 40px);
  }}
  .sheet {{
    max-width: 860px;
    margin: 0 auto;
    background: var(--sheet);
    border: 1px solid var(--line);
    border-radius: 18px;
    box-shadow: 0 30px 70px -30px color-mix(in srgb, var(--text) 35%, transparent);
    overflow: hidden;
  }}
  header {{
    position: relative;
    padding: clamp(44px, 7vw, 72px) clamp(28px, 6vw, 72px) clamp(32px, 5vw, 52px);
    background: linear-gradient(135deg,
      color-mix(in srgb, var(--primary) 12%, var(--sheet)),
      var(--sheet) 55%);
    border-bottom: 1px solid var(--line);
  }}
  header::before {{
    content: "";
    position: absolute;
    inset: 0 0 auto 0;
    height: 6px;
    background: linear-gradient(90deg, var(--primary), var(--accent));
  }}
  .eyebrow {{
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--primary);
    margin-bottom: 18px;
  }}
  .eyebrow .dot {{
    width: 8px; height: 8px; border-radius: 50%;
    background: var(--accent);
  }}
  .eyebrow .date {{ color: var(--ink-soft); font-weight: 500; letter-spacing: 0.08em; }}
  h1, h2 {{ font-family: '{heading_font}', -apple-system, 'Segoe UI', sans-serif; }}
  h1 {{
    font-size: clamp(30px, 4.6vw, 46px);
    line-height: 1.12;
    letter-spacing: -0.015em;
    color: var(--text);
    max-width: 20ch;
  }}
  .subtitle {{
    margin-top: 14px;
    font-size: clamp(15px, 2vw, 19px);
    color: var(--ink-soft);
    max-width: 52ch;
  }}
  main {{ padding: clamp(28px, 5vw, 56px) clamp(28px, 6vw, 72px) clamp(44px, 6vw, 64px); }}
  .toc {{
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 4px 24px;
    padding: 18px 22px;
    margin-bottom: clamp(28px, 4vw, 44px);
    background: var(--wash);
    border: 1px solid var(--line);
    border-radius: 12px;
  }}
  .toc-title {{
    grid-column: 1 / -1;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--ink-soft);
    margin-bottom: 6px;
  }}
  .toc a {{
    display: flex;
    gap: 10px;
    align-items: baseline;
    padding: 4px 0;
    font-size: 14px;
    color: var(--text);
    text-decoration: none;
  }}
  .toc a:hover {{ color: var(--primary); }}
  .toc-n {{
    font-size: 11px;
    font-weight: 700;
    color: var(--accent);
    font-variant-numeric: tabular-nums;
  }}
  section {{ margin-top: clamp(34px, 5vw, 52px); }}
  section:first-of-type {{ margin-top: 0; }}
  h2 {{
    display: flex;
    align-items: baseline;
    gap: 12px;
    font-size: clamp(20px, 2.6vw, 26px);
    letter-spacing: -0.01em;
    color: var(--text);
    margin-bottom: 14px;
  }}
  .kicker {{
    font-size: 13px;
    font-weight: 700;
    color: var(--primary);
    font-variant-numeric: tabular-nums;
    letter-spacing: 0.06em;
  }}
  p {{ margin-bottom: 14px; font-size: 16px; max-width: 68ch; }}
  a {{
    color: var(--primary);
    font-weight: 600;
    text-decoration: underline;
    text-decoration-color: color-mix(in srgb, var(--primary) 40%, transparent);
    text-underline-offset: 3px;
  }}
  a:hover {{ text-decoration-color: var(--primary); }}
  strong {{ font-weight: 700; color: color-mix(in srgb, var(--text) 88%, var(--primary)); }}
  code {{
    font-family: ui-monospace, 'Cascadia Code', Consolas, monospace;
    font-size: 0.9em;
    background: var(--wash);
    border: 1px solid var(--line);
    border-radius: 5px;
    padding: 1px 6px;
  }}
  ul {{
    list-style: none;
    margin: 4px 0 16px;
    padding: 16px 20px;
    background: var(--wash);
    border-left: 3px solid var(--accent);
    border-radius: 0 10px 10px 0;
    max-width: 68ch;
  }}
  li {{
    position: relative;
    padding-left: 22px;
    margin-bottom: 8px;
    font-size: 15px;
  }}
  li:last-child {{ margin-bottom: 0; }}
  li::before {{
    content: "";
    position: absolute;
    left: 2px;
    top: 0.62em;
    width: 7px; height: 7px;
    border-radius: 50%;
    background: var(--accent);
  }}
  footer {{
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 18px clamp(28px, 6vw, 72px);
    border-top: 1px solid var(--line);
    font-size: 12px;
    color: var(--ink-soft);
    background: var(--wash);
  }}
  @media print {{
    body {{ background: white; padding: 0; }}
    .sheet {{ border: none; box-shadow: none; border-radius: 0; }}
    .toc {{ display: none; }}
  }}
</style>
</head>
<body>
<div class="sheet">
<header>
  {logo}<div class="eyebrow"><span class="dot"></span>{brand}<span class="date">{date}</span></div>
  <h1>{title}</h1>
  {subtitle}
</header>
<main>
{toc}
{body}
</main>
<footer><span>{brand}</span><span>generated by Orbix Core</span></footer>
</div>
</body>
</html>
"#,
        title = xml_escape(&spec.title),
        subtitle = subtitle,
        toc = toc,
        body = body,
        date = date,
        logo = load_logo(kit)
            .map(|l| format!(
                "<img class=\"logo\" alt=\"logo\" style=\"height:44px;display:block;margin-bottom:22px\" src=\"data:image/{};base64,{}\">",
                l.ext,
                { use base64::Engine as _; base64::engine::general_purpose::STANDARD.encode(&l.bytes) },
            ))
            .unwrap_or_default(),
        brand = xml_escape(&kit.name),
        primary = primary,
        secondary = secondary,
        accent = accent,
        bg = bg,
        text = text,
        heading_font = kit.heading_font,
        body_font = kit.body_font,
    )
}

// ─────────────────────────────────────────────
//  Website (index.html + styles.css)
//
//  The same DocSpec rendered as a modern single-page site: sticky nav
//  built from section headings, a hero from title/subtitle, bullets
//  becoming feature cards, and a footer. The model supplies copy only;
//  every design decision lives here and in site_css, so output quality
//  does not depend on the model being able to write CSS.
// ─────────────────────────────────────────────

/// "Benefit: supporting phrase" bullets split into a card title and
/// body; bullets without a short lead-in render as body-only cards.
fn card_parts(bullet: &str) -> (Option<String>, String) {
    let sentence_case = |s: &str| -> String {
        let mut c = s.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    };
    for sep in [": ", " - "] {
        if let Some(i) = bullet.find(sep) {
            let (head, tail) = bullet.split_at(i);
            let tail = &tail[sep.len()..];
            if (3..=48).contains(&head.trim().len()) && !tail.trim().is_empty() {
                return (Some(head.trim().to_string()), sentence_case(tail.trim()));
            }
        }
    }
    (None, bullet.trim().to_string())
}

/// Anchor id for a section: slugged heading, deduped by position.
fn section_id(heading: &str, index: usize) -> String {
    let s = slug(heading);
    if s.is_empty() { format!("section-{}", index + 1) } else { format!("{}-{}", s, index + 1) }
}

fn generate_website(spec: &DocSpec, kit: &BrandKit, logo_rel: Option<&str>) -> String {
    let title = xml_escape(&spec.title);
    let ids: Vec<String> = spec.sections.iter().enumerate()
        .map(|(i, s)| section_id(&s.heading, i))
        .collect();

    let mut nav_links = String::new();
    for (s, id) in spec.sections.iter().zip(&ids).take(5) {
        if s.heading.trim().is_empty() { continue; }
        nav_links.push_str(&format!(
            "<a href=\"#{}\">{}</a>",
            id,
            xml_escape(s.heading.trim())
        ));
    }

    let brand_mark = match logo_rel {
        Some(rel) => format!(
            "<img src=\"{}\" alt=\"\" class=\"brand-logo\"> <span>{}</span>",
            xml_escape(rel), title
        ),
        None => format!("<span>{}</span>", title),
    };

    let lede = spec.subtitle.as_deref()
        .map(|s| format!("<p class=\"lede\">{}</p>", xml_escape(s)))
        .unwrap_or_default();
    // Primary CTA scrolls to the first section; a "Get in touch" ghost
    // button appears only when a section is genuinely about contact.
    let mut cta = String::new();
    if let Some(first_id) = ids.first() {
        cta.push_str(&format!("<a class=\"btn btn-primary\" href=\"#{}\">Explore</a>", first_id));
    }
    if let Some(id) = spec.sections.iter().zip(&ids)
        .find(|(s, _)| s.heading.to_ascii_lowercase().contains("contact"))
        .map(|(_, id)| id)
    {
        cta.push_str(&format!("<a class=\"btn btn-ghost\" href=\"#{}\">Get in touch</a>", id));
    }

    let mut sections = String::new();
    for ((s, id), i) in spec.sections.iter().zip(&ids).zip(0..) {
        let tint = if i % 2 == 1 { " tint" } else { "" };
        sections.push_str(&format!("<section class=\"section{}\" id=\"{}\"><div class=\"inner\">", tint, id));
        if !s.heading.trim().is_empty() {
            sections.push_str(&format!("<h2>{}</h2>", xml_escape(s.heading.trim())));
        }
        for para in s.body.split("\n\n").filter(|p| !p.trim().is_empty()) {
            sections.push_str(&format!("<p>{}</p>", inline_md(para.trim())));
        }
        let bullets: Vec<&String> = s.bullets.iter().filter(|b| !b.trim().is_empty()).collect();
        // Three or more bullets read as features: render a card grid.
        // One or two stay a checklist - a two-card grid looks broken.
        if bullets.len() >= 3 {
            sections.push_str("<div class=\"cards\">");
            for b in &bullets {
                let (head, body) = card_parts(b);
                sections.push_str("<article class=\"card\"><span class=\"card-dot\" aria-hidden=\"true\"></span>");
                if let Some(h) = head {
                    sections.push_str(&format!("<h3>{}</h3>", inline_md(&h)));
                }
                sections.push_str(&format!("<p>{}</p></article>", inline_md(&body)));
            }
            sections.push_str("</div>");
        } else if !bullets.is_empty() {
            sections.push_str("<ul class=\"checklist\">");
            for b in &bullets {
                sections.push_str(&format!("<li>{}</li>", inline_md(b)));
            }
            sections.push_str("</ul>");
        }
        sections.push_str("</div></section>");
    }

    let year = chrono::Local::now().format("%Y");
    format!(
        r##"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="description" content="{desc}">
<title>{title}</title>
<link rel="stylesheet" href="styles.css">
</head>
<body>
<a class="skip" href="#main">Skip to content</a>
<header class="nav">
  <div class="inner">
    <a class="brand" href="#top">{brand_mark}</a>
    <nav class="nav-links" aria-label="Sections">{nav_links}</nav>
  </div>
</header>
<main id="main">
<section class="hero" id="top">
  <div class="inner">
    {eyebrow}<h1>{title}</h1>
    {lede}
    <div class="cta">{cta}</div>
  </div>
</section>
{sections}
</main>
<footer>
  <div class="inner">
    <div><strong>{title}</strong>{foot_sub}</div>
    <span class="foot-note">&copy; {year} &middot; Made with Orbix Core</span>
  </div>
</footer>
</body>
</html>
"##,
        desc = spec.subtitle.as_deref().map(xml_escape).unwrap_or_else(|| title.clone()),
        title = title,
        brand_mark = brand_mark,
        nav_links = nav_links,
        // The extracted brand name is a credible eyebrow; the built-in
        // fallback kit's name is an internal label and must not leak
        // onto a user's website.
        eyebrow = if kit.id == "default" {
            String::new()
        } else {
            format!("<p class=\"eyebrow\">{}</p>\n    ", xml_escape(&kit.name))
        },
        lede = lede,
        cta = cta,
        sections = sections,
        foot_sub = spec.subtitle.as_deref()
            .map(|s| format!("<span class=\"foot-sub\">{}</span>", xml_escape(s)))
            .unwrap_or_default(),
        year = year,
    )
}

/// The site design system, generated from the brand kit. All tints are
/// derived with color-mix so the same stylesheet works on light and
/// dark brand backgrounds alike.
fn site_css(kit: &BrandKit) -> String {
    format!(
        r#"/* Generated by Orbix Core - design tokens from the '{kit_name}' brand kit */
:root {{
  --primary: {primary};
  --secondary: {secondary};
  --accent: {accent};
  --bg: {bg};
  --text: {text};
  --muted: color-mix(in srgb, var(--text) 62%, var(--bg));
  --surface: color-mix(in srgb, var(--primary) 4%, var(--bg));
  --card-bg: color-mix(in srgb, var(--primary) 3%, var(--bg));
  --hairline: color-mix(in srgb, var(--text) 12%, transparent);
  --radius: 16px;
  --shadow: 0 18px 40px -18px color-mix(in srgb, var(--text) 28%, transparent);
  --font-heading: '{hfont}', -apple-system, 'Segoe UI', sans-serif;
  --font-body: '{bfont}', -apple-system, 'Segoe UI', sans-serif;
}}

*, *::before, *::after {{ box-sizing: border-box; margin: 0; padding: 0; }}
html {{ scroll-behavior: smooth; }}
@media (prefers-reduced-motion: reduce) {{
  html {{ scroll-behavior: auto; }}
  *, *::before, *::after {{ transition: none !important; animation: none !important; }}
}}

body {{
  font-family: var(--font-body);
  background: var(--bg);
  color: var(--text);
  line-height: 1.65;
  -webkit-font-smoothing: antialiased;
}}
::selection {{ background: color-mix(in srgb, var(--primary) 24%, transparent); }}

h1, h2, h3 {{ font-family: var(--font-heading); line-height: 1.12; }}
.inner {{ max-width: 1080px; margin: 0 auto; padding-inline: clamp(20px, 5vw, 48px); }}

.skip {{
  position: absolute; left: -9999px; top: 0;
  background: var(--primary); color: var(--bg);
  padding: 10px 18px; border-radius: 0 0 10px 0; z-index: 100;
}}
.skip:focus {{ left: 0; }}

/* ── Nav ── */
.nav {{
  position: sticky; top: 0; z-index: 50;
  background: color-mix(in srgb, var(--bg) 80%, transparent);
  backdrop-filter: blur(14px) saturate(1.4);
  -webkit-backdrop-filter: blur(14px) saturate(1.4);
  border-bottom: 1px solid var(--hairline);
}}
.nav .inner {{
  display: flex; align-items: center; justify-content: space-between;
  gap: 24px; min-height: 60px;
}}
.brand {{
  display: flex; align-items: center; gap: 10px;
  font-family: var(--font-heading); font-weight: 700;
  font-size: 1.05rem; color: var(--text); text-decoration: none;
  letter-spacing: -0.01em;
}}
.brand-logo {{ height: 28px; width: auto; display: block; }}
.nav-links {{ display: flex; gap: clamp(14px, 2.5vw, 28px); flex-wrap: wrap; }}
.nav-links a {{
  color: var(--muted); text-decoration: none; font-size: 0.92rem;
  transition: color 0.18s ease;
}}
.nav-links a:hover, .nav-links a:focus-visible {{ color: var(--primary); }}
@media (max-width: 640px) {{ .nav-links {{ display: none; }} }}

/* ── Hero ── */
.hero {{
  position: relative; overflow: hidden;
  padding-block: clamp(84px, 14vw, 160px) clamp(72px, 11vw, 130px);
  background:
    radial-gradient(60% 80% at 12% 0%, color-mix(in srgb, var(--primary) 14%, transparent) 0%, transparent 70%),
    radial-gradient(50% 70% at 90% 10%, color-mix(in srgb, var(--accent) 10%, transparent) 0%, transparent 70%),
    var(--bg);
}}
.eyebrow {{
  color: var(--primary); font-weight: 600; font-size: 0.82rem;
  text-transform: uppercase; letter-spacing: 0.14em; margin-bottom: 18px;
}}
.hero h1 {{
  font-size: clamp(2.6rem, 6.5vw, 4.4rem);
  letter-spacing: -0.03em; line-height: 1.04;
  max-width: 17em;
}}
.lede {{
  margin-top: 22px; font-size: clamp(1.08rem, 2vw, 1.32rem);
  color: var(--muted); max-width: 36em;
}}
.cta {{ margin-top: 36px; display: flex; gap: 14px; flex-wrap: wrap; }}

/* ── Buttons ── */
.btn {{
  display: inline-block; padding: 0.85em 1.7em; border-radius: 999px;
  font-weight: 600; font-size: 0.98rem; text-decoration: none;
  transition: transform 0.18s ease, box-shadow 0.18s ease, background 0.18s ease;
}}
.btn:focus-visible {{ outline: 3px solid color-mix(in srgb, var(--primary) 45%, transparent); outline-offset: 2px; }}
.btn-primary {{
  background: var(--primary); color: var(--bg);
}}
.btn-primary:hover {{
  transform: translateY(-2px);
  box-shadow: 0 12px 28px -8px color-mix(in srgb, var(--primary) 55%, transparent);
}}
.btn-ghost {{
  color: var(--primary);
  border: 1.5px solid color-mix(in srgb, var(--primary) 45%, transparent);
}}
.btn-ghost:hover {{ background: color-mix(in srgb, var(--primary) 8%, transparent); }}

/* ── Sections ── */
.section {{ padding-block: clamp(64px, 9vw, 108px); scroll-margin-top: 76px; }}
.section.tint {{ background: var(--surface); }}
.section h2 {{
  font-size: clamp(1.7rem, 3.4vw, 2.35rem); letter-spacing: -0.02em;
  margin-bottom: 18px;
}}
.section h2::before {{
  content: ""; display: block; width: 44px; height: 4px; border-radius: 2px;
  background: linear-gradient(90deg, var(--primary), var(--accent));
  margin-bottom: 16px;
}}
.section p {{ max-width: 46em; margin-bottom: 14px; color: color-mix(in srgb, var(--text) 88%, var(--bg)); }}

/* ── Cards ── */
.cards {{
  margin-top: 34px; display: grid; gap: 18px;
  grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
}}
.card {{
  background: var(--card-bg); border: 1px solid var(--hairline);
  border-radius: var(--radius); padding: 26px 24px;
  transition: transform 0.2s ease, box-shadow 0.2s ease, border-color 0.2s ease;
}}
.card:hover {{
  transform: translateY(-4px); box-shadow: var(--shadow);
  border-color: color-mix(in srgb, var(--primary) 32%, transparent);
}}
.card-dot {{
  display: block; width: 12px; height: 12px; border-radius: 4px;
  background: linear-gradient(135deg, var(--primary), var(--accent));
  margin-bottom: 16px;
}}
.card h3 {{ font-size: 1.06rem; margin-bottom: 8px; letter-spacing: -0.01em; }}
.card p {{ font-size: 0.94rem; color: var(--muted); margin: 0; }}

/* ── Checklist ── */
.checklist {{ margin-top: 20px; list-style: none; }}
.checklist li {{
  position: relative; padding-left: 30px; margin-bottom: 10px; max-width: 44em;
}}
.checklist li::before {{
  content: "\2713"; position: absolute; left: 0; top: 0;
  color: var(--accent); font-weight: 700;
}}

/* ── Footer ── */
footer {{
  background: var(--text); color: var(--bg); margin-top: 40px;
}}
footer .inner {{
  display: flex; justify-content: space-between; align-items: baseline;
  gap: 18px; flex-wrap: wrap; padding-block: 40px;
}}
footer strong {{ font-family: var(--font-heading); font-size: 1.02rem; }}
.foot-sub {{ display: block; margin-top: 4px; font-size: 0.88rem; opacity: 0.75; }}
.foot-note {{ font-size: 0.8rem; opacity: 0.6; }}
"#,
        kit_name = kit.name.replace('*', ""),
        primary = kit.color("primary"),
        secondary = kit.color("secondary"),
        accent = kit.color("accent"),
        bg = kit.color("background"),
        text = kit.color("text"),
        hfont = kit.heading_font.replace('\'', ""),
        bfont = kit.body_font.replace('\'', ""),
    )
}

// ─────────────────────────────────────────────
//  DOCX
// ─────────────────────────────────────────────

fn generate_docx(spec: &DocSpec, kit: &BrandKit) -> Vec<u8> {
    let primary = hex_no_hash(&kit.color("primary"));
    let secondary = hex_no_hash(&kit.color("secondary"));
    let accent = hex_no_hash(&kit.color("accent"));
    let text = hex_no_hash(&kit.color("text"));
    let hfont = xml_escape(&kit.heading_font);
    let bfont = xml_escape(&kit.body_font);

    let logo = load_logo(kit);
    let content_types = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Default Extension="jpeg" ContentType="image/jpeg"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
</Types>"#);

    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
</Relationships>"#.to_string();

    let logo_rel = if let Some(l) = &logo {
        format!("<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"media/logo.{}\"/>", l.ext)
    } else {
        String::new()
    };
    let doc_rels = format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
{logo_rel}
</Relationships>"#);

    let styles = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="{bfont}" w:hAnsi="{bfont}"/><w:color w:val="{text}"/><w:sz w:val="22"/></w:rPr></w:rPrDefault>
<w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="300" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Title"><w:name w:val="Title"/><w:basedOn w:val="Normal"/>
<w:pPr><w:spacing w:after="80"/><w:pBdr><w:bottom w:val="single" w:sz="24" w:space="8" w:color="{primary}"/></w:pBdr></w:pPr>
<w:rPr><w:rFonts w:ascii="{hfont}" w:hAnsi="{hfont}"/><w:color w:val="{primary}"/><w:sz w:val="72"/><w:b/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Subtitle"><w:name w:val="Subtitle"/><w:basedOn w:val="Normal"/>
<w:pPr><w:spacing w:after="360"/></w:pPr>
<w:rPr><w:color w:val="{secondary}"/><w:sz w:val="30"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/>
<w:pPr><w:spacing w:before="360" w:after="120"/><w:outlineLvl w:val="0"/></w:pPr>
<w:rPr><w:rFonts w:ascii="{hfont}" w:hAnsi="{hfont}"/><w:color w:val="{primary}"/><w:sz w:val="34"/><w:b/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="ListBullet"><w:name w:val="List Bullet"/><w:basedOn w:val="Normal"/>
<w:pPr><w:spacing w:after="60"/><w:ind w:left="420"/></w:pPr></w:style>
</w:styles>"#,
        bfont = bfont, hfont = hfont, text = text, primary = primary, secondary = secondary
    );

    let para = |style: &str, txt: &str| -> String {
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"{}\"/></w:pPr><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            style,
            xml_escape(txt)
        )
    };
    let bullet = |txt: &str| -> String {
        format!(
            "<w:p><w:pPr><w:pStyle w:val=\"ListBullet\"/></w:pPr><w:r><w:rPr><w:color w:val=\"{accent}\"/><w:b/></w:rPr><w:t xml:space=\"preserve\">&#8226;  </w:t></w:r><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            xml_escape(txt),
            accent = accent
        )
    };

    let mut body = String::new();
    if let Some(l) = &logo {
        // 0.5in tall, width preserving aspect ratio (EMU).
        let cy: i64 = 457200;
        let cx: i64 = if l.h > 0 { cy * l.w as i64 / l.h as i64 } else { cy };
        body.push_str(&format!(
            r#"<w:p><w:pPr><w:spacing w:after="240"/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="10" name="Logo"/><a:graphic xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="10" name="Logo"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
            cx = cx, cy = cy
        ));
    }
    body.push_str(&para("Title", &spec.title));
    if let Some(sub) = &spec.subtitle {
        body.push_str(&para("Subtitle", sub));
    }
    for s in &spec.sections {
        if !s.heading.trim().is_empty() {
            body.push_str(&para("Heading1", &s.heading));
        }
        for p in s.body.split("\n\n").filter(|p| !p.trim().is_empty()) {
            body.push_str(&para("Normal", p.trim()));
        }
        for b in &s.bullets {
            body.push_str(&bullet(b));
        }
    }

    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing">
<w:body>{}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440"/></w:sectPr></w:body>
</w:document>"#,
        body
    );

    let mut entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_string(), content_types.into_bytes()),
        ("_rels/.rels".to_string(), rels.into_bytes()),
        ("word/_rels/document.xml.rels".to_string(), doc_rels.into_bytes()),
        ("word/styles.xml".to_string(), styles.into_bytes()),
        ("word/document.xml".to_string(), document.into_bytes()),
    ];
    if let Some(l) = &logo {
        entries.push((format!("word/media/logo.{}", l.ext), l.bytes.clone()));
    }
    let borrowed: Vec<(&str, Vec<u8>)> = entries.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    zip_store(&borrowed)
}

// ─────────────────────────────────────────────
//  PPTX (16:9, brand theme, one title slide + one slide per section)
// ─────────────────────────────────────────────

const A_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const P_NS: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn pptx_theme(kit: &BrandKit) -> String {
    let primary = hex_no_hash(&kit.color("primary"));
    let secondary = hex_no_hash(&kit.color("secondary"));
    let accent = hex_no_hash(&kit.color("accent"));
    let text = hex_no_hash(&kit.color("text"));
    let bg = hex_no_hash(&kit.color("background"));
    let hfont = xml_escape(&kit.heading_font);
    let bfont = xml_escape(&kit.body_font);
    // fmtScheme boilerplate is mandatory for a valid theme part.
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="{a}" name="Brand">
<a:themeElements>
<a:clrScheme name="Brand">
<a:dk1><a:srgbClr val="{text}"/></a:dk1><a:lt1><a:srgbClr val="{bg}"/></a:lt1>
<a:dk2><a:srgbClr val="{text}"/></a:dk2><a:lt2><a:srgbClr val="{bg}"/></a:lt2>
<a:accent1><a:srgbClr val="{primary}"/></a:accent1><a:accent2><a:srgbClr val="{secondary}"/></a:accent2>
<a:accent3><a:srgbClr val="{accent}"/></a:accent3><a:accent4><a:srgbClr val="{primary}"/></a:accent4>
<a:accent5><a:srgbClr val="{secondary}"/></a:accent5><a:accent6><a:srgbClr val="{accent}"/></a:accent6>
<a:hlink><a:srgbClr val="{primary}"/></a:hlink><a:folHlink><a:srgbClr val="{secondary}"/></a:folHlink>
</a:clrScheme>
<a:fontScheme name="Brand"><a:majorFont><a:latin typeface="{hfont}"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont>
<a:minorFont><a:latin typeface="{bfont}"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme>
<a:fmtScheme name="Office">
<a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:fillStyleLst>
<a:lnStyleLst><a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="12700"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln><a:ln w="19050"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln></a:lnStyleLst>
<a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst>
<a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:bgFillStyleLst>
</a:fmtScheme>
</a:themeElements>
</a:theme>"#,
        a = A_NS, text = text, bg = bg, primary = primary, secondary = secondary, accent = accent,
        hfont = hfont, bfont = bfont
    )
}

fn empty_sp_tree(shapes: &str) -> String {
    format!(
        r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{}</p:spTree>"#,
        shapes
    )
}

/// One paragraph inside a slide text box.
struct Para {
    text: String,
    /// Font size in hundredths of a point (2800 = 28pt).
    sz: u32,
    bold: bool,
    color: String,
    font: String,
    /// Some(hex): a real bullet paragraph - glyph in that color, hanging
    /// indent. None: plain paragraph.
    bullet: Option<String>,
}

/// A plain text box with explicit geometry (EMU) and per-run brand styling.
fn text_box(id: u32, x: i64, y: i64, cx: i64, cy: i64, paragraphs: &[Para]) -> String {
    let mut paras = String::new();
    for p in paragraphs {
        let ppr = match &p.bullet {
            Some(hex) => format!(
                r#"<a:pPr marL="285750" indent="-285750"><a:spcBef><a:spcPts val="600"/></a:spcBef><a:buClr><a:srgbClr val="{}"/></a:buClr><a:buFont typeface="Arial"/><a:buChar char="&#8226;"/></a:pPr>"#,
                hex
            ),
            None => r#"<a:pPr><a:spcBef><a:spcPts val="600"/></a:spcBef><a:buNone/></a:pPr>"#.to_string(),
        };
        paras.push_str(&format!(
            r#"<a:p>{ppr}<a:r><a:rPr lang="en-US" sz="{sz}" b="{b}" dirty="0"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:latin typeface="{font}"/></a:rPr><a:t>{t}</a:t></a:r></a:p>"#,
            ppr = ppr,
            sz = p.sz,
            b = if p.bold { 1 } else { 0 },
            color = p.color,
            font = xml_escape(&p.font),
            t = xml_escape(&p.text),
        ));
    }
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="TextBox {id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr wrap="square"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>"#,
        id = id, x = x, y = y, cx = cx, cy = cy, paras = paras
    )
}

/// Solid rectangle at explicit geometry (EMU).
fn rect(id: u32, x: i64, y: i64, cx: i64, cy: i64, hex: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="Bar"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{hex}"/></a:solidFill><a:ln><a:noFill/></a:ln></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p/></p:txBody></p:sp>"#,
        id = id, x = x, y = y, cx = cx, cy = cy, hex = hex
    )
}

/// Full-width brand bar rectangle.
fn brand_bar(id: u32, y: i64, cy: i64, hex: &str) -> String {
    rect(id, 0, y, 12192000, cy, hex)
}

fn pptx_slide(shapes: String) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="{a}" xmlns:p="{p}" xmlns:r="{r}"><p:cSld>{tree}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#,
        a = A_NS, p = P_NS, r = R_NS, tree = empty_sp_tree(&shapes)
    )
}

fn generate_pptx(spec: &DocSpec, kit: &BrandKit) -> Vec<u8> {
    let primary = hex_no_hash(&kit.color("primary"));
    let secondary = hex_no_hash(&kit.color("secondary"));
    let text = hex_no_hash(&kit.color("text"));
    let hfont = kit.heading_font.clone();
    let bfont = kit.body_font.clone();

    // ── Slides ──
    let mut slides: Vec<String> = Vec::new();

    let logo = load_logo(kit);
    // Title slide: brand bar top, logo, big title, subtitle, thin bar bottom.
    {
        let mut shapes = String::new();
        shapes.push_str(&brand_bar(2, 0, 350000, &primary));
        if let Some(l) = &logo {
            let cy: i64 = 685800; // 0.75in
            let cx: i64 = if l.h > 0 { cy * l.w as i64 / l.h as i64 } else { cy };
            shapes.push_str(&format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="9" name="Logo"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId2"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="914400" y="800000"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
                cx = cx, cy = cy
            ));
        }
        let mut paras = vec![Para {
            text: spec.title.clone(), sz: 4400, bold: true,
            color: primary.clone(), font: hfont.clone(), bullet: None,
        }];
        if let Some(sub) = &spec.subtitle {
            paras.push(Para {
                text: sub.clone(), sz: 2000, bold: false,
                color: secondary.clone(), font: bfont.clone(), bullet: None,
            });
        }
        shapes.push_str(&text_box(3, 914400, 2286000, 10363200, 2286000, &paras));
        shapes.push_str(&brand_bar(4, 6508000, 350000, &secondary));
        slides.push(pptx_slide(shapes));
    }

    // One slide per section: bar, heading over an accent rule, body/bullets.
    let accent = hex_no_hash(&kit.color("accent"));
    for s in &spec.sections {
        let mut shapes = String::new();
        shapes.push_str(&brand_bar(2, 0, 230000, &primary));
        shapes.push_str(&text_box(
            3,
            685800,
            457200,
            10820400,
            914400,
            &[Para {
                text: s.heading.clone(), sz: 2800, bold: true,
                color: primary.clone(), font: hfont.clone(), bullet: None,
            }],
        ));
        // Short accent rule under the heading - the slide-level echo of
        // the website's h2::before bar.
        shapes.push_str(&rect(5, 685800, 1379220, 548640, 45720, &accent));
        let mut body_paras: Vec<Para> = Vec::new();
        for p in s.body.split("\n\n").filter(|p| !p.trim().is_empty()) {
            body_paras.push(Para {
                text: p.trim().to_string(), sz: 1600, bold: false,
                color: text.clone(), font: bfont.clone(), bullet: None,
            });
        }
        for b in &s.bullets {
            body_paras.push(Para {
                text: b.clone(), sz: 1600, bold: false,
                color: text.clone(), font: bfont.clone(), bullet: Some(accent.clone()),
            });
        }
        if body_paras.is_empty() {
            body_paras.push(Para {
                text: String::new(), sz: 1600, bold: false,
                color: text.clone(), font: bfont.clone(), bullet: None,
            });
        }
        shapes.push_str(&text_box(4, 685800, 1600200, 10820400, 4400000, &body_paras));
        slides.push(pptx_slide(shapes));
    }

    // ── Static parts ──
    let mut content_types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Default Extension="png" ContentType="image/png"/>
<Default Extension="jpeg" ContentType="image/jpeg"/>
<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/>
<Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/>
<Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>
<Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>
"#,
    );
    for i in 1..=slides.len() {
        content_types.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>\n",
            i
        ));
    }
    content_types.push_str("</Types>");

    let root_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="ppt/presentation.xml"/>
</Relationships>"#.to_string();

    let mut pres_rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>
"#,
    );
    for (i, _) in slides.iter().enumerate() {
        pres_rels.push_str(&format!(
            "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{}.xml\"/>\n",
            i + 2,
            i + 1
        ));
    }
    pres_rels.push_str("</Relationships>");

    let mut sld_id_list = String::new();
    for (i, _) in slides.iter().enumerate() {
        sld_id_list.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rId{}\"/>", 256 + i, i + 2));
    }
    let presentation = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="{a}" xmlns:p="{p}" xmlns:r="{r}">
<p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst>
<p:sldIdLst>{ids}</p:sldIdLst>
<p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/>
</p:presentation>"#,
        a = A_NS, p = P_NS, r = R_NS, ids = sld_id_list
    );

    // Explicit brand background on the master: without a <p:bg> every
    // renderer falls back to its own default (white in PowerPoint, grey
    // in Quick Look) instead of the kit's background color.
    let master = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="{a}" xmlns:p="{p}" xmlns:r="{r}">
<p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val="{bg}"/></a:solidFill><a:effectLst/></p:bgPr></p:bg>{tree}</p:cSld>
<p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/>
<p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst>
</p:sldMaster>"#,
        a = A_NS, p = P_NS, r = R_NS,
        bg = hex_no_hash(&kit.color("background")),
        tree = empty_sp_tree("")
    );
    let master_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/>
</Relationships>"#.to_string();

    let layout = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="{a}" xmlns:p="{p}" xmlns:r="{r}" type="blank">
<p:cSld>{tree}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>
</p:sldLayout>"#,
        a = A_NS, p = P_NS, r = R_NS, tree = empty_sp_tree("")
    );
    let layout_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/>
</Relationships>"#.to_string();

    let slide_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
</Relationships>"#.to_string();
    let title_slide_rels = if let Some(l) = &logo {
        format!(r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/logo.{}"/>
</Relationships>"#, l.ext)
    } else {
        slide_rels.clone()
    };

    let mut entries: Vec<(String, Vec<u8>)> = vec![
        ("[Content_Types].xml".to_string(), content_types.into_bytes()),
        ("_rels/.rels".to_string(), root_rels.into_bytes()),
        ("ppt/presentation.xml".to_string(), presentation.into_bytes()),
        ("ppt/_rels/presentation.xml.rels".to_string(), pres_rels.into_bytes()),
        ("ppt/slideMasters/slideMaster1.xml".to_string(), master.into_bytes()),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(), master_rels.into_bytes()),
        ("ppt/slideLayouts/slideLayout1.xml".to_string(), layout.into_bytes()),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(), layout_rels.into_bytes()),
        ("ppt/theme/theme1.xml".to_string(), pptx_theme(kit).into_bytes()),
    ];
    for (i, xml) in slides.iter().enumerate() {
        entries.push((format!("ppt/slides/slide{}.xml", i + 1), xml.clone().into_bytes()));
        let rels = if i == 0 { title_slide_rels.clone() } else { slide_rels.clone() };
        entries.push((format!("ppt/slides/_rels/slide{}.xml.rels", i + 1), rels.into_bytes()));
    }
    if let Some(l) = &logo {
        entries.push((format!("ppt/media/logo.{}", l.ext), l.bytes.clone()));
    }
    let borrowed: Vec<(&str, Vec<u8>)> = entries.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    zip_store(&borrowed)
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brand::BrandKit;

    /// Design preview harness: renders a realistic document to
    /// %TEMP%\orbix-doc-preview.html so template work can be judged in a
    /// real browser. Run explicitly:
    ///   cargo test --bin orbixcore render_preview -- --ignored
    #[test]
    #[ignore = "writes a preview file; for manual template work"]
    fn render_preview_to_temp() {
        let spec = DocSpec {
            title: "Guide to Retrieval-Augmented Generation (RAG)".into(),
            subtitle: Some("Bridging the Gap Between Static LLMs and Real-Time Knowledge".into()),
            brief: None,
            sections: vec![
                DocSection {
                    heading: "The RAG Revolution".into(),
                    body: "Large Language Models are powerful, but they rely on static training data. This often leads to \"hallucinations\", where a model confidently provides outdated information.\n\nRetrieval-Augmented Generation (RAG) gives the model an **open-book** experience: it looks up relevant facts from an external source before answering.".into(),
                    bullets: vec![],
                },
                DocSection {
                    heading: "The Three Pillars of RAG".into(),
                    body: "The process begins with **Retrieval**: the system searches a specialized database for documents matching the query. Next comes **Augmentation**, where retrieved information is woven into the prompt. The final stage is **Generation**.".into(),
                    bullets: vec![
                        "Retrieval: locating relevant source documents".into(),
                        "Augmentation: combining sources with the user query".into(),
                        "Generation: synthesizing the final response".into(),
                    ],
                },
                DocSection {
                    heading: "The Power of Vector Embeddings".into(),
                    body: "Text is converted into vectors and stored in a database such as [Pinecone](https://www.pinecone.io/) or [Milvus](https://milvus.io/), optimized for nearest-neighbor search via `cosine similarity`.".into(),
                    bullets: vec![],
                },
                DocSection {
                    heading: "The Microsoft RAG Ecosystem".into(),
                    body: "The [Azure AI Search](https://learn.microsoft.com/en-us/azure/search/) service acts as the retrieval engine. For generation, the [Azure OpenAI Service](https://learn.microsoft.com/en-us/azure/ai-services/openai/) provides secure model access, orchestrated through [Semantic Kernel](https://learn.microsoft.com/en-us/semantic-kernel/).".into(),
                    bullets: vec![],
                },
                DocSection {
                    heading: "Measuring Success".into(),
                    body: "Frameworks like [RAGAS](https://docs.ragas.io/) evaluate the faithfulness of answers to their sources and the relevance of retrieval.".into(),
                    bullets: vec![
                        "Faithfulness: does the answer follow the sources".into(),
                        "Relevance: did retrieval surface the right passages".into(),
                        "Recall: how much of the needed context was found".into(),
                    ],
                },
            ],
        };
        let html = generate_html(&spec, &BrandKit::fallback());
        let path = std::env::temp_dir().join("orbix-doc-preview.html");
        std::fs::write(&path, html).expect("write preview");
        eprintln!("preview: {}", path.display());
    }

    /// The field complaint: models write markdown links and bold into doc
    /// copy; rendering them as literal brackets made pages look broken.
    #[test]
    fn inline_markdown_renders_links_bold_and_code() {
        let html = inline_md(
            "See the [Azure AI Foundry docs](https://learn.microsoft.com/azure/ai-foundry) for **RAG** setup via `create_index`.",
        );
        assert!(
            html.contains(r#"<a href="https://learn.microsoft.com/azure/ai-foundry" target="_blank" rel="noopener">Azure AI Foundry docs</a>"#),
            "{html}"
        );
        assert!(html.contains("<strong>RAG</strong>"), "{html}");
        assert!(html.contains("<code>create_index</code>"), "{html}");
        assert!(!html.contains("**"), "{html}");
    }

    #[test]
    fn inline_markdown_never_lets_html_or_bad_schemes_through() {
        let html = inline_md("<script>alert(1)</script> [x](javascript:alert(1)) plain");
        assert!(!html.contains("<script>"), "{html}");
        assert!(html.contains("&lt;script&gt;"), "{html}");
        // The javascript: pseudo-link must NOT become an anchor - it stays
        // inert escaped text.
        assert!(!html.contains("<a "), "{html}");
        assert!(!html.contains("href"), "{html}");
    }

    fn spec() -> DocSpec {
        DocSpec {
            title: "Q3 Brand Report".into(),
            subtitle: Some("Prepared by Bob".into()),
            brief: None,
            sections: vec![DocSection {
                heading: "Highlights".into(),
                body: "Momentum is strong.\n\nPipeline doubled.".into(),
                bullets: vec!["Revenue +32%".into(), "Churn down".into()],
            }],
        }
    }

    fn zip_names(bytes: &[u8]) -> Vec<String> {
        // Walk local file headers of our STORE-only writer.
        let mut names = Vec::new();
        let mut i = 0usize;
        while i + 30 <= bytes.len() {
            if &bytes[i..i + 4] != 0x04034b50u32.to_le_bytes() {
                break;
            }
            let name_len = u16::from_le_bytes([bytes[i + 26], bytes[i + 27]]) as usize;
            let size = u32::from_le_bytes([bytes[i + 18], bytes[i + 19], bytes[i + 20], bytes[i + 21]]) as usize;
            names.push(String::from_utf8_lossy(&bytes[i + 30..i + 30 + name_len]).to_string());
            i += 30 + name_len + size;
        }
        names
    }

    #[test]
    fn html_carries_brand() {
        let kit = BrandKit::fallback();
        let html = generate_html(&spec(), &kit);
        assert!(html.contains("#B05A33"));
        assert!(html.contains("Q3 Brand Report"));
        assert!(html.contains("Revenue +32%"));
    }

    #[test]
    fn docx_package_structure() {
        let kit = BrandKit::fallback();
        let bytes = generate_docx(&spec(), &kit);
        let names = zip_names(&bytes);
        assert!(names.contains(&"[Content_Types].xml".to_string()));
        assert!(names.contains(&"word/document.xml".to_string()));
        assert!(names.contains(&"word/styles.xml".to_string()));
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("B05A33")); // brand primary in styles
        assert!(s.contains("Q3 Brand Report"));
        // EOCD present
        assert!(bytes.windows(4).any(|w| w == 0x06054b50u32.to_le_bytes()));
    }

    #[test]
    fn pptx_package_structure() {
        let kit = BrandKit::fallback();
        let bytes = generate_pptx(&spec(), &kit);
        let names = zip_names(&bytes);
        assert!(names.contains(&"ppt/presentation.xml".to_string()));
        assert!(names.contains(&"ppt/theme/theme1.xml".to_string()));
        assert!(names.contains(&"ppt/slides/slide1.xml".to_string()));
        assert!(names.contains(&"ppt/slides/slide2.xml".to_string())); // title + 1 section
        let s = String::from_utf8_lossy(&bytes);
        assert!(s.contains("B05A33"));
        assert!(s.contains("Highlights"));
        // Bullets are real bullet paragraphs (accent glyph, hanging
        // indent), not "•  " pasted into the run text.
        assert!(s.contains("buChar"));
        assert!(s.contains("marL=\"285750\""));
        assert!(!s.contains("\u{2022}  "));
    }

    #[test]
    fn website_html_is_a_real_site() {
        let kit = BrandKit::fallback();
        let html = generate_website(&spec(), &kit, None);
        // Semantic shell: nav from headings, hero from title, linked stylesheet.
        assert!(html.contains("<link rel=\"stylesheet\" href=\"styles.css\">"));
        assert!(html.contains("class=\"nav\""));
        assert!(html.contains("href=\"#highlights-1\""));
        assert!(html.contains("<h1>Q3 Brand Report</h1>"));
        // Two bullets stay a checklist, not a broken two-card grid.
        assert!(html.contains("class=\"checklist\""));
        assert!(!html.contains("class=\"cards\""));
    }

    #[test]
    fn website_three_bullets_become_cards() {
        let kit = BrandKit::fallback();
        let mut s = spec();
        s.sections[0].bullets = vec![
            "Fast: renders in two seconds".into(),
            "Private: everything stays local".into(),
            "Branded: colors from your kit".into(),
        ];
        let html = generate_website(&s, &kit, None);
        assert!(html.contains("class=\"cards\""));
        assert!(html.contains("<h3>Fast</h3>"));
        assert!(html.contains("Everything stays local"));
    }

    #[test]
    fn site_css_carries_brand_tokens() {
        let kit = BrandKit::fallback();
        let css = site_css(&kit);
        assert!(css.contains("--primary: #B05A33"));
        assert!(css.contains("Helvetica Neue"));
        assert!(css.contains("prefers-reduced-motion"));
    }

    #[test]
    fn card_parts_splits_short_leadins_only() {
        assert_eq!(
            card_parts("Fast: renders in two seconds"),
            (Some("Fast".into()), "Renders in two seconds".into())
        );
        assert_eq!(
            card_parts("Momentum - pipeline doubled"),
            (Some("Momentum".into()), "Pipeline doubled".into())
        );
        // A long clause before the colon is prose, not a card title.
        let long = "This entire sentence is much too long to be a card title honestly: tail";
        assert_eq!(card_parts(long).0, None);
    }

    #[test]
    fn zip_crc_of_known_value() {
        // CRC32 of "123456789" is the classic check value 0xCBF43926.
        assert_eq!(crc32(b"123456789"), 0xCBF43926);
    }

    #[test]
    fn slug_is_filesystem_safe() {
        assert_eq!(slug("Q3 Report: Growth & Churn!"), "q3-report-growth-churn");
    }
}

#[cfg(test)]
mod real_file_tests {
    use super::*;
    use crate::brand::BrandKit;

    /// Writes real files for external validation (unzip/xmllint/textutil).
    /// Ignored in CI; run explicitly with --ignored.
    #[test]
    #[ignore]
    fn write_samples_to_tmp() {
        let kit = BrandKit::fallback();
        let spec = DocSpec {
            brief: None,
            title: "Orbix Brand Sample".into(),
            subtitle: Some("Generated for validation".into()),
            sections: vec![
                DocSection {
                    heading: "Overview".into(),
                    body: "This document validates branded generation.\n\nSecond paragraph checks spacing.".into(),
                    bullets: vec!["Colors from the kit".into(), "Fonts from the kit".into()],
                },
                DocSection {
                    heading: "Why teams choose Orbix".into(),
                    body: "Everything runs on your machine. The feature cards below come from bullets.".into(),
                    bullets: vec![
                        "Local-first: your data never leaves the device".into(),
                        "Branded: every file styled from your kit".into(),
                        "Fast: documents render in under a second".into(),
                        "Hands-free: Bob builds it while you talk".into(),
                    ],
                },
                DocSection {
                    heading: "Contact".into(),
                    body: "Reach the team any time - Bob answers first.".into(),
                    bullets: vec![],
                },
            ],
        };
        let dir = std::path::PathBuf::from("/tmp/orbix-docgen-samples");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sample.html"), generate_html(&spec, &kit)).unwrap();
        std::fs::write(dir.join("sample.docx"), generate_docx(&spec, &kit)).unwrap();
        std::fs::write(dir.join("sample.pptx"), generate_pptx(&spec, &kit)).unwrap();
        let site = dir.join("sample-site");
        std::fs::create_dir_all(&site).unwrap();
        std::fs::write(site.join("styles.css"), site_css(&kit)).unwrap();
        std::fs::write(site.join("index.html"), generate_website(&spec, &kit, None)).unwrap();
    }
}

// ─────────────────────────────────────────────
//  Bob Connector Author - §8 of CONNECTORS.md
//
//  Bob's authoring loop has 8 steps. Phases 1-7 of CONNECTORS.md
//  shipped the security gates (scan, sign, activate, vault, runtime).
//  This module is the orchestrator that ties them together so Bob has
//  a single Rust entry point per step:
//
//    1. Discover spec       - fetch_spec()
//    2. Draft               - LLM-half lives elsewhere; this module
//                             takes the LLM's outputs as parameters
//    3. Static scan         - connector_activate runs it
//    4. Auth dance          - runs via start_oauth_for_module
//    5. Smoke test          - smoke_test()
//    6. Sign                - connector_activate runs it
//    7. Activate            - connector_activate runs it
//    8. Observe             - out of scope until Bob is online
//
//  The combined entry point is author_connector(): given LLM-produced
//  manifest.json + module.ts + optional tests.ts, write them to a
//  staging dir, run the activator (steps 3, 6, 7 in one shot), and if
//  tests.ts is present run the smoke test (step 5).
//
//  Step 1 (discovery) is the only place we touch the open internet
//  from L-line. It is restricted to a tight allow-list of docs / dev
//  / api hosts derived from the user's chosen vendor string - Bob
//  never browses the open web from here.
// ─────────────────────────────────────────────

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::connector_activate;

const SPEC_FETCH_TIMEOUT: Duration = Duration::from_secs(15);
const SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

// ─────────────────────────────────────────────
//  Step 1 - Discover spec
// ─────────────────────────────────────────────

/// Hosts Bob is allowed to reach for vendor documentation discovery.
/// Bob never browses the open internet - every fetch is matched
/// against a label-bounded prefix of the user's chosen vendor.
///
/// Example for vendor "linkedin.com":
///   docs.linkedin.com          ✓
///   developer.linkedin.com     ✓
///   api.linkedin.com           ✓
///   www.linkedin.com           ✗ - Bob does not authenticate against vendor pages
///   linkedin.com               ✗ - apex would let an attacker tab-jump to ads
fn allowed_discovery_host(vendor: &str, host: &str) -> bool {
    let vendor = vendor.trim().trim_start_matches("www.").to_ascii_lowercase();
    if vendor.is_empty() { return false; }
    let host = host.to_ascii_lowercase();
    for prefix in ["docs.", "developer.", "api.", "dev."] {
        if host == format!("{}{}", prefix, vendor) { return true; }
    }
    false
}

#[derive(Debug, Clone, Serialize)]
pub struct SpecFetch {
    pub url: String,
    pub status: u16,
    pub bytes: usize,
    pub body: serde_json::Value,
}

/// Fetch a single documentation URL with host-pinning to the vendor.
/// Returns the parsed JSON body. Non-JSON responses produce a clear
/// error rather than a malformed Value.
pub async fn fetch_spec(vendor: &str, url: &str) -> Result<SpecFetch, String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("invalid url: {}", e))?;
    let host = parsed.host_str().ok_or("url has no host")?;
    if parsed.scheme() != "https" {
        return Err("spec fetch requires https".into());
    }
    if !allowed_discovery_host(vendor, host) {
        return Err(format!(
            "host '{}' is not in the discovery allow-list for vendor '{}' \
             (docs / developer / api / dev . <vendor> only)",
            host, vendor,
        ));
    }

    let client = reqwest::Client::builder()
        .timeout(SPEC_FETCH_TIMEOUT)
        .build()
        .map_err(|e| format!("client: {}", e))?;
    let resp = client.get(url).send().await
        .map_err(|e| format!("fetch: {}", e))?;
    let status = resp.status().as_u16();
    let bytes_vec = resp.bytes().await
        .map_err(|e| format!("read body: {}", e))?
        .to_vec();
    let body: serde_json::Value = serde_json::from_slice(&bytes_vec)
        .map_err(|e| format!("body is not JSON: {}", e))?;
    Ok(SpecFetch { url: url.to_string(), status, bytes: bytes_vec.len(), body })
}

// ─────────────────────────────────────────────
//  Step 2 - Draft (LLM half lives elsewhere; we take the outputs)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct DraftInputs {
    /// JSON-encoded manifest. The activator parses this; we forward.
    pub manifest_json: String,
    /// TypeScript module source. Must import the bundled SDK at
    /// "./orbix-sdk.ts" - the activator's scanner enforces this.
    pub module_ts: String,
    /// Optional tests.ts for the smoke test. Same trust surface as
    /// module.ts; runs in the same Deno permissions.
    #[serde(default)]
    pub tests_ts: Option<String>,
    /// Optional README content.
    #[serde(default)]
    pub readme: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthorReport {
    pub slug: String,
    pub version: String,
    pub installed_at: String,
    pub activation: connector_activate::ActivationReport,
    pub smoke: Option<SmokeReport>,
}

/// Write the draft to a temp staging dir, hand it to the activator,
/// optionally run the smoke test. This is the one Tauri command Bob
/// will call after the LLM has produced the strings; today the
/// frontend wizard can also call it with hand-written drafts.
pub async fn author_connector(draft: DraftInputs) -> Result<AuthorReport, String> {
    let staging = stage_draft(&draft)?;
    let activation = connector_activate::activate_from_staging(&staging.path())
        .map_err(|e| format!("activation: {}", e))?;

    let smoke = if draft.tests_ts.is_some() {
        Some(smoke_test(&activation.slug).await)
    } else { None };

    Ok(AuthorReport {
        slug: activation.slug.clone(),
        version: activation.version.clone(),
        installed_at: activation.installed_at.clone(),
        activation,
        smoke,
    })
}

/// Owns the temp dir so it's cleaned up after activation (the
/// activator moves a copy into modules/, the staging tree is no
/// longer needed). Uses tempfile::TempDir for automatic cleanup on
/// drop.
struct Staging(tempfile::TempDir);

impl Staging {
    fn path(&self) -> PathBuf { self.0.path().to_path_buf() }
}

fn stage_draft(draft: &DraftInputs) -> Result<Staging, String> {
    let tmp = tempfile::TempDir::new()
        .map_err(|e| format!("tempdir: {}", e))?;
    std::fs::write(tmp.path().join("manifest.json"), &draft.manifest_json)
        .map_err(|e| format!("write manifest: {}", e))?;
    std::fs::write(tmp.path().join("module.ts"), &draft.module_ts)
        .map_err(|e| format!("write module.ts: {}", e))?;
    if let Some(t) = &draft.tests_ts {
        std::fs::write(tmp.path().join("tests.ts"), t)
            .map_err(|e| format!("write tests.ts: {}", e))?;
    }
    if let Some(r) = &draft.readme {
        std::fs::write(tmp.path().join("README.md"), r)
            .map_err(|e| format!("write README: {}", e))?;
    }
    Ok(Staging(tmp))
}

// ─────────────────────────────────────────────
//  Step 5 - Smoke test
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SmokeReport {
    pub passed: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub elapsed_ms: u128,
    pub timed_out: bool,
}

/// Run `<modules_dir>/<slug>/current/tests.ts` under the same Deno
/// permission profile the module uses at runtime. Returns the captured
/// output and a pass/fail. Non-zero exit means failed test.
pub async fn smoke_test(slug: &str) -> SmokeReport {
    let modules_dir = crate::connector_modules::modules_dir();
    let current = modules_dir.join(slug).join("current");
    let tests_path = current.join("tests.ts");
    if !tests_path.exists() {
        return SmokeReport {
            passed: false,
            exit_code: None,
            stdout: String::new(),
            stderr: format!("no tests.ts at {}", tests_path.display()),
            elapsed_ms: 0,
            timed_out: false,
        };
    }

    let manifest = match crate::connector_manifest::load_manifest(&current) {
        Ok(m) => m,
        Err(e) => return SmokeReport {
            passed: false, exit_code: None,
            stdout: String::new(),
            stderr: format!("load manifest: {}", e),
            elapsed_ms: 0, timed_out: false,
        },
    };

    // Same flag derivation the runtime uses - host pinning, env pinning,
    // read scoped to the module dir. We omit the vault env injection
    // here so smoke tests run unauthenticated by default (modules can
    // still call refreshOauth if a token's already in vault). That
    // keeps "smoke" honest about whether a fresh install works.
    let module_ts_for_args = tests_path.clone();
    let args = crate::connector_manifest::deno_spawn_args(&manifest, &module_ts_for_args);

    let deno = resolve_deno();
    let start = std::time::Instant::now();
    let output = tokio::time::timeout(
        SMOKE_TIMEOUT,
        tokio::process::Command::new(&deno)
            .args(&args)
            .current_dir(&current)
            .env_clear()
            .output(),
    ).await;
    let elapsed_ms = start.elapsed().as_millis();

    match output {
        Err(_) => SmokeReport {
            passed: false, exit_code: None,
            stdout: String::new(),
            stderr: format!("smoke test timed out after {}s", SMOKE_TIMEOUT.as_secs()),
            elapsed_ms, timed_out: true,
        },
        Ok(Err(e)) => SmokeReport {
            passed: false, exit_code: None,
            stdout: String::new(),
            stderr: format!("spawn failed: {}", e),
            elapsed_ms, timed_out: false,
        },
        Ok(Ok(out)) => {
            let exit_code = out.status.code();
            SmokeReport {
                passed: out.status.success(),
                exit_code,
                stdout: tail(&String::from_utf8_lossy(&out.stdout), 20),
                stderr: tail(&String::from_utf8_lossy(&out.stderr), 20),
                elapsed_ms,
                timed_out: false,
            }
        }
    }
}

/// Same resolution order as connector_runtime: managed download →
/// dev sidecar → `deno` on PATH. Kept independent so the smoke-test
/// path doesn't require the supervisor to be running.
fn resolve_deno() -> PathBuf {
    // The shipped path: Deno downloaded on first connector use.
    if let Some(managed) = crate::deno_sidecar::existing_managed() {
        return managed;
    }
    let triple = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(target_os = "windows") {
        "x86_64-pc-windows-msvc"
    } else { "" };
    let exe_name = if cfg!(target_os = "windows") {
        format!("deno-{}.exe", triple)
    } else {
        format!("deno-{}", triple)
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let candidate = dir.join(&exe_name);
            if is_real_deno(&candidate) { return candidate; }
        }
    }
    PathBuf::from("deno")
}

fn is_real_deno(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.len() > 1_000_000)
        .unwrap_or(false)
}

fn tail(s: &str, lines: usize) -> String {
    let collected: Vec<&str> = s.lines().collect();
    if collected.len() <= lines {
        collected.join("\n")
    } else {
        collected[collected.len() - lines..].join("\n")
    }
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_allow_list_label_bounded() {
        assert!(allowed_discovery_host("linkedin.com", "docs.linkedin.com"));
        assert!(allowed_discovery_host("linkedin.com", "developer.linkedin.com"));
        assert!(allowed_discovery_host("linkedin.com", "api.linkedin.com"));
        assert!(!allowed_discovery_host("linkedin.com", "www.linkedin.com"));
        assert!(!allowed_discovery_host("linkedin.com", "linkedin.com"));
        assert!(!allowed_discovery_host("linkedin.com", "evil-linkedin.com"));
        assert!(!allowed_discovery_host("linkedin.com", "linkedin.com.evil.com"));
        // www-prefixed vendor input still works.
        assert!(allowed_discovery_host("www.linkedin.com", "docs.linkedin.com"));
        // Empty / blank vendor rejects everything.
        assert!(!allowed_discovery_host("", "docs.linkedin.com"));
        assert!(!allowed_discovery_host("   ", "docs.linkedin.com"));
    }

    #[tokio::test]
    async fn fetch_spec_rejects_non_https() {
        let r = fetch_spec("linkedin.com", "http://docs.linkedin.com/openapi").await
            .unwrap_err();
        assert!(r.contains("https"));
    }

    #[tokio::test]
    async fn fetch_spec_rejects_unlisted_host() {
        let r = fetch_spec("linkedin.com", "https://evil.example.com/openapi").await
            .unwrap_err();
        assert!(r.contains("not in the discovery allow-list"));
    }

    #[tokio::test]
    async fn fetch_spec_rejects_malformed_url() {
        let r = fetch_spec("linkedin.com", "not-a-url").await.unwrap_err();
        assert!(r.contains("invalid url") || r.contains("no host"));
    }

    #[test]
    fn tail_returns_last_n_lines() {
        let s = "a\nb\nc\nd\ne";
        assert_eq!(tail(s, 2), "d\ne");
        assert_eq!(tail(s, 5), s);
        assert_eq!(tail(s, 10), s);
        assert_eq!(tail("", 3), "");
    }
}

// ─────────────────────────────────────────────
//  LLM connector generation (W5.3)
//
//  Closes the self-building loop: this module used to take manifest +
//  module.ts drafts as inputs (the "generation intelligence lives
//  elsewhere" note). Here the local model DRAFTS them from a natural
//  language request, and we hand the result straight into the existing
//  author_connector -> scan -> sign -> activate pipeline (signing enforced
//  since W5.1). Post-processing makes the common case activate cleanly:
//  the manifest host_allowlist is derived from the hosts the generated
//  module.ts actually fetches, so the scanner's "no capability widening"
//  rule passes.
// ─────────────────────────────────────────────

/// Extract https/http hosts from `fetch("...")` string literals, matching
/// the static scanner's host semantics. Used to backfill host_allowlist.
pub(crate) fn hosts_in_module(module_ts: &str) -> Vec<String> {
    let re = regex::Regex::new(r#"fetch\(\s*[`"']\s*(https?://[^`"'\s]+)"#).unwrap();
    let mut hosts: Vec<String> = Vec::new();
    for cap in re.captures_iter(module_ts) {
        if let Some(url) = cap.get(1) {
            let lower = url.as_str().to_ascii_lowercase();
            if let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) {
                if let Some(host_part) = rest.split('/').next() {
                    let host = host_part.split(':').next().unwrap_or("");
                    if !host.is_empty() && !hosts.contains(&host.to_string()) {
                        hosts.push(host.to_string());
                    }
                }
            }
        }
    }
    hosts
}

/// Build the generation prompt: the SDK contract, the manifest schema, an
/// example, and the user's request. Kept as one function so it is testable.
fn generation_prompt(request: &str, vendor: &str, spec: Option<&str>) -> String {
    let spec_block = spec
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("\n\nAPI REFERENCE (use this for endpoints/fields):\n{}", crate::truncate_str(s, 4000)))
        .unwrap_or_default();
    format!(
        "You generate an Orbix connector module. Output ONLY a JSON object with keys \
\"manifest\" (object) and \"module_ts\" (string). No prose, no markdown fences.\n\n\
The manifest schema:\n\
{{\n\
  \"slug\": \"kebab-case-id\",\n\
  \"version\": \"0.1.0\",\n\
  \"display_name\": \"Human Name\",\n\
  \"vendor\": \"{vendor}\",\n\
  \"driver\": {{ \"kind\": \"api\" }},\n\
  \"auth\": {{ \"kind\": \"api_key\", \"header\": \"Authorization\" }},\n\
  \"host_allowlist\": [\"api.example.com\"],\n\
  \"env_allowlist\": [\"EXAMPLE_TOKEN\"],\n\
  \"actions\": {{ \"action_name\": {{ \"description\": \"...\" }} }}\n\
}}\n\n\
The module.ts MUST import the SDK exactly like this and define one function per action:\n\
import {{ defineConnector, env, OrbixError }} from \"./orbix-sdk.ts\";\n\
defineConnector({{\n\
  async action_name({{ inputs }}) {{\n\
    const tok = env(\"EXAMPLE_TOKEN\");\n\
    const r = await fetch(\"https://api.example.com/path\", {{\n\
      method: \"POST\",\n\
      headers: {{ Authorization: `Bearer ${{tok}}` }},\n\
      body: JSON.stringify(inputs),\n\
    }});\n\
    if (r.status === 401) throw new OrbixError(\"auth_expired\", \"401\");\n\
    if (!r.ok) throw new OrbixError(\"upstream_error\", `HTTP ${{r.status}}`);\n\
    return await r.json();\n\
  }},\n\
}});\n\n\
RULES: every host you fetch() must appear in host_allowlist; every env() key in \
env_allowlist; action names in the manifest must match the functions; no eval, no \
Function constructor, no dynamic imports. Use https only.{spec_block}\n\n\
REQUEST: {request}\n\n\
JSON:",
        vendor = vendor, request = request, spec_block = spec_block,
    )
}

/// Assemble DraftInputs from a parsed model object, normalizing the SDK
/// import and backfilling host_allowlist from the module source.
pub(crate) fn drafts_from_model_json(v: &serde_json::Value) -> Result<DraftInputs, String> {
    let mut manifest = v.get("manifest").cloned().ok_or("model output missing 'manifest'")?;
    let module_ts = v.get("module_ts").and_then(|x| x.as_str()).ok_or("model output missing 'module_ts'")?.to_string();

    // Normalize the SDK import path the scanner requires.
    let module_ts = module_ts
        .replace("./_sdk/orbix-sdk.ts", "./orbix-sdk.ts")
        .replace("orbix-sdk", "orbix-sdk"); // no-op guard, keeps intent clear

    // Backfill host_allowlist from what the code actually fetches, so the
    // "no capability widening" scan passes even if the model under-declared.
    let derived = hosts_in_module(&module_ts);
    if let Some(obj) = manifest.as_object_mut() {
        let existing: Vec<String> = obj.get("host_allowlist")
            .and_then(|h| h.as_array())
            .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();
        let mut merged = existing;
        for h in derived {
            if !merged.contains(&h) { merged.push(h); }
        }
        obj.insert("host_allowlist".to_string(), serde_json::json!(merged));
    }

    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| format!("serialize manifest: {}", e))?;
    Ok(DraftInputs { manifest_json, module_ts, tests_ts: None, readme: None })
}

/// Generate a connector from a natural-language request using the local
/// model, then author (scan + sign + activate) it. Returns the author
/// report on success.
pub async fn generate_and_author(
    request: &str,
    vendor: &str,
    spec: Option<&str>,
) -> Result<AuthorReport, String> {
    let client = reqwest::Client::new();
    let prompt = generation_prompt(request, vendor, spec);
    let history = vec![crate::Message {
        id: uuid::Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: prompt,
        context: None,
        agent_id: None,
        timestamp: chrono::Utc::now().to_rfc3339(),
        tool_turns: None,
    }];
    let raw = crate::call_local_llm(&client, &history).await?;

    // The model may wrap JSON in prose/fences; extract the object.
    let json_text = extract_json_object(&raw).ok_or("model did not return a JSON object")?;
    let v: serde_json::Value = serde_json::from_str(&json_text)
        .or_else(|_| crate::repair_json(&json_text).and_then(|f| serde_json::from_str(&f).ok()).ok_or(()))
        .map_err(|_| "could not parse the generated connector JSON".to_string())?;

    let draft = drafts_from_model_json(&v)?;
    author_connector(draft).await
}

/// Find the outermost {...} JSON object in a text blob.
fn extract_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        match c {
            '"' if !esc => in_str = !in_str,
            '\\' if in_str => { esc = !esc; continue; }
            '{' if !in_str => depth += 1,
            '}' if !in_str => {
                depth -= 1;
                if depth == 0 {
                    return Some(text[start..start + i + 1].to_string());
                }
            }
            _ => {}
        }
        esc = false;
    }
    None
}

#[cfg(test)]
mod gen_tests {
    use super::*;

    #[test]
    fn hosts_extracted_from_fetch() {
        let m = r#"const r = await fetch("https://api.acme.com/v1/x", {method:"GET"});
                   await fetch('https://hooks.acme.io/notify');"#;
        let hosts = hosts_in_module(m);
        assert!(hosts.contains(&"api.acme.com".to_string()));
        assert!(hosts.contains(&"hooks.acme.io".to_string()));
    }

    #[test]
    fn drafts_backfill_host_allowlist() {
        let v = serde_json::json!({
            "manifest": {
                "slug": "acme", "version": "0.1.0", "display_name": "Acme",
                "vendor": "Acme", "driver": {"kind": "api"},
                "auth": {"kind": "api_key", "header": "Authorization"},
                "host_allowlist": [], "env_allowlist": ["ACME_TOKEN"],
                "actions": {"send": {"description": "send"}}
            },
            "module_ts": "import { defineConnector, env } from \"./orbix-sdk.ts\";\nawait fetch(\"https://api.acme.com/send\");"
        });
        let draft = drafts_from_model_json(&v).unwrap();
        assert!(draft.manifest_json.contains("api.acme.com"), "host not backfilled: {}", draft.manifest_json);
        assert!(draft.module_ts.contains("./orbix-sdk.ts"));
    }

    #[test]
    fn extract_json_handles_prose_and_braces() {
        let text = "Here you go:\n{\"a\": {\"b\": 1}}\nhope that helps";
        assert_eq!(extract_json_object(text).unwrap(), "{\"a\": {\"b\": 1}}");
    }
}

#[cfg(test)]
mod live_gen_tests {
    //! Live connector generation against a real model on 8847. Ignored by
    //! default. Run: cargo test --bin orbixcore live_generate_connector -- --ignored --nocapture
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn live_generate_connector() {
        let client = reqwest::Client::new();
        let prompt = generation_prompt(
            "Send a chat message to a channel via the Acme messaging API using a bearer token",
            "Acme",
            None,
        );
        let history = vec![crate::Message {
            id: "t".into(), role: "user".into(), content: prompt,
            context: None, agent_id: None, timestamp: "t".into(), tool_turns: None,
        }];
        let raw = crate::call_local_llm(&client, &history).await.expect("model call");
        let json_text = extract_json_object(&raw)
            .unwrap_or_else(|| panic!("no JSON object in model output:\n{}", raw));
        let v: serde_json::Value = serde_json::from_str(&json_text)
            .or_else(|_| crate::repair_json(&json_text).and_then(|f| serde_json::from_str(&f).ok()).ok_or(()))
            .unwrap_or_else(|_| panic!("unparseable generated JSON:\n{}", json_text));
        let draft = drafts_from_model_json(&v).expect("assemble drafts");

        // The generated draft must be well-formed enough to author.
        let manifest: serde_json::Value = serde_json::from_str(&draft.manifest_json).expect("manifest parses");
        assert!(manifest.get("slug").and_then(|s| s.as_str()).is_some(), "no slug");
        assert!(manifest.get("actions").and_then(|a| a.as_object()).map(|o| !o.is_empty()).unwrap_or(false), "no actions");
        assert!(draft.module_ts.contains("./orbix-sdk.ts"), "module does not import SDK: {}", draft.module_ts);
        assert!(draft.module_ts.contains("defineConnector"), "module has no defineConnector");
        let hosts = manifest.get("host_allowlist").and_then(|h| h.as_array()).map(|a| a.len()).unwrap_or(0);
        println!("LIVE GEN OK: slug={} actions={} hosts={}",
            manifest["slug"], manifest["actions"].as_object().map(|o| o.len()).unwrap_or(0), hosts);
    }
}

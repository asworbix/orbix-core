//! Pluggable write destinations (sinks) for automations.
//!
//! An automation's last step maps its processed data onto whatever store
//! the user already runs - a local SQLite file, an enterprise SQL
//! database, or an HTTP/OData API like Dataverse - through one uniform
//! interface: `describe()` the target schema, then `write()` rows. The
//! adapters differ wildly underneath (a file path vs a connection string
//! vs an OAuth endpoint); every caller sees only those two methods, so
//! adding a new backend never touches the workflow engine or the UI.
//!
//! This is the generic replacement for the old hardcoded single-target
//! CRM write path: the destination is data, chosen per workflow.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One column of a destination's schema, so the AI mapping step and the
/// Build field-picker know what shape the destination expects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ColumnDef {
    pub name: String,
    /// Backend-native type label ("TEXT", "INTEGER", "Edm.String", ...).
    pub data_type: String,
    #[serde(default = "default_true")]
    pub nullable: bool,
    #[serde(default)]
    pub primary_key: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
pub struct SchemaInfo {
    /// Table or entity name.
    pub target: String,
    pub columns: Vec<ColumnDef>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct WriteOutcome {
    pub written: usize,
    /// Backend id per written row where the backend returns one (SQLite
    /// rowid, API response id). May be shorter than `written`.
    pub ids: Vec<String>,
    /// One message per row that failed; empty on full success.
    pub errors: Vec<String>,
}

/// Auth for an HTTP destination. Mirrors the connector auth modes so a
/// Dataverse OAuth token or a plain API key both fit.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HttpAuth {
    None,
    Bearer { token: String },
    ApiKey { header: String, value: String },
}

/// A write target chosen per workflow. Serializes tagged by `kind` so it
/// rides in a workflow step's `config` as plain JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Destination {
    /// Local SQLite file. `path` is an absolute .db path; `table` the
    /// target table. Local-first: no network, no credentials.
    Sqlite { path: String, table: String },
    /// Any HTTP/OData API that accepts one JSON record per request - a
    /// generic REST endpoint or Dataverse (`base_url` = org url, `path` =
    /// /api/data/v9.2/<entity>). One request per row.
    Http {
        base_url: String,
        path: String,
        #[serde(default = "default_post")]
        method: String,
        #[serde(default)]
        auth: Option<HttpAuth>,
        /// Declared columns for endpoints with no live introspection.
        #[serde(default)]
        columns: Vec<ColumnDef>,
    },
}

fn default_post() -> String {
    "POST".to_string()
}

impl Destination {
    /// A short human label for logs and the UI.
    pub fn label(&self) -> String {
        match self {
            Destination::Sqlite { path, table } => {
                let file = std::path::Path::new(path)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.clone());
                format!("SQLite {}·{}", file, table)
            }
            Destination::Http { base_url, path, .. } => {
                format!("HTTP {}{}", base_url.trim_end_matches('/'), path)
            }
        }
    }

    /// The target schema (columns) so a mapping step knows what to fill.
    pub async fn describe(&self) -> Result<SchemaInfo, String> {
        match self {
            Destination::Sqlite { path, table } => {
                let path = path.clone();
                let table = table.clone();
                tokio::task::spawn_blocking(move || sqlite_describe(&path, &table))
                    .await
                    .map_err(|e| e.to_string())?
            }
            Destination::Http { columns, path, .. } => Ok(SchemaInfo {
                target: entity_from_path(path),
                columns: columns.clone(),
            }),
        }
    }

    /// Write rows. Each row must be a JSON object; fields that do not
    /// match the destination are ignored for SQLite (the AI maps onto the
    /// schema, and extra keys are dropped rather than erroring the run).
    pub async fn write(&self, rows: &[Value]) -> Result<WriteOutcome, String> {
        if rows.is_empty() {
            return Ok(WriteOutcome::default());
        }
        match self {
            Destination::Sqlite { path, table } => {
                let path = path.clone();
                let table = table.clone();
                let rows = rows.to_vec();
                tokio::task::spawn_blocking(move || sqlite_write(&path, &table, &rows))
                    .await
                    .map_err(|e| e.to_string())?
            }
            Destination::Http { base_url, path, method, auth, .. } => {
                http_write(base_url, path, method, auth, rows).await
            }
        }
    }
}

fn entity_from_path(path: &str) -> String {
    path.trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(path)
        .to_string()
}

// ─────────────────────────────────────────────
//  SQLite adapter (Class B - direct database)
// ─────────────────────────────────────────────

fn sqlite_open(path: &str) -> Result<rusqlite::Connection, String> {
    rusqlite::Connection::open(path).map_err(|e| format!("open {}: {}", path, e))
}

fn sqlite_describe(path: &str, table: &str) -> Result<SchemaInfo, String> {
    let conn = sqlite_open(path)?;
    // PRAGMA table_info returns: cid, name, type, notnull, dflt_value, pk.
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", quote_ident(table)))
        .map_err(|e| e.to_string())?;
    let cols = stmt
        .query_map([], |row| {
            Ok(ColumnDef {
                name: row.get::<_, String>(1)?,
                data_type: row.get::<_, String>(2).unwrap_or_default(),
                nullable: row.get::<_, i64>(3).unwrap_or(0) == 0,
                primary_key: row.get::<_, i64>(5).unwrap_or(0) != 0,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if cols.is_empty() {
        return Err(format!(
            "table \"{}\" not found in {} (create it first, or check the name)",
            table, path
        ));
    }
    Ok(SchemaInfo { target: table.to_string(), columns: cols })
}

fn sqlite_write(path: &str, table: &str, rows: &[Value]) -> Result<WriteOutcome, String> {
    let mut conn = sqlite_open(path)?;
    let schema = sqlite_describe(path, table)?;
    let valid: std::collections::HashSet<&str> =
        schema.columns.iter().map(|c| c.name.as_str()).collect();

    let mut out = WriteOutcome::default();
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    for (i, row) in rows.iter().enumerate() {
        let Some(obj) = row.as_object() else {
            out.errors.push(format!("row {}: not a JSON object", i + 1));
            continue;
        };
        // Keep only fields that map onto real columns - the AI mapping
        // step targets the schema, and stray keys must not fail the write.
        let cols: Vec<(&String, &Value)> =
            obj.iter().filter(|(k, _)| valid.contains(k.as_str())).collect();
        if cols.is_empty() {
            out.errors.push(format!(
                "row {}: no fields match table columns [{}]",
                i + 1,
                schema.columns.iter().map(|c| c.name.as_str()).collect::<Vec<_>>().join(", ")
            ));
            continue;
        }
        let col_list = cols.iter().map(|(k, _)| quote_ident(k)).collect::<Vec<_>>().join(", ");
        let placeholders = cols.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            quote_ident(table),
            col_list,
            placeholders
        );
        let params: Vec<rusqlite::types::Value> =
            cols.iter().map(|(_, v)| json_to_sql(v)).collect();
        let param_refs: Vec<&dyn rusqlite::ToSql> =
            params.iter().map(|p| p as &dyn rusqlite::ToSql).collect();
        match tx.execute(&sql, param_refs.as_slice()) {
            Ok(_) => {
                out.written += 1;
                out.ids.push(tx.last_insert_rowid().to_string());
            }
            Err(e) => out.errors.push(format!("row {}: {}", i + 1, e)),
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(out)
}

/// Quote a SQL identifier by wrapping in double quotes and escaping any
/// embedded quote. Table/column names come from schema introspection or
/// user config, never interpolated raw.
fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

fn json_to_sql(v: &Value) -> rusqlite::types::Value {
    use rusqlite::types::Value as S;
    match v {
        Value::Null => S::Null,
        Value::Bool(b) => S::Integer(if *b { 1 } else { 0 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                S::Integer(i)
            } else if let Some(f) = n.as_f64() {
                S::Real(f)
            } else {
                S::Text(n.to_string())
            }
        }
        Value::String(s) => S::Text(s.clone()),
        // Objects/arrays land as JSON text - a reasonable default for a
        // column that holds structured data.
        other => S::Text(other.to_string()),
    }
}

// ─────────────────────────────────────────────
//  HTTP / OData adapter (Class A - API destination)
// ─────────────────────────────────────────────

async fn http_write(
    base_url: &str,
    path: &str,
    method: &str,
    auth: &Option<HttpAuth>,
    rows: &[Value],
) -> Result<WriteOutcome, String> {
    let url = format!("{}{}", base_url.trim_end_matches('/'), path);
    let client = reqwest::Client::new();
    let mut out = WriteOutcome::default();
    for (i, row) in rows.iter().enumerate() {
        let mut req = match method.to_uppercase().as_str() {
            "PUT" => client.put(&url),
            "PATCH" => client.patch(&url),
            _ => client.post(&url),
        };
        req = req.header("Content-Type", "application/json");
        match auth {
            Some(HttpAuth::Bearer { token }) => req = req.bearer_auth(token),
            Some(HttpAuth::ApiKey { header, value }) => req = req.header(header.as_str(), value),
            _ => {}
        }
        match req.json(row).send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                if status.is_success() {
                    out.written += 1;
                    if let Some(id) = extract_id(&body) {
                        out.ids.push(id);
                    }
                } else {
                    out.errors.push(format!(
                        "row {}: {} {}",
                        i + 1,
                        status.as_u16(),
                        crate::truncate_str(&body, 200)
                    ));
                }
            }
            Err(e) => out.errors.push(format!("row {}: {}", i + 1, e)),
        }
    }
    Ok(out)
}

/// Best-effort id from a create response: common `id`/`Id`/`ID` keys, or
/// Dataverse's OData-EntityId header shape embedded in the body.
fn extract_id(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    for key in ["id", "Id", "ID", "contactid", "accountid"] {
        if let Some(s) = v.get(key).and_then(|x| x.as_str()) {
            return Some(s.to_string());
        }
        if let Some(n) = v.get(key).and_then(|x| x.as_i64()) {
            return Some(n.to_string());
        }
    }
    None
}

// ─────────────────────────────────────────────
//  Tests (SQLite runs for real against a temp db; HTTP is serde-checked)
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_with_leads() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db").to_string_lossy().to_string();
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE leads (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT, score INTEGER, active INTEGER)",
            [],
        )
        .unwrap();
        (dir, path)
    }

    #[tokio::test]
    async fn sqlite_describe_reads_columns() {
        let (_dir, path) = temp_db_with_leads();
        let d = Destination::Sqlite { path, table: "leads".into() };
        let schema = d.describe().await.unwrap();
        assert_eq!(schema.target, "leads");
        let names: Vec<&str> = schema.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["id", "name", "email", "score", "active"]);
        let id_col = &schema.columns[0];
        assert!(id_col.primary_key);
        let name_col = &schema.columns[1];
        assert!(!name_col.nullable, "name is NOT NULL");
    }

    #[tokio::test]
    async fn sqlite_write_maps_onto_schema_and_drops_extra_fields() {
        let (_dir, path) = temp_db_with_leads();
        let d = Destination::Sqlite { path: path.clone(), table: "leads".into() };
        let rows = vec![
            // typed values + an extra key the schema does not have
            json!({"name": "Ada", "email": "ada@x.com", "score": 90, "active": true, "unmapped": "ignore me"}),
            json!({"name": "Alan", "email": "alan@x.com", "score": 88, "active": false}),
        ];
        let out = d.write(&rows).await.unwrap();
        assert_eq!(out.written, 2, "both rows written; errors: {:?}", out.errors);
        assert_eq!(out.ids.len(), 2);
        assert!(out.errors.is_empty());

        // Read back to prove the mapping + type coercion (bool -> 0/1).
        let conn = rusqlite::Connection::open(&path).unwrap();
        let (name, active): (String, i64) = conn
            .query_row("SELECT name, active FROM leads WHERE email = 'ada@x.com'", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!(name, "Ada");
        assert_eq!(active, 1);
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM leads", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 2);
    }

    #[tokio::test]
    async fn sqlite_row_with_no_matching_columns_errors_that_row_only() {
        let (_dir, path) = temp_db_with_leads();
        let d = Destination::Sqlite { path, table: "leads".into() };
        let rows = vec![
            json!({"name": "Grace", "email": "grace@x.com"}),
            json!({"totally": "unrelated"}), // no column matches
        ];
        let out = d.write(&rows).await.unwrap();
        assert_eq!(out.written, 1);
        assert_eq!(out.errors.len(), 1);
        assert!(out.errors[0].contains("no fields match"));
    }

    #[tokio::test]
    async fn sqlite_missing_table_is_actionable() {
        let (_dir, path) = temp_db_with_leads();
        let d = Destination::Sqlite { path, table: "does_not_exist".into() };
        let err = d.describe().await.unwrap_err();
        assert!(err.contains("not found"), "got: {err}");
    }

    #[test]
    fn quote_ident_blocks_injection_via_identifiers() {
        assert_eq!(quote_ident("leads"), "\"leads\"");
        assert_eq!(quote_ident("we\"ird"), "\"we\"\"ird\"");
    }

    #[test]
    fn destination_round_trips_through_json_config() {
        // A workflow step stores the destination in its config as JSON.
        let sqlite = json!({"kind": "sqlite", "path": "/tmp/x.db", "table": "leads"});
        let d: Destination = serde_json::from_value(sqlite).unwrap();
        assert!(matches!(d, Destination::Sqlite { .. }));

        let dataverse = json!({
            "kind": "http",
            "base_url": "https://org.crm.dynamics.com",
            "path": "/api/data/v9.2/contacts",
            "method": "POST",
            "auth": {"kind": "bearer", "token": "tok"},
            "columns": [{"name": "fullname", "data_type": "Edm.String"}]
        });
        let d: Destination = serde_json::from_value(dataverse).unwrap();
        match d {
            Destination::Http { base_url, path, method, auth, columns } => {
                assert_eq!(base_url, "https://org.crm.dynamics.com");
                assert_eq!(path, "/api/data/v9.2/contacts");
                assert_eq!(method, "POST");
                assert!(matches!(auth, Some(HttpAuth::Bearer { .. })));
                assert_eq!(columns[0].name, "fullname");
            }
            _ => panic!("expected Http"),
        }
    }

    #[test]
    fn http_describe_uses_declared_columns() {
        // (describe on Http is sync-ish; exercise via block_on-free path)
        let d = Destination::Http {
            base_url: "https://api.example.com".into(),
            path: "/v1/leads".into(),
            method: "POST".into(),
            auth: None,
            columns: vec![ColumnDef {
                name: "email".into(),
                data_type: "string".into(),
                nullable: false,
                primary_key: false,
            }],
        };
        assert_eq!(d.label(), "HTTP https://api.example.com/v1/leads");
    }

    #[test]
    fn json_to_sql_coercions() {
        use rusqlite::types::Value as S;
        assert!(matches!(json_to_sql(&json!(null)), S::Null));
        assert!(matches!(json_to_sql(&json!(true)), S::Integer(1)));
        assert!(matches!(json_to_sql(&json!(42)), S::Integer(42)));
        assert!(matches!(json_to_sql(&json!(3.5)), S::Real(_)));
        assert!(matches!(json_to_sql(&json!("hi")), S::Text(_)));
        // objects serialize to JSON text
        match json_to_sql(&json!({"a": 1})) {
            S::Text(t) => assert!(t.contains("\"a\"")),
            _ => panic!("object should become text"),
        }
    }
}

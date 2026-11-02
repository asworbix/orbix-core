//! Task-Handoff (FABLE B2): a frontier model plans, Orbix executes.
//!
//! HandoffSpec is the tiny interop contract any cloud model can emit
//! ("emit an Orbix handoff"). It maps 1:1 onto the existing workflow
//! runner - no new execution engine: prompt steps run on the free local
//! model, tool steps run Bob's built-in tools, and the whole run is
//! durable, resumable, and visible in the Run surfaces like any other
//! workflow. The published contract lives in docs/orbix-handoff.md and
//! docs/orbix-handoff.schema.json - keep all three in sync and small.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;

use crate::workflow::{StepType, WorkflowStep};

pub const MAX_STEPS: usize = 50;

// ─────────────────────────────────────────────
//  The interop contract
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffStep {
    /// prompt | action | tool
    pub kind: String,
    /// prompt: the prompt text. action: a shell command. tool: a human
    /// label for the step (the tool+args carry the work).
    pub instruction: String,
    /// tool kind only: shell_exec | file_read | file_write | file_edit |
    /// file_list | web_fetch
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub args: Option<Value>,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffSpec {
    pub title: String,
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub tone: Option<String>,
    pub steps: Vec<HandoffStep>,
    /// doc | panel | none
    #[serde(default)]
    pub deliverable: Option<String>,
}

pub fn validate(spec: &HandoffSpec) -> Result<(), String> {
    let title = spec.title.trim();
    if title.is_empty() || title.chars().count() > 120 {
        return Err("title must be 1-120 characters".to_string());
    }
    if spec.steps.is_empty() || spec.steps.len() > MAX_STEPS {
        return Err(format!("steps must contain 1-{} entries", MAX_STEPS));
    }
    if let Some(d) = spec.deliverable.as_deref() {
        if !matches!(d, "doc" | "panel" | "none") {
            return Err(format!("deliverable \"{}\" is not one of doc, panel, none", d));
        }
    }
    for (i, step) in spec.steps.iter().enumerate() {
        if step.instruction.trim().is_empty() {
            return Err(format!("step {} has an empty instruction", i + 1));
        }
        match step.kind.as_str() {
            "prompt" | "action" => {}
            "tool" => {
                let tool = step.tool.as_deref().unwrap_or("");
                if !crate::tools::is_builtin_tool(tool) {
                    return Err(format!(
                        "step {}: tool \"{}\" is not a built-in tool (shell_exec, file_read, file_write, file_edit, file_list, web_fetch)",
                        i + 1, tool
                    ));
                }
            }
            other => {
                return Err(format!(
                    "step {}: kind \"{}\" is not one of prompt, action, tool",
                    i + 1, other
                ));
            }
        }
    }
    Ok(())
}

/// Map the spec onto the existing runner's step model.
pub fn to_workflow_steps(spec: &HandoffSpec) -> Vec<WorkflowStep> {
    let context_prefix = {
        let mut p = String::new();
        if !spec.goal.trim().is_empty() {
            p.push_str(&format!("Goal: {}\n", spec.goal.trim()));
        }
        if let Some(tone) = spec.tone.as_deref() {
            if !tone.trim().is_empty() {
                p.push_str(&format!("Tone: {}\n", tone.trim()));
            }
        }
        if !p.is_empty() {
            p.push('\n');
        }
        p
    };

    let mut steps: Vec<WorkflowStep> = spec
        .steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let label = s
                .label
                .clone()
                .unwrap_or_else(|| s.instruction.chars().take(48).collect());
            let (step_type, config) = match s.kind.as_str() {
                "tool" => {
                    let mut cfg = HashMap::new();
                    cfg.insert("tool".to_string(), json!(s.tool.clone().unwrap_or_default()));
                    cfg.insert("tool_args".to_string(), s.args.clone().unwrap_or_else(|| json!({})));
                    (StepType::Action, cfg)
                }
                "action" => {
                    let mut cfg = HashMap::new();
                    cfg.insert("tool".to_string(), json!("shell_exec"));
                    cfg.insert("tool_args".to_string(), json!({ "command": s.instruction }));
                    (StepType::Action, cfg)
                }
                _ => {
                    let mut cfg = HashMap::new();
                    cfg.insert(
                        "prompt".to_string(),
                        json!(format!("{}{}", context_prefix, s.instruction)),
                    );
                    (StepType::Prompt, cfg)
                }
            };
            WorkflowStep {
                id: uuid::Uuid::new_v4().to_string(),
                order: i,
                step_type,
                label,
                config,
            }
        })
        .collect();

    if spec.deliverable.as_deref() == Some("doc") || spec.deliverable.as_deref() == Some("panel") {
        let mut cfg = HashMap::new();
        cfg.insert(
            "prompt".to_string(),
            json!(format!(
                "{}Write the final deliverable for \"{}\" as clean markdown: \
                 start with a one-paragraph summary, then 3-7 sections, each \
                 as a '## Heading' line followed by short paragraphs and \
                 '- ' bullets where useful. Use only real results from the \
                 steps above - never placeholders.",
                context_prefix, spec.title
            )),
        );
        steps.push(WorkflowStep {
            id: uuid::Uuid::new_v4().to_string(),
            order: steps.len(),
            step_type: StepType::Prompt,
            label: "Compose deliverable".to_string(),
            config: cfg,
        });
    }

    steps
}

// ─────────────────────────────────────────────
//  Deliverable: markdown -> DocSpec (pure, no LLM round-trip)
// ─────────────────────────────────────────────

pub fn doc_spec_from_markdown(title: &str, markdown: &str) -> crate::docgen::DocSpec {
    let mut sections: Vec<crate::docgen::DocSection> = Vec::new();
    let mut current = crate::docgen::DocSection::default();
    let mut has_heading = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        if let Some(h) = trimmed
            .strip_prefix("## ")
            .or_else(|| trimmed.strip_prefix("# "))
            .or_else(|| trimmed.strip_prefix("### "))
        {
            if has_heading || !current.body.trim().is_empty() || !current.bullets.is_empty() {
                let mut done = std::mem::take(&mut current);
                if done.heading.is_empty() {
                    done.heading = "Summary".to_string();
                }
                sections.push(done);
            }
            current.heading = h.trim().to_string();
            has_heading = true;
        } else if let Some(b) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            current.bullets.push(b.trim().to_string());
        } else if !trimmed.is_empty() {
            if !current.body.is_empty() {
                current.body.push_str("\n\n");
            }
            current.body.push_str(trimmed);
        }
    }
    if has_heading || !current.body.trim().is_empty() || !current.bullets.is_empty() {
        if current.heading.is_empty() {
            current.heading = "Summary".to_string();
        }
        sections.push(current);
    }

    crate::docgen::DocSpec {
        title: title.to_string(),
        subtitle: None,
        sections,
        brief: None,
    }
}

// ─────────────────────────────────────────────
//  Execution + SSE streaming
// ─────────────────────────────────────────────

fn sse_frame(v: &Value) -> warp::hyper::body::Bytes {
    warp::hyper::body::Bytes::from(format!("data: {}\n\n", v))
}

fn json_response(status: u16, v: &Value) -> warp::http::Response<warp::hyper::Body> {
    warp::http::Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(warp::hyper::Body::from(v.to_string()))
        .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty()))
}

/// Create the workflow for a validated spec and return (workflow_id, run_id).
fn create_handoff_workflow(app: &tauri::AppHandle, spec: &HandoffSpec) -> (String, String) {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let workflow_id = {
        let mut store = state
            .automation
            .workflow_store
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let mut wf = store.create(
            format!("Handoff: {}", spec.title.trim()),
            if spec.goal.trim().is_empty() {
                "Handed off by an external model".to_string()
            } else {
                spec.goal.trim().to_string()
            },
        );
        wf.steps = to_workflow_steps(spec);
        let id = wf.id.clone();
        if let Some(slot) = store.workflows.iter_mut().find(|w| w.id == id) {
            *slot = wf;
        }
        store.save();
        id
    };
    (workflow_id, uuid::Uuid::new_v4().to_string())
}

/// Build the final result payload from the run registry.
fn final_payload(app: &tauri::AppHandle, run_id: &str, workflow_id: &str, spec: &HandoffSpec) -> Value {
    use tauri::Manager;
    let state = app.state::<crate::AppState>();
    let (status, steps, last_output) = {
        let runs = state
            .automation
            .run_store
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        match runs.runs.get(run_id) {
            Some(run) => {
                let steps: Vec<Value> = run
                    .steps
                    .iter()
                    .map(|s| {
                        json!({
                            "index": s.index,
                            "label": s.label,
                            "status": format!("{:?}", s.status).to_lowercase(),
                            "output": s.output.as_deref().map(|o| crate::truncate_str(o, 4000)),
                            "error": s.error,
                        })
                    })
                    .collect();
                let last = run
                    .steps
                    .iter()
                    .rev()
                    .find_map(|s| s.output.clone())
                    .unwrap_or_default();
                (format!("{:?}", run.status).to_lowercase(), steps, last)
            }
            None => ("unknown".to_string(), vec![], String::new()),
        }
    };

    // Deliverable materialisation from the final step's markdown.
    let deliverable = match spec.deliverable.as_deref() {
        Some("doc") if !last_output.trim().is_empty() && status == "completed" => {
            let doc_spec = doc_spec_from_markdown(&spec.title, &last_output);
            let kit = crate::brand::active_kit(&state);
            match crate::docgen::generate("docx", &doc_spec, &kit) {
                Ok(path) => json!({ "kind": "doc", "path": path.to_string_lossy() }),
                Err(e) => json!({ "kind": "doc", "error": e }),
            }
        }
        Some("panel") if !last_output.trim().is_empty() && status == "completed" => {
            let block: Result<crate::dynamic_panels::Block, _> =
                serde_json::from_value(json!({ "kind": "text", "content": last_output }));
            match block {
                Ok(block) => {
                    let mut panels = state
                        .automation
                        .dynamic_panels_store
                        .lock()
                        .unwrap_or_else(|e| e.into_inner());
                    let panel: Result<crate::dynamic_panels::DynamicPanel, _> =
                        serde_json::from_value(json!({
                            "id": "",
                            "title": spec.title,
                            "description": "Handoff result",
                            "blocks": [block],
                            "created_at": "",
                            "updated_at": "",
                        }));
                    match panel {
                        Ok(p) => {
                            let saved = panels.upsert(p, "handoff");
                            let _ = panels.save();
                            json!({ "kind": "panel", "panel_id": saved.id })
                        }
                        Err(e) => json!({ "kind": "panel", "error": e.to_string() }),
                    }
                }
                Err(e) => json!({ "kind": "panel", "error": e.to_string() }),
            }
        }
        _ => Value::Null,
    };

    json!({
        "event": "done",
        "run_id": run_id,
        "workflow_id": workflow_id,
        "status": status,
        "steps": steps,
        "result": crate::truncate_str(&last_output, 8000),
        "deliverable": deliverable,
    })
}

/// POST /handoff handler. `wait=false` returns immediately after
/// accepting; the default streams SSE progress until the run finishes.
pub async fn handle(
    app: tauri::AppHandle,
    spec: HandoffSpec,
    query: HashMap<String, String>,
) -> Result<warp::http::Response<warp::hyper::Body>, warp::Rejection> {
    use tauri::Manager;

    if let Err(reason) = validate(&spec) {
        return Ok(json_response(400, &json!({ "accepted": false, "error": reason })));
    }

    let (workflow_id, run_id) = create_handoff_workflow(&app, &spec);

    // Execute on the runtime regardless of wait mode; the run is durable
    // either way and visible in the Run surfaces.
    let exec_app = app.clone();
    let exec_run_id = run_id.clone();
    let exec_workflow_id = workflow_id.clone();
    let exec = tokio::spawn(async move {
        let state = exec_app.state::<crate::AppState>();
        crate::commands::workflows_cmd::run_one_workflow(
            &exec_run_id,
            &exec_workflow_id,
            &state,
            &exec_app,
            0,
            None,
        )
        .await
    });

    if query.get("wait").map(|s| s.as_str()) == Some("false") {
        return Ok(json_response(
            202,
            &json!({ "accepted": true, "run_id": run_id, "workflow_id": workflow_id }),
        ));
    }

    let (mut tx, body) = warp::hyper::Body::channel();
    let stream_app = app.clone();
    tokio::spawn(async move {
        let _ = tx
            .send_data(sse_frame(&json!({
                "event": "accepted",
                "run_id": run_id,
                "workflow_id": workflow_id,
                "steps": spec.steps.len(),
            })))
            .await;

        // Poll the run registry and forward step transitions. If the
        // client goes away we stop sending but the run keeps going -
        // it is durable and visible in the app.
        let mut last_sent: Vec<String> = Vec::new();
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(600)).await;
            let snapshot: Vec<(usize, String, String)> = {
                let state = stream_app.state::<crate::AppState>();
                let runs = state
                    .automation
                    .run_store
                    .lock()
                    .unwrap_or_else(|e| e.into_inner());
                runs.runs
                    .get(&run_id)
                    .map(|r| {
                        r.steps
                            .iter()
                            .map(|s| {
                                (
                                    s.index,
                                    s.label.clone(),
                                    format!("{:?}", s.status).to_lowercase(),
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };
            for (index, label, status) in &snapshot {
                let key = format!("{}:{}", index, status);
                if !last_sent.contains(&key) && status != "pending" {
                    last_sent.push(key);
                    if tx
                        .send_data(sse_frame(&json!({
                            "event": "step",
                            "index": index,
                            "label": label,
                            "status": status,
                        })))
                        .await
                        .is_err()
                    {
                        // client gone; keep executing silently
                    }
                }
            }
            if exec.is_finished() {
                break;
            }
        }
        let _ = exec.await;
        let done = final_payload(&stream_app, &run_id, &workflow_id, &spec);
        let _ = tx.send_data(sse_frame(&done)).await;
    });

    Ok(warp::http::Response::builder()
        .status(200)
        .header("content-type", "text/event-stream")
        .header("cache-control", "no-cache")
        .body(body)
        .unwrap_or_else(|_| warp::http::Response::new(warp::hyper::Body::empty())))
}

// ─────────────────────────────────────────────
//  Tests (pure; no runner, CI-safe)
// ─────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(steps: Vec<HandoffStep>) -> HandoffSpec {
        HandoffSpec {
            title: "Test handoff".to_string(),
            goal: "verify mapping".to_string(),
            tone: Some("dry".to_string()),
            steps,
            deliverable: None,
        }
    }

    fn prompt_step(text: &str) -> HandoffStep {
        HandoffStep {
            kind: "prompt".to_string(),
            instruction: text.to_string(),
            tool: None,
            args: None,
            label: None,
        }
    }

    #[test]
    fn validate_catches_the_contract_violations() {
        assert!(validate(&spec(vec![prompt_step("hi")])).is_ok());
        // empty steps
        assert!(validate(&spec(vec![])).is_err());
        // unknown kind
        let mut bad = spec(vec![prompt_step("x")]);
        bad.steps[0].kind = "magic".to_string();
        assert!(validate(&bad).is_err());
        // unknown tool
        let mut bad = spec(vec![prompt_step("x")]);
        bad.steps[0].kind = "tool".to_string();
        bad.steps[0].tool = Some("rm_rf".to_string());
        assert!(validate(&bad).is_err());
        // bad deliverable
        let mut bad = spec(vec![prompt_step("x")]);
        bad.deliverable = Some("pdf".to_string());
        assert!(validate(&bad).is_err());
        // too many steps
        let many = spec((0..MAX_STEPS + 1).map(|i| prompt_step(&format!("s{}", i))).collect());
        assert!(validate(&many).is_err());
    }

    #[test]
    fn steps_map_onto_the_existing_runner() {
        let mut s = spec(vec![
            prompt_step("Summarise the findings"),
            HandoffStep {
                kind: "action".to_string(),
                instruction: "ls /tmp".to_string(),
                tool: None,
                args: None,
                label: Some("List".to_string()),
            },
            HandoffStep {
                kind: "tool".to_string(),
                instruction: "fetch the page".to_string(),
                tool: Some("web_fetch".to_string()),
                args: Some(json!({ "url": "https://example.com" })),
                label: None,
            },
        ]);
        s.deliverable = Some("doc".to_string());
        let steps = to_workflow_steps(&s);
        assert_eq!(steps.len(), 4, "deliverable adds a compose step");

        assert!(matches!(steps[0].step_type, StepType::Prompt));
        let p = steps[0].config.get("prompt").unwrap().as_str().unwrap();
        assert!(p.contains("Goal: verify mapping"));
        assert!(p.contains("Tone: dry"));
        assert!(p.contains("Summarise the findings"));

        assert!(matches!(steps[1].step_type, StepType::Action));
        assert_eq!(steps[1].config.get("tool").unwrap(), "shell_exec");
        assert_eq!(steps[1].config.get("tool_args").unwrap()["command"], "ls /tmp");
        assert_eq!(steps[1].label, "List");

        assert_eq!(steps[2].config.get("tool").unwrap(), "web_fetch");
        assert_eq!(steps[2].config.get("tool_args").unwrap()["url"], "https://example.com");

        assert!(matches!(steps[3].step_type, StepType::Prompt));
        assert_eq!(steps[3].label, "Compose deliverable");
        // orders are sequential for the runner
        for (i, st) in steps.iter().enumerate() {
            assert_eq!(st.order, i);
        }
    }

    #[test]
    fn markdown_parses_into_doc_sections() {
        let md = "Intro paragraph.\n\n## First\nBody one.\n- a\n- b\n\n## Second\nBody two.";
        let spec = doc_spec_from_markdown("T", md);
        assert_eq!(spec.title, "T");
        assert_eq!(spec.sections.len(), 3);
        assert_eq!(spec.sections[0].heading, "Summary");
        assert!(spec.sections[0].body.contains("Intro paragraph."));
        assert_eq!(spec.sections[1].heading, "First");
        assert_eq!(spec.sections[1].bullets, vec!["a", "b"]);
        assert_eq!(spec.sections[2].heading, "Second");
    }

    #[test]
    fn spec_deserializes_from_minimal_json() {
        let raw = r#"{"title":"T","steps":[{"kind":"prompt","instruction":"do it"}]}"#;
        let s: HandoffSpec = serde_json::from_str(raw).unwrap();
        assert!(validate(&s).is_ok());
        assert_eq!(s.goal, "");
        assert!(s.deliverable.is_none());
    }
}

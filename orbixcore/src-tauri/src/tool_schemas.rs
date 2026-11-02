//! OpenAI-style tool schemas for Bob's action vocabulary (W1.5).
//!
//! llama-server (b8753, --jinja default on) parses these server-side and
//! emits `finish_reason:"tool_calls"` with valid JSON arguments, replacing
//! the brittle `<action>{json}</action>` tag parsing for models whose chat
//! template supports tools. Each `function.name` maps 1:1 to an action
//! `type` that `execute_actions_counted` already handles, and every field
//! matches exactly what that executor reads from the action JSON. The
//! bridge in `bob_agentic` reconstructs `{"type": name, ...arguments}` from
//! a tool call and runs it through the identical executor.

use serde_json::json;

fn f(name: &str, description: &str, params: serde_json::Value) -> serde_json::Value {
    json!({
        "type": "function",
        "function": {
            "name": name,
            "description": description,
            "parameters": params,
        }
    })
}

fn obj(props: serde_json::Value, required: &[&str]) -> serde_json::Value {
    json!({
        "type": "object",
        "properties": props,
        "required": required,
    })
}

/// The full tools array advertised to the local model. Kept to the subset
/// `execute_actions_counted` actually executes, highest-value first.
/// The tools a workflow action step may name. The app-bridge tier is
/// offered only where it can run (`app_bridge::tier_available`: always on
/// macOS, on Windows when a scriptable app such as classic Outlook is
/// installed), so the model never writes a step that cannot run.
pub(crate) fn step_tool_names() -> Vec<&'static str> {
    let mut names = vec![
        "web_search", "web_fetch", "file_read", "file_write", "file_list", "shell_exec",
        // Paired apps, MCP servers and connected services (pair mode).
        "peer_send", "peer_read", "peer_open", "peer_press", "peer_fill", "mcp_call", "api_call",
    ];
    if crate::app_bridge::tier_available() {
        names.push("app_bridge");
    }
    names
}

fn step_tool_description() -> &'static str {
    if !crate::app_bridge::tier_available() {
        "action steps: the tool to run. peer_send/peer_read/peer_open/peer_press/peer_fill act on a PAIRED APP by name; mcp_call runs an MCP server tool; api_call calls a connected service's API."
    } else if cfg!(target_os = "macos") {
        "action steps: the tool to run. app_bridge reads desktop apps (Calendar, Mail, Notes, Reminders, Messages, Outlook, plus any Bob learned). peer_send/peer_read/peer_open/peer_press/peer_fill act on a PAIRED APP by name (ChatGPT, Claude, Slack, ...); mcp_call runs an MCP server tool; api_call calls a connected service's API."
    } else {
        "action steps: the tool to run. app_bridge reads desktop apps (Outlook mail and calendar). peer_send/peer_read/peer_open/peer_press/peer_fill act on a PAIRED APP by name; mcp_call runs an MCP server tool; api_call calls a connected service's API."
    }
}

fn step_tool_args_description() -> &'static str {
    if !crate::app_bridge::tier_available() {
        "arguments for `tool`. web_search: {query}. web_fetch: {url}. file tools: {path} (forward slashes). peer_send: {app, text}. peer_read: {app}. peer_open: {app, action, args}. peer_press: {app, control}. peer_fill: {app, field, value}. mcp_call: {server, tool, args}. api_call: {connector, method, url, body}. Use {{prev}} to pass the previous step's result, e.g. peer_send {\"app\":\"ChatGPT\",\"text\":\"Summarise: {{prev}}\"}."
    } else if cfg!(target_os = "macos") {
        "arguments for `tool`. web_search: {query}. web_fetch: {url}. file tools: {path}. app_bridge: {app, action, args} e.g. {\"app\":\"Calendar\",\"action\":\"upcoming_events\",\"args\":{\"days\":1}}. peer_send: {app, text}. peer_read: {app}. peer_open: {app, action, args}. peer_press: {app, control}. peer_fill: {app, field, value}. mcp_call: {server, tool, args}. api_call: {connector, method, url, body}. Use {{prev}} to pass the previous step's result, e.g. peer_send {\"app\":\"ChatGPT\",\"text\":\"Summarise: {{prev}}\"}."
    } else {
        "arguments for `tool`. web_search: {query}. web_fetch: {url}. file tools: {path} (forward slashes). app_bridge: {app, action, args} e.g. {\"app\":\"Outlook\",\"action\":\"upcoming_events\",\"args\":{\"days\":1}}. peer_send: {app, text}. peer_read: {app}. peer_open: {app, action, args}. peer_press: {app, control}. peer_fill: {app, field, value}. mcp_call: {server, tool, args}. api_call: {connector, method, url, body}. Use {{prev}} to pass the previous step's result."
    }
}

/// What app_bridge reads, in the words of the host platform.
fn app_bridge_description() -> &'static str {
    if cfg!(target_os = "macos") {
        "Read data from a Mac app the user is already logged into (Outlook, Mail, Calendar, Notes, Reminders, Messages) through its scripting interface. No OAuth, no connector setup. Call list_app_bridges first when unsure which apps or actions exist."
    } else {
        "Read data from a desktop app the user is already logged into (Outlook mail and calendar) through its scripting interface. No OAuth, no connector setup. Call list_app_bridges first when unsure which apps or actions exist."
    }
}

fn app_bridge_app_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "app name, e.g. Outlook, Mail, Calendar, Notes, Reminders, Messages"
    } else {
        "app name, e.g. Outlook"
    }
}

fn app_bridge_action_hint() -> &'static str {
    if cfg!(target_os = "macos") {
        "action id, e.g. recent_messages, unread_count, upcoming_events, recent_notes, open_reminders"
    } else {
        "action id, e.g. recent_messages, unread_count, search_messages, upcoming_events, today_events"
    }
}

pub(crate) fn bob_tools() -> serde_json::Value {
    let s = |desc: &str| json!({"type": "string", "description": desc});
    let step = json!({
        "type": "object",
        "description": "Every step must be RUNNABLE, not just a label. prompt steps: include the full `prompt` text. action steps: include `tool`+`tool_args`, or a connector_id/command. Steps without substance are rejected.",
        "properties": {
            "type": {"type": "string", "enum": ["prompt", "action"]},
            "label": {"type": "string"},
            "prompt": {"type": "string", "description": "prompt steps: the FULL instruction the model runs, written out"},
            "tool": {"type": "string",
                     "enum": step_tool_names(),
                     "description": step_tool_description()},
            "tool_args": {"type": "object",
                          "description": step_tool_args_description()},
            "command": {"type": "string", "description": "action steps: a shell command, when no tool fits"},
            "connector_id": {"type": "string"},
        },
        "required": ["type", "label"],
    });

    let tools = json!([
        f("create_workflow",
          "Create a new automation workflow from an ordered list of steps. ONLY for processes the user wants to REPEAT (scheduled posts, fetch-map-write pipelines, backups). NEVER create a workflow to answer a question the user just asked - fetch with tools and answer directly. Workflows run unattended: a step that asks the user for information is invalid; steps must gather data with tools (web_search, web_fetch, files, shell). Use create_agent instead for a standing responsibility that needs judgment each run.",
          obj(json!({
              "name": s("workflow name"),
              "description": s("what the workflow does"),
              "run": {"type": "boolean", "description": "run immediately after creating"},
              "schedule": s("optional recurrence: a cron string like '0 9 * * *' or a keyword: daily, hourly, weekly, weekdays, monthly, every5minutes"),
              "steps": {"type": "array", "items": step},
          }), &["name"])),
        f("run_workflow", "Run an existing workflow by name.",
          obj(json!({"name": s("workflow name")}), &["name"])),
        f("create_panel",
          "Build a live UI panel the user opens from My UI. Use for 'build me a panel/dashboard/view showing X'. Blocks render in order; fill them with REAL data you gathered first (web_search etc.), never placeholders.",
          obj(json!({
              "title": s("panel title"),
              "description": s("one line on what the panel shows"),
              "blocks": {"type": "array", "description": "ordered blocks", "items": {
                  "type": "object",
                  "properties": {
                      "kind": {"type": "string", "enum": ["header", "text", "metric", "list", "table", "divider"]},
                      "text": {"type": "string", "description": "header: heading text"},
                      "content": {"type": "string", "description": "text: markdown body"},
                      "label": {"type": "string", "description": "metric: label"},
                      "value": {"type": "string", "description": "metric: value"},
                      "items": {"type": "array", "items": {"type": "string"}, "description": "list: entries"},
                      "columns": {"type": "array", "items": {"type": "object"}, "description": "table: [{key,label}]"},
                      "rows": {"type": "array", "items": {"type": "object"}, "description": "table: row objects keyed by column key"},
                  },
                  "required": ["kind"],
              }},
          }), &["title", "blocks"])),
        f("extract_brand_kit",
          "Extract a brand kit (colors, fonts, tone of voice) from a brand guideline file (PDF, Markdown, text, or HTML). The kit becomes active and styles every generated document.",
          obj(json!({ "path": s("absolute path to the guideline file") }), &["path"])),
        f("use_skill",
          "Load a user-installed skill (from the workspace's skills folder) and follow its instructions for the rest of the task. Call it FIRST when the request matches an installed skill's description, or when the user writes [skill: name].",
          obj(json!({ "name": s("the skill's name from the SKILLS index") }), &["name"])),
        f("research",
          "Deep research on a topic: plans several diverse search angles, runs them all, reads the top pages in full, and returns a source-numbered dossier. Use BEFORE writing anything substantive (documents, websites, briefs, comparisons) and whenever the user says research, go deep, or be thorough. Write from the dossier and cite its numbered sources where it matters.",
          obj(json!({
              "topic": s("the topic to research, stated plainly"),
          }), &["topic"])),
        f("create_document",
          "Create a real, branded document in the user's Documents/Orbix Documents folder and open it. kind 'docx' = Word, 'pptx' = PowerPoint deck (one slide per section), 'html' = styled article-style web page, 'website' = a complete website folder (index.html + styles.css) with nav, hero, and feature cards - use it whenever the user asks for a website, landing page, or site. kind 'app' = an INTERACTIVE single-page web app written in full (HTML + CSS + JS): explorers, calculators, games, quizzes, simulations, dashboards, demos - use it whenever the user wants to click, explore, play, or calculate; put everything it should do and show in `brief` and skip sections. Styled with the active brand kit. For the other kinds provide 4-7 sections with substantive REAL content: 2-4 sentences of body per section plus 3-5 bullets where useful - never placeholders, never one-line stubs. For 'website', write bullets as 'Benefit: short supporting phrase' - they render as feature cards.",
          obj(json!({
              "kind": {"type": "string", "enum": ["docx", "pptx", "html", "website", "app"]},
              "title": s("document title"),
              "subtitle": s("optional subtitle"),
              "brief": s("for kind 'app': what is on screen, what each interaction does, and the data to include (e.g. the eight planets with distance, size, facts)"),
              "sections": {"type": "array", "items": {
                  "type": "object",
                  "properties": {
                      "heading": s("section heading / slide title"),
                      "body": s("paragraph text; separate paragraphs with a blank line"),
                      "bullets": {"type": "array", "items": {"type": "string"}},
                  },
                  "required": ["heading"],
              }},
          }), &["kind", "title"])),
        f("update_panel",
          "Update an existing My UI panel by id (title, description, blocks).",
          obj(json!({
              "id": s("panel id"),
              "title": s("new title (optional)"),
              "blocks": {"type": "array", "items": {"type": "object"}, "description": "replacement blocks (optional)"},
          }), &["id"])),
        f("update_workflow", "Change an existing workflow: rename it, re-describe it, or replace its steps (the user says 'change step 2 to...', 'add a step that posts to Slack', 'make it only weekdays'). Pass the FULL new list of steps when changing any of them; each step must be runnable like in create_workflow.",
          obj(json!({
              "name": s("current workflow name"),
              "new_name": s("new name (optional)"),
              "description": s("new description (optional)"),
              "steps": {"type": "array", "items": step, "description": "the complete replacement step list (optional)"},
          }), &["name"])),
        f("delete_workflow", "Delete a workflow by name.",
          obj(json!({"name": s("workflow name")}), &["name"])),
        f("stop_workflow", "Stop the currently running workflow.",
          obj(json!({}), &[])),
        f("create_agent",
          "Hire an agent: an employee with a standing job (charter) that works shifts using granted tools. Use an AGENT when each run needs judgment ('watch my inbox', 'keep this repo's tests green'); use create_workflow for fixed repeatable steps. For a code agent on a repo, put the absolute repo path in the charter and grant ['tool'] (files + shell + web). Local agents run on-device on the local model.",
          obj(json!({
              "name": s("agent name"),
              "charter": s("standing job description - what this agent is responsible for, including any repo/folder paths it works in. Required for the agent to actually work shifts."),
              "tools": {"type": "array", "items": {"type": "string"},
                        "description": "tool names this agent may use during shifts, e.g. ['tool', 'search_memory', 'add_memory']. 'tool' = files/shell/web toolbox. Omit for safe read-only defaults."},
              "every_minutes": {"type": "integer", "description": "work a shift automatically every N minutes; omit for on-demand shifts"},
              "run": {"type": "boolean", "description": "work the first shift immediately after hiring"},
              "provider": {"type": "string", "enum": ["local", "anthropic"],
                           "description": "local (default) or anthropic"},
              "model": s("local model id from the catalog (e.g. gemma-4-e4b, qwen-3.5-9b); omit to use the current model"),
          }), &["name"])),
        f("update_agent", "Update an existing agent by name.",
          obj(json!({"name": s("agent name"), "model": s("new model id")}), &["name"])),
        f("delete_agent", "Delete an agent by name.",
          obj(json!({"name": s("agent name")}), &["name"])),
        f("toggle_agent", "Enable or disable an agent by name.",
          obj(json!({"name": s("agent name")}), &["name"])),
        f("run_chain", "Run an orchestrator agent chain by name with an input.",
          obj(json!({"name": s("chain name"), "input": s("input text")}), &["name"])),
        f("tool",
          "Run a built-in tool. web_search searches the live web (use for \
           news, current events, latest/recent info, prices, weather, or \
           anything not in your training data); shell_exec runs a shell \
           command; file_read/file_write/file_list act on paths; file_edit \
           replaces an exact unique snippet (prefer over file_write for \
           changes); web_fetch fetches a specific URL.",
          obj(json!({
              "tool": {"type": "string",
                       "enum": ["web_search", "shell_exec", "file_read", "file_write", "file_edit", "file_list", "web_fetch"]},
              "args": {
                  "type": "object",
                  "properties": {
                      "query": s("search text for web_search"),
                      "command": s("shell command for shell_exec"),
                      "cwd": s("folder to run shell_exec in (absolute path); defaults to the user's workspace"),
                      "path": s("file or directory path"),
                      "content": s("content for file_write"),
                      "old_string": s("exact text to replace for file_edit"),
                      "new_string": s("replacement text for file_edit"),
                      "url": s("url for web_fetch"),
                  },
              },
          }), &["tool", "args"])),
        f("api_call", "Make an HTTP request. With `connector`, call a connected service's API as the user: Orbix adds their token, and `url` may be a path relative to that service's API base (e.g. connector \"slack\", url \"/conversations.list\"). The prompt's CONNECTED SERVICES block lists what is connected and where to start.",
          obj(json!({
              "method": {"type": "string", "enum": ["GET", "POST", "PUT", "PATCH", "DELETE"]},
              "url": s("request URL, or a path relative to the connector's API base"),
              "body": {"type": "object", "description": "JSON body for POST/PUT/PATCH"},
              "connector": s("optional connected service: microsoft, google, github, slack, linkedin"),
          }), &["method", "url"])),
        f("add_memory",
          "Store something the user asked you to keep for later: a fact about them, a preference, how they like things done, or context about their work. It persists across conversations. Call it whenever the user says remember, note that, keep in mind, or states a lasting preference; one call per item.",
          obj(json!({
              "key": s("short key"),
              "value": s("the thing to remember"),
              "category": {"type": "string", "enum": ["fact", "preference", "context", "skill"]},
          }), &["key", "value"])),
        f("search_memory", "Search remembered items.",
          obj(json!({"query": s("search text")}), &["query"])),
        f("delete_memory", "Forget memories by key.",
          obj(json!({"key": s("key to forget")}), &["key"])),
        f("connect_oauth",
          "Connect an external service account (Microsoft, Google, GitHub, Slack, LinkedIn) through OAuth. Opens the sign-in flow for the user; use when they ask to connect a service.",
          obj(json!({
              "service": {"type": "string", "enum": ["microsoft", "google", "github", "slack", "linkedin"],
                          "description": "the service to connect"},
          }), &["service"])),
        f("check_connectors", "Show which external services are connected and which are available to connect.",
          obj(json!({}), &[])),
        f("list_connectors", "List connected services.", obj(json!({}), &[])),
        f("list_triggers", "List event triggers.", obj(json!({}), &[])),
        f("recent_activity", "Show recent agent activity.", obj(json!({}), &[])),
        f("list_notifications", "List notifications.", obj(json!({}), &[])),
        f("send_email",
          "Send an email through the user's connected mail account (Gmail or Outlook).",
          obj(json!({
              "to": s("recipient address(es), comma separated"),
              "subject": s("subject line"),
              "body": s("email body (HTML allowed)"),
          }), &["to", "subject", "body"])),
        f("create_calendar_event",
          "Create a calendar event on the user's connected calendar.",
          obj(json!({
              "subject": s("event title"),
              "start": s("start time, RFC3339 e.g. 2026-07-06T09:00:00Z"),
              "end": s("end time, RFC3339"),
              "attendees": {"type": "array", "items": {"type": "string"}, "description": "attendee email addresses"},
          }), &["subject", "start", "end"])),
        f("list_calendar_events", "List the user's upcoming calendar events.",
          obj(json!({}), &[])),
        f("browse_navigate", "Open a URL in the user's Chrome (must be running with remote debugging).",
          obj(json!({"url": s("the URL to open")}), &["url"])),
        f("browse_click", "Click an element in the browser by CSS selector.",
          obj(json!({"selector": s("CSS selector")}), &["selector"])),
        f("browse_type", "Type text into an input/textarea by CSS selector.",
          obj(json!({"selector": s("CSS selector"), "text": s("text to enter")}), &["selector", "text"])),
        f("browse_read", "Read the visible text of the page or an element by CSS selector.",
          obj(json!({"selector": s("optional CSS selector; omit for whole page")}), &[])),
        f("list_app_bridges",
          "List the desktop apps Bob can read directly through their scripting interface (no account setup): install/run/scriptable status per app plus its available actions.",
          obj(json!({}), &[])),
        f("app_bridge",
          app_bridge_description(),
          obj(json!({
              "app": s(app_bridge_app_hint()),
              "action": s(app_bridge_action_hint()),
              "args": {"type": "object", "description": "action arguments, e.g. {\"limit\": 10} or {\"days\": 7} or {\"query\": \"...\"}"},
          }), &["app", "action"])),
        f("read_app_window",
          "Read the visible content of any running app's focused window (labels, values, list rows) through the OS accessibility tree (Accessibility on macOS, UI Automation on Windows). Use it for whatever is on the user's screen inside a desktop app when no app bridge covers it; browser pages read better via browse_read.",
          obj(json!({
              "app": s("running app name, e.g. Notion, Slack, Preview"),
          }), &["app"])),
        f("pair_app",
          "Pair Orbix with a running desktop app so Bob can work with it (the same as the user dragging the pair handle onto its window). Bob probes what the app allows: exposing Bob's tools inside it (Claude, Cursor), typing into it and reading it (any app), or clipboard only. Returns what worked in plain language. Call once per app; paired apps are listed in the prompt.",
          obj(json!({
              "app": s("running app name, e.g. ChatGPT, Claude, Numbers"),
          }), &["app"])),
        f("list_paired_apps",
          "List the apps paired with Orbix and what Bob can do with each (send, read, calls Bob's tools).",
          obj(json!({}), &[])),
        f("peer_send",
          "Put text into a paired app: type it into the app's message input and send it, or copy it for the user to paste when the app allows nothing more. Use it to hand a question, draft, or result to ChatGPT, Claude, or any paired app.",
          obj(json!({
              "app": s("paired app name"),
              "text": s("the text to send"),
          }), &["app", "text"])),
        f("peer_read",
          "Read what a paired app currently shows in its front window (its reply, the document, the list). Returns the window text.",
          obj(json!({
              "app": s("paired app name"),
          }), &["app"])),
        f("unpair_app",
          "Remove a paired app and undo anything Bob set up in it.",
          obj(json!({
              "app": s("paired app name"),
          }), &["app"])),
        f("peer_open",
          "Open or create something in a paired app through one of its link actions (URL scheme): open a Slack channel, start a Teams chat with a message, make an Obsidian note, join a Zoom meeting, open a file in VS Code. The prompt lists each paired app's actions and their args under PAIRED APPS.",
          obj(json!({
              "app": s("paired app name"),
              "action": s("action id from the paired-apps list, e.g. open_channel"),
              "args": {"type": "object", "description": "the action's args, e.g. {\"team\": \"T123\", \"channel\": \"C456\"}"},
          }), &["app", "action"])),
        f("add_url_action",
          "Teach a paired app a link action from its URL scheme, when the user tells you one or you know the app's deep-link format. Template uses {{arg}} placeholders, e.g. obsidian://new?vault={{vault}}&name={{name}}. Stored untested until it opens cleanly.",
          obj(json!({
              "app": s("paired app name"),
              "id": s("snake_case action id"),
              "description": s("what it does"),
              "template": s("URL with {{arg}} placeholders; scheme must be one the app registered"),
          }), &["app", "id", "description", "template"])),
        f("peer_controls",
          "List the buttons and fields in a paired app's front window (fresh), so peer_press and peer_fill can use them by label.",
          obj(json!({ "app": s("paired app name") }), &["app"])),
        f("peer_press",
          "Press a button in a paired app's front window by its label (e.g. New chat, Search, Save). Use peer_controls first when unsure of labels.",
          obj(json!({ "app": s("paired app name"), "control": s("button label or id") }), &["app", "control"])),
        f("peer_fill",
          "Fill a field in a paired app's front window by its label, e.g. a search box or a form field. For the message composer use peer_send instead.",
          obj(json!({ "app": s("paired app name"), "field": s("field label or id"), "value": s("text to put in it") }), &["app", "field", "value"])),
        f("mcp_tools",
          "List the tools an MCP server offers (starts it if needed). Servers come from the MCP panel or from pairing an app that publishes one; the prompt's MCP SERVERS block names them.",
          obj(json!({
              "server": s("server name from the MCP SERVERS block"),
          }), &["server"])),
        f("mcp_call",
          "Run one tool on an MCP server with JSON arguments matching its input schema (see mcp_tools). Returns the tool's text.",
          obj(json!({
              "server": s("server name"),
              "tool": s("tool name from mcp_tools"),
              "args": {"type": "object", "description": "arguments per the tool's input schema"},
          }), &["server", "tool"])),
        f("learn_app",
          "Teach Bob to read a scriptable Mac app directly: reads the app's scripting dictionary, drafts a read-only bridge, tests it against the running app, and activates it so app_bridge can use it. Takes a minute. Use when the user wants Bob to work with an app that has no bridge yet.",
          obj(json!({
              "app": s("running app name, e.g. Music, Numbers, Safari"),
          }), &["app"])),
        f("run_user_action",
          "Do a step that was waiting on the user, once the user agrees: restart an app so it sees Bob's tools, open the Accessibility settings, or re-check a paired app. The pending steps are listed in the prompt under WAITING ON THE USER. Never run one the user did not ask for.",
          obj(json!({
              "id": s("the pending step's id, or the app it is about (e.g. Claude)"),
          }), &["id"])),
        f("create_connector",
          "Build a new connector to an external API from a description, then activate it.",
          obj(json!({
              "request": s("what the connector should do, e.g. 'send messages via the Acme API'"),
              "vendor": s("the service/vendor name"),
          }), &["request"])),
    ]);

    // The bridge tier is advertised only where it can read something.
    // Advertising it anyway sends the model down a dead path (field
    // failure 2026-09-16: a calendar question went to app_bridge, returned
    // nothing, and Bob told the user to 'connect their calendar' while
    // list_calendar_events sat unused). On Windows that still holds for a
    // PC with only the new Outlook, which has no scripting interface, so
    // the check is the device's real state, not the OS.
    let mut tools = tools;
    if !crate::app_bridge::tier_available() {
        if let Some(arr) = tools.as_array_mut() {
            arr.retain(|t| {
                let name = t["function"]["name"].as_str().unwrap_or("");
                name != "app_bridge" && name != "list_app_bridges"
            });
        }
    }
    tools
}

/// True if a reconstructed tool-call name is one Bob's executor handles.
/// Guards against a model inventing a tool the executor would silently drop.
pub(crate) fn is_known_tool(name: &str) -> bool {
    matches!(
        name,
        "create_workflow" | "run_workflow" | "update_workflow" | "delete_workflow"
            | "stop_workflow" | "create_agent" | "update_agent" | "delete_agent"
            | "toggle_agent" | "run_chain" | "tool" | "api_call" | "add_memory"
            | "search_memory" | "delete_memory" | "list_connectors" | "list_triggers"
            | "connect_oauth" | "check_connectors"
            | "recent_activity" | "list_notifications"
            | "send_email" | "create_calendar_event" | "list_calendar_events"
            | "browse_navigate" | "browse_click" | "browse_type" | "browse_read"
            | "list_app_bridges" | "app_bridge" | "read_app_window"
            | "create_connector" | "create_panel" | "update_panel"
            | "extract_brand_kit" | "create_document" | "research" | "use_skill"
            | "pair_app" | "list_paired_apps" | "peer_send" | "peer_read" | "unpair_app"
            | "run_user_action" | "learn_app" | "peer_open" | "add_url_action"
            | "mcp_tools" | "mcp_call" | "peer_controls" | "peer_press" | "peer_fill"
    )
}

/// Tools that reach past this device and the user's own Orbix setup: they
/// send, post, submit, or connect to other people and services. A run the
/// user did not script step by step (Imagine) never advertises them.
pub(crate) fn is_outward_tool(name: &str) -> bool {
    matches!(
        name,
        "send_email" | "create_calendar_event" | "api_call" | "connect_oauth"
            | "create_connector" | "browse_navigate" | "browse_click" | "browse_type"
            | "peer_send" | "pair_app" | "peer_open" | "mcp_call" | "peer_press" | "peer_fill"
    )
}

/// Tools that remove, stop, or switch off what the user built, or fire the
/// user's own automations (which may send on their behalf).
pub(crate) fn is_disruptive_tool(name: &str) -> bool {
    matches!(
        name,
        "delete_workflow" | "delete_agent" | "delete_memory" | "stop_workflow"
            | "toggle_agent" | "run_workflow" | "run_chain"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tools_are_well_formed() {
        let tools = bob_tools();
        let arr = tools.as_array().unwrap();
        assert!(arr.len() >= 15);
        for t in arr {
            let name = t["function"]["name"].as_str().unwrap();
            assert!(is_known_tool(name), "advertised tool not known: {}", name);
            assert_eq!(t["type"], "function");
            assert!(t["function"]["parameters"]["type"] == "object");
        }
    }

    /// Every array-typed parameter must declare `items`. The gpt-oss
    /// (harmony) chat template in llama.cpp renders tool schemas with a
    /// jinja loop that evaluates `param['items']` in a boolean context;
    /// an array with no `items` makes minja resolve the dict's `.items`
    /// method instead of a value and the whole request 500s ("Function
    /// is not a bool value"), breaking the agentic path for gpt-oss.
    #[test]
    fn every_array_param_declares_items() {
        fn walk(node: &serde_json::Value, path: &str, bad: &mut Vec<String>) {
            match node {
                serde_json::Value::Object(map) => {
                    if map.get("type").and_then(|t| t.as_str()) == Some("array")
                        && !map.contains_key("items")
                    {
                        bad.push(path.to_string());
                    }
                    for (k, v) in map {
                        walk(v, &format!("{}.{}", path, k), bad);
                    }
                }
                serde_json::Value::Array(items) => {
                    for (i, v) in items.iter().enumerate() {
                        walk(v, &format!("{}[{}]", path, i), bad);
                    }
                }
                _ => {}
            }
        }
        let tools = bob_tools();
        let mut bad = Vec::new();
        for t in tools.as_array().unwrap() {
            let name = t["function"]["name"].as_str().unwrap_or("?");
            walk(&t["function"]["parameters"], name, &mut bad);
        }
        assert!(bad.is_empty(), "array params missing `items` (gpt-oss template will 500): {:?}", bad);
    }
}

#[cfg(test)]
mod bridge_tests {
    use crate::llm_engine::reconstruct_tool_call;

    /// Mirror of the streaming accumulator: fold OpenAI tool_calls deltas
    /// (index-keyed, id/name once then arguments in fragments) exactly as
    /// call_local_llm_streaming_tools does, so the fixtures exercise the
    /// same logic. Returns reconstructed action objects.
    fn accumulate(deltas: &[serde_json::Value]) -> Vec<serde_json::Value> {
        use std::collections::BTreeMap;
        let mut acc: BTreeMap<u64, (String, String, String)> = BTreeMap::new();
        for delta in deltas {
            let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) else { continue };
            for call in calls {
                let idx = call.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                let e = acc.entry(idx).or_default();
                if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                    if !id.is_empty() { e.0 = id.to_string(); }
                }
                if let Some(func) = call.get("function") {
                    if let Some(n) = func.get("name").and_then(|v| v.as_str()) {
                        if !n.is_empty() { e.1 = n.to_string(); }
                    }
                    if let Some(a) = func.get("arguments").and_then(|v| v.as_str()) {
                        e.2.push_str(a);
                    }
                }
            }
        }
        acc.into_values()
            .filter_map(|(_id, name, args)| reconstruct_tool_call(&name, &args))
            .collect()
    }

    #[test]
    fn fragmented_arguments_accumulate() {
        // arguments arrive split across four chunks, name only in the first
        let deltas = vec![
            serde_json::json!({"tool_calls": [{"index": 0, "id": "abc", "function": {"name": "create_workflow", "arguments": ""}}]}),
            serde_json::json!({"tool_calls": [{"index": 0, "function": {"arguments": "{\"name\":\"Invoice"}}]}),
            serde_json::json!({"tool_calls": [{"index": 0, "function": {"arguments": " Chase\",\"run\":"}}]}),
            serde_json::json!({"tool_calls": [{"index": 0, "function": {"arguments": "true}"}}]}),
        ];
        let out = accumulate(&deltas);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["type"], "create_workflow");
        assert_eq!(out[0]["name"], "Invoice Chase");
        assert_eq!(out[0]["run"], true);
    }

    #[test]
    fn multiple_parallel_calls() {
        let deltas = vec![
            serde_json::json!({"tool_calls": [{"index": 0, "id": "a", "function": {"name": "create_agent", "arguments": "{\"name\":\"A\"}"}}]}),
            serde_json::json!({"tool_calls": [{"index": 1, "id": "b", "function": {"name": "run_workflow", "arguments": "{\"name\":\"W\"}"}}]}),
        ];
        let out = accumulate(&deltas);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["type"], "create_agent");
        assert_eq!(out[1]["type"], "run_workflow");
    }

    #[test]
    fn unknown_tool_dropped() {
        let deltas = vec![
            serde_json::json!({"tool_calls": [{"index": 0, "id": "x", "function": {"name": "rm_rf_everything", "arguments": "{}"}}]}),
        ];
        assert!(accumulate(&deltas).is_empty());
    }

    #[test]
    fn malformed_args_repaired() {
        // trailing comma - repair_json fixes it
        let out = reconstruct_tool_call("add_memory", "{\"key\":\"k\",\"value\":\"v\",}");
        assert!(out.is_some());
        assert_eq!(out.unwrap()["value"], "v");
    }

    #[test]
    fn tool_action_shape_matches_executor() {
        // the executor reads action["tool"] and action["args"]; ensure a
        // native `tool` call reconstructs with those exact keys
        let out = reconstruct_tool_call(
            "tool",
            "{\"tool\":\"file_read\",\"args\":{\"path\":\"/tmp/x\"}}",
        ).unwrap();
        assert_eq!(out["type"], "tool");
        assert_eq!(out["tool"], "file_read");
        assert_eq!(out["args"]["path"], "/tmp/x");
    }
}

#[cfg(test)]
mod unwrap_tests {
    use crate::llm_engine::unwrap_tool_wrapped_action;
    use serde_json::json;

    #[test]
    fn wrapped_app_bridge_is_unwrapped_to_first_class() {
        // The exact failure: model emitted a first-class action inside the
        // generic tool envelope, which hit "Unknown tool: app_bridge".
        let out = unwrap_tool_wrapped_action(json!({
            "type": "tool",
            "tool": "app_bridge",
            "args": { "app": "Outlook", "action": "recent_messages", "args": { "limit": 10 } }
        }));
        assert_eq!(out["type"], "app_bridge");
        assert_eq!(out["app"], "Outlook");
        assert_eq!(out["action"], "recent_messages");
        assert_eq!(out["args"]["limit"], 10);
    }

    #[test]
    fn wrapped_read_app_window_and_list_bridges_unwrap() {
        let a = unwrap_tool_wrapped_action(json!({"type":"tool","tool":"read_app_window","args":{"app":"Notion"}}));
        assert_eq!(a["type"], "read_app_window");
        assert_eq!(a["app"], "Notion");
        let b = unwrap_tool_wrapped_action(json!({"type":"tool","tool":"list_app_bridges"}));
        assert_eq!(b["type"], "list_app_bridges");
    }

    #[test]
    fn builtin_tools_stay_wrapped() {
        // shell_exec/file_read must keep going through tools::execute_tool.
        for t in ["shell_exec", "file_read", "file_write", "file_list", "web_fetch"] {
            let out = unwrap_tool_wrapped_action(json!({
                "type": "tool", "tool": t, "args": { "path": "/tmp/x" }
            }));
            assert_eq!(out["type"], "tool", "{t} must stay wrapped");
            assert_eq!(out["tool"], t);
        }
    }

    #[test]
    fn unknown_wrapped_tool_left_untouched() {
        // Not a first-class action and not a builtin: leave it so the
        // existing "Unknown tool" path still reports it.
        let out = unwrap_tool_wrapped_action(json!({"type":"tool","tool":"rm_rf","args":{}}));
        assert_eq!(out["type"], "tool");
        assert_eq!(out["tool"], "rm_rf");
    }

    #[test]
    fn first_class_actions_pass_through_unchanged() {
        let out = unwrap_tool_wrapped_action(json!({"type":"app_bridge","app":"Mail","action":"unread_count"}));
        assert_eq!(out["type"], "app_bridge");
        assert_eq!(out["app"], "Mail");
    }
}

// Live acceptance harness (W1.5). Ignored by default; run explicitly with a
// llama-server on port 8899 and the E4B model loaded:
//   cargo test --bin orbixcore -- --ignored native_tool_acceptance --nocapture
#[cfg(test)]
mod live_acceptance {
    use super::*;
    use crate::llm_engine::reconstruct_tool_call;

    #[tokio::test]
    #[ignore]
    async fn native_tool_acceptance() {
        let prompts = [
            "Create a workflow named Invoice Chase that runs daily.",
            "Make a workflow called Morning Briefing.",
            "Set up an automation named Lead Follow Up.",
            "Create a workflow to back up my desktop nightly.",
            "Build a workflow named Weekly Report.",
            "Create an agent called Research Assistant.",
            "Make a new local agent named Scheduler.",
            "Add an agent for summarizing emails.",
            "Run the workflow named Invoice Chase.",
            "Delete the workflow called Old Cleanup.",
            "Disable the agent named Scheduler.",
            "Remember that my timezone is Pacific.",
            "Remember my company is Orbix.",
            "Search my memory for timezone.",
            "List my connected services.",
            "Show me recent activity.",
            "Read the file at /etc/hostname.",
            "List the files in /tmp.",
            "Rename the workflow Weekly Report to Monthly Report.",
            "Create an agent named Analyst using gemma-4-e4b.",
        ];
        let client = reqwest::Client::new();
        let url = "http://127.0.0.1:8899/v1/chat/completions";
        let tools = bob_tools();
        let mut valid = 0usize;
        for (i, p) in prompts.iter().enumerate() {
            let body = serde_json::json!({
                "messages": [
                    {"role": "system", "content": "You are Bob. Use the provided tools to act on the user's request. Prefer calling a tool over describing what you would do."},
                    {"role": "user", "content": p}
                ],
                "tools": tools,
                "tool_choice": "auto",
                "temperature": 0.4,
                "max_tokens": 400,
            });
            let resp = client.post(url).json(&body).send().await
                .expect("server on :8899 (load E4B first)");
            let v: serde_json::Value = resp.json().await.unwrap();
            let calls = v["choices"][0]["message"]["tool_calls"].as_array();
            let mut ok = false;
            if let Some(calls) = calls {
                for c in calls {
                    let name = c["function"]["name"].as_str().unwrap_or("");
                    let args = c["function"]["arguments"].as_str().unwrap_or("");
                    if reconstruct_tool_call(name, args).is_some() {
                        ok = true;
                    }
                }
            }
            if ok { valid += 1; } else {
                eprintln!("[{}] NO valid tool call for: {}", i, p);
            }
        }
        eprintln!("ACCEPTANCE: {}/{} prompts produced a valid tool call", valid, prompts.len());
        assert!(valid >= 18, "only {}/20 valid (need >=18)", valid);
    }
}

#[cfg(test)]
mod dump {
    #[test]
    #[ignore]
    fn dump_tools_json() {
        std::fs::write(
            "/tmp/bob_tools.json",
            serde_json::to_string_pretty(&super::bob_tools()).unwrap(),
        )
        .unwrap();
    }
}

#[cfg(test)]
mod schema_tests {
    use super::*;

    /// Prints the tools array so external acceptance scripts consume the
    /// exact schemas the app ships (single source of truth).
    #[test]
    fn dump_tools_json() {
        if std::env::var("ORBIX_DUMP_TOOLS").is_err() {
            return;
        }
        println!("{}", serde_json::to_string(&bob_tools()).unwrap());
    }

    #[test]
    fn all_tools_have_valid_names() {
        let tools = bob_tools();
        for t in tools.as_array().unwrap() {
            let name = t["function"]["name"].as_str().unwrap();
            assert!(is_known_tool(name), "unknown tool in schema: {}", name);
        }
    }

    /// The policy lists name real tools; a typo there would silently let
    /// a sending tool through to an unscripted run.
    #[test]
    fn policy_lists_name_known_tools() {
        let names = [
            "send_email", "create_calendar_event", "api_call", "connect_oauth",
            "create_connector", "browse_navigate", "browse_click", "browse_type",
            "delete_workflow", "delete_agent", "delete_memory", "stop_workflow",
            "toggle_agent", "run_workflow", "run_chain",
        ];
        for n in names {
            assert!(is_known_tool(n), "policy names unknown tool {}", n);
            assert!(is_outward_tool(n) || is_disruptive_tool(n));
        }
        assert!(!is_outward_tool("create_document"));
        assert!(!is_disruptive_tool("create_workflow"));
    }
}

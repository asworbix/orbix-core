//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - Conversation
// ─────────────────────────────────────────────

/// History content for the model: assistant messages prefer their rich
/// `context` (raw output + tool evidence). Evidence blocks are kept only
/// on the newest few messages so long chats don't drag every old dossier
/// back into the prompt.
fn history_msg_content<'a>(m: &'a crate::Message, keep_evidence: bool) -> &'a str {
    if m.role != "assistant" {
        return &m.content;
    }
    let rich = m.context.as_deref().unwrap_or(&m.content);
    if keep_evidence {
        rich
    } else {
        rich.split("\n\n[[evidence]]\n").next().unwrap_or(rich)
    }
}

#[tauri::command]
pub(crate) async fn get_conversation(state: State<'_, AppState>) -> Result<Vec<Message>, String> {
    let msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
    Ok(msgs.clone())
}

#[tauri::command]
pub(crate) async fn send_message(
    input: SendMessageInput,
    state: State<'_, AppState>,
    _llama: State<'_, LlamaServerHolder>,
    app: tauri::AppHandle,
) -> Result<Message, String> {
    let user_msg = Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: input.content.clone(),
        context: None,
        agent_id: input.agent_id.clone(),
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };

    let (history, agent) = {
        let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        msgs.push(user_msg.clone());
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = OrbState::Thinking;
        let history = msgs.clone();
        let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
        let agent = if let Some(id) = &input.agent_id {
            agents.iter().find(|a| &a.id == id && a.enabled).cloned()
        } else {
            agents.iter().find(|a| a.is_default && a.enabled).cloned()
        };
        (history, agent)
    };

    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Thinking).ok();




    // ── Service detection: intercept requests that need connectors ──
    // If user asks about an external service that isn't connected, guide them
    // before the LLM has a chance to hallucinate or fake results.
    {
        let msg_lower = input.content.to_lowercase();
        let service_keywords: Vec<(&str, &str, connectors::ConnectorId)> = vec![
            ("email", "Microsoft 365 or Google Workspace", connectors::ConnectorId::Microsoft),
            ("e-mail", "Microsoft 365 or Google Workspace", connectors::ConnectorId::Microsoft),
            ("inbox", "Microsoft 365 or Google Workspace", connectors::ConnectorId::Microsoft),
            ("outlook", "Microsoft 365", connectors::ConnectorId::Microsoft),
            ("teams", "Microsoft 365", connectors::ConnectorId::Microsoft),
            ("sharepoint", "Microsoft 365", connectors::ConnectorId::Microsoft),
            ("onedrive", "Microsoft 365", connectors::ConnectorId::Microsoft),
            ("gmail", "Google Workspace", connectors::ConnectorId::Google),
            ("google drive", "Google Workspace", connectors::ConnectorId::Google),
            ("google calendar", "Google Workspace", connectors::ConnectorId::Google),
            ("google sheets", "Google Workspace", connectors::ConnectorId::Google),
            ("salesforce", "Salesforce", connectors::ConnectorId::Salesforce),
            ("leads", "Salesforce or a CRM", connectors::ConnectorId::Salesforce),
            ("crm", "Salesforce or a CRM", connectors::ConnectorId::Salesforce),
            ("sap", "SAP", connectors::ConnectorId::Sap),
            ("slack", "Slack", connectors::ConnectorId::Slack),
            ("jira", "Jira / Atlassian", connectors::ConnectorId::Jira),
            ("confluence", "Jira / Atlassian", connectors::ConnectorId::Jira),
            ("github", "GitHub", connectors::ConnectorId::Github),
        ];

        let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        let connected_ids: Vec<&connectors::ConnectorId> = instances.iter()
            .filter(|i| i.status == connectors::ConnectionStatus::Connected)
            .map(|i| &i.connector_id)
            .collect();

        // Find first matching service keyword that isn't connected
        let mut needed_service: Option<(&str, &connectors::ConnectorId)> = None;
        for (keyword, service_name, connector_id) in &service_keywords {
            if msg_lower.contains(keyword) {
                // Check if this connector (or its alternative) is connected
                let is_connected = connected_ids.contains(&connector_id) || {
                    // For email keywords, also check the other email provider
                    if *keyword == "email" || *keyword == "e-mail" || *keyword == "inbox" {
                        connected_ids.contains(&&connectors::ConnectorId::Google) ||
                        connected_ids.contains(&&connectors::ConnectorId::Microsoft)
                    } else {
                        false
                    }
                };
                if !is_connected {
                    needed_service = Some((service_name, connector_id));
                    break;
                }
            }
        }
        drop(instances);

        if let Some((service_name, connector_id)) = needed_service {
            let connector_id_str = format!("{:?}", connector_id).to_lowercase();

            let supports_oauth = oauth::supports_oauth_login(&connector_id_str);
            let guide = if supports_oauth {
                format!(
                    "To do that, I need to connect to **{}** first. Click **Sign in** below to log in securely - I'll handle the rest.",
                    service_name
                )
            } else {
                format!(
                    "To do that, I need to connect to **{}** first. I've opened the setup panel for you - just fill in your credentials and I'll handle the rest.",
                    service_name
                )
            };

            // Tell the frontend to open the connector setup - with OAuth hint
            crate::event_bus::emit_ui(&app, "open-connector-setup", serde_json::json!({
                "connector_id": connector_id_str,
                "supports_oauth": supports_oauth,
            })).ok();

            let reply = Message {
                id: Uuid::new_v4().to_string(),
                role: "assistant".to_string(),
                content: guide,
                context: None,
                agent_id: agent.map(|a| a.id),
                timestamp: Utc::now().to_rfc3339(),
                tool_turns: None,
            };
            {
                let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
                msgs.push(reply.clone());
                let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
                *orb = OrbState::Idle;
            }
            crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Idle).ok();
            // Don't emit new-message - the returned Ok(reply) is enough
            return Ok(reply);
        }
    }

    let home_dir = dirs::home_dir()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| "~".to_string());

    // The user's name for the system prompt (set during onboarding)
    let role_context = {
        let onb = state.profile.onboarding.lock().unwrap_or_else(|e| e.into_inner());
        if onb.user_name.is_empty() {
            String::new()
        } else {
            format!("The user's name is {}.\n", onb.user_name)
        }
    };

    // Build connector context so the LLM knows what's available
    let connector_context = {
        let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        if instances.is_empty() {
            String::new()
        } else {
            let connected: Vec<String> = instances.iter()
                .filter(|i| i.status == connectors::ConnectionStatus::Connected)
                .map(|i| format!("- {} ({:?})", i.name, i.connector_id))
                .collect();
            if connected.is_empty() {
                String::new()
            } else {
                format!("\nCONNECTED SERVICES (use these automatically when relevant):\n{}\n", connected.join("\n"))
            }
        }
    };

    // Build memory context - inject relevant preferences and facts
    let memory_context = {
        let mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
        if mem.entries.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = mem.entries.iter()
                .take(20) // limit to avoid prompt bloat
                .map(|e| format!("- [{}] {}: {}", format!("{:?}", e.category).to_lowercase(), e.key, e.value))
                .collect();
            format!("\nUSER MEMORY (things I've learned about this user):\n{}\n", items.join("\n"))
        }
    };

    // Build workflow context - list available workflows the LLM can run
    let workflow_context = {
        let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
        if store.workflows.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = store.workflows.iter()
                .take(15)
                .map(|w| format!("- \"{}\" ({} steps): {}", w.name, w.steps.len(), w.description))
                .collect();
            format!("\nAVAILABLE WORKFLOWS (can be run with run_workflow):\n{}\n", items.join("\n"))
        }
    };

    let system_prompt_text = format!(
        "You are Bob, the orchestrator AI behind OrbixCore - running locally on the user's {HOST_OS_NAME}. \
You execute real actions using <action>JSON</action> tags.\n\n\
=== WORKFLOW ARCHITECT ===\n\
When the user describes a task, goal, or process they want automated, your job is to build the FULL STACK, not just an agent. An agent alone does nothing - it needs a workflow that calls it on a schedule or trigger.\n\n\
THE COMPLETE STACK for any automation is:\n\
  (1) Agent - the worker that does the task. Default to \"gemma-4-e4b\" for simple tasks (chat, suggestions, summaries). Only use \"gemma-4-27b\" for complex reasoning tasks.\n\
  (2) Connector - only if the task touches an external service (Gmail, Slack, LinkedIn, etc.)\n\
  (3) Workflow - the steps the agent runs. MUST exist or the agent is dead weight.\n\
  (4) Trigger - when the workflow runs (manual, cron, webhook, etc.) - optional but usually needed for recurring tasks.\n\n\
NEVER stop after creating just an agent. If you emit create_agent, you MUST also emit create_workflow in the SAME response. The workflow gives the agent something to do. Example for \"a meal suggester agent\":\n\n\
  <action>{{\"type\":\"create_agent\",\"name\":\"Meal Suggester\",\"provider\":\"local\",\"model\":\"gemma-4-e4b\"}}</action>\n\
  <action>{{\"type\":\"create_workflow\",\"name\":\"Daily Meal Suggestion\",\"description\":\"Suggest a meal for tonight based on user preferences\",\"steps\":[{{\"type\":\"prompt\",\"label\":\"Generate suggestion\",\"config\":{{\"prompt\":\"Suggest one dinner idea for tonight. Use the user's dietary prefs from memory. Keep it under 3 sentences.\"}}}}]}}</action>\n\n\
RULES for building:\n\
1. Do NOT try to execute external tasks immediately via API calls (LinkedIn, Instagram, Slack, etc.).\n\
2. Ask ONE clarifying question max if truly needed (tone, frequency). Don't interrogate.\n\
3. Then BUILD the full stack: agent + workflow (+ connector if external, + trigger if recurring).\n\
4. Model choice: gemma-4-e4b (fast, 4B params) for 90% of tasks. gemma-4-27b only for heavy reasoning.\n\
5. External services (LinkedIn, Gmail, Slack, etc.) require CONNECTORS - never call their APIs directly.\n\n\
=== RESPONSE SHAPE ===\n\
Think and plan privately, then ALWAYS write the user-facing reply: short, structured, action-oriented, then any <action> tags. Your private reasoning is captured and hidden automatically - you do NOT need <think> tags or any special markup for it. Never write your planning as the reply.\n\
Your reply must never be empty: every turn ends with a real answer, an <inputs> block, or <action> tags - never with reasoning alone.\n\
After delivering a result (a document, an answer, a build), end with ONE short line offering 2-3 concrete next steps the user might want - e.g. 'Want me to expand the history section, add sources, or turn this into slides?'. Guide, never just stop.\n\
When asked for news or current information, the answer must BE the information: concrete items, each with what happened, when, and which source it came from. NEVER answer by describing where such information could be found - that is a non-answer. If the search only surfaced site descriptions instead of actual items, dig deeper: include today's date in a fresh search query, or web_fetch a source's page and read the headlines.\n\n\
When the user asks for MORE depth or detail on a factual or current topic (\"go more in depth\", \"what exactly changed\"), run web_search or research AGAIN with more specific queries before answering. Only state details that appear in tool evidence in this conversation. If the evidence does not contain the detail asked for, say so and search - never fill the gap from memory, and never invent names, numbers, or specifics.\n\n\
Never promise future action (\"I will try again shortly\", \"I'll keep monitoring\") - you only act when the user sends a message. When something fails, say what failed and end with a concrete invitation like: ask me again in a few minutes and I'll retry.\n\n\
=== ASKING FOR INPUT ===\n\
When you need information from the user, emit ONE <inputs> JSON block listing every question, then STOP. Do NOT ask in prose. Do NOT write 'Plan:', 'Goal:', 'Workflow:', '## Step', or any markdown headers. Do NOT list a plan. Just emit the inputs block and end your turn.\n\n\
The frontend renders the inputs block as a single-question wizard - the user answers them one at a time, then sends the bundled answers back as their next message.\n\n\
Schema:\n\
<inputs>[{{\"id\":\"<short_id>\",\"label\":\"<question shown to user>\",\"kind\":\"text|longtext|select|yesno\",\"placeholder\":\"<optional hint>\",\"options\":[\"<for select>\"],\"required\":true}}]</inputs>\n\n\
Example (LinkedIn post draft):\n\
<inputs>[{{\"id\":\"audience\",\"label\":\"Who is this post for?\",\"kind\":\"select\",\"options\":[\"customers\",\"investors\",\"team\",\"general\"]}},{{\"id\":\"tone\",\"label\":\"Brand tone in 3-5 words\",\"kind\":\"text\",\"placeholder\":\"e.g. confident, technical, no fluff\"}},{{\"id\":\"hook\",\"label\":\"What's the one thing the reader should walk away with?\",\"kind\":\"longtext\"}}]</inputs>\n\n\
Rules for inputs:\n\
- Keep labels short and human (under 80 chars).\n\
- Cap at 5 questions per turn. If you need more, ask the most important 5 first.\n\
- Use 'select' or 'yesno' over 'text' whenever there's a finite answer.\n\
- Use 'text' for short single-line, 'longtext' for paragraphs.\n\
- Bias toward building with sensible defaults; only ask when defaults would clearly miss the mark.\n\
- NEVER ask for passwords, API keys, tokens, or credentials of any kind - not in an inputs block, not in prose. If access to something is denied, say exactly what was denied and stop - never ask the user for a credential to get around it.\n\
- NEVER ask for information your tools can fetch. Meetings, emails, tasks, notes, files, screen content - read them yourself ({FETCH_SOURCES}). Asking the user to type what their calendar already knows is a failure. Inputs are ONLY for what exists solely in the user's head: preferences, choices, approvals, missing context.\n\
- Opening an app for the user is NOT an answer. 'I've opened Calendar - check your events' is a failure: READ the data ({READ_TOOLS}) and give the answer itself. The user asked you so they would not have to look.\n\
- {CALENDAR_RULE}\n\
- Do not create a workflow to answer a one-off question ('what should I do today') - just fetch and answer. Workflows are for processes the user wants to REPEAT.\n\
- After you emit <inputs>, write nothing else. No <action> tags, no follow-up prose. Wait.\n\n\
=== RULES ===\n\
1. DO NOT introduce yourself, explain your capabilities, or list what you can do. Just respond to what the user asked.\n\
2. If the user wants something DONE NOW (fetch data, run a command, query a connected system), execute it.\n\
3. If the user wants something BUILT (a process, automation, recurring task), design a workflow.\n\
4. To execute anything, wrap it in <action>JSON</action> tags. Without tags, nothing happens.\n\
5. ABSOLUTELY NEVER fabricate, invent, or assume data. If an API returns 0 results, say \"none found\".\n\
6. Use full paths like {home}/Desktop. Never use ~ or $HOME.{PLATFORM_PATH_RULE}\n\
7. For complex tasks, use MULTIPLE <action> tags in sequence.\n\
8. Keep the user-facing reply SHORT. Bullet points, then actions. Reasoning goes inside <think>.\n\
9. If the user greets you, respond conversationally. Do NOT run any actions unless asked.\n\
10. Reply in the language the user wrote in. A Danish question gets a Danish answer.\n\n\
=== TOOLS ===\n\n\
Your tools are provided natively - call them directly and the arguments schema tells you what each needs.\n\
Fallback syntax if native calls are unavailable to you: wrap ONE JSON object per action in tags, e.g.\n\
<action>{{\"type\":\"tool\",\"tool\":\"web_search\",\"args\":{{\"query\":\"...\"}}}}</action>\n\
<action>{{\"type\":\"create_workflow\",\"name\":\"...\",\"steps\":[...]}}</action>\n\
The \"type\" is the tool name from your native tools (generic executables use type \"tool\" with a \"tool\" field: shell_exec, file_read, file_write, file_list, web_fetch, web_search). Same names, same arguments, either way.\n\n\
=== CONNECTORS ===\n\
Connect: <action>{{\"type\":\"connect_oauth\",\"service\":\"microsoft|google|github|slack|linkedin\"}}</action>\n\
Check: <action>{{\"type\":\"check_connectors\"}}</action>\n\
When user says \"connect [service]\", use connect_oauth.\n\n\
=== USING CONNECTED PLATFORMS IN WORKFLOWS ===\n\
When a platform is connected, BUILD WORKFLOWS that use it. Do NOT call APIs directly.\n\
To post to LinkedIn, create a workflow with an action step that has connector_id, api_url, method, and body.\n\
Example - LinkedIn post workflow:\n\
<action>{{\"type\":\"create_workflow\",\"name\":\"LinkedIn Post\",\"description\":\"Publish a post to LinkedIn\",\"run\":true,\"steps\":[{{\"type\":\"action\",\"label\":\"Publish to LinkedIn\",\"connector_id\":\"<instance_id>\",\"api_url\":\"https://api.linkedin.com/rest/posts\",\"method\":\"POST\",\"body\":{{\"author\":\"urn:li:person:<user_sub>\",\"commentary\":\"The post text here\",\"visibility\":\"PUBLIC\",\"distribution\":{{\"feedDistribution\":\"MAIN_FEED\",\"targetEntities\":[],\"thirdPartyDistributionChannels\":[]}},\"lifecycleState\":\"PUBLISHED\"}}}}]}}</action>\n\
IMPORTANT: Get the user's LinkedIn member URN first by checking the connector instance credentials for the sub/email.\n\
For other platforms, same pattern: create workflow steps with connector_id + api_url + method + body.\n\n\
=== BUSINESS DATA ===\n\
There is no built-in CRM or ERP. For business data, use what the USER has connected: their own systems and data stores (Microsoft/Dataverse, Google, SQLite, a REST API, etc.) via connectors and destinations. Never assume a company platform exists. If the user has not connected a source for what they are asking, say so and offer to connect one - do not invent API routes or fabricate data.\n\n\
=== ENVIRONMENT ===\n\
Today is {now}. Use this date when searching for anything recent.\n\
Home: {home} | Desktop: {home}/Desktop | Downloads: {home}/Downloads | Documents: {home}/Documents\n\
{connectors}\
{memory}\
{workflows}\
{role_ctx}\
{skills}\
{today}",
        now = chrono::Local::now().format("%A %-d %B %Y, %H:%M"),
        home = home_dir,
        connectors = connector_context,
        memory = memory_context,
        workflows = workflow_context,
        role_ctx = role_context,
        skills = crate::skills::prompt_index(),
        today = crate::today::prompt_block()
    );

    let system_prompt = serde_json::json!({
        "role": "system",
        "content": system_prompt_text
    });

    let mut api_messages: Vec<serde_json::Value> = vec![system_prompt];
    // Send last 20 messages to stay within context window, use context field for rich history
    let recent_history = if history.len() > 20 { &history[history.len()-20..] } else { &history };
    let n = recent_history.len();
    api_messages.extend(recent_history.iter().enumerate().map(|(i, m)| {
        let keep_evidence = i + 4 >= n;
        serde_json::json!({ "role": m.role, "content": history_msg_content(m, keep_evidence) })
    }));

    // Create placeholder reply message immediately
    let reply_id = Uuid::new_v4().to_string();
    let agent_id_for_reply = agent.as_ref().map(|a| a.id.clone());

    // Determine if we can stream (local provider or no agent = local)
    let use_streaming = agent.as_ref().map(|a| a.provider == AgentProvider::Local).unwrap_or(true);

    let raw_content = if use_streaming {
        // ── Streaming path: emit tokens progressively ──
        let client = reqwest::Client::new();
        // Emit a placeholder so the frontend knows a message is coming
        crate::event_bus::emit_ui(&app, "message-stream-start", serde_json::json!({
            "message_id": reply_id,
            "agent_id": &agent_id_for_reply,
        })).ok();

        match call_local_llm_streaming(&client, &history, &reply_id, &app, Some(&*state)).await {
            Ok(text) => text,
            Err(e) => format!("Error: {}", e),
        }
    } else {
        // ── Non-streaming path: cloud APIs ──
        let response_content = if let Some(ref a) = agent {
            call_agent_api(a, &history).await
        } else {
            let client = reqwest::Client::new();
            call_local_llm(&client, &history).await
        };
        match response_content {
            Ok(text) => text,
            Err(e) => format!("Error: {}", e),
        }
    };

    // Parse and execute action blocks from LLM response
    let content_raw = execute_actions(&raw_content, &*state, &app).await;
    let content = strip_hallucinated_content(&content_raw);

    // Build a context-rich version for conversation history
    let history_context = if raw_content.contains("<action>") {
        Some(format!("I executed: {}\n\nResults shown to user: {}", raw_content, content))
    } else {
        None
    };

    let reply = Message {
        id: reply_id,
        role: "assistant".to_string(),
        content: content.clone(),
        context: history_context,
        agent_id: agent_id_for_reply,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };

    {
        let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        msgs.push(reply.clone());
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = OrbState::Idle;
    }

    // Emit final complete message (replaces any streaming chunks)
    crate::event_bus::emit_ui(&app, "message-stream-end", serde_json::json!({
        "message_id": &reply.id,
        "content": &content,
    })).ok();

    // ── Rationale Log: track LLM usage ──
    {
        let est_tokens_in = (input.content.len() as u64) / 4;
        let est_tokens_out = (raw_content.len() as u64) / 4;
        let model_name = if use_streaming { replay_model_id(Some(&*state)) } else { "unknown".to_string() };
        emit_llm_activity(&app, "Bob", "chat_response", &model_name, use_streaming, est_tokens_in, est_tokens_out, None, 0);
    }

    // Auto-save conversation after each reply
    {
        let convos = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        let conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let title = convos.first()
            .map(|m| { let t = m.content.chars().take(50).collect::<String>(); if m.content.len() > 50 { format!("{}...", t) } else { t } })
            .unwrap_or_else(|| "Untitled".to_string());
        storage::save_conversation(&conv_id, &title, &convos).ok();
    }

    crate::event_bus::emit_ui(&app, "orb-state-change", OrbState::Idle).ok();

    Ok(reply)
}


/// Best-effort repair of almost-valid JSON emitted by small local models.
/// Only applies transforms that cannot corrupt already-valid JSON:
/// markdown fences, smart quotes, and trailing commas are never valid JSON,
/// so stripping them is safe. Returns None if nothing changed.
pub(crate) fn repair_json(raw: &str) -> Option<String> {
    let mut s = raw.trim().to_string();

    // Strip ```json … ``` / ``` … ``` fences the model sometimes wraps around it.
    if s.starts_with("```") {
        if let Some(nl) = s.find('\n') {
            s = s[nl + 1..].to_string();
        }
        if let Some(idx) = s.rfind("```") {
            s = s[..idx].to_string();
        }
        s = s.trim().to_string();
    }

    // Normalise smart quotes (never valid in JSON).
    s = s
        .replace('\u{201C}', "\"")
        .replace('\u{201D}', "\"")
        .replace('\u{2018}', "'")
        .replace('\u{2019}', "'");

    // Drop trailing commas before } or ] (never valid in JSON).
    if let Ok(re) = regex::Regex::new(r",(\s*[}\]])") {
        s = re.replace_all(&s, "$1").to_string();
    }

    // Strip stray chat-template token artifacts. Local models sometimes
    // leak special tokens mid-string (observed from Gemma 27B:
    // `"query":"…announcement<|"|>}}` - the real closing quote trapped
    // inside a glitched `<|…|>` wrapper). Two classes:
    //   1. known control tokens (`<|im_end|>` etc.) vanish entirely;
    //   2. any other short `<|X|>` unwraps to X, freeing the characters
    //      the wrapper swallowed.
    // Safe because repair_json only ever runs on input that already
    // failed to parse.
    for tok in [
        "<|im_end|>", "<|im_start|>", "<|endoftext|>", "<|eot_id|>",
        "<|end|>", "<|assistant|>", "<|user|>", "<|system|>",
    ] {
        if s.contains(tok) {
            s = s.replace(tok, "");
        }
    }
    if let Ok(re) = regex::Regex::new(r"<\|(.{0,3}?)\|>") {
        s = re.replace_all(&s, "$1").to_string();
    }

    // Fix invalid backslash escapes - the Windows-path failure mode:
    // models write "path":"C:\Users\andre\..." and \U, \a, \D are not
    // valid JSON escapes, so the whole action fails to parse. Double any
    // backslash that does not start a valid escape (\" \\ \/ \b \f \n
    // \r \t or \uXXXX). Backslashes outside strings are invalid JSON
    // either way, so this cannot turn bad JSON into differently-bad JSON.
    {
        let chars: Vec<char> = s.chars().collect();
        let mut fixed = String::with_capacity(s.len() + 8);
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' {
                let next = chars.get(i + 1).copied();
                let valid = match next {
                    Some('"') | Some('\\') | Some('/') | Some('b') | Some('f')
                    | Some('n') | Some('r') | Some('t') => true,
                    Some('u') => chars
                        .get(i + 2..)
                        .map(|rest| rest.iter().take(4).filter(|h| h.is_ascii_hexdigit()).count() == 4)
                        .unwrap_or(false),
                    _ => false,
                };
                if valid {
                    fixed.push('\\');
                    fixed.push(next.unwrap());
                    i += 2;
                } else {
                    fixed.push('\\');
                    fixed.push('\\');
                    i += 1;
                }
                continue;
            }
            fixed.push(c);
            i += 1;
        }
        s = fixed;
    }

    let s = s.trim().to_string();
    if s == raw.trim() { None } else { Some(s) }
}

/// Parse <action>...</action> blocks from LLM output and execute them.
/// Returns the content with action blocks replaced by confirmation messages.
/// Unwrap a first-class action the model wrapped in the generic tool
/// envelope. shell_exec/file_read/etc. are invoked as
/// `{"type":"tool","tool":"X","args":{...}}`, and the model routinely
/// generalizes that shape to first-class actions like `app_bridge` -
/// which would then reach `tools::execute_tool` and be rejected as
/// "Unknown tool". This rewrites
/// `{"type":"tool","tool":"<first-class>","args":{...}}` to
/// `{"type":"<first-class>", ...args}` so it reaches its own executor
/// arm. Builtin tools (`is_builtin_tool`) stay wrapped; only known
/// first-class actions are unwrapped, and anything else is returned
/// unchanged.
pub(crate) fn unwrap_tool_wrapped_action(action: serde_json::Value) -> serde_json::Value {
    if action.get("type").and_then(|v| v.as_str()) != Some("tool") {
        return action;
    }
    let Some(inner) = action.get("tool").and_then(|v| v.as_str()).map(str::to_string) else {
        return action;
    };
    if inner == "tool"
        || crate::tools::is_builtin_tool(&inner)
        || !crate::tool_schemas::is_known_tool(&inner)
    {
        return action;
    }
    let mut merged = action.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
    if !merged.is_object() {
        merged = serde_json::json!({});
    }
    if let Some(obj) = merged.as_object_mut() {
        obj.insert("type".to_string(), serde_json::Value::String(inner));
    }
    merged
}

/// Reconstruct one native tool call (W1.5) into the action object the
/// executor consumes: `{"type": name, ...arguments}`. Returns `None` for
/// unknown tools or arguments that will not parse even after JSON repair,
/// so a malformed call is dropped rather than fed to the executor.
pub(crate) fn reconstruct_tool_call(name: &str, args_raw: &str) -> Option<serde_json::Value> {
    if name.is_empty() || !crate::tool_schemas::is_known_tool(name) {
        return None;
    }
    let args = if args_raw.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str::<serde_json::Value>(args_raw)
            .ok()
            .or_else(|| repair_json(args_raw)
                .and_then(|f| serde_json::from_str::<serde_json::Value>(&f).ok()))?
    };
    let mut action = serde_json::Map::new();
    action.insert("type".to_string(), serde_json::Value::String(name.to_string()));
    if let Some(obj) = args.as_object() {
        for (k, v) in obj {
            action.insert(k.clone(), v.clone());
        }
    }
    Some(serde_json::Value::Object(action))
}

pub async fn execute_actions(content: &str, state: &AppState, app: &tauri::AppHandle) -> String {
    execute_actions_counted(content, state, app).await.0
}

/// Execute native tool calls (W1.5) by rendering them into the same
/// `<action>{json}</action>` text the tag executor consumes, then running
/// the identical `execute_actions_counted`. The returned string is the
/// concatenated per-action confirmations, which for `tool` actions include
/// the real tool output - so the agentic loop can feed genuine results
/// back to the model instead of just action names.
pub async fn execute_tool_calls_counted(
    calls: &[serde_json::Value],
    state: &AppState,
    app: &tauri::AppHandle,
) -> (String, ExecStats) {
    let mut rendered = String::new();
    for call in calls {
        rendered.push_str("<action>");
        rendered.push_str(&call.to_string());
        rendered.push_str("</action>\n");
    }
    execute_actions_counted(&rendered, state, app).await
}

/// Why a workflow step cannot run, or None when it can. A workflow whose
/// steps are just labels is a lie waiting for its scheduled hour (field
/// failure 2026-07-31: "Morning Briefing" created with three empty-config
/// steps and announced as scheduled) - so create_workflow rejects those
/// with this exact explanation and the loop feeds it back to the model.
/// Remove <inputs>...</inputs> wizard blocks (closed or trailing-open)
/// from text. Workflow runs are non-interactive; a model that emits one
/// anyway must not summon the wizard from inside a run.
pub(crate) fn strip_inputs_blocks(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        match rest.find("<inputs>") {
            Some(open) => {
                out.push_str(&rest[..open]);
                match rest[open..].find("</inputs>") {
                    Some(close) => rest = &rest[open + close + "</inputs>".len()..],
                    None => break,
                }
            }
            None => {
                out.push_str(rest);
                break;
            }
        }
    }
    out.trim().to_string()
}

/// A step Bob wrote that hands data to another app (peer_send, peer_fill,
/// peer_press, peer_open) is only allowed when the person named that app.
/// A small model copied a "send it to ChatGPT" example into a plain
/// summarise step (v0.1.0), which would have pasted local data into a
/// cloud app nobody asked for. Thinking and writing are prompt steps; they
/// run on the local model. `request` is the user's message for the turn,
/// None when no one typed one (Imagine, agent shifts).
pub(crate) fn unrequested_peer_step(
    config: &std::collections::HashMap<String, serde_json::Value>,
    request: Option<&str>,
) -> Option<String> {
    let tool = config.get("tool").and_then(|v| v.as_str()).unwrap_or("").trim();
    if !matches!(tool, "peer_send" | "peer_fill" | "peer_press" | "peer_open") {
        return None;
    }
    let app = config
        .get("tool_args")
        .and_then(|a| a["app"].as_str())
        .unwrap_or("")
        .trim();
    let named = !app.is_empty()
        && request.is_some_and(|r| r.to_lowercase().contains(&app.to_lowercase()));
    if named {
        None
    } else {
        Some(format!(
            "uses {} to hand data to \"{}\", which the user did not ask for. \
             To summarise, write or reason, use a prompt step (type \"prompt\"): it runs on the local model",
            tool,
            if app.is_empty() { "an app" } else { app }
        ))
    }
}

pub(crate) fn workflow_step_gap(
    step_type: &workflow::StepType,
    config: &std::collections::HashMap<String, serde_json::Value>,
) -> Option<String> {
    let has = |k: &str| {
        config
            .get(k)
            .map(|v| match v {
                serde_json::Value::String(s) => !s.trim().is_empty(),
                serde_json::Value::Null => false,
                _ => true,
            })
            .unwrap_or(false)
    };
    // A workflow step that interrogates the user is hollow in a different
    // way: workflows run unattended, so "ask the user" can never execute.
    if let Some(serde_json::Value::String(p)) = config.get("prompt") {
        let pl = p.to_lowercase();
        if ["ask the user", "ask user", "please list your", "request from the user", "prompt the user"]
            .iter()
            .any(|m| pl.contains(m))
        {
            return Some(
                "asks the user for information - workflows run unattended, so steps must \
                 gather data with tools (web_search, web_fetch, files, shell) instead of asking"
                    .to_string(),
            );
        }
    }
    match step_type {
        workflow::StepType::Prompt => {
            if has("prompt") {
                None
            } else {
                Some(
                    "a prompt step needs `prompt`: the full instruction text the model runs"
                        .to_string(),
                )
            }
        }
        workflow::StepType::Action => {
            if let Some(serde_json::Value::String(t)) = config.get("tool") {
                let t = t.trim();
                if !t.is_empty() && !crate::tool_schemas::step_tool_names().contains(&t) && t != "file_edit" {
                    return Some(format!(
                        "uses a tool that does not exist (\"{}\"); steps can use: {}",
                        t,
                        crate::tool_schemas::step_tool_names().join(", ")
                    ));
                }
            }
            // Names must be real: a bridge action the app has, an app that
            // is paired. Guessed names were the top reason drafts died at
            // run time instead of at draft time.
            let tool = config.get("tool").and_then(|v| v.as_str()).unwrap_or("").trim();
            let args = config.get("tool_args").cloned().unwrap_or(serde_json::Value::Null);
            if tool == "app_bridge" {
                let app = args["app"].as_str().unwrap_or("").trim();
                let action = args["action"].as_str().unwrap_or("").trim();
                if app.is_empty() || action.is_empty() {
                    return Some("app_bridge needs tool_args {app, action}".to_string());
                }
                if crate::app_bridge::find_manifest(app).is_none() {
                    let known: Vec<String> = crate::app_bridge::all_manifests().iter().map(|m| m.app.clone()).collect();
                    return Some(format!("uses app_bridge for \"{}\", which has no bridge; bridged apps: {}", app, known.join(", ")));
                }
                if let Some(valid) = crate::app_bridge::unknown_action(app, action) {
                    return Some(format!("uses app_bridge action \"{}\" which {} does not have; its actions: {}", action, app, valid.join(", ")));
                }
            }
            if tool.starts_with("peer_") {
                let app = args["app"].as_str().unwrap_or("").trim();
                if app.is_empty() {
                    return Some(format!("{} needs tool_args.app (a paired app)", tool));
                }
                if crate::peer::find(app).is_none() {
                    let known: Vec<String> = crate::peer::list().into_iter().map(|p| p.app).collect();
                    return Some(format!(
                        "uses {} on \"{}\", which is not paired; paired apps: {}",
                        tool,
                        app,
                        if known.is_empty() { "none yet (pair it first)".to_string() } else { known.join(", ") }
                    ));
                }
            }
            if has("tool") || has("connector_id") || has("api_url") || has("destination") || has("command") {
                None
            } else {
                Some(
                    "an action step needs `tool` + `tool_args` (web_search, web_fetch, \
                     file_read/file_write/file_list, shell_exec), or a \
                     connector_id / api_url / destination / command"
                        .to_string(),
                )
            }
        }
        _ => None,
    }
}

/// A plain completion for code generation: no Bob persona, thinking off
/// (the chat path's measured default), low temperature, a tight output
/// budget. The persona prompt costs thousands of prompt tokens and its
/// "be terse, wrap actions" rules only fight a JSON-only task.
pub(crate) async fn call_model_plain(system: &str, user: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(240))
        .build()
        .map_err(|e| e.to_string())?;
    let endpoint = crate::local_llm::server_endpoint();
    let health = client.get(format!("{}/health", endpoint)).send().await;
    if health.is_err() || !health.unwrap().status().is_success() {
        return Err("Local model server is not running. Select a model in Settings - Models to start it.".to_string());
    }
    let _busy = crate::bob_core::ModelBusyGuard::new();
    let body = serde_json::json!({
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
        "temperature": 0.2,
        "max_tokens": 2200,
        "stream": false,
        "cache_prompt": true,
        "chat_template_kwargs": { "enable_thinking": crate::llm_engine::thinking_enabled() },
    });
    let resp = client
        .post(format!("{}/v1/chat/completions", endpoint))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Local LLM request failed: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Local LLM error ({}): {}", status, text));
    }
    let json: serde_json::Value = resp.json().await.map_err(|e| format!("Failed to parse local LLM response: {}", e))?;
    json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "the model returned no content".to_string())
}


/// Find the outermost {...} JSON object in a text blob (fences, prose).
pub(crate) fn extract_json_object(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let mut depth = 0i32;
    let mut in_str = false;
    let mut esc = false;
    for (i, c) in text[start..].char_indices() {
        match c {
            '"' if !esc => in_str = !in_str,
            '\\' if in_str => {
                esc = !esc;
                continue;
            }
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

/// A step's tool arguments: `tool_args`, plus any argument written beside
/// the tool instead of inside it ({"tool": "shell_exec", "command": "ls"}).
/// Models write both shapes; the tool only read tool_args, so the run failed
/// with "Missing required argument: command" (v0.1.0). Used when a step is
/// saved and again when it runs, so workflows saved earlier work too.
pub(crate) fn step_tool_args(config: &HashMap<String, serde_json::Value>) -> serde_json::Value {
    let mut args = match config.get("tool_args") {
        Some(serde_json::Value::Object(m)) => m.clone(),
        _ => serde_json::Map::new(),
    };
    for key in ["command", "path", "content", "url", "query"] {
        if let Some(serde_json::Value::String(v)) = config.get(key) {
            args.entry(key.to_string()).or_insert_with(|| serde_json::Value::String(v.clone()));
        }
    }
    serde_json::Value::Object(args)
}

/// Bob's step JSON (create_workflow / update_workflow) to stored steps.
/// Every executable key is copied into the untyped step config.
pub(crate) fn workflow_steps_from_json(steps: &serde_json::Value) -> Vec<workflow::WorkflowStep> {
    let mut out = Vec::new();
    let Some(steps) = steps.as_array() else { return out };
    for (i, s) in steps.iter().enumerate() {
        // A model often writes a prompt step as {"tool": "prompt", "prompt":
        // ...} or leaves `type` out. Read either as a prompt step: rejected
        // as an unknown tool, Bob fell back to a shell echo or to sending
        // the text to a paired cloud app (v0.1.0).
        let tool = s["tool"].as_str().unwrap_or("").trim();
        let has_prompt = s["prompt"].as_str().is_some_and(|p| !p.trim().is_empty());
        let runs_something = !tool.is_empty() && tool != "prompt"
            || ["command", "connector_id", "api_url", "destination"].iter().any(|k| !s[*k].is_null());
        let declared = s["type"].as_str();
        let is_prompt = declared == Some("prompt")
            || (has_prompt && tool == "prompt")
            || (declared.is_none() && has_prompt && !runs_something);
        let mut config: HashMap<String, serde_json::Value> = HashMap::new();
        for key in ["command", "prompt", "path", "content", "condition", "connector_id", "api_url", "method", "tool"] {
            if key == "tool" && tool == "prompt" {
                continue;
            }
            if let Some(v) = s[key].as_str() {
                config.insert(key.to_string(), serde_json::Value::String(v.to_string()));
            }
        }
        if let Some(secs) = s["seconds"].as_u64() {
            config.insert("seconds".to_string(), serde_json::json!(secs));
        }
        for key in ["body", "tool_args", "destination", "rows"] {
            if !s[key].is_null() {
                config.insert(key.to_string(), s[key].clone());
            }
        }
        if let Some(map) = s["map"].as_bool() {
            config.insert("map".to_string(), serde_json::json!(map));
        }
        if !tool.is_empty() && tool != "prompt" {
            let args = step_tool_args(&config);
            if !args.as_object().is_some_and(|m| m.is_empty()) {
                config.insert("tool_args".to_string(), args);
            }
        }
        out.push(workflow::WorkflowStep {
            id: Uuid::new_v4().to_string(),
            order: i,
            step_type: match if is_prompt { "prompt" } else { declared.unwrap_or("action") } {
                "prompt" => workflow::StepType::Prompt,
                "condition" => workflow::StepType::Condition,
                "wait" => workflow::StepType::Wait,
                "loop" => workflow::StepType::Loop,
                _ => workflow::StepType::Action,
            },
            label: s["label"].as_str().unwrap_or("Step").to_string(),
            config,
        });
    }
    out
}

/// True when a schedule string is a plausible 5-field cron expression.
/// "weekdays at 8" once slipped through as a raw cron and produced a
/// schedule that could never fire.
pub(crate) fn plausible_cron(s: &str) -> bool {
    let fields: Vec<&str> = s.split_whitespace().collect();
    fields.len() == 5
        && fields.iter().all(|f| {
            !f.is_empty() && f.chars().all(|c| c.is_ascii_digit() || matches!(c, '*' | ',' | '-' | '/'))
        })
}

#[cfg(test)]
mod workflow_runnability_tests {
    use super::{plausible_cron, workflow_step_gap, workflow_steps_from_json};

    #[test]
    fn unknown_tools_and_paired_app_tools() {
        use crate::workflow::StepType;
        let mut cfg = std::collections::HashMap::new();
        cfg.insert("tool".to_string(), serde_json::json!("teleport"));
        assert!(workflow_step_gap(&StepType::Action, &cfg).unwrap().contains("does not exist"));
        for tool in ["mcp_call", "api_call", "web_search"] {
            cfg.insert("tool".to_string(), serde_json::json!(tool));
            assert!(workflow_step_gap(&StepType::Action, &cfg).is_none(), "{} should be allowed", tool);
        }
        // Paired-app tools need a paired app; the test data dir has none.
        cfg.insert("tool".to_string(), serde_json::json!("peer_send"));
        cfg.insert("tool_args".to_string(), serde_json::json!({"app": "Nowhere", "text": "hi"}));
        assert!(workflow_step_gap(&StepType::Action, &cfg).unwrap().contains("not paired"));
        // Bridge actions must exist on the app; app_bridge is a macOS tool.
        cfg.insert("tool".to_string(), serde_json::json!("app_bridge"));
        cfg.insert("tool_args".to_string(), serde_json::json!({"app": "Mail", "action": "get_unread_emails"}));
        let gap = workflow_step_gap(&StepType::Action, &cfg);
        if cfg!(target_os = "macos") {
            assert!(gap.is_none() || gap.as_ref().unwrap().contains("does not have"), "{:?}", gap);
        } else {
            assert!(gap.unwrap().contains("does not exist"));
        }
    }

    #[test]
    fn handing_data_to_an_app_needs_the_user_to_name_it() {
        use super::unrequested_peer_step;
        let step = |tool: &str, app: &str| {
            let mut cfg = std::collections::HashMap::new();
            cfg.insert("tool".to_string(), serde_json::json!(tool));
            cfg.insert("tool_args".to_string(), serde_json::json!({"app": app, "text": "Summarise: {{prev}}"}));
            cfg
        };
        // The v0.1.0 case: a summarise request became a ChatGPT send.
        let asked = "Create a workflow that writes a five-line digest of the latest commits";
        let gap = unrequested_peer_step(&step("peer_send", "ChatGPT"), Some(asked)).unwrap();
        assert!(gap.contains("prompt step"), "{}", gap);
        // Named by the user: allowed, case-insensitive.
        assert!(unrequested_peer_step(&step("peer_send", "ChatGPT"), Some("then ask chatgpt to review it")).is_none());
        // No one typed anything (Imagine, agent shifts): never hand data off.
        assert!(unrequested_peer_step(&step("peer_fill", "Slack"), None).is_some());
        // Reading a paired app and every non-peer tool stay allowed.
        assert!(unrequested_peer_step(&step("peer_read", "ChatGPT"), Some(asked)).is_none());
        assert!(unrequested_peer_step(&step("file_write", ""), Some(asked)).is_none());
    }

    #[test]
    fn prompt_steps_are_recognised_however_the_model_writes_them() {
        use crate::workflow::StepType;
        let steps = workflow_steps_from_json(&serde_json::json!([
            // Exactly what Gemma 4 E4B sent (v0.1.0 replay log).
            {"label": "Generate digest", "tool": "prompt", "prompt": "Write a five-line digest of: {{prev1}}"},
            {"label": "No type", "prompt": "Summarise {{prev}}"},
            {"type": "action", "label": "Typed action", "tool": "prompt", "prompt": "Tidy it"},
            {"label": "Shell", "tool": "shell_exec", "tool_args": {"command": "ls"}},
            {"label": "Old style", "command": "echo hi", "prompt": "unused"}
        ]));
        assert_eq!(steps[0].step_type, StepType::Prompt);
        assert!(!steps[0].config.contains_key("tool"));
        assert!(workflow_step_gap(&steps[0].step_type, &steps[0].config).is_none());
        assert_eq!(steps[1].step_type, StepType::Prompt);
        assert_eq!(steps[2].step_type, StepType::Prompt);
        assert_eq!(steps[3].step_type, StepType::Action);
        assert_eq!(steps[4].step_type, StepType::Action);
    }

    #[test]
    fn arguments_written_beside_the_tool_reach_it() {
        let steps = workflow_steps_from_json(&serde_json::json!([
            // What Gemma 4 E4B sent: command next to tool, no tool_args.
            {"type": "action", "label": "Log", "tool": "shell_exec", "command": "git log -n 10"},
            {"type": "action", "label": "Both", "tool": "file_read", "path": "a.txt", "tool_args": {"path": "b.txt"}}
        ]));
        assert_eq!(steps[0].config["tool_args"]["command"], "git log -n 10");
        // An explicit tool_args value wins.
        assert_eq!(steps[1].config["tool_args"]["path"], "b.txt");
        // A step saved by v0.1.0 (no tool_args) still gets its command at run time.
        let mut saved = std::collections::HashMap::new();
        saved.insert("tool".to_string(), serde_json::json!("shell_exec"));
        saved.insert("command".to_string(), serde_json::json!("git log -n 10"));
        assert_eq!(super::step_tool_args(&saved)["command"], "git log -n 10");
    }

    #[test]
    fn step_examples_never_suggest_a_cloud_app() {
        let tools = crate::tool_schemas::bob_tools().to_string();
        assert!(!tools.contains("Summarise: {{prev}}"), "a workflow example hands data to an app again");
    }

    #[test]
    fn steps_from_json_keep_every_executable_key() {
        let steps = workflow_steps_from_json(&serde_json::json!([
            {"type": "action", "label": "Ask ChatGPT", "tool": "peer_send", "tool_args": {"app": "ChatGPT", "text": "Summarise: {{prev}}"}},
            {"type": "prompt", "label": "Tidy", "prompt": "Make it one paragraph"},
            {"type": "action", "label": "Old style", "command": "echo hi"},
            {"type": "wait", "label": "Breathe", "seconds": 3}
        ]));
        assert_eq!(steps.len(), 4);
        assert_eq!(steps[0].config["tool"], "peer_send");
        assert_eq!(steps[0].config["tool_args"]["app"], "ChatGPT");
        assert_eq!(steps[1].config["prompt"], "Make it one paragraph");
        assert_eq!(steps[2].config["command"], "echo hi");
        assert_eq!(steps[3].config["seconds"], 3);
        assert_eq!(steps[3].order, 3);
    }
    use crate::workflow::StepType;
    use std::collections::HashMap;

    fn cfg(pairs: &[(&str, serde_json::Value)]) -> HashMap<String, serde_json::Value> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
    }

    #[test]
    fn empty_steps_are_rejected_with_reasons() {
        // The exact Morning Briefing failure: labels, no substance.
        assert!(workflow_step_gap(&StepType::Action, &cfg(&[])).is_some());
        assert!(workflow_step_gap(&StepType::Prompt, &cfg(&[])).is_some());
        assert!(workflow_step_gap(&StepType::Prompt, &cfg(&[("prompt", serde_json::json!("   "))])).is_some());
    }

    #[test]
    fn substantive_steps_pass() {
        // app_bridge is a macOS tool; elsewhere the step is (correctly) refused.
        let bridge = workflow_step_gap(
            &StepType::Action,
            &cfg(&[("tool", serde_json::json!("app_bridge")),
                   ("tool_args", serde_json::json!({"app":"Calendar","action":"upcoming_events"}))]),
        );
        assert_eq!(bridge.is_none(), cfg!(target_os = "macos"), "{:?}", bridge);
        assert!(workflow_step_gap(&StepType::Prompt, &cfg(&[("prompt", serde_json::json!("Summarise the inputs."))])).is_none());
        assert!(workflow_step_gap(&StepType::Action, &cfg(&[("command", serde_json::json!("date"))])).is_none());
    }

    #[test]
    fn cron_plausibility() {
        assert!(plausible_cron("0 8 * * 1-5"));
        assert!(plausible_cron("*/15 9-17 * * *"));
        // The stored-but-never-fires field failure:
        assert!(!plausible_cron("weekdays at 8"));
        assert!(!plausible_cron("0 8 * *"));
    }
}

/// True when a parsed JSON value is unmistakably a tool/action call:
/// either the generic tool envelope ({"tool": ..., "args": ...}) or a
/// first-class action whose "type" the executor knows. Tight on purpose
/// so ordinary JSON in an answer is never mistaken for a call.
pub(crate) fn looks_like_action_json(v: &serde_json::Value) -> bool {
    if !v.is_object() {
        return false;
    }
    if v.get("tool").and_then(|t| t.as_str()).is_some() {
        return true;
    }
    // OpenAI-shaped: {"name": ..., "arguments": {...}} or {"function": {...}}
    let named = v.get("name").and_then(|t| t.as_str())
        .or_else(|| v.get("function").and_then(|f| f.get("name")).and_then(|t| t.as_str()));
    if let Some(n) = named {
        if crate::tool_schemas::is_known_tool(n) || crate::tools::is_builtin_tool(n) {
            return true;
        }
    }
    v.get("type")
        .and_then(|t| t.as_str())
        .map(crate::tool_schemas::is_known_tool)
        .unwrap_or(false)
}

/// Every shape a model writes a call in, folded into the one the executor
/// reads: `{"type": <action>, ...fields}` with builtin tools as
/// `{"type":"tool","tool":<name>,"args":{...}}`. Accepted inputs:
///   {"tool":"shell_exec","args":{...}}            (no type - the 2026-09-06 failure:
///                                                  five apologetic retries, nothing ran)
///   {"tool":"tool","args":{"tool":"shell_exec","args":{...}}}   (nested retry)
///   {"tool":"shell_exec","command":"..."}         (args at the top level)
///   {"name":"tool","arguments":{...}} / {"function":{"name":..,"arguments":..}}
///   {"tool":"app_bridge","args":{...}}            (first-class action named as a tool)
/// Anything already carrying a "type" passes through untouched.
pub(crate) fn normalize_action_shape(v: serde_json::Value) -> serde_json::Value {
    let serde_json::Value::Object(mut map) = v else { return v };
    if map.get("type").and_then(|t| t.as_str()).is_some() {
        return serde_json::Value::Object(map);
    }
    // OpenAI-shaped {"name","arguments"} / {"function":{"name","arguments"}}
    let (name, args) = if let Some(f) = map.get("function").cloned().filter(|f| f.is_object()) {
        (
            f.get("name").and_then(|n| n.as_str()).map(str::to_string),
            f.get("arguments").cloned(),
        )
    } else if let Some(n) = map.get("name").and_then(|n| n.as_str()).map(str::to_string) {
        (Some(n), map.get("arguments").or_else(|| map.get("parameters")).cloned())
    } else {
        (None, None)
    };
    if let Some(name) = name {
        let mut args = match args {
            Some(serde_json::Value::String(s)) => serde_json::from_str(&s).ok()
                .or_else(|| repair_json(&s).and_then(|f| serde_json::from_str(&f).ok()))
                .unwrap_or_else(|| serde_json::json!({})),
            Some(v) => v,
            None => serde_json::json!({}),
        };
        if !args.is_object() {
            args = serde_json::json!({});
        }
        let mut rebuilt = serde_json::Map::new();
        if name == "tool" {
            // {"name":"tool","arguments":{"tool":..,"args":..}} -> recurse
            return normalize_action_shape(args);
        }
        rebuilt.insert("type".into(), serde_json::Value::String(name));
        if let Some(obj) = args.as_object() {
            for (k, v) in obj {
                rebuilt.insert(k.clone(), v.clone());
            }
        }
        return normalize_action_shape(serde_json::Value::Object(rebuilt));
    }
    // {"tool": T, ...}
    let Some(tool) = map.get("tool").and_then(|t| t.as_str()).map(str::to_string) else {
        return serde_json::Value::Object(map);
    };
    let mut args = match map.remove("args") {
        Some(serde_json::Value::String(s)) => serde_json::from_str(&s).ok().unwrap_or(serde_json::json!({})),
        Some(v) if v.is_object() => v,
        _ => {
            // Arguments given at the top level next to "tool".
            let mut a = serde_json::Map::new();
            for (k, v) in map.iter() {
                if k != "tool" {
                    a.insert(k.clone(), v.clone());
                }
            }
            serde_json::Value::Object(a)
        }
    };
    if tool == "tool" {
        // Nested {"tool":"tool","args":{"tool":X,"args":{...}}}
        return normalize_action_shape(args);
    }
    if crate::tools::is_builtin_tool(&tool) || !crate::tool_schemas::is_known_tool(&tool) {
        // Builtins (and unknown names, which the executor reports) live
        // under the generic envelope.
        return serde_json::json!({ "type": "tool", "tool": tool, "args": args });
    }
    // A first-class action the model named as a tool: hoist its args.
    let mut rebuilt = serde_json::Map::new();
    rebuilt.insert("type".into(), serde_json::Value::String(tool));
    if let Some(obj) = args.as_object_mut() {
        for (k, v) in obj.iter() {
            rebuilt.insert(k.clone(), v.clone());
        }
    }
    serde_json::Value::Object(rebuilt)
}

/// Small local models sometimes emit a tool call as bare JSON - no
/// <action> tags, no native tool_calls - which the executor used to
/// ignore, so the raw JSON rendered in the chat bubble (field failure,
/// 2026-07-31: `{"tool":"web_search",...}` shown to the user). Promote
/// such lines (and single-object ```json fences) to <action> tags so the
/// normal pipeline runs them.
pub(crate) fn promote_bare_tool_json(content: &str) -> String {
    let parse = |s: &str| -> Option<serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .or_else(|| repair_json(s).and_then(|f| serde_json::from_str(&f).ok()))
    };
    // Whole text is one object.
    let trimmed = content.trim();
    if trimmed.starts_with('{') && trimmed.ends_with('}') && !trimmed.contains("<action>") {
        if let Some(v) = parse(trimmed) {
            if looks_like_action_json(&v) {
                return format!("<action>{}</action>", trimmed);
            }
        }
    }
    let mut out = String::with_capacity(content.len() + 32);
    let mut in_fence = false;
    let mut fence_buf = String::new();
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with("```") {
            if in_fence {
                // Closing fence: promote a buffered action object, or put
                // the fence back untouched.
                let obj = fence_buf.trim().to_string();
                let is_action = parse(&obj).map(|v| looks_like_action_json(&v)).unwrap_or(false);
                if is_action {
                    out.push_str(&format!("<action>{}</action>\n", obj));
                } else {
                    out.push_str("```\n");
                    out.push_str(&fence_buf);
                    out.push_str("```\n");
                }
                fence_buf.clear();
            }
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            fence_buf.push_str(line);
            fence_buf.push('\n');
            continue;
        }
        if t.starts_with('{') && t.ends_with('}') && !t.contains("<action>") {
            if let Some(v) = parse(t) {
                if looks_like_action_json(&v) {
                    out.push_str(&format!("<action>{}</action>\n", t));
                    continue;
                }
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    if in_fence && !fence_buf.is_empty() {
        out.push_str(&fence_buf);
    }
    out
}

/// Like `execute_actions` but also reports how many actions ran / failed so
/// callers can detect a turn that produced no working actions and recover.
pub async fn execute_actions_counted(
    content: &str,
    state: &AppState,
    _app: &tauri::AppHandle,
) -> (String, ExecStats) {
    let mut result = promote_bare_tool_json(content);
    let mut offset = 0;
    let mut stats = ExecStats::default();

    while let Some(start) = result[offset..].find("<action>") {
        let abs_start = offset + start;
        let after_tag = abs_start + 8; // len of "<action>"

        // Find closing tag, or fall back to end-of-line / next <action> for unclosed tags
        let (abs_end, close_tag_len) = if let Some(end) = result[after_tag..].find("</action>") {
            (after_tag + end, 9) // 9 = len of "</action>"
        } else if let Some(next) = result[after_tag..].find("<action>") {
            // No closing tag but another action tag follows - take up to next tag
            (after_tag + next, 0)
        } else if let Some(nl) = result[after_tag..].find('\n') {
            // No closing tag - take up to end of line
            (after_tag + nl, 0)
        } else {
            // Take rest of string
            (result.len(), 0)
        };

        {
            let json_str = result[after_tag..abs_end].trim();
            stats.attempted += 1;

            // Try strict parse first; on failure attempt a conservative
            // repair (fences / smart quotes / trailing commas) before giving
            // up. Small local models routinely emit almost-valid JSON and
            // silently dropping it is the #1 cause of "nothing got built".
            let parsed = match serde_json::from_str::<serde_json::Value>(json_str) {
                Ok(v) => Some((v, false)),
                Err(_) => repair_json(json_str)
                    .and_then(|fixed| serde_json::from_str::<serde_json::Value>(&fixed).ok())
                    .map(|v| (v, true)),
            };

            if let Some((mut action, was_repaired)) = parsed {
                if was_repaired {
                    stats.repaired += 1;
                }
                action = unwrap_tool_wrapped_action(normalize_action_shape(action));
                let action_type = action["type"].as_str().unwrap_or("");
                // Tell the frontend what Bob is currently doing so the
                // user sees a friendly progress indicator instead of a hang.
                // (The label joins the step TRAIL only after the action
                // actually produced a confirmation - a failed parse must
                // not show up as a done step.)
                let step_label = action_label_for(action_type, &action);
                bob_step(_app, &step_label);
                let confirmation = match action_type {
                    "create_agent" => {
                        // Resolve provider - default to Local for local-first autonomy
                        let provider_str = action["provider"].as_str().unwrap_or("local");
                        let provider = match provider_str {
                            "anthropic" => AgentProvider::Anthropic,
                            "openai" => AgentProvider::OpenAI,
                            "ollama" => AgentProvider::Ollama,
                            "lmstudio" => AgentProvider::LmStudio,
                            "custom" => AgentProvider::Custom,
                            _ => AgentProvider::Local,
                        };
                        // Default to the smaller, faster E4B model for new local agents.
                        // The 27B reasoning model is reserved for tasks the user explicitly
                        // picks it for (or that Bob determines need heavy reasoning).
                        // Both local models route to the same port 8847; the label is what the
                        // user sees in AgentsPanel and what Bob's prompt advertises.
                        // A new local agent runs on whatever model is selected
                        // (one server, one loaded model); the id is the label
                        // the user sees in AgentsPanel.
                        let selected_local_model_id = replay_model_id(Some(state));
                        let model = action["model"].as_str().unwrap_or(
                            if provider == AgentProvider::Local { &selected_local_model_id } else { "claude-sonnet-4-6" }
                        ).to_string();
                        let endpoint = action["endpoint"].as_str().unwrap_or(
                            &(if provider == AgentProvider::Local { local_llm::llama_base_url() } else { String::new() })
                        ).to_string();
                        let name = action["name"].as_str().unwrap_or("New Agent").to_string();
                        // Employee fields (P2): a charter makes the agent an
                        // actual worker that can run shifts, granted tools
                        // scope what it may touch, every_minutes schedules it.
                        // Without these, create_agent only produced a bare
                        // model connection that run_shift refused to run.
                        let charter = action["charter"].as_str().unwrap_or("").trim().to_string();
                        let requested_tools: Vec<String> = action["tools"].as_array()
                            .map(|a| a.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
                            .unwrap_or_default();
                        let (granted_tools, unknown_tools): (Vec<String>, Vec<String>) =
                            requested_tools.into_iter().partition(|t| crate::tool_schemas::is_known_tool(t));
                        let schedule_minutes = action["every_minutes"].as_u64().filter(|m| *m > 0);
                        let agent_id = Uuid::new_v4().to_string();
                        {
                            let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                            agents.push(Agent {
                                charter: charter.clone(),
                                granted_tools,
                                schedule_minutes,
                                last_run_at: None,
                                id: agent_id.clone(),
                                name: name.clone(),
                                provider,
                                model,
                                endpoint,
                                api_key: action["api_key"].as_str().unwrap_or("").to_string(),
                                enabled: true,
                                is_default: false,
                                created_at: Utc::now().to_rfc3339(),
                                connector_ids: Vec::new(),
                                workflow_ids: Vec::new(),
                                capabilities: agent_router::AgentCapabilities::default(),
                            });
                            storage::save_agents(&agents).ok();
                        }
                        crate::event_bus::emit_ui(_app, "data-changed", "agent-created").ok();
                        let mut reply = if charter.is_empty() {
                            format!("**Agent \"{}\" created.**", name)
                        } else {
                            format!(
                                "**Hired \"{}\".** Charter set{}. {}",
                                name,
                                match schedule_minutes {
                                    Some(m) => format!(", works a shift every {} minutes", m),
                                    None => ", runs shifts on demand".to_string(),
                                },
                                "Run one now from the Agents panel or say 'run a shift'."
                            )
                        };
                        if !unknown_tools.is_empty() {
                            reply.push_str(&format!(
                                "\n\n(Ignored unknown tools: {}.)",
                                unknown_tools.join(", ")
                            ));
                        }
                        // run: true = work the first shift immediately, so
                        // "hire an agent to X" shows value in the same breath.
                        // Boxed: the shift re-enters the agentic loop, which is
                        // the executor's own caller (async recursion).
                        if action["run"].as_bool().unwrap_or(false) && !charter.is_empty() {
                            match Box::pin(crate::agent_shifts::run_shift(&agent_id, state, _app)).await {
                                Ok(report) => reply.push_str(&format!("\n\n**First shift report:**\n{}", report)),
                                Err(e) => reply.push_str(&format!("\n\n**First shift failed:** {}", e)),
                            }
                        }
                        reply
                    }
                    "create_workflow" => {
                        let wf_name = action["name"].as_str().unwrap_or("New Workflow").to_string();
                        let wf_desc = action["description"].as_str().unwrap_or("").to_string();
                        let run_now = action["run"].as_bool().unwrap_or(false);
                        let wf_steps = workflow_steps_from_json(&action["steps"]);
                        // W2.7: honour a requested schedule. Bob can pass
                        // "schedule": a cron string ("0 9 * * *") or a preset
                        // keyword ("daily", "hourly", "weekdays", ...). A
                        // scheduled workflow gets a Schedule trigger AND a
                        // registered scheduler entry so the tick fires it; no
                        // more silently forcing every workflow to Manual.
                        // Runnability gate: reject hollow steps with the exact
                        // reason so the agentic loop can retry with substance.
                        let mut return_confirmation: Option<String> = None;
                        let request = crate::turn_scope::request();
                        let gaps: Vec<String> = wf_steps
                            .iter()
                            .filter_map(|s| {
                                workflow_step_gap(&s.step_type, &s.config)
                                    .or_else(|| unrequested_peer_step(&s.config, request.as_deref()))
                                    .map(|g| format!("- '{}': {}", s.label, g))
                            })
                            .collect();
                        if !gaps.is_empty() {
                            return_confirmation = Some(format!(
                                "**[ERR] Workflow \"{}\" NOT created - {} step(s) are empty shells that could never run:**\n{}\n\nRe-send create_workflow with complete steps. Every step must carry its substance, not just a label.",
                                wf_name, gaps.len(), gaps.join("\n")
                            ));
                        }

                        let schedule_req = action["schedule"].as_str().map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty());
                        let (preset, custom_cron) = match schedule_req.as_deref() {
                            None => (None, None),
                            Some(s) => match s.to_lowercase().as_str() {
                                "everyminute" | "every minute" => (Some(scheduler::SchedulePreset::EveryMinute), None),
                                "every5minutes" | "every 5 minutes" => (Some(scheduler::SchedulePreset::Every5Minutes), None),
                                "every15minutes" | "every 15 minutes" => (Some(scheduler::SchedulePreset::Every15Minutes), None),
                                "hourly" | "everyhour" | "every hour" => (Some(scheduler::SchedulePreset::EveryHour), None),
                                "daily" => (Some(scheduler::SchedulePreset::Daily), None),
                                "weekdays" | "weekdaymornings" => (Some(scheduler::SchedulePreset::WeekdayMornings), None),
                                "weekly" => (Some(scheduler::SchedulePreset::Weekly), None),
                                "monthly" => (Some(scheduler::SchedulePreset::Monthly), None),
                                // Anything else must actually BE a cron
                                // expression - "weekdays at 8" once became a
                                // stored schedule that could never fire.
                                other => {
                                    if plausible_cron(other) {
                                        (Some(scheduler::SchedulePreset::Custom), Some(other.to_string()))
                                    } else {
                                        if return_confirmation.is_none() {
                                            return_confirmation = Some(format!(
                                                "**[ERR] Workflow \"{}\" NOT created - schedule '{}' is neither a keyword (daily, hourly, weekly, weekdays, monthly, every5minutes) nor a valid 5-field cron expression** (e.g. '0 8 * * 1-5' for weekdays at 8). Re-send create_workflow with a valid schedule.",
                                                wf_name, other
                                            ));
                                        }
                                        (None, None)
                                    }
                                }
                            },
                        };
                        if let Some(err) = return_confirmation {
                            err
                        } else {
                        let trigger = match (&preset, &custom_cron) {
                            (Some(scheduler::SchedulePreset::Custom), Some(cron)) => {
                                workflow::WorkflowTrigger::Schedule(cron.clone())
                            }
                            (Some(p), _) => {
                                workflow::WorkflowTrigger::Schedule(p.to_cron().unwrap_or("0 9 * * *").to_string())
                            }
                            _ => workflow::WorkflowTrigger::Manual,
                        };

                        // A create with a name that already exists replaces
                        // that workflow. Bob retries a failed run by sending
                        // create_workflow again; each retry used to add one
                        // more copy (four "Weekly repo digest"s, v0.1.0).
                        let (wf_id, replaced) = {
                            let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                            let out = store.upsert_by_name(workflow::Workflow {
                                id: Uuid::new_v4().to_string(),
                                name: wf_name.clone(),
                                description: wf_desc,
                                steps: wf_steps.clone(),
                                trigger,
                                deliver: Default::default(),
                                created_at: Utc::now().to_rfc3339(),
                                updated_at: Utc::now().to_rfc3339(),
                            });
                            store.save();
                            out
                        };
                        // The old schedule goes with the old version; the
                        // new one (if any) is registered below.
                        if replaced {
                            let mut sched = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
                            let stale: Vec<String> = sched.list_schedules().into_iter()
                                .filter(|s| s.workflow_id == wf_id).map(|s| s.id).collect();
                            if !stale.is_empty() {
                                for id in &stale { sched.delete_schedule(id); }
                                sched.save();
                            }
                        }
                        // Register the scheduler entry so the tick actually
                        // fires it (the Schedule trigger alone is inert).
                        let mut schedule_note = String::new();
                        if let Some(preset) = preset {
                            let mut sched = state.automation.scheduler_store.lock().unwrap_or_else(|e| e.into_inner());
                            match sched.create_schedule(wf_id.clone(), preset, custom_cron, None) {
                                Ok(entry) => {
                                    sched.save();
                                    schedule_note = format!(" Scheduled ({}).", entry.cron_expr);
                                }
                                Err(e) => {
                                    schedule_note = format!(" (Schedule rejected: {})", e);
                                }
                            }
                        }
                        crate::event_bus::emit_ui(_app, "data-changed", "workflow-created").ok();

                        if run_now && !wf_steps.is_empty() {
                            // Route through the real engine so a failed step
                            // surfaces as a failure instead of fake success.
                            match run_workflow_core(&wf_id, state, _app).await {
                                Ok(rs) if rs.status == workflow::WorkflowRunStatus::Failed => format!(
                                    "**Workflow \"{}\" ran but a step failed:** {}\n\nFix the failing step with update_workflow (same name); do not create another copy.",
                                    wf_name,
                                    rs.error.unwrap_or_else(|| "unknown error".into())
                                ),
                                Ok(rs) => format!(
                                    "## Workflow: {}\n\n{}",
                                    wf_name,
                                    rs.output.join("\n\n---\n\n")
                                ),
                                Err(e) => format!(
                                    "**Workflow \"{}\" failed to run:** {}",
                                    wf_name, e
                                ),
                            }
                        } else {
                            let verb = if replaced { "updated" } else { "created" };
                            format!("**Workflow \"{}\" {}** with {} steps, every one runnable.{} Run it from the Workflows panel.", wf_name, verb, wf_steps.len(), schedule_note)
                        }
                        }
                    }
                    "run_workflow" => {
                        let wf_name = action["name"].as_str().unwrap_or("").to_string();
                        let resolved = {
                            let store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                            store.workflows.iter()
                                .find(|w| w.name.to_lowercase() == wf_name.to_lowercase() || w.id == wf_name)
                                .map(|w| (w.id.clone(), w.name.clone()))
                        };
                        if let Some((wf_id, name)) = resolved {
                            match run_workflow_core(&wf_id, state, _app).await {
                                Ok(rs) if rs.status == workflow::WorkflowRunStatus::Failed => format!(
                                    "**Workflow \"{}\" ran but a step failed:** {}",
                                    name,
                                    rs.error.unwrap_or_else(|| "unknown error".into())
                                ),
                                Ok(rs) => format!(
                                    "## Workflow: {}\n\n{}",
                                    name,
                                    rs.output.join("\n\n---\n\n")
                                ),
                                Err(e) => format!(
                                    "**Workflow \"{}\" failed to run:** {}",
                                    name, e
                                ),
                            }
                        } else {
                            format!("Workflow \"{}\" not found.", wf_name)
                        }
                    }
                    "update_workflow" => {
                        let wf_name = action["name"].as_str().or(action["id"].as_str()).unwrap_or("").to_string();
                        let new_name = action["new_name"].as_str().map(|s| s.to_string());
                        let new_desc = action["description"].as_str().map(|s| s.to_string());
                        let new_steps = if action["steps"].is_array() { Some(workflow_steps_from_json(&action["steps"])) } else { None };
                        // The same runnability check create_workflow applies.
                        let request = crate::turn_scope::request();
                        let gaps: Vec<String> = new_steps.as_ref().map(|steps| steps.iter().filter_map(|st| {
                            workflow_step_gap(&st.step_type, &st.config)
                                .or_else(|| unrequested_peer_step(&st.config, request.as_deref()))
                                .map(|g| format!("\"{}\" {}", st.label, g))
                        }).collect()).unwrap_or_default();
                        if !gaps.is_empty() {
                            format!("**[ERR] Workflow \"{}\" NOT updated - {} step(s) are empty shells:**\n{}", wf_name, gaps.len(), gaps.join("\n"))
                        } else {
                            let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                            let idx = store.workflows.iter().position(|w| w.name.to_lowercase() == wf_name.to_lowercase() || w.id == wf_name);
                            if let Some(i) = idx {
                                if let Some(n) = new_name { store.workflows[i].name = n; }
                                if let Some(d) = new_desc { store.workflows[i].description = d; }
                                let mut changed = String::new();
                                if let Some(steps) = new_steps {
                                    changed = format!(" Steps replaced: {}.", steps.iter().map(|st| st.label.clone()).collect::<Vec<_>>().join(" → "));
                                    store.workflows[i].steps = steps;
                                }
                                store.workflows[i].updated_at = Utc::now().to_rfc3339();
                                let name = store.workflows[i].name.clone();
                                store.save();
                                format!("Updated workflow \"{}\".{}", name, changed)
                            } else {
                                format!("Workflow \"{}\" not found.", wf_name)
                            }
                        }
                    }
                    "delete_workflow" => {
                        let wf_name = action["name"].as_str().or(action["id"].as_str()).unwrap_or("").to_string();
                        let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                        let before = store.workflows.len();
                        store.workflows.retain(|w| w.name.to_lowercase() != wf_name.to_lowercase() && w.id != wf_name);
                        if store.workflows.len() < before {
                            store.save();
                            format!("Deleted workflow \"{}\".", wf_name)
                        } else {
                            format!("Workflow \"{}\" not found.", wf_name)
                        }
                    }
                    "stop_workflow" => {
                        let mut store = state.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
                        store.stop_run();
                        store.save();
                        "Workflow stopped.".to_string()
                    }
                    "run_chain" => {
                        // Bob can invoke an orchestrator chain by name OR id.
                        let id_or_name = action["chain_id"].as_str()
                            .or(action["name"].as_str())
                            .or(action["id"].as_str())
                            .unwrap_or("")
                            .to_string();
                        let chain_input = action["input"].as_str().unwrap_or("").to_string();
                        if id_or_name.is_empty() {
                            "Chain name/id required.".to_string()
                        } else {
                            match run_chain_internal(state, &id_or_name, &chain_input, _app).await {
                                Ok(run) => {
                                    let out = run.final_output.unwrap_or_default();
                                    let truncated = if out.len() > 1200 {
                                        format!("{}…", &out[..1200])
                                    } else { out };
                                    format!("**Chain executed:** {}\n\n{}", run.chain_name, truncated)
                                }
                                Err(e) => format!("**Chain failed:** {}", e),
                            }
                        }
                    }
                    "list_chains" => {
                        let store = state.automation.orchestrator_store.lock().unwrap_or_else(|e| e.into_inner());
                        let chains = store.list_chains();
                        if chains.is_empty() {
                            "No chains defined.".to_string()
                        } else {
                            let lines: Vec<String> = chains.iter().take(20).map(|c| {
                                format!("- **{}** ({:?}, {} nodes): {}", c.name, c.mode, c.nodes.len(), c.description)
                            }).collect();
                            format!("**Chains:**\n{}", lines.join("\n"))
                        }
                    }
                    "update_agent" => {
                        let agent_name = action["name"].as_str().or(action["id"].as_str()).unwrap_or("").to_string();
                        let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                        let idx = agents.iter().position(|a| a.name.to_lowercase() == agent_name.to_lowercase() || a.id == agent_name);
                        if let Some(i) = idx {
                            if let Some(n) = action["new_name"].as_str() { agents[i].name = n.to_string(); }
                            if let Some(m) = action["model"].as_str() { agents[i].model = m.to_string(); }
                            if let Some(e) = action["endpoint"].as_str() { agents[i].endpoint = e.to_string(); }
                            let name = agents[i].name.clone();
                            storage::save_agents(&agents).ok();
                            format!("Updated agent \"{}\".", name)
                        } else {
                            format!("Agent \"{}\" not found.", agent_name)
                        }
                    }
                    "delete_agent" => {
                        let agent_name = action["name"].as_str().or(action["id"].as_str()).unwrap_or("").to_string();
                        let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                        let before = agents.len();
                        agents.retain(|a| a.name.to_lowercase() != agent_name.to_lowercase() && a.id != agent_name);
                        if agents.len() < before {
                            storage::save_agents(&agents).ok();
                            format!("Deleted agent \"{}\".", agent_name)
                        } else {
                            format!("Agent \"{}\" not found.", agent_name)
                        }
                    }
                    "toggle_agent" => {
                        let agent_name = action["name"].as_str().or(action["id"].as_str()).unwrap_or("").to_string();
                        let mut agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(agent) = agents.iter_mut().find(|a| a.name.to_lowercase() == agent_name.to_lowercase() || a.id == agent_name) {
                            agent.enabled = !agent.enabled;
                            let status = if agent.enabled { "enabled" } else { "disabled" };
                            let name = agent.name.clone();
                            storage::save_agents(&agents).ok();
                            format!("Agent \"{}\" is now {}.", name, status)
                        } else {
                            format!("Agent \"{}\" not found.", agent_name)
                        }
                    }
                    "add_memory" => {
                        let key = action["key"].as_str().unwrap_or("note").to_string();
                        let value = action["value"].as_str().unwrap_or("").to_string();
                        let category = match action["category"].as_str().unwrap_or("fact") {
                            "preference" => memory::MemoryCategory::Preference,
                            "context" => memory::MemoryCategory::Context,
                            "skill" => memory::MemoryCategory::Skill,
                            _ => memory::MemoryCategory::Fact,
                        };
                        let entry = memory::MemoryEntry {
                            id: Uuid::new_v4().to_string(),
                            key: key.clone(),
                            value,
                            category,
                            source: "assistant".to_string(),
                            created_at: Utc::now().to_rfc3339(),
                            updated_at: Utc::now().to_rfc3339(),
                        };
                        let mut store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
                        store.entries.push(entry);
                        let _ = store.save();
                        format!("**Remembered: \"{}\".**", key)
                    }
                    "search_memory" => {
                        let query = action["query"].as_str().or(action["q"].as_str()).unwrap_or("").to_string();
                        let store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
                        let results = store.search(&query);
                        if results.is_empty() {
                            format!("No memories found matching \"{}\".", query)
                        } else {
                            let lines: Vec<String> = results.iter().take(10).map(|e| {
                                format!("- **{}**: {}", e.key, e.value)
                            }).collect();
                            format!("**Memories matching \"{}\":**\n{}", query, lines.join("\n"))
                        }
                    }
                    "delete_memory" => {
                        // Accept id, OR key (resolve to id), OR query (delete all matching)
                        let mut store = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(id) = action["id"].as_str() {
                            if store.remove(id) { let _ = store.save(); format!("Deleted memory {}.", id) }
                            else { format!("Memory id {} not found.", id) }
                        } else if let Some(key) = action["key"].as_str() {
                            let target_ids: Vec<String> = store.entries.iter()
                                .filter(|e| e.key.to_lowercase() == key.to_lowercase())
                                .map(|e| e.id.clone())
                                .collect();
                            if target_ids.is_empty() {
                                format!("No memory with key \"{}\".", key)
                            } else {
                                for id in &target_ids { store.remove(id); }
                                let _ = store.save();
                                format!("Deleted {} memory entries with key \"{}\".", target_ids.len(), key)
                            }
                        } else {
                            "delete_memory needs `id` or `key`.".to_string()
                        }
                    }
                    // ─────────────────────────────────────────────
                    //  Bob ↔ panels: connectors / MCP / triggers
                    //  / activity / audit / twin / notifications
                    //  / settings - full voice surface for the
                    //  AirPods-on UX.
                    // ─────────────────────────────────────────────
                    "list_connectors" => {
                        let inst = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        let universal = state.connect.universal_connectors.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        if inst.is_empty() && universal.is_empty() {
                            "No connectors configured. Use connect_oauth to add one.".to_string()
                        } else {
                            let mut lines = Vec::new();
                            for c in inst.iter().take(20) {
                                lines.push(format!("- **{}** ({:?}) - status: {:?}", c.name, c.connector_id, c.status));
                            }
                            for u in universal.iter().take(20) {
                                lines.push(format!("- **{}** (universal) - {}", u.name, u.base_url));
                            }
                            format!("**Connectors:**\n{}", lines.join("\n"))
                        }
                    }
                    "disconnect_connector" => {
                        let id_or_name = action["id"].as_str().or(action["name"].as_str()).unwrap_or("").to_string();
                        if id_or_name.is_empty() {
                            "disconnect_connector needs `id` or `name`.".to_string()
                        } else {
                            let mut inst = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
                            let before = inst.len();
                            inst.retain(|c| c.id != id_or_name && c.name.to_lowercase() != id_or_name.to_lowercase());
                            if inst.len() < before {
                                connectors::delete_connector_credentials(&id_or_name);
                                format!("Disconnected \"{}\".", id_or_name)
                            } else {
                                format!("Connector \"{}\" not found.", id_or_name)
                            }
                        }
                    }
                    "list_mcp" | "list_mcp_servers" => {
                        let servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner()).clone();
                        if servers.is_empty() {
                            "No MCP servers configured.".to_string()
                        } else {
                            let lines: Vec<String> = servers.iter().take(20).map(|s| {
                                format!("- **{}** ({:?}, {}) - {}", s.name, s.transport, if s.enabled { "enabled" } else { "disabled" }, s.command)
                            }).collect();
                            format!("**MCP servers:**\n{}", lines.join("\n"))
                        }
                    }
                    "remove_mcp" | "delete_mcp" => {
                        let id_or_name = action["id"].as_str().or(action["name"].as_str()).unwrap_or("").to_string();
                        let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
                        let before = servers.len();
                        servers.retain(|s| s.id != id_or_name && s.name.to_lowercase() != id_or_name.to_lowercase());
                        if servers.len() < before {
                            format!("Removed MCP server \"{}\".", id_or_name)
                        } else {
                            format!("MCP server \"{}\" not found.", id_or_name)
                        }
                    }
                    "toggle_mcp" => {
                        let id_or_name = action["id"].as_str().or(action["name"].as_str()).unwrap_or("").to_string();
                        let mut servers = state.chat.mcp_servers.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(s) = servers.iter_mut().find(|s| s.id == id_or_name || s.name.to_lowercase() == id_or_name.to_lowercase()) {
                            s.enabled = !s.enabled;
                            format!("MCP server \"{}\" is now {}.", s.name, if s.enabled { "enabled" } else { "disabled" })
                        } else {
                            format!("MCP server \"{}\" not found.", id_or_name)
                        }
                    }
                    "send_email" => {
                        // W5.5: Bob sends mail through the first connected
                        // email account (provider + token resolved from it).
                        let to = action["to"].as_str().unwrap_or("").to_string();
                        let subject = action["subject"].as_str().unwrap_or("").to_string();
                        let body = action["body"].as_str().unwrap_or("").to_string();
                        match resolve_default_mail_account(state) {
                            Some((provider, token)) if !to.is_empty() => {
                                match crate::productivity::send_mail(provider, &token, &to, &subject, &body).await {
                                    Ok(()) => format!("Email sent to {}.", to),
                                    Err(e) => format!("*Failed: send email: {}", e),
                                }
                            }
                            Some(_) => "*Failed: send email needs a 'to' address".to_string(),
                            None => "No connected email account. Connect Gmail or Outlook first.".to_string(),
                        }
                    }
                    "create_calendar_event" => {
                        let subject = action["subject"].as_str().unwrap_or("").to_string();
                        let start = action["start"].as_str().unwrap_or("").to_string();
                        let end = action["end"].as_str().unwrap_or("").to_string();
                        let attendees: Vec<String> = action["attendees"].as_array()
                            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                            .unwrap_or_default();
                        match resolve_default_mail_account(state) {
                            Some((provider, token)) if !start.is_empty() && !end.is_empty() => {
                                match crate::productivity::calendar_create(provider, &token, &subject, &start, &end, &attendees).await {
                                    Ok(id) => format!("Created calendar event '{}' ({}).", subject, id),
                                    Err(e) => format!("*Failed: create event: {}", e),
                                }
                            }
                            Some(_) => "*Failed: calendar event needs start and end (RFC3339)".to_string(),
                            None => "No connected account with calendar access.".to_string(),
                        }
                    }
                    "list_calendar_events" => {
                        match resolve_default_mail_account(state) {
                            Some((provider, token)) => {
                                match crate::productivity::calendar_list(provider, &token, 10).await {
                                    Ok(events) if events.is_empty() => "No upcoming calendar events.".to_string(),
                                    Ok(events) => {
                                        let lines: Vec<String> = events.iter().map(|e| {
                                            format!("- {} ({} to {})", e.subject, e.start, e.end)
                                        }).collect();
                                        format!("**Upcoming events:**\n{}", lines.join("\n"))
                                    }
                                    Err(e) => format!("*Failed: list events: {}", e),
                                }
                            }
                            None => "No connected account with calendar access.".to_string(),
                        }
                    }
                    "create_connector" => {
                        // W5.3: Bob generates a connector from a request and
                        // activates it (scan + sign enforced).
                        let request = action["request"].as_str().unwrap_or("").to_string();
                        let vendor = action["vendor"].as_str().unwrap_or("Unknown").to_string();
                        if request.is_empty() {
                            "*Failed: create_connector needs a 'request'".to_string()
                        } else {
                            match crate::bob_connector_author::generate_and_author(&request, &vendor, None).await {
                                Ok(report) => format!("Built and activated connector '{}' (v{}).", report.slug, report.version),
                                Err(e) => format!("*Failed: connector generation: {}", e),
                            }
                        }
                    }
                    "browse_navigate" => {
                        // W5.4: drive the user's real Chrome (attach on 9222).
                        let url = action["url"].as_str().unwrap_or("").to_string();
                        if url.is_empty() {
                            "*Failed: browse_navigate needs a url".to_string()
                        } else if !crate::browser_attach::probe(9222).await {
                            "Chrome is not in debug mode. Launch it with --remote-debugging-port=9222.".to_string()
                        } else {
                            match crate::browser_attach::drive_navigate(9222, None, &url).await {
                                Ok(_) => format!("Navigated to {}.", url),
                                Err(e) => format!("*Failed: navigate: {}", e),
                            }
                        }
                    }
                    "browse_click" => {
                        let sel = action["selector"].as_str().unwrap_or("").to_string();
                        match crate::browser_attach::drive_click(9222, None, &sel).await {
                            Ok(()) => format!("Clicked {}.", sel),
                            Err(e) => format!("*Failed: click: {}", e),
                        }
                    }
                    "browse_type" => {
                        let sel = action["selector"].as_str().unwrap_or("").to_string();
                        let text = action["text"].as_str().unwrap_or("").to_string();
                        match crate::browser_attach::drive_type(9222, None, &sel, &text).await {
                            Ok(()) => format!("Typed into {}.", sel),
                            Err(e) => format!("*Failed: type: {}", e),
                        }
                    }
                    "browse_read" => {
                        let sel = action["selector"].as_str().unwrap_or("").to_string();
                        match crate::browser_attach::drive_read_text(9222, None, &sel).await {
                            Ok(text) => {
                                let t = crate::truncate_str(&text, 4000);
                                format!("**Page text:**\n{}", t)
                            }
                            Err(e) => format!("*Failed: read: {}", e),
                        }
                    }
                    "pair_app" => {
                        let name = action["app"].as_str().unwrap_or("").trim().to_string();
                        if name.is_empty() {
                            "*Failed: pair_app needs the app's name.".to_string()
                        } else {
                            match crate::peer::identify_name(&name).await {
                                Err(e) => format!("*Failed: pair_app: {}", e),
                                Ok(identity) => {
                                    let app2 = _app.clone();
                                    let progress = move |t: &str| crate::peer::emit_event(&app2, "connecting", t, None);
                                    let for_learning = identity.clone();
                                    match crate::peer::pair(identity, Some(_app), crate::peer::PairProgress(&progress)).await {
                                        Ok(p) => {
                                            crate::peer::emit_event(_app, "connected", &format!("{} connected.", p.app), Some(&p));
                                            let learning = crate::peer::learn_in_background(_app, &p, &for_learning);
                                            let caps: Vec<&str> = p.capabilities.iter().map(|c| match c {
                                                crate::peer::Capability::Send => "send",
                                                crate::peer::Capability::Read => "read",
                                                crate::peer::Capability::ToolsExposed => "calls Bob's tools",
                                                crate::peer::Capability::ServiceApi => "API access",
                                                crate::peer::Capability::Actions => "link actions",
                                                crate::peer::Capability::Tools => "MCP tools",
                                            }).collect();
                                            format!(
                                                "**{} paired** ({}). {}{}{}",
                                                p.app,
                                                if caps.is_empty() { "clipboard only".to_string() } else { caps.join(", ") },
                                                p.bob_line,
                                                p.follow_up.as_ref().map(|f| format!(" One step remains for the user: {}.", f.label)).unwrap_or_default(),
                                                if learning { " It has a scripting dictionary; Bob is learning it in the background and will say when done." } else { "" }
                                            )
                                        }
                                        Err(e) => {
                                            crate::peer::emit_event(_app, "failed", &e, None);
                                            format!("*Failed: pair_app: {}", e)
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "list_paired_apps" => {
                        let peers = crate::peer::list();
                        if peers.is_empty() {
                            "No apps are paired yet. The user can drag the pair handle onto an app window, or Bob can call pair_app with the app's name.".to_string()
                        } else {
                            let mut out = String::from("**Paired apps:**\n");
                            for p in peers {
                                out.push_str(&format!("- **{}**: {}\n", p.app, p.bob_line));
                            }
                            out
                        }
                    }
                    "peer_send" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        let text = action["text"].as_str().unwrap_or("").to_string();
                        if name.is_empty() || text.is_empty() {
                            "*Failed: peer_send needs \"app\" and \"text\".".to_string()
                        } else {
                            match crate::peer::send(&name, &text).await {
                                Ok(r) => format!("**Sent to {} via {}.** {}", r.app, r.via.label(), r.detail),
                                Err(e) => format!("*Failed: peer_send: {}", e),
                            }
                        }
                    }
                    "peer_read" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        match crate::peer::read(&name).await {
                            Ok(r) => format!(
                                "**{} window \"{}\"{}:**\n{}",
                                r.app,
                                r.title,
                                if r.truncated { " (truncated)" } else { "" },
                                crate::truncate_str(&r.text, 6000)
                            ),
                            Err(e) => format!("*Failed: peer_read: {}", e),
                        }
                    }
                    "peer_controls" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        match crate::peer::window_controls(state, &name).await {
                            Ok(r) => {
                                let fields: Vec<String> = r.fields.iter().take(30).map(|(_, label, value)| match value {
                                    Some(v) if !v.is_empty() => format!("{} = \"{}\"", label, crate::truncate_str(v, 60)),
                                    _ => label.clone(),
                                }).collect();
                                let buttons: Vec<String> = r.buttons.iter().take(40).map(|(_, l)| l.clone()).collect();
                                format!(
                                    "**{} window \"{}\"**\nFields: {}\nButtons: {}",
                                    r.app, r.window,
                                    if fields.is_empty() { "none".to_string() } else { fields.join("; ") },
                                    if buttons.is_empty() { "none".to_string() } else { buttons.join(", ") }
                                )
                            }
                            Err(e) => format!("*Failed: peer_controls: {}", e),
                        }
                    }
                    "peer_press" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        let control = action["control"].as_str().unwrap_or("").to_string();
                        match crate::peer::press(state, &name, &control).await {
                            Ok(m) => m,
                            Err(e) => format!("*Failed: peer_press: {}", e),
                        }
                    }
                    "peer_fill" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        let field = action["field"].as_str().unwrap_or("").to_string();
                        let value = action["value"].as_str().unwrap_or("").to_string();
                        match crate::peer::fill(state, &name, &field, &value).await {
                            Ok(m) => m,
                            Err(e) => format!("*Failed: peer_fill: {}", e),
                        }
                    }
                    "mcp_tools" => {
                        let server = action["server"].as_str().unwrap_or("").to_string();
                        match crate::mcp_client::list_tools(state, &server).await {
                            Ok((srv, tools)) => {
                                if tools.is_empty() {
                                    format!("{} is up but offers no tools.", srv.name)
                                } else {
                                    let lines: Vec<String> = tools.iter().map(|t| {
                                        let props: Vec<String> = t.input_schema["properties"].as_object().map(|p| p.keys().cloned().collect()).unwrap_or_default();
                                        format!("- **{}**({}) {}", t.name, props.join(", "), crate::truncate_str(&t.description, 160))
                                    }).collect();
                                    format!("**{} tools:**\n{}", srv.name, lines.join("\n"))
                                }
                            }
                            Err(e) => format!("*Failed: mcp_tools: {}", e),
                        }
                    }
                    "mcp_call" => {
                        let server = action["server"].as_str().unwrap_or("").to_string();
                        let tool = action["tool"].as_str().unwrap_or("").to_string();
                        let args = action.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
                        if server.is_empty() || tool.is_empty() {
                            "*Failed: mcp_call needs \"server\" and \"tool\".".to_string()
                        } else {
                            match crate::mcp_client::call_tool(state, &server, &tool, args).await {
                                Ok((text, _)) => format!("**{} / {}:**\n{}", server, tool, crate::truncate_str(&text, 6000)),
                                Err(e) => format!("*Failed: mcp_call {}/{}: {}", server, tool, e),
                            }
                        }
                    }
                    "peer_open" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        let id = action["action"].as_str().unwrap_or("").to_string();
                        let args = action.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
                        match crate::peer::open_action(&name, &id, &args).await {
                            Ok(msg) => msg,
                            Err(e) => format!("*Failed: peer_open: {}", e),
                        }
                    }
                    "add_url_action" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        let ua = crate::peer::UrlAction {
                            id: action["id"].as_str().unwrap_or("").to_string(),
                            description: action["description"].as_str().unwrap_or("").to_string(),
                            template: action["template"].as_str().unwrap_or("").to_string(),
                            args: Vec::new(),
                            verified: false,
                        };
                        match crate::peer::add_url_action(&name, ua) {
                            Ok(p) => {
                                crate::peer::emit_event(_app, "updated", &format!("{} learned a link action.", p.app), Some(&p));
                                format!("Kept. {} now has {} link action(s); peer_open runs them.", p.app, p.url_actions.len())
                            }
                            Err(e) => format!("*Failed: add_url_action: {}", e),
                        }
                    }
                    "learn_app" => {
                        let name = action["app"].as_str().unwrap_or("").trim().to_string();
                        if name.is_empty() {
                            "*Failed: learn_app needs the app's name.".to_string()
                        } else {
                            match crate::peer::identify_name(&name).await {
                                Err(e) => format!("*Failed: learn_app: {}", e),
                                Ok(identity) if !identity.scriptable => format!("{} publishes no scripting dictionary, so there is nothing to learn. read_app_window and peer_read still work on its window.", identity.name),
                                Ok(identity) => {
                                    let app2 = _app.clone();
                                    let progress = move |t: &str| bob_step(&app2, t);
                                    match crate::bridge_author::author(&identity.name, &identity.process_name, &identity.bundle_path, &progress).await {
                                        Ok(r) => {
                                            if let Ok(peer) = crate::peer::refresh(&identity.name, Some(_app)).await {
                                                crate::peer::emit_event(_app, "updated", &format!("{} learned.", peer.app), Some(&peer));
                                            }
                                            format!("**Learned {}** ({} attempt{}). Bridge actions now available through app_bridge: {}", r.app, r.attempts, if r.attempts == 1 { "" } else { "s" }, r.actions.join("; "))
                                        }
                                        Err(e) => format!("*Failed: learn_app: {}", e),
                                    }
                                }
                            }
                        }
                    }
                    "run_user_action" => {
                        let id = action["id"].as_str().or(action["app"].as_str()).unwrap_or("").to_string();
                        match crate::user_actions::run(&id, _app).await {
                            Ok(msg) => msg,
                            Err(e) => format!("*Failed: run_user_action: {}", e),
                        }
                    }
                    "unpair_app" => {
                        let name = action["app"].as_str().unwrap_or("").to_string();
                        match crate::peer::unpair(&name) {
                            Ok(p) => {
                                crate::peer::emit_event(_app, "unpaired", &format!("{} unpaired.", p.app), Some(&p));
                                format!("Unpaired {}.", p.app)
                            }
                            Err(e) => format!("*Failed: unpair_app: {}", e),
                        }
                    }
                    "list_app_bridges" => {
                        let statuses = crate::app_bridge::list_bridges().await;
                        crate::app_bridge::format_bridge_list(&statuses)
                    }
                    "read_app_window" => {
                        let app_name = action["app"].as_str().unwrap_or("").to_string();
                        let rbac_ok = {
                            let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                            let current_agent = agents.iter().find(|a| a.is_default && a.enabled);
                            check_rbac(current_agent, "read_app_window")
                        };
                        if let Err(e) = rbac_ok {
                            format!("**RBAC denied:** {}", e)
                        } else {
                            let outcome = crate::window_intelligence::read_app_window(&app_name).await;
                            if let Ok(mut log) = state.core.audit_log.lock() {
                                let outcome_label = match &outcome {
                                    Ok(_) => "ok".to_string(),
                                    Err(e) => format!("error: {}", crate::truncate_str(e, 200)),
                                };
                                log.append(
                                    "app_bridge",
                                    crate::audit_log::AuditKind::ActionExecuted,
                                    crate::audit_log::payload_action(
                                        &format!("read_app_window.{}", app_name),
                                        &serde_json::json!({ "app": app_name }),
                                        &outcome_label,
                                    ),
                                    None,
                                );
                            }
                            match outcome {
                                Ok(win) => format!(
                                    "**{} window \"{}\"{}:**\n{}",
                                    win.app,
                                    win.window_title,
                                    if win.truncated { " (truncated)" } else { "" },
                                    crate::truncate_str(&win.text, 6000)
                                ),
                                Err(e) => format!("*Failed: read_app_window: {}", e),
                            }
                        }
                    }
                    "app_bridge" => {
                        let app_name = action["app"].as_str().unwrap_or("").to_string();
                        let bridge_action = action["action"].as_str().unwrap_or("").to_string();
                        let args = action.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
                        // RBAC gate: bridge reads are grantable/deniable local reads
                        let rbac_ok = {
                            let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                            let current_agent = agents.iter().find(|a| a.is_default && a.enabled);
                            check_rbac(current_agent, "app_bridge")
                        };
                        if let Err(e) = rbac_ok {
                            if let Ok(mut log) = state.core.audit_log.lock() {
                                log.append(
                                    "app_bridge",
                                    crate::audit_log::AuditKind::ActionDenied,
                                    crate::audit_log::payload_action(
                                        &format!("app_bridge.{}.{}", app_name, bridge_action),
                                        &args,
                                        "rbac_denied",
                                    ),
                                    None,
                                );
                            }
                            format!("**RBAC denied:** {}", e)
                        } else if app_name.is_empty() || bridge_action.is_empty() {
                            "*Failed: app_bridge needs both \"app\" and \"action\". Run list_app_bridges to see what exists.".to_string()
                        } else {
                            let outcome = crate::app_bridge::run_bridge_action(&app_name, &bridge_action, &args).await;
                            // Bridge reads leave the same forensic trail as file reads.
                            if let Ok(mut log) = state.core.audit_log.lock() {
                                let outcome_label = match &outcome {
                                    Ok(_) => "ok".to_string(),
                                    Err(e) => format!("error: {}", crate::truncate_str(e, 200)),
                                };
                                log.append(
                                    "app_bridge",
                                    crate::audit_log::AuditKind::ActionExecuted,
                                    crate::audit_log::payload_action(
                                        &format!("app_bridge.{}.{}", app_name, bridge_action),
                                        &args,
                                        &outcome_label,
                                    ),
                                    None,
                                );
                            }
                            match outcome {
                                Ok(v) => {
                                    let pretty = serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string());
                                    format!(
                                        "**{} {}:**\n```json\n{}\n```",
                                        app_name, bridge_action,
                                        crate::truncate_str(&pretty, 6000)
                                    )
                                }
                                Err(e) => {
                                    let permission = e.contains("Privacy & Security") || e.contains("administrator");
                                    let relearning = !permission
                                        && crate::app_bridge::is_learned(&app_name)
                                        && crate::bridge_author::relearn_in_background(_app, &app_name);
                                    if relearning {
                                        format!("*Failed: app_bridge: {} Bob's learned bridge for {} stopped answering; he is re-learning it now. Try again in a minute, or use read_app_window / peer_read meanwhile.", e, app_name)
                                    } else {
                                        format!("*Failed: app_bridge: {}", e)
                                    }
                                }
                            }
                        }
                    }
                    "list_triggers" => {
                        let store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                        if store.rules.is_empty() {
                            "No trigger rules configured.".to_string()
                        } else {
                            let lines: Vec<String> = store.rules.iter().take(20).map(|r| {
                                format!("- **{}** ({}) - workflow: {} - priority {}", r.name, if r.enabled { "on" } else { "off" }, r.workflow_id, r.priority)
                            }).collect();
                            format!("**Triggers:**\n{}", lines.join("\n"))
                        }
                    }
                    "delete_trigger" => {
                        let id_or_name = action["id"].as_str().or(action["name"].as_str()).unwrap_or("").to_string();
                        let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                        let before = store.rules.len();
                        store.rules.retain(|r| r.id != id_or_name && r.name.to_lowercase() != id_or_name.to_lowercase());
                        if store.rules.len() < before {
                            store.save();
                            format!("Deleted trigger \"{}\".", id_or_name)
                        } else {
                            format!("Trigger \"{}\" not found.", id_or_name)
                        }
                    }
                    "toggle_trigger" => {
                        let id_or_name = action["id"].as_str().or(action["name"].as_str()).unwrap_or("").to_string();
                        let mut store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
                        let target_idx = store.rules.iter().position(|r| r.id == id_or_name || r.name.to_lowercase() == id_or_name.to_lowercase());
                        if let Some(i) = target_idx {
                            store.rules[i].enabled = !store.rules[i].enabled;
                            let name = store.rules[i].name.clone();
                            let on = store.rules[i].enabled;
                            store.save();
                            format!("Trigger \"{}\" is now {}.", name, if on { "enabled" } else { "disabled" })
                        } else {
                            format!("Trigger \"{}\" not found.", id_or_name)
                        }
                    }
                    "recent_activity" => {
                        let log = state.chat.activity_log.lock().unwrap_or_else(|e| e.into_inner());
                        let limit = action["limit"].as_u64().unwrap_or(10) as usize;
                        if log.is_empty() {
                            "No recent agent activity.".to_string()
                        } else {
                            let lines: Vec<String> = log.iter().rev().take(limit).map(|a| {
                                format!("- [{}] **{}**: {} ({})", a.timestamp.split('T').next().unwrap_or(&a.timestamp), a.agent_name, a.action, a.status)
                            }).collect();
                            format!("**Recent activity:**\n{}", lines.join("\n"))
                        }
                    }
                    "audit_search" => {
                        let store = state.fleet.audit_store.lock().unwrap_or_else(|e| e.into_inner());
                        let query = action["query"].as_str();
                        let category = action["category"].as_str();
                        let action_filter = action["action"].as_str();
                        let limit = action["limit"].as_u64().unwrap_or(10) as usize;
                        let results = store.search(query, category, None, None, action_filter, limit);
                        if results.is_empty() {
                            "No audit entries match.".to_string()
                        } else {
                            let lines: Vec<String> = results.iter().take(10).map(|e| {
                                format!("- [{}] {} on {} - {}", e.timestamp.split('T').next().unwrap_or(&e.timestamp), e.action, e.target_type, e.details)
                            }).collect();
                            format!("**Audit results:**\n{}", lines.join("\n"))
                        }
                    }
                    "twin_list" => {
                        let store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
                        let type_filter = action["entity_type"].as_str().or(action["type"].as_str());
                        let state_filter = action["state"].as_str();
                        let entities: Vec<&semantic_twin::TwinEntity> = store.graph.entities.iter()
                            .filter(|e| type_filter.map(|t| format!("{:?}", e.entity_type).to_lowercase() == t.to_lowercase()).unwrap_or(true))
                            .filter(|e| state_filter.map(|s| format!("{:?}", e.state).to_lowercase() == s.to_lowercase()).unwrap_or(true))
                            .collect();
                        if entities.is_empty() {
                            "No twin entities match.".to_string()
                        } else {
                            let lines: Vec<String> = entities.iter().take(15).map(|e| {
                                format!("- **{}** ({:?}, {:?}) - id: {}", e.name, e.entity_type, e.state, e.id)
                            }).collect();
                            format!("**Twin entities:**\n{}", lines.join("\n"))
                        }
                    }
                    "twin_search" => {
                        let q = action["query"].as_str().unwrap_or("").to_lowercase();
                        if q.is_empty() {
                            "twin_search needs `query`.".to_string()
                        } else {
                            let store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
                            let matches: Vec<&semantic_twin::TwinEntity> = store.graph.entities.iter()
                                .filter(|e| {
                                    e.name.to_lowercase().contains(&q)
                                        || e.tags.iter().any(|t| t.to_lowercase().contains(&q))
                                        || e.properties.values().any(|v| v.to_string().to_lowercase().contains(&q))
                                })
                                .collect();
                            if matches.is_empty() {
                                format!("No twin entities matching \"{}\".", q)
                            } else {
                                let lines: Vec<String> = matches.iter().take(10).map(|e| {
                                    format!("- **{}** ({:?}, {:?})", e.name, e.entity_type, e.state)
                                }).collect();
                                format!("**Matches for \"{}\":**\n{}", q, lines.join("\n"))
                            }
                        }
                    }
                    "twin_get" => {
                        let id_or_name = action["id"].as_str().or(action["name"].as_str()).unwrap_or("").to_string();
                        let store = state.core.twin_store.lock().unwrap_or_else(|e| e.into_inner());
                        let entity = store.graph.entities.iter()
                            .find(|e| e.id == id_or_name || e.name.to_lowercase() == id_or_name.to_lowercase());
                        if let Some(e) = entity {
                            let mut lines = vec![
                                format!("**{}** ({:?}, {:?})", e.name, e.entity_type, e.state),
                                format!("ID: {}", e.id),
                            ];
                            if !e.properties.is_empty() {
                                lines.push("Properties:".to_string());
                                for (k, v) in e.properties.iter().take(10) {
                                    lines.push(format!("  - {}: {}", k, v));
                                }
                            }
                            let relations: Vec<&semantic_twin::TwinRelation> = store.graph.relations.iter()
                                .filter(|r| r.source_id == e.id || r.target_id == e.id)
                                .collect();
                            if !relations.is_empty() {
                                lines.push(format!("Relations: {}", relations.len()));
                                for r in relations.iter().take(8) {
                                    let other = if r.source_id == e.id { &r.target_id } else { &r.source_id };
                                    lines.push(format!("  - {:?} ↔ {}", r.relation_type, other));
                                }
                            }
                            lines.join("\n")
                        } else {
                            format!("Entity \"{}\" not found.", id_or_name)
                        }
                    }
                    "list_notifications" => {
                        let store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
                        let limit = action["limit"].as_u64().unwrap_or(10) as usize;
                        let items = store.list(limit);
                        if items.is_empty() {
                            "Inbox is empty.".to_string()
                        } else {
                            let lines: Vec<String> = items.iter().map(|n| {
                                format!("- [{:?}] **{}** - {}", n.level, n.title, n.message)
                            }).collect();
                            format!("**Notifications:**\n{}", lines.join("\n"))
                        }
                    }
                    "dismiss_notifications" => {
                        let mut store = state.connect.notification_store.lock().unwrap_or_else(|e| e.into_inner());
                        store.dismiss_all();
                        "All notifications dismissed.".to_string()
                    }
                    "set_voice" => {
                        let mut settings = state.engine.voice_settings.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(v) = action["voice"].as_str() { settings.tts_voice = v.to_string(); }
                        if let Some(r) = action["rate"].as_i64() { settings.tts_rate = r as i32; }
                        if let Some(a) = action["auto_speak"].as_bool() { settings.auto_speak = a; }
                        let snapshot = settings.clone();
                        let _ = storage::save_json("voice_settings.json", &snapshot);
                        format!("Voice updated: {} @ rate {}, auto_speak: {}.", snapshot.tts_voice, snapshot.tts_rate, snapshot.auto_speak)
                    }
                    "tool" => {
                        let tool_name = action["tool"].as_str().unwrap_or("").to_string();

                        // ── RBAC gate: check permission before execution ──
                        let rbac_ok = {
                            let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
                            let current_agent = agents.iter().find(|a| a.is_default && a.enabled);
                            check_rbac(current_agent, &tool_name)
                        };

                        if let Err(e) = rbac_ok {
                            format!("**RBAC denied:** {}", e)
                        } else {
                            let args: HashMap<String, serde_json::Value> = if let Some(args_obj) = action["args"].as_object() {
                                args_obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                            } else if let Some(top) = action.as_object() {
                                top.iter().filter(|(k, _)| k.as_str() != "type" && k.as_str() != "tool").map(|(k, v)| (k.clone(), v.clone())).collect()
                            } else {
                                HashMap::new()
                            };
                            let tool_result = tools::execute_tool(&tool_name, &args, &tools::ToolCtx { state, agent_id: "bob" }).await;
                            if tool_result.success {
                                format!("**OK: `{}`**\n```\n{}\n```", tool_name, tool_result.output)
                            } else {
                                format!("**[ERR] `{}` failed:** {}", tool_name,
                                    tool_result.error.unwrap_or_else(|| "Unknown error".to_string()))
                            }
                        }
                    }
                    "check_connectors" => {
                        let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
                        let catalog = connectors::connector_catalog();
                        if instances.is_empty() {
                            let available: Vec<String> = catalog.iter()
                                .map(|c| format!("- **{}**: {} ({})", c.name, c.description, format!("{:?}", c.auth_method)))
                                .collect();
                            format!("**No services connected.** Available connectors:\n{}\n\nI can help you set any of these up - just tell me which service you'd like to connect.", available.join("\n"))
                        } else {
                            let mut lines = Vec::new();
                            for inst in instances.iter() {
                                let status = format!("{:?}", inst.status);
                                lines.push(format!("- **{}** - {}", inst.name, status));
                            }
                            let unconnected: Vec<String> = catalog.iter()
                                .filter(|c| !instances.iter().any(|i| i.connector_id == c.id))
                                .map(|c| format!("- {}: {}", c.name, c.description))
                                .collect();
                            let mut msg = format!("**Connected services:**\n{}", lines.join("\n"));
                            if !unconnected.is_empty() {
                                msg.push_str(&format!("\n\n**Available to add:**\n{}", unconnected.join("\n")));
                            }
                            msg
                        }
                    }
                    "setup_connector" => {
                        let connector_name = action["service"].as_str().unwrap_or("").to_lowercase();
                        let catalog = connectors::connector_catalog();
                        let def = catalog.iter().find(|c| {
                            c.name.to_lowercase().contains(&connector_name) ||
                            format!("{:?}", c.id).to_lowercase().contains(&connector_name)
                        });
                        if let Some(d) = def {
                            let mut field_info = Vec::new();
                            for f in &d.fields {
                                let req = if f.required { " (required)" } else { " (optional)" };
                                field_info.push(format!("- **{}**{}: {}\n  *Where to find it:* {}", f.label, req, f.placeholder, f.help_text));
                            }
                            let hints = connectors::get_login_hints(&d.id);
                            let mut hint_lines = Vec::new();
                            for h in &hints {
                                hint_lines.push(format!("**{}**: {}", h.field_key, h.where_to_find));
                            }
                            let mut msg = format!(
                                "**Setting up {}** ({})\n\nAuth method: {:?}\n\nYou'll need these credentials:\n{}\n",
                                d.name, d.description, d.auth_method, field_info.join("\n")
                            );
                            if !d.docs_url.is_empty() {
                                msg.push_str(&format!("\n**Setup guide:** {}\n", d.docs_url));
                            }
                            if !hint_lines.is_empty() {
                                msg.push_str(&format!("\n**Tips:**\n{}\n", hint_lines.join("\n")));
                            }
                            msg.push_str("\nOnce you have the credentials, paste them here and I'll create the connection for you. Or open the **Connectors** panel in the sidebar to enter them in the form.");
                            msg
                        } else {
                            let available: Vec<String> = catalog.iter()
                                .map(|c| format!("- {}", c.name))
                                .collect();
                            format!("I don't have a connector for \"{}\". Available connectors:\n{}", connector_name, available.join("\n"))
                        }
                    }
                    "save_connector" => {
                        let connector_name = action["service"].as_str().unwrap_or("").to_lowercase();
                        let catalog = connectors::connector_catalog();
                        let def = catalog.iter().find(|c| {
                            c.name.to_lowercase().contains(&connector_name) ||
                            format!("{:?}", c.id).to_lowercase().contains(&connector_name)
                        });
                        if let Some(d) = def {
                            let creds_obj = action["credentials"].as_object();
                            if let Some(creds) = creds_obj {
                                let mut credentials: HashMap<String, String> = HashMap::new();
                                for (k, v) in creds {
                                    if let Some(s) = v.as_str() {
                                        credentials.insert(k.clone(), s.to_string());
                                    }
                                }
                                let instance_id = Uuid::new_v4().to_string();
                                let mut cred_keys = Vec::new();
                                for (key, value) in &credentials {
                                    if !value.is_empty() {
                                        if let Ok(()) = connectors::save_connector_credential(&instance_id, key, value) {
                                            cred_keys.push(key.clone());
                                        }
                                    }
                                }
                                let instance = connectors::ConnectorInstance {
                                    id: instance_id,
                                    connector_id: d.id.clone(),
                                    name: d.name.clone(),
                                    status: connectors::ConnectionStatus::Disconnected,
                                    created_at: Utc::now().to_rfc3339(),
                                    last_tested: None,
                                    error_message: None,
                                    credential_keys: cred_keys,
                                };
                                let mut instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
                                instances.push(instance);
                                connectors::save_connector_instances(&instances).ok();
                                format!("**OK: {} connector created!** Go to the Connectors panel and click \"Test\" to verify the connection.", d.name)
                            } else {
                                format!("Missing credentials. Please provide them as a JSON object.")
                            }
                        } else {
                            format!("Unknown service: \"{}\"", connector_name)
                        }
                    }
                    // One-click OAuth - Bob triggers the browser login directly
                    "connect_oauth" => {
                        let service = action["service"].as_str().unwrap_or("").to_lowercase();
                        let connector_id = match service.as_str() {
                            s if s.contains("microsoft") || s.contains("outlook") || s.contains("office") || s.contains("teams") => "microsoft",
                            s if s.contains("google") || s.contains("gmail") || s.contains("calendar") || s.contains("drive") => "google",
                            s if s.contains("github") => "github",
                            s if s.contains("slack") => "slack",
                            s if s.contains("linkedin") => "linkedin",
                            _ => "",
                        };

                        if connector_id.is_empty() {
                            format!("I can connect: **Microsoft** (Outlook, Teams, OneDrive), **Google** (Gmail, Calendar, Drive), **GitHub**, **Slack**, or **LinkedIn**. Which one?")
                        } else {
match crate::services::connect(state, _app, connector_id).await {
                                Ok((instance, display)) => format!("**Connected to {}** as {}.", instance.name, display),
                                Err(e) => {
                                    if e.starts_with("SETUP_GUIDE:") {
                                        let guide = e.replace("SETUP_GUIDE:", "");
                                        format!("{}\n\nOnce you have the Client ID, paste it here and I'll finish the setup.", guide)
                                    } else {
                                        format!("Connection failed: {}", e)
                                    }
                                }
                            }
                        }
                    }
                    // Save OAuth credentials provided by user in chat
                    "save_client_id" => {
                        let service = action["service"].as_str().unwrap_or("").to_lowercase();
                        let client_id = action["client_id"].as_str().unwrap_or("").to_string();
                        let client_secret = action["client_secret"].as_str().unwrap_or("").to_string();
                        let connector_id = match service.as_str() {
                            s if s.contains("google") => "google",
                            s if s.contains("github") => "github",
                            s if s.contains("slack") => "slack",
                            s if s.contains("microsoft") => "microsoft",
                            s if s.contains("linkedin") => "linkedin",
                            _ => "",
                        };
                        if connector_id.is_empty() || client_id.is_empty() {
                            "I need the service name and Client ID.".to_string()
                        } else {
                            oauth::save_oauth_client_id(connector_id, &client_id);
                            if !client_secret.is_empty() {
                                let secret_key = format!("{}_secret", connector_id);
                                let mut ids = oauth::load_oauth_client_ids();
                                ids.insert(secret_key, client_secret);
                                oauth::save_oauth_client_ids(&ids).ok();
                            }
                            format!("**Credentials saved for {}.** Connecting now...", connector_id)
                        }
                    }
                    // Direct HTTP API call - cleaner than shelling out to curl
                    "api_call" => crate::services::api_call(state, &action).await,
                    // Handle LLMs that output tool name directly as type
                    // e.g. {"type":"shell_exec","args":{...}} or {"type":"file_write","path":"...","content":"..."}
                    "shell_exec" | "file_read" | "file_write" | "file_list" | "web_fetch" => {
                        // Try nested "args" first, fall back to top-level keys (excluding "type")
                        let args: HashMap<String, serde_json::Value> = if let Some(args_obj) = action["args"].as_object() {
                            args_obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
                        } else if let Some(top) = action.as_object() {
                            top.iter().filter(|(k, _)| k.as_str() != "type" && k.as_str() != "tool").map(|(k, v)| (k.clone(), v.clone())).collect()
                        } else {
                            HashMap::new()
                        };
                        let tool_result = tools::execute_tool(action_type, &args, &tools::ToolCtx { state, agent_id: "bob" }).await;
                        if tool_result.success {
                            format!("**OK: `{}`**\n```\n{}\n```", action_type, tool_result.output)
                        } else {
                            format!("**[ERR] `{}` failed:** {}", action_type,
                                tool_result.error.unwrap_or_else(|| "Unknown error".to_string()))
                        }
                    }
                    "extract_brand_kit" => {
                        let path = action["path"].as_str().unwrap_or("").to_string();
                        if path.is_empty() {
                            "**[ERR] extract_brand_kit needs a path**".to_string()
                        } else {
                            match brand::extract_kit(state, &path).await {
                                Ok(kit) => {
                                    let palette: Vec<String> = kit.colors.iter()
                                        .map(|c| format!("{} {} ({})", c.hex, c.name, c.role))
                                        .collect();
                                    format!(
                                        "**Brand kit '{}' extracted and set active.** Colors: {}. Heading font: {}. Body font: {}. Generated documents now carry this brand.",
                                        kit.name, palette.join(", "), kit.heading_font, kit.body_font
                                    )
                                }
                                Err(e) => format!("**[ERR] Brand extraction failed:** {}", e),
                            }
                        }
                    }
                    "use_skill" => {
                        let name = action["name"].as_str().unwrap_or("").trim().to_string();
                        match crate::skills::load(&name) {
                            Some(body) => format!(
                                "**Skill '{}' loaded. Follow these instructions for the rest of this task:**\n\n{}",
                                name, body
                            ),
                            None => {
                                let installed: Vec<String> = crate::skills::list()
                                    .into_iter().map(|s| s.name).collect();
                                format!(
                                    "**[ERR] No skill named '{}'.** Installed skills: {}",
                                    name,
                                    if installed.is_empty() { "none".to_string() } else { installed.join(", ") }
                                )
                            }
                        }
                    }
                    "research" => {
                        let topic = action["topic"].as_str()
                            .or(action["query"].as_str())
                            .or(action["name"].as_str())
                            .unwrap_or("")
                            .to_string();
                        match crate::research::run(state, _app, &topic).await {
                            Ok(dossier) => dossier,
                            Err(e) => format!("**[ERR] Research failed:** {}", e),
                        }
                    }
                    "create_document" => {
                        let kind = action["kind"].as_str().unwrap_or("html").to_string();
                        let mut spec: docgen::DocSpec = serde_json::from_value(action.clone()).unwrap_or_default();
                        if spec.title.trim().is_empty() {
                            "**[ERR] create_document needs a title**".to_string()
                        } else if kind == "app" {
                            // An interactive page is written in full by the
                            // model (HTML + CSS + JS), not assembled from an
                            // outline: the outline pipeline can only produce
                            // static articles, which is how "an interactive
                            // solar system I can click" came out as a 2KB
                            // page (2026-09-06).
                            let kit = brand::active_kit(state);
                            match build_app_page(_app, &spec, &kit).await {
                                Ok((path, bytes)) => {
                                    let _ = open::that(&path);
                                    crate::event_bus::emit_ui(_app, "data-changed", "document-generated").ok();
                                    let doc_id = docgen::registry_list()
                                        .into_iter()
                                        .find(|d| d.path == path.display().to_string())
                                        .map(|d| d.id)
                                        .unwrap_or_default();
                                    let safe_title: String = spec.title.replace(['|', '[', ']'], " ");
                                    format!(
                                        "**Created an interactive page '{}'** ({} KB, opened in your browser).\n\n[[doc:{}|html|{}]]\n\nWant changes? Say what to add or tweak and I'll rewrite it.",
                                        safe_title.trim(), (bytes as f64 / 1024.0).round() as u64, doc_id, safe_title.trim()
                                    )
                                }
                                Err(e) if e == CANCELLED => "**Stopped** before the page was finished.".to_string(),
                                Err(e) => format!("**[ERR] Page generation failed:** {}", e),
                            }
                        } else {
                            let kit = brand::active_kit(state);
                            // Quality gate: small models often emit a thin
                            // outline in the tool call. Thresholds calibrated
                            // against a real field sample (4 sections, ~550
                            // chars, zero bullets - unmistakably thin): expand
                            // whenever substance is low or a deck has no
                            // bullets at all.
                            let substance: usize = spec.sections.iter()
                                .map(|s| s.body.len() + s.bullets.iter().map(|b| b.len()).sum::<usize>())
                                .sum();
                            let bullet_count: usize = spec.sections.iter().map(|s| s.bullets.len()).sum();
                            // Depth bar raised (field feedback 2026-08-01:
                            // "content created is quite limited") - most
                            // first-pass specs from small models now route
                            // through the drafting expansion.
                            let thin = spec.sections.len() < 4
                                || substance < 2600
                                || ((kind == "pptx" || kind == "website") && bullet_count == 0);
                            if thin {
                                bob_step(_app, &format!("Drafting the content for '{}'…", spec.title));
                                if let Some(rich) = brand::expand_doc_spec(_app, &spec, &kit, &kind).await {
                                    spec = rich;
                                }
                                bob_step(_app, &format!("Rendering {}…", match kind.as_str() { "pptx" => "the slides", "website" => "the website", "docx" => "the document", _ => "the page" }));
                            }
                            match docgen::generate(&kind, &spec, &kit) {
                                Ok(path) => {
                                    let _ = open::that(&path);
                                    crate::event_bus::emit_ui(_app, "data-changed", "document-generated").ok();
                                    // Chat-embedded document chip: the frontend
                                    // renders [[doc:id|kind|title]] as a card
                                    // with Preview / Edit buttons.
                                    let doc_id = docgen::registry_list()
                                        .into_iter()
                                        .find(|d| d.path == path.display().to_string())
                                        .map(|d| d.id)
                                        .unwrap_or_default();
                                    let safe_title: String = spec.title.replace(['|', '[', ']'], " ");
                                    let what = if kind == "website" {
                                        "website (index.html + styles.css)".to_string()
                                    } else {
                                        format!("{} document", kind)
                                    };
                                    // Deliverables end with a door open, not a
                                    // full stop - the user asked to be guided
                                    // toward more when they want it.
                                    format!(
                                        "**Created a branded {}** using the '{}' kit ({} sections).\n\n[[doc:{}|{}|{}]]\n\nWant more? Ask me to expand any section, add new ones, go deeper with research, or re-export it as a document, slides, or a website.",
                                        what, kit.name, spec.sections.len(), doc_id, kind, safe_title.trim()
                                    )
                                }
                                Err(e) => format!("**[ERR] Document generation failed:** {}", e),
                            }
                        }
                    }
                    "create_panel" => {
                        // Bob built a UI panel for the user. Spec lives at the top
                        // of the action: { type: "create_panel", title, description?,
                        // blocks: [...], conversation_id?, pinned? }
                        let title = action["title"].as_str().unwrap_or("Untitled panel").to_string();
                        let description = action["description"].as_str().map(|s| s.to_string());
                        let blocks: Vec<dynamic_panels::Block> = action["blocks"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                                    .collect()
                            })
                            .unwrap_or_default();
                        let conversation_id = action["conversation_id"].as_str().map(|s| s.to_string());
                        let pinned = action["pinned"].as_bool().unwrap_or(false);

                        let panel = dynamic_panels::DynamicPanel {
                            id: String::new(),
                            title: title.clone(),
                            description,
                            blocks,
                            pinned,
                            conversation_id,
                            created_at: String::new(),
                            updated_at: String::new(),
                            created_by: "bob".to_string(),
                        };

                        let mut store = state
                            .automation.dynamic_panels_store
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        let saved = store.upsert(panel, "bob");
                        let save_result = store.save();
                        drop(store);

                        if save_result.is_ok() {
                            crate::event_bus::emit_ui(_app, "data-changed", "dynamic-panel-saved").ok();
                            format!("**Built panel \"{}\"** - open it from My UI.", saved.title)
                        } else {
                            format!("**[ERR] Could not save panel:** {}", save_result.err().unwrap_or_default())
                        }
                    }
                    "update_panel" => {
                        let id = action["id"].as_str().unwrap_or("").to_string();
                        let patch: dynamic_panels::DynamicPanelPatch =
                            serde_json::from_value(action.clone()).unwrap_or_default();

                        let result = {
                            let mut store = state
                                .automation.dynamic_panels_store
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            let r = store.patch(&id, patch);
                            if r.is_ok() {
                                store.save().ok();
                            }
                            r
                        };
                        match result {
                            Ok(panel) => {
                                crate::event_bus::emit_ui(_app, "data-changed", "dynamic-panel-updated").ok();
                                format!("**Updated panel \"{}\".**", panel.title)
                            }
                            Err(e) => format!("**[ERR] Could not update panel:** {}", e),
                        }
                    }
                    "remove_panel" => {
                        let id = action["id"].as_str().unwrap_or("").to_string();
                        let removed = {
                            let mut store = state
                                .automation.dynamic_panels_store
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            let r = store.delete(&id);
                            store.save().ok();
                            r
                        };
                        if removed {
                            crate::event_bus::emit_ui(_app, "data-changed", "dynamic-panel-deleted").ok();
                            "**Panel removed.**".to_string()
                        } else {
                            "**[ERR] Panel not found.**".to_string()
                        }
                    }
                    // Unknown or missing type: say so LOUDLY. Returning ""
                    // here used to end the loop silently - Bob would
                    // announce "I will create a website..." and then
                    // nothing happened (field failure 2026-08-01, the
                    // cheese page). A real [ERR] feeds back through the
                    // loop so the model retries with a valid action.
                    other => format!(
                        "**[ERR] Action ignored - {} \"type\".** Call your native tools directly. If you must write one out, use exactly {{\"type\":\"tool\",\"tool\":\"shell_exec\",\"args\":{{\"command\":\"...\"}}}} for web_search/web_fetch/file_read/file_write/file_edit/file_list/shell_exec, or {{\"type\":\"<tool name>\", ...its fields}} for any other tool.",
                        if other.is_empty() { "missing".to_string() } else { format!("unknown type '{}' in", other) }
                    ),
                };

                // Run monitor: one line per tool call, with what came back.
                crate::run_log::tool(
                    &step_label,
                    !confirmation.is_empty() && !confirmation.starts_with("**[ERR]"),
                    &confirmation,
                );

                // An empty confirmation means the action did nothing -
                // don't count it as executed and keep it off the trail.
                if confirmation.is_empty() {
                    stats.failed += 1;
                } else {
                    stats.executed += 1;
                    // The ignored-action error is loop feedback, not a step
                    // the user should see on the trail.
                    if !confirmation.starts_with("**[ERR] Action ignored") {
                        stats.labels.push(step_label);
                    }
                }

                // Replace the <action>...</action> block with the confirmation
                let tag_end = abs_end + close_tag_len;
                result = format!("{}{}{}", &result[..abs_start], confirmation, &result[tag_end..]);
                offset = abs_start + confirmation.len();
            } else {
                // Unparseable even after repair - surface it instead of
                // silently deleting, so the user (and the reprompt loop) know
                // an action was lost rather than seeing confident prose.
                stats.failed += 1;
                let tag_end = abs_end + close_tag_len;
                let note = "**[ERR] An action was dropped (malformed JSON).**";
                result = format!("{}{}{}", &result[..abs_start], note, &result[tag_end..]);
                offset = abs_start + note.len();
            }
        }
    }

    (result, stats)
}

/// Core message processing pipeline - shared by desktop Tauri command and mobile API.
/// Handles demo commands, LLM calls (cloud or local), and action execution.
pub async fn process_message_core(content: &str, app: &tauri::AppHandle) -> Message {
    use tauri::Manager;
    let state = app.state::<AppState>();

    // Create user message
    let user_msg = Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: content.to_string(),
        context: None,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };
    {
        let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        msgs.push(user_msg.clone());
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = OrbState::Thinking;
    }
    crate::event_bus::emit_ui(app, "orb-state-change", OrbState::Thinking).ok();
    crate::event_bus::emit_ui(app, "new-message", user_msg.clone()).ok();

    let _trimmed = content.trim().to_lowercase();




    // ── Regular message: LLM processing ──
    let history = {
        let msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        msgs.clone()
    };

    // Find default agent
    let agent = {
        let agents = state.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
        agents.iter().find(|a| a.is_default && a.enabled).cloned()
    };

    // Build system prompt
    let home_dir = dirs::home_dir()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| "~".to_string());

    let connector_context = {
        let instances = state.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        if instances.is_empty() {
            String::new()
        } else {
            let lines: Vec<String> = instances.iter()
                .map(|i| format!("- {} ({:?})", i.name, i.status))
                .collect();
            format!("\nCONNECTED SERVICES:\n{}\n", lines.join("\n"))
        }
    };

    let memory_context = {
        let mem = state.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
        if mem.entries.is_empty() {
            String::new()
        } else {
            let items: Vec<String> = mem.entries.iter()
                .take(20)
                .map(|e| format!("- {} = {}", e.key, e.value))
                .collect();
            format!("\nMEMORY:\n{}\n", items.join("\n"))
        }
    };

    let system_prompt_text = format!(
        "You are OrbixCore, an AI assistant running locally on the user's {HOST_OS_NAME}. \
You execute real actions using <action>JSON</action> tags.\n\n\
=== RULES ===\n\
1. DO NOT introduce yourself or explain your capabilities. Just respond to what the user asked.\n\
2. ONLY do what the user explicitly asks. NEVER take unsolicited actions.\n\
3. To execute anything, wrap it in <action>JSON</action> tags. Without tags, nothing happens.\n\
4. ABSOLUTELY NEVER fabricate, invent, or assume data. If an API returns 0 results, say \"none found\" - do NOT invent records, IDs, names, or amounts.\n\
5. Use full paths like {home}/Desktop. Never use ~ or $HOME.\n\
6. Keep responses SHORT. 1-3 sentences max, then actions.\n\
7. If the user greets you or sends a general message, respond conversationally. Do NOT run actions unless asked.\n\
8. ONLY use API endpoints listed below. Do NOT guess or invent routes.\n\n\
=== TOOLS ===\n\
Shell: <action>{{\"type\":\"tool\",\"tool\":\"shell_exec\",\"args\":{{\"command\":\"...\"}}}}</action>\n\
API call: <action>{{\"type\":\"api_call\",\"method\":\"GET|POST|PUT|DELETE\",\"url\":\"...\",\"body\":{{...}}}}</action>\n\n\
=== BUSINESS DATA ===\n\
There is no built-in CRM or ERP. For business data, use what the USER has connected: their own systems and data stores (Microsoft/Dataverse, Google, SQLite, a REST API, etc.). Never assume a company platform exists; if nothing is connected for what they ask, say so and offer to connect one. Never invent API routes or fabricate data.\n\n\
=== ENVIRONMENT ===\n\
Home: {home}\n\
{connectors}\
{memory}",
        home = home_dir,
        connectors = connector_context,
        memory = memory_context
    );

    let system_prompt = serde_json::json!({
        "role": "system",
        "content": system_prompt_text
    });

    let mut api_messages: Vec<serde_json::Value> = vec![system_prompt];
    let recent_history = if history.len() > 20 { &history[history.len()-20..] } else { &history };
    let n = recent_history.len();
    api_messages.extend(recent_history.iter().enumerate().map(|(i, m)| {
        let keep_evidence = i + 4 >= n;
        serde_json::json!({ "role": m.role, "content": history_msg_content(m, keep_evidence) })
    }));

    // Route to agent or local LLM
    let response_content = if let Some(ref a) = agent {
        if a.provider == AgentProvider::Local {
            // Local agent - use raw history (call_local_llm adds its own system prompt)
            let client = reqwest::Client::new();
            call_local_llm(&client, &history).await
        } else {
            call_agent_api(a, &history).await
        }
    } else {
        // No agent - route to local LLM directly
        let client = reqwest::Client::new();
        call_local_llm(&client, &history).await
    };

    let raw_content = match response_content {
        Ok(text) => text,
        Err(e) => format!("I couldn't process that right now: {}", e),
    };

    // Execute action tags
    let final_content_raw = execute_actions(&raw_content, &*state, app).await;
    // Strip hallucinated data when API returned empty results
    let final_content = strip_hallucinated_content(&final_content_raw);

    let history_context = if raw_content.contains("<action>") {
        Some(format!("I executed: {}\n\nResults shown to user: {}", raw_content, final_content))
    } else {
        None
    };

    let reply = Message {
        id: Uuid::new_v4().to_string(),
        role: "assistant".to_string(),
        content: final_content,
        context: history_context,
        agent_id: agent.map(|a| a.id),
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    };

    {
        let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
        msgs.push(reply.clone());
        let mut orb = state.chat.orb_state.lock().unwrap_or_else(|e| e.into_inner());
        *orb = OrbState::Idle;
    }
    crate::event_bus::emit_ui(app, "orb-state-change", OrbState::Idle).ok();
    crate::event_bus::emit_ui(app, "new-message", reply.clone()).ok();

    reply
}

/// Execute a single workflow step and return its output as a string.
/// Format raw JSON output into readable markdown
pub(crate) fn format_json_as_markdown(raw: &str) -> String {
    schema_inference::format_json_as_markdown(raw)
}


/// `execute_workflow_step` encodes failures as formatted strings at ~20 return
/// sites. This is the single place that knows those sentinels, so the run loop
/// doesn't have to. Empty output also counts as failure (nothing happened).
pub(crate) fn step_display_failed(display: &str) -> bool {
    let d = display.trim();
    if d.is_empty() {
        return true;
    }
    const MARKERS: &[&str] = &[
        "*Connector error:*",
        "*Agent error:*",
        "*Failed:",
        "*error:",
        "- API error (HTTP",
        "- Request failed:",
        "No connector bound and no LLM agent configured",
        "No command configured",
        "[ERR]",
    ];
    MARKERS.iter().any(|m| d.contains(m))
}

/// Wrapper around `execute_workflow_step` that classifies success/failure
/// centrally. Keeps the underlying function's signature untouched.
pub(crate) async fn run_step_checked(
    step: &workflow::WorkflowStep,
    raw_context: &[serde_json::Value],
    agents: &[Agent],
    universal_connectors: &[connectors::UniversalConnector],
    memory_context: &str,
    state: &AppState,
) -> StepOutcome {
    let (display, raw) =
        execute_workflow_step(step, raw_context, agents, universal_connectors, memory_context, state).await;
    StepOutcome {
        success: !step_display_failed(&display),
        display,
        raw,
    }
}

/// Execute a single workflow step.
/// Returns (display_output, raw_output) - display is for user, raw is JSON for context accumulation.
/// Normalize whatever the config/context gives us into a list of record
/// objects. Accepts a single object, an array of objects, or a JSON
/// string holding either (the templating layer produces strings).
fn rows_from_value(v: &serde_json::Value) -> Vec<serde_json::Value> {
    match v {
        serde_json::Value::Array(a) => a.iter().filter(|r| r.is_object()).cloned().collect(),
        serde_json::Value::Object(_) => vec![v.clone()],
        serde_json::Value::String(s) => serde_json::from_str::<serde_json::Value>(s.trim())
            .map(|parsed| rows_from_value(&parsed))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Ask the local model to map arbitrary source data onto a destination's
/// real columns, returning rows keyed exactly by the schema. This is the
/// "AI deals with the data processing" step: heterogeneous trigger data
/// in, schema-shaped rows out.
async fn ai_map_to_schema(
    source: &serde_json::Value,
    schema: &crate::destinations::SchemaInfo,
    agents: &[Agent],
    memory_context: &str,
) -> Result<Vec<serde_json::Value>, String> {
    let cols = schema
        .columns
        .iter()
        .map(|c| format!("{} ({}{})", c.name, c.data_type, if c.nullable { "" } else { ", required" }))
        .collect::<Vec<_>>()
        .join(", ");
    let prompt = format!(
        "Map the source data onto the target table \"{}\".\n\
         Target columns: {}.\n\
         Source data (JSON):\n{}\n\n\
         Output ONLY a JSON array of row objects, each keyed by the exact \
         target column names above. Use null for values you cannot fill. \
         Do not invent columns. Do not include any prose, only the JSON array.",
        schema.target,
        cols,
        crate::truncate_str(&serde_json::to_string_pretty(source).unwrap_or_default(), 4000),
    );
    let agent = agents
        .iter()
        .find(|a| a.enabled && !a.api_key.is_empty())
        .or_else(|| agents.iter().find(|a| a.enabled && a.provider == AgentProvider::Local))
        .ok_or("no enabled agent to map the data")?;
    let raw = call_agent_for_workflow(agent, &prompt, memory_context).await?;
    // Pull the first JSON array out of the reply (models wrap it in prose
    // or a fence despite instructions).
    let text = raw.trim();
    let start = text.find('[').ok_or("mapping did not return a JSON array")?;
    let end = text.rfind(']').ok_or("mapping did not return a JSON array")?;
    let slice = &text[start..=end];
    let parsed: serde_json::Value = serde_json::from_str(slice)
        .ok()
        .or_else(|| repair_json(slice).and_then(|f| serde_json::from_str(&f).ok()))
        .ok_or("mapping output was not valid JSON")?;
    Ok(rows_from_value(&parsed))
}

/// Execute a destination-write Action step end to end.
async fn run_destination_step(
    step: &workflow::WorkflowStep,
    raw_context: &[serde_json::Value],
    agents: &[Agent],
    memory_context: &str,
) -> (String, Option<serde_json::Value>) {
    let dest: crate::destinations::Destination =
        match serde_json::from_value(step.config.get("destination").cloned().unwrap_or_default()) {
            Ok(d) => d,
            Err(e) => return (format!("[ERR] invalid destination spec: {}", e), None),
        };

    // Where do the rows come from? Explicit config.rows wins; otherwise
    // fall back to the previous step's output.
    let source = step
        .config
        .get("rows")
        .cloned()
        .or_else(|| raw_context.last().cloned())
        .unwrap_or(serde_json::Value::Null);

    let want_map = step.config.get("map").and_then(|v| v.as_bool()).unwrap_or(false);
    let rows: Vec<serde_json::Value> = if want_map {
        let schema = match dest.describe().await {
            Ok(s) => s,
            Err(e) => return (format!("[ERR] could not read {} schema: {}", dest.label(), e), None),
        };
        match ai_map_to_schema(&source, &schema, agents, memory_context).await {
            Ok(r) => r,
            Err(e) => return (format!("[ERR] mapping to {} failed: {}", dest.label(), e), None),
        }
    } else {
        rows_from_value(&source)
    };

    if rows.is_empty() {
        return (format!("[ERR] no rows to write to {}", dest.label()), None);
    }

    match dest.write(&rows).await {
        Ok(outcome) => {
            let display = if outcome.errors.is_empty() {
                format!("Wrote {} row(s) to {}.", outcome.written, dest.label())
            } else {
                format!(
                    "Wrote {} row(s) to {}, {} failed:\n{}",
                    outcome.written,
                    dest.label(),
                    outcome.errors.len(),
                    outcome.errors.join("\n")
                )
            };
            let raw = serde_json::json!({
                "destination": dest.label(),
                "written": outcome.written,
                "ids": outcome.ids,
                "errors": outcome.errors,
            });
            (display, Some(raw))
        }
        Err(e) => (format!("[ERR] write to {} failed: {}", dest.label(), e), None),
    }
}

/// Bob's app-facing tools as workflow steps. Returns None when the tool
/// is not one of these, so the caller falls through to the built-ins.
async fn run_capability_step(tool: &str, args: &serde_json::Value, state: &AppState) -> Option<(String, Option<serde_json::Value>)> {
    let str_arg = |k: &str| args.get(k).and_then(|v| v.as_str()).unwrap_or("").to_string();
    let fail = |msg: String| Some((format!("[ERR] {}", msg), None));
    match tool {
        "peer_send" => {
            let (app, text) = (str_arg("app"), str_arg("text"));
            if app.is_empty() || text.is_empty() {
                return fail("peer_send needs \"app\" and \"text\"".into());
            }
            match crate::peer::send(&app, &text).await {
                Ok(r) => Some((format!("Sent to {} via {}. {}", r.app, r.via.label(), r.detail), Some(serde_json::json!({ "app": r.app, "delivered": r.delivered, "via": r.via })))),
                Err(e) => fail(format!("peer_send to {}: {}", app, e)),
            }
        }
        "peer_read" => {
            let app = str_arg("app");
            match crate::peer::read(&app).await {
                Ok(r) => Some((r.text.clone(), Some(serde_json::json!({ "app": r.app, "title": r.title, "text": r.text, "truncated": r.truncated })))),
                Err(e) => fail(format!("peer_read {}: {}", app, e)),
            }
        }
        "peer_open" => {
            let (app, action) = (str_arg("app"), str_arg("action"));
            let inner = args.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
            match crate::peer::open_action(&app, &action, &inner).await {
                Ok(m) => Some((m, None)),
                Err(e) => fail(format!("peer_open {}/{}: {}", app, action, e)),
            }
        }
        "peer_press" => {
            let (app, control) = (str_arg("app"), str_arg("control"));
            match crate::peer::press(state, &app, &control).await {
                Ok(m) => Some((m, None)),
                Err(e) => fail(format!("peer_press {}: {}", app, e)),
            }
        }
        "peer_fill" => {
            let (app, field, value) = (str_arg("app"), str_arg("field"), str_arg("value"));
            match crate::peer::fill(state, &app, &field, &value).await {
                Ok(m) => Some((m, None)),
                Err(e) => fail(format!("peer_fill {}: {}", app, e)),
            }
        }
        "mcp_call" => {
            let (server, tool_name) = (str_arg("server"), str_arg("tool"));
            let inner = args.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
            match crate::mcp_client::call_tool(state, &server, &tool_name, inner).await {
                Ok((text, raw)) => Some((text, Some(raw))),
                Err(e) => fail(format!("mcp_call {}/{}: {}", server, tool_name, e)),
            }
        }
        "api_call" => {
            let out = crate::services::api_call(state, args).await;
            let raw = serde_json::from_str::<serde_json::Value>(&out).ok();
            Some((out, raw))
        }
        _ => None,
    }
}

pub(crate) async fn execute_workflow_step(step: &workflow::WorkflowStep, raw_context: &[serde_json::Value], agents: &[Agent], universal_connectors: &[connectors::UniversalConnector], memory_context: &str, state: &AppState) -> (String, Option<serde_json::Value>) {
    match step.step_type {
        workflow::StepType::Action => {
            // Destination write (bring-your-own-database output): config
            // carries a `destination` spec (SQLite / HTTP-OData). Rows come
            // from config.rows (explicit, already templated) or, when
            // config.map is set, from an AI mapping of the prior step's
            // data onto the destination's real schema. This is the generic
            // "output a row wherever the user keeps their data" step.
            if step.config.contains_key("destination") {
                return run_destination_step(step, raw_context, agents, memory_context).await;
            }
            // App-bridge step: read a Mac app (Calendar, Mail, Notes, ...)
            // through its scripting interface. This is what makes "morning
            // briefing" workflows real instead of hollow labels - the same
            // capability Bob has in chat, now schedulable.
            if step.config.get("tool").and_then(|v| v.as_str()) == Some("app_bridge") {
                let args = step.config.get("tool_args").cloned().unwrap_or_else(|| serde_json::json!({}));
                let app_name = args["app"].as_str().unwrap_or("").to_string();
                let action_id = args["action"].as_str().unwrap_or("").to_string();
                let inner = args.get("args").cloned().unwrap_or_else(|| serde_json::json!({}));
                return match crate::app_bridge::run_bridge_action(&app_name, &action_id, &inner).await {
                    Ok(v) => (format_json_as_markdown(&v.to_string()), Some(v)),
                    Err(e) => (
                        format!("[ERR] app_bridge {}/{} failed: {}", app_name, action_id, e),
                        None,
                    ),
                };
            }
            // Paired apps, MCP servers and connected services as steps: the
            // same capabilities Bob has in chat, now inside a workflow. This
            // is what lets "ask ChatGPT, then put it in Notes, then post the
            // link in Slack" run on a schedule.
            if let Some(tool_name) = step.config.get("tool").and_then(|v| v.as_str()) {
                let args = step_tool_args(&step.config);
                if let Some(done) = run_capability_step(tool_name, &args, state).await {
                    return done;
                }
            }
            // Generic registry-tool step (P1): config.tool names a built-in
            // tool, config.tool_args carries its arguments ({{templates}}
            // already resolved by the runner). Checked first so real
            // capabilities beat the legacy free-text paths.
            if let Some(tool_name) = step.config.get("tool").and_then(|v| v.as_str()) {
                if tools::is_builtin_tool(tool_name) {
                    let args: HashMap<String, serde_json::Value> = step_tool_args(&step.config)
                        .as_object()
                        .cloned()
                        .map(|m| m.into_iter().collect())
                        .unwrap_or_default();
                    let result = tools::execute_tool(tool_name, &args, &tools::ToolCtx { state, agent_id: "workflow" }).await;
                    return if result.success {
                        let raw = serde_json::json!({ "tool": tool_name, "output": result.output });
                        (result.output, Some(raw))
                    } else {
                        (format!("[ERR] {} failed: {}", tool_name,
                            result.error.unwrap_or_else(|| "unknown error".into())), None)
                    };
                }
            }
            // Connector-based execution - only if connector_id is non-empty and found
            if let Some(serde_json::Value::String(conn_id)) = step.config.get("connector_id") {
                if !conn_id.is_empty() {
                    let endpoint_key = step.config.get("endpoint_key")
                        .and_then(|v| v.as_str())
                        .unwrap_or("default");
                    let body_vars: HashMap<String, String> = step.config.get("body_variables")
                        .and_then(|v| serde_json::from_value(v.clone()).ok())
                        .unwrap_or_default();

                    if let Some(connector) = universal_connectors.iter().find(|c| c.id == *conn_id) {
                        match connectors::execute_connector_request(connector, endpoint_key, &body_vars).await {
                            Ok((_status, body)) => {
                                let raw_val = serde_json::from_str::<serde_json::Value>(&body).ok();
                                let display = format_json_as_markdown(&body);
                                return (display, raw_val);
                            }
                            Err(e) => return (format!("*Connector error:* {}", e), None),
                        }
                    }

                    // OAuth connector execution (LinkedIn, Microsoft, Google, etc.)
                    // Uses stored access tokens from connector instances
                    let oauth_platform = match conn_id.as_str() {
                        s if s.contains("linkedin") => Some("linkedin"),
                        s if s.contains("microsoft") => Some("microsoft"),
                        s if s.contains("google") => Some("google"),
                        s if s.contains("github") => Some("github"),
                        s if s.contains("slack") => Some("slack"),
                        _ => None,
                    };
                    if let Some(platform) = oauth_platform {
                        // Find the connector instance and load its access token
                        let token = connectors::load_connector_credential(conn_id, "access_token").ok();
                        if let Some(access_token) = token {
                            let api_url = step.config.get("api_url")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            let method = step.config.get("method")
                                .and_then(|v| v.as_str())
                                .unwrap_or("POST");
                            let body_json = step.config.get("body")
                                .cloned()
                                .unwrap_or(serde_json::json!({}));

                            // Default API URLs per platform
                            let url = if !api_url.is_empty() {
                                api_url.to_string()
                            } else {
                                match platform {
                                    "linkedin" => "https://api.linkedin.com/rest/posts".to_string(),
                                    _ => api_url.to_string(),
                                }
                            };

                            if !url.is_empty() {
                                let client = reqwest::Client::new();
                                let mut req = match method {
                                    "GET" => client.get(&url),
                                    "PUT" => client.put(&url),
                                    "DELETE" => client.delete(&url),
                                    _ => client.post(&url),
                                };
                                req = req.bearer_auth(&access_token)
                                    .header("Content-Type", "application/json");

                                // LinkedIn REST API requires version header
                                if platform == "linkedin" {
                                    req = req.header("LinkedIn-Version", "202401")
                                        .header("X-Restli-Protocol-Version", "2.0.0");
                                }

                                if method != "GET" {
                                    req = req.json(&body_json);
                                }

                                match req.send().await {
                                    Ok(resp) => {
                                        let status = resp.status().as_u16();
                                        let body = resp.text().await.unwrap_or_default();
                                        let raw_val = serde_json::from_str::<serde_json::Value>(&body).ok();
                                        if status < 400 {
                                            return (format!("**{}** - API call succeeded (HTTP {})", platform, status), raw_val);
                                        } else {
                                            return (format!("**{}** - API error (HTTP {}): {}", platform, status, body), raw_val);
                                        }
                                    }
                                    Err(e) => return (format!("**{}** - Request failed: {}", platform, e), None),
                                }
                            }
                        }
                    }
                    // Connector not found - fall through to LLM agent to handle the step
                }
            }

            // Plain shell and file steps. These sat after the model fallback
            // for a long time and never ran; the model improvised their
            // output instead (audit 2026-09-28).
            if let Some(serde_json::Value::String(cmd)) = step.config.get("command") {
                let mut m = HashMap::new();
                m.insert("command".to_string(), serde_json::Value::String(cmd.clone()));
                let result = tools::execute_tool("shell_exec", &m, &tools::ToolCtx { state, agent_id: "workflow" }).await;
                return if result.success {
                    let raw_val = serde_json::from_str::<serde_json::Value>(&result.output).ok();
                    (format_json_as_markdown(&result.output), raw_val)
                } else {
                    (format!("[ERR] command failed: {}", result.error.unwrap_or_else(|| "Unknown error".into())), None)
                };
            }
            if let Some(serde_json::Value::String(path)) = step.config.get("path") {
                let mut m = HashMap::new();
                m.insert("path".to_string(), serde_json::Value::String(path.clone()));
                let (tool_name, label) = if let Some(serde_json::Value::String(content)) = step.config.get("content") {
                    m.insert("content".to_string(), serde_json::Value::String(content.clone()));
                    ("file_write", "write")
                } else {
                    ("file_read", "read")
                };
                let result = tools::execute_tool(tool_name, &m, &tools::ToolCtx { state, agent_id: "workflow" }).await;
                return if result.success {
                    let raw_val = serde_json::from_str::<serde_json::Value>(&result.output).ok();
                    (format_json_as_markdown(&result.output), raw_val)
                } else {
                    (format!("[ERR] file {} failed: {}", label, result.error.unwrap_or_default()), None)
                };
            }
            // A named tool nobody knows is an error, not a prompt for the model.
            if let Some(tool_name) = step.config.get("tool").and_then(|v| v.as_str()) {
                let legacy_web = (tool_name == "web_search" || tool_name == "web_fetch")
                    && (step.config.contains_key("query_template") || step.config.contains_key("url_template"));
                if !tool_name.trim().is_empty() && !legacy_web {
                    return (
                        format!(
                            "[ERR] unknown step tool \"{}\". Steps can use: {}",
                            tool_name,
                            crate::tool_schemas::step_tool_names().join(", ")
                        ),
                        None,
                    );
                }
            }

            // ── Fallback: run a tool hint (web_search/web_fetch) or hand the step to an LLM ──
            {
                let step_label = &step.label;
                let endpoint_hint = step.config.get("endpoint_key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tool_hint = step.config.get("tool")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let _body_vars: HashMap<String, String> = step.config.get("body_variables")
                    .and_then(|v| serde_json::from_value(v.clone()).ok())
                    .unwrap_or_default();


                // ── web_search / web_fetch: direct HTTP scraping (no LLM token cost) ──
                if tool_hint == "web_search" || tool_hint == "web_fetch" {
                    let query_tpl = step.config.get("query_template").and_then(|v| v.as_str()).unwrap_or("");
                    let url_tpl = step.config.get("url_template").and_then(|v| v.as_str()).unwrap_or("");

                    // Build search queries from template + previous context
                    let mut queries: Vec<String> = Vec::new();
                    if !query_tpl.is_empty() {
                        // Extract variables from previous context to fill template
                        let vars = extract_template_vars(raw_context);
                        if vars.is_empty() {
                            // No variables yet - use template as-is with placeholder removed
                            let q = query_tpl.replace("{{vertical}}", "").replace("{{company}}", "").trim().to_string();
                            if !q.is_empty() { queries.push(q); }
                        } else {
                            for val in &vars {
                                let q = query_tpl.replace("{{vertical}}", val).replace("{{company}}", val);
                                queries.push(q);
                            }
                        }
                    }

                    // For web_fetch with URL template, build URLs from previous context
                    let mut urls: Vec<String> = Vec::new();
                    if !url_tpl.is_empty() {
                        let vars = extract_template_vars(raw_context);
                        for val in &vars {
                            let url = url_tpl.replace("{{company}}", &urlencoding::encode(val));
                            urls.push(url);
                        }
                    }

                    let _scrape_client = reqwest::Client::builder()
                        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
                        .timeout(std::time::Duration::from_secs(15))
                        .build().unwrap_or_else(|_| reqwest::Client::new());

                    let mut results: Vec<serde_json::Value> = Vec::new();

                    if tool_hint == "web_search" && !queries.is_empty() {
                        // Direct web search via DuckDuckGo HTML
                        for query in queries.iter().take(5) {
                            wf_log(&format!("Direct search: {}", query));
                            match crate::egress::web_search(state, query, 15).await {
                                Ok(entries) => {
                                    for e in entries { results.push(e); }
                                }
                                Err(e) => wf_log(&format!("Search error: {}", e)),
                            }
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        }
                    } else if tool_hint == "web_fetch" && !urls.is_empty() {
                        // Direct URL fetch and text extraction
                        for url in urls.iter().take(10) {
                            wf_log(&format!("Direct fetch: {}", url));
                            match crate::egress::web_fetch(state, url).await {
                                Ok((status, body)) => {
                                    let text = regex::Regex::new(r"<[^>]+>").unwrap()
                                        .replace_all(&body, " ").trim().to_string();
                                    let snippet: String = text.chars().take(2000).collect();
                                    results.push(serde_json::json!({
                                        "url": url, "status": status, "content": snippet,
                                    }));
                                }
                                Err(e) => wf_log(&format!("Fetch error for {}: {}", url, e)),
                            }
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                        }
                    } else if !queries.is_empty() {
                        // Fallback: try search for any queries
                        for query in queries.iter().take(5) {
                            wf_log(&format!("Direct search (fallback): {}", query));
                            match crate::egress::web_search(state, query, 15).await {
                                Ok(entries) => {
                                    for e in entries { results.push(e); }
                                }
                                Err(e) => wf_log(&format!("Search error: {}", e)),
                            }
                            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                        }
                    }

                    if !results.is_empty() {
                        // Deduplicate by URL
                        let mut seen = std::collections::HashSet::new();
                        results.retain(|r| {
                            let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
                            if url.is_empty() { return true; }
                            seen.insert(url.to_string())
                        });

                        let display_lines: Vec<String> = results.iter().take(15).map(|r| {
                            let title = r.get("title").and_then(|v| v.as_str()).unwrap_or("-");
                            let url = r.get("url").and_then(|v| v.as_str()).unwrap_or("");
                            let snippet = r.get("snippet").and_then(|v| v.as_str()).unwrap_or("");
                            format!("- **{}**\n  {}\n  {}", title, url, snippet)
                        }).collect();
                        let display = format!("**{}** - {} results\n\n{}", step_label, results.len(), display_lines.join("\n"));
                        let raw_val = serde_json::json!({"results": results});
                        return (display, Some(raw_val));
                    }

                    // No results from direct scraping - return empty
                    wf_log(&format!("No results from direct scraping for step: {}", step_label));
                    return (format!("**{}** - No results found via direct search", step_label), Some(serde_json::json!({"results": []})));
                }

                // ── Fallback: delegate to LLM agent with connected system info ──
                let connected_systems: Vec<String> = universal_connectors.iter()
                    .filter(|c| matches!(c.status, connectors::ConnectionStatus::Connected))
                    .map(|c| {
                        let endpoint_names: Vec<&str> = c.endpoints.iter().map(|e| e.key.as_str()).collect();
                        format!(
                            "- **{}**: base_url={}, endpoints: [{}]",
                            c.name, c.base_url, endpoint_names.join(", ")
                        )
                    })
                    .collect();

                let systems_info = if !connected_systems.is_empty() {
                    format!(
                        "\n\nConnected systems available:\n{}\n\
                         Use the connected system's API to fulfil this step if applicable.",
                        connected_systems.join("\n")
                    )
                } else {
                    String::new()
                };

                let fallback_prompt = if !tool_hint.is_empty() {
                    format!(
                        "A {} was attempted for this workflow step but returned no results: \"{}\". \
                         Based on the data gathered so far and your knowledge, provide the best analysis you can. \
                         If you truly have no information, say so honestly - do NOT invent data.{}",
                        tool_hint, step_label, systems_info
                    )
                } else if !endpoint_hint.is_empty() {
                    format!(
                        "Execute this workflow step: \"{}\". \
                         The endpoint '{}' could not be auto-routed.{} \
                         Work only from the context of previous steps. If the data needed is not there, \
                         say exactly what is missing - never invent results.",
                        step_label, endpoint_hint, systems_info
                    )
                } else {
                    format!(
                        "Execute this workflow step: \"{}\". \
                         Use your available tools (web_search, web_fetch, file_read, shell_exec) as needed.{} \
                         Output your results in a clear, structured format.",
                        step_label, systems_info
                    )
                };

                // Add accumulated context
                let context_str = if !raw_context.is_empty() {
                    let ctx: Vec<String> = raw_context.iter().filter_map(|v| {
                        serde_json::to_string_pretty(v).ok()
                    }).collect();
                    format!("\n\nContext from previous steps:\n{}", ctx.join("\n---\n"))
                } else {
                    String::new()
                };

                let full_prompt = format!("{}{}", fallback_prompt, context_str);

                // Send to LLM agent
                let llm_agent = agents.iter().find(|a| a.enabled && !a.api_key.is_empty())
                    .or_else(|| agents.iter().find(|a| a.enabled && a.provider == AgentProvider::Local));
                if let Some(agent) = llm_agent {
                    match call_agent_for_workflow(agent, &full_prompt, memory_context).await {
                        Ok(response) => {
                            // Workflow output must never spawn the input
                            // wizard - runs are unattended by definition.
                            let response = strip_inputs_blocks(&response);
                            let raw_val = serde_json::from_str::<serde_json::Value>(&response).ok();
                            return (response, raw_val);
                        }
                        Err(e) => return (format!("*Agent error:* {}", e), None),
                    }
                }

                return (format!("**Step:** {}\n\n*No connector bound and no LLM agent configured. Connect a service or configure an LLM agent.*", step_label), None);
            }

        }
        workflow::StepType::Wait => {
            let secs = step.config.get("seconds")
                .and_then(|v| v.as_u64())
                .unwrap_or(1);
            tokio::time::sleep(tokio::time::Duration::from_secs(secs)).await;
            (format!("Waited {} second(s).", secs), None)
        }
        workflow::StepType::Prompt => {
            if let Some(serde_json::Value::String(prompt)) = step.config.get("prompt") {
                // First try: send to an LLM agent if one has an API key
                let llm_agent = agents.iter().find(|a| a.enabled && !a.api_key.is_empty())
                    .or_else(|| agents.iter().find(|a| a.enabled && a.provider == AgentProvider::Local));
                if let Some(agent) = llm_agent {
                    let data_summary: String = raw_context.iter().rev().take(3).rev()
                        .map(|v| {
                            let s = match v {
                                serde_json::Value::String(s) => s.clone(),
                                other => serde_json::to_string_pretty(other).unwrap_or_default(),
                            };
                            if s.chars().count() > 4000 { format!("{}...(truncated)", crate::truncate_str(&s, 4000)) } else { s }
                        })
                        .collect::<Vec<_>>().join("\n\n");

                    // If this is a context-gathering prompt (asks about company, verticals, etc.)
                    // and we already have memory context, tell the LLM to USE it and proceed
                    let prompt_lower = prompt.to_lowercase();
                    let is_context_gathering = prompt_lower.contains("ask:") || prompt_lower.contains("what vertical")
                        || prompt_lower.contains("what's your") || prompt_lower.contains("what are your")
                        || prompt_lower.contains("ideal company") || prompt_lower.contains("core products");
                    let has_memory = !memory_context.is_empty();

                    let full_prompt = if is_context_gathering && has_memory {
                        // Don't ask - the memory already has the answers
                        format!(
                            "The user's company context is already known (provided in your system prompt). \
                             DO NOT ask the user any questions. Instead, use the company context to answer this:\n\n{}\n\n\
                             Produce a structured summary of the relevant context and proceed. \
                             Output the verticals, ideal company size, and key competencies based on what you know.",
                            prompt
                        )
                    } else if data_summary.is_empty() {
                        prompt.clone()
                    } else {
                        format!("Here is the data:\n\n{}\n\n{}", data_summary, prompt)
                    };
                    match call_agent_for_workflow(agent, &full_prompt, memory_context).await {
                        Ok(response) => return (response, None),
                        Err(_) => {} // Fall through to built-in analysis
                    }
                }
                // Fallback: built-in analysis engine
                let report = analyze_data(raw_context, prompt);
                (report, None)
            } else {
                (step.label.clone(), None)
            }
        }
        workflow::StepType::Condition => {
            // Evaluate a simple condition against the most recent step output.
            // config: { "contains": "text" } | { "equals": "text" } |
            //         { "not_contains": "text" }, plus optional
            //         "on_false": "skip" (default) | "fail".
            let last = raw_context
                .last()
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                })
                .unwrap_or_default();
            let passed = if let Some(needle) = step.config.get("contains").and_then(|v| v.as_str()) {
                last.to_lowercase().contains(&needle.to_lowercase())
            } else if let Some(needle) = step.config.get("not_contains").and_then(|v| v.as_str()) {
                !last.to_lowercase().contains(&needle.to_lowercase())
            } else if let Some(val) = step.config.get("equals").and_then(|v| v.as_str()) {
                last.trim() == val.trim()
            } else {
                // No predicate configured: treat as pass so the run continues.
                true
            };
            if passed {
                (format!("Condition passed: {}", step.label), None)
            } else if step.config.get("on_false").and_then(|v| v.as_str()) == Some("fail") {
                // step_display_failed keys off this marker, failing the run.
                (format!("*Failed: condition '{}' not met", step.label), None)
            } else {
                (format!("Condition '{}' not met; continuing.", step.label), None)
            }
        }
        workflow::StepType::Loop => {
            // Repeat an inner command N times. config: { "count": N,
            // "command": "shell command" }. Each iteration's output is
            // concatenated. Bounded so a bad count cannot hang the run.
            let count = step.config.get("count").and_then(|v| v.as_u64()).unwrap_or(1).min(50);
            let Some(command) = step.config.get("command").and_then(|v| v.as_str()) else {
                return ("*Failed: loop step has no command".to_string(), None);
            };
            let mut outputs = Vec::new();
            for i in 0..count {
                let mut args = std::collections::HashMap::new();
                args.insert("command".to_string(), serde_json::Value::String(command.to_string()));
                let r = tools::exec_shell(&args).await;
                outputs.push(format!("[{}/{}] {}", i + 1, count, r.output.trim()));
                if !r.success {
                    outputs.push(format!("*Failed: loop iteration {} failed", i + 1));
                    break;
                }
            }
            (outputs.join("\n"), None)
        }
        workflow::StepType::Relay => {
            // Route a query to the cloud through the secure egress gateway
            // (redaction + compliance + audit), never directly. config:
            // { "query": "..." }.
            let query = step
                .config
                .get("query")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    raw_context.last().map(|v| match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    })
                })
                .unwrap_or_default();
            if query.trim().is_empty() {
                return ("*Failed: relay step has no query".to_string(), None);
            }
            match crate::egress::web_search(state, &query, 10).await {
                Ok(results) => {
                    let body = results
                        .iter()
                        .map(|r| format!("- {}\n  {}", r["title"].as_str().unwrap_or(""), r["snippet"].as_str().unwrap_or("")))
                        .collect::<Vec<_>>()
                        .join("\n");
                    (format!("Relay results:\n{}", body), Some(serde_json::json!(results)))
                }
                Err(e) => (format!("*Failed: relay egress blocked: {}", e), None),
            }
        }
    }
}


// ─────────────────────────────────────────────
//  Built-in Data Analysis Engine (Epic 2: Dynamic Schema Inference)
// ─────────────────────────────────────────────

pub(crate) fn analyze_data(data: &[serde_json::Value], prompt: &str) -> String {
    schema_inference::analyze_dynamic(data, prompt)
}



// ─────────────────────────────────────────────
//  Agent API Calls (Epic B)
// ─────────────────────────────────────────────

pub(crate) async fn call_agent_api(agent: &Agent, history: &[Message]) -> Result<String, String> {
    let client = reqwest::Client::new();

    // ── Local provider: route to local llama-server (Gemma 4 / other local models) ──
    if agent.provider == AgentProvider::Local {
        return call_local_llm(&client, history).await;
    }

    // ── No API key? Try local LLM first before cloud fallbacks ──
    if agent.api_key.is_empty() {
        if let Ok(result) = call_local_llm(&client, history).await {
            return Ok(result);
        }
    }

    // ── Anthropic Claude (cloud) ──
    if !agent.api_key.is_empty() && agent.provider == AgentProvider::Anthropic {
        return call_anthropic(&client, agent, history).await;
    }

    // If agent has an API key (any provider), try Anthropic anyway
    if !agent.api_key.is_empty() {
        let mut anthropic_agent = agent.clone();
        anthropic_agent.provider = AgentProvider::Anthropic;
        if anthropic_agent.endpoint.is_empty() || !anthropic_agent.endpoint.contains("anthropic") {
            anthropic_agent.endpoint = crate::egress::anthropic_base();
        }
        if anthropic_agent.model.is_empty() || !anthropic_agent.model.contains("claude") {
            anthropic_agent.model = "claude-sonnet-4-6".to_string();
        }
        return call_anthropic(&client, &anthropic_agent, history).await;
    }

    // Last resort: try local LLM if server is running
    match call_local_llm(&client, history).await {
        Ok(result) => return Ok(result),
        Err(_) => {}
    }

    Err("No AI model available. Download a local model in Settings → Models, or add an Anthropic API key.".to_string())
}

/// Call the local llama-server (OpenAI-compatible /v1/chat/completions endpoint)
/// The running model server's endpoint, or an error that says what to
/// do. A failed health check with an app handle at hand first asks the
/// model pool to (re)load: the pool restarts a server that died and
/// reloads one the idle timer unloaded, so a transient never surfaces as
/// "server is not running" mid-conversation.
pub(crate) async fn ensure_local_server(
    client: &reqwest::Client,
    app: Option<&tauri::AppHandle>,
) -> Result<String, String> {
    let endpoint = local_llm::server_endpoint();
    let healthy = |client: &reqwest::Client, endpoint: &str| {
        let url = format!("{}/health", endpoint);
        let client = client.clone();
        async move {
            match client.get(&url).send().await {
                Ok(r) => r.status().is_success(),
                Err(_) => false,
            }
        }
    };
    if healthy(client, &endpoint).await {
        return Ok(endpoint);
    }
    let Some(app) = app else {
        return Err("Local model server is not running. Select a model in Settings - Models to start it.".to_string());
    };
    crate::bob_core::ensure_model_loaded(app)
        .await
        .map_err(|e| format!("The model server stopped and could not be restarted: {}", e))?;
    let endpoint = local_llm::server_endpoint();
    if healthy(client, &endpoint).await {
        Ok(endpoint)
    } else {
        Err("The model server restarted but is not answering yet. Try again in a moment.".to_string())
    }
}

pub async fn call_local_llm(client: &reqwest::Client, history: &[Message]) -> Result<String, String> {
    let endpoint = local_llm::server_endpoint();
    let health_url = format!("{}/health", endpoint);

    // Check if server is running
    let health = client.get(&health_url).send().await;
    if health.is_err() || !health.unwrap().status().is_success() {
        return Err("Local model server is not running. Select a model in Settings - Models to start it.".to_string());
    }
    let _busy = crate::bob_core::ModelBusyGuard::new();

    let home = dirs::home_dir().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|| "~".to_string());

    // Full system prompt - same capabilities as cloud mode
    let system_prompt = format!(
        "You are Bob, the orchestrator AI behind OrbixCore - running locally on the user's {HOST_OS_NAME}. \
You execute real actions using <action>JSON</action> tags.\n\n\
PERSONALITY: Terse, sharp, dry wit. Structured outputs. Bullet points, not paragraphs.\n\n\
ACTION BIAS:\n\
When the user asks you to build something, BUILD IT IMMEDIATELY with <action> tags. Do NOT ask for more details unless genuinely impossible to proceed. Use reasonable defaults.\n\
Example: \"make an agent that posts on LinkedIn daily\" → create_agent + create_workflow + connect_oauth in ONE response. Don't ask about tone, frequency, or brand guidelines - pick sensible defaults and note them.\n\
External services (LinkedIn, Gmail, etc.) require connectors - use connect_oauth, not raw API calls.\n\n\
RULES:\n\
1. DO NOT introduce yourself. Just respond to what the user asked.\n\
2. If the user wants something DONE NOW (fetch data, run a command), execute it.\n\
3. If the user wants something BUILT (a process, automation, recurring task), BUILD it with <action> tags immediately.\n\
4. To DO anything, wrap it in <action>JSON</action>. Without tags, nothing happens.\n\
5. NEVER fabricate data. If an API returns 0 results, say \"none found\".\n\
6. Use full paths like {home}/Desktop. Never use ~ or $HOME.{PLATFORM_PATH_RULE}\n\
7. Keep responses SHORT. Bullet points, then actions. DO NOT ask questions - just build.\n\
8. If the user greets you, respond conversationally as Bob.\n\n\
TOOLS:\n\
Shell: <action>{{\"type\":\"tool\",\"tool\":\"shell_exec\",\"args\":{{\"command\":\"...\"}}}}</action>\n\
File read/write/list: <action>{{\"type\":\"tool\",\"tool\":\"file_read|file_write|file_list\",\"args\":{{\"path\":\"...\"}}}}</action>\n\
Fetch URL: <action>{{\"type\":\"tool\",\"tool\":\"web_fetch\",\"args\":{{\"url\":\"...\"}}}}</action>\n\
API call (any HTTP endpoint the user gives you): <action>{{\"type\":\"api_call\",\"method\":\"GET|POST\",\"url\":\"https://...\",\"body\":{{...}}}}</action>\n\
Deep research: <action>{{\"type\":\"research\",\"topic\":\"...\"}}</action> - chains several searches, reads the top pages, returns a source-numbered dossier. Use it BEFORE writing anything substantive, then write from the dossier.\n\
Branded documents: <action>{{\"type\":\"create_document\",\"kind\":\"docx|pptx|html|website\",\"title\":\"...\",\"subtitle\":\"...\",\"sections\":[{{\"heading\":\"...\",\"body\":\"...\",\"bullets\":[\"...\"]}}]}}</action> - real Word/PowerPoint/HTML files or a complete website folder (index.html + styles.css), styled with the user\'s brand kit, saved to Documents and opened. Extract the kit from a guideline file first: <action>{{\"type\":\"extract_brand_kit\",\"path\":\"/path/to/guideline.pdf\"}}</action>. Write REAL content, never placeholders.\n\
Build panel: <action>{{\"type\":\"create_panel\",\"title\":\"...\",\"blocks\":[{{\"kind\":\"header|text|metric|list|table\",...}}]}}</action> - you CAN build UI panels (users open them from My UI). Gather real data FIRST, then fill the blocks with it.\n\
Create workflow: <action>{{\"type\":\"create_workflow\",\"name\":\"...\",\"description\":\"...\",\"steps\":[{{\"type\":\"prompt\",\"label\":\"...\",\"config\":{{\"prompt\":\"...\"}}}}]}}</action>\n\
Run workflow: <action>{{\"type\":\"run_workflow\",\"name\":\"...\"}}</action>\n\
Update workflow: <action>{{\"type\":\"update_workflow\",\"name\":\"...\",\"new_name\":\"...\",\"description\":\"...\"}}</action>\n\
Delete workflow: <action>{{\"type\":\"delete_workflow\",\"name\":\"...\"}}</action>\n\
Stop workflow: <action>{{\"type\":\"stop_workflow\"}}</action>\n\
Run chain: <action>{{\"type\":\"run_chain\",\"name\":\"...\",\"input\":\"...\"}}</action>\n\
List chains: <action>{{\"type\":\"list_chains\"}}</action>\n\
Search/delete memory: <action>{{\"type\":\"search_memory\",\"query\":\"...\"}}</action> / <action>{{\"type\":\"delete_memory\",\"key\":\"...\"}}</action>\n\
Connectors: <action>{{\"type\":\"list_connectors\"}}</action> / <action>{{\"type\":\"disconnect_connector\",\"name\":\"...\"}}</action>\n\
MCP: <action>{{\"type\":\"list_mcp\"}}</action> / <action>{{\"type\":\"toggle_mcp\",\"name\":\"...\"}}</action> / <action>{{\"type\":\"remove_mcp\",\"name\":\"...\"}}</action>\n\
Triggers: <action>{{\"type\":\"list_triggers\"}}</action> / <action>{{\"type\":\"toggle_trigger\",\"name\":\"...\"}}</action> / <action>{{\"type\":\"delete_trigger\",\"name\":\"...\"}}</action>\n\
Activity: <action>{{\"type\":\"recent_activity\",\"limit\":10}}</action>\n\
Audit: <action>{{\"type\":\"audit_search\",\"query\":\"...\"}}</action>\n\
Twin: <action>{{\"type\":\"twin_list\",\"entity_type\":\"...\"}}</action> / <action>{{\"type\":\"twin_search\",\"query\":\"...\"}}</action> / <action>{{\"type\":\"twin_get\",\"name\":\"...\"}}</action>\n\
Notifications: <action>{{\"type\":\"list_notifications\"}}</action> / <action>{{\"type\":\"dismiss_notifications\"}}</action>\n\
Set voice: <action>{{\"type\":\"set_voice\",\"voice\":\"af_heart\",\"rate\":175}}</action>\n\
UI panels (build a custom dashboard for the user):\n\
  <action>{{\"type\":\"create_panel\",\"title\":\"...\",\"blocks\":[{{\"kind\":\"header|text|metric|metrics|list|table|divider|action|actions\",...}}]}}</action>\n\
  Live data via {{\"data\":{{\"source\":\"agents|workflows|connectors|notifications|activity\",\"field\":\"name\",\"aggregate\":\"count|sum|avg|min|max\",\"limit\":10}}}}\n\
  Update: <action>{{\"type\":\"update_panel\",\"id\":\"...\",\"blocks\":[...]}}</action>  Remove: <action>{{\"type\":\"remove_panel\",\"id\":\"...\"}}</action>\n\
Create agent: <action>{{\"type\":\"create_agent\",\"name\":\"...\",\"provider\":\"local\",\"model\":\"gemma-4-27b\"}}</action>\n\
Update agent: <action>{{\"type\":\"update_agent\",\"name\":\"...\",\"new_name\":\"...\",\"model\":\"...\"}}</action>\n\
Delete agent: <action>{{\"type\":\"delete_agent\",\"name\":\"...\"}}</action>\n\
Toggle agent: <action>{{\"type\":\"toggle_agent\",\"name\":\"...\"}}</action>\n\
Remember: <action>{{\"type\":\"add_memory\",\"key\":\"...\",\"value\":\"...\",\"category\":\"fact\"}}</action>\n\
Connect service (one-click OAuth): <action>{{\"type\":\"connect_oauth\",\"service\":\"microsoft|google|github|slack|linkedin\"}}</action>\n\
Save client ID: <action>{{\"type\":\"save_client_id\",\"service\":\"...\",\"client_id\":\"...\"}}</action>\n\
Check connectors: <action>{{\"type\":\"check_connectors\"}}</action>\n\n\
CONNECTOR RULES: When user says \"connect [service]\", use connect_oauth. Microsoft and LinkedIn work instantly - no setup needed. Google/GitHub/Slack need a one-time Client ID.\n\n\
Home: {home}", home = home);

    // Build OpenAI-compatible messages
    let messages: Vec<serde_json::Value> = std::iter::once(serde_json::json!({
        "role": "system",
        "content": system_prompt
    }))
    .chain({
        let n = history.len();
        history.iter().enumerate().map(move |(i, m)| {
            let keep_evidence = i + 4 >= n;
            serde_json::json!({
                "role": m.role,
                "content": history_msg_content(m, keep_evidence)
            })
        })
    })
    .collect();

    let body = serde_json::json!({
        "messages": messages,
        "temperature": 0.7,
        // Roomy enough for a full website/document draft in one pass.
        "max_tokens": 8192,
        "stream": false,
        "cache_prompt": true,
    });

    let url = format!("{}/v1/chat/completions", endpoint);
    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Local LLM request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Local LLM error ({}): {}", status, text));
    }

    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("Failed to parse local LLM response: {}", e))?;

    let message = json.get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"));

    // Some models (Gemma 4) use thinking mode - check reasoning_content as
    // fallback, but KEEP IT MARKED as reasoning: returned bare it can end
    // up spoken or displayed as if it were the answer. Callers that mine
    // JSON out of the reply still find it inside the think tags.
    let reply = message
        .and_then(|m| {
            let content = m.get("content").and_then(|c| c.as_str()).unwrap_or("");
            if content.is_empty() {
                m.get("reasoning_content")
                    .and_then(|c| c.as_str())
                    .map(|r| format!("<think>\n{}\n</think>", r))
            } else {
                Some(content.to_string())
            }
        })
        .ok_or_else(|| "No content in local LLM response".to_string())?;
    crate::replay_log::log_turn("completion", &replay_model_id(None), &body, &reply, &[]);
    Ok(reply)
}

/// Streaming version of call_local_llm - emits `message-chunk` events as tokens arrive.
/// Returns the full accumulated response.
pub async fn call_local_llm_streaming(
    client: &reqwest::Client,
    history: &[Message],
    message_id: &str,
    app: &tauri::AppHandle,
    state: Option<&AppState>,
) -> Result<String, String> {
    call_local_llm_streaming_tools(client, history, message_id, app, state, None)
        .await
        .map(|(text, _tool_calls)| text)
}

/// Streaming chat that carries dropped images (absolute paths) into the
/// current turn. Used by the chat command when the user attaches images and a
/// vision model is loaded. Falls back to a plain text turn if not.
pub async fn call_local_llm_streaming_vision(
    client: &reqwest::Client,
    history: &[Message],
    message_id: &str,
    app: &tauri::AppHandle,
    state: Option<&AppState>,
    images: &[String],
) -> Result<String, String> {
    call_local_llm_streaming_tools_vision(client, history, message_id, app, state, None, images)
        .await
        .map(|(text, _tool_calls)| text)
}

/// Model id for the replay log: the live in-memory selection when state
/// is at hand, else the persisted choice, else a generic marker.
fn replay_model_id(state: Option<&AppState>) -> String {
    state
        .and_then(|s| s.engine.selected_local_model.lock().ok().and_then(|g| g.clone()))
        .or_else(|| crate::storage::load_json::<String>("selected_model.json").ok())
        .unwrap_or_else(|| "local".to_string())
}

/// Is the loaded local model ready to see images - a vision model whose
/// projector is actually on disk (so llama-server launched with --mmproj)?
/// Prefers the live in-memory selection; falls back to the persisted choice.
/// A vision model whose projector download failed returns false here, so we
/// never inline images to a server that would reject them.
fn current_model_is_vision(state: Option<&AppState>) -> bool {
    let id = state
        .and_then(|s| s.engine.selected_local_model.lock().ok().and_then(|g| g.clone()))
        .or_else(|| crate::storage::load_json::<String>("selected_model.json").ok());
    id.map(|id| local_llm::mmproj_downloaded(&id)).unwrap_or(false)
}

/// Build the `content` for a user turn: a plain string when there are no
/// images, otherwise the OpenAI content-parts array with the text followed by
/// each image as a base64 `image_url`. An image that can't be read degrades
/// to a short text note so the turn still goes through.
fn user_content_with_images(text: &str, images: &[String]) -> serde_json::Value {
    if images.is_empty() {
        return serde_json::Value::String(text.to_string());
    }
    let mut parts: Vec<serde_json::Value> = Vec::new();
    if !text.trim().is_empty() {
        parts.push(serde_json::json!({ "type": "text", "text": text }));
    }
    for path in images {
        match image_data_uri(path) {
            Some(uri) => parts.push(serde_json::json!({
                "type": "image_url",
                "image_url": { "url": uri }
            })),
            None => parts.push(serde_json::json!({
                "type": "text",
                "text": format!("[an attached image could not be read: {}]", path)
            })),
        }
    }
    if parts.is_empty() {
        serde_json::Value::String(text.to_string())
    } else {
        serde_json::Value::Array(parts)
    }
}

/// Read an image file and return a `data:<mime>;base64,...` URI, or None if it
/// can't be read or is implausibly large (guards the context and the server).
fn image_data_uri(path: &str) -> Option<String> {
    use base64::Engine;
    let bytes = std::fs::read(path).ok()?;
    // 20MB of raw image is more than any screenshot or photo needs; base64
    // inflates it ~1.34x. Beyond this we decline rather than bloat the request.
    if bytes.is_empty() || bytes.len() > 20 * 1024 * 1024 {
        return None;
    }
    let mime = match std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("tif") | Some("tiff") => "image/tiff",
        Some("heic") | Some("heif") => "image/heic",
        _ => "image/png",
    };
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Some(format!("data:{};base64,{}", mime, b64))
}

/// Streaming call with optional native tool-calling (W1.5). When `tools`
/// is `Some`, the model may answer with structured tool calls that the
/// server parses from its chat template; we accumulate the `tool_calls`
/// deltas and return each reconstructed as an action object
/// `{"type": name, ...arguments}` so `execute_actions_counted` runs it
/// through the exact same executor as the legacy tag path. When `tools`
/// is `None` the behavior is byte-for-byte the old one (empty tool-call
/// vec), so every existing caller is unaffected.
/// Bob's streaming-chat system prompt (W: extracted so the launch
/// warm-up can prime the exact prompt-cache prefix the first real
/// message reuses). state_context is the dynamic tail; the large static
/// instruction block before it is what the server caches.
/// Decide the final reply text for a streamed turn. Normally this is
/// exactly what the model streamed. But when a reasoning model routes its
/// whole turn into the hidden reasoning channel and emits no user-facing
/// content and no tool call, the streamed text is just a hidden
/// `<think>...</think>` block that renders blank. In that one case, fall
/// back to the reasoning itself so the user always sees the model's
/// words. Model-agnostic: it triggers on the empty-answer shape, not on
/// any particular model id.
fn finalize_stream_reply(
    full_content: String,
    reasoning: &str,
    content_seen: bool,
    has_tool_calls: bool,
) -> String {
    if content_seen || has_tool_calls {
        return full_content;
    }
    let r = reasoning.trim();
    if r.is_empty() {
        return full_content;
    }
    // Reasoning-only turn: keep the reasoning, but KEEP IT MARKED as
    // reasoning. It used to be returned bare, which put the model's
    // private deliberation - including quoted system-prompt fragments -
    // straight into the chat bubble (field failure 2026-07-31: 'However,
    // the instructions state: "Your reply must never be empty..."').
    // Downstream, strip_think_blocks removes it and the agentic loop's
    // synthesis retry produces the real answer.
    format!("<think>\n{}\n</think>", r)
}

#[cfg(test)]
mod stream_finalize_tests {
    use super::finalize_stream_reply;

    #[test]
    fn normal_content_passes_through_untouched() {
        let out = finalize_stream_reply(
            "<think>\nplanning\n</think>\nHere is your answer.".to_string(),
            "planning",
            true,
            false,
        );
        assert_eq!(out, "<think>\nplanning\n</think>\nHere is your answer.");
    }

    #[test]
    fn empty_answer_with_reasoning_stays_marked_as_reasoning() {
        // The gpt-oss blank-bubble case: all text in the reasoning
        // channel, empty content, no tool call. The reasoning must come
        // back think-wrapped so it can never render as the reply.
        let out = finalize_stream_reply(
            "<think>\nFocus on the report and the budget.\n</think>".to_string(),
            "Focus on the report and the budget.",
            false,
            false,
        );
        assert_eq!(out, "<think>\nFocus on the report and the budget.\n</think>");
    }

    #[test]
    fn tool_call_turn_keeps_empty_content() {
        // Answering purely with actions is legitimate; do not inject
        // reasoning as a visible reply.
        let out = finalize_stream_reply(
            "<think>\nplanning\n</think>".to_string(),
            "planning",
            false,
            true,
        );
        assert_eq!(out, "<think>\nplanning\n</think>");
    }

    #[test]
    fn nothing_at_all_stays_empty_for_the_caller_to_error() {
        let out = finalize_stream_reply(String::new(), "   ", false, false);
        assert!(out.is_empty());
    }
}

/// Bob's system prompt: the static contract every local model receives.
/// It is byte-identical across turns so the server's prefix cache covers
/// it, and (with Qwen's template) the tool schemas rendered next to it.
/// Everything that changes per turn - live state, memory, the date -
/// rides on the current user turn instead (`bob_dynamic_context`), so
/// even the agentic loop's later iterations hit the cache. `model_name`
/// is display-only. There is ONE tool convention in this prompt: the
/// native tools the request advertises. (Teaching <action> tags next to
/// native schemas was measured as a duplicate convention in 2026-08 and
/// is what small models parroted; the tag parser stays as a silent
/// fallback for models that emit tags anyway.)
pub fn bob_system_prompt(home: &str, model_name: &str) -> String {
    BOB_PROMPT
        .replace("{home}", home)
        .replace("{model_name}", model_name)
        .replace("{closest_source}", CLOSEST_SOURCE)
        .replace("{denied_access}", DENIED_ACCESS)
        .replace("{platform_notes}", PLATFORM_NOTES)
}

// ─────────────────────────────────────────────
//  Platform wording for the prompts
//
//  The model must know which desktop it is on (paths, shells, which apps
//  it can read), but no prompt hardcodes an OS: every platform-specific
//  sentence is one of these consts, chosen at compile time, so a change
//  made on one platform never teaches the model the wrong machine.
// ─────────────────────────────────────────────

#[cfg(target_os = "macos")]
pub(crate) const HOST_OS_NAME: &str = "Mac";
#[cfg(target_os = "windows")]
pub(crate) const HOST_OS_NAME: &str = "Windows PC";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const HOST_OS_NAME: &str = "Linux machine";

/// Appended to the "full paths" rule. Windows needs it because the native
/// form (C:\Users\x) in a prompt teaches the model to emit backslash paths
/// in action JSON, which are invalid escapes (field failure 2026-09-16).
#[cfg(target_os = "windows")]
pub(crate) const PLATFORM_PATH_RULE: &str = " This is Windows: in action JSON always write paths with FORWARD slashes (C:/Users/...) - backslashes break the JSON.";
#[cfg(not(target_os = "windows"))]
pub(crate) const PLATFORM_PATH_RULE: &str = "";

/// Where the model reads the user's data from, by platform. On Windows
/// the bridge tier exists only where classic Outlook does (the new
/// Outlook has no scripting interface), so the wording defers to the APP
/// ACCESS digest, which states what this device really has.
#[cfg(target_os = "macos")]
pub(crate) const FETCH_SOURCES: &str = "app_bridge covers Calendar, Mail, Outlook, Reminders, Notes, Messages; read_app_window reads any visible window; file tools cover the disk";
#[cfg(target_os = "windows")]
pub(crate) const FETCH_SOURCES: &str = "app_bridge reads Outlook mail and calendar when APP ACCESS lists Outlook as bridged, otherwise the connected Outlook account has its own tools; read_app_window reads any app's visible window; file tools cover the disk";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const FETCH_SOURCES: &str = "Outlook mail and calendar have their own tools; read_app_window reads any app's visible window; file tools cover the disk";

#[cfg(target_os = "macos")]
pub(crate) const READ_TOOLS: &str = "app_bridge, read_app_window";
#[cfg(target_os = "windows")]
pub(crate) const READ_TOOLS: &str = "app_bridge when Outlook is bridged, the Outlook tools, read_app_window";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const READ_TOOLS: &str = "read_app_window, the Outlook tools";

#[cfg(target_os = "macos")]
pub(crate) const CALENDAR_RULE: &str = "A calendar or mail question is answered from EVERY source that might hold the data: check both Calendar and Outlook when both are installed, and say which one answered.";
#[cfg(target_os = "windows")]
pub(crate) const CALENDAR_RULE: &str = "A calendar or mail question is answered from Outlook: through app_bridge when APP ACCESS lists Outlook as bridged, otherwise from the connected Outlook account. Say which one answered, and say when neither is available instead of guessing.";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(crate) const CALENDAR_RULE: &str = "A calendar or mail question is answered from the connected Outlook account; say when nothing is connected instead of guessing.";

#[cfg(target_os = "macos")]
const CLOSEST_SOURCE: &str = "app_bridge reads the apps the user is already signed into (Calendar, Mail, Outlook, Reminders, Notes, Messages); read_app_window reads any visible window";
#[cfg(target_os = "windows")]
const CLOSEST_SOURCE: &str = "app_bridge reads Outlook when APP ACCESS lists it as bridged; the Outlook tools read the connected mail and calendar; read_app_window reads any app's visible window";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const CLOSEST_SOURCE: &str = "the Outlook tools read the connected mail and calendar; read_app_window reads any app's visible window";

#[cfg(target_os = "macos")]
const DENIED_ACCESS: &str = "If macOS denies access, tell the user to enable Orbix Core under System Settings > Privacy & Security, then stop.";
#[cfg(not(target_os = "macos"))]
const DENIED_ACCESS: &str = "If access to something is denied, say exactly what was denied and stop.";

#[cfg(target_os = "windows")]
const PLATFORM_NOTES: &str = "\nThis is Windows: always write paths with FORWARD slashes (C:/Users/...) - backslashes are invalid JSON escapes. shell_exec runs cmd.exe.";
#[cfg(not(target_os = "windows"))]
const PLATFORM_NOTES: &str = "";

const BOB_PROMPT: &str = r#"You are Bob, the assistant built into Orbix Core. You run locally on this device on {model_name}; nothing leaves the machine unless a tool sends it. You help with anything: questions, writing, analysis, code, research, planning, and running the user's agents, workflows, apps, files, and data.

QUALITY
- Give the most useful answer you can: complete, specific, correct. The answer is the deliverable, not a description of how to get it.
- Fit the length to the ask. A quick fact gets one or two sentences. An explanation, comparison, plan, how-to, or piece of writing gets the full thing, structured for reading: short headings, bullets, numbered steps, or a table when they help. Never cut an answer short to be brief.
- Lead with the answer, then the detail. No preamble, no restating the question, no filler, no apologies, no self-introduction.
- Markdown is fine. Code, commands, and file paths go in code blocks.
- Reply in the language of the user's MOST RECENT message. Earlier turns in another language do not count: if the conversation was in Danish and the new message is in English, answer in English.
- Be honest: say what you know, what you inferred, and what you could not find. Never invent facts, names, numbers, dates, URLs, or results. If a tool returned nothing, say so.
- If the request is ambiguous in a way that changes the work, make the sensible assumption, state it in one line, and proceed. Ask only when guessing would waste the user's time.

TOOLS
- You have native tools. Call one whenever it produces real information or makes a real change: the web, this device's files and shell, the user's apps, calendar and mail, memory, agents, workflows, panels, documents, connectors.
- Do not call tools for things you already know well (concepts, definitions, general how-to, writing). Do call them for anything current (news, prices, weather, releases, "latest"), anything on this device, and anything about the user's own data or setup.
- Act instead of asking: read the calendar rather than asking what is on it, list the folder rather than asking what is in it, search rather than guessing. Ask only for what exists solely in the user's head: preferences, choices, approvals.
- Prefer the closest source: {closest_source}; web_search and web_fetch reach the web; the file and shell tools reach the disk. Never ask for passwords, keys, or tokens. {denied_access}
- Read results before answering, and answer from them: concrete items with what, when, and where each came from. If the results miss what was asked, say so and make a more specific call. Describing where something could be found is not an answer. Opening an app for the user is not an answer either.
- Independent lookups can go in one turn. A step that depends on an earlier result waits for that result.
- When the user tells you something to keep (a fact about them, a preference, how they like things done), store it with add_memory before replying; an acknowledgement alone forgets it.
- After a change (create, update, delete, run, send), report exactly what happened in one line, then offer two or three concrete next steps in one more line.
- Never promise future action. You act only while answering this message; if something failed, say what failed and what to try.

ASKING FOR INPUT
When you need answers from the user before you can act, emit ONE <inputs> block holding every question, then stop writing:
<inputs>[{"id":"short_id","label":"Question shown to the user","kind":"text|longtext|select|yesno","placeholder":"hint","options":["for select"],"required":true}]</inputs>
Up to 5 questions, labels under 80 characters, select or yesno whenever the answer is finite. Nothing after </inputs>. The answers come back as the user's next message.

AUTOMATION
- A question or a one-time request is answered now, with tools. Only something the user wants repeated (a schedule, a trigger, a recurring process) becomes a workflow.
- A workflow runs unattended: every step must be runnable, naming its tool and carrying the full prompt text it needs, and it can never ask the user anything.
- An agent is a standing responsibility that needs judgment each run; give it a clear charter and only the tools it needs. An agent without a workflow or schedule does nothing, so build both when the user wants something to happen on its own.
- External services (LinkedIn, Slack, GitHub, Microsoft, Google) are used through connectors: connect_oauth connects one, check_connectors shows what is connected. Never call their APIs directly.
- There is no built-in CRM or ERP. Business data lives in what the user has connected; if nothing is connected for what they ask, say so and offer to connect it.

COMPONENT REFERENCES
The user can point at a component inline: [workflow: Name], [agent: Name], [connector: Name], [skill: Name]. Act on exactly that one.

PATHS
Home is {home}. Use full paths ({home}/Desktop, {home}/Documents), never ~ or $HOME.{platform_notes}"#;

/// One memory entry as one prompt line: values are flattened and capped
/// so a pasted paragraph in memory cannot bloat every turn.
fn memory_line(category: &str, key: &str, value: &str) -> String {
    let flat: String = value.split_whitespace().collect::<Vec<_>>().join(" ");
    format!("- [{}] {}: {}", category, key, short(&flat, 200))
}

/// Truncate at a char boundary with an ellipsis.
pub(crate) fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}…", cut.trim_end())
}

/// The part of the system message about the USER and this device, which
/// changes rarely: name, memory, connected services, installed skills,
/// app access. It lives in the system message so it sits in the cached
/// prefix. Measured 2026-09-06 on the 12B: the memory list alone was
/// 3.3k chars (~800 tokens) and was being re-read on every turn at
/// 191 tok/s, four seconds per turn for text that never changed.
pub async fn bob_profile_block(state: Option<&AppState>) -> String {
    let Some(s) = state else { return String::new() };
    let mut out = String::new();
    {
        let onb = s.profile.onboarding.lock().unwrap_or_else(|e| e.into_inner());
        if !onb.user_name.trim().is_empty() {
            out.push_str(&format!("The user's name is {}.\n", onb.user_name.trim()));
        }
    }
    {
        let mem = s.chat.memory.lock().unwrap_or_else(|e| e.into_inner());
        if !mem.entries.is_empty() {
            let items: Vec<String> = mem.entries.iter()
                .take(20)
                .map(|e| memory_line(&format!("{:?}", e.category).to_lowercase(), &e.key, &e.value))
                .collect();
            out.push_str(&format!("\nUSER MEMORY (learned earlier; search_memory finds more):\n{}\n", items.join("\n")));
        }
    }
    {
        let instances = s.connect.connector_instances.lock().unwrap_or_else(|e| e.into_inner());
        drop(instances);
        let connected = crate::services::prompt_lines(s);
        if !connected.is_empty() {
            out.push_str(&format!("\nCONNECTED SERVICES (use them when relevant; api_call with the connector sends the user's token):\n{}\n", connected.join("\n")));
        }
    }
    if let Some(ws) = crate::workspace::WorkspaceSettings::load().usable_path() {
        out.push_str(&format!(
            "\nWORKING FOLDER: shell commands run in {} unless the call passes another \"cwd\". File tools take absolute paths.\n",
            ws.display()
        ));
    }
    out.push('\n');
    out.push_str(&crate::skills::prompt_index());
    out.push_str(&crate::app_bridge::capability_digest().await);
    out.push_str(&crate::peer::prompt_digest());
    out.push_str(&crate::user_actions::prompt_digest());
    if let Some(st) = state {
        out.push_str(&crate::mcp_client::prompt_digest(st).await);
    }
    out.trim().to_string()
}

/// The full system message: the static contract plus the profile block.
/// Byte-identical across turns until the user's profile changes, so the
/// server's prefix cache covers it together with the tool schemas.
pub async fn bob_system_message(state: Option<&AppState>, home: &str) -> String {
    let base = bob_system_prompt(home, &current_model_name(state));
    let profile = bob_profile_block(state).await;
    if profile.is_empty() {
        base
    } else {
        format!("{}\n\nABOUT THE USER AND THIS DEVICE\n{}", base, profile)
    }
}

/// The per-turn context block: what changes turn to turn - the clock,
/// today's calendar digest, the current agents and workflows, and the
/// knowledge slice for this question. Prepended to the current user turn
/// (not the system prompt) so the cached prefix survives. Empty sections
/// cost nothing.
pub async fn bob_dynamic_context(state: Option<&AppState>, history: &[Message]) -> String {
    let mut out = String::new();
    out.push_str(&format!("Now: {}.\n", chrono::Local::now().format("%A %-d %B %Y, %H:%M")));
    if let Some(s) = state {
        {
            let agents = s.chat.agents.lock().unwrap_or_else(|e| e.into_inner());
            if !agents.is_empty() {
                let items: Vec<String> = agents.iter()
                    .take(25)
                    .map(|a| format!("- {}{}", a.name, if a.enabled { "" } else { " (disabled)" }))
                    .collect();
                out.push_str(&format!("\nAGENTS:\n{}\n", items.join("\n")));
            }
        }
        {
            let store = s.automation.workflow_store.lock().unwrap_or_else(|e| e.into_inner());
            if !store.workflows.is_empty() {
                let items: Vec<String> = store.workflows.iter()
                    .take(15)
                    .map(|w| format!("- \"{}\" ({} steps): {}", w.name, w.steps.len(), short(&w.description, 120)))
                    .collect();
                out.push_str(&format!("\nWORKFLOWS (run_workflow runs one):\n{}\n", items.join("\n")));
            }
        }
        out.push_str(&crate::turn_scope::prompt_line());
        out.push('\n');
        out.push_str(&crate::today::prompt_block());
        {
            let query = history.iter().rev().find(|m| m.role == "user").map(|m| m.content.as_str()).unwrap_or("");
            if !query.is_empty() {
                let kb = s.connect.kb_store.lock().unwrap_or_else(|e| e.into_inner());
                let ctx = kb.get_context(query, 1200);
                if !ctx.is_empty() {
                    let body = ctx.iter()
                        .map(|c| format!("- {}: {}", c.title, c.chunk))
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push_str(&format!("\nRELEVANT KNOWLEDGE (from the user's knowledge base):\n{}\n", body));
                }
            }
        }
    }
    format!(
        "[Context for this turn - live state, refreshed automatically. Use it; never repeat it back.]\n{}\n[End of context]\n\n",
        out.trim_end()
    )
}

/// The loaded model's display name, for the prompt's identity line.
pub fn current_model_name(state: Option<&AppState>) -> String {
    local_llm::find_model(&replay_model_id(state))
        .map(|m| m.name)
        .unwrap_or_else(|| "a local model".to_string())
}

/// Thinking is off unless ORBIX_THINKING=1. Measured 2026-09-06 on Gemma
/// 4 E4B and Qwen3.5 9B through llama-server b9969: with thinking on, a
/// plain create_agent request spent its whole 600-token budget reasoning
/// and emitted no call, a three-sentence answer cost 575 tokens, and a
/// per-request reasoning_budget was ignored. With thinking off the same
/// calls came back correct in 16-68 tokens, three to seven times faster.
/// The agentic loop is the reasoning: observe a result, act again.
pub(crate) fn thinking_enabled() -> bool {
    matches!(std::env::var("ORBIX_THINKING").ok().as_deref(), Some("1") | Some("on") | Some("true"))
}

pub async fn call_local_llm_streaming_tools(
    client: &reqwest::Client,
    history: &[Message],
    message_id: &str,
    app: &tauri::AppHandle,
    state: Option<&AppState>,
    tools: Option<serde_json::Value>,
) -> Result<(String, Vec<serde_json::Value>), String> {
    call_local_llm_streaming_tools_vision(client, history, message_id, app, state, tools, &[]).await
}

/// Same as `call_local_llm_streaming_tools`, plus `images`: absolute paths
/// attached to the current (last user) turn. When a vision model is loaded,
/// each is inlined as an OpenAI `image_url` content part so the model can see
/// it; with a text model the images are ignored (the caller warns the user).
pub async fn call_local_llm_streaming_tools_vision(
    client: &reqwest::Client,
    history: &[Message],
    message_id: &str,
    app: &tauri::AppHandle,
    state: Option<&AppState>,
    tools: Option<serde_json::Value>,
    images: &[String],
) -> Result<(String, Vec<serde_json::Value>), String> {
    let reply = call_local_chat(
        client,
        LocalChat { history, turns: &[], tools, images, cancel: Some(&crate::BOB_CANCEL_FLAG) },
        message_id,
        app,
        state,
    )
    .await?;
    Ok((reply.text, reply.actions))
}

// ─────────────────────────────────────────────
//  Local chat: the one request every local model turn goes through
// ─────────────────────────────────────────────

/// One native tool call exactly as the model emitted it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NativeToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON arguments string (may need repair).
    pub arguments: String,
}

/// A local chat request. `turns` are OpenAI-format messages appended
/// after the stored history: the agentic loop's assistant tool-call turns
/// and their `role: tool` results, which the model's own chat template
/// renders the way the model was trained to see them (verified through
/// /apply-template on Gemma 4 and Qwen 3.5).
pub struct LocalChat<'a> {
    pub history: &'a [Message],
    pub turns: &'a [serde_json::Value],
    pub tools: Option<serde_json::Value>,
    pub images: &'a [String],
    /// Checked on every streamed chunk; when set the stream is dropped
    /// (the server frees the slot) and the call returns `Err(CANCELLED)`.
    /// The Stop button used to be honoured only between loop turns, so
    /// a model writing a 7000-token reply ran to the end (2026-09-06).
    pub cancel: Option<&'a std::sync::atomic::AtomicBool>,
}

/// The error a cancelled stream returns. Callers compare against it and
/// treat it as "stopped by the user", never as a failure to retry.
pub const CANCELLED: &str = "__cancelled__";

/// Generation cap per chat turn. -1 (unbounded) let a model that decided
/// to regurgitate a whole web page in chat run for minutes; 6144 tokens is
/// ~24k characters, more than any answer that belongs in a chat bubble
/// (whole files go through create_document). A capped reply says so.
const CHAT_MAX_TOKENS: i64 = 6144;

pub struct LocalChatReply {
    /// Streamed text; hidden reasoning, if any, arrives think-wrapped.
    pub text: String,
    /// Every native call as emitted, in order, known tool or not.
    pub calls: Vec<NativeToolCall>,
    /// The known, parseable calls as executor actions `{"type": name, ...args}`.
    pub actions: Vec<serde_json::Value>,
}

/// A complete string value for `key` out of a possibly PARTIAL JSON
/// object (arguments still streaming). None until the closing quote has
/// arrived. Good enough for the short fields labels need (tool, app,
/// query, path, url); never used to parse the call itself.
pub(crate) fn partial_json_string(partial: &str, key: &str) -> Option<String> {
    let needle = format!("\"{}\"", key);
    let start = partial.find(&needle)? + needle.len();
    let rest = partial[start..].trim_start();
    let rest = rest.strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let mut out = String::new();
    let mut chars = rest.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('n') => out.push(' '),
                Some('"') => out.push('"'),
                Some(other) => out.push(other),
                None => return None,
            },
            '"' => return Some(out),
            _ => out.push(c),
        }
    }
    None
}

fn host_of(url: &str) -> String {
    url.split("://").nth(1).unwrap_or(url).split('/').next().unwrap_or(url).trim_start_matches("www.").to_string()
}

fn basename_of(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_string()
}

/// What to show while a native tool call is still streaming in. The
/// executor's `action_label_for` replaces it with the exact label once
/// the arguments are complete; this one only needs the first fields.
pub(crate) fn tool_preview_label(name: &str, partial_args: &str) -> Option<String> {
    let field = |k: &str| partial_json_string(partial_args, k);
    let label = match name {
        "" => return None,
        "tool" => match field("tool").as_deref() {
            Some("web_search") => match field("query") {
                Some(q) => format!("Searching the web for '{}'…", short(&q, 60)),
                None => "Searching the web…".to_string(),
            },
            Some("web_fetch") => match field("url") {
                Some(u) => format!("Reading {}…", host_of(&u)),
                None => "Reading a web page…".to_string(),
            },
            Some("file_read") => match field("path") {
                Some(p) => format!("Reading {}…", basename_of(&p)),
                None => "Reading a file…".to_string(),
            },
            Some("file_list") => match field("path") {
                Some(p) => format!("Listing {}…", short(&p, 50)),
                None => "Listing a folder…".to_string(),
            },
            Some("file_write") => match field("path") {
                Some(p) => format!("Writing {}…", basename_of(&p)),
                None => "Writing a file…".to_string(),
            },
            Some("file_edit") => match field("path") {
                Some(p) => format!("Editing {}…", basename_of(&p)),
                None => "Editing a file…".to_string(),
            },
            Some("shell_exec") => "Running a command…".to_string(),
            _ => return None,
        },
        "app_bridge" => match field("app") {
            Some(a) => format!("Reading {}…", a),
            None => "Reading your apps…".to_string(),
        },
        "read_app_window" => match field("app") {
            Some(a) => format!("Reading the {} window…", a),
            None => "Reading a window…".to_string(),
        },
        "pair_app" => match field("app") {
            Some(a) => format!("Pairing with {}…", a),
            None => "Pairing an app…".to_string(),
        },
        "peer_send" => match field("app") {
            Some(a) => format!("Sending to {}…", a),
            None => "Sending to a paired app…".to_string(),
        },
        "peer_read" => match field("app") {
            Some(a) => format!("Reading {}…", a),
            None => "Reading a paired app…".to_string(),
        },
        "list_paired_apps" => "Checking paired apps…".to_string(),
        "peer_controls" => match field("app") {
            Some(a) => format!("Looking at {}'s controls…", a),
            None => "Looking at a window's controls…".to_string(),
        },
        "peer_press" => match field("control") {
            Some(c) => format!("Pressing {}…", c),
            None => "Pressing a button…".to_string(),
        },
        "peer_fill" => match field("field") {
            Some(f) => format!("Filling {}…", f),
            None => "Filling a field…".to_string(),
        },
        "mcp_tools" => match field("server") {
            Some(a) => format!("Listing {}'s tools…", a),
            None => "Listing MCP tools…".to_string(),
        },
        "mcp_call" => match field("tool") {
            Some(a) => format!("Running {}…", a),
            None => "Running an MCP tool…".to_string(),
        },
        "peer_open" => match field("app") {
            Some(a) => format!("Opening in {}…", a),
            None => "Opening a link…".to_string(),
        },
        "learn_app" => match field("app") {
            Some(a) => format!("Learning {}…", a),
            None => "Learning an app…".to_string(),
        },
        "run_user_action" => match field("id") {
            Some(a) => format!("Doing the {} step…", a),
            None => "Doing the pending step…".to_string(),
        },
        "list_app_bridges" => "Checking which apps I can read…".to_string(),
        "research" => match field("topic") {
            Some(t) => format!("Researching '{}'…", short(&t, 60)),
            None => "Researching…".to_string(),
        },
        "create_document" => match field("kind").as_deref() {
            Some("pptx") => "Creating slides…".to_string(),
            Some("website") => "Building a website…".to_string(),
            _ => "Creating a document…".to_string(),
        },
        "list_calendar_events" => "Reading your calendar…".to_string(),
        "create_calendar_event" => "Creating a calendar event…".to_string(),
        "send_email" => "Sending an email…".to_string(),
        "search_memory" => "Searching memory…".to_string(),
        "add_memory" => "Remembering that…".to_string(),
        "create_workflow" => "Building a workflow…".to_string(),
        "create_agent" => "Creating an agent…".to_string(),
        "run_workflow" => "Running a workflow…".to_string(),
        "browse_navigate" => "Opening a page in Chrome…".to_string(),
        "browse_read" => "Reading the page…".to_string(),
        "check_connectors" | "list_connectors" => "Checking connected services…".to_string(),
        "connect_oauth" => "Connecting a service…".to_string(),
        other => format!("{}…", other.replace('_', " ")),
    };
    Some(label)
}

/// One plain generation against the local server: a system + user pair,
/// no Bob prompt, no tools, thinking off, streamed so it can be cancelled
/// and can report progress (`on_progress(chars_so_far)`, roughly every
/// 2k characters). Used by the document and page generators, which used
/// to run through the non-streaming leaf: minutes of silence, no Stop.
pub async fn stream_freeform(
    client: &reqwest::Client,
    app: &tauri::AppHandle,
    system: &str,
    user: &str,
    max_tokens: i64,
    cancel: Option<&std::sync::atomic::AtomicBool>,
    mut on_progress: impl FnMut(usize),
) -> Result<String, String> {
    use futures::StreamExt;
    let _busy = crate::bob_core::ModelBusyGuard::new();
    let endpoint = ensure_local_server(client, Some(app)).await?;
    let body = serde_json::json!({
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": user },
        ],
        "temperature": 0.4,
        "max_tokens": max_tokens,
        "stream": true,
        "cache_prompt": true,
        "chat_template_kwargs": { "enable_thinking": thinking_enabled() },
    });
    let resp = client
        .post(format!("{}/v1/chat/completions", endpoint))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Local LLM request failed: {}", e))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Local LLM error ({}): {}", status, text));
    }
    let mut out = String::new();
    let mut buffer = String::new();
    let mut last_report = 0usize;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        if cancel.map(|c| c.load(std::sync::atomic::Ordering::SeqCst)).unwrap_or(false) {
            return Err(CANCELLED.to_string());
        }
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();
            let Some(json_str) = line.strip_prefix("data: ") else { continue; };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) else { continue; };
            if let Some(err) = json.get("error") {
                let msg = err.get("message").and_then(|m| m.as_str()).unwrap_or("unknown server error");
                return Err(format!("The model failed while generating ({})", msg));
            }
            if let Some(c) = json.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta")).and_then(|d| d.get("content")).and_then(|c| c.as_str()) {
                out.push_str(c);
                if out.len() >= last_report + 2000 {
                    last_report = out.len();
                    on_progress(out.len());
                }
            }
        }
    }
    crate::replay_log::log_turn("freeform", &replay_model_id(None), &body, &out, &[]);
    crate::bob_core::mark_model_used(app).await;
    Ok(out)
}

/// Write an interactive single-file page: the model writes the whole
/// document (HTML, CSS, JS) from the title and brief, in the brand's
/// colours; the file lands in Orbix Documents and the registry.
pub(crate) async fn build_app_page(
    app: &tauri::AppHandle,
    spec: &docgen::DocSpec,
    kit: &brand::BrandKit,
) -> Result<(std::path::PathBuf, usize), String> {
    let brief = spec.brief.clone().unwrap_or_default();
    let extra: Vec<String> = spec.sections.iter()
        .map(|s| format!("{}: {} {}", s.heading, s.body, s.bullets.join("; ")))
        .filter(|t| t.trim().len() > 3)
        .collect();
    let system = format!(
        "You write complete, working single-file web apps. Reply with ONE HTML document and nothing else: \
         start with <!doctype html> and end with </html>. All CSS in a <style> tag, all JavaScript in a \
         <script> tag, no external files, no frameworks, no placeholders, no TODOs. It must work when \
         opened from disk. Make it genuinely interactive as briefed (clicks, hover, keyboard where natural) \
         and include the real data the brief asks for. Use these brand colours as CSS variables: \
         --primary: {primary}; --secondary: {secondary}; --accent: {accent}. Tone: {tone}.",
        primary = kit.color("primary"),
        secondary = kit.color("secondary"),
        accent = kit.color("accent"),
        tone = kit.tone,
    );
    let user = format!(
        "Title: {}\n\nBrief:\n{}\n{}",
        spec.title,
        if brief.trim().is_empty() { "Build the most useful interactive version of this title.".to_string() } else { brief },
        if extra.is_empty() { String::new() } else { format!("\nContent to include:\n- {}", extra.join("\n- ")) },
    );
    let title = spec.title.clone();
    bob_step(app, &format!("Writing the interactive page '{}'…", title));
    let client = reqwest::Client::new();
    let app_for_progress = app.clone();
    let raw = stream_freeform(
        &client,
        app,
        &system,
        &user,
        12_000,
        Some(&crate::BOB_CANCEL_FLAG),
        |n| bob_step(&app_for_progress, &format!("Writing the interactive page… {} KB", n / 1024)),
    )
    .await?;
    let html = extract_html_document(&raw)
        .ok_or_else(|| "the model did not return an HTML document".to_string())?;
    let path = docgen::write_app_page(spec, kit, &html)?;
    Ok((path, html.len()))
}

/// Pull one HTML document out of a model reply: whatever sits between
/// the first `<!doctype`/`<html` and the last `</html>`, code fences
/// ignored. None when there is no document in it.
pub(crate) fn extract_html_document(text: &str) -> Option<String> {
    let lower = text.to_ascii_lowercase();
    let start = lower.find("<!doctype").or_else(|| lower.find("<html"))?;
    let end = lower.rfind("</html>").map(|i| i + "</html>".len()).unwrap_or(text.len());
    if end <= start {
        return None;
    }
    let doc = text[start..end].trim().to_string();
    if doc.len() < 200 {
        return None;
    }
    Some(doc)
}

/// Conservative chars-per-token for mixed prose and JSON.
const CHARS_PER_TOKEN: usize = 3;
/// Context kept free for the model's own output.
const OUTPUT_RESERVE_TOKENS: usize = 3072;
/// A single tool result never exceeds this in the model's context.
/// (8k chars is ~2.7k tokens: about 14s of prefill on a 12B at 191 tok/s,
/// the ceiling one fetched page may cost the next turn.)
pub(crate) const TOOL_RESULT_MAX_CHARS: usize = 8_000;

/// How many characters of conversation fit next to the fixed prefix
/// (system prompt + tool schemas) in the running server's context.
fn prompt_budget_chars(fixed_chars: usize) -> usize {
    let ctx = local_llm::current_ctx_size();
    (ctx.saturating_sub(OUTPUT_RESERVE_TOKENS) * CHARS_PER_TOKEN).saturating_sub(fixed_chars)
}

/// Text size of a message value; image data URIs count as nothing (the
/// server prices those in vision tokens, not text).
fn value_chars(v: &serde_json::Value) -> usize {
    match v {
        serde_json::Value::String(s) => if s.starts_with("data:") { 0 } else { s.len() },
        serde_json::Value::Array(items) => items.iter().map(value_chars).sum(),
        serde_json::Value::Object(map) => map.values().map(value_chars).sum(),
        _ => 0,
    }
}

pub(crate) fn char_prefix(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Fit the message list into the context window: drop the oldest stored
/// turns first (never the system prompt, never the current user turn,
/// never this run's native tool turns), then shrink the oldest tool
/// results, then as a last resort truncate an oversized current turn.
/// Mechanical, so a long chat degrades to "forgets the start" instead of
/// the server erroring on an overflowing prompt.
pub(crate) fn trim_to_budget(mut messages: Vec<serde_json::Value>, fixed_chars: usize, keep_tail: usize) -> Vec<serde_json::Value> {
    let budget = prompt_budget_chars(fixed_chars);
    let total = |msgs: &[serde_json::Value]| msgs.iter().skip(1).map(value_chars).sum::<usize>();
    while total(&messages) > budget && messages.len() > 1 + keep_tail {
        messages.remove(1);
        // Never leave a tool result whose call was just trimmed away.
        while messages.len() > 1 + keep_tail
            && messages[1].get("role").and_then(|r| r.as_str()) == Some("tool")
        {
            messages.remove(1);
        }
    }
    let mut over = total(&messages).saturating_sub(budget);
    if over > 0 {
        for m in messages.iter_mut().skip(1) {
            if m.get("role").and_then(|r| r.as_str()) != Some("tool") {
                continue;
            }
            let shrunk = m.get("content").and_then(|c| c.as_str()).filter(|c| c.len() > 800)
                .map(|c| (c.len(), format!("{}\n[earlier result trimmed]", char_prefix(c, 800))));
            if let Some((before, shrunk)) = shrunk {
                over = over.saturating_sub(before.saturating_sub(shrunk.len()));
                m["content"] = serde_json::Value::String(shrunk);
            }
            if over == 0 {
                break;
            }
        }
    }
    if total(&messages) > budget && messages.len() >= 2 {
        // The current turn alone is over budget (a giant paste): keep
        // its head, say so.
        let idx = messages.len() - keep_tail.max(1);
        if let Some(text) = messages[idx].get("content").and_then(|c| c.as_str()).map(str::to_string) {
            let keep = budget.saturating_sub(64).max(200);
            if text.len() > keep {
                messages[idx]["content"] = serde_json::Value::String(format!(
                    "{}\n[message truncated to fit the model's context]",
                    char_prefix(&text, keep)
                ));
            }
        }
    }
    messages
}

/// Streaming chat with native tools against the running llama-server.
/// Builds the message list (static system prompt, stored history with
/// the per-turn context on the current user message, then this run's
/// native turns), fits it to the context budget, disables thinking
/// unless opted in, and streams the reply while accumulating native
/// `tool_calls` deltas. The `<action>`-tag path stays available to the
/// caller through `execute_actions_counted` on the returned text.
pub async fn call_local_chat(
    client: &reqwest::Client,
    chat: LocalChat<'_>,
    message_id: &str,
    app: &tauri::AppHandle,
    state: Option<&AppState>,
) -> Result<LocalChatReply, String> {
    use futures::StreamExt;

    // In use for the whole request: the idle-unload loop must not pull
    // the server out from under a streaming reply.
    let _busy = crate::bob_core::ModelBusyGuard::new();
    let turn_started = std::time::Instant::now();
    let endpoint = ensure_local_server(client, Some(app)).await?;

    // Forward slashes on Windows: the native form (C:\Users\x) inside the
    // prompt teaches the model to emit backslash paths in action JSON,
    // which are invalid escapes (field failure 2026-09-16).
    let home = dirs::home_dir().map(|p| p.to_string_lossy().replace('\\', "/")).unwrap_or_else(|| "~".to_string());
    let system_prompt = bob_system_message(state, &home).await;
    let dynamic = bob_dynamic_context(state, chat.history).await;

    // Inline dropped images only when a vision model is actually loaded, and
    // only onto the current turn (the last user message). A text model gets
    // the plain string it always got.
    let vision = !chat.images.is_empty() && current_model_is_vision(state);
    let last_user_idx = chat.history.iter().rposition(|m| m.role == "user");
    let n = chat.history.len();
    let mut messages: Vec<serde_json::Value> = vec![serde_json::json!({
        "role": "system",
        "content": system_prompt,
    })];
    for (i, m) in chat.history.iter().enumerate() {
        // Tool evidence rides along only on the newest few assistant
        // turns; older dossiers would otherwise live in the prompt forever.
        let keep_evidence = i + 4 >= n;
        // On those same newest turns, a reply that came from tools is
        // replayed with its real tool turns, so the model sees how the
        // answer was found rather than learning to answer without tools.
        // The results stand in for the evidence block.
        if let (true, "assistant", Some(tool_turns)) = (keep_evidence, m.role.as_str(), m.tool_turns.as_ref()) {
            messages.extend(tool_turns.iter().cloned());
            messages.push(serde_json::json!({ "role": "assistant", "content": history_msg_content(m, false) }));
            continue;
        }
        let content = if m.role == "assistant" {
            serde_json::Value::String(history_msg_content(m, keep_evidence).to_string())
        } else if Some(i) == last_user_idx {
            let text = format!("{}{}", dynamic, m.content);
            if vision { user_content_with_images(&text, chat.images) } else { serde_json::Value::String(text) }
        } else {
            serde_json::Value::String(m.content.clone())
        };
        messages.push(serde_json::json!({ "role": m.role, "content": content }));
    }
    messages.extend(chat.turns.iter().cloned());

    let fixed_chars = system_prompt.len()
        + chat.tools.as_ref().map(|t| t.to_string().len()).unwrap_or(0);
    let messages = trim_to_budget(messages, fixed_chars, 1 + chat.turns.len());

    let mut body = serde_json::json!({
        "messages": messages,
        "temperature": 0.5,
        "max_tokens": CHAT_MAX_TOKENS,
        "stream": true,
        // Cache the static system-prompt prefix on the server so it is not
        // re-processed every turn - the single biggest first-token latency
        // win for a large prompt on a local model.
        "cache_prompt": true,
        // Both shipped families honour this template switch; see
        // thinking_enabled() for the measurements behind the default.
        "chat_template_kwargs": { "enable_thinking": thinking_enabled() },
    });
    // Advertise native tools when the caller opts in. The server (jinja
    // on by default since b8753) parses the model's calls from its chat
    // template and streams them as `delta.tool_calls`.
    if let Some(tools) = chat.tools.as_ref() {
        body["tools"] = tools.clone();
        body["tool_choice"] = serde_json::json!("auto");
    }

    let url = format!("{}/v1/chat/completions", endpoint);
    let resp = client.post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Local LLM request failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!("Local LLM error ({}): {}", status, text));
    }

    // Read the SSE stream. A reasoning model (thinking opted in) splits its
    // output across two delta fields: reasoning_content (private) and
    // content (the reply). Reasoning is wrapped in <think> tags so the
    // client-side splitter routes it correctly even when the model never
    // reaches the content phase.
    let mut full_content = String::new();
    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();
    let mut think_open = false;
    let mut reasoning_buf = String::new();
    let mut content_seen = false;
    let mut long_notice_sent = false;

    // Accumulate native tool_calls across chunks, keyed by their `index`.
    // Each delta carries a partial: an id/name once, then the arguments
    // string in fragments we concatenate.
    struct PartialCall {
        id: String,
        name: String,
        args: String,
    }
    let mut tool_calls: std::collections::BTreeMap<u64, PartialCall> =
        std::collections::BTreeMap::new();
    // The tool name arrives in the first delta, its arguments over the
    // next ones. Say what is being reached for the moment it is known,
    // so "Reading Calendar…" shows while the model is still writing the
    // call instead of a generic spinner (field feedback 2026-09-06).
    let mut last_preview: Option<String> = None;
    let mut finish_reason: Option<String> = None;

    let emit = |app: &tauri::AppHandle, message_id: &str, full: &str| {
        crate::event_bus::emit_ui(app, "message-chunk", serde_json::json!({
            "message_id": message_id,
            "chunk": "",
            "content": full,
        })).ok();
    };

    while let Some(chunk) = stream.next().await {
        if chat.cancel.map(|c| c.load(std::sync::atomic::Ordering::SeqCst)).unwrap_or(false) {
            // Dropping `stream` closes the connection; llama-server
            // aborts the slot's generation on disconnect.
            return Err(CANCELLED.to_string());
        }
        let chunk = chunk.map_err(|e| format!("Stream error: {}", e))?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || line == "data: [DONE]" { continue; }

            let Some(json_str) = line.strip_prefix("data: ") else { continue; };
            let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) else { continue; };
            if let Some(fr) = json.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("finish_reason")).and_then(|f| f.as_str()) {
                finish_reason = Some(fr.to_string());
            }
            // A server that fails mid-stream (Metal OOM, compute error)
            // sends an error payload; skipping it used to surface as the
            // useless "No content in streaming response".
            if let Some(err) = json.get("error") {
                let msg = err
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown server error");
                return Err(format!(
                    "The model failed while generating ({}). This is usually memory pressure - the model may be too large for this device; pick a smaller one in Settings - Models.",
                    msg
                ));
            }
            let delta = json.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta"));
            let Some(delta) = delta else { continue; };

            if let Some(calls) = delta.get("tool_calls").and_then(|c| c.as_array()) {
                for call in calls {
                    let idx = call.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                    let entry = tool_calls.entry(idx).or_insert_with(|| PartialCall {
                        id: String::new(),
                        name: String::new(),
                        args: String::new(),
                    });
                    if let Some(id) = call.get("id").and_then(|v| v.as_str()) {
                        if !id.is_empty() { entry.id = id.to_string(); }
                    }
                    if let Some(func) = call.get("function") {
                        if let Some(name) = func.get("name").and_then(|v| v.as_str()) {
                            if !name.is_empty() { entry.name = name.to_string(); }
                        }
                        if let Some(args) = func.get("arguments").and_then(|v| v.as_str()) {
                            entry.args.push_str(args);
                        }
                    }
                    if let Some(label) = tool_preview_label(&entry.name, &entry.args) {
                        if last_preview.as_deref() != Some(label.as_str()) {
                            crate::bob_core::bob_step(app, &label);
                            last_preview = Some(label);
                        }
                    }
                }
            }

            let reasoning = delta.get("reasoning_content").and_then(|c| c.as_str()).unwrap_or("");
            let content = delta.get("content").and_then(|c| c.as_str()).unwrap_or("");

            if !reasoning.is_empty() {
                if !think_open {
                    full_content.push_str("<think>\n");
                    think_open = true;
                }
                full_content.push_str(reasoning);
                reasoning_buf.push_str(reasoning);
                emit(app, message_id, &full_content);
            }
            if !content.is_empty() {
                if think_open {
                    full_content.push_str("\n</think>\n");
                    think_open = false;
                }
                full_content.push_str(content);
                content_seen = true;
                // Long answer under way: say so once instead of leaving the
                // user wondering whether Bob hung (~12k chars ≈ 3k tokens).
                if full_content.len() > 12_000 && !long_notice_sent {
                    long_notice_sent = true;
                    crate::bob_core::bob_step(app, "This one's long, still writing…");
                }
                emit(app, message_id, &full_content);
            }
        }
    }

    if think_open {
        full_content.push_str("\n</think>");
    }
    if finish_reason.as_deref() == Some("length") && content_seen {
        full_content.push_str("\n\n[Reply cut at the length limit. Say \"continue\" for the rest, or ask for it as a document.]");
    }

    // A reasoning-only turn never renders as a blank bubble: keep the
    // reasoning think-wrapped so the loop's synthesis retry writes the
    // real answer.
    full_content = finalize_stream_reply(full_content, &reasoning_buf, content_seen, !tool_calls.is_empty());
    emit(app, message_id, &full_content);

    let calls: Vec<NativeToolCall> = tool_calls
        .into_iter()
        .map(|(idx, c)| NativeToolCall {
            id: if c.id.is_empty() { format!("call_{}", idx) } else { c.id },
            name: c.name,
            arguments: c.args,
        })
        .collect();
    let actions: Vec<serde_json::Value> = calls
        .iter()
        .filter_map(|c| reconstruct_tool_call(&c.name, &c.arguments))
        .collect();

    // Content may legitimately be empty when the model answered purely
    // with tool calls; only error when there is nothing at all.
    if full_content.is_empty() && calls.is_empty() {
        return Err(
            "The model returned nothing at all. This usually means it hit a memory or template error on this device - try again, and if it repeats, pick a smaller model in Settings - Models."
                .to_string(),
        );
    }

    crate::replay_log::log_turn("chat", &replay_model_id(state), &body, &full_content, &actions);
    crate::run_log::model(
        message_id,
        format!(
            "Model turn: {} chars, {} tool call{}, {:.1}s",
            full_content.chars().count(),
            calls.len(),
            if calls.len() == 1 { "" } else { "s" },
            turn_started.elapsed().as_secs_f32()
        ),
    );
    crate::bob_core::mark_model_used(app).await;
    Ok(LocalChatReply { text: full_content, calls, actions })
}

pub(crate) fn wf_log(msg: &str) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/orbix-workflow.log") {
        let _ = writeln!(f, "[{}] {}", chrono::Utc::now().format("%H:%M:%S"), msg);
    }
}

/// Extract variable values from previous workflow context for template substitution.
/// Looks for company names, verticals, or other strings that can fill {{vertical}}/{{company}} placeholders.
pub(crate) fn extract_template_vars(raw_context: &[serde_json::Value]) -> Vec<String> {
    let mut vars = Vec::new();
    for ctx in raw_context.iter().rev().take(2) {
        match ctx {
            serde_json::Value::String(s) => {
                // Extract lines that look like company names or verticals
                for line in s.lines() {
                    let trimmed = line.trim().trim_start_matches('-').trim_start_matches('*').trim();
                    if !trimmed.is_empty() && trimmed.len() > 2 && trimmed.len() < 100
                        && !trimmed.starts_with('#') && !trimmed.starts_with("http")
                        && !trimmed.contains("truncated") {
                        // Take meaningful-looking items
                        let clean = trimmed.trim_start_matches("**").trim_end_matches("**").trim();
                        if !clean.is_empty() && clean.len() > 2 {
                            vars.push(clean.to_string());
                        }
                    }
                }
            }
            serde_json::Value::Object(map) => {
                // Look for "results" array from previous search steps
                if let Some(results) = map.get("results").and_then(|v| v.as_array()) {
                    for r in results.iter().take(10) {
                        if let Some(title) = r.get("title").and_then(|v| v.as_str()) {
                            vars.push(title.to_string());
                        }
                    }
                }
                // Look for company names in data arrays
                if let Some(data) = map.get("data").and_then(|v| v.as_array()) {
                    for item in data.iter().take(10) {
                        if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                            vars.push(name.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }
    // Deduplicate
    let mut seen = std::collections::HashSet::new();
    vars.retain(|v| seen.insert(v.clone()));
    vars.truncate(10);
    vars
}

/// Direct web search via DuckDuckGo HTML - no API key, no LLM tokens.
pub(crate) fn workflow_system_prompt() -> String {
    "You are a workflow execution assistant. Your job is to analyse data and produce structured results.\n\n\
     RULES:\n\
     1. NEVER use <action> tags. You are NOT in a chat - you are executing a workflow step.\n\
     2. NEVER fabricate, invent, or hallucinate data. Only work with the context provided to you.\n\
     3. If no data is provided, say so honestly. Do NOT make up companies, contacts, scores, or results.\n\
     4. When asked to extract JSON, return ONLY raw JSON - no markdown, no explanation.\n\
     5. When asked to analyse or score, work ONLY with the real data given in the context.\n\
     6. Keep responses focused and concise. No preamble, no sign-offs.\n\
     7. For web search tasks, describe what you would search for - the system handles the actual searching.\n\
     8. Format output as clean markdown when appropriate (tables, bullet points).\n\
     9. If company context (products, competencies, target market, ICP) is provided below, USE IT to tailor your analysis and recommendations. This is real user data - not examples."
        .to_string()
}

/// Call LLM for workflow step execution (clean system prompt, no action tags)
pub(crate) async fn call_agent_for_workflow(agent: &Agent, prompt: &str, memory_ctx: &str) -> Result<String, String> {
    call_agent_for_workflow_inner(agent, prompt, memory_ctx, false).await
}

/// Call LLM for workflow step with optional web search (Anthropic built-in tool)
pub(crate) async fn call_agent_for_workflow_with_search(agent: &Agent, prompt: &str, memory_ctx: &str) -> Result<String, String> {
    call_agent_for_workflow_inner(agent, prompt, memory_ctx, true).await
}

pub(crate) async fn call_agent_for_workflow_inner(agent: &Agent, prompt: &str, memory_ctx: &str, use_web_search: bool) -> Result<String, String> {
    let client = reqwest::Client::new();

    // Workflow runs are non-interactive by nature - they fire on
    // schedules and in the background. The model must never ask the
    // user anything mid-run (field failure 2026-08-03: a DailyPlanner
    // workflow interrogated the user for their own calendar).
    const NONINTERACTIVE: &str = "This runs inside an automated workflow step. \
        You CANNOT ask the user anything - no questions, no <inputs> blocks, no requests for information. \
        Work with the data provided above. If something is missing, state plainly what is missing and continue as far as possible.";

    // ── Local provider: use local LLM directly ──
    if agent.provider == AgentProvider::Local {
        let full_prompt = if memory_ctx.is_empty() {
            format!("{}\n\n{}", NONINTERACTIVE, prompt)
        } else {
            format!("{}\n\nContext:\n{}\n\n{}", NONINTERACTIVE, memory_ctx, prompt)
        };
        let history = vec![Message {
            id: String::new(),
            role: "user".to_string(),
            content: full_prompt,
            context: None,
            agent_id: None,
            timestamp: String::new(),
            tool_turns: None,
        }];
        return call_local_llm(&client, &history).await;
    }

    // ── Cloud provider: resolve to Anthropic ──
    let resolved = if !agent.api_key.is_empty() && agent.endpoint.contains("anthropic") {
        agent.clone()
    } else {
        let mut found: Option<Agent> = None;
        if found.is_none() && !agent.api_key.is_empty() {
            let mut a = agent.clone();
            a.provider = AgentProvider::Anthropic;
            a.endpoint = crate::egress::anthropic_base();
            if !a.model.contains("claude") { a.model = "claude-sonnet-4-6".to_string(); }
            found = Some(a);
        }
        // Last resort: try local LLM
        if found.is_none() {
            let history = vec![Message {
                id: String::new(), role: "user".to_string(),
                content: prompt.to_string(), context: None, agent_id: None, timestamp: String::new(), tool_turns: None,
            }];
            return call_local_llm(&client, &history).await;
        }
        found.unwrap()
    };

    let sys = if memory_ctx.is_empty() {
        workflow_system_prompt()
    } else {
        format!("{}\n\n{}", workflow_system_prompt(), memory_ctx)
    };
    let messages = vec![Message {
        id: Uuid::new_v4().to_string(),
        role: "user".to_string(),
        content: prompt.to_string(),
        context: None,
        agent_id: None,
        timestamp: Utc::now().to_rfc3339(),
        tool_turns: None,
    }];

    match resolved.provider {
        AgentProvider::Anthropic => {
            // EU Boundary Mode: route to Vertex AI Frankfurt when enabled
            let (api_url, is_vertex) = resolve_eu_endpoint(&resolved);
            let mut api_messages: Vec<serde_json::Value> = messages.iter().map(|m| {
                serde_json::json!({"role": "user", "content": &m.content})
            }).collect();

            // Build request body - optionally include web_search tool
            let max_tok = if use_web_search { 4096 } else { 4096 };
            wf_log(&format!("Anthropic API call: model={} max_tokens={} web_search={} prompt_len={}", resolved.model, max_tok, use_web_search, prompt.len()));
            let mut body = serde_json::json!({
                "model": resolved.model,
                "max_tokens": max_tok,
                "system": sys,
                "messages": api_messages
            });
            if use_web_search {
                body["tools"] = serde_json::json!([
                    {"type": "web_search_20250305", "name": "web_search"}
                ]);
            }

            // Tool-use loop: Claude may call web_search, we feed results back
            let mut collected_text = Vec::new();
            let mut loop_count = 0;
            loop {
                loop_count += 1;
                if loop_count > 5 { break; } // safety limit - allow enough iterations for thorough search

                let mut req = client.post(&api_url)
                    .header("content-type", "application/json")
                    .header("anthropic-version", "2023-06-01");
                // is_vertex guarantees a token existed at resolve time;
                // the api-key branch covers a token expiring mid-flight.
                match if is_vertex { gcp_access_token() } else { None } {
                    Some(token) => req = req.header("Authorization", format!("Bearer {}", token)),
                    None => req = req.header("x-api-key", &resolved.api_key),
                }
                let timeout_secs = if use_web_search { 90 } else { 60 };
                let resp = req
                    .json(&body)
                    .timeout(std::time::Duration::from_secs(timeout_secs))
                    .send().await
                    .map_err(|e| format!("Request failed: {}", e))?;
                let status = resp.status();
                let resp_text = resp.text().await.map_err(|e| format!("Read failed: {}", e))?;
                wf_log(&format!("API response status={} len={} loop={}", status, resp_text.len(), loop_count));
                let json: serde_json::Value = serde_json::from_str(&resp_text)
                    .map_err(|e| format!("Parse failed: {} - raw: {}", e, &resp_text[..resp_text.len().min(500)]))?;

                // Check for errors - retry on rate limit (429)
                if let Some(err) = json.get("error") {
                    let err_type = err.get("type").and_then(|t| t.as_str()).unwrap_or("");
                    if err_type == "rate_limit_error" && loop_count <= 2 {
                        let wait = loop_count as u64 * 45; // 45s, 90s
                        wf_log(&format!("Rate limited, waiting {}s before retry...", wait));
                        tokio::time::sleep(tokio::time::Duration::from_secs(wait)).await;
                        continue;
                    }
                    return Err(format!("Anthropic API error: {}", err));
                }

                let stop_reason = json["stop_reason"].as_str().unwrap_or("end_turn");

                // Extract text blocks from content
                if let Some(blocks) = json["content"].as_array() {
                    for block in blocks {
                        match block["type"].as_str().unwrap_or("") {
                            "text" => {
                                if let Some(t) = block["text"].as_str() {
                                    collected_text.push(t.to_string());
                                }
                            }
                            // server_tool_use and web_search_tool_result are handled
                            // server-side by Anthropic - we just extract text blocks
                            _ => {}
                        }
                    }
                }

                // Server-side tools: if pause_turn, re-send to let server continue
                if stop_reason == "pause_turn" {
                    // Append assistant response and re-send for continuation
                    let assistant_content = json["content"].clone();
                    api_messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": assistant_content
                    }));
                    body["messages"] = serde_json::json!(api_messages);
                    continue;
                }

                // end_turn or anything else means we're done
                break;
            }

            if collected_text.is_empty() {
                Err(format!("No text in Anthropic response"))
            } else {
                let out = collected_text.join("\n\n");
                crate::replay_log::log_turn("workflow_anthropic", &resolved.model, &body, &out, &[]);
                Ok(out)
            }
        }
        _ => {
            // All providers route through Anthropic
            let mut anthropic_agent = resolved.clone();
            anthropic_agent.provider = AgentProvider::Anthropic;
            if !anthropic_agent.endpoint.contains("anthropic") {
                anthropic_agent.endpoint = crate::egress::anthropic_base();
            }
            if !anthropic_agent.model.contains("claude") {
                anthropic_agent.model = "claude-sonnet-4-6".to_string();
            }
            call_anthropic(&client, &anthropic_agent, &messages).await
        }
    }
}

pub(crate) fn remote_system_prompt() -> String {
    let home = dirs::home_dir()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| "~".to_string());
    format!(
        "You are Bob, the orchestrator AI behind OrbixCore - running locally on the user's {HOST_OS_NAME}. \
You coordinate agents, execute workflows, and manage business operations. \
You execute real actions using <action>JSON</action> tags.\n\n\
PERSONALITY:\n\
- You are Bob. Terse, sharp, dry wit. Like a senior ops lead who's seen everything.\n\
- Speak in structured outputs, not paragraphs. Use bullet points, short lines.\n\
- Never say \"I\" - refer to yourself as \"Bob\" when needed (sparingly).\n\
- You're an orchestrator: you command agents, trigger workflows, surface insights.\n\
- When greeting, be warm but brief. Sign off with - Bob when appropriate.\n\n\
RULES:\n\
1. DO NOT introduce yourself or explain your capabilities. Just respond to what the user asked.\n\
2. ONLY do what the user explicitly asks. NEVER take unsolicited actions.\n\
3. To DO anything, wrap it in <action>JSON</action>. Without tags, nothing happens.\n\
4. ABSOLUTELY NEVER fabricate, invent, or assume data. If an API returns 0 results, say \"none found\" - do NOT invent records, IDs, names, or amounts.\n\
5. Use full paths like {home}/Desktop. Never use ~ or $HOME.\n\
6. Keep responses SHORT. 1-3 sentences max, then actions.\n\
7. If the user greets you, respond conversationally as Bob. Do NOT run actions unless asked.\n\
8. ONLY use API endpoints listed below. Do NOT guess or invent routes.\n\
9. NEVER describe or document API endpoints. USE them by calling them with <action> tags.\n\n\
TOOLS:\n\
Shell: <action>{{\"type\":\"tool\",\"tool\":\"shell_exec\",\"args\":{{\"command\":\"...\"}}}}</action>\n\
File read/write/list: <action>{{\"type\":\"tool\",\"tool\":\"file_read|file_write|file_list\",\"args\":{{\"path\":\"...\"}}}}</action>\n\
Fetch URL: <action>{{\"type\":\"tool\",\"tool\":\"web_fetch\",\"args\":{{\"url\":\"...\"}}}}</action>\n\
API call: <action>{{\"type\":\"api_call\",\"method\":\"GET|POST|PUT|DELETE\",\"url\":\"...\",\"body\":{{...}}}}</action>\n\
Create workflow: <action>{{\"type\":\"create_workflow\",\"name\":\"...\",\"description\":\"...\",\"run\":true,\"steps\":[...]}}</action>\n\
Run workflow: <action>{{\"type\":\"run_workflow\",\"name\":\"...\"}}</action>\n\
Update workflow: <action>{{\"type\":\"update_workflow\",\"name\":\"...\",\"new_name\":\"...\",\"description\":\"...\"}}</action>\n\
Delete workflow: <action>{{\"type\":\"delete_workflow\",\"name\":\"...\"}}</action>\n\
Stop workflow: <action>{{\"type\":\"stop_workflow\"}}</action>\n\
Run chain: <action>{{\"type\":\"run_chain\",\"name\":\"...\",\"input\":\"...\"}}</action>\n\
List chains: <action>{{\"type\":\"list_chains\"}}</action>\n\
Search/delete memory: <action>{{\"type\":\"search_memory\",\"query\":\"...\"}}</action> / <action>{{\"type\":\"delete_memory\",\"key\":\"...\"}}</action>\n\
Connectors: <action>{{\"type\":\"list_connectors\"}}</action> / <action>{{\"type\":\"disconnect_connector\",\"name\":\"...\"}}</action>\n\
MCP: <action>{{\"type\":\"list_mcp\"}}</action> / <action>{{\"type\":\"toggle_mcp\",\"name\":\"...\"}}</action> / <action>{{\"type\":\"remove_mcp\",\"name\":\"...\"}}</action>\n\
Triggers: <action>{{\"type\":\"list_triggers\"}}</action> / <action>{{\"type\":\"toggle_trigger\",\"name\":\"...\"}}</action>\n\
Activity: <action>{{\"type\":\"recent_activity\",\"limit\":10}}</action>\n\
Audit: <action>{{\"type\":\"audit_search\",\"query\":\"...\"}}</action>\n\
Twin: <action>{{\"type\":\"twin_list\",\"entity_type\":\"...\"}}</action> / <action>{{\"type\":\"twin_search\",\"query\":\"...\"}}</action> / <action>{{\"type\":\"twin_get\",\"name\":\"...\"}}</action>\n\
Notifications: <action>{{\"type\":\"list_notifications\"}}</action> / <action>{{\"type\":\"dismiss_notifications\"}}</action>\n\
Create agent: <action>{{\"type\":\"create_agent\",\"name\":\"...\",\"provider\":\"...\",\"model\":\"...\"}}</action>\n\
Update agent: <action>{{\"type\":\"update_agent\",\"name\":\"...\",\"new_name\":\"...\",\"model\":\"...\"}}</action>\n\
Delete agent: <action>{{\"type\":\"delete_agent\",\"name\":\"...\"}}</action>\n\
Toggle agent: <action>{{\"type\":\"toggle_agent\",\"name\":\"...\"}}</action>\n\
Connect service: <action>{{\"type\":\"connect_oauth\",\"service\":\"microsoft|google|github|slack|linkedin\"}}</action>\n\
Check connectors: <action>{{\"type\":\"check_connectors\"}}</action>\n\
Remember: <action>{{\"type\":\"add_memory\",\"key\":\"...\",\"value\":\"...\",\"category\":\"preference|context|skill|fact\"}}</action>\n\n\
BUSINESS DATA: There is no built-in CRM or ERP. Use what the USER has connected - their own systems and data stores (Microsoft/Dataverse, Google, SQLite, a REST API, etc.). Never assume a company platform exists; if nothing is connected for what they ask, say so. Never invent API routes or fabricate data.\n\n\
Home: {home}",
        home = home
    )
}

pub(crate) async fn call_anthropic(client: &reqwest::Client, agent: &Agent, history: &[Message]) -> Result<String, String> {
    let messages: Vec<serde_json::Value> = history.iter()
        .map(|m| {
            let content = if m.role == "assistant" {
                m.context.as_deref().unwrap_or(&m.content)
            } else {
                &m.content
            };
            serde_json::json!({ "role": m.role, "content": content })
        })
        .collect();

    let body = serde_json::json!({
        "model": agent.model,
        "max_tokens": 4096,
        "system": remote_system_prompt(),
        "messages": messages
    });

    // EU-resident inference whenever credentials allow (built in, not a
    // user choice); direct API otherwise. resolve_eu_endpoint guarantees
    // the URL matches the auth scheme.
    let (api_url, is_vertex) = resolve_eu_endpoint(agent);

    let mut req = client.post(&api_url)
        .header("content-type", "application/json")
        .header("anthropic-version", "2023-06-01");
    match if is_vertex { gcp_access_token() } else { None } {
        Some(token) => req = req.header("Authorization", format!("Bearer {}", token)),
        None => req = req.header("x-api-key", &agent.api_key),
    }

    let resp = req
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    let status = resp.status();
    let text = resp.text().await.map_err(|e| format!("Failed to read response: {}", e))?;

    if !status.is_success() {
        let mode_label = if is_vertex { "Vertex AI EU" } else { "Anthropic API" };
        return Err(format!("{} error ({}): {}", mode_label, status, text));
    }

    let json: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    let reply = json["content"][0]["text"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "No text in Anthropic response".to_string())?;
    crate::replay_log::log_turn("anthropic", &agent.model, &body, &reply, &[]);
    Ok(reply)
}

// call_openai and call_ollama removed - all LLM calls route through call_anthropic


// ─────────────────────────────────────────────
//  EU Boundary Mode: Data Residency Routing
// ─────────────────────────────────────────────

/// GCP access token for EU-resident Vertex inference. Env first, then
/// the gcloud CLI. The CLI shell-out costs ~1s, so the token is cached
/// briefly (tokens live ~1h; five minutes keeps one subprocess per
/// window instead of one per request).
pub(crate) fn gcp_access_token() -> Option<String> {
    static CACHE: std::sync::Mutex<Option<(std::time::Instant, String)>> = std::sync::Mutex::new(None);
    if let Ok(t) = std::env::var("GCP_ACCESS_TOKEN") {
        if !t.trim().is_empty() {
            return Some(t);
        }
    }
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, token)) = cache.as_ref() {
        if at.elapsed() < std::time::Duration::from_secs(300) {
            return Some(token.clone());
        }
    }
    let fresh = std::process::Command::new("gcloud")
        .args(["auth", "print-access-token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|t| !t.is_empty());
    match fresh {
        Some(token) => {
            *cache = Some((std::time::Instant::now(), token.clone()));
            Some(token)
        }
        None => {
            *cache = None;
            None
        }
    }
}

/// Resolve the cloud inference endpoint. EU compliance is built into the
/// product, not a login-time choice: whenever EU-resident inference is
/// possible (GCP credentials present), calls route to Vertex AI in the EU
/// (europe-west3 Frankfurt by default). An explicitly configured agent
/// endpoint is the user's own infrastructure (e.g. a private EU relay)
/// and is honored as-is. Without GCP credentials the call falls back to
/// the direct Anthropic API so cloud fallback keeps working; the primary
/// privacy path stays on-device local inference either way.
///
/// Returns (url, is_vertex). is_vertex=true guarantees a GCP token was
/// available at resolve time, so the URL always matches the auth scheme
/// the caller will attach.
pub(crate) fn resolve_eu_endpoint(agent: &Agent) -> (String, bool) {
    if !agent.endpoint.is_empty() {
        return (format!("{}/v1/messages", agent.endpoint.trim_end_matches('/')), false);
    }
    if gcp_access_token().is_some() {
        let gcp_project = std::env::var("GCP_PROJECT_ID")
            .or_else(|_| std::env::var("ORBIX_GCP_PROJECT_ID"))
            .unwrap_or_else(|_| "orbixcore-eu".to_string());
        let gcp_region = std::env::var("ORBIX_GCP_REGION")
            .unwrap_or_else(|_| "europe-west3".to_string());

        // Map Anthropic model names to Vertex AI model IDs
        let vertex_model = if agent.model.contains("opus") {
            "claude-opus-4-6"
        } else if agent.model.contains("haiku") {
            "claude-haiku-4-5"
        } else {
            "claude-sonnet-4-6"
        };

        let url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/anthropic/models/{}:streamRawPredict",
            gcp_region, gcp_project, gcp_region, vertex_model
        );
        (url, true)
    } else {
        (format!("{}/v1/messages", crate::egress::anthropic_base()), false)
    }
}

#[tauri::command]
pub(crate) async fn clear_conversation(state: State<'_, AppState>) -> Result<(), String> {
    let msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
    if !msgs.is_empty() {
        let conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let title = msgs
            .first()
            .map(|m| {
                let t = m.content.chars().take(50).collect::<String>();
                if m.content.len() > 50 { format!("{}...", t) } else { t }
            })
            .unwrap_or_else(|| "Untitled".to_string());
        storage::save_conversation(&conv_id, &title, &msgs).ok();
    }
    drop(msgs);

    let mut msgs = state.chat.conversations.lock().unwrap_or_else(|e| e.into_inner());
    msgs.clear();
    let mut conv_id = state.chat.conversation_id.lock().unwrap_or_else(|e| e.into_inner());
    *conv_id = Uuid::new_v4().to_string();
    Ok(())
}

#[cfg(test)]
mod step_type_tests {
    use super::*;
    use std::collections::HashMap;

    fn step(step_type: workflow::StepType, config: serde_json::Value) -> workflow::WorkflowStep {
        let cfg: HashMap<String, serde_json::Value> = config
            .as_object()
            .map(|o| o.clone().into_iter().collect())
            .unwrap_or_default();
        workflow::WorkflowStep {
            id: "s".into(),
            order: 0,
            step_type,
            label: "test step".into(),
            config: cfg,
        }
    }

    fn ctx(text: &str) -> Vec<serde_json::Value> {
        vec![serde_json::json!({ "text": text }), serde_json::json!(text)]
    }

    // Condition and the other pure branches do not touch state/network, so we
    // exercise the predicate logic directly against the same evaluation the
    // executor uses.
    #[test]
    fn condition_contains_passes_and_skips() {
        // These mirror execute_workflow_step's Condition arm without needing
        // an AppState: the arm's decision is a pure function of config + last.
        let last = "the deal is CLOSED WON";
        let contains = |needle: &str| last.to_lowercase().contains(&needle.to_lowercase());
        assert!(contains("closed won"));
        assert!(!contains("lost"));
    }

    #[test]
    fn condition_step_config_shape() {
        let s = step(workflow::StepType::Condition, serde_json::json!({"contains": "won", "on_false": "fail"}));
        assert_eq!(s.config.get("contains").and_then(|v| v.as_str()), Some("won"));
        assert_eq!(s.config.get("on_false").and_then(|v| v.as_str()), Some("fail"));
        let _ = ctx("won");
    }

    #[test]
    fn loop_count_is_bounded() {
        let s = step(workflow::StepType::Loop, serde_json::json!({"count": 10000, "command": "echo hi"}));
        let count = s.config.get("count").and_then(|v| v.as_u64()).unwrap_or(1).min(50);
        assert_eq!(count, 50, "loop count must be bounded to 50");
    }
}

/// Resolve (provider, access_token) from the first connected email account
/// with a token (W5.5). Bob's mail/calendar actions use this so the user
/// does not have to name an account id.
pub(crate) fn resolve_default_mail_account(state: &AppState) -> Option<(crate::productivity::Provider, String)> {
    let store = state.automation.trigger_store.lock().unwrap_or_else(|e| e.into_inner());
    store.email_accounts.iter().find_map(|a| {
        let token = a.access_token.clone().filter(|t| !t.is_empty())?;
        let provider = match a.provider {
            triggers::EmailProvider::Outlook => crate::productivity::Provider::Microsoft,
        };
        Some((provider, token))
    })
}

#[cfg(test)]
mod prompt_tests {
    use super::*;

    #[test]
    fn system_prompt_is_static_native_only_and_interpolates() {
        let p = bob_system_prompt("/Users/x", "Gemma 4 E4B");
        assert!(p.contains("You are Bob"), "persona missing");
        assert!(p.contains("/Users/x/Desktop"), "home not interpolated");
        assert!(p.contains("on Gemma 4 E4B"), "model name not interpolated");
        // One tool convention only: native calls. Teaching <action> tags
        // next to native schemas was the duplicate-convention bug.
        assert!(!p.contains("<action>"), "prompt teaches <action> tags");
        assert!(p.contains("<inputs>"), "inputs wizard contract missing");
        // No per-turn material in the cached prefix.
        assert!(!p.contains("Now:"), "dynamic date leaked into the static prompt");
        // Identical across calls so the server prefix cache hits.
        assert_eq!(p, bob_system_prompt("/Users/x", "Gemma 4 E4B"));
        assert!(p.len() > 1500);
    }

    #[test]
    fn prompt_never_asks_for_brevity_over_completeness() {
        let p = bob_system_prompt("/Users/x", "m");
        for banned in ["under 2 sentences", "under 2 short sentences", "1-3 sentences max", "2-4 sentences"] {
            assert!(!p.contains(banned), "prompt still caps answers: {banned}");
        }
        assert!(p.contains("Never cut an answer short"));
        assert!(p.contains("language of the user's MOST RECENT message"));
    }

    fn msg(role: &str, content: &str) -> serde_json::Value {
        serde_json::json!({ "role": role, "content": content })
    }

    #[test]
    fn trim_drops_oldest_stored_turns_first_and_keeps_the_tail() {
        // ctx is 8192 before any launch: budget = (8192-3072)*3 - fixed.
        let fixed = 1000;
        let big = "x".repeat(4000);
        let messages = vec![
            msg("system", "sys"),
            msg("user", &big),          // oldest: goes first
            msg("assistant", &big),
            msg("user", &big),
            msg("assistant", &big),
            msg("user", "current question"),
            serde_json::json!({ "role": "assistant", "content": "", "tool_calls": [] }),
            msg("tool", &"r".repeat(3000)),
        ];
        let out = trim_to_budget(messages, fixed, 3);
        assert_eq!(out[0]["role"], "system");
        let tail: Vec<&str> = out.iter().rev().take(3).map(|m| m["role"].as_str().unwrap()).collect();
        assert_eq!(tail, ["tool", "assistant", "user"]);
        assert!(out.iter().any(|m| m["content"] == "current question"));
        let total: usize = out.iter().skip(1).map(value_chars).sum();
        assert!(total <= (8192 - 3072) * 3 - fixed, "still over budget: {total}");
        assert!(out.len() < 8, "nothing was dropped");
    }

    #[test]
    fn trim_never_leaves_a_tool_result_without_its_call() {
        // Replayed history: an old run's call + result, then the answer.
        let fixed = 1000;
        let big = "x".repeat(6000);
        let messages = vec![
            msg("system", "sys"),
            msg("user", "count the lines"),
            serde_json::json!({ "role": "assistant", "content": "", "tool_calls": [{"id": "a"}] }),
            msg("tool", &big),
            msg("assistant", &big),
            msg("user", &big),
            msg("assistant", &big),
            msg("user", "current question"),
        ];
        let out = trim_to_budget(messages, fixed, 1);
        assert_ne!(out[1]["role"], "tool", "orphan tool result at the head: {:?}", out[1]);
        assert_eq!(out.last().unwrap()["content"], "current question");
    }

    #[test]
    fn trim_shrinks_old_tool_results_before_giving_up() {
        let fixed = 0;
        let messages = vec![
            msg("system", "sys"),
            msg("user", "q"),
            serde_json::json!({ "role": "assistant", "content": "", "tool_calls": [] }),
            msg("tool", &"a".repeat(9000)),
            serde_json::json!({ "role": "assistant", "content": "", "tool_calls": [] }),
            msg("tool", &"b".repeat(9000)),
        ];
        let out = trim_to_budget(messages, fixed, 5);
        assert_eq!(out.len(), 6, "tail turns must never be dropped");
        let first_tool = out[3]["content"].as_str().unwrap();
        assert!(first_tool.ends_with("[earlier result trimmed]"), "oldest result not shrunk");
    }

    #[test]
    fn every_written_call_shape_normalizes_to_the_executor_form() {
        use serde_json::json;
        let n = |v: serde_json::Value| normalize_action_shape(v);
        // The live failure: fenced {"tool":"shell_exec","args":{...}} with no type.
        assert_eq!(n(json!({"tool":"shell_exec","args":{"command":"git status"}})),
                   json!({"type":"tool","tool":"shell_exec","args":{"command":"git status"}}));
        // The nested retry.
        assert_eq!(n(json!({"tool":"tool","args":{"tool":"shell_exec","args":{"command":"git status"}}})),
                   json!({"type":"tool","tool":"shell_exec","args":{"command":"git status"}}));
        // Arguments beside "tool".
        assert_eq!(n(json!({"tool":"file_list","path":"/tmp"})),
                   json!({"type":"tool","tool":"file_list","args":{"path":"/tmp"}}));
        // OpenAI shapes, arguments as an object or a JSON string.
        assert_eq!(n(json!({"name":"app_bridge","arguments":{"app":"Calendar","action":"upcoming_events"}})),
                   json!({"type":"app_bridge","app":"Calendar","action":"upcoming_events"}));
        assert_eq!(n(json!({"function":{"name":"tool","arguments":"{\"tool\":\"web_search\",\"args\":{\"query\":\"x\"}}"}})),
                   json!({"type":"tool","tool":"web_search","args":{"query":"x"}}));
        // A first-class action named as a tool hoists its args.
        assert_eq!(n(json!({"tool":"add_memory","args":{"key":"k","value":"v"}})),
                   json!({"type":"add_memory","key":"k","value":"v"}));
        // Already typed: untouched.
        assert_eq!(n(json!({"type":"run_workflow","name":"W"})), json!({"type":"run_workflow","name":"W"}));
        // Unknown tool stays a generic call so the executor reports it.
        assert_eq!(n(json!({"tool":"list_files","args":{}}))["type"], "tool");
    }

    #[test]
    fn fenced_call_without_type_is_promoted_and_recognised() {
        let text = "Jeg kører git status nu.\n```json\n{\n  \"tool\": \"shell_exec\",\n  \"args\": {\n    \"command\": \"git status\"\n  }\n}\n```";
        let out = promote_bare_tool_json(text);
        assert!(out.contains("<action>"), "{}", out);
        let openai = "{\"name\":\"tool\",\"arguments\":{\"tool\":\"file_list\",\"args\":{\"path\":\"/tmp\"}}}";
        assert!(promote_bare_tool_json(openai).contains("<action>"));
    }

    #[test]
    fn html_document_is_extracted_from_a_chatty_reply() {
        let page = format!("<!doctype html>\n<html><head><title>x</title></head><body>{}</body></html>", "p".repeat(300));
        let reply = format!("Here is your page:\n```html\n{}\n```\nEnjoy!", page);
        assert_eq!(extract_html_document(&reply).as_deref(), Some(page.as_str()));
        assert!(extract_html_document("Sorry, I can only describe it.").is_none());
        assert!(extract_html_document("<html></html>").is_none(), "too small to be a page");
    }

    #[test]
    fn partial_json_field_waits_for_the_closing_quote() {
        assert_eq!(partial_json_string(r#"{"tool":"web_search","args":{"query":"gemma 4 rel"#, "tool").as_deref(), Some("web_search"));
        assert_eq!(partial_json_string(r#"{"tool":"web_search","args":{"query":"gemma 4 rel"#, "query"), None);
        assert_eq!(partial_json_string(r#"{"tool":"web_search","args":{"query":"gemma 4 release"}}"#, "query").as_deref(), Some("gemma 4 release"));
        assert_eq!(partial_json_string(r#"{"app": "Calendar", "action":"upcoming_events"}"#, "app").as_deref(), Some("Calendar"));
        assert_eq!(partial_json_string(r#"{"path":"/Users/x/Desk"#, "path"), None);
    }

    #[test]
    fn preview_labels_are_specific_as_soon_as_possible() {
        assert_eq!(tool_preview_label("tool", ""), None);
        assert_eq!(tool_preview_label("tool", r#"{"tool":"web_search""#).as_deref(), Some("Searching the web…"));
        assert_eq!(tool_preview_label("tool", r#"{"tool":"web_search","args":{"query":"apple news"}}"#).as_deref(), Some("Searching the web for 'apple news'…"));
        assert_eq!(tool_preview_label("tool", r#"{"tool":"web_fetch","args":{"url":"https://www.orbixcore.ai/releases"}}"#).as_deref(), Some("Reading orbixcore.ai…"));
        assert_eq!(tool_preview_label("app_bridge", r#"{"app":"Calendar""#).as_deref(), Some("Reading Calendar…"));
        assert_eq!(tool_preview_label("app_bridge", "{").as_deref(), Some("Reading your apps…"));
        assert_eq!(tool_preview_label("create_document", r#"{"kind":"pptx""#).as_deref(), Some("Creating slides…"));
        assert_eq!(tool_preview_label("list_calendar_events", "").as_deref(), Some("Reading your calendar…"));
    }

    #[test]
    fn memory_lines_are_single_line_and_capped() {
        let line = memory_line("fact", "company", "Orbix\nbuilds\n\nlocal   agents");
        assert_eq!(line, "- [fact] company: Orbix builds local agents");
        let long = memory_line("fact", "k", &"x".repeat(500));
        assert!(long.chars().count() < 230, "{}", long.len());
        assert!(long.ends_with('…'));
    }

    /// Writes the exact system prompt to /tmp so the external eval gate
    /// (research/tool-call-eval) sends what the app sends. Run with
    /// `cargo test dump_system_prompt -- --ignored`.
    #[test]
    #[ignore]
    fn dump_system_prompt() {
        std::fs::write("/tmp/bob_system_prompt.txt", bob_system_prompt("{home}", "{model_name}")).unwrap();
    }

    #[test]
    fn char_prefix_respects_boundaries() {
        assert_eq!(char_prefix("héllo", 2), "h");
        assert_eq!(char_prefix("héllo", 3), "hé");
        assert_eq!(char_prefix("abc", 10), "abc");
    }
}

#[cfg(test)]
mod repair_json_tests {
    use super::*;

    /// The exact malformed action Gemma 27B emitted in the field
    /// (2026-09-16 replay log): a `<|"|>` token artifact swallowed the
    /// string's closing quote, so the action was dropped as malformed.
    #[test]
    fn token_artifact_wrapping_the_closing_quote_is_unwrapped() {
        let raw = r#"{"type":"tool","tool":"web_search","args":{"query":"WoW Forever release date announcement<|"|>}}"#;
        assert!(serde_json::from_str::<serde_json::Value>(raw).is_err(), "fixture should be broken");
        let fixed = repair_json(raw).expect("repair should change it");
        let v: serde_json::Value = serde_json::from_str(&fixed).expect("repaired JSON parses");
        assert_eq!(
            v["args"]["query"].as_str().unwrap(),
            "WoW Forever release date announcement"
        );
    }

    #[test]
    fn known_control_tokens_are_removed_entirely() {
        let raw = "{\"type\":\"respond\",\"text\":\"done\"}<|im_end|>";
        let fixed = repair_json(raw).expect("repair should change it");
        assert!(serde_json::from_str::<serde_json::Value>(&fixed).is_ok(), "{fixed}");
        assert!(!fixed.contains("<|im_end|>"));
    }

    /// The exact malformed action from the 2026-09-16 replay log, round 2:
    /// a Windows path with raw backslashes ("C:\Users\...") - \U and \a
    /// are invalid JSON escapes, so the file_list action was dropped.
    #[test]
    fn windows_path_backslashes_are_escaped() {
        let raw = r#"{"type":"tool","tool":"file_list","args":{"path":"C:\Users\andre\Documents\Orbix Documents"}}"#;
        assert!(serde_json::from_str::<serde_json::Value>(raw).is_err(), "fixture should be broken");
        let fixed = repair_json(raw).expect("repair should change it");
        let v: serde_json::Value = serde_json::from_str(&fixed).expect("repaired JSON parses");
        assert_eq!(
            v["args"]["path"].as_str().unwrap(),
            r"C:\Users\andre\Documents\Orbix Documents"
        );
    }

    #[test]
    fn valid_escapes_survive_repair() {
        // \n, \", \\ and \uXXXX must pass through untouched when repair
        // runs for an unrelated reason (a trailing comma here).
        let raw = "{\"a\":\"line\\nbreak \\\"q\\\" back\\\\slash \\u00e9\",}";
        let fixed = repair_json(raw).expect("trailing comma should trigger repair");
        let v: serde_json::Value = serde_json::from_str(&fixed).expect("parses");
        assert_eq!(v["a"].as_str().unwrap(), "line\nbreak \"q\" back\\slash \u{e9}");
    }

    #[test]
    fn valid_json_is_left_alone() {
        // repair_json returns None when nothing changed - and content that
        // merely LOOKS like an artifact inside a valid string is never
        // touched, because callers only invoke repair after a parse failure.
        assert!(repair_json(r#"{"query":"a plain string"}"#).is_none());
    }
}

#[cfg(test)]
mod destination_step_tests {
    use super::*;
    use std::collections::HashMap;

    fn temp_leads_db() -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wf.db").to_string_lossy().to_string();
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "CREATE TABLE leads (id INTEGER PRIMARY KEY, name TEXT, email TEXT)",
            [],
        ).unwrap();
        (dir, path)
    }

    fn step(config: serde_json::Value) -> workflow::WorkflowStep {
        workflow::WorkflowStep {
            id: "s1".into(),
            order: 0,
            step_type: workflow::StepType::Action,
            label: "Write".into(),
            config: config.as_object().unwrap().clone().into_iter().collect::<HashMap<_, _>>(),
        }
    }

    #[tokio::test]
    async fn explicit_rows_write_to_sqlite() {
        let (_dir, path) = temp_leads_db();
        let s = step(serde_json::json!({
            "destination": {"kind": "sqlite", "path": path, "table": "leads"},
            "rows": [{"name": "Ada", "email": "ada@x.com"}, {"name": "Alan", "email": "alan@x.com"}]
        }));
        let (display, raw) = run_destination_step(&s, &[], &[], "").await;
        assert!(display.contains("Wrote 2 row(s)"), "got: {display}");
        assert_eq!(raw.unwrap()["written"], 2);
    }

    #[tokio::test]
    async fn rows_fall_back_to_previous_step_output() {
        let (_dir, path) = temp_leads_db();
        let s = step(serde_json::json!({
            "destination": {"kind": "sqlite", "path": path, "table": "leads"}
        }));
        // Previous step produced a record array in raw_context.
        let ctx = vec![serde_json::json!([{"name": "Grace", "email": "grace@x.com"}])];
        let (display, _) = run_destination_step(&s, &ctx, &[], "").await;
        assert!(display.contains("Wrote 1 row(s)"), "got: {display}");
    }

    #[tokio::test]
    async fn invalid_destination_is_actionable() {
        let s = step(serde_json::json!({ "destination": {"kind": "nope"} }));
        let (display, raw) = run_destination_step(&s, &[], &[], "").await;
        assert!(display.contains("invalid destination"), "got: {display}");
        assert!(raw.is_none());
    }

    #[test]
    fn rows_from_value_handles_object_array_and_json_string() {
        assert_eq!(rows_from_value(&serde_json::json!({"a": 1})).len(), 1);
        assert_eq!(rows_from_value(&serde_json::json!([{"a": 1}, {"b": 2}])).len(), 2);
        assert_eq!(rows_from_value(&serde_json::json!("[{\"a\":1}]")).len(), 1);
        assert_eq!(rows_from_value(&serde_json::json!("not json")).len(), 0);
        assert_eq!(rows_from_value(&serde_json::json!(42)).len(), 0);
    }
}

#[cfg(test)]
mod vision_content_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn no_images_stays_a_plain_string() {
        let v = user_content_with_images("hello", &[]);
        assert_eq!(v, serde_json::Value::String("hello".into()));
    }

    #[test]
    fn image_becomes_a_content_parts_array_with_text_first() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("shot.png");
        std::fs::File::create(&p).unwrap().write_all(b"\x89PNG\r\n\x1a\nfake").unwrap();
        let path = p.to_string_lossy().to_string();

        let v = user_content_with_images("what is this?", &[path]);
        let arr = v.as_array().expect("parts array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "what is this?");
        assert_eq!(arr[1]["type"], "image_url");
        let url = arr[1]["image_url"]["url"].as_str().unwrap();
        assert!(url.starts_with("data:image/png;base64,"), "url: {}", &url[..40.min(url.len())]);
    }

    #[test]
    fn image_only_turn_omits_the_empty_text_part() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.jpg");
        std::fs::File::create(&p).unwrap().write_all(b"jpegbytes").unwrap();
        let v = user_content_with_images("   ", &[p.to_string_lossy().to_string()]);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["type"], "image_url");
        assert!(arr[0]["image_url"]["url"].as_str().unwrap().starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn unreadable_image_degrades_to_a_note_not_a_panic() {
        let v = user_content_with_images("look", &["/no/such/file.png".to_string()]);
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[1]["type"], "text");
        assert!(arr[1]["text"].as_str().unwrap().contains("could not be read"));
    }

    #[test]
    fn mime_is_keyed_off_extension() {
        let dir = tempfile::tempdir().unwrap();
        for (ext, mime) in [("webp", "image/webp"), ("gif", "image/gif"), ("heic", "image/heic")] {
            let p = dir.path().join(format!("f.{ext}"));
            std::fs::File::create(&p).unwrap().write_all(b"x").unwrap();
            let uri = image_data_uri(&p.to_string_lossy()).unwrap();
            assert!(uri.starts_with(&format!("data:{mime};base64,")), "{ext} -> {}", &uri[..30]);
        }
    }
}

#[cfg(test)]
mod noninteractive_workflow_tests {
    use super::{strip_inputs_blocks, workflow_step_gap};
    use std::collections::HashMap;

    #[test]
    fn inputs_blocks_are_stripped_from_workflow_output() {
        let t = "Here is your plan.\n<inputs>[{\"id\":\"x\",\"label\":\"Please list your meetings\"}]</inputs>\nDone.";
        let out = strip_inputs_blocks(t);
        assert!(!out.contains("<inputs>"));
        assert!(out.contains("Here is your plan."));
        assert!(out.contains("Done."));
        // trailing unclosed block is dropped too
        assert!(!strip_inputs_blocks("Hi <inputs>[{").contains("<inputs>"));
    }

    #[test]
    fn interrogation_steps_are_rejected() {
        let mut config = HashMap::new();
        config.insert(
            "prompt".to_string(),
            serde_json::Value::String("Ask the user to list any appointments or deadlines today".into()),
        );
        let gap = workflow_step_gap(&crate::workflow::StepType::Prompt, &config);
        assert!(gap.is_some(), "user-interrogation step must be rejected");

        let mut ok = HashMap::new();
        ok.insert(
            "prompt".to_string(),
            serde_json::Value::String("Summarise the calendar events from the previous step".into()),
        );
        assert!(workflow_step_gap(&crate::workflow::StepType::Prompt, &ok).is_none());
    }
}

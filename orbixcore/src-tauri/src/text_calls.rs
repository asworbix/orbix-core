// ─────────────────────────────────────────────
//  Text calls - a tool call the model wrote as text
//
//  Measured 2026-10-21 on Gemma 4 E4B (llama-server b9969) with the real
//  per-turn context: for "what's in this folder?" and "git status here"
//  the model often decided to call a tool but wrote the call into the
//  reply text instead of emitting it natively:
//
//    file_list{path:<|"|>/Users/you/Projects/x<|"|>}      (its own
//        call syntax, missing the opening <|tool_call> token, so the
//        server never parsed it)
//    ```json [{"tool":"shell_exec","args":{"command":"wc -l a.txt"}}] ```
//
//  The user then saw the raw call as Bob's answer and nothing ran. When a
//  turn has no native calls and its text is NOTHING BUT well-formed calls
//  to real tools, those calls are what the model meant: recover them and
//  run them through the native path, so the next turn sees a proper
//  tool-call turn instead of learning the broken format from itself.
//  Prose that merely mentions a call is never executed.
// ─────────────────────────────────────────────

use crate::llm_engine::NativeToolCall;
use serde_json::{json, Map, Value};

const GEMMA_QUOTE: &str = "<|\"|>";

/// The calls `text` consists of, or empty when it is anything else.
pub fn recover(text: &str) -> Vec<NativeToolCall> {
    let body = strip_wrappers(text);
    if body.is_empty() {
        return Vec::new();
    }
    let parsed = if body.starts_with('{') || body.starts_with('[') {
        parse_json_calls(&body)
    } else {
        parse_gemma_calls(&body)
    };
    let Some(raw) = parsed else { return Vec::new() };
    let mut out = Vec::new();
    for (name, args) in raw {
        let Some((name, args)) = to_registry(&name, args) else { return Vec::new() };
        out.push(NativeToolCall {
            id: format!("text-{}", uuid::Uuid::new_v4().simple()),
            name,
            arguments: args.to_string(),
        });
    }
    out
}

/// Drop code fences, the model's call tokens, and a leading "call:".
fn strip_wrappers(text: &str) -> String {
    let mut s = crate::strip_think_blocks(text).trim().to_string();
    for tok in ["<|tool_call>", "<tool_call|>", "<|tool_call|>"] {
        s = s.replace(tok, "\n");
    }
    // A fence line is dropped when it holds only a language tag; a call
    // written on the fence line itself ("```shell_exec{...}") is kept.
    let lines: Vec<&str> = s
        .lines()
        .filter_map(|l| match l.trim_start().strip_prefix("```") {
            None => Some(l),
            Some(rest) if rest.trim().chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') => None,
            Some(rest) => Some(rest),
        })
        .collect();
    let joined = lines.join("\n");
    let mut t = joined.trim();
    if let Some(rest) = t.strip_prefix("call:") {
        t = rest.trim_start();
    }
    t.to_string()
}

/// A built-in (file_list, shell_exec, ...) is called through `tool`;
/// any other name must be a tool Bob actually has.
fn to_registry(name: &str, args: Value) -> Option<(String, Value)> {
    let args = if args.is_object() { args } else { json!({}) };
    if crate::tools::is_builtin_tool(name) {
        return Some(("tool".to_string(), json!({ "tool": name, "args": args })));
    }
    if name == "tool" {
        let inner = args.get("tool").and_then(|t| t.as_str())?;
        return crate::tools::is_builtin_tool(inner).then_some((name.to_string(), args));
    }
    crate::tool_schemas::is_known_tool(name).then(|| (name.to_string(), args))
}

// ── JSON: {"tool": .., "args": ..} and the other shapes models use ──

fn parse_json_calls(body: &str) -> Option<Vec<(String, Value)>> {
    let v: Value = serde_json::from_str(body).ok()?;
    let items = match v {
        Value::Array(a) if !a.is_empty() => a,
        Value::Object(_) => vec![v],
        _ => return None,
    };
    items.into_iter().map(json_call).collect()
}

fn json_call(v: Value) -> Option<(String, Value)> {
    let o = v.as_object()?;
    let func = o.get("function").and_then(|f| f.as_object());
    let name = ["tool", "tool_name", "name"]
        .iter()
        .find_map(|k| o.get(*k).and_then(|n| n.as_str()))
        .or_else(|| func.and_then(|f| f.get("name")).and_then(|n| n.as_str()))?
        .to_string();
    let args = ["args", "tool_args", "arguments", "parameters"]
        .iter()
        .find_map(|k| o.get(*k).cloned())
        .or_else(|| func.and_then(|f| f.get("arguments")).cloned())
        .unwrap_or_else(|| json!({}));
    // Arguments sometimes arrive as a JSON string.
    let args = match args {
        Value::String(s) => serde_json::from_str(&s).ok()?,
        other => other,
    };
    Some((name, args))
}

// ── Gemma 4's own syntax: name{key:<|"|>text<|"|>,n:3,o:{..},l:[..]} ──

fn parse_gemma_calls(body: &str) -> Option<Vec<(String, Value)>> {
    let mut p = Parser { s: body, i: 0 };
    let mut calls = Vec::new();
    loop {
        p.skip_separators();
        if p.done() {
            break;
        }
        let name = p.ident()?;
        p.skip_ws();
        let args = p.object()?;
        calls.push((name, args));
    }
    (!calls.is_empty()).then_some(calls)
}

struct Parser<'a> {
    s: &'a str,
    i: usize,
}

impl<'a> Parser<'a> {
    fn rest(&self) -> &'a str {
        &self.s[self.i..]
    }
    fn done(&self) -> bool {
        self.i >= self.s.len()
    }
    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }
    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.i += c.len_utf8();
        Some(c)
    }
    fn skip_ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.bump();
        }
    }
    fn skip_separators(&mut self) {
        while self.peek().is_some_and(|c| c.is_whitespace() || c == ',' || c == ';') {
            self.bump();
        }
    }
    fn ident(&mut self) -> Option<String> {
        let start = self.i;
        while self.peek().is_some_and(|c| c.is_ascii_alphanumeric() || c == '_') {
            self.bump();
        }
        (self.i > start).then(|| self.s[start..self.i].to_string())
    }
    fn expect(&mut self, c: char) -> Option<()> {
        self.skip_ws();
        (self.bump()? == c).then_some(())
    }
    fn object(&mut self) -> Option<Value> {
        self.expect('{')?;
        let mut map = Map::new();
        loop {
            self.skip_ws();
            if self.peek()? == '}' {
                self.bump();
                return Some(Value::Object(map));
            }
            let key = if self.rest().starts_with(GEMMA_QUOTE) {
                self.quoted()?
            } else if self.peek()? == '"' {
                self.bump();
                let end = self.rest().find('"')?;
                let k = self.rest()[..end].to_string();
                self.i += end + 1;
                k
            } else {
                self.ident()?
            };
            self.expect(':')?;
            let v = self.value()?;
            map.insert(key, v);
            self.skip_ws();
            match self.peek()? {
                ',' => {
                    self.bump();
                }
                '}' => {}
                _ => return None,
            }
        }
    }
    fn array(&mut self) -> Option<Value> {
        self.expect('[')?;
        let mut items = Vec::new();
        loop {
            self.skip_ws();
            if self.peek()? == ']' {
                self.bump();
                return Some(Value::Array(items));
            }
            items.push(self.value()?);
            self.skip_ws();
            match self.peek()? {
                ',' => {
                    self.bump();
                }
                ']' => {}
                _ => return None,
            }
        }
    }
    fn quoted(&mut self) -> Option<String> {
        self.i += GEMMA_QUOTE.len();
        let end = self.rest().find(GEMMA_QUOTE)?;
        let text = self.rest()[..end].to_string();
        self.i += end + GEMMA_QUOTE.len();
        Some(text)
    }
    fn value(&mut self) -> Option<Value> {
        self.skip_ws();
        if self.rest().starts_with(GEMMA_QUOTE) {
            return self.quoted().map(Value::String);
        }
        match self.peek()? {
            '{' => self.object(),
            '[' => self.array(),
            '"' => {
                // A plain JSON string, escapes and all.
                let mut de = serde_json::Deserializer::from_str(self.rest()).into_iter::<String>();
                let s = de.next()?.ok()?;
                self.i += de.byte_offset();
                Some(Value::String(s))
            }
            _ => {
                let end = self.rest().find([',', '}', ']']).unwrap_or(self.rest().len());
                let lit = self.rest()[..end].trim().to_string();
                self.i += end;
                if lit.is_empty() {
                    return None;
                }
                Some(serde_json::from_str::<Value>(&lit).unwrap_or(Value::String(lit)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(text: &str) -> (String, Value) {
        let calls = recover(text);
        assert_eq!(calls.len(), 1, "{text}");
        (calls[0].name.clone(), serde_json::from_str(&calls[0].arguments).unwrap())
    }

    #[test]
    fn gemma_syntax_without_its_opening_token() {
        // Verbatim from the 2026-10-21 replay.
        let (name, args) = one("file_list{path:<|\"|>/Users/you/Projects/orbix<|\"|>}");
        assert_eq!(name, "tool");
        assert_eq!(args, json!({"tool":"file_list","args":{"path":"/Users/you/Projects/orbix"}}));
    }

    #[test]
    fn gemma_syntax_in_a_fence_with_the_closing_token() {
        let (_, args) = one("```shell_exec{command:<|\"|>wc -l notes.txt<|\"|>,cwd:<|\"|>/tmp<|\"|>}\n```");
        assert_eq!(args["args"], json!({"command":"wc -l notes.txt","cwd":"/tmp"}));
        let (_, args) = one("shell_exec{command:<|\"|>git status<|\"|>}<tool_call|>");
        assert_eq!(args["args"]["command"], "git status");
    }

    #[test]
    fn gemma_values_keep_their_types() {
        let (name, args) = one("create_agent{name:<|\"|>Scout<|\"|>,every_minutes:30,enabled:true,tags:[<|\"|>a<|\"|>,<|\"|>b, c<|\"|>]}");
        assert_eq!(name, "create_agent");
        assert_eq!(args, json!({"name":"Scout","every_minutes":30,"enabled":true,"tags":["a","b, c"]}));
    }

    #[test]
    fn fenced_json_array_of_calls() {
        // Verbatim shape from the 2026-10-21 replay.
        let text = "```json\n[\n  {\n    \"tool\": \"shell_exec\",\n    \"args\": {\n      \"command\": \"wc -l notes.txt\"\n    }\n  }\n]\n```";
        let (name, args) = one(text);
        assert_eq!(name, "tool");
        assert_eq!(args["args"]["command"], "wc -l notes.txt");
    }

    #[test]
    fn tool_name_and_tool_args_keys() {
        let (_, args) = one("```json\n[{\"tool_name\":\"file_read\",\"tool_args\":{\"path\":\"a.txt\"}}]\n```");
        assert_eq!(args, json!({"tool":"file_read","args":{"path":"a.txt"}}));
    }

    #[test]
    fn two_calls_in_one_reply() {
        let calls = recover("file_list{path:<|\"|>/a<|\"|>}\nfile_list{path:<|\"|>/b<|\"|>}");
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn prose_is_never_executed() {
        assert!(recover("I listed the folder: it has README.md and src/.").is_empty());
        assert!(recover("Run file_list{path:<|\"|>/a<|\"|>} to see it.").is_empty());
        assert!(recover("/Users/you/Projects/planner/\nfile_list{path:<|\"|>/x<|\"|>}").is_empty());
        assert!(recover("```json\n{\"name\": \"Ada\", \"age\": 3}\n```").is_empty());
    }

    #[test]
    fn unknown_tools_are_not_recovered() {
        assert!(recover("delete_everything{path:<|\"|>/<|\"|>}").is_empty());
        assert!(recover("[{\"tool\":\"rm_rf\",\"args\":{}}]").is_empty());
    }
}

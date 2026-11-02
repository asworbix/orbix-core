//! Commands split out of main.rs (W0.3). Mechanical move, no behavior change.

#[allow(unused_imports)]
use crate::prelude::*;
#[allow(unused_imports)]
use crate::*;

// ─────────────────────────────────────────────
//  Commands - CLI / Code Gen (AI-assisted)
// ─────────────────────────────────────────────


#[tauri::command]
pub(crate) async fn generate_cli_template(template_type: String) -> Result<CliTemplate, String> {
    let (name, description, code, config) = match template_type.as_str() {
        "mcp_server" => (
            "my-mcp-server".to_string(),
            "A basic MCP server template".to_string(),
            r#"#!/usr/bin/env node
import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { ListToolsRequestSchema, CallToolRequestSchema } from "@modelcontextprotocol/sdk/types.js";

pub(crate) const server = new Server(
  { name: "my-mcp-server", version: "1.0.0" },
  { capabilities: { tools: {} } }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: [
    {
      name: "hello_world",
      description: "A simple hello world tool",
      inputSchema: {
        type: "object",
        properties: {
          name: { type: "string", description: "Name to greet" }
        },
        required: ["name"]
      }
    }
  ]
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  if (request.params.name === "hello_world") {
    const { name } = request.params.arguments;
    return { content: [{ type: "text", text: `Hello, ${name}!` }] };
  }
  throw new Error(`Unknown tool: ${request.params.name}`);
});

pub(crate) const transport = new StdioServerTransport();
await server.connect(transport);
"#.to_string(),
            r#"{
  "mcpServers": {
    "my-mcp-server": {
      "command": "node",
      "args": ["./index.js"]
    }
  }
}"#.to_string(),
        ),
        "cli_tool" => (
            "my-cli-tool".to_string(),
            "A basic CLI tool template".to_string(),
            r#"#!/usr/bin/env bash
# OrbixCore CLI Tool Template
# Usage: ./tool.sh [command] [args...]

set -euo pipefail

TOOL_NAME="my-cli-tool"
VERSION="1.0.0"

usage() {
  echo "Usage: $TOOL_NAME [command]"
  echo ""
  echo "Commands:"
  echo "  run     Execute main action"
  echo "  status  Show current status"
  echo "  help    Show this help"
}

cmd_run() {
  echo "Running $TOOL_NAME v$VERSION..."
  # Add your logic here
}

cmd_status() {
  echo "Status: OK"
}

case "${1:-help}" in
  run)    cmd_run ;;
  status) cmd_status ;;
  help|*) usage ;;
esac
"#.to_string(),
            "{}".to_string(),
        ),
        _ => return Err(format!("Unknown template type: {}", template_type)),
    };

    Ok(CliTemplate {
        name,
        description,
        template_type,
        code,
        config,
    })
}


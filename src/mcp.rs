//! MCP server for live ayatsuri state inspection.
//!
//! Tools:
//!   `get_state`    — full ECS state snapshot (displays, workspaces, windows, config)
//!   `get_focused`  — currently focused window
//!   `get_displays` — all displays with bounds, workspaces, and windows
//!   `get_config`   — config flags (FFM, MFF, `auto_center`, etc.)
//!   `send_command` — dispatch a command to the running daemon

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
    transport::stdio,
};
use serde::Deserialize;

use crate::reader::CommandReader;

// ── Tool input types ────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SendCommandInput {
    #[schemars(
        description = "Command string to send (e.g. 'focus east', 'swap west', 'center', 'cycle'). Multiple words separated by spaces."
    )]
    command: String,
}

// ── MCP Server ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct AyatsuriMcp {
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl AyatsuriMcp {
    fn new() -> Self {
        Self {
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        description = "Get full ayatsuri state: displays, workspaces, windows, config flags. Returns JSON."
    )]
    async fn get_state(&self) -> String {
        query_daemon("state")
    }

    #[tool(description = "Get the currently focused window. Returns JSON or null.")]
    async fn get_focused(&self) -> String {
        query_daemon("focused")
    }

    #[tool(
        description = "Get all display info with bounds, dock position, and workspaces. Returns JSON array."
    )]
    async fn get_displays(&self) -> String {
        query_daemon("displays")
    }

    #[tool(
        description = "Get current config flags: focus_follows_mouse, mouse_follows_focus, auto_center, skip_reshuffle, mission_control_active, initializing. Returns JSON."
    )]
    async fn get_config(&self) -> String {
        query_daemon("config")
    }

    #[tool(
        description = "Send a command to the running ayatsuri daemon (e.g. 'focus east', 'swap west', 'center', 'cycle')"
    )]
    async fn send_command(&self, Parameters(input): Parameters<SendCommandInput>) -> String {
        let words: Vec<String> = input.command.split_whitespace().map(String::from).collect();
        match CommandReader::send_command(words) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        }
    }
}

#[tool_handler]
impl ServerHandler for AyatsuriMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some(
                "Ayatsuri window manager — live ECS state inspection and command dispatch.".into(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn query_daemon(query: &str) -> String {
    match CommandReader::send_query(query) {
        Ok(json) => json,
        Err(e) => format!(
            r#"{{"error":"Failed to connect to ayatsuri daemon: {e}"}}"#
        ),
    }
}

pub async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let server = AyatsuriMcp::new().serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}

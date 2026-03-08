//! MCP server for live ayatsuri state inspection and control.
//!
//! Tools:
//!   `get_state`       — full ECS state snapshot (displays, workspaces, windows, config)
//!   `get_focused`     — currently focused window
//!   `get_displays`    — all displays with bounds, workspaces, and windows
//!   `get_config`      — config flags (FFM, MFF, `auto_center`, etc.)
//!   `get_full_config` — complete config options as JSON
//!   `list_windows`    — all windows across all displays
//!   `get_window`      — single window by ID
//!   `list_workspaces` — all workspaces with display association
//!   `send_command`    — dispatch a command to the running daemon
//!   `focus_window`    — focus a specific window by ID
//!   `move_window`     — move focused window to x,y position
//!   `resize_window`   — resize focused window to w,h dimensions
//!   `set_config`      — dynamically change config values at runtime
//!   `reload_config`   — reload config from disk
//!   `set_mode`        — switch between tiling and floating mode

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
        description = "Command string to send (e.g. 'window focus east', 'window swap west', 'window center', 'window resize'). Multiple words separated by spaces."
    )]
    command: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct WindowIdInput {
    #[schemars(description = "The platform window ID (integer).")]
    window_id: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct MoveWindowInput {
    #[schemars(description = "Target X coordinate in pixels.")]
    x: i32,
    #[schemars(description = "Target Y coordinate in pixels.")]
    y: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ResizeWindowInput {
    #[schemars(description = "Target width in pixels.")]
    width: i32,
    #[schemars(description = "Target height in pixels.")]
    height: i32,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetConfigInput {
    #[schemars(
        description = "JSON object with config keys to set. Merged into current config. Example: {\"spring\": {\"stiffness\": 1200}, \"dim_inactive_windows\": 0.3}"
    )]
    config: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetModeInput {
    #[schemars(description = "Window management mode: 'tiling' or 'floating'.")]
    mode: String,
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

    // ── Query tools ─────────────────────────────────────────────────────────

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
        description = "Get current config flags: focus_follows_mouse, mouse_follows_focus, auto_center, etc. Returns JSON."
    )]
    async fn get_config(&self) -> String {
        query_daemon("config")
    }

    #[tool(
        description = "Get the full config options as JSON, including spring, animation, display, edge_snap, and all other settings."
    )]
    async fn get_full_config(&self) -> String {
        query_daemon("full_config")
    }

    #[tool(
        description = "List all windows across all displays. Returns JSON array with id, title, app_name, bundle_id, bounds, focused/stacked/unmanaged status, display_id, workspace_id."
    )]
    async fn list_windows(&self) -> String {
        query_daemon("windows")
    }

    #[tool(
        description = "Get a single window by its platform window ID. Returns JSON object or null."
    )]
    async fn get_window(&self, Parameters(input): Parameters<WindowIdInput>) -> String {
        query_daemon(&format!("window:{}", input.window_id))
    }

    #[tool(
        description = "List all workspaces with display association and window counts. Returns JSON array."
    )]
    async fn list_workspaces(&self) -> String {
        query_daemon("workspaces")
    }

    // ── Command tools ───────────────────────────────────────────────────────

    #[tool(
        description = "Send a raw command to the running ayatsuri daemon. Commands: 'window focus east/west/north/south/first/last', 'window swap east/west/north/south', 'window center', 'window resize', 'window fullwidth', 'window manage', 'window equalize', 'window stack', 'window unstack', 'window nextdisplay', 'window focus_id <id>', 'window move_to <x> <y>', 'window resize_to <w> <h>', 'mode tiling/floating', 'reload', 'quit'."
    )]
    async fn send_command(&self, Parameters(input): Parameters<SendCommandInput>) -> String {
        let words: Vec<String> = input.command.split_whitespace().map(String::from).collect();
        match CommandReader::send_command(words) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        }
    }

    #[tool(description = "Focus a specific window by its platform window ID.")]
    async fn focus_window(&self, Parameters(input): Parameters<WindowIdInput>) -> String {
        let cmd = format!("window focus_id {}", input.window_id);
        let words: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        match CommandReader::send_command(words) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        }
    }

    #[tool(description = "Move the currently focused window to an exact x,y position in pixels.")]
    async fn move_window(&self, Parameters(input): Parameters<MoveWindowInput>) -> String {
        let cmd = format!("window move_to {} {}", input.x, input.y);
        let words: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        match CommandReader::send_command(words) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        }
    }

    #[tool(
        description = "Resize the currently focused window to exact pixel dimensions (width, height)."
    )]
    async fn resize_window(&self, Parameters(input): Parameters<ResizeWindowInput>) -> String {
        let cmd = format!("window resize_to {} {}", input.width, input.height);
        let words: Vec<String> = cmd.split_whitespace().map(String::from).collect();
        match CommandReader::send_command(words) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        }
    }

    #[tool(
        description = "Set config values dynamically at runtime. Pass a JSON string with the config keys to change. Example: '{\"spring\": {\"stiffness\": 1200}, \"dim_inactive_windows\": 0.3}'. Changes take effect immediately."
    )]
    async fn set_config(&self, Parameters(input): Parameters<SetConfigInput>) -> String {
        match CommandReader::send_set_config(&input.config) {
            Ok(response) => response,
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        }
    }

    #[tool(description = "Reload the config file from disk. Applies all changes immediately.")]
    async fn reload_config(&self) -> String {
        let words = vec!["reload".to_string()];
        match CommandReader::send_command(words) {
            Ok(()) => r#"{"ok":true}"#.to_string(),
            Err(e) => format!(r#"{{"error":"{e}"}}"#),
        }
    }

    #[tool(
        description = "Switch between tiling and floating window management modes. Mode: 'tiling' or 'floating'."
    )]
    async fn set_mode(&self, Parameters(input): Parameters<SetModeInput>) -> String {
        let cmd = format!("mode {}", input.mode);
        let words: Vec<String> = cmd.split_whitespace().map(String::from).collect();
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

use serde::Serialize;

/// Complete snapshot of ayatsuri's ECS state, suitable for JSON serialization.
/// Built every Bevy tick and shared with the socket reader thread via `ArcSwap`.
#[derive(Serialize, Clone, Default)]
pub struct StateSnapshot {
    pub displays: Vec<DisplaySnapshot>,
    pub focused_window: Option<WindowSnapshot>,
    pub config_flags: ConfigFlags,
}

#[derive(Serialize, Clone)]
pub struct DisplaySnapshot {
    pub id: u32,
    pub is_active: bool,
    pub bounds: BoundsSnapshot,
    pub dock: Option<String>,
    pub workspaces: Vec<WorkspaceSnapshot>,
}

#[derive(Serialize, Clone)]
pub struct WorkspaceSnapshot {
    pub id: u64,
    pub is_active: bool,
    pub layout_strip: LayoutStripSnapshot,
}

#[derive(Serialize, Clone)]
pub struct LayoutStripSnapshot {
    pub windows: Vec<WindowSnapshot>,
}

#[derive(Serialize, Clone)]
pub struct WindowSnapshot {
    pub id: i32,
    pub title: String,
    pub app_name: String,
    pub bundle_id: String,
    pub bounds: BoundsSnapshot,
    pub is_focused: bool,
    pub is_unmanaged: bool,
    pub is_full_width: bool,
}

#[derive(Serialize, Clone)]
pub struct BoundsSnapshot {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Serialize, Clone, Default)]
#[allow(clippy::struct_excessive_bools)]
pub struct ConfigFlags {
    pub mode: String,
    pub enable_manage_toggle: bool,
    pub focus_follows_mouse: bool,
    pub mouse_follows_focus: bool,
    pub auto_center: bool,
    pub skip_reshuffle: bool,
    pub mission_control_active: bool,
    pub initializing: bool,
    pub edge_snap_left: bool,
    pub edge_snap_right: bool,
    pub edge_snap_preview: bool,
    pub edge_snap_sticky_dwell_ms: u64,
    pub suppress_four_finger: bool,
    pub suppress_five_finger_pinch: bool,
    pub suppress_five_finger_spread: bool,
}

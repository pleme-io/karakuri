use std::sync::Arc;

use arc_swap::ArcSwap;
use bevy::app::{App, Plugin, PostUpdate};
use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;

use crate::config::Config;
use crate::ecs::{
    ActiveDisplayMarker, ActiveWorkspaceMarker, DockPosition, FocusedMarker, FullWidthMarker,
    Initializing, MissionControlActive, SkipReshuffle, Unmanaged,
};
use crate::manager::{Application, Display, LayoutStrip, Window};
use crate::snapshot::{
    BoundsSnapshot, ConfigFlags, DisplaySnapshot, LayoutStripSnapshot, StateSnapshot,
    WindowSnapshot, WorkspaceSnapshot,
};

/// Shared state published by the Bevy `sync_snapshot` system and read by the socket thread / MCP.
#[derive(Resource, Clone)]
pub struct SharedState(pub Arc<ArcSwap<StateSnapshot>>);

pub struct SnapshotPlugin;

impl Plugin for SnapshotPlugin {
    fn build(&self, app: &mut App) {
        // SharedState resource is pre-inserted by setup_bevy_app() so the same
        // Arc can be shared with the CommandReader thread. Only register the system.
        app.add_systems(PostUpdate, sync_snapshot);
    }
}

#[allow(clippy::needless_pass_by_value, clippy::type_complexity, clippy::too_many_arguments)]
fn sync_snapshot(
    config: Res<Config>,
    skip_reshuffle: Res<SkipReshuffle>,
    mission_control: Res<MissionControlActive>,
    initializing: Option<Res<Initializing>>,
    displays: Query<(&Display, Entity, Has<ActiveDisplayMarker>, Option<&DockPosition>)>,
    workspaces: Query<(&LayoutStrip, Entity, &ChildOf, Has<ActiveWorkspaceMarker>)>,
    windows: Query<(
        &Window,
        Entity,
        &ChildOf,
        Option<&Unmanaged>,
        Option<&FullWidthMarker>,
    )>,
    focused: Query<Entity, With<FocusedMarker>>,
    apps: Query<&Application>,
    shared: Res<SharedState>,
) {
    let focused_entity = focused.single().ok();

    let mut display_snapshots = Vec::new();
    let mut focused_window_snapshot = None;

    for (display, display_entity, is_active, dock) in &displays {
        let mut workspace_snapshots = Vec::new();

        for (strip, _ws_entity, child, ws_active) in &workspaces {
            if child.parent() != display_entity {
                continue;
            }

            let mut win_snapshots = Vec::new();
            for entity in strip.all_windows() {
                if let Ok((window, win_entity, win_child, unmanaged, full_width)) =
                    windows.get(entity)
                {
                    let bundle_id = apps
                        .get(win_child.parent())
                        .ok()
                        .and_then(|app| app.bundle_id().map(str::to_owned))
                        .unwrap_or_default();

                    let is_focused = focused_entity.is_some_and(|f| f == win_entity);
                    let frame = window.frame();
                    let snap = WindowSnapshot {
                        id: window.id(),
                        title: window.title().unwrap_or_default(),
                        app_name: bundle_id.clone(),
                        bundle_id,
                        bounds: BoundsSnapshot {
                            x: frame.min.x,
                            y: frame.min.y,
                            width: frame.width(),
                            height: frame.height(),
                        },
                        is_focused,
                        is_unmanaged: unmanaged.is_some(),
                        is_full_width: full_width.is_some(),
                    };

                    if is_focused {
                        focused_window_snapshot = Some(snap.clone());
                    }
                    win_snapshots.push(snap);
                }
            }

            workspace_snapshots.push(WorkspaceSnapshot {
                id: strip.id(),
                is_active: ws_active,
                layout_strip: LayoutStripSnapshot {
                    windows: win_snapshots,
                },
            });
        }

        let dock_str = dock.map(|d| match d {
            DockPosition::Bottom(px) => format!("bottom({px})"),
            DockPosition::Left(px) => format!("left({px})"),
            DockPosition::Right(px) => format!("right({px})"),
            DockPosition::Hidden => "hidden".to_string(),
        });

        let bounds = display.bounds();
        display_snapshots.push(DisplaySnapshot {
            id: display.id(),
            is_active,
            bounds: BoundsSnapshot {
                x: bounds.min.x,
                y: bounds.min.y,
                width: bounds.width(),
                height: bounds.height(),
            },
            dock: dock_str,
            workspaces: workspace_snapshots,
        });
    }

    let options = config.options();
    let snapshot = StateSnapshot {
        displays: display_snapshots,
        focused_window: focused_window_snapshot,
        config_flags: ConfigFlags {
            mode: match options.mode {
                crate::config::WindowMode::Tiling => "tiling".to_string(),
                crate::config::WindowMode::Floating => "floating".to_string(),
            },
            enable_manage_toggle: options.enable_manage_toggle.unwrap_or(true),
            focus_follows_mouse: options.focus_follows_mouse.is_none_or(|v| v),
            mouse_follows_focus: options.mouse_follows_focus.is_none_or(|v| v),
            auto_center: options.auto_center.is_some_and(|v| v),
            skip_reshuffle: skip_reshuffle.0,
            mission_control_active: mission_control.0,
            initializing: initializing.is_some(),
            edge_snap_left: options.edge_snap.left.unwrap_or(false),
            edge_snap_right: options.edge_snap.right.unwrap_or(false),
            edge_snap_preview: options.edge_snap.preview.unwrap_or(true),
            edge_snap_sticky_dwell_ms: options.edge_snap.sticky_dwell_ms.unwrap_or(300),
            suppress_five_finger_pinch: options.gesture_suppress.five_finger_pinch.unwrap_or(false),
            suppress_five_finger_spread: options.gesture_suppress.five_finger_spread.unwrap_or(false),
        },
    };

    shared.0.store(Arc::new(snapshot));
}

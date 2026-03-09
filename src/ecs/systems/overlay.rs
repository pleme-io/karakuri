//! Overlay rendering systems — window borders, dim inactive, snap preview.

use bevy::ecs::entity::Entity;
use bevy::ecs::query::With;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res};
use bevy::math::IRect;

use crate::config::Config;
use crate::ecs::params::{ActiveDisplay, Configuration, Windows};
use crate::ecs::state::DragContext;
use crate::ecs::WindowDraggedMarker;
use crate::manager::{Application, Display};
use crate::overlay::OverlayManager;

/// Updates dim and border overlays for the focused window.
///
/// Skips floating/unmanaged/fullscreen windows. When no managed window
/// is focused, all overlays are hidden.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn update_overlays(
    windows: Windows,
    applications: Query<&Application>,
    _: ActiveDisplay, // prevents this system from running without an active workspace
    overlay_mgr: Option<NonSendMut<OverlayManager>>,
    config: Configuration,
) {
    use crate::overlay::{BorderParams, OverlayApi};

    let Some(mut overlay_mgr) = overlay_mgr else {
        return;
    };

    if !config.config().window_management_enabled() {
        overlay_mgr.remove_all();
        return;
    }

    let dim_opacity = config.config().dim_inactive_opacity();
    let border_enabled = config.config().border_active_window();

    if dim_opacity == 0.0 && !border_enabled {
        overlay_mgr.remove_all();
        return;
    }

    // Find the focused managed window's absolute CG frame.
    // Skip floating/unmanaged windows — no overlay or border for those.
    let (focused_frame, focused_border_radius) = if let Some((window, _, unmanaged)) = windows
        .focused()
        .and_then(|(_, entity)| windows.get_managed(entity))
        && unmanaged.is_none()
        && !window.is_full_screen()
    {
        let frame = window.frame();
        let h_pad = window.horizontal_padding();
        let v_pad = window.vertical_padding();
        let focused_frame = Some(IRect::new(
            frame.min.x + h_pad,
            frame.min.y + v_pad,
            frame.max.x - h_pad,
            frame.max.y - v_pad,
        ));

        // Look up per-window border_radius from config (dynamic, respects hot-reload).
        let title = window.title().unwrap_or_default();
        let bundle_id = windows
            .find_parent(window.id())
            .and_then(|(_, _, parent)| applications.get(parent).ok())
            .map(|app| app.bundle_id().unwrap_or_default())
            .unwrap_or_default();
        let properties = config.find_window_properties(&title, bundle_id);
        let focused_border_radius = properties.iter().find_map(|p| p.border_radius);

        (focused_frame, focused_border_radius)
    } else {
        overlay_mgr.hide_all();
        return;
    };

    let border_params = border_enabled.then(|| BorderParams {
        color: config.config().border_color(),
        opacity: config.config().border_opacity(),
        width: config.config().border_width(),
        radius: focused_border_radius.unwrap_or_else(|| config.config().border_radius()),
    });

    let dim_color = config.config().dim_inactive_color();
    overlay_mgr.update(
        dim_opacity,
        dim_color,
        focused_frame,
        border_params.as_ref(),
    );
}

/// Shows/hides the snap preview overlay based on `DragContext`.
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn update_snap_preview(
    drag_ctx: Option<Res<DragContext>>,
    displays: Query<&Display>,
    config: Res<Config>,
    overlay_mgr: Option<NonSendMut<OverlayManager>>,
) {
    use crate::logic::snap::snap_frame;
    use crate::overlay::{BorderParams, OverlayApi};

    let Some(mut overlay_mgr) = overlay_mgr else {
        return;
    };

    if !config.window_management_enabled() || !config.edge_snap_preview_enabled() {
        overlay_mgr.hide_snap_preview();
        return;
    }

    let zone = drag_ctx.as_ref().and_then(|s| s.snap_zone);

    let Some(zone) = zone else {
        overlay_mgr.hide_snap_preview();
        return;
    };

    let Some(ctx) = drag_ctx.as_ref() else {
        overlay_mgr.hide_snap_preview();
        return;
    };
    let display_id = ctx.display_id;
    let Some(display) = displays.iter().find(|d| d.id() == display_id) else {
        overlay_mgr.hide_snap_preview();
        return;
    };

    let bounds = display.bounds();
    let pad = config.edge_padding();
    let (origin, size) = snap_frame(zone, &bounds, pad);
    let frame = IRect::from_corners(origin, origin + size);

    let opacity = config.edge_snap_preview_opacity();
    let border = BorderParams {
        color: config.border_color(),
        opacity: config.border_opacity(),
        width: config.border_width(),
        radius: config.border_radius(),
    };

    overlay_mgr.update_snap_preview(frame, opacity, &border);
}

/// Despawns stale `WindowDraggedMarker` entities during a swipe.
///
/// Gated with `run_if(in_state(Swiping))` — replaces the manual
/// `swipe_tracker.active()` guard that was previously inside
/// `reposition_dragged_window`.
pub(crate) fn swipe_cleanup_drag_markers(
    markers: Query<Entity, With<WindowDraggedMarker>>,
    mut commands: Commands,
) {
    for entity in &markers {
        commands.entity(entity).despawn();
    }
}

/// Hides all overlays while Swiping or MissionControl is active.
///
/// Counterpart to `update_overlays`, which now only runs outside those modes.
pub(crate) fn hide_overlays_on_mode_change(
    overlay_mgr: Option<NonSendMut<OverlayManager>>,
) {
    if let Some(mut overlay_mgr) = overlay_mgr {
        use crate::overlay::OverlayApi;
        overlay_mgr.hide_all();
    }
}

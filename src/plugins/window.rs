use bevy::app::{App, Plugin, PostUpdate, PreUpdate, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::prelude::{not, SystemCondition};
use bevy::state::condition::in_state;

use crate::config::Config;
use crate::ecs::state::{InteractionMode, ReloadGuard, SwipeContext};
use crate::ecs::{systems, triggers};

/// Plugin for window management: layout, focus, animation, drag, swipe, overlays,
/// and all window-related commands (focus, swap, center, resize, fullwidth, manage, stack).
pub struct WindowPlugin;

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        // Pre-update: pump events first, then dispatch triggers that read them
        app.add_systems(
            PreUpdate,
            (systems::pump_events, systems::dispatch_toplevel_triggers).chain(),
        );

        // Update: window lifecycle, drag reposition (suppressed during swipe)
        app.add_systems(
            Update,
            (
                systems::fresh_marker_cleanup,
                systems::timeout_ticker,
                systems::window_update_frame,
                systems::reposition_dragged_window
                    .run_if(not(in_state(InteractionMode::Swiping))),
            ),
        );

        // Post-update: swipe → layout → animation → overlays
        app.add_systems(
            PostUpdate,
            (
                // Swipe systems (moved from Update — layout computation belongs in PostUpdate)
                systems::window_swiper,
                systems::swipe_idle_tracker
                    .run_if(|swipe_tracker: Option<bevy::ecs::system::Res<SwipeContext>>| {
                        swipe_tracker.is_some()
                    }),
                // Clean up stale drag markers during swipe
                systems::swipe_cleanup_drag_markers
                    .run_if(in_state(InteractionMode::Swiping)),
                // Reload guard debounce
                systems::reload_guard_ticker
                    .run_if(|guard: Option<bevy::ecs::system::Res<ReloadGuard>>| {
                        guard.is_some()
                    })
                    .after(systems::window_swiper)
                    .after(systems::swipe_idle_tracker),
                // Layout reshuffle (suppressed during swipe — FSM gate replaces manual guard)
                systems::reshuffle_layout_strip
                    .run_if(not(in_state(InteractionMode::Swiping)))
                    .after(systems::reload_guard_ticker),
                // Animation (must see swipe-produced markers)
                systems::animate_windows
                    .after(systems::reshuffle_layout_strip)
                    .after(systems::window_swiper)
                    .after(systems::swipe_idle_tracker),
                systems::animate_resize_windows
                    .after(systems::reshuffle_layout_strip)
                    .after(systems::animate_windows),
                // Overlays (suppressed during swipe and mission control)
                systems::update_overlays
                    .run_if(
                        not(in_state(InteractionMode::Swiping))
                            .and(not(in_state(InteractionMode::MissionControl))),
                    )
                    .run_if(|config: Option<bevy::ecs::system::Res<Config>>| {
                        config.is_some_and(|config| {
                            config.dim_inactive_opacity() > 0.0 || config.border_active_window()
                        })
                    })
                    .after(systems::animate_windows)
                    .after(systems::animate_resize_windows),
                // Hide overlays when in swipe or mission control mode
                systems::hide_overlays_on_mode_change
                    .run_if(
                        in_state(InteractionMode::Swiping)
                            .or(in_state(InteractionMode::MissionControl)),
                    )
                    .after(systems::animate_resize_windows),
                systems::update_snap_preview
                    .after(systems::update_overlays),
            ),
        );

        // Window triggers
        app.add_observer(triggers::mouse_moved_trigger)
            .add_observer(triggers::mouse_down_trigger)
            .add_observer(triggers::mouse_dragged_trigger)
            .add_observer(triggers::window_focused_trigger)
            .add_observer(triggers::swipe_gesture_trigger)
            .add_observer(triggers::window_destroyed_trigger)
            .add_observer(triggers::window_unmanaged_trigger)
            .add_observer(triggers::window_managed_trigger)
            .add_observer(triggers::spawn_window_trigger)
            .add_observer(triggers::stray_focus_observer)
            .add_observer(triggers::window_removal_trigger)
            .add_observer(triggers::send_message_trigger)
            .add_observer(triggers::edge_snap_drag_trigger)
            .add_observer(triggers::edge_snap_release_trigger);

        // Command systems
        app.add_plugins(crate::commands::register_commands);
    }
}

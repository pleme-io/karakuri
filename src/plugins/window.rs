use bevy::app::{App, Plugin, PostUpdate, PreUpdate, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;

use crate::config::Config;
use crate::ecs::TrackpadSwipe;
use crate::ecs::{systems, triggers};

/// Plugin for window management: layout, focus, animation, drag, swipe, overlays,
/// and all window-related commands (focus, swap, center, resize, fullwidth, manage, stack).
pub struct WindowPlugin;

impl Plugin for WindowPlugin {
    fn build(&self, app: &mut App) {
        // Pre-update: event dispatch and pumping
        app.add_systems(
            PreUpdate,
            (systems::dispatch_toplevel_triggers, systems::pump_events),
        );

        // Update: window lifecycle, animation prep, swipe
        app.add_systems(
            Update,
            (
                systems::window_swiper,
                systems::swipe_idle_tracker
                    .run_if(|swipe_tracker: Option<bevy::ecs::system::Res<TrackpadSwipe>>| {
                        swipe_tracker.is_some()
                    }),
                systems::fresh_marker_cleanup,
                systems::timeout_ticker,
                systems::window_update_frame,
                systems::reposition_dragged_window,
            ),
        );

        // Post-update: layout reshuffling and animation
        app.add_systems(
            PostUpdate,
            (
                systems::reshuffle_layout_strip,
                systems::animate_windows.after(systems::reshuffle_layout_strip),
                systems::animate_resize_windows.after(systems::reshuffle_layout_strip),
                systems::update_overlays
                    .after(systems::animate_windows)
                    .after(systems::animate_resize_windows)
                    .run_if(|config: Option<bevy::ecs::system::Res<Config>>| {
                        config.is_some_and(|config| {
                            config.dim_inactive_opacity() > 0.0 || config.border_active_window()
                        })
                    }),
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

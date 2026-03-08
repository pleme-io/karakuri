use std::time::Duration;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::time::common_conditions::on_timer;

use crate::ecs::{PollForNotifications, systems, triggers};

/// Display poll frequency. Matches `Config::display_poll_interval()` default.
/// Bevy's `on_timer` requires a compile-time duration; dynamic adjustment
/// would require replacing with a manual time check.
const DISPLAY_CHANGE_CHECK_FREQ_MS: u64 = 1000;

/// Plugin for display management: gathering displays, detecting rearrangements,
/// display change notifications, menubar height sync, and orphaned workspace cleanup.
pub struct DisplayPlugin;

impl Plugin for DisplayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, systems::gather_displays);

        app.add_systems(
            Update,
            (
                systems::sync_menubar_height,
                systems::displays_rearranged,
                systems::find_orphaned_workspaces
                    .run_if(on_timer(Duration::from_millis(DISPLAY_CHANGE_CHECK_FREQ_MS))),
            ),
        );

        app.add_systems(
            Update,
            systems::display_changes_watcher
                .run_if(resource_exists::<PollForNotifications>)
                .run_if(on_timer(Duration::from_millis(DISPLAY_CHANGE_CHECK_FREQ_MS))),
        );

        app.add_observer(triggers::display_change_trigger);
    }
}

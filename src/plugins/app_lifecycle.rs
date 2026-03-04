use std::time::Duration;

use bevy::app::{App, Plugin, Startup, Update};
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::schedule::common_conditions::resource_exists;
use bevy::time::common_conditions::on_timer;

use crate::ecs::{Initializing, PollForNotifications, StartupPending, systems, triggers};

const DISPLAY_CHANGE_CHECK_FREQ_MS: u64 = 1000;

/// Plugin for application lifecycle management: process/app discovery, launch tracking,
/// workspace changes, mission control, front-switched events, and dock location.
pub struct AppLifecyclePlugin;

impl Plugin for AppLifecyclePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, systems::gather_initial_processes);

        app.add_systems(
            Update,
            (
                (
                    systems::add_existing_process,
                    systems::add_existing_application,
                    systems::finish_setup,
                )
                    .chain()
                    .run_if(resource_exists::<Initializing>),
                systems::add_launched_process,
                systems::add_launched_application,
                systems::spawn_startup_apps
                    .run_if(resource_exists::<StartupPending>),
                systems::startup_app_ticker,
            ),
        );

        app.add_systems(
            Update,
            systems::workspace_change_watcher
                .run_if(resource_exists::<PollForNotifications>)
                .run_if(on_timer(Duration::from_millis(DISPLAY_CHANGE_CHECK_FREQ_MS))),
        );

        app.add_observer(triggers::workspace_change_trigger)
            .add_observer(triggers::active_workspace_trigger)
            .add_observer(triggers::front_switched_trigger)
            .add_observer(triggers::center_mouse_trigger)
            .add_observer(triggers::mission_control_trigger)
            .add_observer(triggers::application_event_trigger)
            .add_observer(triggers::dispatch_application_messages)
            .add_observer(triggers::locate_dock_trigger);
    }
}

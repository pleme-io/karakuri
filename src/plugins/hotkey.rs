use bevy::app::{App, Plugin};

use crate::ecs::triggers;

/// Plugin for hotkey handling: config file reload trigger.
/// The actual CGEventTap is set up in PlatformCallbacks (platform layer),
/// and events flow through the MPSC channel → pump_events (WindowPlugin).
/// This plugin registers the configuration refresh trigger which handles
/// hotkey rebinding on config changes.
pub struct HotkeyPlugin;

impl Plugin for HotkeyPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(triggers::refresh_configuration_trigger);
    }
}

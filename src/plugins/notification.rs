use bevy::app::{App, Plugin};
use bevy::ecs::message::{Message, Messages};
use tracing::info;

/// Message for sending a notification.
#[derive(Clone, Debug, Message)]
#[allow(dead_code)] // Fields consumed by notification dispatch (scripting, MCP).
pub struct SendNotification {
    pub title: String,
    pub body: String,
}

/// Message fired when a notification is clicked.
#[derive(Clone, Debug, Message)]
#[allow(dead_code)] // Fields consumed by notification click observer.
pub struct NotificationClicked {
    pub title: String,
}

/// Plugin for macOS notification support.
/// Provides `notify(title, body)` functionality and notification click events.
pub struct NotificationPlugin;

impl Plugin for NotificationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages<SendNotification>>();
        app.init_resource::<Messages<NotificationClicked>>();
        info!("notification plugin initialized");
    }
}

/// Send a macOS notification.
/// Called from Rhai scripts via the scripting API.
#[allow(dead_code)]
pub fn send_notification(title: &str, body: &str) {
    info!("[notification] {title}: {body}");
}

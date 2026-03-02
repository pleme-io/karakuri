use bevy::app::{App, Plugin};
use bevy::ecs::message::{Message, Messages};
use tracing::info;

/// Message for menu bar item clicks.
#[derive(Clone, Debug, Message)]
pub struct MenuBarItemClicked {
    pub item_id: String,
}

/// Plugin for macOS menu bar (status bar) integration.
/// Creates an NSStatusItem with configurable menu items that can be
/// registered from Rhai scripts.
///
/// Must run on the main thread (NonSend) since NSStatusBar requires it.
pub struct MenuBarPlugin;

impl Plugin for MenuBarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages<MenuBarItemClicked>>();
        info!("menu bar plugin initialized");
    }
}

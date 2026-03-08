pub mod api;
pub mod engine;
pub mod loader;

use bevy::app::{App, Plugin};
use tracing::{info, warn};

use engine::ScriptEngine;

/// Plugin that provides Rhai scripting support.
/// Loads `~/.config/ayatsuri/init.rhai` and `~/.config/ayatsuri/scripts/*.rhai` on startup.
pub struct ScriptingPlugin;

impl ScriptingPlugin {
    pub fn new() -> Self {
        Self
    }
}

impl Plugin for ScriptingPlugin {
    fn build(&self, app: &mut App) {
        let mut script_engine = ScriptEngine::new();

        // Register API functions into the Rhai engine before loading scripts.
        {
            let engine_arc = script_engine.engine();
            match engine_arc.write() {
                Ok(mut engine) => api::register_all(&mut engine),
                Err(poisoned) => {
                    tracing::error!("scripting engine lock poisoned, recovering");
                    api::register_all(&mut poisoned.into_inner());
                }
            }
        }

        // Load user scripts.
        loader::load_scripts(&mut script_engine);

        let script_count = script_engine.scripts.len();
        let hotkey_count = script_engine.hotkey_handlers.len();
        info!(
            "scripting: loaded {script_count} script(s), {hotkey_count} hotkey handler(s)"
        );

        // Apply startup wallpaper from config if set.
        if let Ok(config) =
            crate::config::Config::new(crate::config::CONFIGURATION_FILE.as_path())
            && let Some(ref path) = config.options().wallpaper
        {
            if let Err(e) = crate::platform::wallpaper::set_wallpaper_all(path) {
                warn!("failed to set startup wallpaper: {e}");
            } else {
                info!("startup wallpaper set: {path}");
            }
        }

        app.insert_resource(script_engine);
    }
}

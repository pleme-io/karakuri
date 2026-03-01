pub mod api;
pub mod engine;
pub mod loader;

use bevy::app::{App, Plugin};
use tracing::info;

use engine::ScriptEngine;

/// Plugin that provides Rhai scripting support.
/// Loads `~/.config/karakuri/init.rhai` and `~/.config/karakuri/scripts/*.rhai` on startup.
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
            let mut engine = engine_arc.write().expect("engine lock for API registration");
            api::register_all(&mut engine);
        }

        // Load user scripts.
        loader::load_scripts(&mut script_engine);

        let script_count = script_engine.scripts.len();
        let hotkey_count = script_engine.hotkey_handlers.len();
        info!(
            "scripting: loaded {script_count} script(s), {hotkey_count} hotkey handler(s)"
        );

        app.insert_resource(script_engine);
    }
}

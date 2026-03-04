pub mod hotkey;
pub mod utility;
pub mod wallpaper;

use rhai::Engine;

/// Register all API modules into the Rhai engine.
pub fn register_all(engine: &mut Engine) {
    hotkey::register(engine);
    utility::register(engine);
    wallpaper::register(engine);
}

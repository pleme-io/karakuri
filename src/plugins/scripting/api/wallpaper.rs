use rhai::Engine;
use tracing::{info, warn};

/// Register wallpaper functions into the Rhai engine.
///
/// Available in scripts:
/// - `set_wallpaper("~/Pictures/wallpaper.png")` — all screens
/// - `set_wallpaper_screen("~/Pictures/wallpaper.png", 0)` — specific screen
/// - `get_wallpaper()` — main screen path
/// - `get_wallpaper_screen(1)` — specific screen path
pub fn register(engine: &mut Engine) {
    engine.register_fn("set_wallpaper", |path: &str| -> bool {
        match crate::platform::wallpaper::set_wallpaper_all(path) {
            Ok(()) => {
                info!("[rhai] set_wallpaper({path:?}) ok");
                true
            }
            Err(e) => {
                warn!("[rhai] set_wallpaper({path:?}) failed: {e}");
                false
            }
        }
    });

    #[allow(clippy::cast_sign_loss)]
    engine.register_fn("set_wallpaper_screen", |path: &str, screen: i64| -> bool {
        match crate::platform::wallpaper::set_wallpaper_for_screen(path, screen as usize) {
            Ok(()) => {
                info!("[rhai] set_wallpaper_screen({path:?}, {screen}) ok");
                true
            }
            Err(e) => {
                warn!("[rhai] set_wallpaper_screen({path:?}, {screen}) failed: {e}");
                false
            }
        }
    });

    engine.register_fn("get_wallpaper", || -> String {
        match crate::platform::wallpaper::get_wallpaper() {
            Ok(path) => path,
            Err(e) => {
                warn!("[rhai] get_wallpaper() failed: {e}");
                String::new()
            }
        }
    });

    #[allow(clippy::cast_sign_loss)]
    engine.register_fn("get_wallpaper_screen", |screen: i64| -> String {
        match crate::platform::wallpaper::get_wallpaper_for_screen(screen as usize) {
            Ok(path) => path,
            Err(e) => {
                warn!("[rhai] get_wallpaper_screen({screen}) failed: {e}");
                String::new()
            }
        }
    });
}

use rhai::Engine;
use tracing::info;

/// Register utility functions into the Rhai engine.
///
/// Available in scripts:
///   log("message");
///   reload_config();
pub fn register(engine: &mut Engine) {
    engine.register_fn("log", |msg: &str| {
        info!("[rhai] {msg}");
    });

    // reload_config is a placeholder — the actual reload is triggered
    // through the existing config watcher / event system.
    engine.register_fn("reload_config", || {
        info!("[rhai] reload_config requested");
    });
}

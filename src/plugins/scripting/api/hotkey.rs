use rhai::Engine;

/// Register hotkey-related functions into the Rhai engine.
///
/// Available in scripts:
///   on_hotkey("cmd-h", || { ... });
pub fn register(engine: &mut Engine) {
    // The on_hotkey function is a no-op at engine level —
    // it gets intercepted by the ScriptingPlugin's init system
    // which wraps script evaluation to capture on_hotkey calls.
    //
    // We register a placeholder so scripts don't error on parse.
    engine.register_fn("on_hotkey", |_binding: &str, _callback: rhai::FnPtr| {
        // Actual registration is handled by the plugin's eval wrapper.
    });
}

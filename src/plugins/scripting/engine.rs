use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use bevy::ecs::resource::Resource;
use rhai::{AST, Engine, FnPtr, Scope};
use tracing::{error, info};

/// Bevy resource holding the Rhai scripting engine and handler registry.
#[derive(Resource)]
#[allow(dead_code)]
pub struct ScriptEngine {
    /// The Rhai engine instance, wrapped for thread-safe access.
    engine: Arc<RwLock<Engine>>,
    /// Compiled ASTs for loaded scripts.
    pub(crate) scripts: Vec<(String, AST)>,
    /// Hotkey handlers: binding string → Rhai function pointer.
    pub(crate) hotkey_handlers: HashMap<String, FnPtr>,
    /// Event callback handlers: event name → list of Rhai function pointers.
    pub(crate) event_handlers: HashMap<String, Vec<FnPtr>>,
}

#[allow(dead_code)]
impl ScriptEngine {
    /// Creates a new `ScriptEngine` with a default Rhai engine.
    pub fn new() -> Self {
        let mut engine = Engine::new();
        // Set reasonable limits for user scripts.
        engine.set_max_expr_depths(64, 32);
        engine.set_max_call_levels(32);
        engine.set_max_operations(100_000);

        Self {
            engine: Arc::new(RwLock::new(engine)),
            scripts: Vec::new(),
            hotkey_handlers: HashMap::new(),
            event_handlers: HashMap::new(),
        }
    }

    /// Returns a clone of the engine Arc for registration.
    pub fn engine(&self) -> Arc<RwLock<Engine>> {
        self.engine.clone()
    }

    /// Register a hotkey handler from a Rhai script.
    pub fn register_hotkey(&mut self, binding: String, handler: FnPtr) {
        info!("registering hotkey handler: {binding}");
        self.hotkey_handlers.insert(binding, handler);
    }

    /// Register an event callback from a Rhai script.
    pub fn register_event_handler(&mut self, event_name: String, handler: FnPtr) {
        info!("registering event handler: {event_name}");
        self.event_handlers
            .entry(event_name)
            .or_default()
            .push(handler);
    }

    /// Compile and evaluate a script string.
    pub fn eval_script(&mut self, name: &str, source: &str) -> Result<(), String> {
        let engine = self
            .engine
            .read()
            .map_err(|e| format!("engine lock: {e}"))?;
        let ast = engine
            .compile(source)
            .map_err(|e| format!("compile {name}: {e}"))?;
        let mut scope = Scope::new();
        engine
            .run_ast_with_scope(&mut scope, &ast)
            .map_err(|e| format!("run {name}: {e}"))?;
        drop(engine);
        self.scripts.push((name.to_string(), ast));
        Ok(())
    }

    /// Call a registered hotkey handler by binding string.
    pub fn call_hotkey(&self, binding: &str) -> bool {
        let Some(handler) = self.hotkey_handlers.get(binding) else {
            return false;
        };
        let engine = match self.engine.read() {
            Ok(e) => e,
            Err(e) => {
                error!("engine lock for hotkey {binding}: {e}");
                return false;
            }
        };
        match handler.call::<()>(&engine, &AST::empty(), ()) {
            Ok(()) => true,
            Err(e) => {
                error!("hotkey handler {binding}: {e}");
                false
            }
        }
    }

    /// Call all registered event handlers for a given event name.
    pub fn fire_event(&self, event_name: &str) {
        let Some(handlers) = self.event_handlers.get(event_name) else {
            return;
        };
        let engine = match self.engine.read() {
            Ok(e) => e,
            Err(e) => {
                error!("engine lock for event {event_name}: {e}");
                return;
            }
        };
        for handler in handlers {
            if let Err(e) = handler.call::<()>(&engine, &AST::empty(), ()) {
                error!("event handler {event_name}: {e}");
            }
        }
    }

    /// Clear all scripts and handlers (for hot-reload).
    pub fn clear(&mut self) {
        self.scripts.clear();
        self.hotkey_handlers.clear();
        self.event_handlers.clear();
    }
}

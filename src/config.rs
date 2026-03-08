use arc_swap::{ArcSwap, Guard};
use bevy::ecs::resource::Resource;
use figment::{
    Figment,
    providers::{Env, Format, Toml as FigToml, Yaml as FigYaml},
};
use objc2_core_foundation::{CFData, CFString};
use regex::Regex;
use serde::{Deserialize, Deserializer, Serialize, de};
use std::{
    collections::HashMap,
    env,
    ffi::c_void,
    path::{Path, PathBuf},
    ptr::NonNull,
    sync::{Arc, LazyLock},
};
use stdext::function_name;
use tracing::{error, info, warn};

use crate::{
    commands::{Command, Direction, MouseMove, Operation},
    platform::{Modifiers, OSStatus},
};
use crate::{
    errors::{Error, Result},
    util::MacResult,
};
use crate::{platform::CFStringRef, util::AXUIWrapper};

/// A `LazyLock` that determines the path to the application's configuration file.
/// It checks the `AYATSURI_CONFIG` environment variable first, then standard XDG locations and user home directory.
/// If no configuration file is found, the application will panic.
pub static CONFIGURATION_FILE: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Ok(path_str) = env::var("AYATSURI_CONFIG") {
        let path = PathBuf::from(path_str);
        if path.exists() {
            return path;
        }
        warn!(
            "{}: $AYATSURI_CONFIG is set to {}, but the file does not exist. Falling back to default locations.",
            function_name!(),
            path.display()
        );
    }

    let standard_paths = [
        // YAML (preferred format)
        env::var("XDG_CONFIG_HOME")
            .ok()
            .map(|x| PathBuf::from(x).join("ayatsuri/ayatsuri.yaml")),
        env::var("XDG_CONFIG_HOME")
            .ok()
            .map(|x| PathBuf::from(x).join("ayatsuri/ayatsuri.yml")),
        env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".config/ayatsuri/ayatsuri.yaml")),
        env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".config/ayatsuri/ayatsuri.yml")),
        // TOML (backwards compatible)
        env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".ayatsuri")),
        env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join(".ayatsuri.toml")),
        env::var("XDG_CONFIG_HOME")
            .ok()
            .map(|x| PathBuf::from(x).join("ayatsuri/ayatsuri.toml")),
    ];

    standard_paths
        .into_iter()
        .flatten()
        .find(|path| path.exists())
        .unwrap_or_else(|| {
            panic!(
                "{}: Configuration file not found. Tried: $AYATSURI_CONFIG, $HOME/.ayatsuri, $HOME/.ayatsuri.toml, $XDG_CONFIG_HOME/ayatsuri/ayatsuri.toml",
                function_name!()
            )
        })
});

/// Parses a string into a `Direction` enum.
///
/// # Arguments
///
/// * `dir` - The string representation of the direction (e.g., "north", "west").
///
/// # Returns
///
/// `Ok(Direction)` if the string is a valid direction, otherwise `Err(Error::InvalidConfig)`.
fn parse_direction(dir: &str) -> Result<Direction> {
    Ok(match dir {
        "north" => Direction::North,
        "south" => Direction::South,
        "west" => Direction::West,
        "east" => Direction::East,
        "first" => Direction::First,
        "last" => Direction::Last,
        _ => {
            return Err(Error::InvalidConfig(format!(
                "{}: Unhandled direction {dir}",
                function_name!()
            )));
        }
    })
}

/// Parses a command argument vector into an `Operation` enum.
///
/// # Arguments
///
/// * `argv` - A slice of strings representing the command arguments (e.g., `["focus", "east"]`).
///
/// # Returns
///
/// `Ok(Operation)` if the arguments represent a valid operation, otherwise `Err(Error::InvalidConfig)`.
fn parse_operation(argv: &[&str]) -> Result<Operation> {
    let empty = "";
    let cmd = *argv.first().unwrap_or(&empty);
    let err = Error::InvalidConfig(format!("{}: Invalid command '{argv:?}'", function_name!()));

    let out = match cmd {
        "focus" => Operation::Focus(parse_direction(argv.get(1).ok_or(err)?)?),
        "focus_id" => {
            let id: i32 = argv
                .get(1)
                .ok_or_else(|| err.clone())?
                .parse()
                .map_err(|_| err.clone())?;
            Operation::FocusById(id)
        }
        "swap" => Operation::Swap(parse_direction(argv.get(1).ok_or(err)?)?),
        "center" => Operation::Center,
        "resize" => Operation::Resize,
        "resize_to" => {
            let w: i32 = argv.get(1).ok_or_else(|| err.clone())?.parse().map_err(|_| err.clone())?;
            let h: i32 = argv.get(2).ok_or_else(|| err.clone())?.parse().map_err(|_| err.clone())?;
            Operation::ResizeTo(w, h)
        }
        "fullwidth" => Operation::FullWidth,
        "manage" => Operation::Manage,
        "equalize" => Operation::Equalize,
        "stack" => Operation::Stack(true),
        "unstack" => Operation::Stack(false),
        "nextdisplay" => Operation::ToNextDisplay,
        "move_to" => {
            let x: i32 = argv.get(1).ok_or_else(|| err.clone())?.parse().map_err(|_| err.clone())?;
            let y: i32 = argv.get(2).ok_or_else(|| err.clone())?.parse().map_err(|_| err.clone())?;
            Operation::MoveTo(x, y)
        }
        _ => {
            return Err(err);
        }
    };
    Ok(out)
}

/// Parses a command argument vector into a `MouseMove` enum.
fn parse_mouse_move(argv: &[&str]) -> Result<MouseMove> {
    let empty = "";
    let cmd = *argv.first().unwrap_or(&empty);
    let err = Error::InvalidConfig(format!(
        "{}: Invalid mouse command '{argv:?}'",
        function_name!()
    ));

    let out = match cmd {
        "nextdisplay" => MouseMove::ToDisplay(crate::commands::Direction::East),
        _ => {
            return Err(err);
        }
    };
    Ok(out)
}

/// Parses a command argument vector into a `Command` enum.
///
/// # Arguments
///
/// * `argv` - A slice of strings representing the command arguments (e.g., `["window", "focus", "east"]`).
///
/// # Returns
///
/// `Ok(Command)` if the arguments represent a valid command, otherwise `Err(Error::InvalidConfig)`.
pub fn parse_command(argv: &[&str]) -> Result<Command> {
    let empty = "";
    let cmd = *argv.first().unwrap_or(&empty);

    let out = match cmd {
        "printstate" => Command::PrintState,
        "window" => Command::Window(parse_operation(&argv[1..])?),
        "mouse" => Command::Mouse(parse_mouse_move(&argv[1..])?),
        "quit" => Command::Quit,
        "exec" => Command::Exec(argv[1..].join(" ")),
        "mode" => {
            let mode = argv.get(1).unwrap_or(&"tiling");
            Command::SetMode((*mode).to_string())
        }
        "reload" => Command::ReloadConfig,
        _ => {
            return Err(Error::InvalidConfig(format!(
                "{}: Unhandled command '{argv:?}'",
                function_name!()
            )));
        }
    };
    Ok(out)
}

/// `Config` manages the application's configuration, including options, keybindings, and window-specific parameters.
/// It provides methods for loading, reloading, and querying configuration settings.
#[derive(Clone, Debug, Resource)]
pub struct Config {
    inner: Arc<ArcSwap<InnerConfig>>,
}

impl Config {
    /// Creates a new `Config` instance by loading the configuration from the specified path.
    ///
    /// # Arguments
    ///
    /// * `path` - A reference to the path of the configuration file.
    ///
    /// # Returns
    ///
    /// `Ok(Self)` if the configuration is loaded successfully, otherwise `Err(Error)` with an error message.
    pub fn new(path: &Path) -> Result<Self> {
        Ok(Config {
            inner: Arc::new(ArcSwap::from_pointee(InnerConfig::from_figment(path)?)),
        })
    }

    /// Reloads the configuration from the specified path, updating the internal options and keybindings.
    ///
    /// # Arguments
    ///
    /// * `path` - A reference to the path of the new configuration file.
    ///
    /// # Returns
    ///
    /// `Ok(())` if the configuration is reloaded successfully, otherwise `Err(Error)` with an error message.
    pub fn reload_config(&mut self, path: &Path) -> Result<()> {
        let new = InnerConfig::from_figment(path)?;
        self.inner.store(Arc::new(new));
        Ok(())
    }

    /// Returns a read guard to the inner `InnerConfig` for read-only access.
    ///
    /// # Returns
    ///
    /// A `Guard<Arc<InnerConfig>>` allowing read access to `InnerConfig`.
    fn inner(&self) -> Guard<Arc<InnerConfig>> {
        self.inner.load()
    }

    /// Returns a clone of the `MainOptions` from the current configuration.
    ///
    /// # Returns
    ///
    /// A `MainOptions` struct containing the main configuration options.
    pub fn options(&self) -> MainOptions {
        self.inner().options.clone()
    }

    /// Finds a keybinding matching the given `keycode` and `modifier` mask.
    ///
    /// # Arguments
    ///
    /// * `keycode` - The raw key code of the keybinding to find.
    /// * `mask` - The modifier mask (e.g., `Alt`, `Shift`, `Cmd`, `Ctrl`) of the keybinding.
    ///
    /// # Returns
    ///
    /// `Some(Command)` if a matching keybinding is found, otherwise `None`.
    pub fn find_keybind(&self, keycode: u8, mask: &Modifiers) -> Option<Command> {
        let config = self.inner();
        config
            .bindings
            .values()
            .flat_map(|binds| binds.all())
            .find_map(|bind| {
                (bind.code == keycode && bind.modifiers == *mask).then_some(bind.command.clone())
            })
    }

    /// Finds window properties for a given `title` and `bundle_id`.
    /// It iterates through configured window parameters and returns the first match.
    ///
    /// # Arguments
    ///
    /// * `title` - The title of the window to match.
    /// * `bundle_id` - The bundle identifier of the application owning the window.
    ///
    /// # Returns
    ///
    /// `Some(WindowParams)` if matching window properties are found, otherwise `None`.
    pub fn find_window_properties(&self, title: &str, bundle_id: &str) -> Vec<WindowParams> {
        self.inner()
            .windows
            .as_ref()
            .map(|windows| {
                windows
                    .values()
                    .filter(|params| {
                        let bundle_match =
                            params.bundle_id.as_ref().map(|id| id.as_str() == bundle_id);
                        bundle_match.is_none_or(|m| m) && params.title.is_match(title)
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    pub fn sliver_height(&self) -> f64 {
        self.options().sliver_height.unwrap_or(1.0).clamp(0.1, 1.0)
    }

    pub fn sliver_width(&self) -> i32 {
        i32::from(self.options().sliver_width.unwrap_or(5)).max(1)
    }

    pub fn edge_padding(&self) -> (i32, i32, i32, i32) {
        let o = self.options();
        (
            i32::from(o.padding_top.unwrap_or(0)),
            i32::from(o.padding_right.unwrap_or(0)),
            i32::from(o.padding_bottom.unwrap_or(0)),
            i32::from(o.padding_left.unwrap_or(0)),
        )
    }

    pub fn preset_column_widths(&self) -> Vec<f64> {
        self.options().preset_column_widths
    }

    pub fn continuous_swipe(&self) -> bool {
        self.options().continuous_swipe.unwrap_or(true)
    }

    pub fn swipe_gesture_direction(&self) -> SwipeGestureDirection {
        self.options()
            .swipe_gesture_direction
            .unwrap_or(SwipeGestureDirection::Natural)
    }
    pub fn dim_inactive_opacity(&self) -> f64 {
        self.options()
            .dim_inactive_windows
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }

    pub fn dim_inactive_color(&self) -> (f64, f64, f64) {
        self.options()
            .dim_inactive_color
            .as_deref()
            .map_or((0.0, 0.0, 0.0), parse_hex_color)
    }

    pub fn border_active_window(&self) -> bool {
        self.options().border_active_window.unwrap_or(false)
    }

    pub fn border_color(&self) -> (f64, f64, f64) {
        self.options()
            .border_color
            .as_deref()
            .map_or((1.0, 1.0, 1.0), parse_hex_color)
    }

    pub fn border_opacity(&self) -> f64 {
        self.options().border_opacity.unwrap_or(1.0).clamp(0.0, 1.0)
    }

    pub fn border_width(&self) -> f64 {
        self.options().border_width.unwrap_or(2.0).max(0.0)
    }

    pub fn border_radius(&self) -> f64 {
        self.options().border_radius.unwrap_or(10.0).max(0.0)
    }

    pub fn menubar_height(&self) -> Option<i32> {
        self.options().menubar_height.map(i32::from)
    }

    pub fn swipe_sensitivity(&self) -> f64 {
        self.options()
            .swipe_sensitivity
            .unwrap_or(0.35)
            .clamp(0.1, 2.0)
    }

    pub fn swipe_deceleration(&self) -> f64 {
        self.options()
            .swipe_deceleration
            .unwrap_or(4.0)
            .clamp(1.0, 10.0)
    }

    pub fn is_floating_mode(&self) -> bool {
        self.options().mode == WindowMode::Floating
    }

    pub fn enable_manage_toggle(&self) -> bool {
        self.options().enable_manage_toggle.unwrap_or(true)
    }

    pub fn edge_snap(&self) -> EdgeSnapConfig {
        self.options().edge_snap.clone()
    }

    pub fn edge_snap_threshold(&self) -> i32 {
        i32::from(self.options().edge_snap.threshold.unwrap_or(10)).max(1)
    }

    pub fn edge_snap_any_enabled(&self) -> bool {
        let s = self.options().edge_snap;
        s.left.unwrap_or(false)
            || s.right.unwrap_or(false)
            || s.top.unwrap_or(false)
            || s.bottom.unwrap_or(false)
            || s.fullscreen.unwrap_or(false)
    }

    pub fn edge_snap_preview_enabled(&self) -> bool {
        self.options().edge_snap.preview.unwrap_or(true) && self.edge_snap_any_enabled()
    }

    pub fn edge_snap_preview_opacity(&self) -> f64 {
        self.options().edge_snap.preview_opacity.unwrap_or(0.15)
    }

    pub fn edge_snap_sticky_dwell(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.options().edge_snap.sticky_dwell_ms.unwrap_or(300))
    }

    /// Returns true if any gesture suppression is enabled.
    pub fn suppress_gestures(&self) -> bool {
        let g = &self.options().gesture_suppress;
        g.four_finger.unwrap_or(false)
            || g.five_finger_pinch.unwrap_or(false)
            || g.five_finger_spread.unwrap_or(false)
    }

    pub fn startup_apps(&self) -> Vec<StartupApp> {
        self.options().startup
    }

    /// Build `SpringParams` from config (or defaults).
    pub fn spring_params(&self) -> crate::logic::spring::SpringParams {
        let s = &self.options().spring;
        crate::logic::spring::SpringParams {
            stiffness: s.stiffness.unwrap_or(800.0).max(1.0),
            damping_ratio: s.damping_ratio.unwrap_or(1.0).max(0.01),
            epsilon: s.epsilon.unwrap_or(0.5).max(0.01),
        }
    }

    /// How many frames the reload guard waits before firing a consolidated reshuffle.
    pub fn settle_frames(&self) -> u32 {
        self.options().animation.settle_frames.unwrap_or(2).max(1)
    }

    /// Display arrangement change poll interval. Used by MCP `get_config` and tests.
    /// Display plugin uses compile-time constant (Bevy `on_timer` limitation).
    #[allow(dead_code)]
    pub fn display_poll_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(
            self.options().display.change_poll_ms.unwrap_or(1000).max(100),
        )
    }

    /// Apply a JSON patch to the current config, merging new values into the existing options.
    /// This reuses the same ArcSwap mechanism as file-based hot-reload.
    pub fn apply_patch(&self, patch: &serde_json::Value) -> Result<()> {
        let inner = self.inner();
        // Serialize current options to JSON, merge patch, deserialize back.
        let mut current = serde_json::to_value(&inner.options)
            .map_err(|e| Error::InvalidConfig(format!("serialize current config: {e}")))?;
        json_merge(&mut current, patch);
        let patched: MainOptions = serde_json::from_value(current)
            .map_err(|e| Error::InvalidConfig(format!("invalid config patch: {e}")))?;
        let new_inner = InnerConfig {
            options: patched,
            bindings: inner.bindings.clone(),
            windows: inner.windows.clone(),
            scripting: inner.scripting.clone(),
            execs: inner.execs.clone(),
            system_defaults: inner.system_defaults.clone(),
        };
        self.inner.store(Arc::new(new_inner));
        Ok(())
    }

    /// Returns the full config options as a JSON value (for MCP get_full_config).
    pub fn options_json(&self) -> serde_json::Value {
        serde_json::to_value(&self.options()).unwrap_or_default()
    }

    /// Returns a merged map of all defaults to apply: explicit `system_defaults`
    /// entries combined with entries derived from `gesture_suppress`.
    pub fn merged_system_defaults(&self) -> HashMap<String, HashMap<String, DefaultsValue>> {
        let mut merged: HashMap<String, HashMap<String, DefaultsValue>> = self
            .inner()
            .system_defaults
            .clone()
            .unwrap_or_default();

        // Merge gesture_suppress entries into the map
        Self::merge_gesture_defaults(&self.options().gesture_suppress, &mut merged);
        merged
    }

    /// Converts `gesture_suppress` fields into concrete `system_defaults` entries
    /// for the trackpad and dock domains.
    fn merge_gesture_defaults(
        gestures: &GestureSuppress,
        map: &mut HashMap<String, HashMap<String, DefaultsValue>>,
    ) {
        const TRACKPAD_DOMAINS: &[&str] = &[
            "com.apple.AppleMultitouchTrackpad",
            "com.apple.driver.AppleBluetoothMultitouch.trackpad",
        ];

        if let Some(suppress) = gestures.four_finger {
            let value = if suppress { 0 } else { 2 };
            for domain in TRACKPAD_DOMAINS {
                let entry = map.entry((*domain).to_string()).or_default();
                entry
                    .entry("TrackpadFourFingerHorizSwipeGesture".into())
                    .or_insert(DefaultsValue::Int(value));
                entry
                    .entry("TrackpadFourFingerVertSwipeGesture".into())
                    .or_insert(DefaultsValue::Int(value));
                entry
                    .entry("TrackpadFourFingerPinchGesture".into())
                    .or_insert(DefaultsValue::Int(value));
            }
            if suppress {
                for domain in TRACKPAD_DOMAINS {
                    let entry = map.entry((*domain).to_string()).or_default();
                    entry
                        .entry("TrackpadThreeFingerDrag".into())
                        .or_insert(DefaultsValue::Int(0));
                    entry
                        .entry("TrackpadThreeFingerHorizSwipeGesture".into())
                        .or_insert(DefaultsValue::Int(2));
                }
            }
        }

        if let Some(suppress) = gestures.five_finger_pinch {
            let trackpad_value = if suppress { 0 } else { 2 };
            let dock_value = !suppress;
            for domain in TRACKPAD_DOMAINS {
                let entry = map.entry((*domain).to_string()).or_default();
                entry
                    .entry("TrackpadFiveFingerPinchGesture".into())
                    .or_insert(DefaultsValue::Int(trackpad_value));
            }
            let dock = map.entry("com.apple.dock".into()).or_default();
            dock.entry("showLaunchpadGestureEnabled".into())
                .or_insert(DefaultsValue::Bool(dock_value));
        }

        if let Some(suppress) = gestures.five_finger_spread {
            let dock_value = !suppress;
            let dock = map.entry("com.apple.dock".into()).or_default();
            dock.entry("showDesktopGestureEnabled".into())
                .or_insert(DefaultsValue::Bool(dock_value));
        }
    }
}

/// Deep-merge `patch` into `target`. Object keys are merged recursively;
/// non-object values are overwritten.
fn json_merge(target: &mut serde_json::Value, patch: &serde_json::Value) {
    use serde_json::Value;
    match (target, patch) {
        (Value::Object(t), Value::Object(p)) => {
            for (k, v) in p {
                json_merge(t.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (target, patch) => *target = patch.clone(),
    }
}

fn parse_hex_color(hex: &str) -> (f64, f64, f64) {
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if hex.len() != 6 {
        return (1.0, 1.0, 1.0);
    }
    let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(255);
    let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(255);
    let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(255);
    (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    )
}

impl Default for Config {
    /// Returns a default `Config` instance with an empty `InnerConfig`.
    fn default() -> Self {
        Config {
            inner: Arc::new(ArcSwap::from_pointee(InnerConfig::default())),
        }
    }
}

impl TryFrom<&str> for Config {
    type Error = crate::errors::Error;

    fn try_from(input: &str) -> std::result::Result<Self, Self::Error> {
        Ok(Config {
            inner: Arc::new(ArcSwap::from_pointee(InnerConfig::parse_config(input)?)),
        })
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(untagged)]
enum OneOrMore {
    Single(Keybinding),
    Multiple(Vec<Keybinding>),
}

impl OneOrMore {
    fn all(&self) -> Vec<&Keybinding> {
        match self {
            OneOrMore::Single(one) => vec![one],
            OneOrMore::Multiple(many) => many.iter().collect::<Vec<_>>(),
        }
    }

    fn all_mut(&mut self) -> Vec<&mut Keybinding> {
        match self {
            OneOrMore::Single(one) => vec![one],
            OneOrMore::Multiple(many) => many.iter_mut().collect::<Vec<_>>(),
        }
    }
}

/// `InnerConfig` holds the actual configuration data parsed from a file, including options, keybindings, and window parameters.
/// It is typically accessed via an `Arc<RwLock<InnerConfig>>` within the `Config` struct.
#[derive(Deserialize, Clone, Debug, Default)]
#[serde(default)]
struct InnerConfig {
    options: MainOptions,
    bindings: HashMap<String, OneOrMore>,
    windows: Option<HashMap<String, WindowParams>>,
    scripting: Option<ScriptingConfig>,
    /// Named shell commands referenced by `exec_<name>` bindings.
    execs: Option<HashMap<String, String>>,
    /// Arbitrary macOS `defaults write` entries applied at startup and hot-reload.
    /// Outer key = domain (e.g. "com.apple.dock"), inner key = preference key.
    system_defaults: Option<HashMap<String, HashMap<String, DefaultsValue>>>,
}

/// A typed value for the `system_defaults` config section.
/// Represents any value that can be written via `defaults write`.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(untagged)]
pub enum DefaultsValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<DefaultsValue>),
    Dict(HashMap<String, DefaultsValue>),
}

/// Configuration for the Rhai scripting engine.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct ScriptingConfig {
    /// Path to the init script (e.g., `~/.config/ayatsuri/init.rhai`).
    pub init_script: Option<String>,
    /// Directories to scan for additional .rhai scripts.
    #[serde(default)]
    pub script_dirs: Vec<String>,
    /// Enable hot-reload of scripts on file changes (default: true).
    pub hot_reload: Option<bool>,
}

impl InnerConfig {
    /// Loads configuration using figment's layered provider chain:
    /// defaults (serde) → environment variables → config file (TOML or YAML).
    fn from_figment(path: &Path) -> Result<InnerConfig> {
        let mut figment = Figment::new()
            .merge(Env::prefixed("AYATSURI_").split("__"));

        // Layer the config file on top (highest priority per the figment pattern).
        match path.extension().and_then(|e| e.to_str()) {
            Some("yaml" | "yml") => {
                figment = figment.merge(FigYaml::file(path));
            }
            _ => {
                figment = figment.merge(FigToml::file(path));
            }
        }

        let mut config: InnerConfig = figment
            .extract()
            .map_err(|e| Error::InvalidConfig(e.to_string()))?;
        config.resolve_keybindings()?;
        Ok(config)
    }

    /// Parses configuration from a TOML string (used in tests and legacy paths).
    fn parse_config(input: &str) -> Result<InnerConfig> {
        let mut config: InnerConfig = toml::from_str(input)?;
        config.resolve_keybindings()?;
        Ok(config)
    }

    /// Resolves keybinding codes and commands by looking up virtual keys and literal keycodes.
    fn resolve_keybindings(&mut self) -> Result<()> {
        let virtual_keys = generate_virtual_keymap();
        let execs = self.execs.clone();

        for (command, bindings) in &mut self.bindings {
            let argv = command.split('_').collect::<Vec<_>>();
            for binding in bindings.all_mut() {
                // exec_<name> bindings resolve the shell command from the [execs] map.
                if argv.first() == Some(&"exec") && argv.len() >= 2 {
                    let exec_name = argv[1..].join("_");
                    let shell_cmd = execs
                        .as_ref()
                        .and_then(|e| e.get(&exec_name))
                        .ok_or_else(|| {
                            Error::InvalidConfig(format!(
                                "exec binding '{command}' references unknown exec '{exec_name}'"
                            ))
                        })?
                        .clone();
                    binding.command = Command::Exec(shell_cmd);
                } else {
                    binding.command = parse_command(&argv)?;
                }

                let code = virtual_keys
                    .iter()
                    .find(|(key, _)| key == &binding.key)
                    .map(|(_, code)| *code)
                    .or_else(|| {
                        literal_keycode()
                            .find(|(key, _)| key == &binding.key)
                            .map(|(_, code)| *code)
                    });
                if let Some(code) = code {
                    binding.code = code;
                } else {
                    error!("{}: invalid key '{}'", function_name!(), &binding.key);
                }
                info!("bind: {binding:?}");
            }
        }
        Ok(())
    }
}

/// Window management mode: tiling (automatic column layout) or floating (free positioning).
#[derive(Clone, Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WindowMode {
    /// Windows are automatically arranged in a column layout.
    #[default]
    Tiling,
    /// Windows are free-positioned (standard macOS behavior).
    Floating,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum SwipeGestureDirection {
    Natural,
    Reversed,
}

/// Configuration for suppressing trackpad gestures.
/// Prevents macOS from acting on the specified gestures by swallowing
/// the events at the CGEventTap level.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct GestureSuppress {
    /// Suppress all 4-finger gestures (Space switching, etc.). Default: false.
    pub four_finger: Option<bool>,
    /// Suppress 5-finger pinch gesture (Launchpad). Default: false.
    pub five_finger_pinch: Option<bool>,
    /// Suppress 5-finger spread gesture (Show Desktop). Default: false.
    pub five_finger_spread: Option<bool>,
}

/// Configuration for edge snapping in floating mode.
/// Each field enables snapping to a specific screen zone when the cursor is near
/// that edge on mouse-up.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct EdgeSnapConfig {
    /// Snap to left half of screen.
    pub left: Option<bool>,
    /// Snap to right half of screen.
    pub right: Option<bool>,
    /// Snap to top half of screen.
    pub top: Option<bool>,
    /// Snap to bottom half of screen.
    pub bottom: Option<bool>,
    /// Snap to fullscreen.
    pub fullscreen: Option<bool>,
    /// Pixel distance from edge to trigger snap. Default: 10.
    pub threshold: Option<u16>,
    /// Show a translucent preview of the snap zone during drag. Default: true.
    pub preview: Option<bool>,
    /// Opacity of the snap preview fill (0.0–1.0). Default: 0.15.
    pub preview_opacity: Option<f64>,
    /// Dwell time (ms) cursor sticks at display edges during drag. Default: 300.
    /// Set to 0 to disable sticky edges.
    pub sticky_dwell_ms: Option<u64>,
}

/// Spring animation parameters. Controls how windows animate to their target positions.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct SpringConfig {
    /// How stiff the spring is. Higher = faster/snappier. Default: 800.0.
    pub stiffness: Option<f64>,
    /// Damping ratio. 1.0 = critically damped (fastest without overshoot).
    /// < 1.0 = bouncy. > 1.0 = mushy. Default: 1.0.
    pub damping_ratio: Option<f64>,
    /// Stop threshold in pixels. Animation snaps to target when displacement
    /// and velocity are both below this. Default: 0.5.
    pub epsilon: Option<f64>,
}

/// Animation timing parameters.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct AnimationConfig {
    /// Frames to wait for cascading events to settle before consolidated reshuffle.
    /// Default: 2.
    pub settle_frames: Option<u32>,
}

/// Display management parameters.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct DisplayConfig {
    /// How often to poll for display arrangement changes (milliseconds). Default: 1000.
    pub change_poll_ms: Option<u64>,
}

/// `MainOptions` represents the primary configuration options for the window manager.
/// These options control various behaviors such as mouse focus, gesture recognition, and window animation.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
pub struct MainOptions {
    /// Window management mode: "tiling" (default) or "floating".
    /// In floating mode all windows are free-positioned; the tiling layout is disabled.
    #[serde(default)]
    pub mode: WindowMode,
    /// Allow the `window_manage` keybinding to toggle individual windows between
    /// tiled and floating. When false the keybinding is ignored. Default: true.
    pub enable_manage_toggle: Option<bool>,
    /// Enables or disables focus follows mouse behavior.
    pub focus_follows_mouse: Option<bool>,
    /// Enables or disables mouse follows focus behavior.
    pub mouse_follows_focus: Option<bool>,
    /// The number of fingers required for swipe gestures to move windows.
    pub swipe_gesture_fingers: Option<usize>,
    /// Which direction swipe gestures should move windows.
    pub swipe_gesture_direction: Option<SwipeGestureDirection>,
    /// A list of preset column widths (as ratios) used for resizing windows.
    #[serde(default = "default_preset_column_widths")]
    pub preset_column_widths: Vec<f64>,
    /// The animation speed for window movements in pixels per second.
    pub animation_speed: Option<f64>,
    /// Automatically center the window when switching focus with keyboard.
    pub auto_center: Option<bool>,
    /// Height of off-screen window slivers as a ratio (0.0–1.0) of the display height.
    /// Lower values hide the window's corner radius at screen edges.
    /// Default: 1.0 (full height).
    pub sliver_height: Option<f64>,
    /// Width of off-screen window slivers in pixels.
    /// Default: 5 pixels.
    pub sliver_width: Option<u16>,
    /// Padding applied at screen edges (in pixels). Independent from between-window gaps.
    /// Default: 0 on all sides.
    pub padding_top: Option<u16>,
    pub padding_bottom: Option<u16>,
    pub padding_left: Option<u16>,
    pub padding_right: Option<u16>,
    /// Opacity of the dim overlay on inactive windows (0.0=off, 1.0=fully black).
    /// Default: 0.0 (disabled).
    pub dim_inactive_windows: Option<f64>,
    /// Hex color for the dim overlay, e.g. "#000000".
    /// Default: "#000000" (black).
    pub dim_inactive_color: Option<String>,
    /// Whether to draw a border around the active (focused) window.
    /// Default: false.
    pub border_active_window: Option<bool>,
    /// Hex color for the active window border, e.g. "#FF0000".
    /// Default: "#FFFFFF" (white).
    pub border_color: Option<String>,
    /// Opacity of the active window border (0.0–1.0).
    /// Default: 1.0.
    pub border_opacity: Option<f64>,
    /// Width of the active window border in pixels.
    /// Default: 2.0.
    pub border_width: Option<f64>,
    /// Corner radius of the active window border.
    /// Default: 10.0.
    pub border_radius: Option<f64>,
    /// Override the system menubar height (in pixels).
    /// When set, this value is used instead of the height reported by macOS.
    pub menubar_height: Option<u16>,

    /// Swiping keeps sliding windows until the first or last window.
    /// Set to false to clamp so edge windows stay on-screen. Default: true.
    pub continuous_swipe: Option<bool>,

    /// Swipe sensitivity multiplier. Lower values = less distance per finger
    /// movement. Range: 0.1–2.0. Default: 0.35.
    pub swipe_sensitivity: Option<f64>,

    /// Swipe inertia deceleration rate. Higher values = faster stop.
    /// Range: 1.0–10.0. Default: 4.0.
    pub swipe_deceleration: Option<f64>,

    /// Path to a wallpaper image applied on startup. Supports ~ expansion.
    /// When set, ayatsuri sets the desktop wallpaper on all screens at launch.
    pub wallpaper: Option<String>,

    /// Edge snapping configuration for floating mode.
    /// Dragging a window to a screen edge and releasing snaps it to fill that zone.
    #[serde(default)]
    pub edge_snap: EdgeSnapConfig,

    /// 5-finger gesture suppression.
    /// Prevents macOS Launchpad (pinch) and Show Desktop (spread) triggers.
    #[serde(default)]
    pub gesture_suppress: GestureSuppress,

    /// Applications to launch on startup, with optional delays.
    #[serde(default)]
    pub startup: Vec<StartupApp>,

    /// Spring animation parameters (stiffness, damping, epsilon).
    #[serde(default)]
    pub spring: SpringConfig,

    /// Animation timing parameters (settle_frames).
    #[serde(default)]
    pub animation: AnimationConfig,

    /// Display management parameters (change_poll_ms).
    #[serde(default)]
    pub display: DisplayConfig,
}

/// An application to launch on startup.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct StartupApp {
    /// Application name (passed to `open -a`).
    pub app: String,
    /// Delay in seconds before launching (default: 0).
    #[serde(default)]
    pub delay: f64,
}

/// Returns a default set of column widths.
pub fn default_preset_column_widths() -> Vec<f64> {
    vec![0.25, 0.33333, 0.50, 0.66667, 0.75]
}

/// `Keybinding` represents a keyboard shortcut and the command it triggers.
/// It includes the key, its raw keycode, modifier keys, and the associated command.
#[derive(Debug, Clone)]
pub struct Keybinding {
    pub key: String,
    pub code: u8,
    pub modifiers: Modifiers,
    pub command: Command,
}

impl<'de> Deserialize<'de> for Keybinding {
    /// Deserializes a `Keybinding` from a string input. The input string is expected to be in a format like "`modifier+modifier-key`" or "`key`".
    /// Examples: "`ctrl+alt-q`", "`shift-tab`", "`h`".
    ///
    /// # Arguments
    ///
    /// * `deserializer` - The deserializer used to parse the input.
    ///
    /// # Returns
    ///
    /// `Ok(Self)` if the deserialization is successful, otherwise `Err(D::Error)` with a custom error message.
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let input = String::deserialize(deserializer)?;
        let mut parts = input.split('-').map(str::trim).collect::<Vec<_>>();
        let Some(key) = parts.pop() else {
            return Err(de::Error::custom(format!("Empty keybinding: {input:?}")));
        };

        if parts.len() > 1 {
            return Err(de::Error::custom(format!("Too many dashes: {input:?}")));
        }

        let modifiers = match parts.pop() {
            Some(modifiers) => parse_modifiers(modifiers).map_err(de::Error::custom)?,
            None => Modifiers::empty(),
        };

        Ok(Keybinding {
            key: key.to_string(),
            code: 0,
            modifiers,
            command: Command::Quit,
        })
    }
}

/// `WindowParams` defines rules and properties for specific windows based on their title or bundle ID.
/// These parameters can override default window management behavior, such as forcing a window to float or setting its initial index.
#[derive(Clone, Debug, Deserialize)]
pub struct WindowParams {
    /// A regular expression to match against the window's title.
    #[serde(deserialize_with = "deserialize_title")]
    title: Regex,
    /// An optional bundle identifier to match against the application's bundle ID.
    bundle_id: Option<String>,
    /// If `true`, the window will be managed as a floating window (not tiled).
    pub floating: Option<bool>,
    /// An optional preferred index for the window's position in the window strip.
    pub index: Option<usize>,
    pub vertical_padding: Option<i32>,
    pub horizontal_padding: Option<i32>,
    pub dont_focus: Option<bool>,
    /// An optional initial width ratio (0.0–1.0) relative to the display width.
    /// Overrides the default column width when the window is first managed.
    pub width: Option<f64>,
    /// Grid placement for floating windows: "cols:rows:x:y:w:h".
    /// Divides the display into a grid and positions the window at the given cell/span.
    pub grid: Option<String>,
    /// Per-window override for the active window border corner radius.
    pub border_radius: Option<f64>,
}

impl WindowParams {
    /// Parses the grid string into `(x_ratio, y_ratio, w_ratio, h_ratio)`, all 0.0–1.0.
    pub fn grid_ratios(&self) -> Option<(f64, f64, f64, f64)> {
        let grid = self.grid.as_ref()?;
        let parts: Vec<f64> = grid.split(':').filter_map(|s| s.parse().ok()).collect();
        if parts.len() != 6 {
            return None;
        }
        let (cols, rows) = (parts[0], parts[1]);
        if cols <= 0.0 || rows <= 0.0 {
            return None;
        }
        Some((
            parts[2] / cols,
            parts[3] / rows,
            parts[4] / cols,
            parts[5] / rows,
        ))
    }
}

/// Deserializes a regular expression from a string for window titles.
fn deserialize_title<'de, D>(deserializer: D) -> std::result::Result<Regex, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Regex::new(&s).map_err(de::Error::custom)
}

/// Parses a string containing modifier names (e.g., "alt", "shift", "cmd", "ctrl") separated by "+", and returns their combined bitmask.
///
/// # Arguments
///
/// * `input` - The string containing modifier names (e.g., "ctrl+alt").
///
/// # Returns
///
/// `Ok(Modifiers)` with the combined modifier bitmask if parsing is successful, otherwise `Err(String)` with an error message for an invalid modifier.
fn parse_modifiers(input: &str) -> Result<Modifiers> {
    let mut out = Modifiers::empty();

    let modifiers = input.split('+').map(str::trim).collect::<Vec<_>>();
    for modifier in &modifiers {
        out |= match *modifier {
            "alt" => Modifiers::ALT,
            "shift" => Modifiers::SHIFT,
            "cmd" => Modifiers::CMD,
            "ctrl" => Modifiers::CTRL,
            _ => {
                return Err(Error::InvalidConfig(format!(
                    "{}: Invalid modifier: {modifier}",
                    function_name!()
                )));
            }
        }
    }
    Ok(out)
}

#[link(name = "Carbon", kind = "framework")]
unsafe extern "C" {
    /// Returns a reference to the currently selected keyboard layout input source that is ASCII-capable.
    ///
    /// # Returns
    ///
    /// A raw pointer to the `TISInputSourceRef` (a `c_void` pointer) if successful, otherwise `null_mut()`.
    fn TISCopyCurrentASCIICapableKeyboardLayoutInputSource() -> *mut c_void;

    /// Retrieves a specified property of an input source.
    ///
    /// # Arguments
    ///
    /// * `keyboard` - The raw pointer to the `TISInputSourceRef`.
    /// * `property` - The `CFStringRef` representing the property to retrieve (e.g., `kTISPropertyUnicodeKeyLayoutData`).
    ///
    /// # Returns
    ///
    /// A raw pointer to `CFData` containing the property value.
    fn TISGetInputSourceProperty(keyboard: *const c_void, property: CFStringRef) -> *mut CFData;

    /// Translates a virtual key code to a Unicode string according to the specified keyboard layout.
    ///
    /// # Arguments
    ///
    /// * `keyLayoutPtr` - A pointer to the keyboard layout data.
    /// * `virtualKeyCode` - The virtual key code to translate.
    /// * `keyAction` - The key action (e.g., `UCKeyAction::Down`).
    /// * `modifierKeyState` - The state of the modifier keys (e.g., `kUCKeyModifierAlphaLockBit`).
    /// * `keyboardType` - The type of keyboard, typically obtained from `LMGetKbdType()`.
    /// * `keyTranslateOptions` - Options for the translation process.
    /// * `deadKeyState` - A mutable reference to a `u32` representing the dead key state.
    /// * `maxStringLength` - The maximum length of the output Unicode string buffer.
    /// * `actualStringLength` - A mutable reference to an `isize` to store the actual length of the output Unicode string.
    /// * `unicodeString` - A mutable pointer to the buffer to store the resulting Unicode string.
    ///
    /// # Returns
    ///
    /// An `OSStatus` indicating success or failure.
    fn UCKeyTranslate(
        keyLayoutPtr: *mut u8,
        virtualKeyCode: u16,
        keyAction: u16,
        modifierKeyState: u32,
        keyboardType: u32,
        keyTranslateOptions: u32,
        deadKeyState: &mut u32,
        maxStringLength: usize,
        actualStringLength: &mut isize,
        unicodeString: *mut u16,
    ) -> OSStatus;

    /// Returns the keyboard type for the system.
    ///
    /// # Returns
    ///
    /// A `u8` representing the keyboard type.
    fn LMGetKbdType() -> u8;

    /// A constant `CFStringRef` representing the property key for Unicode keyboard layout data.
    static kTISPropertyUnicodeKeyLayoutData: CFStringRef;

}

/// Returns an iterator over static tuples of virtual key names and their corresponding keycodes.
/// These keycodes identify physical keys on an ANSI-standard US keyboard layout.
///
/// # Returns
///
/// An iterator yielding references to `(&'static str, u8)` tuples.
fn virtual_keycode() -> impl Iterator<Item = &'static (&'static str, u8)> {
    /*
     *  Summary:
     *    Virtual keycodes
     *
     *  Discussion:
     *    These constants are the virtual keycodes defined originally in
     *    Inside Mac Volume V, pg. V-191. They identify physical keys on a
     *    keyboard. Those constants with "ANSI" in the name are labeled
     *    according to the key position on an ANSI-standard US keyboard.
     *    For example, kVK_ANSI_A indicates the virtual keycode for the key
     *    with the letter 'A' in the US keyboard layout. Other keyboard
     *    layouts may have the 'A' key label on a different physical key;
     *    in this case, pressing 'A' will generate a different virtual
     *    keycode.
     */
    static VIRTUAL_KEYCODE: LazyLock<Vec<(&'static str, u8)>> = LazyLock::new(|| {
        vec![
            ("a", 0x00),
            ("s", 0x01),
            ("d", 0x02),
            ("f", 0x03),
            ("h", 0x04),
            ("g", 0x05),
            ("z", 0x06),
            ("x", 0x07),
            ("c", 0x08),
            ("v", 0x09),
            ("section", 0x0a), // iso keyboards only.
            ("b", 0x0b),
            ("q", 0x0c),
            ("w", 0x0d),
            ("e", 0x0e),
            ("r", 0x0f),
            ("y", 0x10),
            ("t", 0x11),
            ("1", 0x12),
            ("2", 0x13),
            ("3", 0x14),
            ("4", 0x15),
            ("6", 0x16),
            ("5", 0x17),
            ("equal", 0x18),
            ("9", 0x19),
            ("7", 0x1a),
            ("minus", 0x1b),
            ("8", 0x1c),
            ("0", 0x1d),
            ("rightbracket", 0x1e),
            ("o", 0x1f),
            ("u", 0x20),
            ("leftbracket", 0x21),
            ("i", 0x22),
            ("p", 0x23),
            ("l", 0x25),
            ("j", 0x26),
            ("quote", 0x27),
            ("k", 0x28),
            ("semicolon", 0x29),
            ("backslash", 0x2a),
            ("comma", 0x2b),
            ("slash", 0x2c),
            ("n", 0x2d),
            ("m", 0x2e),
            ("period", 0x2f),
            ("grave", 0x32),
            ("keypaddecimal", 0x41),
            ("keypadmultiply", 0x43),
            ("keypadplus", 0x45),
            ("keypadclear", 0x47),
            ("keypaddivide", 0x4b),
            ("keypadenter", 0x4c),
            ("keypadminus", 0x4e),
            ("keypadequals", 0x51),
            ("keypad0", 0x52),
            ("keypad1", 0x53),
            ("keypad2", 0x54),
            ("keypad3", 0x55),
            ("keypad4", 0x56),
            ("keypad5", 0x57),
            ("keypad6", 0x58),
            ("keypad7", 0x59),
            ("keypad8", 0x5b),
            ("keypad9", 0x5c),
        ]
    });
    VIRTUAL_KEYCODE.iter()
}

/// Returns an iterator over static tuples of literal key names and their corresponding keycodes.
/// These keycodes are for keys that are independent of the keyboard layout (e.g., Return, Tab, Space).
///
/// # Returns
///
/// An iterator yielding references to `(&'static str, u8)` tuples.
fn literal_keycode() -> impl Iterator<Item = &'static (&'static str, u8)> {
    /* keycodes for keys that are independent of keyboard layout*/
    static LITERAL_KEYCODE: LazyLock<Vec<(&'static str, u8)>> = LazyLock::new(|| {
        vec![
            ("return", 0x24),
            ("tab", 0x30),
            ("space", 0x31),
            ("delete", 0x33),
            ("escape", 0x35),
            ("command", 0x37),
            ("shift", 0x38),
            ("capslock", 0x39),
            ("option", 0x3a),
            ("control", 0x3b),
            ("rightcommand", 0x36),
            ("rightshift", 0x3c),
            ("rightoption", 0x3d),
            ("rightcontrol", 0x3e),
            ("function", 0x3f),
            ("f17", 0x40),
            ("volumeup", 0x48),
            ("volumedown", 0x49),
            ("mute", 0x4a),
            ("f18", 0x4f),
            ("f19", 0x50),
            ("f20", 0x5a),
            ("f5", 0x60),
            ("f6", 0x61),
            ("f7", 0x62),
            ("f3", 0x63),
            ("f8", 0x64),
            ("f9", 0x65),
            ("f11", 0x67),
            ("f13", 0x69),
            ("f16", 0x6a),
            ("f14", 0x6b),
            ("f10", 0x6d),
            ("contextualmenu", 0x6e),
            ("f12", 0x6f),
            ("f15", 0x71),
            ("help", 0x72),
            ("home", 0x73),
            ("pageup", 0x74),
            ("forwarddelete", 0x75),
            ("f4", 0x76),
            ("end", 0x77),
            ("f2", 0x78),
            ("pagedown", 0x79),
            ("f1", 0x7a),
            ("leftarrow", 0x7b),
            ("rightarrow", 0x7c),
            ("downarrow", 0x7d),
            ("uparrow", 0x7e),
        ]
    });
    LITERAL_KEYCODE.iter()
}

/// Represents the action of a key, used in `UCKeyTranslate`.
enum UCKeyAction {
    /// The key is going down.
    Down = 0, // key is going down
              /*
              Up = 1,      // key is going up
              AutoKey = 2, // auto-key down
              Display = 3, // get information for key display (as in Key Caps)
              */
}

/// Generates a vector of (`key_name`, keycode) tuples for virtual keys based on the current ASCII-capable keyboard layout.
/// This involves using macOS Carbon API functions to translate virtual keycodes to Unicode characters.
///
/// # Returns
///
/// A `Vec<(String, u8)>` containing the translated key names and their keycodes. Returns an empty vector if an error occurs during keyboard layout fetching.
fn generate_virtual_keymap() -> Vec<(String, u8)> {
    let keyboard = AXUIWrapper::from_retained(unsafe {
        TISCopyCurrentASCIICapableKeyboardLayoutInputSource()
    })
    .ok();
    let keyboard_layout = keyboard
        .and_then(|keyboard| {
            NonNull::new(unsafe {
                TISGetInputSourceProperty(
                    keyboard.as_ptr::<c_void>(),
                    kTISPropertyUnicodeKeyLayoutData,
                )
            })
        })
        .and_then(|uchr| NonNull::new(unsafe { CFData::byte_ptr(uchr.as_ref()).cast_mut() }));
    let Some(keyboard_layout) = keyboard_layout else {
        error!(
            "{}: problem fetching current virtual keyboard layout.",
            function_name!()
        );
        return vec![];
    };

    let mut state = 0u32;
    let mut chars = vec![0u16; 256];
    let mut got: isize = 0;
    virtual_keycode()
        .filter_map(|(_, keycode)| {
            unsafe {
                UCKeyTranslate(
                    keyboard_layout.as_ptr(),
                    (*keycode).into(),
                    UCKeyAction::Down as u16,
                    0,
                    LMGetKbdType().into(),
                    1,
                    &mut state,
                    chars.len(),
                    &mut got,
                    chars.as_mut_ptr(),
                )
            }
            .to_result(function_name!())
            .ok()
            .map(|()| {
                let name = unsafe { CFString::with_characters(None, chars.as_ptr(), got) }
                    .map(|chars| chars.to_string());
                name.zip(Some(*keycode))
            })
        })
        .flatten()
        .collect()
}

#[test]
#[allow(clippy::float_cmp)]
fn test_config_parsing() {
    let input = r#"
[options]
focus_follows_mouse = true

[bindings]
quit = "ctrl+alt-q"
window_manage = "ctrl+alt-t"
window_stack = ["ctrl-s", "alt-s"]

[windows]

[windows.pip]
title = "picture.*picture"
bundle_id = "com.something.apple"
floating = true
index = 1
"#;
    let config = Config {
        inner: Arc::new(ArcSwap::from_pointee(
            InnerConfig::parse_config(input).expect("Failed to parse config"),
        )),
    };
    let find_key = |k| {
        virtual_keycode()
            .find_map(|(s, v)| (format!("{k}") == *s).then_some(*v))
            .unwrap()
    };

    assert_eq!(config.inner().options.focus_follows_mouse, Some(true));

    // Modifiers: alt = 1<<0, ctrl = 1<<3.
    let keycode = find_key('q');
    assert!(matches!(
        config.find_keybind(keycode, &(Modifiers::ALT | Modifiers::CTRL)),
        Some(Command::Quit)
    ));

    let keycode = find_key('t');
    assert!(matches!(
        config.find_keybind(keycode, &(Modifiers::ALT | Modifiers::CTRL)),
        Some(Command::Window(Operation::Manage))
    ));

    let keycode = find_key('s');
    assert!(matches!(
        config.find_keybind(keycode, &Modifiers::CTRL),
        Some(Command::Window(Operation::Stack(true)))
    ));

    assert!(matches!(
        config.find_keybind(keycode, &Modifiers::ALT),
        Some(Command::Window(Operation::Stack(true)))
    ));

    let props = config.find_window_properties("picture in picture", "com.something.apple");
    assert_eq!(props[0].floating, Some(true));
    assert_eq!(props[0].index, Some(1));

    let defaults = Config::default();
    assert_eq!(defaults.swipe_sensitivity(), 0.35);
    assert_eq!(defaults.swipe_deceleration(), 4.0);
}

#[test]
#[allow(clippy::float_cmp)]
fn test_grid_ratios() {
    use regex::Regex;

    let make = |grid: Option<&str>| WindowParams {
        title: Regex::new(".*").unwrap(),
        bundle_id: None,
        floating: None,
        index: None,
        vertical_padding: None,
        horizontal_padding: None,
        dont_focus: None,
        width: None,
        grid: grid.map(Into::into),
        border_radius: None,
    };

    // Standard 2x2 grid, cell (1,1), span 1x1 → bottom-right quarter.
    assert_eq!(
        make(Some("2:2:1:1:1:1")).grid_ratios(),
        Some((0.5, 0.5, 0.5, 0.5))
    );

    // 3x3 grid, cell (0,0), span 2x1 → top-left, 2/3 width, 1/3 height.
    assert_eq!(
        make(Some("3:3:0:0:2:1")).grid_ratios(),
        Some((0.0, 0.0, 2.0 / 3.0, 1.0 / 3.0))
    );

    // Full screen: 1x1 grid, cell (0,0), span 1x1.
    assert_eq!(
        make(Some("1:1:0:0:1:1")).grid_ratios(),
        Some((0.0, 0.0, 1.0, 1.0))
    );

    // Invalid: too few parts.
    assert_eq!(make(Some("2:2:1:1")).grid_ratios(), None);

    // Invalid: zero columns.
    assert_eq!(make(Some("0:2:0:0:1:1")).grid_ratios(), None);

    // No grid set.
    assert_eq!(make(None).grid_ratios(), None);
}

#[test]
fn test_parse_hex_color_valid() {
    assert_eq!(
        parse_hex_color("#89b4fa"),
        (
            f64::from(0x89) / 255.0,
            f64::from(0xb4) / 255.0,
            f64::from(0xfa) / 255.0
        )
    );
    assert_eq!(parse_hex_color("#000000"), (0.0, 0.0, 0.0));
    assert_eq!(parse_hex_color("#FFFFFF"), (1.0, 1.0, 1.0));
    assert_eq!(parse_hex_color("#FF0000"), (1.0, 0.0, 0.0));
}

#[test]
fn test_parse_hex_color_no_hash() {
    assert_eq!(
        parse_hex_color("89b4fa"),
        (
            f64::from(0x89) / 255.0,
            f64::from(0xb4) / 255.0,
            f64::from(0xfa) / 255.0
        )
    );
    assert_eq!(parse_hex_color("FF0000"), (1.0, 0.0, 0.0));
}

#[test]
fn test_parse_hex_color_invalid_length() {
    // Short strings fall back to white.
    assert_eq!(parse_hex_color("#FFF"), (1.0, 1.0, 1.0));
    assert_eq!(parse_hex_color(""), (1.0, 1.0, 1.0));
    assert_eq!(parse_hex_color("#FF"), (1.0, 1.0, 1.0));
}

#[test]
fn test_parse_hex_color_malformed_hex() {
    // Non-hex digits fall back to 255 per channel.
    assert_eq!(parse_hex_color("ZZZZZZ"), (1.0, 1.0, 1.0));
    assert_eq!(parse_hex_color("GG0000"), (1.0, 0.0, 0.0));
}

#[test]
#[allow(clippy::float_cmp)]
fn test_config_defaults() {
    let config = Config::default();
    assert_eq!(config.dim_inactive_opacity(), 0.0);
    assert_eq!(config.dim_inactive_color(), (0.0, 0.0, 0.0));
    assert!(!config.border_active_window());
    assert_eq!(config.border_color(), (1.0, 1.0, 1.0));
    assert_eq!(config.border_opacity(), 1.0);
    assert_eq!(config.border_width(), 2.0);
    assert_eq!(config.border_radius(), 10.0);
    assert_eq!(config.menubar_height(), None);
}

#[test]
#[allow(clippy::float_cmp)]
fn test_spring_config_defaults() {
    let config = Config::default();
    let params = config.spring_params();
    assert_eq!(params.stiffness, 800.0);
    assert_eq!(params.damping_ratio, 1.0);
    assert_eq!(params.epsilon, 0.5);
}

#[test]
#[allow(clippy::float_cmp)]
fn test_spring_config_from_toml() {
    let input = r#"
[options]
[options.spring]
stiffness = 1200.0
damping_ratio = 0.8
epsilon = 1.0
"#;
    let config = Config::try_from(input).unwrap();
    let params = config.spring_params();
    assert_eq!(params.stiffness, 1200.0);
    assert_eq!(params.damping_ratio, 0.8);
    assert_eq!(params.epsilon, 1.0);
}

#[test]
fn test_settle_frames_default() {
    let config = Config::default();
    assert_eq!(config.settle_frames(), 2);
}

#[test]
fn test_settle_frames_from_config() {
    let input = r#"
[options]
[options.animation]
settle_frames = 5
"#;
    let config = Config::try_from(input).unwrap();
    assert_eq!(config.settle_frames(), 5);
}

#[test]
fn test_display_poll_interval_default() {
    let config = Config::default();
    assert_eq!(
        config.display_poll_interval(),
        std::time::Duration::from_millis(1000)
    );
}

#[test]
fn test_display_poll_interval_min_clamp() {
    let input = r#"
[options]
[options.display]
change_poll_ms = 10
"#;
    let config = Config::try_from(input).unwrap();
    // Should be clamped to min 100ms
    assert_eq!(
        config.display_poll_interval(),
        std::time::Duration::from_millis(100)
    );
}

#[test]
fn test_apply_patch_changes_mode() {
    let config = Config::default();
    assert!(!config.is_floating_mode());

    let patch = serde_json::json!({"mode": "floating"});
    config.apply_patch(&patch).unwrap();
    assert!(config.is_floating_mode());
}

#[test]
fn test_apply_patch_changes_spring() {
    let config = Config::default();
    let patch = serde_json::json!({"spring": {"stiffness": 1500.0}});
    config.apply_patch(&patch).unwrap();
    let params = config.spring_params();
    assert!((params.stiffness - 1500.0).abs() < f64::EPSILON);
    // Other spring params should remain default
    assert!((params.damping_ratio - 1.0).abs() < f64::EPSILON);
}

#[test]
fn test_apply_patch_nested_merge() {
    let config = Config::default();

    // First patch sets left padding
    let patch1 = serde_json::json!({"padding_left": 10});
    config.apply_patch(&patch1).unwrap();
    assert_eq!(config.edge_padding().3, 10);

    // Second patch sets right padding — left should remain
    let patch2 = serde_json::json!({"padding_right": 20});
    config.apply_patch(&patch2).unwrap();
    assert_eq!(config.edge_padding().3, 10);
    assert_eq!(config.edge_padding().1, 20);
}

#[test]
fn test_apply_patch_invalid_json_returns_error() {
    let config = Config::default();
    let patch = serde_json::json!({"mode": 42}); // mode expects string
    assert!(config.apply_patch(&patch).is_err());
}

#[test]
fn test_options_json_roundtrip() {
    let config = Config::default();
    let json = config.options_json();
    assert!(json.is_object());
    assert!(json.get("mode").is_some());
    assert!(json.get("spring").is_some());
}

#[test]
fn test_parse_command_all_operations() {
    assert!(matches!(
        parse_command(&["window", "focus", "east"]),
        Ok(Command::Window(Operation::Focus(Direction::East)))
    ));
    assert!(matches!(
        parse_command(&["window", "swap", "west"]),
        Ok(Command::Window(Operation::Swap(Direction::West)))
    ));
    assert!(matches!(
        parse_command(&["window", "center"]),
        Ok(Command::Window(Operation::Center))
    ));
    assert!(matches!(
        parse_command(&["window", "resize"]),
        Ok(Command::Window(Operation::Resize))
    ));
    assert!(matches!(
        parse_command(&["window", "fullwidth"]),
        Ok(Command::Window(Operation::FullWidth))
    ));
    assert!(matches!(
        parse_command(&["window", "manage"]),
        Ok(Command::Window(Operation::Manage))
    ));
    assert!(matches!(
        parse_command(&["window", "equalize"]),
        Ok(Command::Window(Operation::Equalize))
    ));
    assert!(matches!(
        parse_command(&["window", "stack"]),
        Ok(Command::Window(Operation::Stack(true)))
    ));
    assert!(matches!(
        parse_command(&["window", "unstack"]),
        Ok(Command::Window(Operation::Stack(false)))
    ));
    assert!(matches!(
        parse_command(&["window", "nextdisplay"]),
        Ok(Command::Window(Operation::ToNextDisplay))
    ));
    assert!(matches!(
        parse_command(&["quit"]),
        Ok(Command::Quit)
    ));
    assert!(matches!(
        parse_command(&["printstate"]),
        Ok(Command::PrintState)
    ));
}

#[test]
fn test_parse_command_new_operations() {
    assert!(matches!(
        parse_command(&["window", "focus_id", "42"]),
        Ok(Command::Window(Operation::FocusById(42)))
    ));
    assert!(matches!(
        parse_command(&["window", "move_to", "100", "200"]),
        Ok(Command::Window(Operation::MoveTo(100, 200)))
    ));
    assert!(matches!(
        parse_command(&["window", "resize_to", "800", "600"]),
        Ok(Command::Window(Operation::ResizeTo(800, 600)))
    ));
    assert!(matches!(
        parse_command(&["mode", "floating"]),
        Ok(Command::SetMode(_))
    ));
    assert!(matches!(
        parse_command(&["reload"]),
        Ok(Command::ReloadConfig)
    ));
}

#[test]
fn test_parse_command_exec() {
    let result = parse_command(&["exec", "open", "-a", "Safari"]);
    assert!(matches!(result, Ok(Command::Exec(cmd)) if cmd == "open -a Safari"));
}

#[test]
fn test_parse_command_invalid() {
    assert!(parse_command(&["nonexistent"]).is_err());
    assert!(parse_command(&["window", "invalid"]).is_err());
    assert!(parse_command(&["window", "focus"]).is_err()); // missing direction
}

#[test]
fn test_parse_direction_all() {
    assert!(matches!(parse_direction("north"), Ok(Direction::North)));
    assert!(matches!(parse_direction("south"), Ok(Direction::South)));
    assert!(matches!(parse_direction("east"), Ok(Direction::East)));
    assert!(matches!(parse_direction("west"), Ok(Direction::West)));
    assert!(matches!(parse_direction("first"), Ok(Direction::First)));
    assert!(matches!(parse_direction("last"), Ok(Direction::Last)));
    assert!(parse_direction("invalid").is_err());
}

#[test]
fn test_parse_direction_case_sensitive() {
    assert!(parse_direction("North").is_err());
    assert!(parse_direction("EAST").is_err());
    assert!(parse_direction("").is_err());
    assert!(parse_direction("northwest").is_err());
}

#[test]
fn test_parse_modifiers_single() {
    let m = parse_modifiers("ctrl").unwrap();
    assert!(m.contains(Modifiers::CTRL));
    assert!(!m.contains(Modifiers::ALT));
}

#[test]
fn test_parse_modifiers_combined() {
    let m = parse_modifiers("ctrl+alt").unwrap();
    assert!(m.contains(Modifiers::CTRL));
    assert!(m.contains(Modifiers::ALT));
    assert!(!m.contains(Modifiers::SHIFT));
}

#[test]
fn test_parse_modifiers_all() {
    let m = parse_modifiers("ctrl+alt+shift+cmd").unwrap();
    assert!(m.contains(Modifiers::CTRL));
    assert!(m.contains(Modifiers::ALT));
    assert!(m.contains(Modifiers::SHIFT));
    assert!(m.contains(Modifiers::CMD));
}

#[test]
fn test_parse_modifiers_invalid() {
    assert!(parse_modifiers("super").is_err());
    assert!(parse_modifiers("ctrl+meta").is_err());
    assert!(parse_modifiers("").is_err());
}

#[test]
fn test_parse_operation_missing_args() {
    assert!(parse_operation(&["focus"]).is_err());
    assert!(parse_operation(&["swap"]).is_err());
    assert!(parse_operation(&["resize_to", "800"]).is_err());
    assert!(parse_operation(&["move_to", "100"]).is_err());
    assert!(parse_operation(&["focus_id"]).is_err());
}

#[test]
fn test_parse_operation_invalid_numeric_args() {
    assert!(parse_operation(&["focus_id", "abc"]).is_err());
    assert!(parse_operation(&["resize_to", "abc", "600"]).is_err());
    assert!(parse_operation(&["resize_to", "800", "abc"]).is_err());
    assert!(parse_operation(&["move_to", "abc", "200"]).is_err());
}

#[test]
fn test_parse_operation_empty() {
    assert!(parse_operation(&[]).is_err());
}

#[test]
fn test_parse_command_empty() {
    assert!(parse_command(&[]).is_err());
}

#[test]
fn test_parse_mouse_move_invalid() {
    assert!(parse_command(&["mouse", "invalid"]).is_err());
    assert!(parse_command(&["mouse"]).is_err());
}

#[test]
fn test_json_merge() {
    let mut target = serde_json::json!({"a": 1, "b": {"c": 2, "d": 3}});
    let patch = serde_json::json!({"b": {"c": 99}, "e": 5});
    json_merge(&mut target, &patch);
    assert_eq!(target["a"], 1);
    assert_eq!(target["b"]["c"], 99);
    assert_eq!(target["b"]["d"], 3);
    assert_eq!(target["e"], 5);
}

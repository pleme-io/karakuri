use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use tracing::{error, info, warn};

use super::engine::ScriptEngine;

/// Discover the script directories for Karakuri.
/// Returns (init_script_path, script_dirs).
pub fn discover_script_paths() -> (Option<PathBuf>, Vec<PathBuf>) {
    let config_dir = env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            env::var("HOME")
                .map(|h| PathBuf::from(h).join(".config"))
                .unwrap_or_default()
        })
        .join("karakuri");

    let init_script = config_dir.join("init.rhai");
    let script_dir = config_dir.join("scripts");

    let init = if init_script.exists() {
        Some(init_script)
    } else {
        None
    };

    let dirs = if script_dir.is_dir() {
        vec![script_dir]
    } else {
        vec![]
    };

    (init, dirs)
}

/// Load init script and all scripts from script directories into the engine.
pub fn load_scripts(engine: &mut ScriptEngine) {
    let (init_script, script_dirs) = discover_script_paths();

    if let Some(init_path) = init_script {
        load_script_file(engine, &init_path);
    }

    for dir in &script_dirs {
        load_script_dir(engine, dir);
    }
}

/// Load a single Rhai script file into the engine.
pub fn load_script_file(engine: &mut ScriptEngine, path: &Path) {
    match fs::read_to_string(path) {
        Ok(source) => {
            let name = path.display().to_string();
            info!("loading script: {name}");
            if let Err(e) = engine.eval_script(&name, &source) {
                error!("failed to load script {name}: {e}");
            }
        }
        Err(e) => {
            warn!("cannot read script {}: {e}", path.display());
        }
    }
}

/// Load all .rhai files from a directory into the engine.
fn load_script_dir(engine: &mut ScriptEngine, dir: &Path) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("cannot read script dir {}: {e}", dir.display());
            return;
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "rhai")
        })
        .collect();

    // Load in sorted order for determinism.
    paths.sort();

    for path in paths {
        load_script_file(engine, &path);
    }
}

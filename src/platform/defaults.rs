use std::collections::HashMap;
use std::process::Command;

use tracing::{debug, error, info};

use crate::config::{Config, DefaultsValue};

/// Apply all system defaults on startup.
/// Merges explicit `system_defaults` config with entries derived from `gesture_suppress`.
pub fn apply_system_defaults(config: &Config) {
    let merged = config.merged_system_defaults();
    if merged.is_empty() {
        return;
    }

    let mut changed_domains: Vec<&str> = Vec::new();

    for (domain, keys) in &merged {
        for (key, value) in keys {
            write_default(domain, key, value);
        }
        changed_domains.push(domain);
    }

    restart_affected_services(&changed_domains);
}

/// Diff two merged default maps and apply only changed keys.
/// Avoids unnecessary service restarts on hot-reload.
pub fn diff_and_apply(
    old: &HashMap<String, HashMap<String, DefaultsValue>>,
    new: &HashMap<String, HashMap<String, DefaultsValue>>,
) {
    let mut changed_domains: Vec<String> = Vec::new();

    // Apply new/changed keys
    for (domain, new_keys) in new {
        let old_keys = old.get(domain);
        for (key, new_value) in new_keys {
            let changed = match old_keys.and_then(|ok| ok.get(key)) {
                Some(old_value) => old_value != new_value,
                None => true,
            };
            if changed {
                write_default(domain, key, new_value);
                if !changed_domains.contains(domain) {
                    changed_domains.push(domain.clone());
                }
            }
        }
    }

    // Keys removed (present in old but not in new) — we don't delete them
    // since `defaults delete` could remove user-set values. Removal is a no-op.

    restart_affected_services(
        &changed_domains.iter().map(String::as_str).collect::<Vec<_>>(),
    );
}

/// Write a single default value using the `defaults` CLI.
fn write_default(domain: &str, key: &str, value: &DefaultsValue) {
    let domain_arg = if domain == "NSGlobalDomain" {
        "-globalDomain".to_string()
    } else {
        domain.to_string()
    };

    let args = match value {
        DefaultsValue::Bool(b) => {
            let bool_str = if *b { "TRUE" } else { "FALSE" };
            vec![
                "write".into(),
                domain_arg,
                key.into(),
                "-bool".into(),
                bool_str.into(),
            ]
        }
        DefaultsValue::Int(i) => vec![
            "write".into(),
            domain_arg,
            key.into(),
            "-int".into(),
            i.to_string(),
        ],
        DefaultsValue::Float(f) => vec![
            "write".into(),
            domain_arg,
            key.into(),
            "-float".into(),
            f.to_string(),
        ],
        DefaultsValue::String(s) => vec![
            "write".into(),
            domain_arg,
            key.into(),
            "-string".into(),
            s.clone(),
        ],
        DefaultsValue::Array(items) => {
            let mut args = vec![
                "write".into(),
                domain_arg,
                key.into(),
                "-array".into(),
            ];
            for item in items {
                flatten_value_args(item, &mut args);
            }
            args
        }
        DefaultsValue::Dict(entries) => {
            let mut args = vec![
                "write".into(),
                domain_arg,
                key.into(),
                "-dict".into(),
            ];
            for (k, v) in entries {
                args.push(k.clone());
                flatten_value_args(v, &mut args);
            }
            args
        }
    };

    debug!("defaults {}", args.join(" "));
    let result = Command::new("defaults")
        .args(&args)
        .output();

    match result {
        Ok(output) if !output.status.success() => {
            error!(
                "defaults {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => error!("failed to run defaults command: {e}"),
        _ => {}
    }
}

/// Flatten a `DefaultsValue` into CLI args for array/dict items.
fn flatten_value_args(value: &DefaultsValue, args: &mut Vec<String>) {
    match value {
        DefaultsValue::Bool(b) => {
            args.push("-bool".into());
            args.push(if *b { "TRUE".into() } else { "FALSE".into() });
        }
        DefaultsValue::Int(i) => {
            args.push("-int".into());
            args.push(i.to_string());
        }
        DefaultsValue::Float(f) => {
            args.push("-float".into());
            args.push(f.to_string());
        }
        DefaultsValue::String(s) => {
            args.push("-string".into());
            args.push(s.clone());
        }
        // Nested arrays/dicts are not supported by `defaults write` CLI — skip
        DefaultsValue::Array(_) | DefaultsValue::Dict(_) => {
            error!("nested array/dict not supported in defaults write CLI args");
        }
    }
}

/// Restart macOS services that need a kick after their domain is modified.
fn restart_affected_services(domains: &[&str]) {
    let needs_dock = domains.contains(&"com.apple.dock");
    let needs_finder = domains.contains(&"com.apple.finder");
    let needs_sysui = domains.contains(&"com.apple.menuextra.clock")
        || domains.contains(&"com.apple.controlcenter");

    if needs_dock {
        info!("restarting Dock to apply defaults");
        let _ = Command::new("killall").arg("Dock").output();
    }
    if needs_finder {
        info!("restarting Finder to apply defaults");
        let _ = Command::new("killall").arg("Finder").output();
    }
    if needs_sysui {
        info!("restarting SystemUIServer to apply defaults");
        let _ = Command::new("killall").arg("SystemUIServer").output();
    }
}

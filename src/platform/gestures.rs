use std::process::Command;

use tracing::{debug, error, info};

use crate::config::GestureSuppress;

/// Apply macOS system preferences to suppress or restore trackpad gestures.
///
/// CGEventTap interception cannot suppress system-level gestures (4-finger swipe,
/// 5-finger pinch/spread) because WindowServer handles them before they reach
/// the event tap. Instead, we modify the relevant `defaults` domains directly.
///
/// Called on startup and on config hot-reload.
pub fn apply_gesture_preferences(gestures: &GestureSuppress) {
    let mut dock_changed = false;

    // 4-finger gestures: horizontal swipe, vertical swipe, pinch
    if let Some(suppress) = gestures.four_finger {
        let value = if suppress { "0" } else { "2" };

        for domain in TRACKPAD_DOMAINS {
            defaults_write_int(domain, "TrackpadFourFingerHorizSwipeGesture", value);
            defaults_write_int(domain, "TrackpadFourFingerVertSwipeGesture", value);
            defaults_write_int(domain, "TrackpadFourFingerPinchGesture", value);
        }

        // When suppressing 4-finger, ensure 3-finger drag is OFF so that
        // 3-finger horizontal swipe can switch spaces instead of dragging windows.
        if suppress {
            for domain in TRACKPAD_DOMAINS {
                defaults_write_int(domain, "TrackpadThreeFingerDrag", "0");
                // Ensure 3-finger swipe is set to switch spaces (2)
                defaults_write_int(domain, "TrackpadThreeFingerHorizSwipeGesture", "2");
            }
        }
    }

    // 5-finger pinch (Launchpad) — must set BOTH trackpad and Dock domains
    if let Some(suppress) = gestures.five_finger_pinch {
        let trackpad_value = if suppress { "0" } else { "2" };
        let dock_value = if suppress { "false" } else { "true" };

        for domain in TRACKPAD_DOMAINS {
            defaults_write_int(domain, "TrackpadFiveFingerPinchGesture", trackpad_value);
        }
        defaults_write_bool("com.apple.dock", "showLaunchpadGestureEnabled", dock_value);
        dock_changed = true;
    }

    // 5-finger spread (Show Desktop) — Dock domain
    if let Some(suppress) = gestures.five_finger_spread {
        let dock_value = if suppress { "false" } else { "true" };
        defaults_write_bool("com.apple.dock", "showDesktopGestureEnabled", dock_value);
        dock_changed = true;
    }

    // Restart Dock so it picks up the new preferences immediately
    if dock_changed {
        info!("restarting Dock to apply gesture preferences");
        let _ = Command::new("killall").arg("Dock").output();
    }
}

const TRACKPAD_DOMAINS: &[&str] = &[
    "com.apple.AppleMultitouchTrackpad",
    "com.apple.driver.AppleBluetoothMultitouch.trackpad",
];

fn defaults_write_int(domain: &str, key: &str, value: &str) {
    debug!("defaults write {domain} {key} -int {value}");
    let result = Command::new("defaults")
        .args(["write", domain, key, "-int", value])
        .output();
    match result {
        Ok(output) if !output.status.success() => {
            error!(
                "defaults write {domain} {key} -int {value} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => error!("failed to run defaults command: {e}"),
        _ => {}
    }
}

fn defaults_write_bool(domain: &str, key: &str, value: &str) {
    debug!("defaults write {domain} {key} -bool {value}");
    let result = Command::new("defaults")
        .args(["write", domain, key, "-bool", value])
        .output();
    match result {
        Ok(output) if !output.status.success() => {
            error!(
                "defaults write {domain} {key} -bool {value} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Err(e) => error!("failed to run defaults command: {e}"),
        _ => {}
    }
}

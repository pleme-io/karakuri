use std::process::Command;

use tracing::{debug, error};

use crate::config::GestureSuppress;

/// Apply macOS system preferences to suppress or restore trackpad gestures.
///
/// CGEventTap interception cannot suppress system-level gestures (4-finger swipe,
/// 5-finger pinch/spread) because WindowServer handles them before they reach
/// the event tap. Instead, we modify the relevant `defaults` domains directly.
///
/// Called on startup and on config hot-reload.
pub fn apply_gesture_preferences(gestures: &GestureSuppress) {
    // 4-finger horizontal swipe (Mission Control space switching)
    //   Domain: com.apple.AppleMultitouchTrackpad
    //   Key:    TrackpadFourFingerHorizSwipeGesture
    //   Values: 0 = off, 2 = switch spaces
    if let Some(suppress) = gestures.four_finger {
        let value = if suppress { "0" } else { "2" };
        defaults_write_int(
            "com.apple.AppleMultitouchTrackpad",
            "TrackpadFourFingerHorizSwipeGesture",
            value,
        );
        // Also set the Bluetooth trackpad domain for external trackpads
        defaults_write_int(
            "com.apple.driver.AppleBluetoothMultitouch.trackpad",
            "TrackpadFourFingerHorizSwipeGesture",
            value,
        );

        // 4-finger vertical swipe (Mission Control / App Exposé)
        //   Key: TrackpadFourFingerVertSwipeGesture
        //   Values: 0 = off, 2 = on
        let vert_value = if suppress { "0" } else { "2" };
        defaults_write_int(
            "com.apple.AppleMultitouchTrackpad",
            "TrackpadFourFingerVertSwipeGesture",
            vert_value,
        );
        defaults_write_int(
            "com.apple.driver.AppleBluetoothMultitouch.trackpad",
            "TrackpadFourFingerVertSwipeGesture",
            vert_value,
        );

        // 4-finger pinch (Launchpad via 4 fingers)
        //   Key: TrackpadFourFingerPinchGesture
        //   Values: 0 = off, 2 = on
        let pinch_value = if suppress { "0" } else { "2" };
        defaults_write_int(
            "com.apple.AppleMultitouchTrackpad",
            "TrackpadFourFingerPinchGesture",
            pinch_value,
        );
        defaults_write_int(
            "com.apple.driver.AppleBluetoothMultitouch.trackpad",
            "TrackpadFourFingerPinchGesture",
            pinch_value,
        );
    }

    // 5-finger pinch (Launchpad)
    //   Domain: com.apple.dock
    //   Key:    showLaunchpadGestureEnabled
    //   Values: -bool true/false
    if let Some(suppress) = gestures.five_finger_pinch {
        let value = if suppress { "false" } else { "true" };
        defaults_write_bool("com.apple.dock", "showLaunchpadGestureEnabled", value);
    }

    // 5-finger spread (Show Desktop)
    //   Domain: com.apple.dock
    //   Key:    showDesktopGestureEnabled
    //   Values: -bool true/false
    if let Some(suppress) = gestures.five_finger_spread {
        let value = if suppress { "false" } else { "true" };
        defaults_write_bool("com.apple.dock", "showDesktopGestureEnabled", value);
    }
}

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

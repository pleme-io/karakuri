use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSWorkspace};
use objc2_foundation::{NSDictionary, NSString, NSURL};
use tracing::debug;

/// Expand `~` to `$HOME` in a path string.
fn expand_tilde(path: &str) -> String {
    if path.starts_with('~')
        && let Ok(home) = std::env::var("HOME")
    {
        return path.replacen('~', &home, 1);
    }
    path.to_string()
}

/// Get the main thread marker, assuming we're on the main thread.
fn mtm() -> MainThreadMarker {
    MainThreadMarker::new().expect("wallpaper APIs must be called from the main thread")
}

/// Set the desktop wallpaper on all connected screens.
pub fn set_wallpaper_all(path: &str) -> Result<(), String> {
    let expanded = expand_tilde(path);
    let url = file_url(&expanded);
    let workspace = NSWorkspace::sharedWorkspace();
    let options = NSDictionary::new();
    let screens = NSScreen::screens(mtm());
    let count = screens.count();

    if count == 0 {
        return Err("no screens found".into());
    }

    for i in 0..count {
        let screen = screens.objectAtIndex(i);
        unsafe {
            workspace
                .setDesktopImageURL_forScreen_options_error(&url, &screen, &options)
                .map_err(|e| format!("setDesktopImageURL failed: {e}"))?;
        }
    }

    debug!("wallpaper set on all {count} screen(s): {expanded}");
    Ok(())
}

/// Set the desktop wallpaper on a specific screen by index (0-based).
pub fn set_wallpaper_for_screen(path: &str, screen_index: usize) -> Result<(), String> {
    let expanded = expand_tilde(path);
    let url = file_url(&expanded);
    let workspace = NSWorkspace::sharedWorkspace();
    let options = NSDictionary::new();
    let screens = NSScreen::screens(mtm());
    let count = screens.count();

    if screen_index >= count {
        return Err(format!("screen index {screen_index} out of range (have {count})"));
    }

    let screen = screens.objectAtIndex(screen_index);
    unsafe {
        workspace
            .setDesktopImageURL_forScreen_options_error(&url, &screen, &options)
            .map_err(|e| format!("setDesktopImageURL failed: {e}"))?;
    }

    debug!("wallpaper set on screen {screen_index}: {expanded}");
    Ok(())
}

/// Get the current wallpaper path for the main screen.
pub fn get_wallpaper() -> Result<String, String> {
    get_wallpaper_for_screen(0)
}

/// Get the current wallpaper path for a specific screen by index.
pub fn get_wallpaper_for_screen(screen_index: usize) -> Result<String, String> {
    let workspace = NSWorkspace::sharedWorkspace();
    let screens = NSScreen::screens(mtm());
    let count = screens.count();

    if screen_index >= count {
        return Err(format!("screen index {screen_index} out of range (have {count})"));
    }

    let screen = screens.objectAtIndex(screen_index);
    let url = workspace
        .desktopImageURLForScreen(&screen)
        .ok_or("no wallpaper URL for screen")?;

    url.path()
        .map(|p| p.to_string())
        .ok_or_else(|| "wallpaper URL has no path".into())
}

/// Create an `NSURL` from an absolute file path.
fn file_url(path: &str) -> objc2::rc::Retained<NSURL> {
    let ns_path = NSString::from_str(path);
    NSURL::fileURLWithPath(&ns_path)
}

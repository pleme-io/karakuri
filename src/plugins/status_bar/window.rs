use objc2::{MainThreadMarker, MainThreadOnly};
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSFloatingWindowLevel, NSScreen, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectView, NSWindow, NSWindowCollectionBehavior,
    NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use tracing::{error, info};

use super::config::BarEdge;

/// Manages the native NSWindow for the status bar.
///
/// Must live on the main thread (`NonSend` resource in Bevy).
pub struct StatusBarWindow {
    mtm: MainThreadMarker,
    window: Option<Retained<NSWindow>>,
    blur_view: Option<Retained<NSVisualEffectView>>,
}

#[allow(dead_code)] // Phase 2 will use hide/show/destroy/ns_window/height/is_visible.
impl StatusBarWindow {
    pub fn new(mtm: MainThreadMarker) -> Self {
        Self {
            mtm,
            window: None,
            blur_view: None,
        }
    }

    /// Create and show the status bar window on the primary display.
    ///
    /// The window is a borderless, non-activating panel at the top (or bottom)
    /// of the screen with an `NSVisualEffectView` for background blur.
    pub fn create(
        &mut self,
        height: f64,
        edge: &BarEdge,
        blur_radius: u16,
        bg_r: f64,
        bg_g: f64,
        bg_b: f64,
        bg_a: f64,
    ) {
        // Get primary screen bounds
        let Some(screen) = NSScreen::mainScreen(self.mtm) else {
            error!("status_bar: no main screen found");
            return;
        };
        let screen_frame = screen.frame();

        let y = match edge {
            BarEdge::Top => screen_frame.origin.y + screen_frame.size.height - height,
            BarEdge::Bottom => screen_frame.origin.y,
        };

        let frame = NSRect::new(
            NSPoint::new(screen_frame.origin.x, y),
            NSSize::new(screen_frame.size.width, height),
        );

        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(self.mtm),
                frame,
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };

        // Configure window behavior
        window.setOpaque(false);
        window.setHasShadow(false);
        // Float above normal windows (same level as overlays)
        window.setLevel(NSFloatingWindowLevel);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::Stationary
                | NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::IgnoresCycle,
        );
        // Accept mouse events for click tracking
        window.setIgnoresMouseEvents(false);
        window.setAcceptsMouseMovedEvents(true);

        // Background: either blurred vibrancy or solid color
        if blur_radius > 0 {
            let content_rect = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(frame.size.width, frame.size.height),
            );

            let effect_view = NSVisualEffectView::initWithFrame(
                NSVisualEffectView::alloc(self.mtm),
                content_rect,
            );
            effect_view.setMaterial(NSVisualEffectMaterial::HUDWindow);
            effect_view.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
            effect_view.setState(
                objc2_app_kit::NSVisualEffectState::Active,
            );

            let Some(content_view) = window.contentView() else {
                error!("status_bar: window has no content view");
                return;
            };
            content_view.addSubview(&effect_view);
            self.blur_view = Some(effect_view);

            // Tint: set window background to semi-transparent color on top of blur
            let bg_color = NSColor::colorWithSRGBRed_green_blue_alpha(bg_r, bg_g, bg_b, bg_a);
            window.setBackgroundColor(Some(&bg_color));
        } else {
            let bg_color = NSColor::colorWithSRGBRed_green_blue_alpha(bg_r, bg_g, bg_b, bg_a);
            window.setBackgroundColor(Some(&bg_color));
        }

        window.orderFrontRegardless();
        info!(
            "status_bar: created window at ({}, {}), size {}x{}",
            frame.origin.x, frame.origin.y, frame.size.width, frame.size.height
        );

        self.window = Some(window);
    }

    /// Hide the status bar window.
    pub fn hide(&self) {
        if let Some(ref window) = self.window {
            window.orderOut(None);
        }
    }

    /// Show the status bar window.
    pub fn show(&self) {
        if let Some(ref window) = self.window {
            window.orderFrontRegardless();
        }
    }

    /// Destroy the status bar window.
    pub fn destroy(&mut self) {
        if let Some(ref window) = self.window {
            window.orderOut(None);
            window.close();
        }
        self.window = None;
        self.blur_view = None;
    }

    /// Returns the NSWindow if it exists.
    pub fn ns_window(&self) -> Option<&NSWindow> {
        self.window.as_deref()
    }

    /// Returns the bar width (screen width).
    pub fn width(&self) -> f64 {
        self.window
            .as_ref()
            .map(|w| w.frame().size.width as f64)
            .unwrap_or(0.0)
    }

    /// Returns the bar height.
    pub fn height(&self) -> f64 {
        self.window
            .as_ref()
            .map(|w| w.frame().size.height as f64)
            .unwrap_or(0.0)
    }

    /// Returns true if the window exists and is visible.
    pub fn is_visible(&self) -> bool {
        self.window.as_ref().is_some_and(|w| w.isVisible())
    }

    /// Lock focus on the content view, call the draw closure, then unlock.
    ///
    /// Returns `true` if drawing happened, `false` if focus couldn't be acquired.
    /// The closure receives the bar height for coordinate calculations.
    ///
    /// Phase 2: Replace lockFocus with a custom NSView subclass implementing drawRect:.
    #[allow(deprecated)] // lockFocusIfCanDraw/unlockFocus work; drawRect: subclass is Phase 2.
    pub fn lock_and_draw(&self, draw_fn: impl FnOnce(f64)) -> bool {
        let Some(ref window) = self.window else {
            return false;
        };
        let Some(content_view) = window.contentView() else {
            return false;
        };
        // lockFocusIfCanDraw sets up the NSGraphicsContext for immediate drawing.
        let locked = content_view.lockFocusIfCanDraw();
        if !locked {
            return false;
        }
        let bar_height = window.frame().size.height;
        draw_fn(bar_height);
        content_view.unlockFocus();
        true
    }
}

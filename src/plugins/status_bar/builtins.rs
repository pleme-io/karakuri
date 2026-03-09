//! Built-in status bar widgets.
//!
//! Each widget implements `BuiltinWidget` and is registered in `ALL_BUILTINS`.
//! The spawn system auto-creates entities for builtins not already defined
//! in the user's config. The update system calls `update()` each frame.
//!
//! To add a new built-in widget:
//! 1. Create a struct implementing `BuiltinWidget`
//! 2. Add it to `ALL_BUILTINS`
//! That's it — no other wiring needed.

use super::components::{
    ArgbColor, BarItemComponent, BarItemState, BarItemType, BarPosition,
};

/// Trait for built-in status bar widgets.
///
/// Implementing this trait is all that's needed to add a new widget.
/// The status bar systems handle spawning, layout, and rendering automatically.
pub trait BuiltinWidget: Send + Sync {
    /// Unique identifier matching config item IDs (e.g., "clock", "front_app").
    fn id(&self) -> &str;

    /// Default bar position.
    fn position(&self) -> BarPosition;

    /// Default sort order (higher = further from edge).
    fn order(&self) -> u32;

    /// Default subscribed events (empty = timer-only or manual).
    fn subscriptions(&self) -> Vec<String> {
        Vec::new()
    }

    /// Update the item's label/icon. Called every frame when the bar is visible.
    /// Return `Some(new_label)` to update, `None` to keep current.
    fn update(&self, ctx: &WidgetContext<'_>) -> Option<String>;
}

/// Context passed to widget `update()` for reading ECS state.
pub struct WidgetContext<'a> {
    /// The focused window's parent application bundle ID (if any).
    pub focused_app_bundle_id: Option<&'a str>,
}

impl<'a> WidgetContext<'a> {
    /// Build context with a pre-resolved bundle ID.
    pub fn with_bundle_id(bundle_id: Option<&'a str>) -> Self {
        Self {
            focused_app_bundle_id: bundle_id,
        }
    }
}

// ── Built-in widgets ────────────────────────────────────────────────

/// Clock widget — shows current time.
pub struct ClockWidget;

impl BuiltinWidget for ClockWidget {
    fn id(&self) -> &str {
        "clock"
    }

    fn position(&self) -> BarPosition {
        BarPosition::Right
    }

    fn order(&self) -> u32 {
        999
    }

    fn update(&self, _ctx: &WidgetContext<'_>) -> Option<String> {
        Some(current_time_label())
    }
}

/// Front app widget — shows the currently focused application name.
pub struct FrontAppWidget;

impl BuiltinWidget for FrontAppWidget {
    fn id(&self) -> &str {
        "front_app"
    }

    fn position(&self) -> BarPosition {
        BarPosition::Left
    }

    fn order(&self) -> u32 {
        0
    }

    fn subscriptions(&self) -> Vec<String> {
        vec!["front_app_switched".to_string()]
    }

    fn update(&self, ctx: &WidgetContext<'_>) -> Option<String> {
        let bundle_id = ctx.focused_app_bundle_id.unwrap_or("");
        let short_name = bundle_id.rsplit('.').next().unwrap_or(bundle_id);
        Some(short_name.to_string())
    }
}

/// All registered built-in widgets.
///
/// To add a new built-in, just append to this array.
pub static ALL_BUILTINS: &[&dyn BuiltinWidget] = &[&ClockWidget, &FrontAppWidget];

/// Create the default `BarItemComponent` for a built-in widget.
pub fn builtin_component(widget: &dyn BuiltinWidget) -> BarItemComponent {
    BarItemComponent {
        id: widget.id().to_string(),
        item_type: BarItemType::Item,
        position: widget.position(),
        order: widget.order(),
        script: None,
        click_script: None,
        update_freq: 0,
        subscriptions: widget.subscriptions(),
        drawing: true,
        updates_when_shown: false,
        ignore_association: false,
        display_filter: None,
        space_filter: Vec::new(),
    }
}

/// Create the default `BarItemState` for a built-in widget.
pub fn builtin_state(label_color: ArgbColor) -> BarItemState {
    BarItemState {
        label_color,
        ..BarItemState::default()
    }
}

/// Returns the current time formatted as "HH:MM".
fn current_time_label() -> String {
    let now = std::time::SystemTime::now();
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // UTC time — Phase 2 will add timezone support via chrono/time crate.
    let total_minutes = secs / 60;
    let hours = (total_minutes / 60) % 24;
    let minutes = total_minutes % 60;
    format!("{hours:02}:{minutes:02}")
}

use std::collections::VecDeque;

use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use serde::{Deserialize, Serialize};

/// Position on the bar where an item is placed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BarPosition {
    #[default]
    Left,
    Center,
    Right,
    /// Left of notch (MacBook).
    Q,
    /// Right of notch (MacBook).
    E,
}

/// Type of bar item.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BarItemType {
    #[default]
    Item,
    Space,
    Bracket,
    Graph,
    Slider,
    Alias,
}

/// ARGB color parsed from hex string like "0xAARRGGBB" or "#RRGGBB".
#[derive(Clone, Debug, PartialEq)]
pub struct ArgbColor {
    pub a: f64,
    pub r: f64,
    pub g: f64,
    pub b: f64,
}

#[allow(dead_code)] // Utility methods used by Phase 2 animation/render systems.
impl ArgbColor {
    pub const TRANSPARENT: Self = Self {
        a: 0.0,
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };

    pub const WHITE: Self = Self {
        a: 1.0,
        r: 1.0,
        g: 1.0,
        b: 1.0,
    };

    pub const BLACK: Self = Self {
        a: 1.0,
        r: 0.0,
        g: 0.0,
        b: 0.0,
    };

    /// Parse from "0xAARRGGBB" or "#RRGGBB" hex string.
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches("0x").trim_start_matches('#');
        match hex.len() {
            8 => {
                let val = u32::from_str_radix(hex, 16).ok()?;
                Some(Self {
                    a: f64::from((val >> 24) & 0xFF) / 255.0,
                    r: f64::from((val >> 16) & 0xFF) / 255.0,
                    g: f64::from((val >> 8) & 0xFF) / 255.0,
                    b: f64::from(val & 0xFF) / 255.0,
                })
            }
            6 => {
                let val = u32::from_str_radix(hex, 16).ok()?;
                Some(Self {
                    a: 1.0,
                    r: f64::from((val >> 16) & 0xFF) / 255.0,
                    g: f64::from((val >> 8) & 0xFF) / 255.0,
                    b: f64::from(val & 0xFF) / 255.0,
                })
            }
            _ => None,
        }
    }

    /// Linearly interpolate between self and other.
    pub fn lerp(&self, other: &Self, t: f64) -> Self {
        let t = t.clamp(0.0, 1.0);
        Self {
            a: self.a + (other.a - self.a) * t,
            r: self.r + (other.r - self.r) * t,
            g: self.g + (other.g - self.g) * t,
            b: self.b + (other.b - self.b) * t,
        }
    }

    /// Returns true if the color is effectively invisible.
    pub fn is_transparent(&self) -> bool {
        self.a < 0.001
    }
}

impl Default for ArgbColor {
    fn default() -> Self {
        Self::WHITE
    }
}

// ── Core item components ────────────────────────────────────────────

/// Core identity and config for a bar item (ECS component).
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Fields consumed by Phase 2 systems (scripts, events, IPC).
pub struct BarItemComponent {
    /// Unique identifier (e.g., "clock", "battery", "spaces.1").
    pub id: String,
    /// Item type.
    pub item_type: BarItemType,
    /// Position on the bar.
    pub position: BarPosition,
    /// Sort order within position group (lower = closer to edge).
    pub order: u32,
    /// Shell script to run on update events.
    pub script: Option<String>,
    /// Shell script triggered on click.
    pub click_script: Option<String>,
    /// Update frequency in seconds (0 = event-driven only).
    pub update_freq: u32,
    /// Events this item subscribes to.
    pub subscriptions: Vec<String>,
    /// Whether the item is drawn (visible).
    pub drawing: bool,
    /// When scripts execute: true = always, false = only when visible.
    pub updates_when_shown: bool,
    /// Override space/display associations.
    pub ignore_association: bool,
    /// Display filter for this item.
    pub display_filter: Option<String>,
    /// Space indices this item appears on.
    pub space_filter: Vec<u32>,
}

/// Mutable visual state of a bar item (ECS component).
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Fields consumed by Phase 2 render/animation systems.
pub struct BarItemState {
    /// Icon text (glyph, emoji, SF Symbol).
    pub icon: String,
    /// Label text.
    pub label: String,
    /// Icon color.
    pub icon_color: ArgbColor,
    /// Label color.
    pub label_color: ArgbColor,
    /// Icon highlight color (when highlighted).
    pub icon_highlight_color: ArgbColor,
    /// Label highlight color (when highlighted).
    pub label_highlight_color: ArgbColor,
    /// Whether highlight mode is active.
    pub highlight: bool,
    /// Icon font spec override ("Family:Style:Size").
    pub icon_font: Option<String>,
    /// Label font spec override.
    pub label_font: Option<String>,
    /// Background color.
    pub background_color: ArgbColor,
    /// Background border color.
    pub border_color: ArgbColor,
    /// Background border width.
    pub border_width: f64,
    /// Background corner radius.
    pub corner_radius: f64,
    /// Background clip (transparent hole, 0.0–1.0).
    pub background_clip: f64,
    /// Per-item blur radius (0 = inherit from bar).
    pub blur_radius: u16,
    /// Whether the item is hidden.
    pub hidden: bool,
    /// Text alignment: 0=left, 1=center, 2=right.
    pub align: u8,
    /// Vertical offset for this item.
    pub y_offset: f64,
}

impl Default for BarItemState {
    fn default() -> Self {
        Self {
            icon: String::new(),
            label: String::new(),
            icon_color: ArgbColor::WHITE,
            label_color: ArgbColor::WHITE,
            icon_highlight_color: ArgbColor::WHITE,
            label_highlight_color: ArgbColor::WHITE,
            highlight: false,
            icon_font: None,
            label_font: None,
            background_color: ArgbColor::TRANSPARENT,
            border_color: ArgbColor::TRANSPARENT,
            border_width: 0.0,
            corner_radius: 0.0,
            background_clip: 0.0,
            blur_radius: 0,
            hidden: false,
            align: 0,
            y_offset: 0.0,
        }
    }
}

impl BarItemState {
    /// Returns the effective icon color (highlight or normal).
    pub fn effective_icon_color(&self) -> &ArgbColor {
        if self.highlight {
            &self.icon_highlight_color
        } else {
            &self.icon_color
        }
    }

    /// Returns the effective label color (highlight or normal).
    pub fn effective_label_color(&self) -> &ArgbColor {
        if self.highlight {
            &self.label_highlight_color
        } else {
            &self.label_color
        }
    }
}

/// Computed geometry of a bar item (ECS component, set by layout system).
#[derive(Component, Clone, Debug, Default)]
pub struct BarItemGeometry {
    /// X position in bar-local coordinates.
    pub x: f64,
    /// Y position in bar-local coordinates.
    pub y: f64,
    /// Total width (icon + label + padding).
    pub width: f64,
    /// Total height.
    pub height: f64,
    /// Left padding.
    pub padding_left: f64,
    /// Right padding.
    pub padding_right: f64,
}

// ── Shadow component ────────────────────────────────────────────────

/// Shadow properties for a bar item.
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 render system.
pub struct ShadowState {
    /// Whether shadow is drawn.
    pub drawing: bool,
    /// Shadow color.
    pub color: ArgbColor,
    /// Shadow angle in degrees (0 = right, 90 = up, 270 = down).
    pub angle: f64,
    /// Shadow distance in points.
    pub distance: f64,
}

impl Default for ShadowState {
    fn default() -> Self {
        Self {
            drawing: false,
            color: ArgbColor {
                a: 0.5,
                r: 0.0,
                g: 0.0,
                b: 0.0,
            },
            angle: 270.0,
            distance: 2.0,
        }
    }
}

// ── Graph component ─────────────────────────────────────────────────

/// State for graph-type bar items (rolling line chart).
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 graph rendering.
pub struct GraphData {
    /// Circular buffer of data points (0.0–1.0).
    pub data: VecDeque<f64>,
    /// Maximum number of data points.
    pub capacity: usize,
    /// Line color.
    pub line_color: ArgbColor,
    /// Fill color (below line).
    pub fill_color: ArgbColor,
    /// Line width.
    pub line_width: f64,
}

impl GraphData {
    pub fn new(capacity: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(capacity),
            capacity,
            line_color: ArgbColor::WHITE,
            fill_color: ArgbColor::TRANSPARENT,
            line_width: 1.0,
        }
    }

    /// Push a new data point (clamped to 0.0–1.0).
    pub fn push(&mut self, value: f64) {
        if self.data.len() >= self.capacity {
            self.data.pop_front();
        }
        self.data.push_back(value.clamp(0.0, 1.0));
    }
}

// ── Slider component ────────────────────────────────────────────────

/// State for slider-type bar items (interactive progress bar).
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 slider rendering.
pub struct SliderState {
    /// Current percentage (0.0–100.0).
    pub percentage: f64,
    /// Track color.
    pub track_color: ArgbColor,
    /// Knob color.
    pub knob_color: ArgbColor,
    /// Highlight/fill color.
    pub highlight_color: ArgbColor,
    /// Slider width in points.
    pub width: f64,
    /// Whether the user is currently dragging the knob.
    pub dragging: bool,
}

impl Default for SliderState {
    fn default() -> Self {
        Self {
            percentage: 0.0,
            track_color: ArgbColor {
                a: 0.3,
                r: 1.0,
                g: 1.0,
                b: 1.0,
            },
            knob_color: ArgbColor::WHITE,
            highlight_color: ArgbColor::WHITE,
            width: 60.0,
            dragging: false,
        }
    }
}

// ── Space component ─────────────────────────────────────────────────

/// State for space-type bar items (workspace indicators).
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 space system.
pub struct SpaceState {
    /// Space index this item represents.
    pub space_id: u32,
    /// Whether this space is currently active/selected.
    pub selected: bool,
    /// Color when selected.
    pub selected_color: ArgbColor,
    /// Display ID this space belongs to.
    pub display_id: u32,
}

// ── Bracket component ───────────────────────────────────────────────

/// State for bracket-type bar items (visual grouping).
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 bracket rendering.
pub struct BracketState {
    /// IDs (or regex patterns) of items grouped by this bracket.
    pub members: Vec<String>,
}

// ── Alias component ─────────────────────────────────────────────────

/// State for alias-type bar items (native menu bar mirroring).
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 alias system.
pub struct AliasState {
    /// Source identifier: bundle_id or "window_owner,window_name".
    pub source: String,
    /// Tint color for the captured image.
    pub tint_color: ArgbColor,
    /// Scale factor.
    pub scale: f64,
    /// Update frequency in seconds.
    pub update_freq: u32,
    /// Cached image data (raw RGBA pixels, width, height).
    pub cached_image: Option<(Vec<u8>, u32, u32)>,
}

// ── Popup component ─────────────────────────────────────────────────

/// State for popup menus attached to items.
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 popup system.
pub struct PopupState {
    /// Whether the popup is currently visible.
    pub visible: bool,
    /// Whether the layout is horizontal (vs vertical).
    pub horizontal: bool,
    /// Popup height.
    pub height: f64,
    /// Popup blur radius.
    pub blur_radius: u16,
    /// Vertical offset from bar.
    pub y_offset: f64,
    /// Alignment: "left", "center", "right".
    pub align: String,
    /// Background color.
    pub background_color: ArgbColor,
    /// Border color.
    pub border_color: ArgbColor,
    /// Corner radius.
    pub corner_radius: f64,
}

impl Default for PopupState {
    fn default() -> Self {
        Self {
            visible: false,
            horizontal: false,
            height: 200.0,
            blur_radius: 20,
            y_offset: 0.0,
            align: "center".to_string(),
            background_color: ArgbColor {
                a: 0.8,
                r: 0.118,
                g: 0.118,
                b: 0.18,
            },
            border_color: ArgbColor::TRANSPARENT,
            corner_radius: 8.0,
        }
    }
}

// ── Animation component ─────────────────────────────────────────────

/// An in-flight property animation on a bar item.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Phase 2 animation system.
pub struct PropertyAnimation {
    /// Property being animated.
    pub property: AnimatableProperty,
    /// Start value.
    pub from: f64,
    /// Target value.
    pub to: f64,
    /// Duration in frames.
    pub duration: u32,
    /// Frames elapsed.
    pub elapsed: u32,
    /// Easing curve.
    pub curve: AnimationCurve,
}

impl PropertyAnimation {
    /// Returns the current interpolated value.
    pub fn current_value(&self) -> f64 {
        if self.elapsed >= self.duration {
            return self.to;
        }
        let t = self.elapsed as f64 / self.duration as f64;
        let eased = self.curve.ease(t);
        self.from + (self.to - self.from) * eased
    }

    /// Returns true if the animation is complete.
    pub fn is_complete(&self) -> bool {
        self.elapsed >= self.duration
    }

    /// Advance by one frame.
    pub fn tick(&mut self) {
        self.elapsed = self.elapsed.saturating_add(1);
    }
}

/// Properties that can be animated.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Phase 2 animation system.
pub enum AnimatableProperty {
    Width,
    PositionX,
    PositionY,
    IconColorR,
    IconColorG,
    IconColorB,
    IconColorA,
    LabelColorR,
    LabelColorG,
    LabelColorB,
    LabelColorA,
    BackgroundColorR,
    BackgroundColorG,
    BackgroundColorB,
    BackgroundColorA,
    BackgroundCornerRadius,
    BackgroundBorderWidth,
    SliderPercentage,
}

/// Animation easing curves (matching SketchyBar).
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Phase 2 animation system.
pub enum AnimationCurve {
    Linear,
    Quadratic,
    Tanh,
    Sin,
    Exp,
    Circ,
}

impl AnimationCurve {
    /// Apply easing function to t (0.0–1.0).
    pub fn ease(&self, t: f64) -> f64 {
        match self {
            Self::Linear => t,
            Self::Quadratic => t * t,
            Self::Tanh => (t * 3.0).tanh() / 3.0_f64.tanh(),
            Self::Sin => (t * std::f64::consts::FRAC_PI_2).sin(),
            Self::Exp => (1.0 - (-t * 5.0).exp()) / (1.0 - (-5.0_f64).exp()),
            Self::Circ => 1.0 - (1.0 - t * t).sqrt(),
        }
    }
}

impl Default for AnimationCurve {
    fn default() -> Self {
        Self::Linear
    }
}

/// Active animations on a bar item.
#[derive(Component, Clone, Debug, Default)]
#[allow(dead_code)] // Phase 2 animation system.
pub struct AnimationState {
    pub animations: Vec<PropertyAnimation>,
}

impl AnimationState {
    /// Add or replace an animation for a property.
    pub fn animate(&mut self, anim: PropertyAnimation) {
        // Remove existing animation for same property (mid-flight retarget)
        self.animations
            .retain(|a| a.property != anim.property);
        self.animations.push(anim);
    }

    /// Tick all animations and remove completed ones.
    pub fn tick(&mut self) {
        for anim in &mut self.animations {
            anim.tick();
        }
        self.animations.retain(|a| !a.is_complete());
    }
}

// ── Mouse state component ───────────────────────────────────────────

/// Mouse interaction state for a bar item.
#[derive(Component, Clone, Debug, Default)]
#[allow(dead_code)] // Phase 2 mouse system.
pub struct MouseState {
    /// Whether the mouse is currently over this item.
    pub hovered: bool,
    /// Whether a mouse button is pressed on this item.
    pub pressed: bool,
}

// ── Script state component ──────────────────────────────────────────

/// Script execution state for an item.
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 script system.
pub struct ScriptState {
    /// Last script output (stdout).
    pub last_output: String,
    /// Frames since last script execution.
    pub frames_since_update: u32,
    /// Whether a script is currently running.
    pub running: bool,
}

impl Default for ScriptState {
    fn default() -> Self {
        Self {
            last_output: String::new(),
            frames_since_update: 0,
            running: false,
        }
    }
}

// ── Text scroll state ───────────────────────────────────────────────

/// Text scrolling state for items with `scroll_texts` enabled.
#[derive(Component, Clone, Debug)]
#[allow(dead_code)] // Phase 2 scroll system.
pub struct ScrollState {
    /// Current scroll offset in characters.
    pub offset: usize,
    /// Maximum visible characters.
    pub max_chars: u32,
    /// Frames per scroll step.
    pub frames_per_step: u32,
    /// Frame counter.
    pub frame_count: u32,
    /// Whether scrolling is active (text exceeds max_chars).
    pub active: bool,
}

impl Default for ScrollState {
    fn default() -> Self {
        Self {
            offset: 0,
            max_chars: 0,
            frames_per_step: 10,
            frame_count: 0,
            active: false,
        }
    }
}

// ── Global bar state resource ───────────────────────────────────────

/// Global bar state resource.
#[derive(Resource, Clone, Debug)]
pub struct StatusBarState {
    /// Whether the bar is currently visible.
    pub visible: bool,
    /// Bar height in points.
    pub height: f64,
    /// Whether the bar needs a full redraw.
    pub needs_redraw: bool,
    /// Frame counter for timer-based updates.
    pub frame_count: u64,
}

impl Default for StatusBarState {
    fn default() -> Self {
        Self {
            visible: false,
            height: 28.0,
            needs_redraw: true,
            frame_count: 0,
        }
    }
}

// ── Event resource ──────────────────────────────────────────────────

/// Resource holding pending bar events.
#[derive(Resource, Clone, Debug, Default)]
#[allow(dead_code)] // Phase 2 event system.
pub struct BarEventQueue {
    pub events: Vec<BarEvent>,
}

/// A bar event that items can subscribe to.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Phase 2 event system.
pub struct BarEvent {
    /// Event name (e.g., "front_app_switched", "volume_change").
    pub name: String,
    /// Optional JSON payload.
    pub info: String,
}

// ── IPC command resource ────────────────────────────────────────────

/// Resource holding pending IPC commands (from CLI or MCP).
#[derive(Resource, Clone, Debug, Default)]
#[allow(dead_code)] // Phase 2 IPC system.
pub struct BarCommandQueue {
    pub commands: Vec<BarCommand>,
}

/// A runtime command to modify bar state.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Phase 2 IPC system.
pub enum BarCommand {
    /// Set properties on an item: (item_id_or_regex, key, value).
    Set {
        target: String,
        properties: Vec<(String, String)>,
    },
    /// Add a new item.
    Add {
        id: String,
        item_type: BarItemType,
        position: BarPosition,
    },
    /// Remove an item.
    Remove { target: String },
    /// Trigger a custom event.
    Trigger { event: String, info: String },
    /// Query an item's state (response sent via sender).
    Query { target: String },
    /// Set bar-level properties.
    Bar { properties: Vec<(String, String)> },
    /// Reorder items.
    Reorder { ids: Vec<String> },
    /// Push data to a graph item.
    Push { target: String, value: f64 },
    /// Animate properties.
    Animate {
        curve: AnimationCurve,
        duration: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_argb_hex_8_digit() {
        let c = ArgbColor::from_hex("0xCC1e1e2e").expect("should parse");
        assert!((c.a - 0.8).abs() < 0.01);
        assert!((c.r - 0.118).abs() < 0.01);
        assert!((c.g - 0.118).abs() < 0.01);
        assert!((c.b - 0.18).abs() < 0.01);
    }

    #[test]
    fn parse_rgb_hex_6_digit() {
        let c = ArgbColor::from_hex("#FFFFFF").expect("should parse");
        assert!((c.a - 1.0).abs() < f64::EPSILON);
        assert!((c.r - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_hex_no_prefix() {
        let c = ArgbColor::from_hex("FF00FF00").expect("should parse");
        assert!((c.a - 1.0).abs() < f64::EPSILON);
        assert!((c.r - 0.0).abs() < f64::EPSILON);
        assert!((c.g - 1.0).abs() < f64::EPSILON);
        assert!((c.b - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_hex_invalid() {
        assert!(ArgbColor::from_hex("nope").is_none());
        assert!(ArgbColor::from_hex("0xZZZZZZZZ").is_none());
    }

    #[test]
    fn default_state_is_not_hidden() {
        let state = BarItemState::default();
        assert!(!state.hidden);
    }

    #[test]
    fn color_lerp() {
        let a = ArgbColor::BLACK;
        let b = ArgbColor::WHITE;
        let mid = a.lerp(&b, 0.5);
        assert!((mid.r - 0.5).abs() < 0.01);
        assert!((mid.g - 0.5).abs() < 0.01);
        assert!((mid.b - 0.5).abs() < 0.01);
    }

    #[test]
    fn graph_data_circular_buffer() {
        let mut g = GraphData::new(3);
        g.push(0.1);
        g.push(0.2);
        g.push(0.3);
        assert_eq!(g.data.len(), 3);
        g.push(0.4);
        assert_eq!(g.data.len(), 3);
        assert!((g.data[0] - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn animation_curve_linear() {
        assert!((AnimationCurve::Linear.ease(0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn animation_curve_quadratic() {
        assert!((AnimationCurve::Quadratic.ease(0.5) - 0.25).abs() < 0.01);
    }

    #[test]
    fn animation_state_retarget() {
        let mut state = AnimationState::default();
        state.animate(PropertyAnimation {
            property: AnimatableProperty::Width,
            from: 0.0,
            to: 100.0,
            duration: 10,
            elapsed: 5,
            curve: AnimationCurve::Linear,
        });
        assert_eq!(state.animations.len(), 1);
        // Retarget — should replace
        state.animate(PropertyAnimation {
            property: AnimatableProperty::Width,
            from: 50.0,
            to: 200.0,
            duration: 10,
            elapsed: 0,
            curve: AnimationCurve::Linear,
        });
        assert_eq!(state.animations.len(), 1);
        assert!((state.animations[0].from - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn highlight_effective_colors() {
        let mut state = BarItemState::default();
        state.icon_color = ArgbColor::WHITE;
        state.icon_highlight_color = ArgbColor::BLACK;
        assert_eq!(state.effective_icon_color(), &ArgbColor::WHITE);
        state.highlight = true;
        assert_eq!(state.effective_icon_color(), &ArgbColor::BLACK);
    }

    #[test]
    fn slider_default() {
        let s = SliderState::default();
        assert!((s.percentage - 0.0).abs() < f64::EPSILON);
        assert!(!s.dragging);
    }
}

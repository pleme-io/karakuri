use serde::{Deserialize, Serialize};

use super::components::{BarItemType, BarPosition};

/// Top-level status bar configuration, parsed from the `status_bar` section
/// of ayatsuri.yaml.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct StatusBarConfig {
    /// Master enable switch.
    pub enabled: bool,
    /// Bar position: top or bottom.
    pub position: BarEdge,
    /// Bar height in points.
    pub height: u16,
    /// Separate bar height for displays with a notch.
    pub notch_display_height: Option<u16>,
    /// Background blur radius (0 = no blur).
    pub blur_radius: u16,
    /// Background color (ARGB hex).
    pub color: String,
    /// Border color (ARGB hex).
    pub border_color: String,
    /// Border width in points.
    pub border_width: f64,
    /// Corner radius.
    pub corner_radius: f64,
    /// Left/right margin from screen edges.
    pub margin: f64,
    /// Default font spec: "Family:Style:Size".
    pub font: String,
    /// Default icon font spec.
    pub icon_font: String,
    /// Left padding in points.
    pub padding_left: f64,
    /// Right padding in points.
    pub padding_right: f64,
    /// Vertical offset.
    pub y_offset: f64,
    /// Additional vertical offset for displays with a notch.
    pub notch_offset: f64,
    /// Whether the bar is above all windows.
    /// `true` = always topmost, `"window"` = above focused window only.
    pub topmost: TopmostMode,
    /// Whether the bar is visible on all spaces.
    pub sticky: bool,
    /// Which displays to show on: "all", "main", or display index.
    pub display: String,
    /// Space reserved for MacBook notch (points).
    pub notch_width: u16,
    /// Auto-hide the native macOS menu bar.
    pub hide_macos_menubar: bool,
    /// Bar hidden mode: false = visible, true = hidden, "current" = hidden on focused display only.
    pub hidden: HiddenMode,
    /// Enable shadow below the bar.
    pub shadow: bool,
    /// Enable font smoothing (subpixel antialiasing).
    pub font_smoothing: bool,
    /// Default properties for all items.
    #[serde(default)]
    pub defaults: ItemDefaults,
    /// Bar items.
    #[serde(default)]
    pub items: Vec<ItemConfig>,
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: BarEdge::Top,
            height: 28,
            notch_display_height: None,
            blur_radius: 20,
            color: "0xCC1e1e2e".to_string(),
            border_color: "0xFF313244".to_string(),
            border_width: 0.0,
            corner_radius: 0.0,
            margin: 0.0,
            font: "Hack Nerd Font:Regular:14.0".to_string(),
            icon_font: "Hack Nerd Font:Regular:16.0".to_string(),
            padding_left: 8.0,
            padding_right: 8.0,
            y_offset: 0.0,
            notch_offset: 0.0,
            topmost: TopmostMode::Bool(false),
            sticky: true,
            display: "all".to_string(),
            notch_width: 220,
            hide_macos_menubar: false,
            hidden: HiddenMode::Bool(false),
            shadow: false,
            font_smoothing: true,
            defaults: ItemDefaults::default(),
            items: Vec::new(),
        }
    }
}

/// Bar edge position.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BarEdge {
    #[default]
    Top,
    Bottom,
}

/// Topmost mode: bool or "window" (above focused window only).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum TopmostMode {
    Bool(bool),
    Window(String), // "window"
}

impl Default for TopmostMode {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl TopmostMode {
    pub fn is_topmost(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Window(s) => s == "window",
        }
    }

    pub fn is_window_mode(&self) -> bool {
        matches!(self, Self::Window(s) if s == "window")
    }
}

/// Hidden mode: bool or "current" (hidden on focused display only).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum HiddenMode {
    Bool(bool),
    Current(String), // "current"
}

impl Default for HiddenMode {
    fn default() -> Self {
        Self::Bool(false)
    }
}

impl HiddenMode {
    pub fn is_hidden(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            Self::Current(s) => s == "current",
        }
    }

    pub fn is_current_mode(&self) -> bool {
        matches!(self, Self::Current(s) if s == "current")
    }
}

/// Default visual properties applied to all items unless overridden.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ItemDefaults {
    pub icon_color: String,
    pub label_color: String,
    pub background_color: String,
    pub padding_left: f64,
    pub padding_right: f64,
}

impl Default for ItemDefaults {
    fn default() -> Self {
        Self {
            icon_color: "0xFFcdd6f4".to_string(),
            label_color: "0xFFcdd6f4".to_string(),
            background_color: "0x00000000".to_string(),
            padding_left: 6.0,
            padding_right: 6.0,
        }
    }
}

/// Shadow properties for an item or bar element.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ShadowConfig {
    /// Whether shadow is drawn.
    pub drawing: bool,
    /// Shadow color (ARGB hex).
    pub color: String,
    /// Shadow angle in degrees (0 = right, 90 = up).
    pub angle: f64,
    /// Shadow distance in points.
    pub distance: f64,
}

impl Default for ShadowConfig {
    fn default() -> Self {
        Self {
            drawing: false,
            color: "0x80000000".to_string(),
            angle: 270.0,
            distance: 2.0,
        }
    }
}

/// Image properties for an item background or icon.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ImageConfig {
    /// Image source: file path, "app.<bundle_id>", or "media.artwork".
    pub source: String,
    /// Scale factor.
    pub scale: f64,
    /// Border color (ARGB hex).
    pub border_color: String,
    /// Border width.
    pub border_width: f64,
    /// Corner radius.
    pub corner_radius: f64,
    /// Padding from item edges.
    pub padding: f64,
    /// Vertical offset.
    pub y_offset: f64,
}

impl Default for ImageConfig {
    fn default() -> Self {
        Self {
            source: String::new(),
            scale: 1.0,
            border_color: "0x00000000".to_string(),
            border_width: 0.0,
            corner_radius: 0.0,
            padding: 0.0,
            y_offset: 0.0,
        }
    }
}

/// Popup menu configuration for an item.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct PopupConfig {
    /// Whether the popup is drawn (visible).
    pub drawing: bool,
    /// Horizontal layout (vs vertical).
    pub horizontal: bool,
    /// Popup above all windows.
    pub topmost: bool,
    /// Popup height in points.
    pub height: u16,
    /// Background blur radius.
    pub blur_radius: u16,
    /// Vertical offset from the bar.
    pub y_offset: f64,
    /// Alignment: left, center, right.
    pub align: String,
    /// Background color.
    pub background_color: String,
    /// Background border color.
    pub background_border_color: String,
    /// Background corner radius.
    pub background_corner_radius: f64,
    /// Child items in the popup.
    #[serde(default)]
    pub items: Vec<ItemConfig>,
}

impl Default for PopupConfig {
    fn default() -> Self {
        Self {
            drawing: false,
            horizontal: false,
            topmost: true,
            height: 200,
            blur_radius: 20,
            y_offset: 0.0,
            align: "center".to_string(),
            background_color: "0xCC1e1e2e".to_string(),
            background_border_color: "0xFF313244".to_string(),
            background_corner_radius: 8.0,
            items: Vec::new(),
        }
    }
}

/// Configuration for a single bar item.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ItemConfig {
    // ── Identity ──
    /// Unique identifier.
    pub id: String,
    /// Item type.
    #[serde(rename = "type")]
    pub item_type: BarItemType,
    /// Position on the bar.
    pub position: BarPosition,

    // ── Content ──
    /// Icon text (glyph).
    pub icon: Option<String>,
    /// Label text (static).
    pub label: Option<String>,
    /// Shell script for dynamic updates.
    pub script: Option<String>,
    /// Shell script triggered on click.
    pub click_script: Option<String>,
    /// Update frequency in seconds (0 = event-only).
    pub update_freq: u32,
    /// Events to subscribe to.
    #[serde(default)]
    pub subscribe: Vec<String>,

    // ── Visual: colors ──
    /// Icon color override (ARGB hex).
    pub icon_color: Option<String>,
    /// Label color override (ARGB hex).
    pub label_color: Option<String>,
    /// Icon highlight color (used when highlight=true).
    pub icon_highlight_color: Option<String>,
    /// Label highlight color (used when highlight=true).
    pub label_highlight_color: Option<String>,
    /// Whether highlight mode is active (uses highlight colors).
    pub highlight: Option<bool>,

    // ── Visual: fonts ──
    /// Icon font override ("Family:Style:Size").
    pub icon_font: Option<String>,
    /// Label font override ("Family:Style:Size").
    pub label_font: Option<String>,

    // ── Visual: background ──
    /// Background color override.
    pub background_color: Option<String>,
    /// Background corner radius.
    pub background_corner_radius: Option<f64>,
    /// Background height override (for pill-shaped backgrounds).
    pub background_height: Option<f64>,
    /// Background border color.
    pub background_border_color: Option<String>,
    /// Background border width.
    pub background_border_width: Option<f64>,
    /// Background clip (transparent hole through bar, 0.0–1.0).
    pub background_clip: Option<f64>,
    /// Background image configuration.
    #[serde(default)]
    pub background_image: Option<ImageConfig>,
    /// Per-item background blur radius.
    pub blur_radius: Option<u16>,

    // ── Visual: shadow ──
    /// Shadow configuration.
    #[serde(default)]
    pub shadow: Option<ShadowConfig>,

    // ── Visual: image/icon ──
    /// Image source for icon: file path, "app.<bundle_id>", "media.artwork".
    pub icon_image: Option<ImageConfig>,

    // ── Layout ──
    /// Item width override (0 or absent = dynamic).
    pub width: Option<f64>,
    /// Padding left override.
    pub padding_left: Option<f64>,
    /// Padding right override.
    pub padding_right: Option<f64>,
    /// Vertical offset for this item.
    pub y_offset: Option<f64>,
    /// Text alignment within fixed-width items: left, center, right.
    pub align: Option<String>,
    /// Whether the item is drawn (visible). Default true.
    pub drawing: Option<bool>,

    // ── Text effects ──
    /// Enable text scrolling when label exceeds max_chars.
    pub scroll_texts: Option<bool>,
    /// Maximum visible characters before scrolling.
    pub max_chars: Option<u32>,
    /// Scroll animation duration in ms.
    pub scroll_duration: Option<u32>,

    // ── Behavior ──
    /// When scripts execute: "always" or "when_shown".
    pub updates: Option<String>,
    /// Override space/display associations (always show).
    pub ignore_association: Option<bool>,
    /// Display index this item belongs to (for multi-display filtering).
    pub display: Option<String>,
    /// Space indices this item belongs to (for space-specific visibility).
    pub space: Option<Vec<u32>>,

    // ── Space-specific ──
    /// Space indices (for type=space).
    pub spaces: Option<Vec<u32>>,
    /// Color when space is selected.
    pub selected_color: Option<String>,

    // ── Graph-specific ──
    /// Graph line color.
    pub graph_color: Option<String>,
    /// Graph fill color.
    pub graph_fill_color: Option<String>,
    /// Graph line width.
    pub graph_line_width: Option<f64>,
    /// Graph data buffer size (number of data points).
    pub graph_data_points: Option<usize>,

    // ── Slider-specific ──
    /// Slider track color.
    pub slider_color: Option<String>,
    /// Slider knob color.
    pub slider_knob_color: Option<String>,
    /// Slider highlight color (filled portion).
    pub slider_highlight_color: Option<String>,
    /// Initial slider percentage (0–100).
    pub slider_percentage: Option<f64>,
    /// Slider width.
    pub slider_width: Option<f64>,

    // ── Bracket-specific ──
    /// Member item IDs (or regex patterns) grouped by this bracket.
    #[serde(default)]
    pub bracket_members: Vec<String>,

    // ── Alias-specific ──
    /// Alias source: bundle_id or "window_owner,window_name".
    pub alias_source: Option<String>,
    /// Alias tint color.
    pub alias_color: Option<String>,
    /// Alias scale factor.
    pub alias_scale: Option<f64>,
    /// Alias update frequency in seconds.
    pub alias_update_freq: Option<u32>,

    // ── Popup ──
    /// Popup menu configuration.
    #[serde(default)]
    pub popup: Option<PopupConfig>,
}

impl Default for ItemConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            item_type: BarItemType::Item,
            position: BarPosition::Left,
            icon: None,
            label: None,
            script: None,
            click_script: None,
            update_freq: 0,
            subscribe: Vec::new(),
            icon_color: None,
            label_color: None,
            icon_highlight_color: None,
            label_highlight_color: None,
            highlight: None,
            icon_font: None,
            label_font: None,
            background_color: None,
            background_corner_radius: None,
            background_height: None,
            background_border_color: None,
            background_border_width: None,
            background_clip: None,
            background_image: None,
            blur_radius: None,
            shadow: None,
            icon_image: None,
            width: None,
            padding_left: None,
            padding_right: None,
            y_offset: None,
            align: None,
            drawing: None,
            scroll_texts: None,
            max_chars: None,
            scroll_duration: None,
            updates: None,
            ignore_association: None,
            display: None,
            space: None,
            spaces: None,
            selected_color: None,
            graph_color: None,
            graph_fill_color: None,
            graph_line_width: None,
            graph_data_points: None,
            slider_color: None,
            slider_knob_color: None,
            slider_highlight_color: None,
            slider_percentage: None,
            slider_width: None,
            bracket_members: Vec::new(),
            alias_source: None,
            alias_color: None,
            alias_scale: None,
            alias_update_freq: None,
            popup: None,
        }
    }
}

/// Parse font spec "Family:Style:Size" → (family, size).
#[allow(dead_code)] // Used by Phase 2 render system with per-item fonts.
pub fn parse_font_spec(spec: &str) -> (&str, f64) {
    let family = spec.split(':').next().unwrap_or("Hack Nerd Font");
    let size = spec
        .split(':')
        .nth(2)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(14.0);
    (family, size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let cfg = StatusBarConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.height, 28);
        assert_eq!(cfg.position, BarEdge::Top);
    }

    #[test]
    fn default_items_empty() {
        let cfg = StatusBarConfig::default();
        assert!(cfg.items.is_empty());
    }

    #[test]
    fn item_config_defaults() {
        let item = ItemConfig::default();
        assert!(item.id.is_empty());
        assert_eq!(item.update_freq, 0);
        assert!(item.subscribe.is_empty());
        assert!(item.script.is_none());
        assert!(item.click_script.is_none());
        assert!(item.highlight.is_none());
        assert!(item.popup.is_none());
    }

    #[test]
    fn parse_font_spec_full() {
        let (family, size) = parse_font_spec("Hack Nerd Font:Bold:16.0");
        assert_eq!(family, "Hack Nerd Font");
        assert!((size - 16.0).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_font_spec_minimal() {
        let (family, size) = parse_font_spec("Menlo");
        assert_eq!(family, "Menlo");
        assert!((size - 14.0).abs() < f64::EPSILON);
    }

    #[test]
    fn topmost_modes() {
        let t = TopmostMode::Bool(true);
        assert!(t.is_topmost());
        assert!(!t.is_window_mode());

        let w = TopmostMode::Window("window".to_string());
        assert!(w.is_topmost());
        assert!(w.is_window_mode());
    }

    #[test]
    fn hidden_modes() {
        let h = HiddenMode::Bool(false);
        assert!(!h.is_hidden());
        assert!(!h.is_current_mode());

        let c = HiddenMode::Current("current".to_string());
        assert!(c.is_hidden());
        assert!(c.is_current_mode());
    }
}

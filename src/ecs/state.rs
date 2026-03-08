//! Centralized state management for the Ayatsuri window manager.
//!
//! Uses Bevy's `States` system for global mode transitions and typed context
//! resources for associated data. This replaces scattered flag resources with
//! deterministic, provable state management.

use std::collections::HashMap;

use bevy::ecs::resource::Resource;
use bevy::math::IRect;
use bevy::state::state::States;
use objc2_core_graphics::CGDirectDisplayID;

use super::{Entity, SnapZone};
use crate::platform::WinID;

// ---------------------------------------------------------------------------
// Layer 1: Bevy States (global mode)
// ---------------------------------------------------------------------------

/// Top-level application lifecycle phase.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppPhase {
    /// Gathering existing processes and windows.
    #[default]
    Initializing,
    /// Startup apps are being launched (post-init, pre-running).
    StartupPending,
    /// Normal operation.
    Running,
}

/// Mutually exclusive interaction modes. Only one can be active at a time.
#[derive(States, Default, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionMode {
    /// No special interaction in progress.
    #[default]
    Idle,
    /// A window is being dragged (edge-snap active).
    Dragging,
    /// A trackpad swipe gesture is in progress.
    Swiping,
    /// Mission Control is showing.
    MissionControl,
}

// ---------------------------------------------------------------------------
// Layer 2: Context Resources
// ---------------------------------------------------------------------------

/// How the current focus was acquired. Determines reshuffle and warp behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // System variant — set by app lifecycle triggers (app launch, space change).
pub enum FocusSource {
    /// Focus changed via keyboard navigation. Reshuffle + warp cursor.
    Keyboard,
    /// Focus changed via mouse (FFM). No reshuffle, no warp.
    Mouse,
    /// Focus changed by the system (e.g., app launch). No reshuffle, no warp.
    System,
}

/// Replaces `SkipReshuffle` + `FocusFollowsMouse` flag resources.
#[derive(Resource, Debug)]
pub struct FocusContext {
    /// How the current focus was acquired.
    pub source: FocusSource,
    /// The window ID that FFM wants to focus (if any).
    pub ffm_window: Option<WinID>,
}

impl Default for FocusContext {
    fn default() -> Self {
        Self {
            source: FocusSource::Keyboard,
            ffm_window: None,
        }
    }
}

impl FocusContext {
    /// Whether reshuffling should be skipped for the current focus change.
    pub fn skip_reshuffle(&self) -> bool {
        matches!(self.source, FocusSource::Mouse | FocusSource::System)
    }
}

/// Data associated with an active drag interaction.
/// Only meaningful when `InteractionMode::Dragging` is active.
#[derive(Resource, Debug)]
pub struct DragContext {
    /// The entity being dragged.
    pub entity: Entity,
    /// The display the drag is happening on.
    pub display_id: CGDirectDisplayID,
    /// The snap zone the cursor is currently in, if any.
    pub snap_zone: Option<SnapZone>,
    /// The display the snap zone belongs to (may differ during cross-display drags).
    #[allow(dead_code)] // Written during drag; read by cross-display snap rendering.
    pub snap_display: Option<CGDirectDisplayID>,
}

/// Data associated with an active swipe gesture.
/// Only meaningful when `InteractionMode::Swiping` is active.
#[derive(Resource, Debug)]
pub struct SwipeContext {
    /// Current swipe velocity (pixels/tick, exponentially decayed).
    pub velocity: f64,
    /// Accumulated viewport pixel offset.
    pub viewport_offset: i32,
    /// Timestamp of the last finger-on-trackpad event.
    pub last_swipe: std::time::Instant,
}

impl SwipeContext {
    pub fn new(velocity: f64, viewport_offset: i32) -> Self {
        Self {
            velocity,
            viewport_offset,
            last_swipe: std::time::Instant::now(),
        }
    }
}

/// Whether the active display is showing a native macOS fullscreen space.
/// Replaces the `ON_FULLSCREEN_SPACE` static `AtomicBool`.
#[derive(Resource, Debug, Default)]
pub struct FullscreenSpace(pub bool);

/// Default settle frames if no config is available.
#[allow(dead_code)]
const DEFAULT_SETTLE_FRAMES: u32 = 2;

/// Debounce gate for reload-triggered reshuffles.
///
/// When present, cascading reshuffles are suppressed and animations use
/// instant-snap instead of interpolation. The guard captures pre-reload
/// window positions so that unchanged windows emit no movement markers.
#[derive(Resource, Debug)]
pub struct ReloadGuard {
    /// Frames remaining before the consolidated reshuffle fires.
    pub settle_frames: u32,
    /// The configured settle value (for bump resets).
    settle_value: u32,
    /// Window positions captured at the moment the guard was inserted.
    pub pre_positions: HashMap<Entity, IRect>,
}

impl ReloadGuard {
    #[allow(dead_code)]
    pub fn new(pre_positions: HashMap<Entity, IRect>) -> Self {
        Self::with_settle_frames(pre_positions, DEFAULT_SETTLE_FRAMES)
    }

    pub fn with_settle_frames(pre_positions: HashMap<Entity, IRect>, settle: u32) -> Self {
        let settle = settle.max(1);
        Self {
            settle_frames: settle,
            settle_value: settle,
            pre_positions,
        }
    }

    /// Reset the settle counter (called by cascading triggers).
    pub fn bump(&mut self) {
        self.settle_frames = self.settle_value;
    }

    /// Whether the guard has settled (ready for consolidated reshuffle).
    pub fn settled(&self) -> bool {
        self.settle_frames == 0
    }

    /// Decrement the settle counter. Returns `true` when it just reached zero.
    pub fn tick(&mut self) -> bool {
        if self.settle_frames > 0 {
            self.settle_frames -= 1;
            self.settle_frames == 0
        } else {
            false
        }
    }
}

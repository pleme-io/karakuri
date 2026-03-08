use std::sync::Arc;
use std::sync::mpsc::Receiver;
use std::time::Duration;

use arc_swap::ArcSwap;
use bevy::MinimalPlugins;
use bevy::app::App as BevyApp;
use bevy::state::app::AppExtStates;
use bevy::ecs::message::Messages;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::Commands;
use bevy::prelude::Event as BevyEvent;
use bevy::tasks::Task;
use bevy::time::Timer;
use bevy::time::{Time, Virtual};
use bevy::ecs::{component::Component, entity::Entity};
use bevy::math::IVec2;
use derive_more::{Deref, DerefMut};
use objc2_core_graphics::CGDirectDisplayID;

use crate::snapshot::StateSnapshot;

use crate::config::CONFIGURATION_FILE;
use crate::errors::Result;
use crate::events::{Event, EventSender};
use crate::manager::{
    Origin, ProcessApi, Size, Window, WindowManager, WindowManagerApi, WindowManagerOS,
};
use crate::overlay::OverlayManager;
use crate::platform::{PlatformCallbacks, WinID};
use crate::plugins::{
    AppLifecyclePlugin, ClipboardPlugin, DisplayPlugin, HotkeyPlugin, MenuBarPlugin,
    NotificationPlugin, ScriptingPlugin, SnapshotPlugin, WindowPlugin,
};

pub mod params;
pub mod state;
pub(crate) mod systems;
pub(crate) mod triggers;

/// Marker component for the currently focused window.
#[derive(Component)]
pub struct FocusedMarker;

#[derive(Component)]
pub struct ActiveWorkspaceMarker;

/// Marker component for the currently active display.
#[derive(Component)]
pub struct ActiveDisplayMarker;

/// Marker component signifying a freshly created process, application, or window.
#[derive(Component)]
pub struct FreshMarker;

/// Marker component used to gather existing processes and windows during initialization.
#[derive(Component)]
pub struct ExistingMarker;

/// Component representing a request to reposition a window.
#[derive(Component)]
pub struct RepositionMarker {
    /// The new origin (x, y coordinates) for the window.
    pub origin: Origin,
    /// The ID of the display the window should be moved to.
    pub display_id: CGDirectDisplayID,
}

/// Component representing a request to resize a window.
#[derive(Component)]
pub struct ResizeMarker {
    /// The new size (width, height) for the window.
    pub size: Size,
    pub display_id: CGDirectDisplayID,
}

/// Per-window spring animation velocity state.
/// Persists across marker re-insertions so mid-flight retargets
/// preserve momentum instead of restarting from zero.
#[derive(Component, Default)]
pub struct SpringState {
    pub pos_x: crate::logic::spring::SpringAxis,
    pub pos_y: crate::logic::spring::SpringAxis,
    pub size_x: crate::logic::spring::SpringAxis,
    pub size_y: crate::logic::spring::SpringAxis,
}

/// Marker component indicating that a window is currently being dragged by the mouse.
#[derive(Component)]
pub struct WindowDraggedMarker {
    /// The entity ID of the dragged window.
    pub entity: Entity,
    /// The ID of the display the window is being dragged on.
    pub display_id: CGDirectDisplayID,
    /// Window origin when the drag started — used to detect actual window moves
    /// vs. in-window interactions (text selection, scrolling). Snap only triggers
    /// when the window has moved from this initial position.
    pub initial_origin: IVec2,
}

/// Marker component indicating that windows around the marked entity need to be reshuffled.
#[derive(Component)]
pub struct ReshuffleAroundMarker;

/// Marker component placed on a window that was resized internally to compensate
/// for an adjacent stacked window's top-edge drag. When the OS echoes back a
/// `WindowResized` event for this window, the reshuffle is skipped and the marker
/// is removed to prevent a feedback loop.
#[derive(Component)]
pub struct StackAdjustedResize;

#[derive(Component)]
pub struct WindowSwipeMarker(pub f64);

/// Marks a window entity that is currently on a native macOS fullscreen space.
/// The window has been removed from its tiled position in the strip.
/// `order` gives the sequence in which windows went fullscreen (0, 1, 2, …)
/// so they can be navigated left-to-right in that order after the tiled strip.
#[derive(Component)]
pub struct NativeFullscreenMarker {
    pub order: u32,
}

/// Stores the width ratio of a window before it was made full-width.
/// When a stacked window goes full-width, it is unstacked first;
/// `was_stacked` records whether to restack on exit.
#[derive(Component)]
pub struct FullWidthMarker {
    pub width_ratio: f64,
    pub was_stacked: bool,
}

/// Enum component indicating the unmanaged state of a window.
#[derive(Component, Debug)]
pub enum Unmanaged {
    /// The window is floating and not part of the tiling layout.
    Floating,
    /// The window is minimized.
    Minimized,
    /// The window is hidden.
    Hidden,
}

/// Wrapper component for a `ProcessApi` trait object, enabling dynamic dispatch for process-related operations within Bevy.
#[derive(Component, Deref, DerefMut)]
pub struct BProcess(pub Box<dyn ProcessApi>);

/// Component to manage a timeout, often used for delaying actions or retries.
#[derive(Component)]
pub struct Timeout {
    /// The Bevy timer instance.
    pub timer: Timer,
    /// An optional message associated with the timeout.
    pub message: Option<String>,
}

impl Timeout {
    /// Creates a new `Timeout` with a specified duration and an optional message.
    /// The timer is set to run once.
    ///
    /// # Arguments
    ///
    /// * `duration` - The `Duration` for the timeout.
    /// * `message` - An `Option<String>` containing a message to associate with the timeout.
    ///
    /// # Returns
    ///
    /// A new `Timeout` instance.
    pub fn new(duration: Duration, message: Option<String>) -> Self {
        let timer = Timer::from_seconds(duration.as_secs_f32(), bevy::time::TimerMode::Once);
        Self { timer, message }
    }
}

/// Component used as a retry mechanism for stray focus events that arrive before the target window is fully created.
#[derive(Component)]
pub struct StrayFocusEvent(pub WinID);

#[derive(Component)]
pub struct BruteforceWindows(Task<Vec<Window>>);

#[derive(Component, Debug)]
pub enum DockPosition {
    Bottom(i32),
    Left(i32),
    Right(i32),
    Hidden,
}

/// The screen zone a window can snap to when released near a display edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapZone {
    LeftHalf,
    RightHalf,
    TopHalf,
    BottomHalf,
    Fullscreen,
}

/// Resource to control whether the application should poll for notifications.
#[derive(PartialEq, Resource)]
pub struct PollForNotifications;

/// Component for a pending startup app launch with a countdown timer.
#[derive(Component)]
pub struct StartupAppLaunch {
    pub app: String,
    pub timer: Timer,
}

/// Bevy event trigger for general window manager events.
#[derive(BevyEvent)]
pub struct WMEventTrigger(pub Event);

/// Bevy event trigger for spawning new windows.
#[derive(BevyEvent)]
pub struct SpawnWindowTrigger(pub Vec<Window>);

#[derive(BevyEvent)]
pub struct LocateDockTrigger(pub Entity);

#[derive(BevyEvent)]
pub struct SendMessageTrigger(pub Event);

pub fn reposition_entity(
    entity: Entity,
    origin: Origin,
    display_id: CGDirectDisplayID,
    commands: &mut Commands,
) {
    if let Ok(mut entity_cmmands) = commands.get_entity(entity) {
        entity_cmmands.try_insert(RepositionMarker { origin, display_id });
    }
}

pub fn resize_entity(
    entity: Entity,
    size: Size,
    display_id: CGDirectDisplayID,
    commands: &mut Commands,
) {
    if size.x <= 0 || size.y <= 0 {
        return;
    }
    if let Ok(mut entity_cmmands) = commands.get_entity(entity) {
        entity_cmmands.try_insert(ResizeMarker { size, display_id });
    }
}

#[track_caller]
pub fn reshuffle_around(entity: Entity, commands: &mut Commands) {
    if let Ok(mut entity_commands) = commands.get_entity(entity) {
        let caller = std::panic::Location::caller();
        tracing::debug!("reshuffle_around: entity {entity} from {caller}");
        entity_commands.try_insert(ReshuffleAroundMarker);
    }
}

pub fn setup_bevy_app(
    sender: EventSender,
    receiver: Receiver<Event>,
) -> Result<(BevyApp, Arc<ArcSwap<StateSnapshot>>)> {
    let window_manager: Box<dyn WindowManagerApi> = Box::new(WindowManagerOS::new(sender.clone()));
    let watcher = window_manager.setup_config_watcher(CONFIGURATION_FILE.as_path())?;

    // Create the shared snapshot before building the app, so we can hand
    // the same Arc to both the SnapshotPlugin (Bevy writer) and the
    // CommandReader (socket reader).
    let shared_state = Arc::new(ArcSwap::from_pointee(StateSnapshot::default()));

    let mut app = BevyApp::new();
    app.add_plugins(MinimalPlugins)
        .add_plugins(bevy::state::app::StatesPlugin)
        .init_resource::<Messages<Event>>()
        .insert_resource(Time::<Virtual>::from_max_delta(Duration::from_secs(10)))
        .insert_resource(WindowManager(window_manager))
        .init_resource::<state::FocusContext>()
        .init_resource::<state::FullscreenSpace>()
        .insert_resource(PollForNotifications)
        .init_state::<state::AppPhase>()
        .init_state::<state::InteractionMode>()
        .insert_resource(crate::plugins::snapshot::SharedState(shared_state.clone()))
        .insert_non_send_resource(watcher)
        .add_plugins((
            WindowPlugin,
            HotkeyPlugin,
            DisplayPlugin,
            AppLifecyclePlugin,
            ScriptingPlugin::new(),
            ClipboardPlugin,
            NotificationPlugin,
            MenuBarPlugin,
            SnapshotPlugin,
        ));

    let mut platform_callbacks = PlatformCallbacks::new(sender);
    platform_callbacks.setup_handlers()?;
    let overlay_manager = OverlayManager::new(platform_callbacks.main_thread_marker);
    app.insert_non_send_resource(platform_callbacks);
    app.insert_non_send_resource(overlay_manager);
    app.insert_non_send_resource(receiver);

    Ok((app, shared_state))
}

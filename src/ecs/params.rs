use std::time::Duration;

use bevy::{
    ecs::{
        entity::Entity,
        hierarchy::ChildOf,
        query::{With, Without},
        system::{Commands, Query, Res, ResMut, Single, SystemParam},
        world::Mut,
    },
    math::IRect,
    state::state::State,
};
use objc2_core_graphics::CGDirectDisplayID;
use tracing::warn;

use super::ActiveDisplayMarker;
use crate::{
    config::{Config, WindowParams},
    ecs::{
        ActiveWorkspaceMarker, DockPosition, FocusedMarker, FullWidthMarker, Unmanaged,
        state::{AppPhase, FocusContext, FocusSource, InteractionMode, SwipeContext},
    },
    manager::{Application, Display, LayoutStrip, Window},
    platform::{ProcessSerialNumber, WinID},
};

/// A Bevy `SystemParam` that provides read-only access to the application's configuration and focus state.
/// Use this in systems that do not mutate `FocusContext` — it takes a shared `Res` lock,
/// allowing Bevy to schedule these systems in parallel with other readers.
#[derive(SystemParam)]
pub struct Configuration<'w> {
    config: Res<'w, Config>,
    focus_ctx: Res<'w, FocusContext>,
    interaction_mode: Res<'w, State<InteractionMode>>,
    app_phase: Res<'w, State<AppPhase>>,
}

#[allow(dead_code)] // Accessors for features being wired up (FFM, gestures, edge padding).
impl Configuration<'_> {
    pub fn window_management_enabled(&self) -> bool {
        self.config.window_management_enabled()
    }

    pub fn focus_follows_mouse(&self) -> bool {
        self.config
            .options()
            .focus_follows_mouse
            .is_none_or(|ffm| ffm)
    }

    pub fn mouse_follows_focus(&self) -> bool {
        self.config
            .options()
            .mouse_follows_focus
            .is_none_or(|mff| mff)
    }

    pub fn auto_center(&self) -> bool {
        self.config
            .options()
            .auto_center
            .is_some_and(|centered| centered)
    }

    pub fn swipe_gesture_fingers(&self) -> Option<usize> {
        self.config.options().swipe_gesture_fingers
    }

    pub fn find_window_properties(&self, title: &str, bundle_id: &str) -> Vec<WindowParams> {
        self.config.find_window_properties(title, bundle_id)
    }

    pub fn ffm_flag(&self) -> Option<WinID> {
        self.focus_ctx.ffm_window
    }

    pub fn skip_reshuffle(&self) -> bool {
        self.focus_ctx.skip_reshuffle()
    }

    pub fn edge_padding(&self) -> (i32, i32, i32, i32) {
        self.config.edge_padding()
    }

    pub fn mission_control_active(&self) -> bool {
        *self.interaction_mode.get() == InteractionMode::MissionControl
    }

    pub fn mouse_disconnected(&self) -> bool {
        !self.focus_follows_mouse() && !self.mouse_follows_focus()
    }

    pub fn initializing(&self) -> bool {
        *self.app_phase.get() == AppPhase::Initializing
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}

/// A Bevy `SystemParam` that provides mutable access to the application's configuration and focus state.
/// Use this in systems that need to call `set_ffm_flag` or `set_skip_reshuffle`.
#[derive(SystemParam)]
pub struct ConfigurationMut<'w> {
    config: Res<'w, Config>,
    focus_ctx: ResMut<'w, FocusContext>,
    interaction_mode: Res<'w, State<InteractionMode>>,
    app_phase: Res<'w, State<AppPhase>>,
}

#[allow(dead_code)]
impl ConfigurationMut<'_> {
    pub fn window_management_enabled(&self) -> bool {
        self.config.window_management_enabled()
    }

    pub fn focus_follows_mouse(&self) -> bool {
        self.config
            .options()
            .focus_follows_mouse
            .is_none_or(|ffm| ffm)
    }

    pub fn mouse_follows_focus(&self) -> bool {
        self.config
            .options()
            .mouse_follows_focus
            .is_none_or(|mff| mff)
    }

    pub fn auto_center(&self) -> bool {
        self.config
            .options()
            .auto_center
            .is_some_and(|centered| centered)
    }

    pub fn swipe_gesture_fingers(&self) -> Option<usize> {
        self.config.options().swipe_gesture_fingers
    }

    pub fn find_window_properties(&self, title: &str, bundle_id: &str) -> Vec<WindowParams> {
        self.config.find_window_properties(title, bundle_id)
    }

    pub fn ffm_flag(&self) -> Option<WinID> {
        self.focus_ctx.ffm_window
    }

    pub fn set_ffm_flag(&mut self, flag: Option<WinID>) {
        self.focus_ctx.ffm_window = flag;
    }

    pub fn set_skip_reshuffle(&mut self, to: bool) {
        self.focus_ctx.source = if to {
            FocusSource::Mouse
        } else {
            FocusSource::Keyboard
        };
    }

    pub fn skip_reshuffle(&self) -> bool {
        self.focus_ctx.skip_reshuffle()
    }

    pub fn edge_padding(&self) -> (i32, i32, i32, i32) {
        self.config.edge_padding()
    }

    pub fn mission_control_active(&self) -> bool {
        *self.interaction_mode.get() == InteractionMode::MissionControl
    }

    pub fn mouse_disconnected(&self) -> bool {
        !self.focus_follows_mouse() && !self.mouse_follows_focus()
    }

    pub fn initializing(&self) -> bool {
        *self.app_phase.get() == AppPhase::Initializing
    }

    pub fn config(&self) -> &Config {
        &self.config
    }
}

/// A Bevy `SystemParam` that provides immutable access to the currently active `Display` and other displays.
/// It ensures that only one display is marked as active at any given time.
#[derive(SystemParam)]
pub struct ActiveDisplay<'w, 's> {
    strip: Single<'w, 's, &'static LayoutStrip, With<ActiveWorkspaceMarker>>,
    /// The single active `Display` component, marked with `ActiveDisplayMarker`.
    display: Single<
        'w,
        's,
        (&'static Display, Option<&'static DockPosition>),
        With<ActiveDisplayMarker>,
    >,
    /// A query for all other `Display` components that are not marked as active.
    other_displays: Query<'w, 's, &'static Display, Without<ActiveDisplayMarker>>,
}

impl ActiveDisplay<'_, '_> {
    /// Returns an immutable reference to the active `Display`.
    pub fn display(&self) -> &Display {
        self.display.0
    }

    /// Returns the `CGDirectDisplayID` of the active display.
    pub fn id(&self) -> CGDirectDisplayID {
        self.display.0.id()
    }

    /// Returns an iterator over immutable references to all other displays (non-active).
    pub fn other(&self) -> impl Iterator<Item = &Display> {
        self.other_displays.iter()
    }

    pub fn active_strip(&self) -> &LayoutStrip {
        *self.strip
    }

    /// Returns the `CGRect` representing the bounds of the active display.
    pub fn bounds(&self) -> IRect {
        self.display.0.bounds()
    }

    pub fn dock(&self) -> Option<&DockPosition> {
        self.display.1
    }
}

/// A Bevy `SystemParam` that provides mutable access to the currently active `Display` and other displays.
/// It allows systems to modify the active display and its associated `LayoutStrip`s.
#[derive(SystemParam)]
pub struct ActiveDisplayMut<'w, 's> {
    strip: Single<'w, 's, &'static mut LayoutStrip, With<ActiveWorkspaceMarker>>,
    /// The single active `Display` component, marked with `ActiveDisplayMarker`.
    display: Single<'w, 's, &'static mut Display, With<ActiveDisplayMarker>>,
    /// A query for all other `Display` components that are not marked as active.
    other_displays: Query<'w, 's, &'static mut Display, Without<ActiveDisplayMarker>>,
}

impl ActiveDisplayMut<'_, '_> {
    /// Returns an immutable reference to the active `Display`.
    pub fn display(&self) -> &Display {
        &self.display
    }

    /// Returns the `CGDirectDisplayID` of the active display.
    pub fn id(&self) -> CGDirectDisplayID {
        self.display.id()
    }

    /// Returns an iterator over mutable references to all other displays (non-active).
    pub fn other(&mut self) -> impl Iterator<Item = Mut<'_, Display>> {
        self.other_displays.iter_mut()
    }

    pub fn active_strip(&mut self) -> &mut LayoutStrip {
        &mut self.strip
    }

    /// Returns the `CGRect` representing the bounds of the active display.
    pub fn bounds(&self) -> IRect {
        self.display.bounds()
    }
}

#[derive(SystemParam)]
pub struct Windows<'w, 's> {
    #[allow(clippy::type_complexity)]
    all: Query<
        'w,
        's,
        (
            &'static Window,
            Entity,
            &'static ChildOf,
            Option<&'static Unmanaged>,
        ),
    >,
    focus: Query<'w, 's, (&'static Window, Entity), With<FocusedMarker>>,
    previous_size: Query<'w, 's, (&'static Window, Entity, &'static FullWidthMarker)>,
}

impl Windows<'_, '_> {
    #[allow(clippy::type_complexity)]
    fn get_all(&self, entity: Entity) -> Option<(&Window, Entity, &ChildOf, Option<&Unmanaged>)> {
        self.all
            .get(entity)
            .inspect_err(|err| warn!("unable to find window: {err}"))
            .ok()
    }

    pub fn get_managed(&self, entity: Entity) -> Option<(&Window, Entity, Option<&Unmanaged>)> {
        self.get_all(entity)
            .map(|(window, entity, _, unmanaged)| (window, entity, unmanaged))
    }

    pub fn get(&self, entity: Entity) -> Option<&Window> {
        self.get_all(entity).map(|(window, _, _, _)| window)
    }

    pub fn find(&self, window_id: WinID) -> Option<(&Window, Entity)> {
        self.all
            .into_iter()
            .find(|(window, _, _, _)| window.id() == window_id)
            .map(|(window, entity, _, _)| (window, entity))
    }

    pub fn find_parent(&self, window_id: WinID) -> Option<(&Window, Entity, Entity)> {
        self.all.iter().find_map(|(window, entity, childof, _)| {
            (window.id() == window_id).then_some((window, entity, childof.parent()))
        })
    }

    pub fn find_managed(&self, window_id: WinID) -> Option<(&Window, Entity)> {
        self.all.iter().find_map(|(window, entity, _, unmanaged)| {
            (unmanaged.is_none() && window.id() == window_id).then_some((window, entity))
        })
    }

    pub fn focused(&self) -> Option<(&Window, Entity)> {
        self.focus.single().ok()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&Window, Entity)> {
        self.all
            .iter()
            .map(|(window, entity, _, _)| (window, entity))
    }

    pub fn full_width(&self, entity: Entity) -> Option<&FullWidthMarker> {
        self.previous_size
            .get(entity)
            .map(|(_, _, marker)| marker)
            .ok()
    }

    pub fn psn(&self, window_id: WinID, apps: &Query<&Application>) -> Option<ProcessSerialNumber> {
        self.find_parent(window_id)
            .and_then(|(_, _, parent)| apps.get(parent).ok())
            .map(|app| app.psn())
    }
}

#[derive(SystemParam)]
pub struct SmoothSwipeTracking<'w> {
    tracker: Option<ResMut<'w, SwipeContext>>,
}

impl SmoothSwipeTracking<'_> {
    pub fn sliding(&self) -> bool {
        const FINGER_LIFT_THRESHOLD: Duration = Duration::from_millis(50);
        self.tracker
            .as_ref()
            .is_some_and(|tracker| tracker.last_swipe.elapsed() < FINGER_LIFT_THRESHOLD)
    }

    pub fn position(&self) -> Option<(f64, i32)> {
        self.tracker
            .as_ref()
            .map(|tracker| (tracker.velocity, tracker.viewport_offset))
    }

    pub fn update_position(&mut self, decay: f64, viewport_offset: i32) {
        if let Some(ref mut tracker) = self.tracker {
            tracker.velocity *= decay;
            tracker.viewport_offset = viewport_offset;
        }
    }

    pub fn active(&self) -> bool {
        self.tracker.is_some()
    }

    pub fn refresh(velocity: f64, viewport_offset: i32, commands: &mut Commands) {
        commands.insert_resource(SwipeContext::new(velocity, viewport_offset));
    }
}

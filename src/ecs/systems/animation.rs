//! Spring-based window animation systems.

use bevy::ecs::entity::Entity;
use bevy::ecs::query::Has;
use bevy::ecs::system::{Commands, Populated, Query, Res};
use bevy::time::Time;
use tracing::{Level, instrument, trace};

use crate::config::Config;
use crate::ecs::params::SmoothSwipeTracking;
use crate::ecs::{RepositionMarker, ResizeMarker, SpringState};
use crate::ecs::state::ReloadGuard;
use crate::logic::spring;
use crate::manager::{Display, Origin, Size, Window};

/// Animates window repositioning using spring physics.
///
/// Each frame, windows with a `RepositionMarker` are stepped toward their
/// target position using a critically-damped spring. When the spring settles,
/// the marker is removed. During swipe tracking or reload guards, windows
/// snap instantly instead.
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = Level::TRACE, skip_all)]
pub(crate) fn animate_windows(
    windows: Populated<(&mut Window, Entity, &RepositionMarker, Option<&mut SpringState>)>,
    swipe_tracker: SmoothSwipeTracking,
    reload_guard: Option<Res<ReloadGuard>>,
    config: Res<Config>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let dt = time.delta_secs_f64();
    let instant_snap = swipe_tracker.sliding() || reload_guard.is_some();
    let params = config.spring_params();

    for (mut window, entity, RepositionMarker { origin, display_id: _ }, spring) in windows {
        if instant_snap {
            window.reposition(*origin);
            if let Some(mut s) = spring {
                s.pos_x.velocity = 0.0;
                s.pos_y.velocity = 0.0;
            }
            commands.entity(entity).try_remove::<RepositionMarker>();
            continue;
        }

        let cur = window.frame().min;
        if let Some(mut s) = spring {
            let (nx, sx) = spring::step(f64::from(cur.x), f64::from(origin.x), &mut s.pos_x, &params, dt);
            let (ny, sy) = spring::step(f64::from(cur.y), f64::from(origin.y), &mut s.pos_y, &params, dt);
            let delta = Origin::new(nx.round() as i32, ny.round() as i32);

            trace!(
                "window {} source {} dest {origin} spring → {delta}",
                window.id(), cur,
            );
            window.reposition(delta);
            if sx && sy {
                commands.entity(entity).try_remove::<RepositionMarker>();
            }
        } else {
            // No spring state — snap instantly (e.g. test environment).
            window.reposition(*origin);
            commands.entity(entity).try_remove::<RepositionMarker>();
        }
    }
}

/// Animates window resizing using spring physics.
///
/// Resizes windows toward their target size as indicated by `ResizeMarker`.
/// During reload guards, windows snap instantly. If a window is also being
/// repositioned (has `RepositionMarker`), resizing to a *larger* size is
/// deferred to avoid visual glitches.
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = Level::TRACE, skip_all)]
pub(crate) fn animate_resize_windows(
    windows: Populated<(
        &mut Window,
        Entity,
        &ResizeMarker,
        Has<RepositionMarker>,
        Option<&mut SpringState>,
    )>,
    displays: Query<&Display>,
    reload_guard: Option<Res<ReloadGuard>>,
    config: Res<Config>,
    time: Res<Time>,
    mut commands: Commands,
) {
    let dt = time.delta_secs_f64();
    let instant_snap = reload_guard.is_some();
    let params = config.spring_params();

    for (mut window, entity, ResizeMarker { size, display_id }, moving, spring) in windows {
        if instant_snap {
            let Some(display) = displays.iter().find(|d| d.id() == *display_id) else {
                continue;
            };
            if let Some(mut s) = spring {
                s.size_x.velocity = 0.0;
                s.size_y.velocity = 0.0;
            }
            window.resize(*size, display.width());
            commands.entity(entity).try_remove::<ResizeMarker>();
            continue;
        }

        if moving {
            let current_size = window.frame().size();
            if size.x > current_size.x || size.y > current_size.y {
                continue;
            }
        }
        let Some(display) = displays.iter().find(|display| display.id() == *display_id) else {
            continue;
        };

        let cur = window.frame().size();
        if let Some(mut s) = spring {
            let (nw, sw) = spring::step(f64::from(cur.x), f64::from(size.x), &mut s.size_x, &params, dt);
            let (nh, sh) = spring::step(f64::from(cur.y), f64::from(size.y), &mut s.size_y, &params, dt);
            let delta = Size::new(nw.round() as i32, nh.round() as i32);

            trace!(
                "window {} source {cur} dest {size} spring → {delta}",
                window.id(),
            );
            window.resize(delta, display.width());
            if sw && sh {
                commands.entity(entity).try_remove::<ResizeMarker>();
            }
        } else {
            window.resize(*size, display.width());
            commands.entity(entity).try_remove::<ResizeMarker>();
        }
    }
}

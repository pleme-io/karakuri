pub mod builtins;
pub mod components;
pub mod config;
pub mod render;
pub mod window;

use bevy::app::{App, Plugin, PostUpdate};
use bevy::ecs::entity::Entity;
use bevy::ecs::hierarchy::ChildOf;
use bevy::ecs::message::{Message, Messages};
use bevy::ecs::query::With;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, NonSendMut, Query, Res, ResMut};
use tracing::{debug, info};

use self::builtins::{WidgetContext, ALL_BUILTINS};
use self::components::{
    ArgbColor, BarItemComponent, BarItemGeometry, BarItemState, BarPosition, StatusBarState,
};
use self::window::StatusBarWindow;
use crate::config::Config;
use crate::ecs::FocusedMarker;
use crate::logic::bar_layout::{BarItemMeasure, LayoutPosition};
use crate::manager::{Application, Window};

/// Message for status bar item clicks.
#[derive(Clone, Debug, Message)]
#[allow(dead_code)] // Fields consumed by click handlers (Phase 2).
pub struct StatusBarItemClicked {
    pub item_id: String,
    pub button: MouseButton,
}

/// Mouse button for click events.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Variants used when constructing `StatusBarItemClicked` (Phase 2).
pub enum MouseButton {
    Left,
    Right,
    Other,
}

/// Plugin for the built-in status bar.
///
/// Registers ECS components, resources, and systems for bar rendering.
/// The `StatusBarWindow` is a NonSend resource (main thread only).
pub struct StatusBarPlugin;

impl Plugin for StatusBarPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages<StatusBarItemClicked>>();
        app.init_resource::<StatusBarState>();

        // Systems run in PostUpdate, after window layout but before frame end.
        // spawn → update_builtins → layout → render
        app.add_systems(
            PostUpdate,
            (
                spawn_bar_system,
                update_builtin_items_system,
                layout_bar_items_system,
                render_bar_system,
            )
                .chain(),
        );

        info!("status bar plugin initialized");
    }
}

/// System: creates the bar window and spawns item entities from config.
///
/// Runs once on first frame when config is loaded, and again on config reload.
fn spawn_bar_system(
    mut commands: Commands,
    mut bar_state: ResMut<StatusBarState>,
    config: Res<Config>,
    mut bar_window: Option<NonSendMut<StatusBarWindow>>,
) {
    let cfg = config.status_bar();

    // Skip if disabled or already visible
    if !cfg.enabled || bar_state.visible {
        return;
    }

    // Create the window (requires NonSend resource — main thread)
    let Some(ref mut window) = bar_window else {
        return;
    };

    let bg = ArgbColor::from_hex(&cfg.color).unwrap_or(ArgbColor {
        a: 0.8,
        r: 0.118,
        g: 0.118,
        b: 0.18,
    });

    window.create(
        f64::from(cfg.height),
        &cfg.position,
        cfg.blur_radius,
        bg.r,
        bg.g,
        bg.b,
        bg.a,
    );

    bar_state.visible = true;
    bar_state.height = f64::from(cfg.height);
    bar_state.needs_redraw = true;

    // Spawn ECS entities for each configured item
    let defaults = &cfg.defaults;
    for (idx, item_cfg) in cfg.items.iter().enumerate() {
        let icon_color = item_cfg
            .icon_color
            .as_deref()
            .and_then(ArgbColor::from_hex)
            .unwrap_or_else(|| {
                ArgbColor::from_hex(&defaults.icon_color).unwrap_or(ArgbColor::WHITE)
            });
        let label_color = item_cfg
            .label_color
            .as_deref()
            .and_then(ArgbColor::from_hex)
            .unwrap_or_else(|| {
                ArgbColor::from_hex(&defaults.label_color).unwrap_or(ArgbColor::WHITE)
            });
        let background_color = item_cfg
            .background_color
            .as_deref()
            .and_then(ArgbColor::from_hex)
            .unwrap_or_else(|| {
                ArgbColor::from_hex(&defaults.background_color).unwrap_or(ArgbColor::TRANSPARENT)
            });

        let icon_highlight_color = item_cfg
            .icon_highlight_color
            .as_deref()
            .and_then(ArgbColor::from_hex)
            .unwrap_or_else(|| icon_color.clone());
        let label_highlight_color = item_cfg
            .label_highlight_color
            .as_deref()
            .and_then(ArgbColor::from_hex)
            .unwrap_or_else(|| label_color.clone());

        commands.spawn((
            BarItemComponent {
                id: if item_cfg.id.is_empty() {
                    format!("item_{idx}")
                } else {
                    item_cfg.id.clone()
                },
                item_type: item_cfg.item_type.clone(),
                position: item_cfg.position.clone(),
                order: idx as u32,
                script: item_cfg.script.clone(),
                click_script: item_cfg.click_script.clone(),
                update_freq: item_cfg.update_freq,
                subscriptions: item_cfg.subscribe.clone(),
                drawing: item_cfg.drawing.unwrap_or(true),
                updates_when_shown: item_cfg.updates.as_deref() == Some("when_shown"),
                ignore_association: item_cfg.ignore_association.unwrap_or(false),
                display_filter: item_cfg.display.clone(),
                space_filter: item_cfg.space.clone().unwrap_or_default(),
            },
            BarItemState {
                icon: item_cfg.icon.clone().unwrap_or_default(),
                label: item_cfg.label.clone().unwrap_or_default(),
                icon_color,
                label_color,
                icon_highlight_color,
                label_highlight_color,
                highlight: item_cfg.highlight.unwrap_or(false),
                icon_font: item_cfg.icon_font.clone(),
                label_font: item_cfg.label_font.clone(),
                background_color,
                border_color: item_cfg
                    .background_border_color
                    .as_deref()
                    .and_then(ArgbColor::from_hex)
                    .unwrap_or(ArgbColor::TRANSPARENT),
                border_width: item_cfg.background_border_width.unwrap_or(0.0),
                corner_radius: item_cfg.background_corner_radius.unwrap_or(0.0),
                background_clip: item_cfg.background_clip.unwrap_or(0.0),
                blur_radius: item_cfg.blur_radius.unwrap_or(0),
                hidden: false,
                align: match item_cfg.align.as_deref() {
                    Some("center") => 1,
                    Some("right") => 2,
                    _ => 0,
                },
                y_offset: item_cfg.y_offset.unwrap_or(0.0),
            },
            BarItemGeometry::default(),
        ));
    }

    // Auto-spawn built-in widgets not already defined in config
    let default_label_color =
        ArgbColor::from_hex(&defaults.label_color).unwrap_or(ArgbColor::WHITE);
    for builtin in ALL_BUILTINS {
        let already_defined = cfg.items.iter().any(|i| i.id == builtin.id());
        if !already_defined {
            commands.spawn((
                builtins::builtin_component(*builtin),
                builtins::builtin_state(default_label_color.clone()),
                BarItemGeometry::default(),
            ));
        }
    }

    info!(
        "status_bar: spawned {} configured items + {} builtins",
        cfg.items.len(),
        ALL_BUILTINS
            .iter()
            .filter(|b| !cfg.items.iter().any(|i| i.id == b.id()))
            .count()
    );
}

/// System: update built-in item labels from ECS state using the widget trait.
fn update_builtin_items_system(
    bar_state: Res<StatusBarState>,
    mut bar_items: Query<(&BarItemComponent, &mut BarItemState)>,
    focused_window: Query<(&Window, Entity, &ChildOf), With<FocusedMarker>>,
    apps: Query<&Application>,
) {
    if !bar_state.visible {
        return;
    }

    // Pre-resolve the focused app bundle ID once for all widgets.
    let bundle_id_owned: Option<String> = focused_window
        .single()
        .ok()
        .and_then(|(_, _, child_of)| apps.get(child_of.parent()).ok())
        .and_then(|app| app.bundle_id().map(str::to_owned));
    let ctx = WidgetContext::with_bundle_id(bundle_id_owned.as_deref());

    for (component, mut state) in &mut bar_items {
        // Find a matching built-in widget for this item's ID
        if let Some(builtin) = ALL_BUILTINS.iter().find(|b| b.id() == component.id) {
            if let Some(new_label) = builtin.update(&ctx) {
                if state.label != new_label {
                    state.label = new_label;
                }
            }
        }
    }
}

/// System: compute layout positions for all bar items.
fn layout_bar_items_system(
    mut bar_state: ResMut<StatusBarState>,
    config: Res<Config>,
    bar_window: Option<NonSendMut<StatusBarWindow>>,
    mut items: Query<(&BarItemComponent, &BarItemState, &mut BarItemGeometry)>,
) {
    if !bar_state.visible {
        return;
    }

    let Some(ref window) = bar_window else {
        return;
    };

    let bar_width = window.width();
    if bar_width <= 0.0 {
        return;
    }

    let cfg = config.status_bar();
    let font_name = cfg.font.split(':').next().unwrap_or("Hack Nerd Font");
    let font_size = cfg
        .font
        .split(':')
        .nth(2)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(14.0);

    // Collect measured items
    let mut measures: Vec<(usize, BarItemMeasure)> = Vec::new();

    for (idx, (component, state, _geom)) in items.iter().enumerate() {
        if state.hidden {
            continue;
        }

        // Measure text width
        let icon_w = if state.icon.is_empty() {
            0.0
        } else {
            render::measure_text(&state.icon, font_name, font_size) + 4.0
        };
        let label_w = if state.label.is_empty() {
            0.0
        } else {
            render::measure_text(&state.label, font_name, font_size)
        };
        let content_w = icon_w + label_w;

        let pad_l = cfg.defaults.padding_left;
        let pad_r = cfg.defaults.padding_right;

        let position = match component.position {
            BarPosition::Left => LayoutPosition::Left,
            BarPosition::Right => LayoutPosition::Right,
            BarPosition::Center => LayoutPosition::Center,
            BarPosition::Q => LayoutPosition::Q,
            BarPosition::E => LayoutPosition::E,
        };

        measures.push((
            idx,
            BarItemMeasure {
                index: idx,
                position,
                order: component.order,
                width: content_w,
                padding_left: pad_l,
                padding_right: pad_r,
            },
        ));
    }

    let measure_refs: Vec<BarItemMeasure> = measures.iter().map(|(_, m)| m.clone()).collect();

    let notch_center = bar_width / 2.0;
    let notch_width = f64::from(cfg.notch_width);

    let placements = crate::logic::bar_layout::compute_bar_layout(
        &measure_refs,
        bar_width,
        cfg.padding_left,
        cfg.padding_right,
        notch_center,
        notch_width,
    );

    // Apply placements back to entities
    let mut layout_changed = false;
    for placement in &placements {
        if let Some((_component, _state, mut geom)) = items.iter_mut().nth(placement.index) {
            let old_x = geom.x;
            geom.x = placement.x;
            geom.y = 0.0;
            if let Some((_, measure)) = measures.iter().find(|(_, m)| m.index == placement.index) {
                geom.width = measure.padding_left + measure.width + measure.padding_right;
                geom.height = bar_state.height;
                geom.padding_left = measure.padding_left;
                geom.padding_right = measure.padding_right;
            }
            if (old_x - geom.x).abs() > 0.01 {
                layout_changed = true;
            }
        }
    }

    if layout_changed {
        bar_state.needs_redraw = true;
    }
}

/// System: render bar items by locking focus on the content view and drawing.
fn render_bar_system(
    mut bar_state: ResMut<StatusBarState>,
    config: Res<Config>,
    bar_window: Option<NonSendMut<StatusBarWindow>>,
    items: Query<(&BarItemState, &BarItemGeometry)>,
) {
    if !bar_state.visible || !bar_state.needs_redraw {
        return;
    }

    let Some(ref window) = bar_window else {
        return;
    };

    let cfg = config.status_bar();
    let font_name = cfg.font.split(':').next().unwrap_or("Hack Nerd Font");
    let font_size = cfg
        .font
        .split(':')
        .nth(2)
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(14.0);

    // Collect items for the draw call
    let draw_items: Vec<(BarItemState, BarItemGeometry)> = items
        .iter()
        .map(|(s, g)| (s.clone(), g.clone()))
        .collect();

    if draw_items.is_empty() {
        return;
    }

    let font_name_owned = font_name.to_string();
    let drawn = window.lock_and_draw(|bar_height| {
        render::draw_bar_items(&draw_items, bar_height, &font_name_owned, font_size);
    });

    if drawn {
        bar_state.needs_redraw = false;
        debug!("status_bar: rendered {} items", draw_items.len());
    }
}

/// Convert bar position from config to layout position.
impl From<&BarPosition> for LayoutPosition {
    fn from(pos: &BarPosition) -> Self {
        match pos {
            BarPosition::Left => Self::Left,
            BarPosition::Right => Self::Right,
            BarPosition::Center => Self::Center,
            BarPosition::Q => Self::Q,
            BarPosition::E => Self::E,
        }
    }
}

//! Pure layout-frame computation — no ECS, no macOS APIs.
//!
//! Takes raw layout output (entity + local frame) and transforms it into
//! final absolute screen coordinates, handling edge padding, sliver
//! compression, and reload-guard dedup.

use bevy::ecs::entity::Entity;
use bevy::math::{IRect, IVec2};
use std::collections::HashMap;

/// Parameters describing the display geometry.
#[derive(Clone, Copy, Debug)]
pub struct DisplayGeometry {
    /// Full display bounds in absolute coordinates.
    pub bounds: IRect,
    /// Menubar height in pixels (top of usable area).
    pub menubar_height: i32,
    /// Dock height at bottom (0 if dock is on side or hidden).
    pub dock_bottom: i32,
    /// Absolute origin of the display (for coordinate transforms).
    pub origin: IVec2,
}

/// Layout padding configuration.
#[derive(Clone, Copy, Debug)]
pub struct LayoutPadding {
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub left: i32,
}

impl From<(i32, i32, i32, i32)> for LayoutPadding {
    fn from((top, right, bottom, left): (i32, i32, i32, i32)) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }
}

/// Sliver configuration for off-screen windows.
#[derive(Clone, Copy, Debug)]
pub struct SliverConfig {
    /// Number of pixels to show when a window is off-screen.
    pub width: i32,
    /// Fraction of display height to keep visible (0.0–1.0).
    pub height_ratio: f64,
}

/// Information about a single window for layout computation.
#[derive(Clone, Copy, Debug)]
pub struct WindowInfo {
    /// The raw frame from `calculate_layout` (local coords, no padding).
    pub layout_frame: IRect,
    /// The window's current frame (for diff detection).
    pub old_frame: IRect,
    /// Per-window horizontal padding (e.g., from app border insets).
    pub h_pad: i32,
    /// Whether this window is in a stack.
    pub is_stacked: bool,
}

/// What changed for a window after layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameUpdate {
    /// The final absolute frame.
    pub frame: IRect,
    /// Whether the position changed from old_frame.
    pub moved: bool,
    /// Whether the size changed from old_frame.
    pub resized: bool,
}

/// Compute final absolute frames for all windows in a layout strip.
///
/// This is a pure function: given display geometry, padding, sliver config,
/// and per-window info, it produces the final frames without touching any
/// ECS or platform API.
///
/// Returns only windows that actually need updating (position or size changed).
/// Windows that match their pre-positions (reload guard) are excluded.
#[allow(clippy::cast_possible_truncation)]
pub fn compute_final_frames(
    display: &DisplayGeometry,
    padding: &LayoutPadding,
    sliver: &SliverConfig,
    swiping: bool,
    pre_positions: Option<&HashMap<Entity, IRect>>,
    windows: &[(Entity, WindowInfo)],
) -> Vec<(Entity, FrameUpdate)> {
    let display_width = display.bounds.width();
    let usable_height = display.bounds.height() - display.dock_bottom;
    let display_x = display.origin.x;

    // Padded viewport boundaries in absolute coordinates.
    let abs_pad_left = display_x + padding.left;
    let abs_pad_right = display_x + display_width - padding.right;

    let mut results = Vec::with_capacity(windows.len());

    for &(entity, ref info) in windows {
        // Apply absolute coordinates: layout_frame is in local (0,0-based) coords.
        let mut frame = IRect {
            min: display.origin + info.layout_frame.min,
            max: display.origin + info.layout_frame.max,
        };

        // Apply horizontal edge padding offset.
        frame.min.x += padding.left;
        frame.max.x += padding.left;

        // Sliver detection: window with very little visible area in padded viewport.
        let visible_left = frame.min.x.max(abs_pad_left);
        let visible_right = frame.max.x.min(abs_pad_right);
        let visible = (visible_right - visible_left).max(0);
        let is_off_screen = visible <= sliver.width.max(20);

        if is_off_screen {
            let h_pad = info.h_pad;
            let width = frame.width();
            let window_center = frame.center().x;
            if window_center <= abs_pad_left {
                // Off-screen left: show sliver from left edge.
                frame.min.x = display_x + sliver.width + h_pad - width;
                frame.max.x = display_x + sliver.width + h_pad;
            } else {
                // Off-screen right: show sliver from right edge.
                frame.min.x = display_x + display_width - sliver.width - h_pad;
                frame.max.x = frame.min.x + width;
            }

            if swiping || info.is_stacked {
                // Keep full height during swipe or for stacked windows.
                frame.min.y += display.menubar_height + padding.top;
                frame.max.y += display.menubar_height + padding.top;
            } else {
                // Compress height for off-screen non-stacked windows.
                let inset = (f64::from(usable_height - padding.top - padding.bottom)
                    * (1.0 - sliver.height_ratio)
                    / 2.0) as i32;
                frame.min.y += display.menubar_height + padding.top + inset;
                frame.max.y += display.menubar_height + padding.top - inset;
            }
        } else {
            frame.min.y += display.menubar_height + padding.top;
            frame.max.y += display.menubar_height + padding.top;
        }

        // Skip windows whose target matches pre-reload position.
        if let Some(pre) = pre_positions
            && let Some(pre_frame) = pre.get(&entity)
            && pre_frame.min == frame.min
            && pre_frame.size() == frame.size()
        {
            continue;
        }

        let moved = info.old_frame.min != frame.min;
        let resized = info.old_frame.size() != frame.size();

        if moved || resized {
            results.push((
                entity,
                FrameUpdate {
                    frame,
                    moved,
                    resized,
                },
            ));
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_display() -> DisplayGeometry {
        DisplayGeometry {
            bounds: IRect::new(0, 0, 1920, 1080),
            menubar_height: 25,
            dock_bottom: 0,
            origin: IVec2::ZERO,
        }
    }

    fn test_padding() -> LayoutPadding {
        LayoutPadding {
            top: 10,
            right: 10,
            bottom: 10,
            left: 10,
        }
    }

    fn test_sliver() -> SliverConfig {
        SliverConfig {
            width: 30,
            height_ratio: 0.3,
        }
    }

    fn make_entity(world: &mut bevy::ecs::world::World) -> Entity {
        world.spawn_empty().id()
    }

    #[test]
    fn on_screen_window_gets_padding_and_menubar() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(0, 0, 960, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        assert_eq!(results.len(), 1);

        let (_, update) = &results[0];
        // x should be shifted by pad_left (10)
        assert_eq!(update.frame.min.x, 10);
        assert_eq!(update.frame.max.x, 970);
        // y should be shifted by menubar (25) + pad_top (10)
        assert_eq!(update.frame.min.y, 35);
        assert_eq!(update.frame.max.y, 1070);
        assert!(update.moved);
        assert!(update.resized);
    }

    #[test]
    fn off_screen_left_shows_sliver() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        // Window far to the left (center below pad_left)
        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(-500, 0, -100, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        assert_eq!(results.len(), 1);

        let (_, update) = &results[0];
        // Off-screen left: max.x should be sliver.width (30)
        assert_eq!(update.frame.max.x, 30);
        assert_eq!(update.frame.min.x, 30 - 400); // sliver_width - width
    }

    #[test]
    fn off_screen_right_shows_sliver() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        // Window far to the right
        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(2000, 0, 2400, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        assert_eq!(results.len(), 1);

        let (_, update) = &results[0];
        // Off-screen right: min.x should be display_width - sliver_width
        assert_eq!(update.frame.min.x, 1920 - 30);
    }

    #[test]
    fn stacked_off_screen_keeps_full_height() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(-500, 0, -100, 500),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: true,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        let (_, update) = &results[0];

        // Stacked windows keep full height (no inset).
        let expected_height = 500; // same as layout_frame height
        assert_eq!(update.frame.height(), expected_height);
    }

    #[test]
    fn non_stacked_off_screen_compresses_height() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(-500, 0, -100, 500),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        let (_, update) = &results[0];

        // Non-stacked off-screen should be compressed.
        assert!(update.frame.height() < 500);
    }

    #[test]
    fn swiping_off_screen_keeps_full_height() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(-500, 0, -100, 500),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, true, None, &windows);
        let (_, update) = &results[0];

        // During swipe, even non-stacked keeps full height.
        assert_eq!(update.frame.height(), 500);
    }

    #[test]
    fn pre_positions_skip_unchanged() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        // Compute the expected final frame first.
        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(0, 0, 960, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];
        let first = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        let final_frame = first[0].1.frame;

        // Now with pre_positions matching the final frame — should be skipped.
        let mut pre = HashMap::new();
        pre.insert(entity, final_frame);
        let results =
            compute_final_frames(&display, &padding, &sliver, false, Some(&pre), &windows);
        assert!(results.is_empty(), "should skip when pre-position matches");
    }

    #[test]
    fn unchanged_frame_not_emitted() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        // Set old_frame to match the expected final position.
        let windows_first = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(0, 0, 960, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];
        let first =
            compute_final_frames(&display, &padding, &sliver, false, None, &windows_first);
        let final_frame = first[0].1.frame;

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(0, 0, 960, 1035),
                old_frame: final_frame,
                h_pad: 0,
                is_stacked: false,
            },
        )];
        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        assert!(
            results.is_empty(),
            "should not emit update when frame unchanged"
        );
    }

    #[test]
    fn h_pad_adjusts_sliver_position() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        // Off-screen right with h_pad=5
        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(2000, 0, 2400, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 5,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        let (_, update) = &results[0];

        // h_pad shifts sliver inward: display_width - sliver_width - h_pad
        assert_eq!(update.frame.min.x, 1920 - 30 - 5);
    }

    #[test]
    fn multi_display_origin_offset() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        // Second display at x=1920
        let display = DisplayGeometry {
            bounds: IRect::new(1920, 0, 3840, 1080),
            menubar_height: 25,
            dock_bottom: 0,
            origin: IVec2::new(1920, 0),
        };
        let padding = LayoutPadding {
            top: 0,
            right: 0,
            bottom: 0,
            left: 0,
        };
        let sliver = test_sliver();

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(0, 0, 960, 1055),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        let (_, update) = &results[0];

        // Should be offset by display origin (1920, 0)
        assert_eq!(update.frame.min.x, 1920);
        assert_eq!(update.frame.max.x, 2880);
    }

    #[test]
    fn dock_bottom_reduces_usable_height_for_sliver_compression() {
        let mut world = bevy::ecs::world::World::new();
        let e1 = make_entity(&mut world);
        let e2 = make_entity(&mut world);

        let display_no_dock = DisplayGeometry {
            bounds: IRect::new(0, 0, 1920, 1080),
            menubar_height: 25,
            dock_bottom: 0,
            origin: IVec2::ZERO,
        };
        let display_dock = DisplayGeometry {
            bounds: IRect::new(0, 0, 1920, 1080),
            menubar_height: 25,
            dock_bottom: 80,
            origin: IVec2::ZERO,
        };
        let padding = test_padding();
        // Use a high height_ratio so compression doesn't invert the rect.
        let sliver = SliverConfig {
            width: 30,
            height_ratio: 0.9,
        };

        let make_window = |e: Entity| {
            vec![(
                e,
                WindowInfo {
                    layout_frame: IRect::new(-500, 0, -100, 1035),
                    old_frame: IRect::new(0, 0, 0, 0),
                    h_pad: 0,
                    is_stacked: false,
                },
            )]
        };

        let r1 = compute_final_frames(
            &display_no_dock,
            &padding,
            &sliver,
            false,
            None,
            &make_window(e1),
        );
        let r2 = compute_final_frames(
            &display_dock,
            &padding,
            &sliver,
            false,
            None,
            &make_window(e2),
        );

        // With dock, compressed height should be different (less usable space).
        let h1 = r1[0].1.frame.height();
        let h2 = r2[0].1.frame.height();
        assert_ne!(h1, h2, "dock should affect compressed height");
        // Dock reduces usable height → smaller inset → larger compressed window.
        // Actually: less usable_height → less inset → taller compressed window.
        // No: inset = (usable - pad) * (1 - ratio) / 2
        //   More usable → more inset → smaller window. Wait...
        //   height = layout_height - 2*inset. More inset → smaller.
        //   dock reduces usable → reduces inset → increases height.
        // So h2 > h1 when dock is present (less compression).
        assert!(h2 > h1, "dock should reduce compression (larger window): h1={h1}, h2={h2}");
    }

    #[test]
    fn negative_display_origin() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        // Display to the left of the primary (negative x origin)
        let display = DisplayGeometry {
            bounds: IRect::new(-1920, 0, 0, 1080),
            menubar_height: 25,
            dock_bottom: 0,
            origin: IVec2::new(-1920, 0),
        };
        let padding = test_padding();
        let sliver = test_sliver();

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(0, 0, 960, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        assert_eq!(results.len(), 1);

        let (_, update) = &results[0];
        // x: origin(-1920) + layout(0) + pad_left(10) = -1910
        assert_eq!(update.frame.min.x, -1910);
        assert_eq!(update.frame.max.x, -950);
    }

    #[test]
    fn sliver_height_ratio_one_means_no_compression() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = SliverConfig {
            width: 30,
            height_ratio: 1.0, // No compression
        };

        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(-500, 0, -100, 500),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        let (_, update) = &results[0];

        // height_ratio=1.0 → inset=0 → full layout height preserved
        assert_eq!(update.frame.height(), 500);
    }

    #[test]
    fn sliver_height_ratio_zero_maximally_compresses() {
        let mut world = bevy::ecs::world::World::new();
        let entity = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = SliverConfig {
            width: 30,
            height_ratio: 0.0, // Maximum compression
        };

        // Use a tall window so compression is visible
        let windows = vec![(
            entity,
            WindowInfo {
                layout_frame: IRect::new(-500, 0, -100, 1035),
                old_frame: IRect::new(0, 0, 0, 0),
                h_pad: 0,
                is_stacked: false,
            },
        )];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        let (_, update) = &results[0];

        // height_ratio=0 → inset = (usable - pad) / 2 → heavily compressed
        let usable = 1080 - 0; // no dock
        let inset = (f64::from(usable - 10 - 10) * 1.0 / 2.0) as i32;
        let expected_height = 1035 - 2 * inset;
        assert_eq!(update.frame.height(), expected_height);
    }

    #[test]
    fn mixed_stacked_and_non_stacked_off_screen() {
        let mut world = bevy::ecs::world::World::new();
        let stacked = make_entity(&mut world);
        let non_stacked = make_entity(&mut world);

        let display = test_display();
        let padding = test_padding();
        let sliver = SliverConfig {
            width: 30,
            height_ratio: 0.9,
        };

        let windows = vec![
            (
                stacked,
                WindowInfo {
                    layout_frame: IRect::new(-500, 0, -100, 500),
                    old_frame: IRect::new(0, 0, 0, 0),
                    h_pad: 0,
                    is_stacked: true,
                },
            ),
            (
                non_stacked,
                WindowInfo {
                    layout_frame: IRect::new(-500, 0, -100, 500),
                    old_frame: IRect::new(0, 0, 0, 0),
                    h_pad: 0,
                    is_stacked: false,
                },
            ),
        ];

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        assert_eq!(results.len(), 2);

        let stacked_h = results[0].1.frame.height();
        let non_stacked_h = results[1].1.frame.height();

        // Stacked keeps full height, non-stacked is compressed
        assert_eq!(stacked_h, 500);
        assert!(non_stacked_h < 500);
    }

    #[test]
    fn empty_windows_returns_empty() {
        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn many_windows_all_produce_results() {
        let mut world = bevy::ecs::world::World::new();
        let windows: Vec<_> = (0..100)
            .map(|i| {
                let entity = make_entity(&mut world);
                (
                    entity,
                    WindowInfo {
                        layout_frame: IRect::new(i * 19, 0, (i + 1) * 19, 1035),
                        old_frame: IRect::new(0, 0, 0, 0),
                        h_pad: 0,
                        is_stacked: false,
                    },
                )
            })
            .collect();

        let display = test_display();
        let padding = test_padding();
        let sliver = test_sliver();

        let results = compute_final_frames(&display, &padding, &sliver, false, None, &windows);
        // All windows should produce a result (all have different frames from old_frame)
        assert_eq!(results.len(), 100);
    }
}

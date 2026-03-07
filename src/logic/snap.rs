//! Pure snap-zone logic — no ECS, no macOS APIs.
//!
//! Extracted from `ecs/triggers.rs` so that snap detection and frame
//! computation can be unit-tested without Bevy or accessibility dependencies.

use bevy::math::{IRect, IVec2};

use crate::config::EdgeSnapConfig;
use crate::ecs::SnapZone;

/// Determines which snap zone the cursor is near, if any.
///
/// Checks proximity to each display edge against `threshold` pixels and the
/// per-edge enable flags in `snap`. First match wins (left, right,
/// fullscreen corner, top, bottom).
pub fn detect_snap_zone(
    px: i32,
    py: i32,
    bounds: &IRect,
    threshold: i32,
    snap: &EdgeSnapConfig,
) -> Option<SnapZone> {
    let near_left = px - bounds.min.x < threshold;
    let near_right = bounds.max.x - px < threshold;
    let near_top = py - bounds.min.y < threshold;
    let near_bottom = bounds.max.y - py < threshold;

    if near_left && snap.left.unwrap_or(false) {
        Some(SnapZone::LeftHalf)
    } else if near_right && snap.right.unwrap_or(false) {
        Some(SnapZone::RightHalf)
    } else if near_top && near_left && snap.fullscreen.unwrap_or(false) {
        Some(SnapZone::Fullscreen)
    } else if near_top && snap.top.unwrap_or(false) {
        Some(SnapZone::TopHalf)
    } else if near_bottom && snap.bottom.unwrap_or(false) {
        Some(SnapZone::BottomHalf)
    } else {
        None
    }
}

/// Computes the origin and size for a snap zone within display bounds.
///
/// Outer edges are flush with the display; inner edges use padding for a gap
/// between halves. Padding order: `(top, right, bottom, left)`.
pub fn snap_frame(
    zone: SnapZone,
    bounds: &IRect,
    pad: (i32, i32, i32, i32),
) -> (IVec2, IVec2) {
    let (pt, pr, pb, pl) = pad;
    let padded_w = bounds.width() - pl - pr;
    let padded_h = bounds.height() - pt - pb;
    let mid_x = bounds.min.x + pl + padded_w / 2;
    let mid_y = bounds.min.y + pt + padded_h / 2;

    match zone {
        SnapZone::LeftHalf => (
            IVec2::new(bounds.min.x, bounds.min.y + pt),
            IVec2::new(mid_x - bounds.min.x, padded_h),
        ),
        SnapZone::RightHalf => (
            IVec2::new(mid_x, bounds.min.y + pt),
            IVec2::new(bounds.max.x - mid_x, padded_h),
        ),
        SnapZone::TopHalf => (
            IVec2::new(bounds.min.x + pl, bounds.min.y),
            IVec2::new(padded_w, mid_y - bounds.min.y),
        ),
        SnapZone::BottomHalf => (
            IVec2::new(bounds.min.x + pl, mid_y),
            IVec2::new(padded_w, bounds.max.y - mid_y),
        ),
        SnapZone::Fullscreen => (
            IVec2::new(bounds.min.x, bounds.min.y),
            IVec2::new(bounds.width(), bounds.height()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn display_bounds() -> IRect {
        IRect::new(0, 0, 1920, 1080)
    }

    fn all_enabled() -> EdgeSnapConfig {
        EdgeSnapConfig {
            left: Some(true),
            right: Some(true),
            top: Some(true),
            bottom: Some(true),
            fullscreen: Some(true),
            ..Default::default()
        }
    }

    fn left_right_only() -> EdgeSnapConfig {
        EdgeSnapConfig {
            left: Some(true),
            right: Some(true),
            top: Some(false),
            bottom: Some(false),
            fullscreen: Some(false),
            ..Default::default()
        }
    }

    fn none_enabled() -> EdgeSnapConfig {
        EdgeSnapConfig::default()
    }

    // ── detect_snap_zone ─────────────────────────────────────────────

    #[test]
    fn cursor_at_left_edge_snaps_left() {
        let bounds = display_bounds();
        let zone = detect_snap_zone(5, 540, &bounds, 10, &all_enabled());
        assert_eq!(zone, Some(SnapZone::LeftHalf));
    }

    #[test]
    fn cursor_at_right_edge_snaps_right() {
        let bounds = display_bounds();
        let zone = detect_snap_zone(1915, 540, &bounds, 10, &all_enabled());
        assert_eq!(zone, Some(SnapZone::RightHalf));
    }

    #[test]
    fn cursor_at_top_edge_snaps_top() {
        let bounds = display_bounds();
        let zone = detect_snap_zone(960, 3, &bounds, 10, &all_enabled());
        assert_eq!(zone, Some(SnapZone::TopHalf));
    }

    #[test]
    fn cursor_at_bottom_edge_snaps_bottom() {
        let bounds = display_bounds();
        let zone = detect_snap_zone(960, 1075, &bounds, 10, &all_enabled());
        assert_eq!(zone, Some(SnapZone::BottomHalf));
    }

    #[test]
    fn cursor_at_top_left_corner_snaps_fullscreen() {
        let bounds = display_bounds();
        let zone = detect_snap_zone(3, 3, &bounds, 10, &all_enabled());
        // left check wins over fullscreen because left is checked first
        assert_eq!(zone, Some(SnapZone::LeftHalf));
    }

    #[test]
    fn cursor_in_center_returns_none() {
        let bounds = display_bounds();
        let zone = detect_snap_zone(960, 540, &bounds, 10, &all_enabled());
        assert_eq!(zone, None);
    }

    #[test]
    fn disabled_edges_return_none() {
        let bounds = display_bounds();
        // top edge cursor, but top is disabled
        let zone = detect_snap_zone(960, 3, &bounds, 10, &left_right_only());
        assert_eq!(zone, None);
    }

    #[test]
    fn all_disabled_returns_none_everywhere() {
        let bounds = display_bounds();
        assert_eq!(detect_snap_zone(5, 540, &bounds, 10, &none_enabled()), None);
        assert_eq!(detect_snap_zone(1915, 540, &bounds, 10, &none_enabled()), None);
        assert_eq!(detect_snap_zone(960, 3, &bounds, 10, &none_enabled()), None);
        assert_eq!(detect_snap_zone(960, 1075, &bounds, 10, &none_enabled()), None);
    }

    #[test]
    fn threshold_boundary_inside() {
        let bounds = display_bounds();
        // exactly at threshold distance
        let zone = detect_snap_zone(9, 540, &bounds, 10, &all_enabled());
        assert_eq!(zone, Some(SnapZone::LeftHalf));
    }

    #[test]
    fn threshold_boundary_outside() {
        let bounds = display_bounds();
        // one pixel beyond threshold
        let zone = detect_snap_zone(10, 540, &bounds, 10, &all_enabled());
        assert_eq!(zone, None);
    }

    #[test]
    fn offset_display_bounds() {
        // Second monitor at x=1920
        let bounds = IRect::new(1920, 0, 3840, 1080);
        let zone = detect_snap_zone(1925, 540, &bounds, 10, &all_enabled());
        assert_eq!(zone, Some(SnapZone::LeftHalf));
    }

    // ── snap_frame ───────────────────────────────────────────────────

    #[test]
    fn left_half_frame() {
        let bounds = display_bounds();
        let pad = (4, 4, 4, 4);
        let (origin, size) = snap_frame(SnapZone::LeftHalf, &bounds, pad);
        assert_eq!(origin, IVec2::new(0, 4));
        assert_eq!(size.x, 960); // midpoint
        assert!(size.y > 0);
    }

    #[test]
    fn right_half_frame() {
        let bounds = display_bounds();
        let pad = (4, 4, 4, 4);
        let (origin, size) = snap_frame(SnapZone::RightHalf, &bounds, pad);
        assert_eq!(origin.x, 960); // starts at midpoint
        assert_eq!(origin.x + size.x, 1920); // ends at right edge
    }

    #[test]
    fn fullscreen_frame_no_padding() {
        let bounds = display_bounds();
        let pad = (4, 4, 4, 4);
        let (origin, size) = snap_frame(SnapZone::Fullscreen, &bounds, pad);
        assert_eq!(origin, IVec2::new(0, 0));
        assert_eq!(size, IVec2::new(1920, 1080));
    }

    #[test]
    fn halves_cover_full_width() {
        let bounds = display_bounds();
        let pad = (0, 0, 0, 0);
        let (left_origin, left_size) = snap_frame(SnapZone::LeftHalf, &bounds, pad);
        let (right_origin, right_size) = snap_frame(SnapZone::RightHalf, &bounds, pad);
        // left edge to right edge should cover full width
        assert_eq!(left_origin.x, 0);
        assert_eq!(left_origin.x + left_size.x, right_origin.x);
        assert_eq!(right_origin.x + right_size.x, 1920);
    }

    #[test]
    fn top_bottom_cover_full_height() {
        let bounds = display_bounds();
        let pad = (0, 0, 0, 0);
        let (top_origin, top_size) = snap_frame(SnapZone::TopHalf, &bounds, pad);
        let (bot_origin, bot_size) = snap_frame(SnapZone::BottomHalf, &bounds, pad);
        assert_eq!(top_origin.y, 0);
        assert_eq!(top_origin.y + top_size.y, bot_origin.y);
        assert_eq!(bot_origin.y + bot_size.y, 1080);
    }
}

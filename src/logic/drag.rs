//! Pure drag geometry helpers — no ECS, no macOS APIs.
//!
//! Extracted from `triggers.rs` so that window placement logic can be
//! unit-tested without Bevy or accessibility dependencies.

use bevy::math::{IRect, IVec2};

/// Clamps a window origin so the window stays within display bounds.
///
/// Given a desired origin rect, a window size, and the display bounds,
/// returns a new rect whose origin is clamped so no edge exceeds the bounds.
///
/// # Arguments
/// - `origin` — desired window rect (only `min` corner is used)
/// - `size` — window width/height as `IVec2`
/// - `bounds` — display bounds to clamp within
pub fn clamp_origin_to_bounds(origin: IRect, size: IVec2, bounds: IRect) -> IRect {
    let max = (bounds.max - size).max(bounds.min);
    let min = origin.min.clamp(bounds.min, max);
    IRect::from_corners(min, min + size)
}

/// Tries 8 offset positions to find one where the frame fits entirely
/// within bounds. Returns the first valid offset, or the original frame
/// if none fits.
///
/// Used when spawning unmanaged windows to avoid stacking them exactly
/// on top of each other.
///
/// # Arguments
/// - `frame` — current window frame
/// - `bounds` — display bounds
/// - `offset` — pixel distance to shift in each candidate direction
pub fn offset_frame_within_bounds(frame: IRect, bounds: IRect, offset: i32) -> IRect {
    let candidates = [
        (offset, offset),
        (offset, -offset),
        (-offset, offset),
        (-offset, -offset),
        (offset, 0),
        (-offset, 0),
        (0, offset),
        (0, -offset),
    ];

    for (dx, dy) in candidates {
        let moved = IRect::from_corners(
            IVec2::new(frame.min.x + dx, frame.min.y + dy),
            IVec2::new(frame.max.x + dx, frame.max.y + dy),
        );
        if moved.min.x >= bounds.min.x
            && moved.max.x <= bounds.max.x
            && moved.min.y >= bounds.min.y
            && moved.max.y <= bounds.max.y
        {
            return moved;
        }
    }

    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── clamp_origin_to_bounds ───────────────────────────────────────────

    #[test]
    fn clamp_no_op_when_within_bounds() {
        let origin = IRect::new(100, 100, 400, 300);
        let size = IVec2::new(300, 200);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        assert_eq!(result, IRect::new(100, 100, 400, 300));
    }

    #[test]
    fn clamp_right_overflow() {
        let origin = IRect::new(1800, 100, 2100, 300);
        let size = IVec2::new(300, 200);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        assert_eq!(result.min.x, 1620);
        assert_eq!(result.max.x, 1920);
    }

    #[test]
    fn clamp_bottom_overflow() {
        let origin = IRect::new(100, 950, 400, 1150);
        let size = IVec2::new(300, 200);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        assert_eq!(result.min.y, 880);
        assert_eq!(result.max.y, 1080);
    }

    #[test]
    fn clamp_top_left_underflow() {
        let origin = IRect::new(-50, -50, 250, 150);
        let size = IVec2::new(300, 200);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        assert_eq!(result.min, IVec2::new(0, 0));
    }

    #[test]
    fn clamp_window_larger_than_bounds() {
        let origin = IRect::new(100, 100, 2100, 1200);
        let size = IVec2::new(2000, 1100);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        // max corner of (bounds.max - size) = (1920-2000, 1080-1100) = (-80, -20)
        // clamped to bounds.min = (0, 0)
        assert_eq!(result.min, IVec2::new(0, 0));
    }

    #[test]
    fn clamp_with_offset_bounds() {
        let origin = IRect::new(1900, 50, 2200, 250);
        let size = IVec2::new(300, 200);
        let bounds = IRect::new(1920, 0, 3840, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        assert_eq!(result.min.x, 1920);
    }

    // ── offset_frame_within_bounds ───────────────────────────────────────

    #[test]
    fn offset_first_candidate_fits() {
        let frame = IRect::new(100, 100, 200, 200);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = offset_frame_within_bounds(frame, bounds, 32);
        // First candidate: (offset, offset) = (132, 132, 232, 232) — fits
        assert_eq!(result, IRect::new(132, 132, 232, 232));
    }

    #[test]
    fn offset_near_bottom_right_picks_negative() {
        let frame = IRect::new(1800, 1000, 1900, 1080);
        let bounds = IRect::new(0, 0, 1920, 1080);
        // First candidate: (1832, 1032, 1932, 1112) — exceeds right and bottom
        // Candidate (-32, -32): (1768, 968, 1868, 1048) — fits
        let result = offset_frame_within_bounds(frame, bounds, 32);
        assert_eq!(result, IRect::new(1768, 968, 1868, 1048));
    }

    #[test]
    fn offset_no_fit_returns_original() {
        // Frame nearly fills bounds — no offset candidate fits
        let frame = IRect::new(10, 10, 1910, 1070);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = offset_frame_within_bounds(frame, bounds, 32);
        assert_eq!(result, frame);
    }

    #[test]
    fn offset_frame_at_origin() {
        let frame = IRect::new(0, 0, 100, 100);
        let bounds = IRect::new(0, 0, 1920, 1080);
        // First candidate: (32, 32, 132, 132) — fits
        let result = offset_frame_within_bounds(frame, bounds, 32);
        assert_eq!(result, IRect::new(32, 32, 132, 132));
    }

    #[test]
    fn offset_near_top_right_corner() {
        let frame = IRect::new(1850, 0, 1920, 100);
        let bounds = IRect::new(0, 0, 1920, 1080);
        // (32, 32): 1882+70=1952 > 1920 — fail
        // (32, -32): y=-32 < 0 — fail
        // (-32, 32): (1818, 32, 1888, 132) — fits
        let result = offset_frame_within_bounds(frame, bounds, 32);
        assert_eq!(result, IRect::new(1818, 32, 1888, 132));
    }

    #[test]
    fn offset_zero_is_identity() {
        let frame = IRect::new(100, 100, 200, 200);
        let bounds = IRect::new(0, 0, 1920, 1080);
        // All candidates with offset=0 are the same as original
        let result = offset_frame_within_bounds(frame, bounds, 0);
        assert_eq!(result, frame);
    }

    #[test]
    fn clamp_preserves_size() {
        let origin = IRect::new(2000, 2000, 2300, 2200);
        let size = IVec2::new(300, 200);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        assert_eq!(result.width(), 300);
        assert_eq!(result.height(), 200);
    }

    #[test]
    fn clamp_exact_fit() {
        let origin = IRect::new(0, 0, 1920, 1080);
        let size = IVec2::new(1920, 1080);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = clamp_origin_to_bounds(origin, size, bounds);
        assert_eq!(result.min, IVec2::new(0, 0));
        assert_eq!(result.max, IVec2::new(1920, 1080));
    }

    #[test]
    fn offset_preserves_frame_size() {
        let frame = IRect::new(100, 100, 300, 250);
        let bounds = IRect::new(0, 0, 1920, 1080);
        let result = offset_frame_within_bounds(frame, bounds, 32);
        assert_eq!(result.width(), frame.width());
        assert_eq!(result.height(), frame.height());
    }

    #[test]
    fn offset_with_secondary_display_bounds() {
        let frame = IRect::new(1920, 0, 2020, 100);
        let bounds = IRect::new(1920, 0, 3840, 1080);
        let result = offset_frame_within_bounds(frame, bounds, 32);
        // First candidate: (1952, 32, 2052, 132) — within bounds
        assert_eq!(result, IRect::new(1952, 32, 2052, 132));
    }
}

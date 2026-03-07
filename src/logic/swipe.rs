//! Pure swipe physics — no ECS, no macOS APIs.
//!
//! All swipe velocity, deceleration, and viewport offset computations are
//! isolated here so the gesture pipeline can be unit-tested without Bevy.

/// Computes an exponentially smoothed velocity from a new sample and old value.
///
/// Uses a 30/70 EMA (Exponential Moving Average) split to prevent jittery
/// single-frame spikes from dominating the velocity signal.
///
/// # Arguments
/// - `new_velocity` — raw velocity from the current frame
/// - `old_velocity` — smoothed velocity from the previous frame
pub fn smooth_velocity(new_velocity: f64, old_velocity: f64) -> f64 {
    0.3 * new_velocity + 0.7 * old_velocity
}

/// Applies one frame of exponential decay to a velocity value.
///
/// Models friction/inertia: `velocity * e^(-decay_rate * dt)`.
///
/// # Arguments
/// - `velocity` — current velocity (normalized units/sec)
/// - `decay_rate` — deceleration factor (higher = faster stop, 1.0–10.0)
/// - `dt` — frame delta time in seconds
pub fn decay_velocity(velocity: f64, decay_rate: f64, dt: f64) -> f64 {
    velocity * (-decay_rate * dt).exp()
}

/// Converts a normalized velocity into a pixel shift for one frame.
///
/// # Arguments
/// - `velocity` — current velocity (normalized units/sec)
/// - `dt` — frame delta time in seconds
/// - `display_width` — width of the display in pixels
/// - `direction_modifier` — `1.0` for natural, `-1.0` for reversed
pub fn velocity_to_pixel_shift(
    velocity: f64,
    dt: f64,
    display_width: i32,
    direction_modifier: f64,
) -> i32 {
    let frame_delta = velocity * dt;
    (f64::from(display_width) * frame_delta * direction_modifier) as i32
}

/// Clamps a viewport offset so edge windows stay on-screen.
///
/// When `continuous` is true, no clamping is applied (the strip scrolls
/// infinitely). When false, the offset is clamped so the leftmost window
/// can't scroll past the left edge and the rightmost can't scroll past
/// the right edge.
///
/// # Arguments
/// - `offset` — current viewport offset in pixels
/// - `shift` — pixel shift to apply this frame
/// - `total_strip_width` — total pixel width of all columns
/// - `viewport_width` — visible display width
/// - `pad_left` / `pad_right` — edge padding
/// - `continuous` — if true, skip clamping
pub fn clamp_viewport_offset(
    offset: i32,
    shift: i32,
    total_strip_width: i32,
    viewport_width: i32,
    pad_left: i32,
    pad_right: i32,
    continuous: bool,
) -> i32 {
    let new_offset = offset + shift;
    if continuous {
        return new_offset;
    }
    let effective_width = viewport_width - pad_left - pad_right;
    let left_aligned = -pad_left;
    let right_aligned = total_strip_width - effective_width - pad_left;
    new_offset.clamp(left_aligned, right_aligned.max(left_aligned))
}

/// Returns true when inertia velocity is below the sub-pixel threshold
/// and scrolling should stop.
///
/// # Arguments
/// - `velocity` — current normalized velocity
/// - `display_width` — display width in pixels
/// - `threshold_px` — minimum pixel velocity (e.g. 100.0)
pub fn below_stop_threshold(velocity: f64, display_width: i32, threshold_px: f64) -> bool {
    velocity.abs() * f64::from(display_width) < threshold_px
}

/// Converts a raw swipe delta and sensitivity into a pixel shift.
///
/// # Arguments
/// - `delta` — sum of finger deltas from the gesture event
/// - `sensitivity` — user-configured multiplier (0.1–2.0)
/// - `display_width` — display width in pixels
/// - `direction_modifier` — `1.0` for natural, `-1.0` for reversed
pub fn delta_to_shift(
    delta: f64,
    sensitivity: f64,
    display_width: i32,
    direction_modifier: f64,
) -> i32 {
    let scaled = delta * sensitivity;
    (f64::from(display_width) * scaled * direction_modifier) as i32
}

/// Returns true if a swipe delta is below the display's minimum resolution
/// and should be ignored.
pub fn below_swipe_resolution(delta: f64, display_width: i32) -> bool {
    let resolution = 1.0 / f64::from(display_width);
    delta.abs() < resolution
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── smooth_velocity ────────────────────────────────────────────────

    #[test]
    fn smooth_from_zero_takes_30_percent() {
        let v = smooth_velocity(10.0, 0.0);
        assert!((v - 3.0).abs() < 1e-10);
    }

    #[test]
    fn smooth_stable_signal_converges() {
        let mut v = 0.0;
        for _ in 0..100 {
            v = smooth_velocity(5.0, v);
        }
        assert!((v - 5.0).abs() < 0.01);
    }

    #[test]
    fn smooth_opposite_direction_dampens() {
        let v = smooth_velocity(-10.0, 10.0);
        // 0.3 * -10 + 0.7 * 10 = -3 + 7 = 4
        assert!((v - 4.0).abs() < 1e-10);
    }

    // ── decay_velocity ─────────────────────────────────────────────────

    #[test]
    fn decay_reduces_magnitude() {
        let v = decay_velocity(10.0, 4.0, 1.0 / 60.0);
        assert!(v < 10.0);
        assert!(v > 0.0);
    }

    #[test]
    fn decay_zero_dt_preserves_velocity() {
        let v = decay_velocity(10.0, 4.0, 0.0);
        assert!((v - 10.0).abs() < 1e-10);
    }

    #[test]
    fn decay_high_rate_rapidly_decreases() {
        let v_low = decay_velocity(10.0, 1.0, 1.0 / 60.0);
        let v_high = decay_velocity(10.0, 10.0, 1.0 / 60.0);
        assert!(v_high < v_low);
    }

    #[test]
    fn decay_negative_velocity_approaches_zero() {
        let v = decay_velocity(-10.0, 4.0, 1.0 / 60.0);
        assert!(v > -10.0);
        assert!(v < 0.0);
    }

    // ── velocity_to_pixel_shift ────────────────────────────────────────

    #[test]
    fn shift_proportional_to_display_width() {
        let s1 = velocity_to_pixel_shift(1.0, 1.0 / 60.0, 1920, 1.0);
        let s2 = velocity_to_pixel_shift(1.0, 1.0 / 60.0, 3840, 1.0);
        assert_eq!(s2, s1 * 2);
    }

    #[test]
    fn reversed_direction_negates_shift() {
        let s1 = velocity_to_pixel_shift(1.0, 1.0 / 60.0, 1920, 1.0);
        let s2 = velocity_to_pixel_shift(1.0, 1.0 / 60.0, 1920, -1.0);
        assert_eq!(s1, -s2);
    }

    #[test]
    fn zero_velocity_zero_shift() {
        let s = velocity_to_pixel_shift(0.0, 1.0 / 60.0, 1920, 1.0);
        assert_eq!(s, 0);
    }

    // ── clamp_viewport_offset ──────────────────────────────────────────

    #[test]
    fn continuous_allows_any_offset() {
        let result = clamp_viewport_offset(0, 99999, 1000, 1920, 0, 0, true);
        assert_eq!(result, 99999);
    }

    #[test]
    fn clamped_stays_within_bounds() {
        // strip is 1000px, viewport is 1920px, no padding
        let result = clamp_viewport_offset(0, -500, 1000, 1920, 0, 0, false);
        // left_aligned = 0, right_aligned = 1000-1920 = -920, max(-920, 0) = 0
        assert_eq!(result, 0);
    }

    #[test]
    fn clamped_right_edge() {
        // strip is 3000px, viewport is 1920px, no padding
        let result = clamp_viewport_offset(0, 2000, 3000, 1920, 0, 0, false);
        // right_aligned = 3000 - 1920 = 1080
        assert_eq!(result, 1080);
    }

    #[test]
    fn clamped_with_padding() {
        // viewport=1920, pad_left=10, pad_right=10
        // effective_width = 1920 - 10 - 10 = 1900
        // left_aligned = -10
        // right_aligned = 3000 - 1900 - 10 = 1090
        let result = clamp_viewport_offset(0, 2000, 3000, 1920, 10, 10, false);
        assert_eq!(result, 1090);
    }

    #[test]
    fn clamped_left_with_padding() {
        let result = clamp_viewport_offset(0, -1000, 3000, 1920, 10, 10, false);
        assert_eq!(result, -10);
    }

    // ── below_stop_threshold ───────────────────────────────────────────

    #[test]
    fn above_threshold_returns_false() {
        assert!(!below_stop_threshold(1.0, 1920, 100.0));
    }

    #[test]
    fn below_threshold_returns_true() {
        assert!(below_stop_threshold(0.01, 1920, 100.0));
    }

    #[test]
    fn zero_velocity_is_below() {
        assert!(below_stop_threshold(0.0, 1920, 100.0));
    }

    #[test]
    fn negative_velocity_uses_abs() {
        assert!(!below_stop_threshold(-1.0, 1920, 100.0));
    }

    // ── delta_to_shift ─────────────────────────────────────────────────

    #[test]
    fn sensitivity_scales_shift() {
        let s1 = delta_to_shift(0.1, 0.35, 1920, 1.0);
        let s2 = delta_to_shift(0.1, 0.70, 1920, 1.0);
        assert_eq!(s2, s1 * 2);
    }

    #[test]
    fn reversed_direction_negates_delta_shift() {
        let s1 = delta_to_shift(0.1, 0.35, 1920, 1.0);
        let s2 = delta_to_shift(0.1, 0.35, 1920, -1.0);
        assert_eq!(s1, -s2);
    }

    // ── below_swipe_resolution ─────────────────────────────────────────

    #[test]
    fn tiny_delta_below_resolution() {
        assert!(below_swipe_resolution(0.0001, 1920));
    }

    #[test]
    fn normal_delta_above_resolution() {
        assert!(!below_swipe_resolution(0.01, 1920));
    }

    #[test]
    fn exact_resolution_boundary() {
        let resolution = 1.0 / 1920.0;
        assert!(below_swipe_resolution(resolution - 0.00001, 1920));
        assert!(!below_swipe_resolution(resolution + 0.00001, 1920));
    }

    // ── multi-step integration scenarios ──────────────────────────────

    #[test]
    fn full_inertia_sequence_stops() {
        let mut velocity = 5.0;
        let decay_rate = 4.0;
        let dt = 1.0 / 60.0;
        let display_width = 1920;

        let mut frames = 0;
        while !below_stop_threshold(velocity, display_width, 100.0) {
            velocity = decay_velocity(velocity, decay_rate, dt);
            frames += 1;
            assert!(frames < 1000, "inertia should stop within 1000 frames");
        }
        assert!(frames > 5, "should take more than 5 frames to stop");
    }

    #[test]
    fn ema_smoothing_absorbs_spike() {
        let mut v = 1.0;
        // Sudden spike
        v = smooth_velocity(100.0, v);
        // Should be damped: 0.3 * 100 + 0.7 * 1 = 30.7
        assert!((v - 30.7).abs() < 1e-10);
        // Recovery toward baseline
        v = smooth_velocity(1.0, v);
        // 0.3 * 1 + 0.7 * 30.7 = 0.3 + 21.49 = 21.79
        assert!((v - 21.79).abs() < 1e-10);
    }

    #[test]
    fn clamped_offset_monotonic_with_increasing_shift() {
        let total_strip = 5000;
        let viewport = 1920;
        let mut prev = 0;
        for shift in [100, 200, 300, 400, 500] {
            let result = clamp_viewport_offset(0, shift, total_strip, viewport, 0, 0, false);
            assert!(result >= prev, "offset should increase monotonically");
            prev = result;
        }
    }

    #[test]
    fn continuous_vs_clamped_diverge_at_edge() {
        let offset = 2000;
        let shift = 500;
        let strip = 3000;
        let viewport = 1920;

        let continuous = clamp_viewport_offset(offset, shift, strip, viewport, 0, 0, true);
        let clamped = clamp_viewport_offset(offset, shift, strip, viewport, 0, 0, false);

        assert_eq!(continuous, 2500);
        // clamped: right_aligned = 3000 - 1920 = 1080
        assert_eq!(clamped, 1080);
    }

    #[test]
    fn delta_to_shift_zero_sensitivity() {
        let shift = delta_to_shift(0.5, 0.0, 1920, 1.0);
        assert_eq!(shift, 0);
    }

    #[test]
    fn below_swipe_resolution_narrow_display() {
        // On a 100px display, resolution = 0.01
        assert!(below_swipe_resolution(0.005, 100));
        assert!(!below_swipe_resolution(0.02, 100));
    }
}

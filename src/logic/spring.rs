//! Critically-damped spring animation — no ECS, no macOS APIs.
//!
//! Implements the closed-form solution for a damped harmonic oscillator.
//! A critically-damped spring (damping_ratio = 1.0) is the fastest to reach
//! the target without overshooting. Underdamped springs (< 1.0) oscillate;
//! overdamped springs (> 1.0) converge slowly.
//!
//! Reference: <https://www.ryanjuckett.com/damped-springs/>

/// State for a single axis of a spring animation.
#[derive(Clone, Copy, Debug, Default)]
pub struct SpringAxis {
    pub velocity: f64,
}

/// Parameters controlling a spring's behavior.
#[derive(Clone, Copy, Debug)]
pub struct SpringParams {
    /// How stiff the spring is. Higher = faster/snappier.
    /// Typical: 800 (window move), 1000 (workspace switch).
    pub stiffness: f64,
    /// 1.0 = critically damped (fastest without overshoot).
    /// < 1.0 = underdamped (bouncy). > 1.0 = overdamped (mushy).
    pub damping_ratio: f64,
    /// Stop threshold in pixels. Animation snaps to target when
    /// |displacement| and |velocity| are both below this.
    pub epsilon: f64,
}

impl Default for SpringParams {
    fn default() -> Self {
        Self {
            stiffness: 800.0,
            damping_ratio: 1.0,
            epsilon: 0.5,
        }
    }
}

/// Advance a spring one time step. Returns `(new_position, settled)`.
///
/// `current` is the current value, `target` is the desired value,
/// `spring` holds the per-axis velocity, `params` controls the spring,
/// and `dt` is the frame delta in seconds.
///
/// When `settled` is true, the returned position equals `target` and
/// velocity has been zeroed.
pub fn step(
    current: f64,
    target: f64,
    spring: &mut SpringAxis,
    params: &SpringParams,
    dt: f64,
) -> (f64, bool) {
    if dt <= 0.0 {
        return (current, false);
    }

    let x0 = current - target; // displacement
    let v0 = spring.velocity;
    let omega = params.stiffness.sqrt(); // angular frequency

    let (new_x, new_v) = if (params.damping_ratio - 1.0).abs() < 1e-6 {
        // ── Critically damped ──────────────────────────────────────
        // x(t) = (C1 + C2·t) · e^(-ω·t)
        // where C1 = x0, C2 = v0 + ω·x0
        let exp = (-omega * dt).exp();
        let c2 = v0 + omega * x0;
        let new_x = (x0 + c2 * dt) * exp;
        let new_v = (v0 - omega * c2 * dt) * exp;
        (new_x, new_v)
    } else if params.damping_ratio < 1.0 {
        // ── Underdamped ────────────────────────────────────────────
        // x(t) = e^(-ζωt) · (x0·cos(ωd·t) + ((v0 + ζω·x0)/ωd)·sin(ωd·t))
        // v(t) = e^(-ζωt) · ((v0 - ζω·x0)·cos(ωd·t) - (ωd·x0 + ζω·v0/ωd)·sin(ωd·t))
        let zeta = params.damping_ratio;
        let omega_d = omega * (1.0 - zeta * zeta).sqrt();
        let exp = (-zeta * omega * dt).exp();
        let cos_wd = (omega_d * dt).cos();
        let sin_wd = (omega_d * dt).sin();
        let new_x = exp * (x0 * cos_wd + ((v0 + zeta * omega * x0) / omega_d) * sin_wd);
        let new_v = exp
            * ((-zeta * omega * x0 + v0) * cos_wd
                - (omega_d * x0 + zeta * omega * v0 / omega_d) * sin_wd);
        (new_x, new_v)
    } else {
        // ── Overdamped ─────────────────────────────────────────────
        let zeta = params.damping_ratio;
        let s1 = omega * (-zeta + (zeta * zeta - 1.0).sqrt());
        let s2 = omega * (-zeta - (zeta * zeta - 1.0).sqrt());
        let c1 = (v0 - s2 * x0) / (s1 - s2);
        let c2 = x0 - c1;
        let new_x = c1 * (s1 * dt).exp() + c2 * (s2 * dt).exp();
        let new_v = c1 * s1 * (s1 * dt).exp() + c2 * s2 * (s2 * dt).exp();
        (new_x, new_v)
    };

    // Settle check: both displacement and velocity are negligible.
    if new_x.abs() < params.epsilon && new_v.abs() < params.epsilon {
        spring.velocity = 0.0;
        (target, true)
    } else {
        spring.velocity = new_v;
        (target + new_x, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_params() -> SpringParams {
        SpringParams::default()
    }

    #[test]
    fn critically_damped_converges_without_overshoot() {
        let params = default_params();
        let mut spring = SpringAxis::default();
        let target = 500.0;
        let mut pos = 0.0;
        let dt = 1.0 / 60.0;
        let mut max_pos = 0.0_f64;

        for _ in 0..300 {
            let (new_pos, settled) = step(pos, target, &mut spring, &params, dt);
            max_pos = max_pos.max(new_pos);
            pos = new_pos;
            if settled {
                break;
            }
        }

        assert!(
            (pos - target).abs() < params.epsilon,
            "should converge to target: pos={pos}, target={target}"
        );
        assert!(
            max_pos <= target + params.epsilon,
            "critically damped should not overshoot: max={max_pos}, target={target}"
        );
    }

    #[test]
    fn settles_within_reasonable_frames() {
        let params = default_params(); // stiffness=800
        let mut spring = SpringAxis::default();
        let mut pos = 0.0;
        let dt = 1.0 / 60.0;
        let mut frames = 0;

        loop {
            let (new_pos, settled) = step(pos, 1000.0, &mut spring, &params, dt);
            pos = new_pos;
            frames += 1;
            if settled {
                break;
            }
            assert!(frames < 600, "should settle within 10 seconds");
        }

        // stiffness=800, ω≈28.3, time constant ≈ 0.035s
        // Should settle in ~12 frames (200ms) at 60fps
        assert!(
            frames < 30,
            "should settle quickly with stiffness=800: took {frames} frames"
        );
    }

    #[test]
    fn zero_displacement_is_already_settled() {
        let params = default_params();
        let mut spring = SpringAxis::default();
        let (pos, settled) = step(100.0, 100.0, &mut spring, &params, 1.0 / 60.0);
        assert!(settled);
        assert!((pos - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn velocity_preserved_across_retarget() {
        let params = default_params();
        let mut spring = SpringAxis::default();
        let dt = 1.0 / 60.0;
        let mut pos = 0.0;

        // Move toward 500 for a few frames to build up velocity.
        for _ in 0..5 {
            let (new_pos, _) = step(pos, 500.0, &mut spring, &params, dt);
            pos = new_pos;
        }
        let velocity_before = spring.velocity;
        assert!(velocity_before > 0.0, "should have positive velocity");

        // Retarget to 200 — velocity should carry over into next step.
        let (new_pos, _) = step(pos, 200.0, &mut spring, &params, dt);
        // With positive velocity and new target behind current direction,
        // position should still increase slightly (momentum).
        assert!(
            new_pos >= pos - 1.0,
            "momentum should prevent instant reversal: was {pos}, now {new_pos}"
        );
    }

    #[test]
    fn underdamped_overshoots() {
        let params = SpringParams {
            stiffness: 800.0,
            damping_ratio: 0.5,
            epsilon: 0.5,
            ..default_params()
        };
        let mut spring = SpringAxis::default();
        let target = 500.0;
        let mut pos = 0.0;
        let dt = 1.0 / 60.0;
        let mut max_pos = 0.0_f64;

        for _ in 0..600 {
            let (new_pos, settled) = step(pos, target, &mut spring, &params, dt);
            max_pos = max_pos.max(new_pos);
            pos = new_pos;
            if settled {
                break;
            }
        }

        assert!(
            max_pos > target + 1.0,
            "underdamped spring should overshoot: max={max_pos}, target={target}"
        );
        assert!(
            (pos - target).abs() < params.epsilon,
            "should still converge: pos={pos}"
        );
    }

    #[test]
    fn zero_dt_preserves_state() {
        let params = default_params();
        let mut spring = SpringAxis { velocity: 42.0 };
        let (pos, settled) = step(100.0, 500.0, &mut spring, &params, 0.0);
        assert!(!settled);
        assert!((pos - 100.0).abs() < f64::EPSILON);
        assert!((spring.velocity - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn high_stiffness_converges_in_one_frame() {
        let params = SpringParams {
            stiffness: 1_000_000.0,
            damping_ratio: 1.0,
            epsilon: 0.5,
            ..default_params()
        };
        let mut spring = SpringAxis::default();
        let (pos, settled) = step(0.0, 500.0, &mut spring, &params, 1.0 / 60.0);
        assert!(settled, "very high stiffness should settle in one frame");
        assert!((pos - 500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn negative_direction_works() {
        let params = default_params();
        let mut spring = SpringAxis::default();
        let mut pos = 500.0;
        let dt = 1.0 / 60.0;

        for _ in 0..300 {
            let (new_pos, settled) = step(pos, 0.0, &mut spring, &params, dt);
            pos = new_pos;
            if settled {
                break;
            }
        }

        assert!(
            (pos - 0.0).abs() < params.epsilon,
            "should converge to 0: pos={pos}"
        );
    }
}

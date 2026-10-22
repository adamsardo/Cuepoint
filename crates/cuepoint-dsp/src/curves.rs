//! Gain laws for faders and the crossfader.
//!
//! rekordbox's exact curves are an open question (spec 03, section 6); these
//! are conventional DJ-mixer laws until measured captures replace them.

use std::f32::consts::FRAC_PI_2;

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

/// Channel fader position 0..=1 to linear gain. A squared law gives a
/// roughly logarithmic feel: half travel is about -12 dB.
pub fn fader_gain(position: f32) -> f32 {
    let p = position.clamp(0.0, 1.0);
    p * p
}

/// Crossfader position -1 (full left) ..= 1 (full right) to the
/// (left, right) gains, keeping total power constant across the travel.
pub fn constant_power_crossfade(position: f32) -> (f32, f32) {
    let t = (position.clamp(-1.0, 1.0) + 1.0) * 0.5;
    let angle = t * FRAC_PI_2;
    (angle.cos(), angle.sin())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfade_ends_and_centre() {
        let (l, r) = constant_power_crossfade(-1.0);
        assert!((l - 1.0).abs() < 1e-6 && r.abs() < 1e-6);
        let (l, r) = constant_power_crossfade(1.0);
        assert!(l.abs() < 1e-6 && (r - 1.0).abs() < 1e-6);
        let (l, r) = constant_power_crossfade(0.0);
        assert!((l * l + r * r - 1.0).abs() < 1e-5);
    }

    #[test]
    fn fader_law() {
        assert_eq!(fader_gain(0.0), 0.0);
        assert_eq!(fader_gain(1.0), 1.0);
        assert!((20.0 * fader_gain(0.5).log10() + 12.04).abs() < 0.01);
    }
}

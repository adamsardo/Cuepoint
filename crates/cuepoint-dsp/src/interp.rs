/// 4-point, 3rd-order Hermite interpolation between `y1` and `y2` at
/// fraction `t` in 0..1, with neighbours `y0` and `y3`.
#[inline]
pub fn hermite(y0: f32, y1: f32, y2: f32, y3: f32, t: f32) -> f32 {
    let c1 = 0.5 * (y2 - y0);
    let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
    ((c3 * t + c2) * t + c1) * t + y1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passes_through_samples_and_lines() {
        assert_eq!(hermite(0.0, 1.0, 2.0, 3.0, 0.0), 1.0);
        assert_eq!(hermite(0.0, 1.0, 2.0, 3.0, 1.0), 2.0);
        assert!((hermite(0.0, 1.0, 2.0, 3.0, 0.25) - 1.25).abs() < 1e-6);
    }
}

/// Linear ramp from the current value to a target over a fixed number of
/// samples, so parameter changes never step.
#[derive(Debug, Clone, Copy)]
pub struct Ramp {
    current: f32,
    target: f32,
    step: f32,
    remaining: u32,
    length: u32,
}

impl Ramp {
    pub fn new(value: f32, length: u32) -> Self {
        Self {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
            length: length.max(1),
        }
    }

    pub fn set_target(&mut self, target: f32) {
        if target == self.target {
            return;
        }
        self.target = target;
        self.remaining = self.length;
        self.step = (target - self.current) / self.length as f32;
    }

    pub fn target(&self) -> f32 {
        self.target
    }

    pub fn is_settled(&self) -> bool {
        self.remaining == 0
    }

    #[inline]
    pub fn next_value(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.current = if self.remaining == 0 {
                self.target
            } else {
                self.current + self.step
            };
        }
        self.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaches_target_exactly() {
        let mut ramp = Ramp::new(0.0, 4);
        ramp.set_target(1.0);
        let values: Vec<f32> = (0..6).map(|_| ramp.next_value()).collect();
        assert_eq!(values, [0.25, 0.5, 0.75, 1.0, 1.0, 1.0]);
        assert!(ramp.is_settled());
    }
}

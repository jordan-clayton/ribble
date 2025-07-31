use std::f32::consts::PI;

// Basic DC Block filter (discrete-time IIR filter).
pub struct DCBlock {
    prev_input: f32,
    prev_output: f32,
    r: f32,
    cutoff_frequency: f32,
    sample_rate: f32,
}

impl DCBlock {
    // This is in hertz
    const DEFAULT_CUTOFF_FREQUENCY: f32 = 20f32;
    // This is a cheap approximation in case the sample rate isn't provided
    const DEFAULT_R_CONSTANT: f32 = 0.995;

    pub fn new() -> Self {
        Self {
            prev_input: 0f32,
            prev_output: 0f32,
            r: Self::DEFAULT_R_CONSTANT,
            cutoff_frequency: Self::DEFAULT_CUTOFF_FREQUENCY,
            sample_rate: 1f32,
        }
    }
    pub fn with_cutoff_frequency(mut self, cutoff_frequency: f32) -> Self {
        self.cutoff_frequency = cutoff_frequency;
        self.compute_r()
    }

    pub fn with_sample_rate(mut self, sample_rate: f32) -> Self {
        self.sample_rate = sample_rate;
        self.compute_r()
    }

    fn compute_r(mut self) -> Self {
        let nyquist = self.sample_rate / 2.0f32;

        self.r = if self.cutoff_frequency > nyquist {
            let exponent = -2f32 * PI * self.cutoff_frequency / self.sample_rate;
            exponent.exp()
        } else {
            // This is a cheaper approximation that works well for when below the nyquist
            // frequency.
            1f32 - (2f32 * PI * self.cutoff_frequency / self.sample_rate)
        };

        // In case there's a 0div or a nan that infects the calculation, just set back to the
        // default R constant.
        if self.r.is_nan() || self.r.is_infinite() {
            self.r = Self::DEFAULT_R_CONSTANT;
        }

        self
    }

    // DC Block filter recursion
    // y(n) = x(n) - x(n - 1) + R * y(n - 1)
    pub fn process(&mut self, input: f32) -> f32 {
        let y = input - self.prev_input + self.r * self.prev_output;
        self.prev_input = input;
        self.prev_output = y;
        y
    }
}

impl Default for DCBlock {
    fn default() -> Self {
        Self::new()
    }
}

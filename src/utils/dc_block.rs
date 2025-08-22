use std::borrow::Borrow;
use std::f32::consts::PI;

// Basic DC Block filter (discrete-time IIR filter).
#[derive(Copy, Clone)]
pub(crate) struct DCBlock {
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

    pub(crate) fn new() -> Self {
        Self {
            prev_input: 0f32,
            prev_output: 0f32,
            r: Self::DEFAULT_R_CONSTANT,
            cutoff_frequency: Self::DEFAULT_CUTOFF_FREQUENCY,
            sample_rate: 1f32,
        }
    }
    pub(crate) fn with_cutoff_frequency(mut self, cutoff_frequency: f32) -> Self {
        self.cutoff_frequency = cutoff_frequency;
        self.compute_r()
    }

    pub(crate) fn with_sample_rate(mut self, sample_rate: f32) -> Self {
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
        if !self.r.is_finite() {
            self.r = Self::DEFAULT_R_CONSTANT;
        }

        self
    }

    // DC Block filter recursion
    // y(n) = x(n) - x(n - 1) + R * y(n - 1)
    fn process(&mut self, input: f32) -> f32 {
        let y = input - self.prev_input + self.r * self.prev_output;
        self.prev_input = input;
        self.prev_output = y;
        y
    }

    pub(crate) fn process_signal<'a, I>(&mut self, signal: I)
    where
        I: Iterator<Item = &'a mut f32>,
    {
        // TODO: this might be better/more clearly served by a "reset" function
        let old_state = *self;
        signal.for_each(|f| *f = self.process(*f));
        *self = old_state;
    }

    // Consumes & takes an iterator and returns an iterator that applies the block.
    pub(crate) fn process_signal_map<I>(self, signal: I) -> DCBlockMap<I>
    where
        I: Iterator,
    {
        DCBlockMap {
            dc_block: self,
            map_iter: signal,
        }
    }
}

impl Default for DCBlock {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) struct DCBlockMap<I> {
    dc_block: DCBlock,
    map_iter: I,
}

impl<I> Iterator for DCBlockMap<I>
where
    I: Iterator,
    I::Item: Borrow<f32>,
{
    type Item = f32;

    fn next(&mut self) -> Option<Self::Item> {
        self.map_iter
            .next()
            .and_then(|f| Some(self.dc_block.process(*(f.borrow()))))
    }
}

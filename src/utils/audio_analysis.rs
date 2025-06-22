use biquad::{Biquad, Coefficients, DirectForm2Transposed, ToHertz, Type, Q_BUTTERWORTH_F32};
use realfft::{num_complex::ComplexFloat, RealToComplex};


// F32 sample conversions are handled by traits, see pcm_f32.
// TODO: finish pruning/refactoring code -> These functions can probably live elsewhere.

// From: http://www.learningaboutelectronics.com/Articles/Center-frequency-calculator.php
pub fn f_central(f_lower: f32, f_higher: f32) -> f32 {
    if f_higher / f_lower >= 1.1 {
        (f_lower * f_higher).sqrt()
    } else {
        (f_lower + f_higher) / 2.0
    }
}

pub fn bandpass_filter(samples: &mut [f32], sample_rate: f32, f_central: f32) {
    let fs = sample_rate.hz();
    let f0 = f_central.hz();
    // TODO: This should not expect/unwrap -> return an error instead.
    let coeffs = Coefficients::<f32>::from_params(Type::BandPass, fs, f0, Q_BUTTERWORTH_F32)
        .expect("Cutoff does not adhere to Nyquist Freq");

    let mut biquad = DirectForm2Transposed::<f32>::new(coeffs);

    let len = samples.len();
    for i in 0..len {
        samples[i] = biquad.run(samples[i]);
    }
}
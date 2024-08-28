use std::{
    f32::consts::PI,
    sync::{Arc, Mutex},
};

use biquad::{Biquad, Coefficients, DirectForm2Transposed, Q_BUTTERWORTH_F32, ToHertz, Type};
use lazy_static::lazy_static;
use realfft::{
    num_complex::Complex32,
    num_traits::{FromPrimitive, NumCast, ToPrimitive, Zero},
    RealFftPlanner, RealToComplex,
};
use realfft::num_traits::{Bounded, Signed};

use crate::utils::constants;
use crate::utils::errors::{WhisperAppError, WhisperAppErrorType};

lazy_static! {
    static ref FFT_PLANNER: Mutex<RealFftPlanner<f32>> = Mutex::new(RealFftPlanner::<f32>::new());
}


fn apply_gain(samples: &mut [f32], gain: f32) {
    for sample in samples.iter_mut() {
        *sample *= gain;
    }
}

pub fn to_f32_normalized<T: NumCast + Bounded + FromPrimitive + ToPrimitive + Zero>(
    source: &[T],
    dest: &mut [f32],
) {
    let cast = source
        .iter()
        .map(|n| {
            let num = n.to_f32().expect("Failed to cast T to f32");
            let denom = T::max_value()
                .to_f32()
                .expect("Failed to cast T::MAX to f32");
            num / denom
        })
        .collect::<Vec<_>>();
    let cast = cast.as_slice();
    dest.copy_from_slice(cast);
}

pub fn from_f32_normalized<T: Copy + NumCast + Bounded + FromPrimitive + ToPrimitive + Zero>(
    source: &[f32],
    dest: &mut [T],
) {
    let cast = source
        .iter()
        .map(|n| {
            let max = T::max_value().to_f32().expect("Failed to cast T to f32");
            let num = *n * max;
            T::from(num).expect("Failed to cast f32 to T")
        })
        .collect::<Vec<_>>();
    let cast = cast.as_slice();
    dest.copy_from_slice(cast);
}

pub fn cast_to_f32<T: NumCast + FromPrimitive + ToPrimitive + Zero>(
    source: &[T],
    dest: &mut [f32],
) {
    let cast = source
        .iter()
        .map(|n| n.to_f32().expect("Failed to cast T to f32"))
        .collect::<Vec<_>>();
    let cast = cast.as_slice();
    dest.copy_from_slice(cast);
}

pub fn cast_from_f32<T: NumCast + FromPrimitive + ToPrimitive + Zero + Copy>(
    source: &[f32],
    dest: &mut [T],
) {
    let cast: Vec<T> = source
        .iter()
        .map(|n| T::from(*n).expect("Failed to cast f32 to T"))
        .collect();
    let cast = cast.as_slice();
    dest.copy_from_slice(cast);
}

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
    let coeffs = Coefficients::<f32>::from_params(Type::BandPass, fs, f0, Q_BUTTERWORTH_F32)
        .expect("Cutoff does not adhere to Nyquist Freq");

    let mut biquad = DirectForm2Transposed::<f32>::new(coeffs);

    let len = samples.len();
    for i in 0..len {
        samples[i] = biquad.run(samples[i]);
    }
}

fn hann_window(samples: &[f32], window_out: &mut [f32]) {
    assert_eq!(samples.len(), window_out.len(), "Window sizes do not match");
    assert!(samples.len() > 0, "Invalid size");
    let len = samples.len() as f32;
    for (i, sample) in samples.iter().enumerate() {
        let t = (i as f32) / len;
        let hann = 0.5 * (1.0 - (2.0 * PI * t).cos());
        window_out[i] = hann * sample
    }
}

pub fn fft_analysis(samples: &[f32], result: &mut [f32; constants::NUM_BUCKETS]) {
    // Apply the window.
    let len = samples.len();
    let mut window_samples = vec![0.0f32; len];
    hann_window(samples, &mut window_samples);

    // Build frames
    let mut frames = welch_frames(&window_samples, Some(constants::NUM_BUCKETS), None)
        .expect("Failed to build frames");

    // Init FFT
    let mut planner = FFT_PLANNER.lock().expect("Failed to get FFT Planner mutex");
    let fft = planner.plan_fft_forward(frames[0].len());
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    // Take the average signal density per mini fft, on log scale.
    // (Also, grab max log magnitude).
    let mut max_mag = 1.0;
    let log_average_signal_density: Vec<f32> = frames
        .iter_mut()
        .map(|frame| {
            apply_gain(frame, constants::FFT_GAIN);
            process_fft(frame, fft.clone(), &mut input, &mut output);
            let mag = output.iter().map(|c| c.norm_sqr().abs()).sum::<f32>()
                / (output.len() as f32).powi(2);

            let log_mag = mag.log10();

            if log_mag > max_mag {
                max_mag = mag;
            }
            log_mag.max(0.0)
        })
        .collect();

    // Normalize between 0 and 1.
    let normalized_signal: Vec<f32> = log_average_signal_density
        .iter()
        .map(|n| *n / max_mag)
        .collect();

    assert_eq!(
        normalized_signal.len(),
        constants::NUM_BUCKETS,
        "Grouping failed. Expected: {}, Actual: {}",
        constants::NUM_BUCKETS,
        normalized_signal.len()
    );

    debug_assert!(normalized_signal.iter().all(|n| *n <= 1.0 && *n >= 0.0), "Normalizing failed");

    // Copy the normalized signal to the result slice.
    result.copy_from_slice(&normalized_signal);
}

#[inline]
fn process_fft(
    samples: &[f32],
    fft: Arc<dyn RealToComplex<f32>>,
    input: &mut [f32],
    spectrum: &mut [Complex32],
) {
    input.copy_from_slice(samples);

    fft.process(input, spectrum)
        .expect("Slice with incorrect length supplied to fft");
}

// TODO: Fix this implementation -> this needs to frame into constants::NUM_BUCKETS
fn welch_frames(
    windowed_samples: &[f32],
    num_segments: Option<usize>,
    overlap: Option<f32>,
) -> Result<Vec<Vec<f32>>, WhisperAppError> {
    let mut len = windowed_samples.len();
    let k = num_segments.unwrap_or(4);
    let a = overlap.unwrap_or(0.5);

    if len < 4 {
        return Err(WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Invalid sample size {}, minimum is 4", len),
        ));
    }

    let mut samples = vec![0.0; len];
    samples.copy_from_slice(windowed_samples);

    // If len < k, zero pad up to at least k.
    if len < k {
        samples.resize(k, 0.0);
        len = k;
    }

    // l - length - must land on a power of two.
    let mut l = (len as f64 / (k as f64 * (1.0 - a as f64) + a as f64)).trunc() as usize;
    let m = l.next_power_of_two();

    // If l does not land on a power of two, zero-pad the end of the samples.
    if m > l {
        let diff = m.abs_diff(l);
        let new_size = samples.len() + diff;
        samples.resize(new_size, 0.0);
        l = m;
    }

    let overlap = l - (l as f64 * a as f64).round() as usize;

    let mut frames: Vec<Vec<f32>> = samples
        .windows(l)
        .step_by(overlap)
        .map(|s| s.to_vec())
        .collect();


    assert!(frames.len() > 0, "Failed to group into frames");
    assert!(
        frames[0].len().is_power_of_two(),
        "Failed to group in powers of 2"
    );

    // TODO: fix this - this is not a fix.
    if frames.len() != constants::NUM_BUCKETS {
        let diff = frames.len().abs_diff(constants::NUM_BUCKETS);
        let new_size = frames.len() + diff;
        let inner = frames[0].len();
        frames.resize(new_size, vec![0.0; inner]);
    }

    Ok(frames)
}

pub fn smoothing(current: &mut [f32], target: &[f32], dt: f32) {
    assert_eq!(
        current.len(),
        target.len(),
        "Incorrect sizes. Current: {}, Target: {}",
        current.len(),
        target.len()
    );
    debug_assert!(current.iter().all(|n| !n.is_nan()), "Nan values in current before smoothing");
    debug_assert!(current.iter().all(|n| !n.is_infinite()), "Infinite values in current before smoothing");
    debug_assert!(target.iter().all(|n| !n.is_nan()), "Nan values in target before smoothing");
    debug_assert!(target.iter().all(|n| !n.is_infinite()), "Infinite values in target before smoothing");

    for i in 0..current.len() {
        let x = current[i];
        let y = target[i];
        let z = x - y;
        let t = constants::SMOOTH_FACTOR * dt;
        current[i] += (target[i] - current[i]) * constants::SMOOTH_FACTOR * dt;
        if current[i].is_infinite() {
            panic!("Infinite value.\n\
            Case:\n\
            x: {},\
            y: {},\
            x - y: {},\
            t: {}
            ", x, y, z, t);
        }
    }
    debug_assert!(current.iter().all(|n| !n.is_nan()), "Nan values after smoothing");

    debug_assert!(current.iter().all(|n| !n.is_infinite()), "Infinite values after smoothing");
}

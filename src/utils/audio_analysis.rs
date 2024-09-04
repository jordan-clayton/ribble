use std::{
    f32::consts::PI,
    sync::{Arc, Mutex},
};

use atomic_enum::atomic_enum;
use biquad::{Biquad, Coefficients, DirectForm2Transposed, Q_BUTTERWORTH_F32, ToHertz, Type};
use lazy_static::lazy_static;
use realfft::{
    num_complex::Complex32,
    num_traits::{FromPrimitive, NumCast, ToPrimitive, Zero},
    RealFftPlanner, RealToComplex,
};
use realfft::num_complex::ComplexFloat;
use realfft::num_traits::Bounded;
use strum::{Display, EnumIter};

use crate::utils::constants;
use crate::utils::errors::{WhisperAppError, WhisperAppErrorType};

lazy_static! {
    static ref FFT_PLANNER: Mutex<RealFftPlanner<f32>> = Mutex::new(RealFftPlanner::<f32>::new());
}

#[atomic_enum]
#[derive(PartialEq, EnumIter, Display)]
pub enum AnalysisType {
    Waveform = 0,
    Power,
    #[strum(to_string = "Spectrum Density")]
    SpectrumDensity,
}

impl AnalysisType {
    pub fn rotate_clockwise(&self) -> Self {
        match self {
            AnalysisType::Waveform => {
                AnalysisType::Power
            }
            AnalysisType::Power => {
                AnalysisType::SpectrumDensity
            }
            AnalysisType::SpectrumDensity => {
                AnalysisType::Waveform
            }
        }
    }

    pub fn rotate_counterclockwise(&self) -> Self {
        match self {
            AnalysisType::Waveform => {
                AnalysisType::SpectrumDensity
            }
            AnalysisType::Power => {
                AnalysisType::Waveform
            }
            AnalysisType::SpectrumDensity => {
                AnalysisType::Power
            }
        }
    }
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

pub fn normalized_waveform(samples: &[f32], result: &mut [f32; constants::NUM_BUCKETS]) {
    let len = samples.len();

    assert!(
        len >= constants::NUM_BUCKETS,
        "Insufficient samples length: {}",
        len
    );

    let mut audio_samples = samples.to_vec();
    let chunk_size = (len / constants::NUM_BUCKETS).max(1);
    let mut max_amp = 1.0;
    let mut wave_form: Vec<f32> = audio_samples
        .chunks_mut(chunk_size)
        .map(|c| {
            apply_gain(c, constants::WAVEFORM_GAIN);
            let avg_amp = (c.iter().sum::<f32>() / c.len() as f32).abs();
            if avg_amp > max_amp {
                max_amp = avg_amp;
            }
            avg_amp
        })
        .collect();

    for avg in wave_form.iter_mut() {
        *avg = *avg / max_amp;
    }

    // This either truncates or zero-pads input that doesn't fit neatly into the bucket
    // size.
    if wave_form.len() != constants::NUM_BUCKETS {
        wave_form.resize(constants::NUM_BUCKETS, 0.0);
    };

    debug_assert!(
        wave_form.iter().all(|n| *n >= 0.0 && *n <= 1.0),
        "Failed to normalize {:?}", &wave_form
    );

    result.copy_from_slice(&wave_form);
}

pub fn power_analysis(samples: &[f32], result: &mut [f32; constants::NUM_BUCKETS]) {
    // Apply the window.
    let len = samples.len();
    let mut window_samples = vec![0.0f32; len];
    hann_window(samples, &mut window_samples);

    let mut frames = fixed_frames(
        &window_samples,
        Some(constants::NUM_BUCKETS),
        None,
        None,
        None,
    )
        .expect("Failed to build frames");

    // Init FFT
    let mut planner = FFT_PLANNER.lock().expect("Failed to get FFT Planner mutex");
    let fft = planner.plan_fft_forward(frames[0].len());
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();

    let mut max_amp = 1.0;
    let mut power_samples: Vec<f32> = frames
        .iter_mut()
        .map(|frame| {
            apply_gain(frame, constants::FFT_GAIN);
            process_fft(frame, fft.clone(), &mut input, &mut output);

            let power =
                output.iter().map(|c| c.norm_sqr()).sum::<f32>() / (frame.len() as f32).powi(2);
            let log_power = if power > 0.0 { power.log10() } else { 0.0 };

            // This is just to avoid having to re-find the maximum.
            if log_power > max_amp {
                max_amp = log_power;
            }

            log_power
        })
        .collect();

    for amp in power_samples.iter_mut() {
        *amp = (*amp / max_amp).max(0.0);
    }

    debug_assert!(
        power_samples.iter().all(|n| *n >= 0.0 && *n <= 1.0),
        "Failed to normalize"
    );

    result.copy_from_slice(&power_samples);
}

pub fn frequency_analysis(
    samples: &[f32],
    result: &mut [f32; constants::NUM_BUCKETS],
    sample_rate: f64,
) {
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

    let mut max_amp = 1.0;

    // TODO: Determine whether gain is necessary here. Seems more accurate without.
    for frame in frames.iter_mut() {
        process_fft(frame, fft.clone(), &mut input, &mut output);

        let n = output.len();
        let min_freq = sample_rate / (n as f64);
        let max_freq = sample_rate / 2.0;

        let log_min = min_freq.log10();
        let log_max = max_freq.log10();
        let log_range = log_max - log_min;


        // Compute edges
        let bucket_edges: Vec<f64> = (0..constants::NUM_BUCKETS + 1)
            .map(|n| 10.0.powf(log_min + log_range * (n as f64) / (constants::NUM_BUCKETS as f64)))
            .collect();

        for (i, &value) in output.iter().enumerate() {
            let freq = (i as f64) * sample_rate / (n as f64);
            if freq < min_freq || freq > max_freq {
                continue;
            }

            // The value will be normalized, so this can be norm_sqr()
            for j in 0..constants::NUM_BUCKETS {
                if freq >= bucket_edges[j] && freq < bucket_edges[j + 1] {
                    result[j] += value.norm_sqr();
                    if result[j] > max_amp {
                        max_amp = result[j];
                    }
                }
            }
        }
    }


    for res in result.iter_mut() {
        *res = *res / max_amp;
    }

    debug_assert!(
        result.iter().all(|n| *n <= 1.0 && *n >= 0.0),
        "Normalizing failed"
    );
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

fn fixed_frames(
    windowed_samples: &[f32],
    num_segments: Option<usize>,
    overlap: Option<f32>,
    tolerance: Option<usize>,
    max_iterations: Option<usize>,
) -> Result<Vec<Vec<f32>>, WhisperAppError> {
    let max_iterations = max_iterations.unwrap_or(constants::FRAME_CONVERGENCE_ITERATIONS);
    let tolerance = tolerance.unwrap_or(constants::FRAME_CONVERGENCE_TOLERANCE);
    let mut len = windowed_samples.len();
    if len < 1 {
        return Err(WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            String::from("Zero sample size provided."),
        ));
    }
    let mut samples = vec![0.0; len];

    samples.copy_from_slice(windowed_samples);

    let k = num_segments.unwrap_or(4);

    if k < 1 {
        return Err(WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            String::from("Cannot return 0 segments."),
        ));
    }

    if len < k {
        samples.resize(k, 0.0);
        len = k;
    }

    let mut a = overlap.unwrap_or(0.5);

    let mut l = (len as f64 / (k as f64 * (1.0 - a as f64) + a as f64)).trunc() as usize;

    let mut iteration = 0;

    loop {
        let overlap = (l - (l as f64 * a as f64).round() as usize).max(1);
        let n = ((len - l) / overlap) + 1;

        if n.abs_diff(k) <= tolerance {
            break;
        }

        if iteration >= max_iterations {
            break;
        }

        if n > k {
            a = (a + 0.1).max(1.0);
        } else {
            a -= (a - 0.01).min(0.0);
        }

        l = (len as f64 / (k as f64 * (1.0 - a as f64) + a as f64)).trunc() as usize;
        iteration += 1;
    }

    let overlap = (l - (l as f64 * a as f64).round() as usize).max(1);
    let m = l.next_power_of_two();

    let mut frames: Vec<Vec<_>> = samples
        .windows(l)
        .step_by(overlap)
        .map(|s| {
            let mut v = s.to_vec();
            v.resize(m, 0.0);
            v
        })
        .collect();

    if frames.len() == 0 {
        let err = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            String::from("Zero length frames, insufficient sample size"),
        );
        return Err(err);
    }

    // Truncate or zero-pad +- tolerance number of frames.
    if frames.len() != k {
        frames.resize(k, vec![0.0; m]);
    }

    Ok(frames)
}

fn welch_frames(
    windowed_samples: &[f32],
    num_segments: Option<usize>,
    overlap: Option<f32>,
) -> Result<Vec<Vec<f32>>, WhisperAppError> {
    let mut len = windowed_samples.len();

    if len < 1 {
        return Err(WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            String::from("Zero sample size provided."),
        ));
    }
    let k = num_segments.unwrap_or(4);
    if k < 1 {
        return Err(WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            String::from("Cannot return 0 segments."),
        ));
    }
    let a = overlap.unwrap_or(0.5);

    let mut samples = vec![0.0; len];
    samples.copy_from_slice(windowed_samples);

    // If len < k, zero pad up to at least k.
    if len < k {
        samples.resize(k, 0.0);
        len = k;
    }

    // l - length - must land on a power of two.
    let l = (len as f64 / (k as f64 * (1.0 - a as f64) + a as f64)).trunc() as usize;
    let m = l.next_power_of_two();

    let overlap = (l - (l as f64 * a as f64).round() as usize).max(1);

    let frames: Vec<Vec<_>> = samples
        .windows(l)
        .step_by(overlap)
        .map(|s| {
            // If l does not land on a power of two, zero-pad the end of the samples.
            let mut v = s.to_vec();
            v.resize(m, 0.0);
            v
        })
        .collect();

    if frames.len() == 0 {
        let err = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            String::from("Zero length frames, insufficient sample size"),
        );
        return Err(err);
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
    debug_assert!(
        current.iter().all(|n| !n.is_nan()),
        "Nan values in current before smoothing"
    );
    debug_assert!(
        current.iter().all(|n| !n.is_infinite()),
        "Infinite values in current before smoothing"
    );
    debug_assert!(
        target.iter().all(|n| !n.is_nan()),
        "Nan values in target before smoothing"
    );
    debug_assert!(
        target.iter().all(|n| !n.is_infinite()),
        "Infinite values in target before smoothing"
    );

    for i in 0..current.len() {
        let x = current[i];
        let y = target[i];
        let z = x - y;
        let t = constants::SMOOTH_FACTOR * dt;
        current[i] += (target[i] - current[i]) * constants::SMOOTH_FACTOR * dt;
        if current[i].is_infinite() {
            panic!(
                "Infinite value.\n\
            Case:\n\
            x: {},\
            y: {},\
            x - y: {},\
            t: {}
            ",
                x, y, z, t
            );
        }
    }
    debug_assert!(
        current.iter().all(|n| !n.is_nan()),
        "Nan values after smoothing"
    );

    debug_assert!(
        current.iter().all(|n| !n.is_infinite()),
        "Infinite values after smoothing"
    );
}

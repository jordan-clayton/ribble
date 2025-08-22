use crate::controller::{
    AnalysisType, AtomicAnalysisType, RotationDirection, VisualizerPacket, NUM_VISUALIZER_BUCKETS,
};
use crate::utils::errors::RibbleError;
use crossbeam::channel::Receiver;
use parking_lot::RwLock;
use realfft::RealFftPlanner;
use std::error::Error;
use std::f32::consts::PI;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

struct VisualizerEngineState {
    planner: RwLock<RealFftPlanner<f32>>,
    incoming_samples: Receiver<VisualizerPacket>,
    buffer: RwLock<[f32; NUM_VISUALIZER_BUCKETS]>,
    visualizer_running: AtomicBool,
    analysis_type: AtomicAnalysisType,
}

impl VisualizerEngineState {
    // The same overlap is used for both power and spectrum density, so just call it "FFT"
    const FFT_RESOLUTION: f32 = 512.0;
    const FFT_OVERLAP: f32 = 0.5;
    const AMPLITUDE_OVERLAP: f32 = 0.25;
    const AMPLITUDE_GAMMA: f32 = 0.6;

    // ~ -12 dBFS
    const WAVEFORM_REF: f32 = 0.25;
    const WAVEFORM_KNEE: f32 = 2.0;

    fn new(
        incoming_samples: Receiver<VisualizerPacket>,
        starting_analysis_type: AnalysisType,
    ) -> Self {
        let buffer = RwLock::new([0.0; NUM_VISUALIZER_BUCKETS]);
        let visualizer_running = AtomicBool::new(false);
        let analysis_type = AtomicAnalysisType::new(starting_analysis_type);
        let planner = RwLock::new(RealFftPlanner::new());
        Self {
            planner,
            incoming_samples,
            buffer,
            visualizer_running,
            analysis_type,
        }
    }

    // TODO: remove sample_rate if/when cached in the state.
    // Sample rate (should be) is known when recording starts/a write request is sent.
    fn run_analysis(&self, sample: &[f32], sample_rate: f64) -> Result<(), RibbleError> {
        match self.analysis_type.load(Ordering::Acquire) {
            AnalysisType::AmplitudeEnvelope => self.amplitude_envelope(sample),
            AnalysisType::Waveform => self.waveform_oscillation(sample),
            AnalysisType::PowerSpectralDensity => self.power_analysis(sample, sample_rate),
            AnalysisType::LogSpectrum => self.log_spectrum_normalized(sample, sample_rate),
        }
    }

    fn fit_frames(window: &mut Vec<f32>, frame_size: usize, welch_target: f32, overlap_ratio: f32) {
        let padded_size = inverse_welch_frames(frame_size as f32, welch_target, overlap_ratio);

        let diff = (padded_size as f32 - window.len() as f32).abs() as usize;
        window.extend_from_within(window.len().saturating_sub(diff).saturating_sub(1)..);
    }

    // This is Power Spectrum Density estimate.
    fn power_analysis(&self, samples: &[f32], sample_rate: f64) -> Result<(), RibbleError> {
        let mut n_frames = 0;
        let mut power_samples = self.log_spectrum(samples, sample_rate, &mut n_frames)?;

        debug_assert!(n_frames > 0, "Log spectrum issue, n_frames either unset or incorrect");

        // Find the maximum (average) if it's greater than 1.
        let max = power_samples.iter().copied().fold(n_frames as f32, f32::max) / n_frames as f32;

        power_samples.iter_mut().for_each(|s| {
            // Do the welch average
            let avg = *s / n_frames as f32;
            // Normalize with the maximum
            *s = avg / max;
        });

        // Double-check for NaN/Inf
        debug_assert!(power_samples.iter().all(|f| f.is_finite()));

        debug_assert!(
            power_samples.iter().all(|n| *n >= 0.0 && *n <= 1.0),
            "Failed to normalize power analysis."
        );
        debug_assert_eq!(
            power_samples.len(),
            NUM_VISUALIZER_BUCKETS,
            "Failed to fit power_samples into buckets."
        );

        // To avoid an out-of-range memcpy (in release), limit the slice to the buffer size.
        self.buffer
            .write()
            .copy_from_slice(&power_samples[..NUM_VISUALIZER_BUCKETS]);
        Ok(())
    }

    fn waveform_oscillation(&self, samples: &[f32]) -> Result<(), RibbleError> {
        // This is in time domain, so it doesn't require high resolution
        let (frame_size, step_size) = compute_welch_frames(samples.len() as f32, NUM_VISUALIZER_BUCKETS as f32, 0.0);

        if frame_size == 0 {
            return Err(RibbleError::Core("Empty samples sent for waveform analysis".to_string()));
        }
        let mut window = samples
            .to_vec();

        // Do any padding to make sure things fit properly
        // This duplicates the last bit of the signal instead of zero-padding.
        Self::fit_frames(&mut window, frame_size, NUM_VISUALIZER_BUCKETS as f32, 0.0);

        let waveform = window
            .windows(frame_size)
            .step_by(step_size)
            .map(|win| {
                win.iter().map(|f| f.abs()).reduce(f32::max).unwrap_or(0.0)
            })
            .collect::<Vec<_>>();

        // Double-check for NaN/Inf
        debug_assert!(waveform.iter().all(|f| f.is_finite()));

        debug_assert_eq!(
            waveform.len(),
            NUM_VISUALIZER_BUCKETS,
            "Failed to fit waveform into buckets."
        );

        debug_assert!(waveform.iter().all(|f| f.is_finite()));

        // To avoid an out-of-range memcpy (in release), limit the slice to the buffer size.
        self.buffer
            .write()
            .copy_from_slice(&waveform[..NUM_VISUALIZER_BUCKETS]);
        Ok(())
    }

    // This is the time-domain RMS.
    fn amplitude_envelope(&self, samples: &[f32]) -> Result<(), RibbleError> {
        // This is in time domain, so it doesn't require high resolution
        let (frame_size, step_size) =
            compute_welch_frames(samples.len() as f32, NUM_VISUALIZER_BUCKETS as f32, Self::AMPLITUDE_OVERLAP);

        if frame_size == 0 {
            return Err(RibbleError::Core("Empty samples sent for amplitude analysis".to_string()));
        }

        let mut window = samples.to_vec();

        // Do any padding to make sure things fit properly
        // This duplicates the last bit of the signal instead of zero-padding.
        Self::fit_frames(&mut window, frame_size, NUM_VISUALIZER_BUCKETS as f32, Self::AMPLITUDE_OVERLAP);

        let amp_envelope = window
            .windows(frame_size)
            .step_by(step_size)
            .map(|win| {
                // This is just RMS, already normalized to 0 and 1
                (win.iter().copied().map(|n| n.powi(2)).sum::<f32>() / (win.len() as f32)).sqrt()
            })
            .collect::<Vec<_>>();

        // Assert no nan/infinite
        debug_assert!(amp_envelope.iter().all(|f| f.is_finite()));
        debug_assert_eq!(
            amp_envelope.len(),
            NUM_VISUALIZER_BUCKETS,
            "Failed to fit amplitude_envelope into buckets."
        );

        self.buffer
            .write()
            .copy_from_slice(&amp_envelope[..NUM_VISUALIZER_BUCKETS]);
        Ok(())
    }

    // This does the FFT computation and maps everything to frequency space.
    fn log_spectrum(&self, samples: &[f32], sample_rate: f64, n_frames: &mut usize) -> Result<[f32; NUM_VISUALIZER_BUCKETS], RibbleError> {
        let mut window = hann_window(samples);
        let frame_size = Self::FFT_RESOLUTION as usize;
        let step_size = compute_welch_step(frame_size as f32, Self::FFT_OVERLAP);

        let frames = window.windows(frame_size).step_by(step_size);
        *n_frames = frames.len();

        let fft = self.planner.write().plan_fft_forward(frame_size);
        let mut input = fft.make_input_vec();
        let mut output = fft.make_output_vec();
        let mut spectrum_samples = [0.0; NUM_VISUALIZER_BUCKETS];

        let fft_frame_size = output.len();
        let min_freq = sample_rate / (fft_frame_size as f64);
        let max_freq = sample_rate / 2.0;

        let log_min = min_freq.log10();
        let log_max = max_freq.log10();

        let log_range = log_max - log_min;

        debug_assert!(log_range.is_finite() && log_min.is_finite());

        // Compute edges -> map frequency bins to log-spaced buckets
        // (human perception; low frequencies = tighter resolution).
        let bucket_edges: Vec<f64> = (0..=NUM_VISUALIZER_BUCKETS)
            .map(|k| {
                10.0f64.powf(log_min + log_range * (k as f64) / (NUM_VISUALIZER_BUCKETS as f64))
            })
            .collect();

        for frame in frames {
            input.copy_from_slice(frame);
            fft.process(&mut input, &mut output)?;

            for (i, &value) in output.iter().enumerate() {
                // Convert each bin index to a frequency
                let freq = (i as f64) * sample_rate / (fft_frame_size as f64);
                // Check if the frequency falls within log_range
                if freq < min_freq || freq > max_freq {
                    continue;
                }

                debug_assert!(freq.is_finite(), "Frequency inf or NaN: {freq}");
                debug_assert!(value.is_finite(), "Complex value inf or NaN: {value}");

                // Find the bucket and increment it by the value.norm_sqr() (power estimate).
                let closest =
                    bucket_edges.binary_search_by(|edge| edge.partial_cmp(&freq).unwrap());
                let bucket = match closest {
                    // Falls right on an edge -> needs to be 1 less.
                    Ok(index) => index.saturating_sub(1),
                    // Falls closest to an edge -> needs to be 1 less.
                    Err(closest_insertion) => closest_insertion.saturating_sub(1),
                };
                spectrum_samples[bucket] += value.norm_sqr();
            }
        }

        Ok(spectrum_samples)
    }


    // This is for visualizing the frequency distribution
    fn log_spectrum_normalized(&self, samples: &[f32], sample_rate: f64) -> Result<(), RibbleError> {
        let mut _n_frames = 0;
        // Power analysis does welch averaging, this method does not.
        // Since the frame len is computed in log_spectrum and the code is mostly identical up to
        // the spectrum samples, it's easiest to just send a mut ref and just ignore it.
        let mut spectrum_samples = self.log_spectrum(samples, sample_rate, &mut _n_frames)?;
        let max_amp = spectrum_samples.iter().copied().fold(1f32, f32::max);
        // Normalize the buckets
        for res in spectrum_samples.iter_mut() {
            *res /= max_amp;
        }
        debug_assert_eq!(
            spectrum_samples.len(),
            NUM_VISUALIZER_BUCKETS,
            "Failed to fit spectrum_density into buckets"
        );
        debug_assert!(
            spectrum_samples.iter().all(|n| *n <= 1.0 && *n >= 0.0),
            "Failed to normalize in spectrum density calculations"
        );
        // To avoid an out-of-range memcpy (in release), limit the slice to the buffer size.
        self.buffer
            .write()
            .copy_from_slice(&spectrum_samples[..NUM_VISUALIZER_BUCKETS]);
        Ok(())
    }
}

fn hann_window(samples: &[f32]) -> Vec<f32> {
    let len = samples.len() as f32;
    samples
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let t = (i as f32) / len;
            let hann = 0.5 * (1.0 - (2.0 * PI * t).cos());
            *f * hann
        })
        .collect()
}

// This is only good for the time-domain functions.
// I'm really not sure what to call this, but that can be determined later.
// TODO: figure out a better name.
fn compute_welch_frames(sample_len: f32, output_len: f32, overlap_ratio: f32) -> (usize, usize) {
    let frame_size =
        sample_len / (1.0 + (output_len - 1.0) * (1.0 - overlap_ratio));
    let step_size = frame_size * (1.0 - overlap_ratio);
    (frame_size.round() as usize, step_size.round() as usize)
}

#[inline]
fn inverse_welch_frames(frame_size: f32, output_len: f32, overlap_ratio: f32) -> usize {
    (frame_size * (1.0 + (output_len - 1.0) * (1.0 - overlap_ratio))).round() as usize
}
#[inline]
fn compute_welch_step(frame_size: f32, overlap_ratio: f32) -> usize {
    (frame_size * (1f32 - overlap_ratio)).round() as usize
}

// If this can be used elsewhere, expose publicly and possibly move somewhere else.
// NOTE: if fully unused, just remove.
#[inline]
fn interpolate_buckets(src: &[f32], dst: &mut [f32]) {
    let src_len = src.len();
    let dst_len = dst.len();

    let last_index = src_len - 1;
    for (i, val) in dst.iter_mut().enumerate() {
        let t = i as f32 / (dst_len - 1) as f32;
        let frac_idx = last_index as f32 * t;

        let bucket_t = frac_idx.fract();
        let floor = ((frac_idx).floor() as usize).min(last_index);
        let ceil = ((frac_idx).ceil() as usize).min(last_index);

        *val = egui::lerp(src[floor]..=src[ceil], bucket_t);
    }
}

// TODO: kernel-exposed methods for updating sample rate/buffer size for precomputing FFT planner state.
// At the moment, it's not necessary to actually precompute; it's just low-hanging optimization fruit.
pub(super) struct VisualizerEngine {
    inner: Arc<VisualizerEngineState>,
    work_thread: Option<JoinHandle<()>>,
}
impl VisualizerEngine {
    pub(super) fn new(
        incoming_samples: Receiver<VisualizerPacket>,
        starting_analysis_type: AnalysisType,
    ) -> Self {
        let inner = Arc::new(VisualizerEngineState::new(
            incoming_samples,
            starting_analysis_type,
        ));
        let thread_inner = Arc::clone(&inner);

        let work_thread = Some(thread::spawn(move || {
            // When this receives new audio, perform Audio analysis calculations based on the current
            // visualizer Analysis type.

            while let Ok(packet) = thread_inner.incoming_samples.recv() {
                // TODO: if/when pre-computing the FFT planner/windowing, look at implementing a
                // different kind of packet.
                match packet {
                    VisualizerPacket::VisualizerSample {
                        sample,
                        sample_rate,
                    } => {
                        // If the visualizer isn't open, just skip over the sample and don't do the
                        // computation.
                        if !thread_inner.visualizer_running.load(Ordering::Acquire) {
                            continue;
                        }

                        if sample.is_empty() {
                            log::warn!("Visualizer sent empty sample packet!");
                            continue;
                        }

                        // Instead of returning the error to finish the thread, just log it.
                        // There may be errors across each visualization analysis, so the loop should
                        // remain.
                        if let Err(e) = thread_inner.run_analysis(&sample, sample_rate) {
                            log::warn!(
                                "Failed to run visual analysis.\nType: {}, Error: {e}, Error Source: {:#?}",
                                thread_inner.analysis_type.load(Ordering::Acquire),
                                e.source()
                            );
                        }
                    }
                    VisualizerPacket::Shutdown => break,
                }
            }
        }));

        Self { inner, work_thread }
    }

    pub(super) fn set_visualizer_visibility(&self, is_visible: bool) {
        self.inner
            .visualizer_running
            .store(is_visible, Ordering::Release);
    }

    pub(super) fn try_read_visualization_buffer(
        &self,
        copy_buffer: &mut [f32; NUM_VISUALIZER_BUCKETS],
    ) {
        if let Some(buffer) = self.inner.buffer.try_read() {
            copy_buffer.copy_from_slice(buffer.deref())
        }
    }

    pub(super) fn read_visualizer_analysis_type(&self) -> AnalysisType {
        self.inner.analysis_type.load(Ordering::Acquire)
    }

    pub(super) fn write_visualizer_analysis_type(&self, new_type: AnalysisType) {
        self.inner.analysis_type.store(new_type, Ordering::Release);
    }

    // There's no real contention here; rotations are rare,
    // and this isn't RMW critical, so this can be load -> rotate -> store.
    pub(super) fn rotate_visualizer_type(&self, direction: RotationDirection) {
        self.inner.analysis_type.store(
            self.inner
                .analysis_type
                .load(Ordering::Acquire)
                .rotate(direction),
            Ordering::Release,
        )
    }
}

impl Drop for VisualizerEngine {
    fn drop(&mut self) {
        log::info!("Dropping VisualizerEngine.");
        if let Some(handle) = self.work_thread.take() {
            log::info!("Joining VisualizerEngine work thread.");
            handle.join().expect(
                "The visualizer thread is not expected to panic and should run without issues.",
            );
            log::info!("VisualizerEngine work thread joined.");
        }
    }
}

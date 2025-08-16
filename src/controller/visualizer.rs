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
    const POWER_OVERLAP: f32 = 0.5;
    const AMPLITUDE_OVERLAP: f32 = 0.25;
    const POWER_GAIN: f32 = 30.0;
    const WAVEFORM_GAIN: f32 = Self::POWER_GAIN / 2.0;

    // This could be made an option, I suppose?
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
            AnalysisType::Waveform => self.normalized_waveform(sample),
            AnalysisType::Power => self.power_analysis(sample),
            AnalysisType::SpectrumDensity => self.spectrum_density(sample, sample_rate),
        }
    }

    fn fit_frames(window: &mut Vec<f32>, frame_size: usize) {
        let floor_ratio = (window.len() / frame_size) as f32;
        let float_ratio = window.len() as f32 / frame_size as f32;

        // If the number of windows isn't precisely an integer, duplicate the last n samples
        // of the signal.
        if float_ratio > floor_ratio {
            let to_add = (1.0 - float_ratio.fract() * frame_size as f32).ceil() as usize;
            let new_size = window.len() + to_add;
            let start_idx = window.len() - to_add;
            window.extend_from_within(start_idx..);
            debug_assert_eq!(window.len(), new_size);
        }
    }

    fn power_analysis(&self, samples: &[f32]) -> Result<(), RibbleError> {
        // True = apply gain
        let mut window = hann_window(samples, true);

        let (frame_size, step_size) =
            compute_welch_frames(window.len() as f32, Self::POWER_OVERLAP);

        Self::fit_frames(&mut window, frame_size);

        let frames = window.windows(frame_size).step_by(step_size);

        let fft = self.planner.write().plan_fft_forward(frame_size);
        let mut input = fft.make_input_vec();
        let mut output = fft.make_output_vec();
        let mut power_samples = [0.0; NUM_VISUALIZER_BUCKETS];
        for (i, frame) in frames.enumerate() {
            input.copy_from_slice(frame);
            fft.process(&mut input, &mut output)?;
            let power =
                output.iter().map(|c| c.norm_sqr()).sum::<f32>() / (frame.len() as f32).powi(2);
            let log_power = if power > 0.0 { power.log10() } else { 0.0 };
            power_samples[i] = log_power
        }

        // Double-check for NaN/Inf
        debug_assert!(power_samples.iter().all(|f| f.is_finite()));

        let max_amp = power_samples.iter().copied().fold(1.0, f32::max);

        for amp in power_samples.iter_mut() {
            *amp = (*amp / max_amp).max(0.0);
        }

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

    fn normalized_waveform(&self, samples: &[f32]) -> Result<(), RibbleError> {
        let (frame_size, step_size) = compute_welch_frames(samples.len() as f32, 0f32);
        // Apply a gain and normalize the audio signal.
        let mut window = samples
            .iter()
            .map(|s| (*s * Self::WAVEFORM_GAIN) / Self::WAVEFORM_GAIN)
            .collect::<Vec<_>>();

        Self::fit_frames(&mut window, frame_size);

        let waveform = window
            .windows(frame_size)
            .step_by(step_size)
            .map(|win| {
                let sum = win.iter().sum::<f32>();
                let avg = sum / win.len() as f32;

                // Remapping will stick 0 @ 0.5, which isn't really ideal for the "soundbar" widget
                // Instead, treat -1/1 as a "maximum deviation" from silence.
                avg.abs().clamp(0.0, 1.0)
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

    fn amplitude_envelope(&self, samples: &[f32]) -> Result<(), RibbleError> {
        let (frame_size, step_size) =
            compute_welch_frames(samples.len() as f32, Self::AMPLITUDE_OVERLAP);
        let mut window = samples
            .iter()
            .map(|s| *s * Self::WAVEFORM_GAIN)
            .collect::<Vec<_>>();

        Self::fit_frames(&mut window, frame_size);

        let mut amp_envelope = window
            .windows(frame_size)
            .step_by(step_size)
            .map(|win| {
                (win.iter().copied().map(|n| n.powi(2)).sum::<f32>() / (win.len() as f32)).sqrt()
            })
            .collect::<Vec<_>>();

        // Assert no nan/infinite
        debug_assert!(amp_envelope.iter().all(|f| f.is_finite()));

        // Grab the maximum rms
        let max_rms = amp_envelope.iter().copied().fold(1f32, f32::max);

        // Normalize the RMS.
        for rms in amp_envelope.iter_mut() {
            *rms /= max_rms;
        }

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

    fn spectrum_density(&self, samples: &[f32], sample_rate: f64) -> Result<(), RibbleError> {
        // I don't remember why I'm not applying gain here...
        let mut window = hann_window(samples, false);
        let (frame_size, step_size) =
            compute_welch_frames(window.len() as f32, Self::POWER_OVERLAP);

        Self::fit_frames(&mut window, frame_size);

        let frames = window.windows(frame_size).step_by(step_size);

        let fft = self.planner.write().plan_fft_forward(frame_size);
        let mut input = fft.make_input_vec();
        let mut output = fft.make_output_vec();
        let mut spectrum_samples = [0.0; NUM_VISUALIZER_BUCKETS];

        let frame_size = output.len();
        let min_freq = sample_rate / (frame_size as f64);
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
                let freq = (i as f64) * sample_rate / (frame_size as f64);
                // Check if the frequency falls within log_range
                if freq < min_freq || freq > max_freq {
                    continue;
                }

                debug_assert!(freq.is_finite(), "Frequency inf or NaN: {freq}");
                debug_assert!(value.is_finite(), "Complex value inf or NaN: {value}");

                // Find the bucket.
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

// TODO: probably better to just include a gain multiplier as an argument to the function to
// encapsulate the constants within the struct.
fn hann_window(samples: &[f32], apply_gain: bool) -> Vec<f32> {
    let len = samples.len() as f32;
    let multiplier = if apply_gain {
        VisualizerEngineState::POWER_GAIN
    } else {
        1.0
    };
    samples
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let t = (i as f32) / len;
            let hann = 0.5 * (1.0 - (2.0 * PI * t).cos());
            *f * hann * multiplier
        })
        .collect()
}

fn compute_welch_frames(sample_len: f32, overlap_ratio: f32) -> (usize, usize) {
    let frame_size =
        sample_len / (1f32 + (NUM_VISUALIZER_BUCKETS as f32 - 1f32) * (1f32 - overlap_ratio));
    let step_size = frame_size * (1f32 - overlap_ratio);
    (frame_size.round() as usize, step_size.round() as usize)
}

// TODO: kernel-exposed methods for updating sample rate/buffer size
// For precomputing an FFTplanner state + cached sample rate/frequencies
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

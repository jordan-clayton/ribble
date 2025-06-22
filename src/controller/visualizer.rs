use crate::utils::constants::{
    AMPLITUDE_OVERLAP, NUM_BUCKETS, POWER_GAIN, POWER_OVERLAP, WAVEFORM_GAIN,
};
use crate::utils::errors::{RibbleAppError, RibbleError};
use crate::utils::pcm_f32::{IntoPcmF32, PcmF32Convertible};
use atomic_enum::atomic_enum;
use crossbeam::channel::{Receiver, Sender};
use parking_lot::RwLock;
use realfft::num_traits::pow;
use realfft::RealFftPlanner;
use ribble_whisper::utils::constants::INPUT_BUFFER_CAPACITY;
use ribble_whisper::utils::get_channel;
use std::f32::consts::PI;
use std::ops::Deref;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use strum::{Display, EnumIter};

// TODO: might need to move this to somewhere shared.
pub(crate) enum RotationDirection {
    Clockwise,
    CounterClockwise,
}
#[atomic_enum]
#[derive(PartialEq, EnumIter, Display)]
pub(crate) enum AnalysisType {
    #[strum(to_string = "Amplitude")]
    AmplitudeEnvelope = 0,
    Waveform,
    Power,
    #[strum(to_string = "Spectrum Density")]
    SpectrumDensity,
}

impl AnalysisType {
    // NOTE: this is obviously a little un-maintainable and not the greatest solution if the AnalysisTypes grow.
    // If it becomes untenable, look into a macro-based solution.
    // TODO: write a quick test to stamp out bugs here
    pub(crate) fn rotate(&self, direction: RotationDirection) -> Self {
        match (self, direction) {
            (AnalysisType::AmplitudeEnvelope, RotationDirection::Clockwise) => {
                AnalysisType::Waveform
            }
            (AnalysisType::AmplitudeEnvelope, RotationDirection::CounterClockwise) => {
                AnalysisType::SpectrumDensity
            }
            (AnalysisType::Waveform, RotationDirection::Clockwise) => AnalysisType::Power,
            (AnalysisType::Waveform, RotationDirection::CounterClockwise) => {
                AnalysisType::AmplitudeEnvelope
            }
            (AnalysisType::Power, RotationDirection::Clockwise) => AnalysisType::SpectrumDensity,
            (AnalysisType::Power, RotationDirection::CounterClockwise) => AnalysisType::Waveform,
            (AnalysisType::SpectrumDensity, RotationDirection::Clockwise) => {
                AnalysisType::AmplitudeEnvelope
            }
            (AnalysisType::SpectrumDensity, RotationDirection::CounterClockwise) => {
                AnalysisType::Power
            }
        }
    }
}

// TODO: Expect to need to return to this (The underlying FFT utils need significant refactoring)
// TODO: construct the FFT planner inside the Engine + move all utility methods here.

// TODO: possibly add more of these here? --> maybe don't. 32 bit integer audio is weird; this isn't professional recording software.
// TODO twice: Possibly make generic to use across the writer -- not sure yet.
pub(crate) enum VisualizerSample {
    S16(Arc<[i16]>),
    F32(Arc<[f32]>),
}

impl From<Arc<[i16]>> for VisualizerSample {
    fn from(sample: Arc<[i16]>) -> Self {
        Self::S16(sample)
    }
}
impl From<Arc<[f32]>> for VisualizerSample {
    fn from(sample: Arc<[f32]>) -> Self {
        Self::F32(sample)
    }
}
pub(super) struct VisualizerSamplePacket {
    sample: VisualizerSample,
    sample_rate: f64,
}

struct VisualizerEngineState {
    planner: RwLock<RealFftPlanner<f32>>,
    incoming: Receiver<VisualizerSamplePacket>,
    buffer: RwLock<[f32; NUM_BUCKETS]>,
    visualizer_running: AtomicBool,
    analysis_type: AtomicAnalysisType,
}

impl VisualizerEngineState {
    // TODO: precompute the frame size/FFT planner, etc.
    // TODO: maybe return RibbleAppError? Might not matter.
    fn power_analysis(&self, samples: &[f32]) -> Result<(), RibbleError> {
        // True = apply gain
        let mut window_samples = hann_window(samples, true);

        let (frame_size, step_size) =
            compute_welch_frames(window_samples.len() as f32, POWER_OVERLAP);

        let frames = window_samples.windows(frame_size).step_by(step_size);
        debug_assert_eq!(
            frames.len(),
            NUM_BUCKETS,
            "Failed to compute window sizes properly in power analysis."
        );

        let fft = self.planner.write().plan_fft_forward(frame_size);
        let mut input = fft.make_input_vec();
        let mut output = fft.make_output_vec();
        let mut power_samples = vec![0.0; NUM_BUCKETS];
        for (i, frame) in frames.enumerate() {
            input.copy_from_slice(frame);
            fft.process(&mut input, &mut output)?;
            let power =
                output.iter().map(|c| c.norm_sqr()).sum::<f32>() / (frame.len() as f32).powi(2);
            let log_power = if power > 0.0 { power.log10() } else { 0.0 };
            power_samples[i] = log_power
        }

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
            NUM_BUCKETS,
            "Failed to fit power_samples into buckets."
        );

        self.buffer.write().copy_from_slice(&power_samples);
        Ok(())
    }

    // NOTE: this normalizes amplitude to [-1, 1], then remaps the range to [0, 1]
    fn normalized_waveform(&self, samples: &[f32]) -> Result<(), RibbleError> {
        // No smoothing for the waveform
        let (frame_size, step_size) = compute_welch_frames(samples.len() as f32, 0f32);
        let window = samples
            .iter()
            .map(|s| *s * WAVEFORM_GAIN)
            .collect::<Vec<_>>();

        let mut waveform = window
            .windows(frame_size)
            .step_by(step_size)
            .map(|window| window.iter().sum::<f32>() / (window.len() as f32))
            .collect::<Vec<_>>();

        let max_amp = waveform
            .iter()
            .copied()
            .fold(1f32, |acc, n| acc.max(n.abs()));

        // Normalize and remap between [-1, 1]
        for avg in waveform.iter_mut() {
            let normalized = *avg / max_amp;
            *avg = normalized * 0.5 + 0.5;
        }

        debug_assert_eq!(
            waveform.len(),
            NUM_BUCKETS,
            "Failed to fit waveform into buckets."
        );

        self.buffer.write().copy_from_slice(&waveform);
        Ok(())
    }

    fn amplitude_envelope(&self, samples: &[f32]) -> Result<(), RibbleError> {
        let (frame_size, step_size) = compute_welch_frames(samples.len() as f32, AMPLITUDE_OVERLAP);
        let window = samples
            .iter()
            .map(|s| *s * WAVEFORM_GAIN)
            .collect::<Vec<_>>();

        let mut amp_envelope = window
            .windows(frame_size)
            .step_by(step_size)
            .map(|window| {
                (window.iter().map(|n| pow(*n, 2)).sum::<f32>() / (window.len() as f32)).sqrt()
            })
            .collect::<Vec<_>>();

        let max_rms = amp_envelope.iter().fold(1f32, f32::max);

        // Normalize between [-1, 1]
        for rms in amp_envelope.iter_mut() {
            *rms = *rms / max_rms;
        }

        debug_assert_eq!(
            amp_envelope.len(),
            NUM_BUCKETS,
            "Failed to fit amplitude_envelope into buckets."
        );

        self.buffer.write().copy_from_slice(&amp_envelope);
        Ok(())
    }

    fn frequency_analysis(&self, samples: &[f32], sample_rate: f64) -> Result<(), RibbleError> {
        // I don't remember why I'm not applying gain...
        let mut window_samples = hann_window(samples, false);
        // TODO: look at precomputing on changing settings/running transcriber, etc.
        // Assert nonzero frame size
        let (frame_size, step_size) = compute_welch_frames(samples.len() as f32, POWER_OVERLAP);
        let frames = window_samples.windows(frame_size).step_by(step_size);
        debug_assert_eq!(
            frames.len(),
            NUM_BUCKETS,
            "Failed to compute window sizes properly in frequency analysis."
        );

        // TODO: the FFT stuff can be precomputed upon changing the analysis type + Sample Rate
        // Not quite sure how to handle this just yet.

        let fft = self.planner.write().plan_fft_forward(frame_size);
        let mut input = fft.make_input_vec();
        let mut output = fft.make_output_vec();
        let mut spectrum_samples = vec![0.0; NUM_BUCKETS];

        let n = output.len();
        let min_freq = sample_rate / (frame_size as f64);
        let max_freq = sample_rate / 2.0;

        let log_min = min_freq.log10();
        let log_max = max_freq.log10();

        let log_range = log_max - log_min;
        // TODO:
        debug_assert!(
            !(log_min.is_nan()
                || log_min.is_infinite()
                || log_range.is_nan()
                || log_range.is_infinite())
        );
        // Compute edges -> map frequency bins to log-spaced buckets
        // (human perception; low frequencies = tighter resolution).
        let bucket_edges: Vec<f64> = (0..=NUM_BUCKETS)
            .map(|n| 10.0.powf(log_min + log_range * (n as f64) / (NUM_BUCKETS as f64)))
            .collect();

        for frame in frames {
            input.copy_from_slice(frame);
            fft.process(&mut input, &mut output)?;

            for (i, &value) in output.iter().enumerate() {
                // Convert each bin index to a frequency
                let freq = (i as f64) * sample_rate / (n as f64);
                // Check if the frequency falls within log_range
                if freq < min_freq || freq > max_freq {
                    continue;
                }

                // TODO: this might not be necessary.
                debug_assert!(!(freq.is_nan() || freq.is_infinite()));
                debug_assert!(!value.is_nan());

                // Find the bucket.
                let closest =
                    bucket_edges.binary_search_by(|edge| edge.partial_cmp(&value).unwrap());
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
            *res = *res / max_amp;
        }
        debug_assert!(
            spectrum_samples.iter().all(|n| *n <= 1.0 && *n >= 0.0),
            "Failed to normalize in spectrum density calculations"
        );
        self.buffer.write().copy_from_slice(&spectrum_samples);
        Ok(())
    }
}

fn hann_window(samples: &[f32], apply_gain: bool) -> Vec<f32> {
    let len = samples.len() as f32;
    let multiplier = if apply_gain { POWER_GAIN } else { 1.0 };
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
fn apply_gain(samples: &mut [f32], gain: f32) {
    for sample in samples.iter_mut() {
        *sample *= gain;
    }
}

// (Frame size, step size)
// TODO: This should probably be pre-computed according to buffer size
// -> handle this in the controller and cache
fn compute_welch_frames(sample_len: f32, overlap_ratio: f32) -> (usize, usize) {
    let frame_size = sample_len / (1f32 + (NUM_BUCKETS as f32 - 1f32) * (1f32 - overlap_ratio));
    let step_size = frame_size * (1f32 - overlap_ratio);
    (frame_size.round() as usize, step_size.round() as usize)
}

// TODO: kernel-exposed methods for updating sample rate/buffer size
// For precomputing an FFTplanner state that can be ArcSwapped
pub(super) struct VisualizerEngine {
    outgoing: Sender<VisualizerSamplePacket>,
    inner: Arc<VisualizerEngineState>,
    // TODO: swap the error type once errors have been re-implemented.
    work_thread: Option<JoinHandle<Result<(), RibbleAppError>>>,
}
impl VisualizerEngine {
    pub(super) fn new() -> Self {
        let buffer = RwLock::new([0.0; NUM_BUCKETS]);
        let visualizer_running = AtomicBool::new(false);
        let analysis_type = AtomicAnalysisType::new(AnalysisType::Waveform);
        // TODO: determine what the actual size of this should be.
        let (sender, receiver) = get_channel(INPUT_BUFFER_CAPACITY);
        let planner = RwLock::new(RealFftPlanner::new());
        let inner = Arc::new(VisualizerEngineState {
            planner,
            incoming: receiver,
            buffer,
            visualizer_running,
            analysis_type,
        });

        let thread_inner = Arc::clone(&inner);

        let work_thread = Some(thread::spawn(move || {
            // When this receives new audio, perform Audio analysis calculations based on the current
            // visualizer Analysis type.

            while let Ok(packet) = thread_inner.incoming.recv() {
                let VisualizerSamplePacket {
                    sample,
                    sample_rate,
                } = packet;
                // TODO: Possibly bury the implementation down into an internal match.
                match (thread_inner.analysis_type.load(Ordering::Acquire), sample) {
                    (AnalysisType::AmplitudeEnvelope, VisualizerSample::F32(audio)) => {
                        thread_inner.amplitude_envelope(&audio)
                    }
                    (AnalysisType::AmplitudeEnvelope, VisualizerSample::S16(audio)) => {
                        let fp_audio = audio.iter().map(|i| i.into_pcm_f32()).collect();
                        thread_inner.amplitude_envelope(&fp_audio)
                    }
                    (AnalysisType::Waveform, VisualizerSample::F32(audio)) => {
                        thread_inner.normalized_waveform(&audio)
                    }
                    (AnalysisType::Waveform, VisualizerSample::S16(audio)) => {
                        let fp_audio = audio.iter().map(|i| i.into_pcm_f32()).collect();
                        thread_inner.normalized_waveform(&fp_audio)
                    }
                    (AnalysisType::Power, VisualizerSample::F32(audio)) => {
                        // TODO: return to this and unwrap the match once methods have been implemented.
                        thread_inner.power_analysis(&audio)
                    }

                    (AnalysisType::Power, VisualizerSample::S16(audio)) => {
                        let fp_audio = audio.iter().map(|i| i.into_pcm_f32()).collect();
                        // TODO: return to this and unwrap the match once methods have been implemented.
                        thread_inner.power_analysis(&fp_audio)
                    }
                    (AnalysisType::SpectrumDensity, VisualizerSample::F32(audio)) => {
                        thread_inner.frequency_analysis(&audio, sample_rate)
                    }
                    (AnalysisType::SpectrumDensity, VisualizerSample::S16(audio)) => {
                        let fp_audio = audio.iter().map(|i| i.into_pcm_f32()).collect();
                        thread_inner.frequency_analysis(&fp_audio, sample_rate)
                    }
                }?;
            }
            Ok(())
        }));

        Self {
            outgoing: sender,
            inner,
            work_thread,
        }
    }

    pub(super) fn set_visualizer_visibility(&self, visibility: bool) {
        self.inner
            .visualizer_running
            .store(visibility, Ordering::Release);
    }

    // TODO: look at removing this --> I don't think the rest of the application needs to know if the visualizer is currently running.
    pub(super) fn visualizer_running(&self) -> bool {
        self.inner.visualizer_running.load(Ordering::Acquire)
    }

    // The Arc<T> is to make sharing a little quicker -- otherwise there'd be the need to deep clone.
    pub(super) fn update_visualizer_data<T: PcmF32Convertible>(
        &self,
        buffer: Arc<[T]>,
        sample_rate: f64,
    ) {
        // TODO: If the public method gets removed, just make the atomic load here.
        if self.visualizer_running() {
            let sample: VisualizerSample = Arc::clone(&buffer).into();
            let packet = VisualizerSamplePacket {
                sample,
                sample_rate,
            };
            // Since this can only fail if the inner receiver is dropped, that means inner has also dropped->
            // Which means this engine has also dropped, making it impossible to make this method call.

            // It might be okay to just ignore this specific result -> or panic, seeing as something -very- wrong is going on if this fails.
            let _ = self.outgoing.send(packet);
        }
    }
    pub(super) fn try_read_visualization_buffer(&self, copy_buffer: &mut [f32; NUM_BUCKETS]) {
        if let Some(buffer) = self.inner.buffer.try_read() {
            copy_buffer.copy_from_slice(buffer.deref())
        }
    }
    pub(super) fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.inner.analysis_type.load(Ordering::Acquire)
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
        if let Some(handle) = self.work_thread.take() {
            handle
                .join()
                .expect(
                    "The visualizer thread is not expected to panic and should run without issues.",
                )
                // TODO: clarify this
                .expect("I haven't determined what exactly what might cause an error just yet.");
        }
    }
}

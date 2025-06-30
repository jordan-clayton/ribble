// TODO: RecorderEngine -> handle recording stuff here
// NOTE: use the kernel to spawn the write thread, only run the audio fanout in the recording loop
// API needs:
// -> (Possibly) an exposed output file handle
// -> constructors
// -> kernel setter
// -> Accessors (read/write locks) for Configs
// -> The recording loop
use crate::controller::ConsoleMessage;
use crate::controller::Progress;
use crate::controller::RibbleMessage;
use crate::controller::RibbleWorkerHandle;
use crate::controller::kernel::EngineKernel;
use crate::utils::errors::{RibbleAppError, RibbleError};
use crate::utils::pcm_f32::PcmF32Convertible;
use crate::utils::recorder_configs::{
    RibbleChannels, RibblePeriod, RibbleRecordingFormat, RibbleSampleRate,
};
use arc_swap::ArcSwap;
use crossbeam::channel::{Receiver, TrySendError};
use ribble_whisper::audio::audio_backend::CaptureSpec;
use ribble_whisper::audio::microphone::MicCapture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::audio::recorder::RecorderSample;
use ribble_whisper::utils::constants::INPUT_BUFFER_CAPACITY;
use ribble_whisper::utils::get_channel;
use ribble_whisper::whisper::model::ModelRetriever;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};

#[derive(Default, Copy, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct RibbleRecordingConfigs {
    sample_rate: RibbleSampleRate,
    channel_configs: RibbleChannels,
    period: RibblePeriod,
    format: RibbleRecordingFormat,
}

impl RibbleRecordingConfigs {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_sample_rate(mut self, sample_rate: RibbleSampleRate) -> Self {
        self.sample_rate = sample_rate;
        self
    }
    pub(crate) fn with_num_channels(mut self, channel_configs: RibbleChannels) -> Self {
        self.channel_configs = channel_configs;
        self
    }
    pub(crate) fn with_period(mut self, period: RibblePeriod) -> Self {
        self.period = period;
        self
    }
    pub(crate) fn with_recordingformat(mut self, format: RibbleRecordingFormat) -> Self {
        self.format = format;
        self
    }

    pub(crate) fn sample_rate(&self) -> RibbleSampleRate {
        self.sample_rate
    }
    pub(crate) fn num_channels(&self) -> RibbleChannels {
        self.channel_configs
    }
    pub(crate) fn period(&self) -> RibblePeriod {
        self.period
    }
    pub(crate) fn format(&self) -> RibbleRecordingFormat {
        self.format
    }
}

impl From<CaptureSpec> for RibbleRecordingConfigs {
    fn from(value: CaptureSpec) -> Self {
        let sample_rate = value.sample_rate().into();
        let num_channels = value.channels().into();
        let period = value.period().into();
        Self::new()
            .with_sample_rate(sample_rate)
            .with_num_channels(num_channels)
            .with_period(period)
    }
}

impl From<RibbleRecordingConfigs> for CaptureSpec {
    fn from(value: RibbleRecordingConfigs) -> Self {
        Self::new()
            .with_sample_rate(value.sample_rate.into())
            .with_num_channels(value.channel_configs.into())
            .with_period(value.period.into())
    }
}

// TODO: migrate to message queues -> if Inner is no longer relevant, migrate the inner struct to
// the outer one.
struct RecorderEngineState<M: ModelRetriever, E: EngineKernel<Retriever = M>> {
    engine_kernel: Weak<E>,
    recorder_running: Arc<AtomicBool>,
    recorder_configs: ArcSwap<RibbleRecordingConfigs>,
}

impl<M: ModelRetriever, E: EngineKernel<Retriever = M>> RecorderEngineState<M, E> {
    fn new() -> Self {
        let configs = Arc::new(Default::default());
        Self {
            engine_kernel: Weak::new(),
            recorder_running: Arc::new(AtomicBool::new(false)),
            recorder_configs: ArcSwap::from(configs),
        }
    }

    fn run_recorder_loop<T: RecorderSample + PcmF32Convertible>(
        &self,
        spec: CaptureSpec,
    ) -> Result<(), RibbleError> {
        // TODO: the kernel biz needs to be refactored; things are mostly for stubbing right now.
        let kernel = self.engine_kernel.upgrade().ok_or(RibbleError::Core(
            "Kernel not attached to RecorderEngine.".to_string(),
        ))?;

        let (audio_sender, audio_receiver) = get_channel::<Arc<[T]>>(INPUT_BUFFER_CAPACITY);
        let sink = ArcChannelSink::new(audio_sender);
        let mic = kernel.request_audio_capture(spec, sink)?;
        // TODO: send a write job through a channel -> Send the receiver.
        let (write_sender, write_receiver) = get_channel::<Arc<[T]>>(INPUT_BUFFER_CAPACITY);

        let sample_rate = mic.sample_rate();
        mic.play();
        while self.recorder_running.load(Ordering::Acquire) {
            match audio_receiver.recv() {
                Ok(audio) => {
                    if let Err(TrySendError::Disconnected(_)) =
                        write_sender.try_send(Arc::clone(&audio))
                    {
                        self.recorder_running.store(false, Ordering::Release);
                    }

                    let visualizer_converted =
                        audio.iter().copied().map(|s| s.into_pcm_f32()).collect();

                    // TODO: this instead will have to move to the message queue.
                    kernel.update_visualizer_data(
                        Arc::from(visualizer_converted),
                        sample_rate as f64,
                    );
                }
                Err(_) => self.recorder_running.store(false, Ordering::Release),
            }
        }
        mic.pause();

        Ok(())
    }
}

pub(super) struct RecorderEngine<M: ModelRetriever, E: EngineKernel<Retriever = M>> {
    inner: Arc<RecorderEngineState<M, E>>,
}

impl<M: ModelRetriever, E: EngineKernel<Retriever = M>> RecorderEngine<M, E> {
    pub(super) fn new() -> Self {
        Self {
            inner: Arc::new(RecorderEngineState::new()),
        }
    }

    pub(super) fn start_recording(&self) -> RibbleWorkerHandle {
        let thread_inner = Arc::clone(&self.inner);

        let worker = std::thread::spawn(move || {
            let configs = *thread_inner.recorder_configs.load_full();
            let setup_progress = Progress::Indeterminate("Setting up recording.");

            // TODO: send the recorder job -> figure out the best way to actually send this.
            // Perhaps instead it should return a message queue with an enumerated Response type?
            let format = configs.format();
            let spec: CaptureSpec = configs.into();

            // Match on the format, send the configs in as an arg to avoid the extra copy.
            match format {
                RibbleRecordingFormat::F32 => thread_inner.run_recorder_loop::<f32>(spec),
                RibbleRecordingFormat::I16 => thread_inner.run_recorder_loop::<i16>(spec),
            }?;

            let message = String::from("Finished recording!");
            let console_message = ConsoleMessage::Status(message);
            Ok(RibbleMessage::Console(console_message))
        });
        worker
    }

    pub(super) fn recorder_running(&self) -> bool {
        self.inner
            .recorder_running
            .load(std::sync::atomic::Ordering::Acquire)
    }

    pub(super) fn read_recorder_configs(&self) -> Arc<RibbleRecordingConfigs> {
        self.inner.recorder_configs.load_full()
    }
    pub(super) fn write_recorder_configs<
        F: FnOnce(RibbleRecordingConfigs) -> RibbleRecordingConfigs,
    >(
        &self,
        update_closure: F,
    ) {
        let confs = *self.inner.recorder_configs.load_full();
        self.inner
            .recorder_configs
            .store(Arc::new(update_closure(confs)));
    }
}

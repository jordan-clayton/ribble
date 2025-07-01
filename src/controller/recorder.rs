use crate::controller::console::ConsoleMessage;
use crate::controller::kernel::EngineKernel;
use crate::controller::progress::Progress;
use crate::controller::writer::WriteRequest;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::errors::RibbleError;
use crate::utils::pcm_f32::PcmF32Convertible;
use crate::utils::recorder_configs::{RibbleRecordingConfigs, RibbleRecordingExportFormat};
use arc_swap::ArcSwap;
use crossbeam::channel::TrySendError;
use ribble_whisper::audio::audio_backend::CaptureSpec;
use ribble_whisper::audio::microphone::MicCapture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::audio::recorder::RecorderSample;
use ribble_whisper::utils::constants::INPUT_BUFFER_CAPACITY;
use ribble_whisper::utils::{get_channel, Sender};
use ribble_whisper::whisper::model::ModelRetriever;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

// TODO: migrate to message queues -> if Inner is no longer relevant, migrate the inner struct to
// the outer one.
struct RecorderEngineState<M: ModelRetriever, E: EngineKernel<Retriever=M>> {
    engine_kernel: Weak<E>,
    recorder_running: Arc<AtomicBool>,
    recorder_configs: ArcSwap<RibbleRecordingConfigs>,
    write_request_handle: Sender<WriteRequest>,
    // TODO: this also needs a visualizer request handle
}

impl<M: ModelRetriever, E: EngineKernel<Retriever=M>> RecorderEngineState<M, E> {
    fn new(write_requester: Sender<WriteRequest>) -> Self {
        let configs = Arc::new(Default::default());
        Self {
            engine_kernel: Weak::new(),
            recorder_running: Arc::new(AtomicBool::new(false)),
            recorder_configs: ArcSwap::from(configs),
            write_request_handle: write_requester,
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

        // TODO: set up the progress engine message queue
        let setup_progress = Progress::new_indeterminate("Setting up recording.");

        let (audio_sender, audio_receiver) = get_channel::<Arc<[T]>>(INPUT_BUFFER_CAPACITY);
        let sink = ArcChannelSink::new(audio_sender);
        let mic = kernel.request_audio_capture(spec, sink)?;
        // TODO: send a write job through a channel -> Send the receiver.
        let (write_sender, write_receiver) = get_channel::<Arc<[T]>>(INPUT_BUFFER_CAPACITY);
        let confirmed_specs = RibbleRecordingConfigs::from_mic_capture(&mic);

        let request = WriteRequest::new(write_receiver, confirmed_specs);

        // Send off the request to write the file
        if self.write_request_handle.send(request).is_err() {
            let ribble_error = RibbleError::Core("Writing engine no longer receiving write requests.".to_string());
            self.recorder_running.store(false, Ordering::Release);
            return Err(ribble_error);
        }


        let sample_rate = mic.sample_rate();
        // TODO: end the setup progress job here and send it.
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

pub(super) struct RecorderEngine<M: ModelRetriever, E: EngineKernel<Retriever=M>> {
    inner: Arc<RecorderEngineState<M, E>>,
}

impl<M: ModelRetriever, E: EngineKernel<Retriever=M>> RecorderEngine<M, E> {
    // TODO: refactor this to take a "BUS"
    pub(super) fn new(write_request_handle: Sender<WriteRequest>) -> Self {
        Self {
            inner: Arc::new(RecorderEngineState::new(write_request_handle)),
        }
    }

    pub(super) fn start_recording(&self) -> RibbleWorkerHandle {
        let thread_inner = Arc::clone(&self.inner);

        let worker = std::thread::spawn(move || {
            let configs = *thread_inner.recorder_configs.load_full();

            let format = configs.format();
            let spec: CaptureSpec = configs.into();

            // Match on the format, send the configs in as an arg to avoid the extra copy.
            match format {
                RibbleRecordingExportFormat::F32 => thread_inner.run_recorder_loop::<f32>(spec),
                RibbleRecordingExportFormat::I16 => thread_inner.run_recorder_loop::<i16>(spec),
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
            .load(Ordering::Acquire)
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

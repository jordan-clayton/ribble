use crate::controller::visualizer::VisualizerSample;
use crate::controller::writer::WriteRequest;
use crate::controller::{
    Bus, ConsoleMessage, Progress, ProgressMessage, RibbleMessage, WorkRequest, UTILITY_QUEUE_SIZE,
};
use crate::utils::errors::RibbleError;
use crate::utils::recorder_configs::RibbleRecordingConfigs;
use arc_swap::ArcSwap;
use crossbeam::channel::TrySendError;
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::microphone::MicCapture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::utils::{get_channel, Sender};
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

struct RecorderEngineState {
    recorder_running: Arc<AtomicBool>,
    recorder_configs: ArcSwap<RibbleRecordingConfigs>,
    progress_message_sender: Sender<ProgressMessage>,
    write_request_sender: Sender<WriteRequest>,
    visualizer_sample_sender: Sender<VisualizerSample>,
}

impl RecorderEngineState {
    fn new(configs: RibbleRecordingConfigs, bus: &Bus) -> Self {
        Self {
            recorder_running: Arc::new(AtomicBool::new(false)),
            recorder_configs: ArcSwap::from(Arc::new(configs)),
            progress_message_sender: bus.progress_message_sender(),
            write_request_sender: bus.write_request_sender(),
            visualizer_sample_sender: bus.visualizer_sample_sender(),
        }
    }

    fn cleanup_remove_progress_job(&self, maybe_id: Option<usize>) {
        if let Some(id) = maybe_id {
            let remove_setup = ProgressMessage::Remove { job_id: id };
            if let Err(e) = self.progress_message_sender.send(remove_setup) {
                log::warn!(
                    "Progress engine closed. Cannot send recorder remove setup message.\nError source: {:#?}",
                    e.source()
                );
            }
        }
    }

    fn run_recorder_loop<A>(&self, audio_backend: &A) -> Result<(), RibbleError>
    where
        A: AudioBackend<ArcChannelSink<f32>> + Send + Sync,
    {
        let setup_progress = Progress::new_indeterminate("Setting up recording.");
        let (id_sender, id_receiver) = get_channel(1);

        let progress_setup_message = ProgressMessage::Request {
            job: setup_progress,
            id_return_sender: id_sender,
        };

        if let Err(e) = self.progress_message_sender.send(progress_setup_message) {
            log::warn!(
                "Progress engine closed. Cannot send recorder start setup message.\nError source: {:#?}",
                e.source()
            );
        }

        let setup_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(e) => {
                log::warn!(
                    "Progress engine did not complete rendezvous, cannot get job id.\nError source: {:#?}",
                    e.source()
                );
                None
            }
        };

        let (audio_sender, audio_receiver) = get_channel::<Arc<[f32]>>(UTILITY_QUEUE_SIZE);
        let sink = ArcChannelSink::new(audio_sender);
        let spec = (*self.recorder_configs.load_full()).into();
        let mic = audio_backend.open_capture(spec, sink).or_else(|e| {
            self.cleanup_remove_progress_job(setup_id);
            Err(e)
        })?;
        let (write_sender, write_receiver) = get_channel::<Arc<[f32]>>(UTILITY_QUEUE_SIZE);
        let confirmed_specs = RibbleRecordingConfigs::from_mic_capture(&mic);

        let request = WriteRequest::new(write_receiver, confirmed_specs);

        // Send off the request to write the file
        if self.write_request_sender.send(request).is_err() {
            let ribble_error =
                RibbleError::Core("Writing engine no longer receiving write requests.".to_string());
            self.recorder_running.store(false, Ordering::Release);
            return Err(ribble_error);
        }

        let sample_rate = mic.sample_rate();

        if let Some(id) = setup_id {
            let remove_message = ProgressMessage::Remove { job_id: id };
            if let Err(e) = self.progress_message_sender.send(remove_message) {
                log::warn!(
                    "Progress engine closed, cannot send recorder remove message.\nError source: {:#?}",
                    e.source()
                );
            }
        }

        mic.play();
        while self.recorder_running.load(Ordering::Acquire) {
            match audio_receiver.recv() {
                Ok(audio) => {
                    if let Err(TrySendError::Disconnected(_)) =
                        write_sender.try_send(Arc::clone(&audio))
                    {
                        self.recorder_running.store(false, Ordering::Release);
                    }

                    let next_visualizer_sample =
                        VisualizerSample::new(Arc::clone(&audio), sample_rate as f64);
                    if let Err(e) = self.visualizer_sample_sender.try_send(next_visualizer_sample) {
                        log::warn!(
                            "Cannot send new visualizer samples, channel closed or too small.\n\
                            Error: {}\n\
                            Error source: {:#?}",
                            &e,
                            e.source()
                        );
                    }
                }
                // This only happens if the audio callback has been dropped and there's no more
                // audio to process.
                Err(_) => {
                    log::info!(
                        "Audio callback closed, causing sending channel drop. Recording should be complete."
                    );
                    self.recorder_running.store(false, Ordering::Release);
                }
            }
        }
        mic.pause();
        audio_backend.close_capture(mic);

        Ok(())
    }
}

pub(super) struct RecorderEngine {
    inner: Arc<RecorderEngineState>,
    work_request_sender: Sender<WorkRequest>,
}

impl RecorderEngine {
    pub(super) fn new(configs: RibbleRecordingConfigs, bus: &Bus) -> Self {
        Self {
            inner: Arc::new(RecorderEngineState::new(configs, bus)),
            work_request_sender: bus.work_request_sender(),
        }
    }

    pub(super) fn start_recording<A>(&self, audio_backend: Arc<A>)
    where
        A: AudioBackend<ArcChannelSink<f32>> + Send + Sync + 'static,
    {
        // Set the state flag so that the UI can update.
        self.inner.recorder_running.store(true, Ordering::Release);

        let thread_inner = Arc::clone(&self.inner);
        // Spawn a (long job) thread and send it off to the worker to join it.
        let worker = std::thread::spawn(move || {
            thread_inner.run_recorder_loop(audio_backend.as_ref())?;
            let message = String::from("Finished recording!");
            let console_message = ConsoleMessage::Status(message);
            Ok(RibbleMessage::Console(console_message))
        });

        let request = WorkRequest::Long(worker);
        if let Err(e) = self.work_request_sender.try_send(request) {
            log::warn!(
                "Error sending recording work request. Channel may be too small, or worker engine missing.\n\
                Error: {e}\n\
                Error source: {:#?}",
                e.source()
            );
        }
    }

    pub(super) fn stop_recording(&self) {
        self.inner.recorder_running.store(false, Ordering::Release);
    }

    pub(super) fn recorder_running(&self) -> bool {
        self.inner.recorder_running.load(Ordering::Acquire)
    }

    pub(super) fn read_recorder_configs(&self) -> Arc<RibbleRecordingConfigs> {
        self.inner.recorder_configs.load_full()
    }

    pub(super) fn write_recorder_configs(&self, recorder_configs: RibbleRecordingConfigs) {
        self.inner
            .recorder_configs
            .store(Arc::new(recorder_configs));
    }
}

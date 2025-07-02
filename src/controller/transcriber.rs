use crate::controller::Bus;
use crate::controller::RibbleMessage;
use crate::controller::console::ConsoleMessage;
use crate::controller::progress::{Progress, ProgressMessage};
use crate::controller::visualizer::VisualizerSample;
use crate::controller::worker::WorkRequest;
use crate::controller::writer::WriteRequest;
use crate::utils::dc_block::DCBlock;
use crate::utils::errors::RibbleError;
use crate::utils::recorder_configs::{
    RibbleChannels, RibblePeriod, RibbleRecordingConfigs, RibbleSampleRate,
};
use crate::utils::vad_configs::RibbleVAD;
use crate::utils::vad_configs::VadConfigs;
use arc_swap::ArcSwap;
use atomic_enum::atomic_enum;
use crossbeam::channel::TrySendError;
use crossbeam::scope;
use crossbeam::thread::{Scope, ScopedJoinHandle};
use ribble_whisper::audio::audio_backend::{AudioBackend, CaptureSpec};
use ribble_whisper::audio::audio_ring_buffer::AudioRingBuffer;
use ribble_whisper::audio::loading::{audio_file_num_frames, load_normalized_audio_file};
use ribble_whisper::audio::microphone::MicCapture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::audio::{AudioChannelConfiguration, WhisperAudioSample};
use ribble_whisper::transcriber::offline_transcriber::OfflineTranscriberBuilder;
use ribble_whisper::transcriber::realtime_transcriber::RealtimeTranscriberBuilder;
use ribble_whisper::transcriber::{
    CallbackTranscriber, Transcriber, TranscriptionSnapshot, WhisperCallbacks,
    WhisperControlPhrase, WhisperOutput, redirect_whisper_logging_to_hooks,
};
use ribble_whisper::utils::callback::{
    ShortCircuitRibbleWhisperCallback, StaticRibbleWhisperCallback,
};
use ribble_whisper::utils::constants::{INPUT_BUFFER_CAPACITY, WHISPER_SAMPLE_RATE};
use ribble_whisper::utils::{Receiver, Sender, get_channel};
use ribble_whisper::whisper::configs::WhisperRealtimeConfigs;
use ribble_whisper::whisper::model::ModelRetriever;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use strum::{AsRefStr, EnumIter, EnumString};

// Minimal: progress bar only (fastest)
// Progressive: progress bar + snapshotting when new segments are decoded.
// TODO: this might be better to move somewhere else.
#[atomic_enum]
#[repr(C)]
#[derive(Default, PartialEq, Eq, EnumIter, EnumString, AsRefStr)]
pub(crate) enum OfflineTranscriberFeedback {
    #[default]
    Minimal = 0,
    Progressive,
}

struct TranscriberEngineState {
    transcription_configs: ArcSwap<WhisperRealtimeConfigs>,
    vad_configs: ArcSwap<VadConfigs>,
    realtime_running: Arc<AtomicBool>,
    offline_running: Arc<AtomicBool>,
    offline_transcriber_feedback: Arc<AtomicOfflineTranscriberFeedback>,
    feedback_callback_rate: Arc<AtomicU64>,
    current_snapshot: ArcSwap<TranscriptionSnapshot>,
    current_control_phrase: ArcSwap<WhisperControlPhrase>,
    progress_message_sender: Sender<ProgressMessage>,
    visualizer_sample_sender: Sender<VisualizerSample>,
    write_request_sender: Sender<WriteRequest>,
}

impl TranscriberEngineState {
    // Right now, this default feedback rate is at 1.5s
    // NOTE: do some testing, look at implementing dynamic throttling.
    // Possibly expose in the UI as a parameter.
    const DEFAULT_FEEDBACK_RATE_MILLIS: u64 = 1500;
    fn new(
        configs: WhisperRealtimeConfigs,
        v_configs: VadConfigs,
        feedback_type: OfflineTranscriberFeedback,
        bus: Bus,
    ) -> Self {
        let transcription_configs = ArcSwap::new(Arc::new(configs));
        let vad_configs = ArcSwap::new(Arc::new(v_configs));
        let realtime_running = Arc::new(AtomicBool::new(false));
        let offline_running = Arc::new(AtomicBool::new(false));
        let transcriber_feedback = AtomicOfflineTranscriberFeedback::new(feedback_type);
        let offline_transcriber_feedback = Arc::new(transcriber_feedback);
        let feedback_callback_rate = Arc::new(AtomicU64::new(Self::DEFAULT_FEEDBACK_RATE_MILLIS));
        let current_snapshot = ArcSwap::new(Arc::new(TranscriptionSnapshot::default()));
        let current_control_phrase = ArcSwap::new(Arc::new(WhisperControlPhrase::default()));
        Self {
            transcription_configs,
            vad_configs,
            realtime_running,
            offline_running,
            offline_transcriber_feedback,
            feedback_callback_rate,
            current_snapshot,
            current_control_phrase,
            progress_message_sender: bus.progress_message_sender(),
            visualizer_sample_sender: bus.visualizer_sample_sender(),
            write_request_sender: bus.write_request_sender(),
        }
    }

    // TODO: move the realtime/offline transcription here.

    // NOTE: this could probably just spawn the thread here and return it?
    // If choosing to do that, keep the interface consistent and change RecorderEngine.
    fn run_realtime_transcription<M, A>(
        &self,
        audio_backend: &A,
        shared_model_retriever: Arc<M>,
    ) -> Result<RibbleMessage, RibbleError>
    where
        M: ModelRetriever,
        A: AudioBackend<ArcChannelSink<f32>>,
    {
        self.clear_transcription();

        // Send a progress job so the UI can be updated.
        let setup_progress = Progress::new_indeterminate("Setting up real-time transcription.");
        let (id_sender, id_receiver) = get_channel(1);
        let setup_progress_message = ProgressMessage::Request {
            job: setup_progress,
            // TODO: rename this; it's confusing.
            source: id_sender,
        };

        if self
            .progress_message_sender
            .send(setup_progress_message)
            .is_err()
        {
            todo!("LOGGING");
        }

        let setup_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(_) => {
                todo!("LOGGING");
                None
            }
        };

        let audio_ring_buffer = AudioRingBuffer::<f32>::default();
        // Audio fanout channels
        let (audio_sender, audio_receiver) = get_channel::<Arc<[f32]>>(INPUT_BUFFER_CAPACITY);

        // Transcription channels
        let (text_sender, text_receiver) = get_channel(INPUT_BUFFER_CAPACITY);
        // TODO: instead, pass these in an argument
        let vad_configs = *self.vad_configs.load_full().clone();

        let vad = vad_configs.build_vad().or_else(|e| {
            // TODO: this should be factored out into another method.
            if let Some(id) = setup_id {
                let remove_setup = ProgressMessage::Remove { job_id: id };
                if self.progress_message_sender.send(remove_setup).is_err() {
                    todo!("LOGGING");
                }
            }
            Err(e)
        })?;

        // Set up the mic capture -> the default is "Whisper-ready"
        let spec = CaptureSpec::default();
        let sink = ArcChannelSink::new(audio_sender);

        let mic = audio_backend.open_capture(spec, sink).or_else(|e| {
            if let Some(id) = setup_id {
                let remove_setup = ProgressMessage::Remove { job_id: id };
                if self.progress_message_sender.send(remove_setup).is_err() {
                    todo!("LOGGING");
                }
            }
            Err(e)
        })?;

        // Get a copy of the configs
        let configs = *self.transcription_configs.load_full();

        let (transcriber, transcriber_handle) = RealtimeTranscriberBuilder::new()
            .with_configs(configs)
            .with_audio_buffer(&audio_ring_buffer)
            .with_output_sender(text_sender)
            .with_voice_activity_detector(vad)
            .with_shared_model_retriever(shared_model_retriever)
            .build()
            .or_else(|e| {
                if let Some(id) = setup_id {
                    let remove_setup = ProgressMessage::Remove { job_id: id };
                    if self.progress_message_sender.send(remove_setup).is_err() {
                        todo!("LOGGING");
                    }
                }
                Err(e)
            })?;

        let recording_expected_available = AtomicBool::new(true);

        let result = scope(|s| {
            // Audio Fanout
            let a_thread_run_transcription = Arc::clone(&self.realtime_running);
            // Transcriber runner flag
            let t_thread_run_transcription = Arc::clone(&self.realtime_running);

            // Disable stderr/stdout
            redirect_whisper_logging_to_hooks();
            // Close the "Setup" progress job
            if let Some(id) = setup_id {
                let remove_setup = ProgressMessage::Remove { job_id: id };
                if self.progress_message_sender.send(remove_setup).is_err() {
                    todo!("LOGGING");
                }
            }

            // Get the confirmed recording specs for the writer.
            let confirmed_recording_configs = RibbleRecordingConfigs::from_mic_capture(&mic);

            debug_assert_ne!(
                confirmed_recording_configs.sample_rate(),
                RibbleSampleRate::Auto
            );
            debug_assert_ne!(
                confirmed_recording_configs.num_channels(),
                RibbleChannels::Auto
            );

            debug_assert_ne!(confirmed_recording_configs.period(), RibblePeriod::Auto);

            // Start a write job
            let (write_sender, write_receiver) = get_channel::<Arc<[f32]>>(INPUT_BUFFER_CAPACITY);
            let write_request = WriteRequest::new(write_receiver, confirmed_recording_configs);

            // Start the mic feed
            mic.play();

            // Spawn the scoped worker threads
            let _audio_fanout_thread = s.spawn(move || {
                while a_thread_run_transcription.load(Ordering::Acquire) {
                    match audio_receiver.recv() {
                        Ok(audio) => {
                            if !transcriber_handle.ready() {
                                continue;
                            }

                            // Run a cheap DCBlock filter before pushing to the ring buffer
                            let mut dc_block =
                                DCBlock::new().with_sample_rate(WHISPER_SAMPLE_RATE as f32);

                            let filtered =
                                audio.iter().copied().map(|f| dc_block.process(f)).collect();

                            // Write into the ringbuffer
                            audio_ring_buffer.push_audio(&filtered);
                            // Fan the data out.

                            // If the write thread panics, the receiver will be deallocated.
                            if let Err(TrySendError::Disconnected(_)) =
                                write_sender.try_send(Arc::clone(&audio))
                            {
                                recording_expected_available.store(false, Ordering::Release);
                                todo!("LOGGING");
                                a_thread_run_transcription.store(false, Ordering::Release);
                            }

                            // TODO: have to use self, but just use this to stub.
                            // Send out data to the VisualizerEngine
                            let visualizer_sample =
                                VisualizerSample::new(Arc::clone(&audio), WHISPER_SAMPLE_RATE);

                            if self
                                .visualizer_sample_sender
                                .send(visualizer_sample)
                                .is_err()
                            {
                                todo!("LOGGING");
                            }
                        }
                        Err(_) => a_thread_run_transcription.store(false, Ordering::Release),
                    }
                }
            });

            let transcription_thread =
                s.spawn(move || transcriber.process_audio(t_thread_run_transcription));

            // For updating the inner transcription
            let _print_thread = self.print_loop(s, text_receiver, TranscriptionType::Realtime);

            // This -should- properly coerce into RibbleAppError, but it might need to be explicit.
            let res = transcription_thread
                .join()
                .map_err(|e| RibbleError::ThreadPanic(format!("{:?}", e)))??;
            Ok(res)
        })??;

        mic.pause();

        self.finalize_transcription(result);

        // Send a message to the console before returning the result.
        // If the writer thread somehow crashed, then there is unlikely to be a recording
        // available.
        let message = if recording_expected_available.load(Ordering::Acquire) {
            String::from(
                "Finished real-time transcription! Recording available for offline re-transcription.",
            )
        } else {
            String::from(
                "Finished real-time transcription! Recording unavailable for offline re-transcription.",
            )
        };

        let console_message = ConsoleMessage::Status(message);
        Ok(RibbleMessage::Console(console_message))
    }

    fn run_offline_transcription<M: ModelRetriever>(
        &self,
        audio_file_path: Path,
        shared_model_retriever: Arc<M>,
    ) -> Result<RibbleMessage, RibbleError> {
        // Send a progress job so the UI can be updated.
        let setup_progress = Progress::new_indeterminate("Setting up offline transcription.");

        let (id_sender, id_receiver) = get_channel(1);
        let setup_progress_message = ProgressMessage::Request {
            job: setup_progress,
            // TODO: rename this; it's confusing.
            source: id_sender,
        };

        if self
            .progress_message_sender
            .send(setup_progress_message)
            .is_err()
        {
            todo!("LOGGING");
        }

        let setup_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(_) => {
                todo!("LOGGING");
                None
            }
        };

        let vad_configs = self.vad_configs.load_full();
        // Unpack the VAD settings and build a VAD if the user wants to optimize.
        let vad = if !vad_configs.use_vad_offline() {
            let vad = vad_configs.build_vad().or_else(|e| {
                if let Some(id) = setup_id {
                    let remove_setup = ProgressMessage::Remove { job_id: id };
                    if self.progress_message_sender.send(remove_setup).is_err() {
                        todo!("LOGGING");
                    }
                }
                Err(e)
            })?;

            Some(vad)
        } else {
            None
        };

        // Get the configs -> dereference and consume into WhisperConfigsV2 to discard unused
        // realtime parameters.
        let configs = *self
            .transcription_configs
            .load_full()
            .into_whisper_v2_configs();

        let n_frames = audio_file_num_frames(&audio_file_path).or_else(|e| {
            if let Some(id) = setup_id {
                let remove_setup = ProgressMessage::Remove { job_id: id };
                if self.progress_message_sender.send(remove_setup).is_err() {
                    todo!("LOGGING");
                }
            }
            Err(e)
        })?;

        let load_audio_progress = Progress::new_determinate("Loading audio", n_frames);
        let (id_sender, id_receiver) = get_channel(1);

        let load_audio_progress_message = ProgressMessage::Request {
            job: load_audio_progress,
            source: id_sender,
        };

        if self
            .progress_message_sender
            .send(load_audio_progress_message)
            .is_err()
        {
            todo!("LOGGING");
        }

        let load_audio_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(_) => {
                todo!("LOGGING");
                None
            }
        };

        let load_audio_callback = move |progress: usize| {
            if let Some(id) = load_audio_id {
                let update_progress_message = ProgressMessage::Increment {
                    job_id: id,
                    delta: progress as u64,
                };
                if self
                    .progress_message_sender
                    .send(update_progress_message)
                    .is_err()
                {
                    todo!("LOGGING.");
                }
            }
        };

        // Load the audio file.
        let loaded_audio = load_normalized_audio_file(audio_file_path, Some(load_audio_callback))
            .or_else(|e| {
            if let Some(id) = setup_id {
                let remove_message = ProgressMessage::Remove { job_id: id };
                if self.progress_message_sender.send(remove_message).is_err() {
                    todo!("LOGGING");
                }
            }
            if let Some(id) = load_audio_id {
                let remove_message = ProgressMessage::Remove { job_id: id };
                if self.progress_message_sender.send(remove_message).is_err() {
                    todo!("LOGGING");
                }
            }

            Err(e)
        })?;

        let audio = match loaded_audio {
            WhisperAudioSample::F32(audio) => {
                let mut dc_block = DCBlock::new().with_sample_rate(WHISPER_SAMPLE_RATE as f32);

                let filtered = audio.iter().copied().map(|f| dc_block.process(f)).collect();
                WhisperAudioSample::F32(Arc::from(filtered))
            }
            WhisperAudioSample::I16(_) => {
                unreachable!("Loading normalized for whisper should never return integer audio.")
            }
        };

        if let Some(id) = load_audio_id {
            let remove_message = ProgressMessage::Remove { job_id: id };
            if self.progress_message_sender.send(remove_message).is_err() {
                todo!("LOGGING");
            }
        }

        let (sender, receiver) = get_channel(INPUT_BUFFER_CAPACITY);
        let mut offline_transcriber_builder = OfflineTranscriberBuilder::<RibbleVAD, _>::new()
            .with_configs(configs)
            .with_audio(audio)
            .with_channel_configurations(AudioChannelConfiguration::Mono)
            .with_shared_model_retriever(shared_model_retriever);

        if let Some(ribble_vad) = vad {
            offline_transcriber_builder =
                offline_transcriber_builder.with_voice_activity_detector(ribble_vad);
        }

        let offline_transcriber = offline_transcriber_builder.build().or_else(|e| {
            if let Some(id) = setup_id {
                let remove_message = ProgressMessage::Remove { job_id: id };
                if self.progress_message_sender.send(remove_message).is_err() {
                    todo!("LOGGING");
                }
            }
            Err(e)
        })?;

        let run_transcription = Arc::clone(&self.offline_running);
        // Remove the setup progress job.
        if let Some(id) = setup_id {
            let remove_message = ProgressMessage::Remove { job_id: id };
            if self.progress_message_sender.send(remove_message).is_err() {
                todo!("LOGGING");
            }
        }

        let result = scope(|s| {
            // Set up a progress callback for transcription
            // As far as I can tell, this should be in integer percent
            let transcription_progress = Progress::new_determinate("Transcribing", 100);
            let (id_sender, id_receiver) = get_channel(1);
            let transcription_progress_message = ProgressMessage::Request {
                job: transcription_progress,
                source: id_sender,
            };

            if self
                .progress_message_sender
                .send(transcription_progress_message)
                .is_err()
            {
                todo!("LOGGING");
            }

            let transcription_id = match id_receiver.recv() {
                Ok(id) => Some(id),
                Err(_) => {
                    todo!("LOGGING");
                    None
                }
            };

            let transcription_closure = move |percent: i32| {
                if let Some(id) = transcription_id {
                    let update_progress_message = ProgressMessage::Increment {
                        job_id: id,
                        delta: percent as u64,
                    };
                    if self
                        .progress_message_sender
                        .send(update_progress_message)
                        .is_err()
                    {
                        todo!("LOGGING.");
                    }
                }
            };

            let transcription_callback =
                Some(StaticRibbleWhisperCallback::new(transcription_closure));

            let segment_closure = move |snapshot| {
                // Take the snapshot into an Arc (for swapping in the print loop).
                let a_snap = Arc::new(snapshot);
                // Send it off to the print loop -> This shouldn't likely ever have an issue with
                // a full queue--whisper dwarfs the callback, giving the print loop time to receive.
                // If it fails due to a dropped receiver, this sender should -also- be gone.
                let _ = sender.try_send(WhisperOutput::TranscriptionSnapshot(a_snap));
            };

            let callback_offline_feedback = Arc::clone(&self.offline_transcriber_feedback);

            let mut last = Instant::now();

            let segment_short_circuit_closure = move || {
                let offline_feedback = callback_offline_feedback.load(Ordering::Acquire);
                if matches!(offline_feedback, OfflineTranscriberFeedback::Minimal) {
                    return false;
                }

                let now = Instant::now();
                let diff = now.duration_since(last);
                let limit = self.feedback_callback_rate.load(Ordering::Acquire) as u128;

                if diff.as_millis() >= limit {
                    last = now;
                    true
                } else {
                    false
                }
            };

            let segment_callback = Some(ShortCircuitRibbleWhisperCallback::new(
                segment_short_circuit_closure,
                segment_closure,
            ));

            // With how the new_segment callback works, it's not possible atm to have an
            // early escape mechanism to avoid the heavy computation
            // (It's also unlikely to be exposed in the UI when the transcription is running)
            let whisper_callbacks = WhisperCallbacks {
                progress: transcription_callback,
                new_segment: segment_callback,
            };

            // TODO: restructure this such that all setup happens before the scope block.
            // Build the transcriber -in- the transcription thread itself and match

            let transcription_thread = s.spawn(move || {
                let res = offline_transcriber
                    .process_with_callbacks(run_transcription, whisper_callbacks);

                if let Some(id) = transcription_id {
                    let remove_progress_message = ProgressMessage::Remove { job_id: id };
                    if self
                        .progress_message_sender
                        .send(remove_progress_message)
                        .is_err()
                    {
                        todo!("LOGGING");
                    }
                }
                res
            });

            let _print_thread = self.print_loop(s, receiver, TranscriptionType::Offline);

            // If the transcription thread panicked, it's because of an uncaught whisper error
            // -- and thus the progress job most likely needs to be removed.
            // It is also most likely that if this job is still in the buffer, it's the only
            // one in the buffer, (or it did get removed and the buffer is empty).
            // Test this, but if either prove to be true, then it shouldn't matter wrt remove_progress_job.
            let res = transcription_thread.join().or_else(|e| {
                if let Some(id) = transcription_id {
                    let remove_progress_message = ProgressMessage::Remove { job_id: id };
                    if self
                        .progress_message_sender
                        .send(remove_progress_message)
                        .is_err()
                    {
                        todo!("LOGGING");
                    }
                }
                let error = RibbleError::ThreadPanic(format!("{:?}", e));
                Err(error)
            })??;
            Ok(res)
        })??;

        self.finalize_transcription(result);

        // Finalize by preparing a status message for the console.
        let message = format!("Finished transcribing: {}!", audio_file_path);
        let console_message = ConsoleMessage::Status(message);
        Ok(RibbleMessage::Console(console_message))
    }

    fn finalize_transcription(&self, final_transcription: String) {
        let confirmed_transcription = Arc::new(final_transcription);
        let snapshot = TranscriptionSnapshot::new(confirmed_transcription, Default::default());
        self.current_snapshot.store(Arc::new(snapshot));
        // TODO: swap this to idle once implemented.
        self.current_control_phrase
            .store(Arc::new(WhisperControlPhrase::GettingReady));
    }

    fn clear_transcription(&self) {
        self.current_snapshot
            .store(Arc::new(TranscriptionSnapshot::default()));
        // TODO: implement default in whisper_rs -> needs an IDLE.
        self.current_control_phrase
            .store(Arc::new(WhisperControlPhrase::default()))
    }

    fn print_loop<'scope>(
        &self,
        scope: &'scope Scope,
        text_receiver: Receiver<WhisperOutput>,
        transcription_type: TranscriptionType,
    ) -> ScopedJoinHandle<'scope, ()> {
        let running = match transcription_type {
            TranscriptionType::Realtime => Arc::clone(&self.realtime_running),
            TranscriptionType::Offline => Arc::clone(&self.offline_running),
        };
        scope.spawn(move || {
            while running.load(Ordering::Acquire) {
                match text_receiver.recv() {
                    Ok(output) => match output {
                        WhisperOutput::TranscriptionSnapshot(snapshot) => {
                            self.current_snapshot.store(Arc::clone(&snapshot));
                        }
                        WhisperOutput::ControlPhrase(control) => {
                            self.current_control_phrase.store(Arc::new(control));
                        }
                    },
                    Err(_) => {
                        running.store(false, Ordering::Release);
                    }
                }
            }
        })
    }
}

// This is not strictly necessary, but it's more explicit than a boolean.
// TODO: move this to the module level.
pub(super) enum TranscriptionType {
    Realtime,
    Offline,
}

pub(super) struct TranscriberEngine {
    inner: Arc<TranscriberEngineState>,
    work_request_sender: Sender<WorkRequest>,
}

// TODO: Refactor this -> move the bulk of the logic to the inner state struct
// Only spawn the threads in the TranscriberEngine
//
// ALSO: move to message queues, don't monomorphize, move ModelRetriever<M> to the call site of
// run_realtime and run_offline by passing IN the model retriever to the method.
// (Ie. take in the "BUS")
// ALSO TWICE: pass the VadConfigs IN instead of trying to get
// ALSO THRICE: Use only one set of configurations -> change the offline configurations mutator to
// just modify the whisper half & also change from FnOnce callback to just take the new clone
//
// ALSO FOURCE: Instead of using ? on operations that can return an error, call or_else() first
// and send a progress message to remove the old jobs.
// the progress queue to send a remove request instead of relying on the callback.
impl TranscriberEngine {
    // These get passed in upon construction; they should be serialized separately.
    pub(super) fn new(
        transcription_configs: WhisperRealtimeConfigs,
        vad_configs: VadConfigs,
        feedback_type: OfflineTranscriberFeedback,
        bus: Bus,
    ) -> Self {
        let inner = Arc::new(TranscriberEngineState::new(
            transcription_configs,
            vad_configs,
            feedback_type,
            bus,
        ));
        Self {
            inner,
            work_request_sender: bus.work_request_sender(),
        }
    }

    // TODO: remove if unused.
    pub(super) fn transcriber_running(&self) -> bool {
        self.realtime_running() || self.offline_running()
    }
    pub(super) fn realtime_running(&self) -> bool {
        self.inner.realtime_running.load(Ordering::Acquire)
    }
    pub(super) fn offline_running(&self) -> bool {
        self.inner.offline_running.load(Ordering::Acquire)
    }

    pub(super) fn stop_realtime(&self) {
        self.inner.realtime_running.store(false, Ordering::Release);
    }
    pub(super) fn stop_offline(&self) {
        self.inner.offline_running.store(false, Ordering::Release);
    }

    pub(super) fn read_transcription_configs(&self) -> Arc<WhisperRealtimeConfigs> {
        self.inner.transcription_configs.load_full()
    }
    pub(super) fn read_vad_configs(&self) -> Arc<VadConfigs> {
        self.inner.vad_configs.load_full()
    }

    pub(super) fn write_transcription_configs(&self, configs: WhisperRealtimeConfigs) {
        self.inner.transcription_configs.store(Arc::new(configs));
    }
    pub(super) fn write_vad_configs(&self, vad_configs: VadConfigs) {
        self.inner.vad_configs.store(Arc::new(vad_configs));
    }

    pub(super) fn read_transcription_snapshot(&self) -> Arc<TranscriptionSnapshot> {
        self.inner.current_snapshot.load_full()
    }
    pub(super) fn try_read_latest_control(&self) -> Arc<WhisperControlPhrase> {
        self.inner.current_control_phrase.load_full()
    }

    pub(super) fn start_realtime_transcription<M, A>(
        &self,
        audio_backend: &A,
        shared_model_retriever: Arc<M>,
    ) where
        M: ModelRetriever,
        A: AudioBackend<ArcChannelSink<f32>>,
    {
        // Set the flag that the realtime runner is running so that the UI can update.
        self.inner.realtime_running.store(true, Ordering::Release);
        let thread_inner = Arc::clone(&self.inner);
        let worker = std::thread::spawn(move || {
            thread_inner.run_realtime_transcription(audio_backend, shared_model_retriever)
        });

        let work_request = WorkRequest::Long(worker);
        if self.work_request_sender.send(work_request).is_err() {
            todo!("LOGGING");
        }
    }

    pub(super) fn start_offline_transcription<M: ModelRetriever>(
        &self,
        audio_file_path: Path,
        shared_model_retriever: Arc<M>,
    ) {
        // Set the flag that the offline runner is running so that the UI can update.
        self.inner.offline_running.store(true, Ordering::Release);

        let thread_inner = Arc::clone(&self.inner);

        // Set up the worker.
        let worker = std::thread::spawn(move || {
            thread_inner.run_offline_transcription(audio_file_path, shared_model_retriever)
        });

        // Send off the request
        let work_request = WorkRequest::Long(worker);
        if self.work_request_sender.send(work_request).is_err() {
            todo!("LOGGING");
        }
    }
}

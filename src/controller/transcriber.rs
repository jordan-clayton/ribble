use crate::controller::console::ConsoleMessage;
use crate::controller::kernel::{EngineKernel, TranscriberMethod};
use crate::controller::progress::Progress;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::audio_analysis::bandpass_filter;
use crate::utils::constants::APP_ID;
use crate::utils::errors::RibbleError;
use crate::utils::file_mgmt::{get_tmp_file_writer, write_audio_sample};
use arc_swap::ArcSwap;
use atomic_enum::atomic_enum;
use crossbeam::channel::{Receiver, TrySendError};
use crossbeam::scope;
use crossbeam::thread::{Scope, ScopedJoinHandle};
use hound::{SampleFormat, WavSpec};
use ribble_whisper::audio::AudioChannelConfiguration;
use ribble_whisper::audio::audio_backend::CaptureSpec;
use ribble_whisper::audio::audio_ring_buffer::AudioRingBuffer;
use ribble_whisper::audio::loading::{audio_file_num_frames, load_normalized_audio_file};
use ribble_whisper::audio::microphone::MicCapture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::transcriber::offline_transcriber::OfflineTranscriberBuilder;
use ribble_whisper::transcriber::realtime_transcriber::RealtimeTranscriberBuilder;
use ribble_whisper::transcriber::vad::Silero;
use ribble_whisper::transcriber::{
    CallbackTranscriber, Transcriber, TranscriptionSnapshot, WhisperCallbacks,
    WhisperControlPhrase, WhisperOutput, redirect_whisper_logging_to_hooks,
};
use ribble_whisper::utils::callback::StaticRibbleWhisperCallback;
use ribble_whisper::utils::constants::{INPUT_BUFFER_CAPACITY, WHISPER_SAMPLE_RATE};
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::get_channel;
use ribble_whisper::whisper::configs::{WhisperConfigsV2, WhisperRealtimeConfigs};
use ribble_whisper::whisper::model::ModelRetriever;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

// Minimal: progress bar only (fastest)
// Progressive: progress bar + snapshotting when new segments are decoded.
// TODO: this might be better to move somewhere else.
#[atomic_enum]
#[repr(C)]
pub(crate) enum OfflineTranscriberFeedback {
    Minimal,
    Progressive,
}

struct TranscriberState<M: ModelRetriever, E: EngineKernel<Retriever = M>> {
    // Handle for interfacing with the kernel
    engine_kernel: Weak<E>,
    // TODO: refactor configs impl in ribble_whisper.
    realtime_configs: ArcSwap<WhisperRealtimeConfigs>,
    offline_configs: ArcSwap<WhisperConfigsV2>,
    realtime_running: Arc<AtomicBool>,
    offline_running: Arc<AtomicBool>,
    offline_transcriber_feedback: Arc<AtomicOfflineTranscriberFeedback>,
    current_snapshot: ArcSwap<TranscriptionSnapshot>,
    current_control_phrase: ArcSwap<WhisperControlPhrase>,
}

impl<M: ModelRetriever, E: EngineKernel<Retriever = M>> TranscriberState<M, E> {
    fn clear_transcription(&self) {
        self.current_snapshot
            .store(Arc::new(TranscriptionSnapshot::default()));
        // TODO: implement default in whisper_rs -> needs an IDLE.
        self.current_control_phrase
            .store(Arc::new(WhisperControlPhrase::GettingReady))
    }
}

// This is not strictly necessary, but it's more explicit than a boolean.
enum TranscriptionType {
    Realtime,
    Offline,
}
impl<M: ModelRetriever, E: EngineKernel<Retriever = M>> TranscriberState<M, E> {
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

pub(super) struct TranscriberEngine<M: ModelRetriever, E: EngineKernel<Retriever = M>> {
    inner: Arc<TranscriberState<M, E>>,
}

impl<M: ModelRetriever, E: EngineKernel<Retriever = M>> TranscriberEngine<M, E> {
    // These get passed in upon construction; they should be serialized separately.
    pub(super) fn new(
        realtime_configs: WhisperRealtimeConfigs,
        offline_configs: WhisperConfigsV2,
        // TODO: Revisit this once the controller implementation is further along ->
        // might be better to just default this and set later in the initialization step.
        // Might also be able to get away with its non-atomic equivalent re: serialization.
        offline_transcriber_feedback: AtomicOfflineTranscriberFeedback,
    ) -> Self {
        let realtime_running = Arc::new(AtomicBool::new(false));
        let offline_running = Arc::new(AtomicBool::new(false));
        let realtime_configs = ArcSwap::new(Arc::new(realtime_configs));
        let offline_configs = ArcSwap::new(Arc::new(offline_configs));
        let current_snapshot = ArcSwap::new(Arc::new(TranscriptionSnapshot::default()));
        let current_control_phrase = ArcSwap::new(Arc::new(WhisperControlPhrase::GettingReady));
        let offline_transcriber_feedback = Arc::new(offline_transcriber_feedback);

        let inner = Arc::new(TranscriberState {
            engine_kernel: Weak::new(),
            realtime_configs,
            offline_configs,
            realtime_running,
            offline_running,
            current_snapshot,
            current_control_phrase,
            offline_transcriber_feedback,
        });
        Self { inner }
    }

    pub(super) fn set_engine_kernel(&self, kernel_handle: Weak<E>) {
        *self.inner.engine_kernel = kernel_handle;
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

    // These should be reserved for places where it's okay to block (e.g. serialization);
    // Otherwise try-read and accept the option.
    pub(super) fn read_realtime_configs(&self) -> Arc<WhisperRealtimeConfigs> {
        self.inner.realtime_configs.load().clone()
    }
    pub(super) fn read_offline_configs(&self) -> Arc<WhisperConfigsV2> {
        self.inner.offline_configs.load().clone()
    }

    // Takes a closure that updates the realtime configs via builder.
    // It should be possible to just send in builder methods by name.
    pub(super) fn write_realtime_configs<
        F: FnOnce(WhisperRealtimeConfigs) -> WhisperRealtimeConfigs,
    >(
        &self,
        update_closure: F,
    ) {
        let confs = (*self.inner.realtime_configs.load().clone()).clone();
        self.inner
            .realtime_configs
            .store(Arc::new(update_closure(confs)));
    }
    // Takes a closure that updates the offline configs via builder.
    // It should be possible to just send in builder methods by name.
    pub(super) fn write_offline_configs<F: FnOnce(WhisperConfigsV2) -> WhisperConfigsV2>(
        &self,
        update_closure: F,
    ) {
        let confs = (*self.inner.offline_configs.load().clone()).clone();
        self.inner
            .offline_configs
            .store(Arc::new(update_closure(confs)));
    }

    pub(super) fn read_transcription_snapshot(&self) -> Arc<TranscriptionSnapshot> {
        self.inner.current_snapshot.load().clone()
    }
    pub(super) fn try_read_latest_control(&self) -> Arc<WhisperControlPhrase> {
        self.inner.current_control_phrase.load().clone()
    }

    pub(super) fn finalize_transcription(&self, final_transcription: String) {
        let snapshot = TranscriptionSnapshot::new(final_transcription, Default::default());
        self.inner.current_snapshot.store(Arc::new(snapshot));
        // TODO: swap this to idle once implemented.
        self.inner
            .current_control_phrase
            .store(Arc::new(WhisperControlPhrase::GettingReady));
    }

    // TODO: determine how to handle if this thread somehow panics re: removing progress jobs.
    // It might be wise to split jobs out by type, e.g. RealTime, Offline, Download, etc.
    // OR: it could be sufficient to just clear the ProgressEngine... -> not sure.
    // Instead: perhaps it might be of interest to manually implement From<RibbleWhisperError> for RibbleError
    // or some similar mechanism.
    // Such that it contains all required cleanup information PLUS the error.
    // TODO: refactor once errors finished so that the type is correct.
    pub(super) fn run_realtime(&self) -> RibbleWorkerHandle {
        let thread_inner = Arc::clone(&self.inner);
        // Set the flag that the realtime runner is running so that the UI can update.
        thread_inner.realtime_running.store(true, Ordering::Release);

        // Set up the worker.
        let worker = std::thread::spawn(move || {
            // Clear the transcription buffers
            thread_inner.clear_transcription();
            // Get a handle to the kernel; this will error out if the kernel's not set properly.
            // The worker thread that joins this will blast the information to the console (provided it also has a kernel).
            // The error will eventually propagate until it's handled/logged and returned in main.
            let kernel = thread_inner.engine_kernel.upgrade().ok_or(
                RibbleError::Core("Kernel not yet attached to TranscriberEngine.".to_string())
                    .into(),
            )?;

            // Send a progress job so the UI can be updated.
            let setup_progress = Progress::indeterminate("Setting up real-time transcription.");

            let setup_id = kernel.add_progress_job(setup_progress);

            // TODO: read the bandpassconfigurations (should have a "filtering").
            // Get the bandpass configs from the kernel.
            // Set up f_central as an optional f32 based on "filtering" flag
            let f_central: Option<f32> = None;

            // Inner state handles for threads
            let print_thread_inner = Arc::clone(&thread_inner);

            let audio_ring_buffer = AudioRingBuffer::<f32>::default();
            // Audio fanout channels
            let (audio_sender, audio_receiver) = get_channel::<Arc<[f32]>>(INPUT_BUFFER_CAPACITY);
            // TODO: make a writer request to the kernel and send this receiver.
            let (write_sender, write_receiver) = get_channel::<Arc<[f32]>>(INPUT_BUFFER_CAPACITY);

            // Transcription channels
            let (text_sender, text_receiver) = get_channel(INPUT_BUFFER_CAPACITY);
            let vad_configs = kernel.get_vad_configs(TranscriberMethod::Realtime);
            let vad = vad_configs.build_vad().map_err(|e| {
                let cleanup_kernel = Arc::clone(&kernel);
                e.into()
                    .with_cleanup(cleanup_kernel.remove_progress_job(setup_id))
            })?;

            // Set up the mic capture
            let spec = CaptureSpec::default();
            let sink = ArcChannelSink::new(audio_sender);

            let mic = kernel.request_audio_capture(spec, sink)?;

            // Get a copy of the configs
            let configs = (*thread_inner.realtime_configs.load().clone()).clone();

            // Set the model retriever
            let model_retriever = kernel.get_model_retriever();

            let (transcriber, transcriber_handle) = RealtimeTranscriberBuilder::new()
                .with_configs(configs)
                .with_audio_buffer(&audio_ring_buffer)
                .with_output_sender(text_sender)
                .with_voice_activity_detector(vad)
                .with_shared_model_retriever(model_retriever)
                .build()
                .map_err(|e| {
                    let cleanup_kernel = Arc::clone(&kernel);
                    e.into()
                        .with_cleanup(cleanup_kernel.remove_progress_job(setup_id))
                })?;

            // Send a clone of the kernel to the scoped thread
            let scoped_kernel = Arc::clone(&kernel);

            let result = scope(|s| {
                // Audio Fanout
                let a_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);
                // Transcriber runner flag
                let t_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);
                // Write thread runner flag
                let w_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);

                // Disable stderr/stdout
                redirect_whisper_logging_to_hooks();

                // Close the "Setup" progress job
                scoped_kernel.remove_progress_job(setup_id);
                // Get a kernel handle for the audio thread
                let audio_scoped_kernel = Arc::clone(&scoped_kernel);
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

                                // If filtering is toggled on, copy and run the bandpass over the filter
                                // and re-wrap in an Arc<[F32]>.
                                let audio = if f_central.is_some() {
                                    let mut to_filter = *audio;
                                    bandpass_filter(
                                        &mut to_filter,
                                        WHISPER_SAMPLE_RATE as f32,
                                        f_central.unwrap(),
                                    );
                                    let new_audio = Arc::new(to_filter);
                                    new_audio
                                } else {
                                    audio
                                };

                                // Write into the ringbuffer
                                audio_ring_buffer.push_audio(&audio);
                                // Fan data out to the write thread and the FFT thread

                                // TODO: Come back to this once the writer impl has been refactored to request from the kernel.
                                // This should definitely still write to the message queue,
                                // so this probably won't need to change,
                                // but it might have some subtleties that need to be addressed.
                                // If the writer thread receiver is disconnected, the transcription is
                                // most likely finished and state needs to be synchronized.
                                // If it's full, just skip and accept there might be hiccups in the
                                // write thread.
                                if let Err(TrySendError::Disconnected(_)) =
                                    write_sender.try_send(Arc::clone(&audio))
                                {
                                    a_thread_run_transcription.store(false, Ordering::Release);
                                }

                                // Pass data to the visualizer through the kernel.
                                audio_scoped_kernel.update_visualizer_data(
                                    Arc::clone(&audio),
                                    WHISPER_SAMPLE_RATE,
                                );
                            }
                            Err(_) => a_thread_run_transcription.store(false, Ordering::Release),
                        }
                    }

                    // In case any of the other threads are hanging, pump the channel with a 0-length
                    // slice as a signal to "stop"
                    // TODO: refactor this--it's not ideal. This should instead, be an enum (AudioSignal::Stop vs AudioSignal::Data(Arc<[T]>));
                    // The print thread will always terminate because the transcriber will
                    // deallocate the channel once its transcription has stopped, so no need for monads/sentinels
                    // TODO twice: These are most likely unnecessary and are an artifact from the old implementation
                    // I've brought them here from the old implementation and they should be unnecessary -->
                    // The old implementation had pre-allocated message queues (which is unnecessary); these shouldn't suffer that problem and are likely to return a TrySendError.
                    let empty_buffer = Arc::from(&[][..]);
                    let _ = write_sender.try_send(Arc::clone(&empty_buffer));
                });
                let transcription_thread =
                    s.spawn(move || transcriber.process_audio(t_thread_run_transcription));

                // For updating the inner transcription
                let _print_thread =
                    print_thread_inner.print_loop(s, text_receiver, TranscriptionType::Realtime);
                // TODO IMPORTANT: Get this out of here and move it to where it makes sense to live.
                // Like, the kernel, lol.
                // TODO: once kernel implementation is completed (i.e. this code has been properly migrated), make the appropriate kernel request.
                let _write_thread = s.spawn(move || {
                    // TODO: twice: this utility method needs to be retired and replaced with something better.
                    // This should also probably be abstracted away a bit more -> Hound really should be buried.
                    // Also, there need to be hooks for errors so these can bubble-up a bit more easily.
                    let spec = WavSpec {
                        channels: 1,
                        sample_rate: WHISPER_SAMPLE_RATE as u32,
                        bits_per_sample: 32,
                        sample_format: SampleFormat::Float,
                    };

                    // TODO: this will likely need some cleanup.
                    let data_dir =
                        eframe::storage_dir(APP_ID).ok_or(RibbleWhisperError::ParameterError(
                            "Data directory is not properly set.".to_string(),
                        ));

                    if let Err(_e) = data_dir.as_ref() {
                        // TODO: logging.
                        // TODO: possibly write a message to the console -- it's not a fatal error if the temporary file doesn't get written to.
                        return;
                    }

                    let data_dir = data_dir.unwrap();
                    let writer = get_tmp_file_writer(data_dir.as_path(), &spec).map_err(|e| {
                        // TODO: swap this out once errors are refactored.
                        RibbleWhisperError::Unknown(format!(
                            "Failed to set up temporary audio file writer: {:?}",
                            e
                        ))
                    });
                    if let Err(_e) = writer.as_ref() {
                        // TODO: logging.
                        // TODO: possibly write a message to the console -- it's not a fatal error if the temporary file doesn't get written to.
                        return;
                    }

                    let mut writer = writer.unwrap();

                    while w_thread_run_transcription.load(Ordering::Acquire) {
                        match write_receiver.recv() {
                            Ok(audio) => {
                                // TODO: this function also will need rewriting.
                                write_audio_sample(&audio, &mut writer, None::<fn(usize)>);
                            }
                            Err(_) => {
                                w_thread_run_transcription.store(false, Ordering::Release);
                            }
                        }
                    }

                    let _ = writer.finalize();
                    // NOTE TO SELF: It might be of interest to include an extra log here when testing out the new implementation.
                });

                // This -should- properly coerce into RibbleAppError, but it might need to be explicit.
                let res = transcription_thread
                    .join()
                    .map_err(|e| RibbleError::ThreadPanic(format!("{:?}", e)))??;
                Ok(RibbleMessage::TranscriptionOutput(res))
            })?;
            mic.pause();

            // Send an info message to the console to alert the user that the transcription loop
            // has completed.
            let message = String::from("Finished real-time transcription.");
            let console_message = ConsoleMessage::Status(message);
            kernel.send_console_message(console_message);
            result
        });
        worker
    }

    pub(super) fn run_offline(&self) -> RibbleWorkerHandle {
        let thread_inner = Arc::clone(&self.inner);
        // Set the flag that the offline runner is running so that the UI can update.
        thread_inner.offline_running.store(true, Ordering::Release);

        // Set up the worker.
        let worker = std::thread::spawn(move || {
            // Get a handle to the kernel.
            let kernel = thread_inner.engine_kernel.upgrade().ok_or(
                RibbleError::Core("Kernel not yet attached to TranscriberEngine.".to_string())
                    .into(),
            )?;

            // Send a progress job so the UI can be updated.
            let setup_progress = Progress::indeterminate("Setting up real-time transcription.");

            let setup_id = kernel.add_progress_job(setup_progress);

            let vad_configs = kernel.get_vad_configs(TranscriberMethod::Offline);
            let vad = vad_configs.build_vad().map_err(|e| {
                let cleanup_kernel = Arc::clone(&kernel);
                e.into()
                    .with_cleanup(cleanup_kernel.remove_progress_job(setup_id))
            })?;
            // Get the configs
            let configs = (*thread_inner.offline_configs.load().clone()).clone();
            // Get the audio file path (TODO: figure out how to re-handle the temporary audio file)
            let audio_file_path = kernel
                .get_audio_file_path()
                .ok_or(RibbleWhisperError::ParameterError(
                    "File path not supplied to offline transcriber. This should not happen!"
                        .to_string(),
                ))
                .map_err(|e| {
                    let cleanup_kernel = Arc::clone(&kernel);
                    e.into()
                        .with_cleanup(cleanup_kernel.remove_progress_job(setup_id))
                })?;

            // Prep a handle to the kernel to send to the callback.
            let callback_kernel = Arc::clone(&kernel);
            let n_frames = audio_file_num_frames(&audio_file_path).map_err(|e| {
                let cleanup_kernel = Arc::clone(&kernel);
                e.into()
                    .with_cleanup(cleanup_kernel.remove_progress_job(setup_id))
            })?;

            let load_audio_progress = Progress::determinate("Loading audio", n_frames);
            let load_audio_id = kernel.add_progress_job(load_audio_progress);
            let load_audio_callback = move |progress: usize| {
                callback_kernel.update_progress_job(load_audio_id, progress as u64)
            };

            // Load the audio file.
            let audio = load_normalized_audio_file(audio_file_path, Some(load_audio_callback))
                .map_err(|e| {
                    let cleanup_kernel = Arc::clone(&kernel);
                    e.into().with_cleanup(
                        cleanup_kernel.cleanup_progress_jobs(&[setup_id, load_audio_id]),
                    )
                })?;
            // Remove the progress job now that transcription is finished.
            kernel.remove_progress_job(load_audio_id);
            // Set up the offline_transcriber

            let (sender, receiver) = get_channel(INPUT_BUFFER_CAPACITY);
            // Set the model retriever
            // Since the retriever is dynamic, it has to be wrapped in an adapter for the duration of transcription

            let model_retriever = kernel.get_model_retriever();
            let offline_transcriber = OfflineTranscriberBuilder::<Silero, _>::new()
                .with_configs(configs)
                .with_audio(audio)
                .with_channel_configurations(AudioChannelConfiguration::Mono)
                .with_voice_activity_detector(vad)
                .with_shared_model_retriever(model_retriever)
                .build()
                .map_err(|e| {
                    let cleanup_kernel = Arc::clone(&kernel);
                    e.into()
                        .with_cleanup(cleanup_kernel.remove_progress_job(setup_id))
                })?;

            let scoped_kernel = Arc::clone(&kernel);
            let run_transcription = Arc::clone(&thread_inner.offline_running);
            let print_thread_inner = Arc::clone(&thread_inner);
            // Remove the setup progress job.
            kernel.remove_progress_job(setup_id);
            let result = scope(|s| {
                // Set up a progress callback for transcription
                // As far as I can tell, this should be in integer percent
                let transcription_progress = Progress::determinate("Transcribing", 100);
                let transcription_id = scoped_kernel.add_progress_job(transcription_progress);
                let callback_kernel = Arc::clone(&scoped_kernel);
                let transcription_closure = move |percent: i32| {
                    callback_kernel.update_progress_job(transcription_id, percent as u64);
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

                let feedback = thread_inner
                    .offline_transcriber_feedback
                    .load(Ordering::Acquire);
                let segment_callback = match feedback {
                    OfflineTranscriberFeedback::Minimal => None,
                    OfflineTranscriberFeedback::Progressive => {
                        Some(StaticRibbleWhisperCallback::new(segment_closure))
                    }
                };

                // With how the new_segment callback works, it's not possible atm to have an
                // early escape mechanism to avoid the heavy computation
                // (It's also unlikely to be exposed in the UI when the transcription is running)
                let whisper_callbacks = WhisperCallbacks {
                    progress: transcription_callback,
                    new_segment: segment_callback,
                };

                // Spawn the threads.
                let t_kernel = Arc::clone(&kernel);

                // TODO: restructure this such that all setup happens before the scope block.
                // Build the transcriber -in- the transcription thread itself and match

                let transcription_thread = s.spawn(move || {
                    let res = offline_transcriber
                        .process_with_callbacks(run_transcription, whisper_callbacks);
                    t_kernel.remove_progress_job(transcription_id);
                    res
                });

                // TODO: determine how best to handle this:
                // Atm, there's no way to short-circuit the segment callback in ribble_whisper
                // It would require more complexity to implement this behaviour -> and it's likely
                // unwise to let the user make changes to configurations while transcription is running.
                // This might be the best compromise.
                if let OfflineTranscriberFeedback::Progressive = feedback {
                    let _print_thread =
                        print_thread_inner.print_loop(s, receiver, TranscriptionType::Offline);
                }

                // If the transcription thread panicked, it's because of an uncaught whisper error
                // -- and thus the progress job most likely needs to be removed.
                // It is also most likely that if this job is still in the buffer, it's the only
                // one in the buffer, (or it did get removed and the buffer is empty).
                // Test this, but if either prove to be true, then it shouldn't matter wrt remove_progress_job.
                let res = transcription_thread.join().map_err(|e| {
                    let cleanup_kernel = Arc::clone(&kernel);
                    RibbleError::ThreadPanic(format!("{:?}", e))
                        .into()
                        .with_cleanup(cleanup_kernel.remove_progress_job(transcription_id))
                })??;
                Ok(RibbleMessage::TranscriptionOutput(res))
            })?;
            // Send a message to the console before returning the result.
            let message = format!("Finished transcribing: {}", audio_file_path);
            let console_message = ConsoleMessage::Status(message);
            kernel.send_console_message(console_message);
            result
        });
        worker
    }
}

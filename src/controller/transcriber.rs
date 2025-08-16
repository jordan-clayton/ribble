use crate::controller::VisualizerPacket;
use crate::controller::WriteRequest;
use crate::controller::{
    AtomicOfflineTranscriberFeedback, Bus, ConsoleMessage, OfflineTranscriberFeedback, Progress,
    ProgressMessage, RibbleMessage, WorkRequest, UTILITY_QUEUE_SIZE,
};
use crate::utils::dc_block::DCBlock;
use crate::utils::errors::RibbleError;
use crate::utils::recorder_configs::{
    RibbleChannels, RibblePeriod, RibbleRecordingConfigs, RibbleSampleRate,
};
use crate::utils::vad_configs::{NopVAD, VadConfigs, VadType};
use arc_swap::ArcSwap;
use crossbeam::channel::TrySendError;
use crossbeam::scope;
use ribble_whisper::audio::audio_backend::{AudioBackend, CaptureSpec};
use ribble_whisper::audio::audio_ring_buffer::AudioRingBuffer;
use ribble_whisper::audio::loading::{audio_file_num_frames, load_normalized_audio_file};
use ribble_whisper::audio::microphone::MicCapture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::audio::{AudioChannelConfiguration, WhisperAudioSample};
use ribble_whisper::transcriber::offline_transcriber::OfflineTranscriberBuilder;
use ribble_whisper::transcriber::realtime_transcriber::RealtimeTranscriberBuilder;
use ribble_whisper::transcriber::vad::VAD;
use ribble_whisper::transcriber::{
    redirect_whisper_logging_to_hooks, CallbackTranscriber, Transcriber, TranscriptionSnapshot, WhisperCallbacks,
    WhisperControlPhrase, WhisperOutput, WHISPER_SAMPLE_RATE,
};
use ribble_whisper::utils::callback::{
    ShortCircuitRibbleWhisperCallback, StaticRibbleWhisperCallback,
};
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{get_channel, Sender};
use ribble_whisper::whisper::configs::WhisperRealtimeConfigs;
use ribble_whisper::whisper::model::ModelRetriever;
use std::error::Error;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

struct TranscriberEngineState {
    transcription_configs: ArcSwap<WhisperRealtimeConfigs>,
    vad_configs: ArcSwap<VadConfigs>,
    realtime_running: Arc<AtomicBool>,
    offline_running: Arc<AtomicBool>,
    current_audio_file_path: ArcSwap<Option<PathBuf>>,
    offline_transcriber_feedback: Arc<AtomicOfflineTranscriberFeedback>,
    feedback_callback_rate: Arc<AtomicU64>,
    current_snapshot: ArcSwap<TranscriptionSnapshot>,
    current_control_phrase: ArcSwap<WhisperControlPhrase>,
    progress_message_sender: Sender<ProgressMessage>,
    visualizer_sample_sender: Sender<VisualizerPacket>,
    write_request_sender: Sender<WriteRequest>,
}

impl TranscriberEngineState {
    // At the moment, there isn't a noticeable penalty to 1500ms.
    // There also isn't a significant penalty for 500ms for short transcriptions.
    const DEFAULT_FEEDBACK_RATE_MILLIS: u64 = 1500;
    fn new(
        configs: WhisperRealtimeConfigs,
        v_configs: VadConfigs,
        feedback_type: OfflineTranscriberFeedback,
        bus: &Bus,
    ) -> Self {
        let transcription_configs = ArcSwap::new(Arc::new(configs));
        let vad_configs = ArcSwap::new(Arc::new(v_configs));
        let realtime_running = Arc::new(AtomicBool::new(false));
        let offline_running = Arc::new(AtomicBool::new(false));
        let current_audio_file_path = ArcSwap::new(Arc::new(None));
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
            current_audio_file_path,
            offline_transcriber_feedback,
            feedback_callback_rate,
            current_snapshot,
            current_control_phrase,
            progress_message_sender: bus.progress_message_sender(),
            visualizer_sample_sender: bus.visualizer_sample_sender(),
            write_request_sender: bus.write_request_sender(),
        }
    }

    fn cleanup_remove_progress_job(&self, maybe_id: Option<usize>) {
        if let Some(id) = maybe_id {
            let remove_setup = ProgressMessage::Remove { job_id: id };
            if let Err(e) = self.progress_message_sender.send(remove_setup) {
                log::warn!(
                    "Progress channel closed, cannot send transcriber remove progress message.\n\
                Error source: {:#?}",
                    e.source()
                );
            }
        }
    }

    fn build_vad_run_realtime<M, A>(
        &self,
        audio_backend: &A,
        shared_model_retriever: Arc<M>,
    ) -> Result<RibbleMessage, RibbleError>
    where
        M: ModelRetriever + Send + Sync,
        A: AudioBackend<ArcChannelSink<f32>> + Send + Sync,
    {
        let configs = *self.vad_configs.load_full();
        match configs.vad_type() {
            VadType::Silero | VadType::Auto => {
                let vad = configs.build_silero()?;
                self.run_realtime_transcription(audio_backend, shared_model_retriever, vad)
            }
            VadType::WebRtc => {
                let vad = configs.build_webrtc()?;
                self.run_realtime_transcription(audio_backend, shared_model_retriever, vad)
            }
            VadType::Earshot => {
                let vad = configs.build_earshot()?;
                self.run_realtime_transcription(audio_backend, shared_model_retriever, vad)
            }
        }
    }

    fn run_realtime_transcription<M, A, V>(
        &self,
        audio_backend: &A,
        shared_model_retriever: Arc<M>,
        vad: V,
    ) -> Result<RibbleMessage, RibbleError>
    where
        M: ModelRetriever + Send + Sync,
        A: AudioBackend<ArcChannelSink<f32>> + Send + Sync,
        V: VAD<f32> + Send + Sync,
    {
        self.clear_transcription();

        // Send a progress job so the UI can be updated.
        let setup_progress = Progress::new_indeterminate("Setting up real-time transcription.");
        let (id_sender, id_receiver) = get_channel(1);
        let setup_progress_message = ProgressMessage::Request {
            job: setup_progress,
            id_return_sender: id_sender,
        };

        if let Err(e) = self.progress_message_sender.send(setup_progress_message) {
            log::warn!(
                "Progress channel closed, cannot get id in real-time transcriber setup.\n\
            Error source: {:#?}",
                e.source()
            );
        }

        let setup_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(e) => {
                log::warn!(
                    "Progress engine did not complete rendezvous for setup progress job.\n\
                Error source: {:#?}",
                    e.source()
                );
                None
            }
        };

        let audio_ring_buffer = AudioRingBuffer::<f32>::default();
        // Audio fanout channels
        let (audio_sender, audio_receiver) = get_channel::<Arc<[f32]>>(UTILITY_QUEUE_SIZE);

        // Transcription channels
        let (text_sender, text_receiver) = get_channel(UTILITY_QUEUE_SIZE);
        // Set up the mic capture -> the default is "Whisper-ready"
        let spec = CaptureSpec::default();
        let sink = ArcChannelSink::new(audio_sender);

        let mic = audio_backend.open_capture(spec, sink).inspect_err(|_e| {
            self.cleanup_remove_progress_job(setup_id);
        })?;

        // Get a copy of the configs
        let configs = *self.transcription_configs.load_full();

        let (transcriber, transcriber_handle) = RealtimeTranscriberBuilder::<V, M>::new()
            .with_configs(configs)
            .with_audio_buffer(&audio_ring_buffer)
            .with_output_sender(text_sender)
            .with_voice_activity_detector(vad)
            .with_shared_model_retriever(shared_model_retriever)
            .build()
            .inspect_err(|_e| {
                self.cleanup_remove_progress_job(setup_id);
            })?;

        let recording_expected_available = Arc::new(AtomicBool::new(true));
        let a_thread_recording_expected_available = Arc::clone(&recording_expected_available);
        let p_thread_running = Arc::clone(&self.realtime_running);

        let result = scope(|s| {
            // Audio Fanout
            let a_thread_run_transcription = Arc::clone(&self.realtime_running);
            // Transcriber runner flag
            let t_thread_run_transcription = Arc::clone(&self.realtime_running);

            // Redirect whisper logging to the logger.
            redirect_whisper_logging_to_hooks();
            // Close the "Setup" progress job
            self.cleanup_remove_progress_job(setup_id);
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
            let (write_sender, write_receiver) = get_channel::<Arc<[f32]>>(UTILITY_QUEUE_SIZE);
            let write_request = WriteRequest::new_job(write_receiver, confirmed_recording_configs);
            if let Err(e) = self.write_request_sender.send(write_request) {
                log::warn!(
                    "Writer engine closed, cannot send recording request.\nError source: {:#?}",
                    e.source()
                );
            }

            // Start the mic feed
            mic.play();

            // Spawn the scoped worker threads
            let _audio_fanout_thread = s.spawn(move |_| {
                while a_thread_run_transcription.load(Ordering::Acquire) {
                    match audio_receiver.recv() {
                        Ok(audio) => {
                            if !transcriber_handle.ready() {
                                continue;
                            }

                            // Run a cheap DCBlock filter before pushing to the ring buffer
                            let mut dc_block =
                                DCBlock::new().with_sample_rate(WHISPER_SAMPLE_RATE as f32);

                            let filtered = audio
                                .iter()
                                .copied()
                                .map(|f| dc_block.process(f))
                                .collect::<Vec<_>>();

                            // Write into the ringbuffer
                            audio_ring_buffer.push_audio(&filtered);
                            // Fan the data out.

                            // If the write thread panics, the receiver will be deallocated.
                            // Stop the transcription because the recording is gone.
                            if let Err(TrySendError::Disconnected(_)) =
                                write_sender.try_send(Arc::clone(&audio))
                            {
                                a_thread_recording_expected_available
                                    .store(false, Ordering::Release);
                                a_thread_run_transcription.store(false, Ordering::Release);

                                // If it's because of a panic, the panic will be propagated from the writer
                                // to the UI.
                                // It could be the case that the writer thread just finished early,
                                // and this thread just needs to finish.
                                let warning = "Writer thread disconnected during transcription loop.";
                                log::warn!("{warning}");
                            }

                            let visualizer_sample =
                                VisualizerPacket::new(Arc::clone(&audio), WHISPER_SAMPLE_RATE);

                            if let Err(e) = self
                                .visualizer_sample_sender
                                .try_send(visualizer_sample)
                            {
                                log::warn!("Failed to send data to visualizer engine, channel closed or too small.\n\
                                Error: {}\n\
                                Error source: {:#?}", &e, e.source());
                            }
                        }
                        Err(_) => a_thread_run_transcription.store(false, Ordering::Release),
                    }
                }
            });

            let transcription_thread =
                s.spawn(move |_| transcriber.process_audio(t_thread_run_transcription));

            // For updating the inner transcription
            // It's easiest to just duplicate the logic across transcription impls; otherwise it
            // becomes a huge lifetime headache.
            let _print_thread = s.spawn(move |_| {
                while p_thread_running.load(Ordering::Acquire) {
                    match text_receiver.recv() {
                        Ok(output) => match output {
                            WhisperOutput::TranscriptionSnapshot(snapshot) => {
                                self.current_snapshot.store(Arc::clone(&snapshot));
                            }

                            WhisperOutput::ControlPhrase(control) => {
                                #[cfg(debug_assertions)]{
                                    self.current_control_phrase.store(Arc::new(control));
                                }

                                // Filter out all "Debug" control phrases in release mode.
                                #[cfg(not(debug_assertions))]
                                {
                                    match &control {
                                        WhisperControlPhrase::Debug(..) => {}
                                        _ => {
                                            self.current_control_phrase.store(Arc::new(control));
                                        }
                                    }
                                }
                            }
                        },
                        Err(_) => {
                            p_thread_running.store(false, Ordering::Release);
                        }
                    }
                }
            });

            // This -should- properly coerce into RibbleAppError, but it might need to be explicit.
            transcription_thread
                .join()
                .unwrap_or_else(|e| {
                    // Wrap the error in a RibbleWhisper "Unknown" to satisfy the type constraints
                    // of the join.
                    let err = RibbleWhisperError::Unknown(format!("{e:?}"));
                    Err(err)
                })
                .map_err(|e| {
                    if matches!(e, RibbleWhisperError::Unknown(_)) {
                        // Since the format string is auto-appended, remove the "Unknown Error "
                        // prefix to make things a little easier to read.
                        RibbleError::ThreadPanic(e.to_string().replace("Unknown Error ", ""))
                    } else {
                        e.into()
                    }
                })
        })
            // Since the type is opaque here (scope return), it's not entirely known as to what the error is.
            // The easiest thing to do here is to wrap it in a "ThreadPanic", as even if the exit is
            // somewhat graceful, an error has forced the transcriber to stop early.
            .map_err(|e| RibbleError::ThreadPanic(format!("{e:?}")));

        mic.pause();
        // Send the device back to be closed
        // Since SDL AudioDevices can only be dropped on the main thread, this needs to be sent
        // back to be dropped.
        //
        // Until a different/better backend solution is written, this will have to do.
        // NOTE: THE COMPILER MAY FIND THIS TO BE A USE-AFTER-MOVE
        // It shouldn't be -> the early return only happens if there is an error.
        audio_backend.close_capture(mic);

        // Unwrap the result -after- closing the microphone capture.
        let result = result??;

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

    fn build_vad_run_offline<M>(&self, shared_model_retriever: Arc<M>) -> Result<RibbleMessage, RibbleError>
    where
        M: ModelRetriever + Sync + Send,
    {
        let configs = *self.vad_configs.load_full();
        if configs.use_vad_offline() {
            match configs.vad_type() {
                VadType::Silero | VadType::Auto => {
                    let vad = configs.build_silero()?;
                    self.run_offline_transcription(shared_model_retriever, Some(vad))
                }
                VadType::WebRtc => {
                    let vad = configs.build_webrtc()?;
                    self.run_offline_transcription(shared_model_retriever, Some(vad))
                }
                VadType::Earshot => {
                    let vad = configs.build_earshot()?;
                    self.run_offline_transcription(shared_model_retriever, Some(vad))
                }
            }
        } else {
            self.run_offline_transcription(shared_model_retriever, None::<NopVAD>)
        }
    }

    fn run_offline_transcription<M, V>(
        &self,
        shared_model_retriever: Arc<M>,
        vad: Option<V>,
    ) -> Result<RibbleMessage, RibbleError>
    where
        M: ModelRetriever + Sync + Send,
        V: VAD<f32> + Send + Sync,
    {
        // Clear the previous transcription
        self.clear_transcription();

        // Unpack the audio.
        let audio_path = self.current_audio_file_path.load_full();

        let audio_file_path = if let Some(path) = audio_path.as_ref() {
            Ok(path.clone())
        } else {
            Err(RibbleError::Core("Audio file path not loaded.".to_string()))
        }?;

        // Send a progress job so the UI can be updated.
        let setup_progress = Progress::new_indeterminate("Setting up offline transcription.");

        let (id_sender, id_receiver) = get_channel(1);
        let setup_progress_message = ProgressMessage::Request {
            job: setup_progress,
            id_return_sender: id_sender,
        };

        if let Err(e) = self.progress_message_sender.send(setup_progress_message) {
            log::warn!(
                "Progress engine closed, cannot send offline setup job.\n\
            Error source: {:#?}",
                e.source()
            );
        }

        let setup_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(e) => {
                log::warn!(
                    "Progress engine did not complete setup rendezvous.\n\
                Error source: {:#?}",
                    e.source()
                );
                None
            }
        };

        // Get the configs -> dereference and consume into WhisperConfigsV2 to discard unused
        // realtime parameters.
        let configs = (*self.transcription_configs.load_full()).into_whisper_v2_configs();

        let n_frames = audio_file_num_frames(audio_file_path.as_path()).inspect_err(|_e| {
            self.cleanup_remove_progress_job(setup_id);
        })?;

        let load_audio_progress = Progress::new_determinate("Loading audio", n_frames);
        let (id_sender, id_receiver) = get_channel(1);

        let load_audio_progress_message = ProgressMessage::Request {
            job: load_audio_progress,
            id_return_sender: id_sender,
        };

        if let Err(e) = self
            .progress_message_sender
            .send(load_audio_progress_message)
        {
            log::warn!(
                "Progress engine closed, cannot send offline load audio job.\n
                Error source: {:#?}",
                e.source()
            );
        }

        let load_audio_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(e) => {
                log::warn!(
                    "Progress engine did not complete load audio rendezvous.\n
                    Error source: {:#?}",
                    e.source()
                );
                None
            }
        };

        let load_audio_callback = move |progress: usize| {
            if let Some(id) = load_audio_id {
                let update_progress_message = ProgressMessage::Increment {
                    job_id: id,
                    delta: progress as u64,
                };
                if let Err(e) = self.progress_message_sender.send(update_progress_message) {
                    log::warn!(
                        "Progress channel closed, cannot send transcriber increment progress message.\n\
                    Error source: {:#?}",
                        e.source()
                    );
                }
            }
        };

        // Load the audio file.
        let loaded_audio =
            load_normalized_audio_file(audio_file_path.as_path(), Some(load_audio_callback))
                .inspect_err(|_e| {
                    self.cleanup_remove_progress_job(setup_id);
                    self.cleanup_remove_progress_job(load_audio_id);
                })?;

        let audio = match loaded_audio {
            WhisperAudioSample::F32(audio) => {
                let mut dc_block = DCBlock::new().with_sample_rate(WHISPER_SAMPLE_RATE as f32);

                let filtered = audio
                    .iter()
                    .copied()
                    .map(|f| dc_block.process(f))
                    .collect::<Vec<_>>();
                WhisperAudioSample::F32(Arc::from(filtered))
            }
            WhisperAudioSample::I16(_) => {
                unreachable!("Loading normalized for whisper should never return integer audio.")
            }
        };

        self.cleanup_remove_progress_job(load_audio_id);

        let (sender, receiver) = get_channel(UTILITY_QUEUE_SIZE);
        let mut offline_transcriber_builder = OfflineTranscriberBuilder::<V, M>::new()
            .with_configs(configs)
            .with_audio(audio)
            .with_channel_configurations(AudioChannelConfiguration::Mono)
            .with_shared_model_retriever(shared_model_retriever);

        if let Some(ribble_vad) = vad {
            offline_transcriber_builder =
                offline_transcriber_builder.with_voice_activity_detector(ribble_vad);
        }

        let offline_transcriber = offline_transcriber_builder.build().inspect_err(|_e| {
            self.cleanup_remove_progress_job(setup_id);
        })?;

        let run_transcription = Arc::clone(&self.offline_running);
        // Remove the setup progress job.
        self.cleanup_remove_progress_job(setup_id);

        let p_thread_running = Arc::clone(&self.offline_running);

        let result = scope(|s| {
            // Set up a progress callback for transcription
            // As far as I can tell, this should be in integer percent
            let transcription_progress = Progress::new_determinate("Transcribing", 100);
            let (id_sender, id_receiver) = get_channel(1);
            let transcription_progress_message = ProgressMessage::Request {
                job: transcription_progress,
                id_return_sender: id_sender,
            };

            if let Err(e) = self
                .progress_message_sender
                .send(transcription_progress_message)
            {
                log::warn!(
                    "Progress engine closed, cannot send transcription progress job.\n
                    Error source: {:#?}",
                    e.source()
                );
            }

            let transcription_id = match id_receiver.recv() {
                Ok(id) => Some(id),
                Err(e) => {
                    log::warn!(
                        "Progress engine did not complete transcription rendezvous.\n
                        Error source: {:#?}",
                        e.source()
                    );
                    None
                }
            };

            // Since this closure has to outlive static, the sender has to be cloned and the method
            // can't be used.
            let progress_sender = self.progress_message_sender.clone();

            let transcription_closure = move |percent: i32| {
                if let Some(id) = transcription_id {
                    let progress_message = ProgressMessage::Set {
                        job_id: id,
                        pos: percent as u64,
                    };

                    if let Err(e) = progress_sender.try_send(progress_message) {
                        log::warn!("Failed to send progress updates, channel is either closed or too small.\n\
                        Error: {}\n\
                        Error source: {:#?}", &e, e.source());
                    }
                }
            };

            let transcription_callback =
                Some(StaticRibbleWhisperCallback::new(transcription_closure));

            let segment_closure = move |snapshot| {
                // Take the snapshot into an Arc (for swapping in the print loop).
                let a_snap = Arc::new(snapshot);
                if let Err(e) = sender.try_send(WhisperOutput::TranscriptionSnapshot(a_snap)) {
                    log::warn!("Cannot send segment transcription snapshot.\n\
                        Error: {}\n\
                        Error source: {:#?}", &e, e.source());
                }
            };

            // Since the callbacks require static lifetime, copy the inner atomics and pass to the
            // closure instead of self.
            let callback_offline_feedback = Arc::clone(&self.offline_transcriber_feedback);
            let feedback_callback_rate = Arc::clone(&self.feedback_callback_rate);

            let mut last = Instant::now();

            let segment_short_circuit_closure = move || {
                let offline_feedback = callback_offline_feedback.load(Ordering::Acquire);
                if matches!(offline_feedback, OfflineTranscriberFeedback::Minimal) {
                    return false;
                }

                let now = Instant::now();
                let diff = now.duration_since(last);
                let limit = feedback_callback_rate.load(Ordering::Acquire) as u128;

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

            let transcription_thread = s.spawn(move |_| {
                let res = offline_transcriber
                    .process_with_callbacks(run_transcription, whisper_callbacks);
                self.cleanup_remove_progress_job(transcription_id);
                res
            });

            // NOTE: It's easier to just duplicate the code, rather than try to factor this into a
            // method.
            // Lifetime issues are a major pain point and I just don't want to have to deal with
            // them.
            let _print_thread = s.spawn(move |_| {
                while p_thread_running.load(Ordering::Acquire) {
                    match receiver.recv() {
                        Ok(output) => match output {
                            WhisperOutput::TranscriptionSnapshot(snapshot) => {
                                self.current_snapshot.store(Arc::clone(&snapshot));
                            }
                            WhisperOutput::ControlPhrase(control) => {
                                #[cfg(debug_assertions)]{
                                    self.current_control_phrase.store(Arc::new(control));
                                }

                                // Filter out all "Debug" control phrases in release mode.
                                #[cfg(not(debug_assertions))]
                                {
                                    match &control {
                                        WhisperControlPhrase::Debug(..) => {}
                                        _ => {
                                            self.current_control_phrase.store(Arc::new(control));
                                        }
                                    }
                                }
                            }
                        },
                        Err(_) => {
                            p_thread_running.store(false, Ordering::Release);
                        }
                    }
                }
            });

            // If the transcription thread panicked, it's because of an uncaught whisper error
            // -- and thus the progress job most likely needs to be removed.
            // It is also most likely that if this job is still in the buffer, it's the only
            // one in the buffer, (or it did get removed and the buffer is empty).
            // Test this, but if either prove to be true, then it shouldn't matter wrt remove_progress_job.
            transcription_thread
                .join()
                .unwrap_or_else(|e| {
                    self.cleanup_remove_progress_job(transcription_id);
                    let error = RibbleWhisperError::Unknown(format!("{e:?}"));
                    Err(error)
                })
                .map_err(|e| {
                    if matches!(e, RibbleWhisperError::Unknown(_)) {
                        // Remove the prefix from the "Unknown" error to replace it with a
                        // ThreadPanic.
                        RibbleError::ThreadPanic(e.to_string().replace("Unknown Error ", ""))
                    } else {
                        e.into()
                    }
                })
        })
            // NOTE: the type of this is opaque due to the scope return.
            // It is most likely to be a ThreadPanic (ThreadPanic), due to locally scoped threads.
            // If this is particularly obtrusive, look at trying to deduplicate.
            .map_err(|e| RibbleError::ThreadPanic(format!("{e:?}")))??;

        self.finalize_transcription(result);

        // Finalize by preparing a status message for the console.
        let message = format!("Finished transcribing: {}!", audio_file_path.display());
        let console_message = ConsoleMessage::Status(message);
        Ok(RibbleMessage::Console(console_message))
    }

    fn finalize_transcription(&self, final_transcription: String) {
        let confirmed_transcription = Arc::new(final_transcription);
        let snapshot = TranscriptionSnapshot::new(confirmed_transcription, Default::default());
        self.current_snapshot.store(Arc::new(snapshot));
        self.current_control_phrase
            .store(Arc::new(WhisperControlPhrase::default()));
    }

    fn clear_transcription(&self) {
        self.current_snapshot
            .store(Arc::new(TranscriptionSnapshot::default()));
        self.current_control_phrase
            .store(Arc::new(WhisperControlPhrase::default()))
    }

    fn save_transcription(&self, out_path: PathBuf) -> Result<RibbleMessage, RibbleError> {
        // Create a file for writing.
        let file = File::create(out_path.as_path())?;
        let mut bufwriter = BufWriter::new(file);
        // Join the transcription
        let full_transcription = self
            .current_snapshot
            .load_full()
            .as_ref()
            .clone()
            .into_string()
            .into_bytes();

        bufwriter.write_all(&full_transcription)?;

        let console_message =
            ConsoleMessage::Status(format!("Transcription saved to: {}!", out_path.display()));

        let ribble_message = RibbleMessage::Console(console_message);
        Ok(ribble_message)
    }
}

pub(super) struct TranscriberEngine {
    inner: Arc<TranscriberEngineState>,
    work_request_sender: Sender<WorkRequest>,
}

impl TranscriberEngine {
    // These get passed in upon construction; they should be serialized separately.
    pub(super) fn new(
        transcription_configs: WhisperRealtimeConfigs,
        vad_configs: VadConfigs,
        feedback_type: OfflineTranscriberFeedback,
        bus: &Bus,
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
    pub(super) fn read_offline_transcriber_feedback(&self) -> OfflineTranscriberFeedback {
        self.inner
            .offline_transcriber_feedback
            .load(Ordering::Acquire)
    }
    pub(super) fn write_offline_transcriber_feedback(
        &self,
        new_feedback: OfflineTranscriberFeedback,
    ) {
        self.inner
            .offline_transcriber_feedback
            .store(new_feedback, Ordering::Release);
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
    pub(super) fn read_latest_control_phrase(&self) -> Arc<WhisperControlPhrase> {
        self.inner.current_control_phrase.load_full()
    }

    fn update_current_audio_file_path(&self, path: Option<PathBuf>) {
        let new_path = Arc::new(path);
        self.inner.current_audio_file_path.swap(new_path);
    }

    pub(super) fn set_current_audio_file_path(&self, path: PathBuf) {
        self.update_current_audio_file_path(Some(path));
    }
    pub(super) fn clear_current_audio_file_path(&self) {
        self.update_current_audio_file_path(None);
    }

    pub(super) fn read_current_audio_file_path(&self) -> Arc<Option<PathBuf>> {
        self.inner.current_audio_file_path.load_full()
    }

    pub(super) fn start_realtime_transcription<M, A>(
        &self,
        audio_backend: Arc<A>,
        shared_model_retriever: Arc<M>,
    ) where
        M: ModelRetriever + Send + Sync + 'static,
        A: AudioBackend<ArcChannelSink<f32>> + Send + Sync + 'static,
    {
        // Set the flag that the realtime runner is running so that the UI can update.
        self.inner.realtime_running.store(true, Ordering::Release);
        let thread_inner = Arc::clone(&self.inner);
        let worker = std::thread::spawn(move || {
            thread_inner.build_vad_run_realtime(audio_backend.as_ref(), shared_model_retriever)
        });

        let work_request = WorkRequest::Long(worker);
        if let Err(e) = self.work_request_sender.try_send(work_request) {
            log::warn!(
                "Cannot send real-time transcription request, channel is too small or closed.\n\
            Error: {}\n\
                Error source: {:#?}",
                &e,
                e.source()
            );
        }
    }

    pub(super) fn start_offline_transcription<M>(&self, shared_model_retriever: Arc<M>)
    where
        M: ModelRetriever + Send + Sync + 'static,
    {
        // Set the flag that the offline runner is running so that the UI can update.
        self.inner.offline_running.store(true, Ordering::Release);

        let thread_inner = Arc::clone(&self.inner);

        // Set up the worker.
        let worker = std::thread::spawn(move || {
            thread_inner.build_vad_run_offline(shared_model_retriever)
        });

        // Send off the request
        let work_request = WorkRequest::Long(worker);
        if let Err(e) = self.work_request_sender.try_send(work_request) {
            log::warn!(
                "Cannot send offline transcription request, channel is too small or closed.\n\
            Error: {}\n\
                Error source: {:#?}",
                &e,
                e.source()
            );
        }
    }

    pub(super) fn save_transcription(&self, out_path: PathBuf) {
        let thread_inner = Arc::clone(&self.inner);
        let worker = std::thread::spawn(move || thread_inner.save_transcription(out_path));

        let work_request = WorkRequest::Short(worker);
        if let Err(e) = self.work_request_sender.try_send(work_request) {
            log::warn!(
                "Cannot send save request, channel is too small or closed.\n\
            Error: {}\n\
                Error source: {:#?}",
                &e,
                e.source()
            );
        }
    }
}

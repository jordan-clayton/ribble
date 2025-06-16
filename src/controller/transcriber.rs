use crate::controller::kernel::EngineKernel;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::audio_analysis::{
    AnalysisType, bandpass_filter, frequency_analysis, normalized_waveform, power_analysis,
};
use crate::utils::constants::{APP_ID, NUM_BUCKETS};
use crate::utils::file_mgmt::{get_tmp_file_writer, write_audio_sample};
use crate::utils::progress::Progress;
use crossbeam::channel::TrySendError;
use crossbeam::scope;
use hound::{SampleFormat, WavSpec};
use parking_lot::{Mutex, RwLock};
use ribble_whisper::audio::AudioChannelConfiguration;
use ribble_whisper::audio::audio_ring_buffer::AudioRingBuffer;
use ribble_whisper::audio::loading::load_normalized_audio_file;
use ribble_whisper::audio::microphone::{FanoutMicCapture, MicCapture};
use ribble_whisper::audio::recorder::UseArc;
use ribble_whisper::transcriber::offline_transcriber::OfflineTranscriberBuilder;
use ribble_whisper::transcriber::realtime_transcriber::RealtimeTranscriberBuilder;
use ribble_whisper::transcriber::vad::Silero;
use ribble_whisper::transcriber::{
    CallbackTranscriber, Transcriber, WhisperCallbacks, WhisperControlPhrase, WhisperOutput,
    redirect_whisper_logging_to_hooks,
};
use ribble_whisper::utils::callback::StaticProgressCallback;
use ribble_whisper::utils::constants::{INPUT_BUFFER_CAPACITY, WHISPER_SAMPLE_RATE};
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::get_channel;
use ribble_whisper::whisper::configs::{WhisperConfigsV2, WhisperRealtimeConfigs};
use std::ops::DerefMut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};

struct TranscriberState {
    // At the moment, going with a kernel model to avoid the overhead of an event loop.
    // This handle lets the TranscriberEngine ask for resources from the kernel and report data to it.
    engine_kernel: Weak<dyn EngineKernel>,
    realtime_configs: RwLock<WhisperRealtimeConfigs>,
    offline_configs: RwLock<WhisperConfigsV2>,
    realtime_running: Arc<AtomicBool>,
    offline_running: Arc<AtomicBool>,
    current_transcription: Mutex<String>,
    current_segments: Mutex<Vec<String>>,
    current_control_phrase: Mutex<WhisperControlPhrase>, // TODO: latest_control_phrase + Accessors; needs to be mutex/rwlock protected
}

impl TranscriberState {
    fn clear_transcription(&self) {
        let current = self.current_transcription.lock();
        *current = String::new();
        drop(current);
        let segments = self.current_segments.lock();
        *segments = vec![];
        drop(segments);
        let control = self.current_control_phrase.lock();
        *control = WhisperControlPhrase::GettingReady;
        drop(control);
    }
}

pub struct TranscriberEngine {
    inner: Arc<TranscriberState>,
}

impl TranscriberEngine {
    // These get passed in upon construction; they should be serialized separately.
    pub(crate) fn new(
        realtime_configs: WhisperRealtimeConfigs,
        offline_configs: WhisperConfigsV2,
    ) -> Self {
        let realtime_running = Arc::new(AtomicBool::new(false));
        let offline_running = Arc::new(AtomicBool::new(false));
        let realtime_configs = RwLock::new(realtime_configs);
        let offline_configs = RwLock::new(offline_configs);
        let current_transcription = Mutex::new(String::new());
        let current_segments = Mutex::new(Vec::<String>::new());
        // NOTE TO SELF: there should probably be an "IDLE" in WhisperControlPhrase::*
        let current_control_phrase = Mutex::new(WhisperControlPhrase::GettingReady);

        let inner = Arc::new(TranscriberState {
            engine_kernel: Weak::new(),
            realtime_configs,
            offline_configs,
            realtime_running,
            offline_running,
            current_transcription,
            current_segments,
            current_control_phrase,
        });
        Self { inner }
    }

    pub(crate) fn set_engine_kernel(&self, kernel: Weak<dyn EngineKernel>) {
        *self.inner.engine_kernel = kernel;
    }

    // TODO: remove if unused.
    pub(crate) fn transcriber_running(&self) -> bool {
        self.realtime_running() || self.offline_running()
    }
    pub(crate) fn realtime_running(&self) -> bool {
        self.inner.realtime_running.load(Ordering::Acquire)
    }
    pub(crate) fn offline_running(&self) -> bool {
        self.inner.offline_running.load(Ordering::Acquire)
    }

    // NOTE TO SELF: remember to dereference the binding when calling builder methods to mutate,
    // otherwise, it'll just change the local binding.
    // These are for exposing a GUI-facing mutable configs reference so that UI toggles can update
    // the information here.
    pub(crate) fn realtime_configs_mut(&self) -> &mut WhisperRealtimeConfigs {
        self.inner.realtime_configs.write().deref_mut()
    }
    pub(crate) fn offline_configs_mut(&self) -> &mut WhisperConfigsV2 {
        self.inner.offline_configs.write().deref_mut()
    }

    pub(crate) fn try_read_realtime_configs(&self) -> Option<WhisperRealtimeConfigs> {
        self.inner
            .realtime_configs
            .try_read()
            .and_then(|configs| Some(configs.clone()))
    }
    pub(crate) fn try_read_offline_configs(&self) -> Option<WhisperConfigsV2> {
        self.inner
            .offline_configs
            .try_read()
            .and_then(|configs| Some(configs.clone()))
    }

    // These should be reserved for places where it's okay to block (e.g. serialization);
    // Otherwise try-read and accept the option.
    pub(crate) fn read_realtime_configs_blocking(&self) -> WhisperRealtimeConfigs {
        self.inner.realtime_configs.read().clone()
    }
    pub(crate) fn read_offline_configs_blocking(&self) -> WhisperConfigsV2 {
        self.inner.offline_configs.read().clone()
    }

    pub(crate) fn finalize_transcription(&self, final_transcription: String) {
        let current = self.inner.current_transcription.lock();
        *current = final_transcription;
        drop(current);
        // Clear the working segments - they're joined in the final transcription
        let segments = self.inner.current_segments.lock();
        *segments = vec![];
        drop(segments);
        // Set the control phrase to "GETTING READY", really should be IDLE.
        let control = self.inner.current_control_phrase.lock();
        *control = WhisperControlPhrase::GettingReady;
        drop(control);
    }

    // TODO: determine how to handle if this thread somehow panics re: removing progress jobs.
    // It might be wise to split jobs out by type, e.g. RealTime, Offline, Download, etc.
    // TODO: refactor once errors finished so that the type is correct.
    pub(crate) fn run_realtime(&self) -> RibbleWorkerHandle {
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
            let kernel =
                thread_inner
                    .engine_kernel
                    .upgrade()
                    .ok_or(RibbleWhisperError::ParameterError(
                        "Kernel not yet attached to TranscriberEngine.".to_string(),
                    ))?;

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
            let (fft_sender, fft_receiver) = get_channel::<Arc<[f32]>>(INPUT_BUFFER_CAPACITY);
            let (write_sender, write_receiver) = get_channel::<Arc<[f32]>>(INPUT_BUFFER_CAPACITY);
            // Transcription channels
            let (text_sender, text_receiver) = get_channel(INPUT_BUFFER_CAPACITY);
            // TODO: Handle actual vad configurations; these should be exposed sooomewhere.
            // Once configs are hammered out, read and convert to VAD object.
            // NOTE: this might get tricky with dyn impl + the type of the realtime transcriber.
            // Not 100% sure how to go about this just yet; probably an enum that can be unpacked
            // with a tight match on the builder method.
            // TODO: FIRST ORDER OF BUSINESS: GET THE VAD CONFIGS FROM THE KERNEL.
            let vad = Silero::try_new_whisper_realtime_default()?;
            // Set up the mic capture
            let audio_backend = kernel.get_audio_backend();
            let mic: FanoutMicCapture<f32, UseArc> =
                audio_backend.build_whisper_fanout_default(audio_sender)?;
            // Get a copy of the configs
            let configs = thread_inner.realtime_configs.read().clone();
            let (mut transcriber, transcriber_handle) = RealtimeTranscriberBuilder::new()
                .with_configs(configs)
                .with_audio_buffer(&audio_ring_buffer)
                .with_output_sender(text_sender)
                .with_voice_activity_detector(vad)
                .build()?;

            // Get the visualizer flag
            let visualizer_running = kernel.visualizer_running_flag();
            // Send a clone of the kernel to the scoped thread
            let scoped_kernel = Arc::clone(&kernel);

            let result = scope(|s| {
                // Audio Fanout
                let a_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);
                // Transcriber runner flag
                let t_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);
                // Print runner (stores transcription) flag
                let p_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);
                // Visualizer runner flag
                let v_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);
                let vis_running = Arc::clone(&visualizer_running);
                // Write thread runner flag
                let w_thread_run_transcription = Arc::clone(&thread_inner.realtime_running);

                // Disable stderr/stdout
                redirect_whisper_logging_to_hooks();

                // Close the "Setup" progress job
                scoped_kernel.remove_progress_job(setup_id);

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

                                // If the writer thread receiver is disconnected, the transcription is
                                // most likely finished and state needs to be synchronized.
                                // If it's full, just skip and accept there might be hiccups in the
                                // write thread.
                                if let Err(TrySendError::Disconnected(_)) =
                                    write_sender.try_send(Arc::clone(&audio))
                                {
                                    a_thread_run_transcription.store(false, Ordering::Release);
                                }

                                // If the visualizer is running, fan data out to the FFT processing
                                // thread.
                                if !vis_running.load(Ordering::Acquire) {
                                    continue;
                                }

                                if let Err(TrySendError::Disconnected(_)) =
                                    fft_sender.try_send(Arc::clone(&audio))
                                {
                                    a_thread_run_transcription.store(false, Ordering::Release);
                                }
                            }
                            Err(_) => a_thread_run_transcription.store(false, Ordering::Release),
                        }
                    }

                    // In case any of the other threads are hanging, pump the channel with a 0-length
                    // slice as a signal to "stop"
                    // TODO: refactor this--it's not ideal. This should instead, be an enum (AudioSignal::Stop vs AudioSignal::Data(Arc<[T]>));
                    // The print thread will always terminate because the transcriber will
                    // deallocate the channel once its transcription has stopped, so no need for monads/sentinels
                    let empty_buffer = Arc::from(&[][..]);
                    let _ = fft_sender.try_send(Arc::clone(&empty_buffer));
                    let _ = write_sender.try_send(Arc::clone(&empty_buffer));
                });
                let transcription_thread =
                    s.spawn(move || transcriber.process_audio(t_thread_run_transcription));
                let _print_thread = s.spawn(move || {
                    while p_thread_run_transcription.load(Ordering::Acquire) {
                        // TODO: this might work better with batched writes (1 confirmed + 1 segments or something along those lines).
                        match text_receiver.recv() {
                            Ok(output) => {
                                match output {
                                    WhisperOutput::ConfirmedTranscription(confirmed) => {
                                        // Update the confirmed transcription part
                                        let confirmed_guard =
                                            print_thread_inner.current_transcription.lock();
                                        *confirmed_guard = confirmed;
                                    }
                                    WhisperOutput::CurrentSegments(segments) => {
                                        // Update the copy of the working set of segments
                                        let segments_guard =
                                            print_thread_inner.current_segments.lock();
                                        *segments_guard = segments;
                                    }
                                    WhisperOutput::ControlPhrase(control) => {
                                        let control_guard =
                                            print_thread_inner.current_control_phrase.lock();
                                        *control_guard = control;
                                    }
                                }
                            }
                            Err(_) => {
                                p_thread_run_transcription.store(false, Ordering::Release);
                            }
                        }
                    }
                });
                let _write_thread = s.spawn(move || {
                    // TODO: writer setup
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

                    if let Err(e) = data_dir.as_ref() {
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
                    if let Err(e) = writer.as_ref() {
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
                let _visualizer_thread = s.spawn(move || {
                    // Preallocate a buffer for storing the latest computation
                    // This will get passed to the kernel to update the visualizer engine.
                    let mut buffer = [0.0; NUM_BUCKETS];
                    // Get a handle to the analysis type enum
                    while v_thread_run_transcription.load(Ordering::Acquire) {
                        match fft_receiver.recv() {
                            Ok(sample) => {
                                if sample.is_empty() {
                                    v_thread_run_transcription.store(false, Ordering::Release);
                                }

                                // Get the current Analysis type from the kernel
                                let analysis_type = scoped_kernel.get_visualizer_analysis_type();
                                match analysis_type {
                                    AnalysisType::Waveform => {
                                        normalized_waveform(&sample, &mut buffer);
                                    }
                                    AnalysisType::Power => {
                                        power_analysis(&sample, &mut buffer);
                                    }
                                    AnalysisType::SpectrumDensity => {
                                        frequency_analysis(
                                            &sample,
                                            &mut buffer,
                                            WHISPER_SAMPLE_RATE,
                                        );
                                    }
                                }
                            }
                            Err(_) => {
                                v_thread_run_transcription.store(false, Ordering::Release);
                            }
                        }

                        // Update the visualizer via the kernel.
                        scoped_kernel.update_visualizer(&buffer);
                    }
                });

                let res = transcription_thread.join().map_err(|e| {
                    RibbleWhisperError::Unknown(format!("Thread panicked! {:?}", e))
                })??;
                Ok(RibbleMessage::TranscriptionOutput(res))
            })?;
            mic.pause();
            result
        });
        worker
    }

    pub(crate) fn run_offline(&self) -> RibbleWorkerHandle {
        let thread_inner = Arc::clone(&self.inner);
        // Set the flag that the offline runner is running so that the UI can update.
        thread_inner.offline_running.store(true, Ordering::Release);

        // Set up the worker.
        let worker = std::thread::spawn(move || {
            // Get a handle to the kernel.
            let kernel =
                thread_inner
                    .engine_kernel
                    .upgrade()
                    .ok_or(RibbleWhisperError::ParameterError(
                        "Kernel not yet attached to TranscriberEngine.".to_string(),
                    ))?;
            // Get the vad configurations.
            // TODO: implement VadConfigs and replace -> likely should be split between realtime/offline
            // not sure about how to handle
            let vad = Silero::try_new_whisper_offline_default()?;
            // Get the configs
            let configs = thread_inner.offline_configs.read().clone();
            // Get the audio file path (TODO: figure out how to re-handle the temporary audio file)
            let audio_file_path =
                kernel
                    .get_audio_file_path()
                    .ok_or(RibbleWhisperError::ParameterError(
                        "File path not supplied to offline transcriber. This should not happen!"
                            .to_string(),
                    ))?;

            let callback_kernel = Arc::clone(&kernel);
            // NOTE TO SELF: the callback API in the audio loading doesn't currently have a way
            // to get the number of samples...
            // TODO: fix this in the library ASAP and expose functionality here please.
            // 10 000 is inserted as a stub to map out the logic.
            let load_audio_progress = Progress::determinate("Loading audio", 10000);
            let load_audio_id = kernel.add_progress_job(load_audio_progress);
            let load_audio_callback = move |progress: usize| {
                callback_kernel.update_progress_job(load_audio_id, progress as u64)
            };

            // Load the audio file.
            let audio = load_normalized_audio_file(audio_file_path, Some(load_audio_callback))?;
            // Remove the progress job now that transcription is finished.
            kernel.remove_progress_job(load_audio_id);
            // Set up the offline_transcriber
            // TODO: determine whether or not it's desired to set an on new segment
            // callback/print thread to send out and receive data.
            // I'm not 100% sold on it.
            let (sender, receiver) = get_channel(INPUT_BUFFER_CAPACITY);
            let mut offline_transcriber = OfflineTranscriberBuilder::new()
                .with_configs(configs)
                .with_audio(audio)
                .with_channel_configurations(AudioChannelConfiguration::Mono)
                .with_voice_activity_detector(vad)
                .with_sender(sender)
                .build()?;

            // -- If it's determined that it's unnecessary to set up a REPL a la realtime transcription
            // (seeing as it's just as the segments are read), perhaps don't bother.
            let scoped_kernel = Arc::clone(&kernel);
            let run_transcription = Arc::clone(&thread_inner.offline_running);
            let p_thread_run_transcription = Arc::clone(&run_transcription);
            let print_thread_inner = Arc::clone(&thread_inner);

            let result = scope(move |s| {
                // Set up a progress callback for transcription
                // As far as I can tell, this should be in integer percent
                let transcription_progress = Progress::determinate("Transcrbing", 100);
                let transcription_id = scoped_kernel.add_progress_job(transcription_progress);
                let callback_kernel = Arc::clone(&scoped_kernel);
                let transcription_closure = move |percent: i32| {
                    callback_kernel.update_progress_job(transcription_id, percent as u64);
                };
                let transcription_callback =
                    Some(StaticProgressCallback::new(transcription_closure));
                let whisper_callbacks = WhisperCallbacks {
                    progress: transcription_callback,
                };

                // Spawn the threads.
                let transcription_thread = s.spawn(move || {
                    offline_transcriber.process_with_callbacks(run_transcription, whisper_callbacks)
                });

                // TODO: this is identical code to the realtime loop; please handle this.
                // This is screaming to be factored out into a method --> or possibly removed entirely.
                // Again, I'm not 100% sold on the transcription/print thread combo here,
                // As these will only start spitting out as they're pulled from the whisper state.
                let _print_thread = s.spawn(move || {
                    while p_thread_run_transcription.load(Ordering::Acquire) {
                        // TODO: this might work better with batched writes (1 confirmed + 1 segments or something along those lines).
                        match receiver.recv() {
                            Ok(output) => {
                                match output {
                                    WhisperOutput::ConfirmedTranscription(confirmed) => {
                                        // Update the confirmed transcription part
                                        let confirmed_guard =
                                            print_thread_inner.current_transcription.lock();
                                        *confirmed_guard = confirmed;
                                    }
                                    WhisperOutput::CurrentSegments(segments) => {
                                        // Update the copy of the working set of segments
                                        let segments_guard =
                                            print_thread_inner.current_segments.lock();
                                        *segments_guard = segments;
                                    }
                                    WhisperOutput::ControlPhrase(control) => {
                                        let control_guard =
                                            print_thread_inner.current_control_phrase.lock();
                                        *control_guard = control;
                                    }
                                }
                            }
                            Err(_) => {
                                p_thread_run_transcription.store(false, Ordering::Release);
                            }
                        }
                    }
                });

                let res = transcription_thread.join().map_err(|e| {
                    RibbleWhisperError::Unknown(format!("Thread panicked! {:?}", e))
                })??;
                Ok(RibbleMessage::TranscriptionOutput(res))
            })?;
            result
        });
        worker
    }
}

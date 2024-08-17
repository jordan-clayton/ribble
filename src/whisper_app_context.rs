use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;

use sdl2::audio::AudioDevice;
use whisper_realtime::audio_ring_buffer::AudioRingBuffer;
use whisper_realtime::errors::{WhisperRealtimeError, WhisperRealtimeErrorType};
use whisper_realtime::microphone;
use whisper_realtime::recorder::Recorder;
use whisper_realtime::transcriber::realtime_transcriber::RealtimeTranscriber;
use whisper_realtime::transcriber::transcriber::Transcriber;

use crate::utils::configs::{AudioConfigs, AudioConfigType, RecorderConfigs, WorkerType};
use crate::utils::constants;
use crate::utils::progress::Progress;
use crate::utils::sdl_audio_wrapper::SdlAudioWrapper;

// TODO: refactor Errors once App Errors set up.
// TODO: Remaining impls + Download impl.

#[derive(Clone)]
pub struct WhisperAppController(Arc<WhisperAppContext>);

impl std::fmt::Debug for WhisperAppController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

impl WhisperAppController {
    pub fn new(audio_wrapper: Arc<SdlAudioWrapper>, system_theme: Option<eframe::Theme>) -> Self {
        let app_ctx = WhisperAppContext::new(audio_wrapper, system_theme);
        let app_ctx = Arc::new(app_ctx);
        Self(app_ctx)
    }

    pub fn get_system_theme(&self) -> Option<eframe::Theme> {
        self.0
            .system_theme
            .lock()
            .expect("Failed to get mutex")
            .clone()
    }

    pub fn set_system_theme(&mut self, theme: Option<eframe::Theme>) {
        let mut guard = self.0.system_theme.lock().expect("Failed to get mutex");
        *guard = theme;
    }

    pub fn audio_running(&self) -> bool {
        let realtime_running = self.realtime_running();
        let static_running = self.static_running();
        let recorder_running = self.static_running();
        return realtime_running || static_running || recorder_running;
    }

    // STATE
    pub fn is_working(&self) -> bool {
        let audio_running = self.audio_running();
        let downloading = self.is_downloading();
        audio_running || downloading
    }

    // READY
    pub fn realtime_ready(&self) -> bool {
        self.0.realtime_ready.load(Ordering::Acquire)
    }

    pub fn update_realtime_ready(&self, ready: bool) {
        self.0.realtime_ready.store(ready, Ordering::Relaxed);
    }

    pub fn static_ready(&self) -> bool {
        self.0.static_ready.load(Ordering::Acquire)
    }

    pub fn recorder_ready(&self) -> bool {
        self.0.recorder_ready.load(Ordering::Acquire)
    }

    // RUNNING
    pub fn is_downloading(&self) -> bool {
        self.0.downloading.load(Ordering::Acquire)
    }

    pub fn recorder_running(&self) -> bool {
        self.0.recorder_running.load(Ordering::Acquire)
    }

    pub fn realtime_running(&self) -> bool {
        self.0.realtime_running.load(Ordering::Acquire)
    }
    pub fn static_running(&self) -> bool {
        self.0.static_running.load(Ordering::Acquire)
    }
    // CONFIGS

    pub fn send_configs(
        &self,
        configs: AudioConfigs,
    ) -> Result<(), crossbeam::channel::SendError<AudioConfigs>> {
        self.0.configs_sender.send(configs)
    }

    // MSG HANDLING

    pub fn send_progress(
        &self,
        progress: Progress,
    ) -> Result<(), crossbeam::channel::SendError<Progress>> {
        self.0.progress_sender.send(progress)
    }

    pub fn recv_progress(&self) -> Result<Progress, crossbeam::channel::TryRecvError> {
        self.0.progress_receiver.try_recv()
    }

    fn realtime_audio_sender(&self) -> crossbeam::channel::Sender<Vec<f32>> {
        self.0.record_audio_f32_sender.clone()
    }

    // TODO: this might be better as an internal function run on a thread.
    // TODO: This might actually be better to recv and block on a thread.
    fn receive_realtime_audio(&self) -> Result<Vec<f32>, crossbeam::channel::RecvError> {
        self.0.record_audio_f32_receiver.recv()
    }

    // TODO: determine whether to encapsulate or allow producer clone
    // These might be best left internal & handle threading + computation threads here.
    pub fn record_audio_i16_sender(&self) -> crossbeam::channel::Sender<Vec<i16>> {
        self.0.record_audio_i16_sender.clone()
    }

    pub fn receive_audio_i16(&self) -> Result<Vec<i16>, crossbeam::channel::TryRecvError> {
        self.0.record_audio_i16_receiver.try_recv()
    }
    pub fn record_audio_i32_sender(&self) -> crossbeam::channel::Sender<Vec<i32>> {
        self.0.record_audio_i32_sender.clone()
    }

    pub fn receive_audio_i32(&self) -> Result<Vec<i32>, crossbeam::channel::TryRecvError> {
        self.0.record_audio_i32_receiver.try_recv()
    }

    pub fn record_audio_f32_sender(&self) -> crossbeam::channel::Sender<Vec<f32>> {
        self.0.record_audio_f32_sender.clone()
    }

    pub fn receive_audio_f32(&self) -> Result<Vec<f32>, crossbeam::channel::TryRecvError> {
        self.0.record_audio_f32_receiver.try_recv()
    }

    // This needs to be copied and a copy given to the RealtimeTranscriber struct.
    // TODO: possibly handle this internally.
    pub fn transcription_text_sender(
        &self,
    ) -> crossbeam::channel::Sender<Result<(String, bool), WhisperRealtimeError>> {
        self.0.transcription_text_sender.clone()
    }

    pub fn receive_transcription_text(
        &self,
    ) -> Result<Result<(String, bool), WhisperRealtimeError>, crossbeam::channel::TryRecvError>
    {
        self.0.transcription_text_receiver.try_recv()
    }

    pub fn send_error(
        &self,
        error: WhisperRealtimeError,
    ) -> Result<(), crossbeam::channel::SendError<WhisperRealtimeError>> {
        self.0.error_sender.send(error)
    }

    pub fn receive_error(&self) -> Result<WhisperRealtimeError, crossbeam::channel::TryRecvError> {
        self.0.error_receiver.try_recv()
    }

    // TODO: Figure out how to implement a "Setup Progress message"
    // TODO: These can't be scoped threads or the gui will block.
    // TODO: refactor scoped threads -> Include a Setup key & join on replace.
    pub fn start_realtime_transcription(&mut self, ctx: &egui::Context) {
        let ctx = ctx.clone();
        // Update state
        self.0.realtime_running.store(true, Ordering::Release);

        // TODO: This should probably send a progress job - Figure out how to determine and interpolate progress.

        // Get the realtime configs.
        self.0
            .realtime_configs_request_sender
            .send(())
            .expect("Realtime configs request channel already closed");
        let actor = self.clone();
        let c_actor = actor.clone();

        let rt_thread = thread::spawn(move || {
            // This spawns a scoped thread to poll the msg queue and get the correct configurations.
            let configs = get_requested_configs(c_actor.clone(), AudioConfigType::REALTIME, ctx);
            if let Err(e) = &configs {
                c_actor
                    .0
                    .error_sender
                    .send(e.clone())
                    .expect("Error Channel closed");
                c_actor.0.realtime_running.store(false, Ordering::Release);
                panic!();
            }

            let configs = configs.unwrap();

            if !configs.is_realtime() {
                let err = WhisperRealtimeError::new(
                    WhisperRealtimeErrorType::ParameterError,
                    String::from("Invalid configs provided for realtime stream"),
                );
                c_actor
                    .0
                    .error_sender
                    .send(err)
                    .expect("Error Channel closed");
                c_actor.0.realtime_running.store(false, Ordering::Release);
                panic!();
            }

            let AudioConfigs::Realtime(configs) = configs else {
                panic!()
            };

            let configs = Arc::new(configs);
            // TODO: figure out what the borrow checker is fussing over.
            // The setup functions might need to be refactored.
            run_realtime_audio_transcription(
                c_actor.clone(),
                &c_actor.0.audio_wrapper.audio_subsystem,
                configs,
            )
        });

        let mut guard = actor
            .0
            .active_threads
            .lock()
            .expect("Failed to get thread mutex");
        let old_thread = guard.insert(WorkerType::REALTIME.to_key(), rt_thread);

        assert!(old_thread.is_none(), "Recording already in progress");
    }

    // TODO: see rt fn for ideas on how to structure.
    pub fn start_static_transcription(
        &self,
        _file_path: &std::path::Path,
    ) -> Result<(), WhisperRealtimeError> {
        todo!()
        // Setup
        // Progress = setup progress.
        // Set state.
        // Spawn thread:
        // Get configs from Configs pane
        // Load file.
        // Progress = whisper callback progress.
        // Run separate transcription thread.
        // Store join-handles
    }

    pub fn start_recording(&self) -> Result<(), WhisperRealtimeError> {
        todo!()
        // Setup -> Write to tmp file.
        // Spawn threads
        // Store join-handles
        // Set state.
        // Progress = uh. Not sure.
    }

    // TODO: These should probably panic if there is no thread-handle to join.
    // TODO: Possibly do this on a thread.
    // TODO: factor out error function
    pub fn close_worker(&mut self, worker_type: WorkerType) {
        assert!(
            !self.is_working(),
            "Improper close-worker call, not properly validated"
        );
        match worker_type {
            WorkerType::DOWNLOADING => {
                self.0.downloading.store(false, Ordering::Relaxed);
                // Close the main processing thread.
                let mut guard = self.0.active_threads.lock().expect("Failed to get mutex");
                let thread = guard.remove(WorkerType::DOWNLOADING.to_key());
                if let Some(t) = thread {
                    let msg = t.join();
                    match msg {
                        Ok(result) => {
                            if let Err(e) = result {
                                let err = WhisperRealtimeError::new(
                                    WhisperRealtimeErrorType::DownloadError,
                                    format!("Download thread panicked: {:?}", e),
                                );
                                self.send_error(err).expect("Error channel closed");
                            }
                        }
                        Err(e) => {
                            let err = WhisperRealtimeError::new(
                                WhisperRealtimeErrorType::DownloadError,
                                format!("Download thread panicked: {:?}", e),
                            );
                            self.send_error(err).expect("Error channel closed");
                        }
                    }
                }
            }
            WorkerType::REALTIME => {
                self.0.realtime_running.store(false, Ordering::Relaxed);
                let mut guard = self.0.active_threads.lock().expect("Failed to get mutex");
                let thread = guard.remove(WorkerType::REALTIME.to_key());
                if let Some(t) = thread {
                    let msg = t.join();
                    match msg {
                        Ok(result) => {
                            match result {
                                Ok(s) => {
                                    // Send a CLEAR message to the display thread, then the full transcription over.
                                    self.0
                                        .transcription_text_sender
                                        .send(Ok((String::from(constants::CLEAR_MSG), true)))
                                        .expect("Transcription channel closed");
                                    self.0
                                        .transcription_text_sender
                                        .send(Ok((s, true)))
                                        .expect("Transcription channel closed");
                                }
                                Err(e) => {
                                    let err = WhisperRealtimeError::new(
                                        WhisperRealtimeErrorType::DownloadError,
                                        format!("Realtime Processing thread panicked: {:?}", e),
                                    );
                                    self.send_error(err).expect("Error channel closed");
                                }
                            }
                        }
                        Err(e) => {
                            let err = WhisperRealtimeError::new(
                                WhisperRealtimeErrorType::DownloadError,
                                format!("Realtime Processing thread panicked: {:?}", e),
                            );
                            self.send_error(err).expect("Error channel closed");
                        }
                    }
                }
            }
            WorkerType::STATIC => {
                self.0.static_running.store(false, Ordering::Relaxed);
                let mut guard = self.0.active_threads.lock().expect("Failed to get mutex");
                let thread = guard.remove(WorkerType::STATIC.to_key());
                if let Some(t) = thread {
                    let msg = t.join();
                    match msg {
                        Ok(result) => {
                            match result {
                                Ok(s) => {
                                    // Send a CLEAR message to the display thread, then the full transcription over.
                                    self.0
                                        .transcription_text_sender
                                        .send(Ok((String::from(constants::CLEAR_MSG), true)))
                                        .expect("Transcription channel closed");
                                    self.0
                                        .transcription_text_sender
                                        .send(Ok((s, true)))
                                        .expect("Transcription channel closed");
                                }
                                Err(e) => {
                                    let err = WhisperRealtimeError::new(
                                        WhisperRealtimeErrorType::DownloadError,
                                        format!("Static Processing thread panicked: {:?}", e),
                                    );
                                    self.send_error(err).expect("Error channel closed");
                                }
                            }
                        }
                        Err(e) => {
                            let err = WhisperRealtimeError::new(
                                WhisperRealtimeErrorType::DownloadError,
                                format!("Static Processing thread panicked: {:?}", e),
                            );
                            self.send_error(err).expect("Error channel closed");
                        }
                    }
                }
            }
            // TODO: refactor to use proper error.
            WorkerType::RECORDING => {
                self.0.recorder_running.store(false, Ordering::Relaxed);
                let mut guard = self.0.active_threads.lock().expect("Failed to get mutex");
                let thread = guard.remove(WorkerType::RECORDING.to_key());
                if let Some(t) = thread {
                    let msg = t.join();
                    match msg {
                        Ok(result) => {
                            if let Err(e) = result {
                                let err = WhisperRealtimeError::new(
                                    WhisperRealtimeErrorType::Unknown,
                                    format!("Recording thread panicked: {:?}", e),
                                );
                                self.send_error(err).expect("Error channel closed");
                            }
                        }
                        Err(e) => {
                            let err = WhisperRealtimeError::new(
                                WhisperRealtimeErrorType::Unknown,
                                format!("Recording thread panicked: {:?}", e),
                            );
                            self.send_error(err).expect("Error channel closed");
                        }
                    }
                }
            }
        }
    }

    // TODO: Come back to this & possibly rethink - is a possible bottleneck.
    // TODO: Factor this out into a function
}

fn get_requested_configs(
    actor: WhisperAppController,
    requested_config_type: AudioConfigType,
    ctx: egui::Context,
) -> Result<AudioConfigs, WhisperRealtimeError> {
    let start_time = std::time::Instant::now();
    let configs = thread::scope(|s| {
        let c = s.spawn(|| {
            let mut now = std::time::Instant::now();
            loop {
                if now - start_time > constants::CHANNEL_TIMEOUT {
                    panic!("Timeout: failed to receive transcription configs")
                }

                ctx.request_repaint();
                let try_for_configs = actor.0.configs_receiver.try_recv();
                if let Err(e) = &try_for_configs {
                    match e {
                        crossbeam::channel::TryRecvError::Empty => {
                            thread::sleep(constants::SLEEP_DURATION);
                        }
                        crossbeam::channel::TryRecvError::Disconnected => {
                            panic!("Config Channel has been closed")
                        }
                    }
                }

                let configs = try_for_configs.unwrap();
                match requested_config_type {
                    AudioConfigType::REALTIME => {
                        if configs.is_realtime() {
                            return configs;
                        } else {
                            thread::sleep(constants::SLEEP_DURATION);
                        }
                    }
                    AudioConfigType::STATIC => {
                        if configs.is_static() {
                            return configs;
                        } else {
                            thread::sleep(constants::SLEEP_DURATION);
                        }
                    }
                    AudioConfigType::RECORDING => {
                        if configs.is_recording() {
                            return configs;
                        } else {
                            thread::sleep(constants::SLEEP_DURATION);
                        }
                    }
                }

                now = std::time::Instant::now();
            }
        });
        c.join()
    });

    if let Err(e) = &configs {
        return Err(WhisperRealtimeError::new(
            WhisperRealtimeErrorType::ParameterError,
            format!("Parameter Error: {:?}", e),
        ));
    };

    Ok(configs.unwrap())
}

fn run_realtime_audio_transcription(
    actor: WhisperAppController,
    audio_subsystem: &sdl2::AudioSubsystem,
    configs: Arc<whisper_realtime::configs::Configs>,
) -> Result<String, Box<dyn std::any::Any + Send>> {
    // Clone the actor
    let c_actor_audio = actor.clone();
    let c_actor_write = actor.clone();

    // Init model.
    let model = init_model(configs.clone());
    let c_model = model.clone();

    // Clone configs
    let c_configs = configs.clone();

    // Audio buffer.
    let audio = init_audio_buffer(Some(whisper_realtime::constants::INPUT_BUFFER_CAPACITY));
    let c_audio_reader = audio.clone();
    let c_audio_transcriber = audio.clone();

    // Text sender
    let c_text_sender = actor.0.transcription_text_sender.clone();
    // Recording sender for tmp file
    let c_recording_sender = actor.0.record_audio_f32_sender.clone();

    // State Flags - This should likely be refactored.
    let c_realtime_is_ready = actor.0.realtime_ready.clone();
    let c_realtime_is_running = actor.0.realtime_running.clone();

    let mic_stream =
        init_realtime_microphone(audio_subsystem, actor.0.record_audio_f32_sender.clone());
    let c_mic_stream = mic_stream.clone();

    // Init Whisper.
    let ctx = init_whisper_ctx(c_model.clone(), c_configs.use_gpu);
    let mut state = ctx.create_state().expect("Failed to create WhisperState");

    // TODO: consider importing my library to reduce the typing.
    let transcription = thread::scope(|s| {
        let c_realtime_is_running_audio = c_realtime_is_running.clone();
        let c_realtime_is_running_write = c_realtime_is_running.clone();
        c_mic_stream.resume();

        let _write_thread = s.spawn(move || {
            // TODO: Implementation.
            // TODO: Open temporary file for writing wav.
            loop {
                if !c_realtime_is_running_write.load(Ordering::Acquire) {
                    // TODO: Finalize write
                    break;
                }
                let output = c_actor_audio.receive_realtime_audio();
                assert!(output.is_ok(), "F32 Audio Channel Closed");
                // TODO: Write out to the temporary file.
            }
        });

        let _audio_thread = s.spawn(move || loop {
            if !c_realtime_is_running_audio.load(Ordering::Acquire) {
                break;
            }

            let output = c_actor_write.receive_realtime_audio();
            assert!(output.is_ok(), "Realtime Audio Channel closed");

            let mut audio_data = output.unwrap();
            c_audio_reader.push_audio(&mut audio_data);
            let write_propagation = c_recording_sender.send(audio_data.clone());
            assert!(write_propagation.is_ok(), "F32 Audio Channel closed");
        });
        let transcription_thread = s
            .spawn(move || {
                let mut transcriber = RealtimeTranscriber::new_with_configs(
                    c_audio_transcriber,
                    c_text_sender,
                    c_realtime_is_running.clone(),
                    c_realtime_is_ready.clone(),
                    c_configs.clone(),
                    None,
                );
                let output = transcriber.process_audio(&mut state, None::<fn(i32)>);
                output
            })
            .join();
        transcription_thread
    });

    c_mic_stream.pause();

    transcription
}

// TODO: UTILITY FUNCTIONS FILE
// TODO: FN FOR INITIALIZING TEMP FILE PTR.

fn init_audio_buffer<
    T: Default + Clone + Copy + sdl2::audio::AudioFormatNum + Sync + Send + 'static,
>(
    len_ms: Option<usize>,
) -> Arc<AudioRingBuffer<T>> {
    let ms = len_ms.unwrap_or(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    let audio: AudioRingBuffer<T> = AudioRingBuffer::new(ms);
    Arc::new(audio)
}

fn init_microphone<
    T: Default + Clone + Copy + sdl2::audio::AudioFormatNum + Sync + Send + 'static,
>(
    audio_subsystem: &sdl2::AudioSubsystem,
    desired_audio_spec: &sdl2::audio::AudioSpecDesired,
    audio_sender: crossbeam::channel::Sender<Vec<T>>,
) -> Arc<AudioDevice<Recorder<T>>> {
    let mic_stream =
        microphone::build_audio_stream(audio_subsystem, desired_audio_spec, audio_sender);
    Arc::new(mic_stream)
}

fn init_realtime_microphone(
    audio_subsystem: &sdl2::AudioSubsystem,
    audio_sender: crossbeam::channel::Sender<Vec<f32>>,
) -> Arc<AudioDevice<Recorder<f32>>> {
    let desired_audio_spec = microphone::get_desired_audio_spec(
        Some(whisper_realtime::constants::WHISPER_SAMPLE_RATE as i32),
        Some(1),
        Some(1024),
    );

    init_microphone(audio_subsystem, &desired_audio_spec, audio_sender)
}

fn init_recording_microphone<
    T: Default + Clone + Copy + sdl2::audio::AudioFormatNum + Sync + Send + 'static,
>(
    audio_subsystem: &sdl2::AudioSubsystem,
    recording_configs: Arc<RecorderConfigs>,
    audio_sender: crossbeam::channel::Sender<Vec<T>>,
) -> Arc<AudioDevice<Recorder<T>>> {
    let freq = recording_configs.extract_sample_rate();
    let channels = recording_configs.extract_num_channels();
    let buffer_size = recording_configs.extract_buffer_size();
    let desired_audio_spec = microphone::get_desired_audio_spec(freq, channels, buffer_size);
    init_microphone(audio_subsystem, &desired_audio_spec, audio_sender)
}

fn init_model(
    configs: Arc<whisper_realtime::configs::Configs>,
) -> Arc<whisper_realtime::model::Model> {
    let model_type = configs.model;
    let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get Data dir");
    let model = whisper_realtime::model::Model::new_with_type_and_dir(model_type, data_dir);
    assert!(model.is_downloaded(), "Model not downloaded");
    Arc::new(model)
}

fn init_whisper_ctx(
    model: Arc<whisper_realtime::model::Model>,
    use_gpu: bool,
) -> whisper_realtime::whisper_rs::WhisperContext {
    let mut whisper_ctx_params = whisper_realtime::whisper_rs::WhisperContextParameters::default();
    whisper_ctx_params.use_gpu = use_gpu;
    let model_path = model.file_path();
    let model_path = model_path.as_path();
    whisper_realtime::whisper_rs::WhisperContext::new_with_params(
        model_path.to_str().expect("Failed to stringify path"),
        whisper_ctx_params,
    )
    .expect("Failed to load model")
}

struct WhisperAppContext {
    // SYSTEM THEME
    system_theme: Mutex<Option<eframe::Theme>>,
    // SDL AUDIO
    audio_wrapper: Arc<SdlAudioWrapper>,
    // STATE
    realtime_ready: Arc<AtomicBool>,
    static_ready: Arc<AtomicBool>,
    recorder_ready: Arc<AtomicBool>,

    // WORKER FLAGS
    downloading: Arc<AtomicBool>,
    // Recorder flags.
    recorder_running: Arc<AtomicBool>,
    realtime_running: Arc<AtomicBool>,
    static_running: Arc<AtomicBool>,

    // Configs Channels (UNBOUNDED):
    // Request configs
    realtime_configs_request_sender: crossbeam::channel::Sender<()>,
    realtime_configs_request_receiver: crossbeam::channel::Receiver<()>,

    static_configs_request_sender: crossbeam::channel::Sender<()>,
    static_configs_request_receiver: crossbeam::channel::Receiver<()>,

    recording_configs_request_sender: crossbeam::channel::Sender<()>,
    recording_configs_request_receiver: crossbeam::channel::Receiver<()>,

    // Send-Recv Configs channel (UNBOUNDED):
    configs_sender: crossbeam::channel::Sender<AudioConfigs>,
    configs_receiver: crossbeam::channel::Receiver<AudioConfigs>,

    // Recording channels (BOUNDED):
    record_audio_i16_sender: crossbeam::channel::Sender<Vec<i16>>,
    record_audio_i16_receiver: crossbeam::channel::Receiver<Vec<i16>>,

    // TODO: Remove if no wav support.
    record_audio_i32_sender: crossbeam::channel::Sender<Vec<i32>>,
    record_audio_i32_receiver: crossbeam::channel::Receiver<Vec<i32>>,

    record_audio_f32_sender: crossbeam::channel::Sender<Vec<f32>>,
    record_audio_f32_receiver: crossbeam::channel::Receiver<Vec<f32>>,

    // GUI CHANNELS (UNBOUNDED):
    // Transcription channel for passing text output
    transcription_text_sender:
        crossbeam::channel::Sender<Result<(String, bool), WhisperRealtimeError>>,
    transcription_text_receiver:
        crossbeam::channel::Receiver<Result<(String, bool), WhisperRealtimeError>>,

    progress_sender: crossbeam::channel::Sender<Progress>,
    progress_receiver: crossbeam::channel::Receiver<Progress>,

    error_sender: crossbeam::channel::Sender<WhisperRealtimeError>,
    error_receiver: crossbeam::channel::Receiver<WhisperRealtimeError>,

    // TODO: refactor into msg queue.
    active_threads:
        Mutex<HashMap<&'static str, JoinHandle<Result<String, Box<dyn std::any::Any + Send>>>>>,
}

impl WhisperAppContext {
    fn cleanup(&self) {
        let mut guard = self
            .active_threads
            .lock()
            .expect("Failed to get thread mutex");

        let keys: Vec<&str> = guard.keys().cloned().collect();
        for key in keys {
            let t = guard.remove(key).expect("Failed to get join handle");
            let _ = t.join();
        }
    }
}

impl WhisperAppContext {
    fn new(audio_wrapper: Arc<SdlAudioWrapper>, system_theme: Option<eframe::Theme>) -> Self {
        let system_theme = Mutex::new(system_theme);

        // STATE
        let downloading = Arc::new(AtomicBool::new(false));
        let realtime_ready = Arc::new(AtomicBool::new(false));
        let static_ready = Arc::new(AtomicBool::new(false));
        let recorder_ready = Arc::new(AtomicBool::new(false));

        let realtime_running = Arc::new(AtomicBool::new(false));
        let static_running = Arc::new(AtomicBool::new(false));
        let recorder_running = Arc::new(AtomicBool::new(false));

        // CONFIGS
        let (realtime_configs_request_sender, realtime_configs_request_receiver) =
            crossbeam::channel::bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
        let (static_configs_request_sender, static_configs_request_receiver) =
            crossbeam::channel::bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
        let (recording_configs_request_sender, recording_configs_request_receiver) =
            crossbeam::channel::bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);

        let (configs_sender, configs_receiver) = crossbeam::channel::unbounded();

        // RECORDING
        let (record_audio_i16_sender, record_audio_i16_receiver) = crossbeam::channel::unbounded();
        // TODO: remove if no hound spt.
        let (record_audio_i32_sender, record_audio_i32_receiver) = crossbeam::channel::unbounded();
        let (record_audio_f32_sender, record_audio_f32_receiver) = crossbeam::channel::unbounded();

        // GUI
        let (transcription_text_sender, transcription_text_receiver) =
            crossbeam::channel::unbounded();
        let (progress_sender, progress_receiver) = crossbeam::channel::unbounded();
        let (error_sender, error_receiver) = crossbeam::channel::unbounded();

        let active_threads: HashMap<
            &'static str,
            JoinHandle<Result<String, Box<dyn std::any::Any + Send>>>,
        > = HashMap::new();
        let active_threads = Mutex::new(active_threads);

        Self {
            system_theme,
            audio_wrapper,
            realtime_ready,
            static_ready,
            recorder_ready,
            downloading,
            realtime_running,
            static_running,
            recorder_running,
            realtime_configs_request_sender,
            realtime_configs_request_receiver,
            static_configs_request_sender,
            static_configs_request_receiver,
            recording_configs_request_sender,
            recording_configs_request_receiver,
            configs_sender,
            configs_receiver,
            record_audio_i16_sender,
            record_audio_i16_receiver,
            record_audio_i32_sender,
            record_audio_i32_receiver,
            record_audio_f32_sender,
            record_audio_f32_receiver,
            transcription_text_sender,
            transcription_text_receiver,
            progress_sender,
            progress_receiver,
            error_sender,
            error_receiver,
            active_threads,
        }
    }
}

impl Drop for WhisperAppContext {
    fn drop(&mut self) {
        self.cleanup();
    }
}

use std::{
    any::Any,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        Mutex,
        TryLockError,
    },
    thread::{self, JoinHandle},
};

use crossbeam::channel::SendError;
use hound::{SampleFormat, WavSpec};
use sdl2::audio::AudioDevice;
use whisper_realtime::{
    audio_ring_buffer::AudioRingBuffer,
    errors::WhisperRealtimeError,
    microphone,
    recorder::Recorder,
    transcriber::{
        realtime_transcriber::RealtimeTranscriber,
        transcriber::Transcriber,
    },
};

use crate::utils::{
    configs::{AudioConfigs, AudioConfigType, RecorderConfigs, WorkerType},
    console_message::{ConsoleMessage, ConsoleMessageType},
    constants,
    errors::{WhisperAppError, WhisperAppErrorType}, file_mgmt::{get_tmp_file_writer, write_audio_sample},
    progress::Progress,
    sdl_audio_wrapper::SdlAudioWrapper,
};

// TODO: Remaining impls + Download impl.

#[derive(Clone)]
pub struct WhisperAppController(Arc<WhisperAppContext>);

impl std::fmt::Debug for WhisperAppController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

impl WhisperAppController {
    pub fn new(
        audio_wrapper: Arc<SdlAudioWrapper>,
        system_theme: Option<eframe::Theme>,
        thread_handle_sender: crossbeam::channel::Sender<WhisperAppThread>,
    ) -> Self {
        let app_ctx = WhisperAppContext::new(audio_wrapper, system_theme, thread_handle_sender);
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

    pub fn update_static_ready(&self, ready: bool) {
        self.0.static_ready.store(ready, Ordering::Relaxed);
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

    pub fn send_configs(&self, configs: AudioConfigs) -> Result<(), SendError<AudioConfigs>> {
        self.0.configs_sender.send(configs)
    }

    // MSG HANDLING
    // CONFIGS REQUESTS
    pub fn recv_realtime_configs_req(&self) -> Result<(), crossbeam::channel::TryRecvError> {
        self.0.realtime_configs_request_receiver.try_recv()
    }
    pub fn recv_static_configs_req(&self) -> Result<(), crossbeam::channel::TryRecvError> {
        self.0.static_configs_request_receiver.try_recv()
    }
    pub fn recv_recording_configs_req(&self) -> Result<(), crossbeam::channel::TryRecvError> {
        self.0.recording_configs_request_receiver.try_recv()
    }

    pub fn send_progress(&self, progress: Progress) -> Result<(), SendError<Progress>> {
        self.0.progress_sender.send(progress)
    }

    pub fn recv_progress(&self) -> Result<Progress, crossbeam::channel::TryRecvError> {
        self.0.progress_receiver.try_recv()
    }
    pub fn send_console_message(
        &self,
        msg: ConsoleMessage,
    ) -> Result<(), SendError<ConsoleMessage>> {
        self.0.console_sender.send(msg)
    }

    pub fn recv_console_message(&self) -> Result<ConsoleMessage, crossbeam::channel::TryRecvError> {
        self.0.console_receiver.try_recv()
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

    pub fn recv_transcription_text(
        &self,
    ) -> Result<Result<(String, bool), WhisperRealtimeError>, crossbeam::channel::TryRecvError>
    {
        self.0.transcription_text_receiver.try_recv()
    }

    pub fn write_fft_buffer(&self, new_fft: &[f32; constants::NUM_BUCKETS]) {
        let mut guard = self.0.fft_buffer.lock().unwrap();
        guard.copy_from_slice(new_fft);
    }

    pub fn read_fft_buffer(&self, dest: &mut [f32; constants::NUM_BUCKETS]) {
        let guard = self.0.fft_buffer.try_lock();
        match guard {
            Ok(g) => {
                dest.copy_from_slice(g.as_slice());
            }
            Err(TryLockError::WouldBlock) => {
                return;
            }
            Err(_) => {
                panic!();
            }
        }
    }

    fn send_thread_handle(
        &self,
        thread: WhisperAppThread,
    ) -> Result<(), SendError<WhisperAppThread>> {
        self.0.thread_handle_sender.send(thread)
    }

    pub fn start_realtime_transcription(&mut self, ctx: &egui::Context) {
        let job_name = "realtime_setup";
        let c_job_name = job_name.clone();

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 1, 100);
        self.send_progress(progress).expect("Channel closed");

        let ctx = ctx.clone();
        // Update state
        self.0.realtime_running.store(true, Ordering::Release);

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 17, 100);
        self.send_progress(progress).expect("Channel closed");

        // Get the realtime configs.
        self.0
            .realtime_configs_request_sender
            .send(())
            .expect("Realtime configs request channel already closed");
        let actor = self.clone();
        let c_actor = actor.clone();

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 33, 100);
        self.send_progress(progress).expect("Channel closed");

        let rt_thread = thread::spawn(move || {
            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(c_job_name), 50, 100);
            c_actor.send_progress(progress).expect("Channel closed");

            // This spawns a scoped thread to poll the msg queue and get the correct configurations.
            let configs = get_requested_configs(c_actor.clone(), AudioConfigType::Realtime, ctx);
            if let Err(e) = &configs {
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_actor
                    .0
                    .console_sender
                    .send(msg)
                    .expect("Error Channel closed");
                c_actor.0.realtime_running.store(false, Ordering::Release);
                panic!();
            }
            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(c_job_name), 76, 100);
            c_actor.send_progress(progress).expect("Channel closed");

            let configs = configs.unwrap();

            if !configs.is_realtime() {
                let e = WhisperAppError::new(WhisperAppErrorType::ParameterError, String::from("Invalid configs provided to realtime stream. Either invalid data passed, or data race condition"));
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_actor
                    .0
                    .console_sender
                    .send(msg)
                    .expect("Error Channel closed");
                c_actor.0.realtime_running.store(false, Ordering::Release);
                panic!();
            }

            let AudioConfigs::Realtime(configs) = configs else {
                panic!()
            };

            let configs = Arc::new(configs);

            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(job_name), 100, 100);
            c_actor.send_progress(progress).expect("Channel closed");

            run_realtime_audio_transcription(c_actor.clone(), configs)
        });

        let thread = (WorkerType::Realtime, rt_thread);

        // Send to the background actor to join.
        self.send_thread_handle(thread)
            .expect("Thread channel closed");
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
}

fn get_requested_configs(
    actor: WhisperAppController,
    requested_config_type: AudioConfigType,
    ctx: egui::Context,
) -> Result<AudioConfigs, WhisperAppError> {
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
                    AudioConfigType::Realtime => {
                        if configs.is_realtime() {
                            return configs;
                        } else {
                            thread::sleep(constants::SLEEP_DURATION);
                        }
                    }
                    AudioConfigType::Static => {
                        if configs.is_static() {
                            return configs;
                        } else {
                            thread::sleep(constants::SLEEP_DURATION);
                        }
                    }
                    AudioConfigType::Recording => {
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
        return Err(WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Parameter Error: {:?}", e),
        ));
    };

    Ok(configs.unwrap())
}

fn run_realtime_audio_transcription(
    actor: WhisperAppController,
    configs: Arc<whisper_realtime::configs::Configs>,
) -> Result<String, Box<dyn Any + Send>> {
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

    let mic_stream = init_realtime_microphone(
        &actor.0.audio_wrapper.audio_subsystem,
        actor.0.record_audio_f32_sender.clone(),
    );
    let audio_spec = Arc::new(mic_stream.spec().clone());
    let c_audio_spec = audio_spec.clone();
    let c_mic_stream = mic_stream.clone();

    // Init Whisper.
    let ctx = init_whisper_ctx(c_model.clone(), c_configs.use_gpu);
    let mut state = ctx.create_state().expect("Failed to create WhisperState");

    let transcription = thread::scope(|s| {
        let c_realtime_is_running_audio_read_thread = c_realtime_is_running.clone();
        let c_realtime_is_running_write_thread = c_realtime_is_running.clone();
        c_mic_stream.resume();

        let _write_thread = s.spawn(move || {
            let channels = c_audio_spec.channels as u16;
            let sample_rate = (c_audio_spec.freq as i64) as u32;
            let spec = WavSpec {
                channels,
                sample_rate,
                bits_per_sample: 32,
                sample_format: SampleFormat::Float,
            };

            let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data dir");
            let mut writer =
                get_tmp_file_writer(data_dir.as_path(), &spec).expect("Failed to open writer.");

            loop {
                if !c_realtime_is_running_write_thread.load(Ordering::Acquire) {
                    writer.finalize().expect("Failed to close writer.");
                    break;
                }
                let output = c_actor_audio.receive_realtime_audio();
                assert!(output.is_ok(), "F32 Audio Channel Closed");
                let output = output.unwrap();
                write_audio_sample(&output, &mut writer, None::<fn(usize)>);
            }
        });

        let _audio_thread = s.spawn(move || loop {
            if !c_realtime_is_running_audio_read_thread.load(Ordering::Acquire) {
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

// IMPL FN's -> TODO: determine whether to migrate to separate util fn file.
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

type WhisperAppThread = (WorkerType, JoinHandle<Result<String, Box<dyn Any + Send>>>);

struct WhisperAppContext {
    // SYSTEM THEME
    system_theme: Mutex<Option<eframe::Theme>>,
    // SDL AUDIO
    audio_wrapper: Arc<SdlAudioWrapper>,
    // STATE
    realtime_ready: Arc<AtomicBool>,
    static_ready: Arc<AtomicBool>,

    // WORKER FLAGS
    downloading: Arc<AtomicBool>,
    // RECORDER FLAGS.
    recorder_running: Arc<AtomicBool>,
    realtime_running: Arc<AtomicBool>,
    static_running: Arc<AtomicBool>,

    // FFT buffer
    fft_buffer: Mutex<[f32; constants::NUM_BUCKETS]>,
    // Configs Channels (BOUNDED):
    // Request configs
    realtime_configs_request_sender: crossbeam::channel::Sender<()>,
    realtime_configs_request_receiver: crossbeam::channel::Receiver<()>,

    static_configs_request_sender: crossbeam::channel::Sender<()>,
    static_configs_request_receiver: crossbeam::channel::Receiver<()>,

    recording_configs_request_sender: crossbeam::channel::Sender<()>,
    recording_configs_request_receiver: crossbeam::channel::Receiver<()>,

    // Send-Recv Configs channel (BOUNDED):
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

    console_sender: crossbeam::channel::Sender<ConsoleMessage>,
    console_receiver: crossbeam::channel::Receiver<ConsoleMessage>,

    // THREAD HANDLING
    thread_handle_sender: crossbeam::channel::Sender<WhisperAppThread>,
}

impl WhisperAppContext {
    fn new(
        audio_wrapper: Arc<SdlAudioWrapper>,
        system_theme: Option<eframe::Theme>,
        thread_handle_sender: crossbeam::channel::Sender<WhisperAppThread>,
    ) -> Self {
        let system_theme = Mutex::new(system_theme);

        // STATE
        let downloading = Arc::new(AtomicBool::new(false));
        let realtime_ready = Arc::new(AtomicBool::new(false));
        let static_ready = Arc::new(AtomicBool::new(false));

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

        // Recording
        let (record_audio_i16_sender, record_audio_i16_receiver) = crossbeam::channel::unbounded();
        // TODO: remove if no hound spt.
        let (record_audio_i32_sender, record_audio_i32_receiver) = crossbeam::channel::unbounded();
        let (record_audio_f32_sender, record_audio_f32_receiver) = crossbeam::channel::unbounded();

        // GUI
        let (transcription_text_sender, transcription_text_receiver) =
            crossbeam::channel::unbounded();
        let (progress_sender, progress_receiver) = crossbeam::channel::unbounded();
        let (console_sender, console_receiver) = crossbeam::channel::unbounded();

        // FFT BUFFER
        let fft_buffer = Mutex::new([0.0; constants::NUM_BUCKETS]);

        Self {
            system_theme,
            audio_wrapper,
            realtime_ready,
            static_ready,
            downloading,
            recorder_running,
            realtime_running,
            static_running,
            fft_buffer,
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
            console_sender,
            console_receiver,
            thread_handle_sender,
        }
    }
}

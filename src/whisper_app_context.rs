use std::sync::{Arc, mpsc};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvError, SendError, SyncSender, TryRecvError};
use std::thread;
use std::thread::ScopedJoinHandle;

use sdl2::audio::AudioDevice;
use whisper_realtime::audio_ring_buffer::AudioRingBuffer;
use whisper_realtime::configs::Configs;
use whisper_realtime::errors::{WhisperRealtimeError, WhisperRealtimeErrorType};
use whisper_realtime::microphone;
use whisper_realtime::recorder::Recorder;
use whisper_realtime::transcriber::transcriber::Transcriber;

use crate::utils::configs::{AudioWorkerType, RecorderConfigs, WhisperConfigType};
use crate::utils::constants;
use crate::utils::progress::Progress;

// Implement actor model.
// Call functions to obtain the required data as needed.
// Possibly pass some sort of messages...


#[derive(Clone)]
pub struct WhisperActor(Arc<ContextImpl>);

impl std::fmt::Debug for WhisperActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

// TODO: this will need to be refactored once Context Impl has been implemented.
impl Default for WhisperActor {
    fn default() -> Self {
        todo!()
        // let ctx_impl = ContextImpl {};
        //
        // Self(Arc::new(ctx_impl))
    }
}

// TODO: Determine how best to handle Sync + Send without resorting to mutex.
// Message queues are a little inflexible
impl WhisperActor {
    // STATE
    pub fn is_working(&self) -> bool {
        let realtime_running = self.realtime_running();
        let static_running = self.static_running();
        let recorder_running = self.static_running();
        let downloading = self.is_downloading();
        realtime_running || static_running || recorder_running || downloading
    }

    // READY
    pub fn realtime_ready(&self) -> bool {
        self.0.realtime_ready.load(Ordering::Acquire)
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
        self.0.realtime_ready.load(Ordering::Acquire)
    }

    pub fn realtime_running(&self) -> bool {
        self.0.realtime_running.load(Ordering::Acquire)
    }
    pub fn static_running(&self) -> bool {
        self.0.static_running.load(Ordering::Acquire)
    }
    // CONFIGS
    pub fn send_whisper_configs(&self, whisper_configs: Configs, whisper_config_type: WhisperConfigType) -> Result<(), SendError<(Configs, WhisperConfigType)>> {
        self.0.whisper_configs_sender.send((whisper_configs, whisper_config_type))
    }

    pub fn send_recording_configs(&self, recorder_configs: RecorderConfigs) -> Result<(), SendError<RecorderConfigs>> {
        self.0.recorder_configs_sender.send(recorder_configs)
    }


    // MSG HANDLING

    pub fn send_progress(&self, progress: Progress) -> Result<(), SendError<Progress>> {
        self.0.progress_sender.send(progress)
    }

    pub fn recv_progress(&self) -> Result<Progress, TryRecvError> {
        self.0.progress_receiver.try_recv()
    }

    fn realtime_audio_sender(&self) -> mpsc::SyncSender<Vec<f32>> {
        self.0.realtime_audio_sender.clone()
    }

    // TODO: this might be better as an internal function run on a thread.
    // TODO: This might actually be better to recv and block on a thread.
    fn receive_realtime_audio(&self) -> Result<Vec<f32>, RecvError> {
        self.0.realtime_audio_receiver.recv()
    }

    // TODO: determine whether to encapsulate or allow producer clone
    // These might be best left internal & handle threading + computation threads here.
    pub fn record_audio_i16_sender(&self) -> mpsc::SyncSender<Vec<i16>> {
        self.0.record_audio_i16_sender.clone()
    }

    pub fn receive_audio_i16(&self) -> Result<Vec<i16>, TryRecvError> {
        self.0.record_audio_i16_receiver.try_recv()
    }
    pub fn record_audio_i32_sender(&self) -> mpsc::SyncSender<Vec<i32>> {
        self.0.record_audio_i32_sender.clone()
    }

    pub fn receive_audio_i32(&self) -> Result<Vec<i32>, TryRecvError> {
        self.0.record_audio_i32_receiver.try_recv()
    }


    pub fn record_audio_f32_sender(&self) -> mpsc::SyncSender<Vec<f32>> {
        self.0.record_audio_f32_sender.clone()
    }

    pub fn receive_audio_f32(&self) -> Result<Vec<f32>, TryRecvError> {
        self.0.record_audio_f32_receiver.try_recv()
    }

    // This needs to be copied and a copy given to the RealtimeTranscriber struct.
    // TODO: possibly handle this internally.
    pub fn transcription_text_sender(&self) -> mpsc::Sender<Result<(String, bool), WhisperRealtimeError>> {
        self.0.transcription_text_sender.clone()
    }

    pub fn receive_transcription_text(&self) -> Result<Result<(String, bool), WhisperRealtimeError>, TryRecvError> {
        self.0.transcription_text_receiver.try_recv()
    }

    pub fn send_error(&self, error: WhisperRealtimeError) -> Result<(), SendError<WhisperRealtimeError>> {
        self.0.error_sender.send(error)
    }

    pub fn receive_error(&self) -> Result<WhisperRealtimeError, TryRecvError> {
        self.0.error_receiver.try_recv()
    }

    pub fn start_audio_worker(&mut self, worker_type: AudioWorkerType) -> Result<(), WhisperRealtimeError> {
        match worker_type {
            AudioWorkerType::REALTIME => self.start_realtime_transcription(),
            AudioWorkerType::STATIC => self.start_static_transcription(),
            AudioWorkerType::RECORDING => self.start_recording(),
        }
    }

    // TODO: Figure out how to implement a "Setup Progress message"
    fn start_realtime_transcription(&mut self) -> Result<(), WhisperRealtimeError> {
        let mut actor = self.clone();
        // Update state
        actor.0.realtime_running.store(true, Ordering::Release);

        // TODO: This should probably send a progress job.

        // Get the realtime configs.
        actor.0.realtime_configs_request_sender.send(()).expect("Realtime configs request channel already closed");

        thread::scope(|s| {
            s.spawn(|| {
                let configs = actor.get_requested_configs(WhisperConfigType::REALTIME);
                if let Err(e) = configs.clone() {
                    self.0.error_sender.send(e).expect("Error Channel closed");
                    self.0.realtime_running.store(false, Ordering::Release);
                    panic!();
                }
                let configs = configs.unwrap();

                let configs = Arc::new(configs);
                // TODO: figure out what the borrow checker is fussing over.
                // The setup functions might need to be refactored.
                let thread = actor.realtime_setup(configs);
                actor.0.active_threads.push(thread);
            })
        });
        Ok(())
    }


    // TODO: see rt fn for ideas on how to structure.
    pub fn start_static_transcription(&self) -> Result<(), WhisperRealtimeError> {
        todo!()
        // Load file
        // Setup
        // Store join-handles
        // Set state.
        // Progress = whisper callback progress.
    }

    pub fn start_recording(&self) -> Result<(), WhisperRealtimeError> {
        todo!()
        // Setup -> Write to tmp file.
        // Spawn threads
        // Store join-handles
        // Set state.
        // Progress = uh. Not sure.
    }

    // TODO: Come back to this & possibly rethink - is a possible bottleneck.
    fn get_requested_configs(&self, whisper_config_type: WhisperConfigType) -> Result<Configs, WhisperRealtimeError> {
        let start_time = std::time::Instant::now();
        let configs = thread::scope(|s| {
            let c = s.spawn(|| {
                let mut now = std::time::Instant::now();
                loop {
                    let try_for_configs = self.0.whisper_configs_receiver.try_recv();
                    match try_for_configs {
                        Ok(c) => {
                            let (conf, conf_type) = c;
                            if conf_type == whisper_config_type {}
                            match conf_type {
                                WhisperConfigType::REALTIME => {
                                    return conf;
                                }
                                WhisperConfigType::STATIC => {
                                    thread::sleep(constants::SLEEP_DURATION);
                                }
                            }
                        }
                        Err(e) => {
                            match e {
                                TryRecvError::Empty => {
                                    thread::sleep(constants::SLEEP_DURATION);
                                }
                                TryRecvError::Disconnected => {
                                    panic!("Config Channel has been closed")
                                }
                            }
                        }
                    }

                    // Timeout check.
                    now = std::time::Instant::now();
                    if now - start_time > constants::CHANNEL_TIMEOUT {
                        panic!("Timeout: failed to receive transcription configs")
                    }
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

    fn realtime_setup(&self, configs: Arc<Configs>) -> ScopedJoinHandle<String> {
        // Init model.
        let model = init_model(configs.clone());
        let c_model = model.clone();

        // Clone configs
        let c_configs = configs.clone();

        // Audio buffer.
        let audio = init_audio_buffer();
        let c_audio = audio.clone();

        // Text sender
        let c_text_sender = Arc::new(self.0.transcription_text_sender.clone());
        // Recording sender for tmp file
        let c_recording_sender = Arc::new(self.0.record_audio_f32_sender.clone());

        // State Flags - This should likely be refactored.
        let c_realtime_is_ready = self.0.realtime_ready.clone();
        let c_realtime_is_running = self.0.realtime_running.clone();
        let mic_stream = init_microphone(&self.0.sdl_ctx, self.0.realtime_audio_sender.clone(), c_realtime_is_running.clone());
        let c_mic_stream = mic_stream.clone();

        // Init Whisper.
        let ctx = init_whisper_ctx(c_model.clone(), c_configs.use_gpu);
        let mut state = ctx.create_state().expect("Failed to create WhisperState");

        // TODO: consider importing my library to reduce the typing.
        let transcriber_thread = thread::scope(|s| {
            let c_realtime_is_running_audio = c_realtime_is_running.clone();
            let c_realtime_is_running_write = c_realtime_is_running.clone();
            c_mic_stream.resume();

            let _write_thread = s.spawn(move || {
                loop {
                    if !c_realtime_is_running_write.load(Ordering::Acquire) {
                        break;
                    }
                    let output = self.receive_audio_f32();
                    assert!(output.is_ok(), "F32 Audio Channel Closed");
                    // TODO: Write out to the temporary file.
                    // TODO: Function for writing to the temporary file.
                }
            });

            let _audio_thread = s.spawn(move || {
                loop {
                    if !c_realtime_is_running_audio.load(Ordering::Acquire) {
                        break;
                    }

                    let output = self.receive_realtime_audio();
                    assert!(output.is_ok(), "Realtime Audio Channel closed");

                    let mut audio_data = output.unwrap();
                    c_audio.push_audio(&mut audio_data);
                    // WRITE DATA TO THE TEMPORARY BUFFER.
                    // PROPAGATE TO THE RECORDER
                    let write_propagation = c_recording_sender.send(audio_data.clone());
                    assert!(write_propagation.is_ok(), "F32 Audio Channel closed");
                }
                c_mic_stream.pause();
            });
            let transcription_thread = s.spawn(move || {
                let mut transcriber = whisper_realtime::transcriber::realtime_transcriber::RealtimeTranscriber::new_with_configs(
                    c_audio,
                    c_text_sender,
                    c_realtime_is_running.clone(),
                    c_realtime_is_ready.clone(),
                    c_configs.clone(),
                    None,
                );
                let output = transcriber.process_audio(&mut state, None::<fn(i32)>);
                output
            });
            transcription_thread
        });
        transcriber_thread
    }
}

// TODO: UTILITY FUNCTIONS FILE
// TODO: FN FOR INITIALIZING TEMP FILE PTR.

fn init_audio_buffer() -> Arc<AudioRingBuffer<f32>> {
    let audio: AudioRingBuffer<f32> = AudioRingBuffer::new(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    Arc::new(audio)
}

fn init_microphone(sdl_ctx: &sdl2::Sdl, audio_sender: SyncSender<Vec<f32>>, is_running: Arc<AtomicBool>) -> Arc<AudioDevice<Recorder<f32>>> {
    let audio_subsystem = sdl_ctx.audio().expect("Failed to initialize audio");

    let desired_audio_spec = microphone::get_desired_audio_spec(
        Some(whisper_realtime::constants::WHISPER_SAMPLE_RATE as i32),
        Some(1),
        Some(1024),
    );

    // Setup
    let mic_stream: AudioDevice<Recorder<f32>> = microphone::build_audio_stream(
        &audio_subsystem,
        &desired_audio_spec,
        audio_sender,
        is_running,
    );

    Arc::new(mic_stream)
}

fn init_model(configs: Arc<Configs>) -> Arc<whisper_realtime::model::Model> {
    let model_type = configs.model;
    let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get Data dir");
    let model = whisper_realtime::model::Model::new_with_type_and_dir(model_type, data_dir);
    assert!(model.is_downloaded(), "Model not downloaded");
    Arc::new(model)
}

fn init_whisper_ctx(model: Arc<whisper_realtime::model::Model>, use_gpu: bool) -> whisper_realtime::whisper_rs::WhisperContext {
    let mut whisper_ctx_params = whisper_realtime::whisper_rs::WhisperContextParameters::default();
    whisper_ctx_params.use_gpu = use_gpu;
    let model_path = model.file_path();
    let model_path = model_path.as_path();
    whisper_realtime::whisper_rs::WhisperContext::new_with_params(
        model_path.to_str().expect("Failed to stringify path"),
        whisper_ctx_params,
    ).expect("Failed to load model")
}

// TODO: HANDLE SYNC AND SEND -> Guard internally and then implement send + safe
// TODO: Refactor to crossbeam.
struct ContextImpl {
    // SDL
    sdl_ctx: sdl2::Sdl,

    //

    // STATE
    // Worker flag to force the app to repaint continuously.

    downloading: Arc<AtomicBool>,
    realtime_ready: Arc<AtomicBool>,
    static_ready: Arc<AtomicBool>,
    recorder_ready: Arc<AtomicBool>,

    // Recorder flags.
    recorder_running: Arc<AtomicBool>,
    realtime_running: Arc<AtomicBool>,
    static_running: Arc<AtomicBool>,

    progress_sender: mpsc::SyncSender<Progress>,
    progress_receiver: mpsc::Receiver<Progress>,

    // Configs Channels:
    // Realtime
    realtime_configs_request_sender: SyncSender<()>,
    realtime_configs_request_receiver: mpsc::SyncSender<()>,

    // Static
    static_configs_request_sender: mpsc::SyncSender<()>,
    static_configs_request_receiver: mpsc::SyncSender<()>,

    // Recorder
    recorder_configs_request_sender: mpsc::SyncSender<()>,
    recorder_configs_request_receiver: mpsc::SyncSender<()>,

    recorder_configs_sender: mpsc::Sender<RecorderConfigs>,
    recorder_configs_receiver: mpsc::Receiver<RecorderConfigs>,

    // Send-Recv Configs channel.
    whisper_configs_sender: mpsc::Sender<(Configs, WhisperConfigType)>,
    whisper_configs_receiver: mpsc::Receiver<(Configs, WhisperConfigType)>,

    // Realtime audio channel for collecting input from sdl.
    realtime_audio_sender: mpsc::SyncSender<Vec<f32>>,
    realtime_audio_receiver: mpsc::Receiver<Vec<f32>>,

    // Recording channels - This feature might get dropped.
    record_audio_i16_sender: mpsc::SyncSender<Vec<i16>>,
    record_audio_i16_receiver: mpsc::Receiver<Vec<i16>>,

    // I'm not 100% sure whether I can do this with hound.
    record_audio_i32_sender: mpsc::SyncSender<Vec<i32>>,
    record_audio_i32_receiver: mpsc::Receiver<Vec<i32>>,

    record_audio_f32_sender: mpsc::SyncSender<Vec<f32>>,
    record_audio_f32_receiver: mpsc::Receiver<Vec<f32>>,

    // Transcription channel for passing text output
    transcription_text_sender: mpsc::Sender<Result<(String, bool), WhisperRealtimeError>>,
    transcription_text_receiver: mpsc::Receiver<Result<(String, bool), WhisperRealtimeError>>,

    // Error channel for passing errors to error window.
    error_sender: mpsc::Sender<WhisperRealtimeError>,
    error_receiver: mpsc::Receiver<WhisperRealtimeError>,

    // Thread vector to internally manage active threads.
    // TODO: change to hashmap.
    // TODO: Also: Wrap this in a mutex.
    active_threads: Vec<ScopedJoinHandle<'static, String>>,
}
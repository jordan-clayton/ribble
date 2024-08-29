use std::{
    any::Any,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering}, Mutex, TryLockError,
    },
    thread::{self, JoinHandle},
};
use std::any::TypeId;
use std::path::Path;

use crossbeam::channel::{
    bounded, Receiver, RecvError, Sender, SendError, TryRecvError, unbounded,
};
use hound::{Sample, SampleFormat, WavSpec};
#[cfg(feature = "cuda")]
use nvml_wrapper::{cuda_driver_version_major, cuda_driver_version_minor};
use realfft::num_traits::{Bounded, FromPrimitive, NumCast, Zero};
use sdl2::audio::AudioSpecDesired;
use tokio::runtime::Handle;
use whisper_realtime::{
    downloader::{
        download::AsyncDownload,
        request::{async_download_request, reqwest},
    },
    errors::WhisperRealtimeError,
    microphone,
    transcriber::{realtime_transcriber::RealtimeTranscriber, transcriber::Transcriber},
};
use whisper_realtime::transcriber::static_transcriber::{
    StaticTranscriber, SupportedAudioSample, SupportedChannels,
};

use crate::{
    controller::utils::transcriber_utilities::{
        init_audio_ring_buffer, init_model, init_realtime_microphone, init_whisper_ctx,
    },
    utils::{
        configs::{AudioConfigs, AudioConfigType, WorkerType},
        console_message::{ConsoleMessage, ConsoleMessageType},
        constants,
        errors::{WhisperAppError, WhisperAppErrorType},
        file_mgmt::{
            copy_data, get_temp_file_path, get_tmp_file_writer, save_transcription,
            write_audio_sample,
        },
        progress::Progress,
        sdl_audio_wrapper::SdlAudioWrapper,
    },
};
use crate::controller::utils::transcriber_utilities::init_microphone;
use crate::utils::configs::{RecorderConfigs, RecordingFormat};
use crate::utils::file_mgmt::{decode_audio, get_audio_reader};
use crate::utils::recording::{
    bandpass_filter, f_central, frequency_analysis, from_f32_normalized, to_f32_normalized,
};

// TODO: Remaining impls;

#[derive(Clone)]
pub struct WhisperAppController(Arc<WhisperAppContext>);

impl std::fmt::Debug for WhisperAppController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller")
            .field("Context", &self.0)
            .finish()
    }
}

impl WhisperAppController {
    pub fn new(
        client: reqwest::Client,
        handle: Handle,
        audio_wrapper: Arc<SdlAudioWrapper>,
        system_theme: Option<eframe::Theme>,
        thread_handle_sender: Sender<WhisperAppThread>,
    ) -> Self {
        let app_ctx = WhisperAppContext::new(
            client,
            handle,
            audio_wrapper,
            system_theme,
            thread_handle_sender,
        );
        let app_ctx = Arc::new(app_ctx);
        Self(app_ctx)
    }

    pub fn gpu_enabled(&self) -> bool {
        self.0.gpu_support.load(Ordering::Relaxed)
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
        let recorder_running = self.recorder_running();
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

    pub fn save_recording_ready(&self) -> bool {
        self.0.save_recording_ready.load(Ordering::Acquire)
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

    pub fn stop_transcriber(&self, realtime: bool) {
        if realtime {
            self.0.realtime_running.store(false, Ordering::Release);
        } else {
            self.0.static_running.store(false, Ordering::Release);
        }
    }

    pub fn stop_recording(&self) {
        self.0.recorder_running.store(false, Ordering::Release);
    }

    // CONFIGS

    pub fn send_configs(&self, configs: AudioConfigs) -> Result<(), SendError<AudioConfigs>> {
        self.0.configs_sender.send(configs)
    }

    // MSG HANDLING
    // CONFIGS REQUESTS
    pub fn recv_realtime_configs_req(&self) -> Result<(), TryRecvError> {
        self.0.realtime_configs_request_receiver.try_recv()
    }
    pub fn recv_static_configs_req(&self) -> Result<(), TryRecvError> {
        self.0.static_configs_request_receiver.try_recv()
    }
    pub fn recv_recording_configs_req(&self) -> Result<(), TryRecvError> {
        self.0.recording_configs_request_receiver.try_recv()
    }

    pub fn send_progress(&self, progress: Progress) -> Result<(), SendError<Progress>> {
        self.0.progress_sender.send(progress)
    }

    pub fn recv_progress(&self) -> Result<Progress, TryRecvError> {
        self.0.progress_receiver.try_recv()
    }
    pub fn send_console_message(
        &self,
        msg: ConsoleMessage,
    ) -> Result<(), SendError<ConsoleMessage>> {
        self.0.console_sender.send(msg)
    }

    pub fn recv_console_message(&self) -> Result<ConsoleMessage, TryRecvError> {
        self.0.console_receiver.try_recv()
    }

    // fn receive_audio_i16(&self) -> Result<Vec<i16>, RecvError> {
    //     self.0.record_audio_i16_receiver.recv()
    // }
    //
    // fn receive_audio_i32(&self) -> Result<Vec<i32>, RecvError> {
    //     self.0.record_audio_i32_receiver.recv()
    // }

    fn receive_audio_f32(&self) -> Result<Vec<f32>, RecvError> {
        self.0.record_audio_f32_receiver.recv()
    }

    // TODO: faster, better, solution. Internal context cannot be mutated without RWLocking, defeating the purpose of a msg queue.
    fn clear_audio_f32(&self) {
        while let Ok(_) = self.0.record_audio_f32_receiver.try_recv() {
            continue;
        }
    }

    fn clear_audio_i32(&self) {
        while let Ok(_) = self.0.record_audio_i32_receiver.try_recv() {
            continue;
        }
    }

    fn clear_audio_i16(&self) {
        while let Ok(_) = self.0.record_audio_i16_receiver.try_recv() {
            continue;
        }
    }

    // This needs to be copied and a copy given to the RealtimeTranscriber struct.
    // TODO: possibly handle this internally.
    pub fn transcription_text_sender(
        &self,
    ) -> Sender<Result<(String, bool), WhisperRealtimeError>> {
        self.0.transcription_text_sender.clone()
    }

    pub fn recv_transcription_text(
        &self,
    ) -> Result<Result<(String, bool), WhisperRealtimeError>, TryRecvError> {
        self.0.transcription_text_receiver.try_recv()
    }

    // pub fn write_fft_buffer(&self, new_fft: &[f32; constants::NUM_BUCKETS]) {
    //     let mut guard = self.0.fft_buffer.lock().unwrap();
    //     guard.copy_from_slice(new_fft);
    // }

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

    pub fn send_thread_handle(
        &self,
        thread: WhisperAppThread,
    ) -> Result<(), SendError<WhisperAppThread>> {
        self.0.thread_handle_sender.send(thread)
    }

    // TODO: try and get rid of the need for ctx -> Should be called anyway.
    pub fn start_realtime_transcription(&mut self, ctx: &egui::Context) {
        let job_name = "Realtime Setup";

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 1, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        let ctx = ctx.clone();
        // Update state
        self.0.realtime_running.store(true, Ordering::Release);
        self.0.save_recording_ready.store(false, Ordering::Release);

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 17, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        // Get the realtime configs.
        self.0
            .realtime_configs_request_sender
            .send(())
            .expect("Realtime configs request channel already closed");
        let controller = self.clone();
        let c_controller = controller.clone();

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 33, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        let rt_thread = thread::spawn(move || {
            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(job_name), 50, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            // This spawns a scoped thread to poll the msg queue and get the correct configurations.
            // TODO: REFACTOR -> THIS DOES NOT NEED TO BE ON A SEPARATE THREAD.
            let configs =
                get_requested_configs(c_controller.clone(), AudioConfigType::Realtime, ctx);
            if let Err(e) = &configs {
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_controller
                    .0
                    .realtime_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            }
            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(job_name), 76, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            let configs = configs.unwrap();

            let AudioConfigs::Realtime(confs) = configs else {
                let e = WhisperAppError::new(WhisperAppErrorType::ParameterError, String::from("Invalid configs provided to realtime stream. Either invalid data passed, or data race condition"));
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_controller
                    .0
                    .realtime_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            };

            let configs = Arc::new(confs);

            // Clear the transcription buffer.
            c_controller
                .0
                .transcription_text_sender
                .send(Ok((String::from(constants::CLEAR_MSG), true)))
                .expect("Transcription channel closed");

            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(job_name), 100, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            // Run
            run_realtime_audio_transcription(c_controller.clone(), configs)
        });

        let thread = (WorkerType::Realtime, rt_thread);

        // Send to the background controller to join.
        self.send_thread_handle(thread)
            .expect("Thread channel closed");
    }

    pub fn start_static_transcription(&self, audio_file: &Path, ctx: &egui::Context) {
        let job_name = "Static Setup";
        let audio_file = audio_file.to_path_buf();

        let progress = Progress::new(String::from(job_name), 1, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        let ctx = ctx.clone();
        self.0.static_running.store(true, Ordering::Release);

        let progress = Progress::new(String::from(job_name), 10, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        // Send for static configs
        self.0
            .static_configs_request_sender
            .send(())
            .expect("Static configs request channel closed");

        let controller = self.clone();
        let c_controller = controller.clone();
        let progress = Progress::new(String::from(job_name), 20, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        let st_thread = thread::spawn(move || {
            let audio_file = audio_file.as_path();
            let progress = Progress::new(String::from(job_name), 30, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            // Get the static configs.
            let configs = get_requested_configs(c_controller.clone(), AudioConfigType::Static, ctx);

            let progress = Progress::new(String::from(job_name), 50, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            // TODO: possibly factor out into a function.
            if let Err(e) = &configs {
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_controller
                    .0
                    .static_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            }

            let configs = configs.unwrap();

            // Ensure configs type
            let AudioConfigs::Static(confs) = configs else {
                let e = WhisperAppError::new(WhisperAppErrorType::ParameterError, String::from("Invalid configs provided to static stream. Either invalid data passed, or data race condition"));
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_controller
                    .0
                    .static_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            };

            // Wrap for static transcriber.
            let configs = Arc::new(confs);

            // Clear the transcription buffer.
            c_controller
                .0
                .transcription_text_sender
                .send(Ok((String::from(constants::CLEAR_MSG), true)))
                .expect("Transcription channel closed");

            // Load the file & decode.
            let audio_reader = get_audio_reader(audio_file);
            if let Err(e) = &audio_reader {
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_controller
                    .0
                    .static_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            }

            let (id, format, decoder) = audio_reader.unwrap();
            let progress = Progress::new(String::from(job_name), 60, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            // Second progress job -> decode audio.
            let decode_job_name = "Decoding audio";
            let total_size = if let Ok(m) = std::fs::metadata(audio_file) {
                m.len() as usize
            } else {
                0
            };

            let decoder_progress_callback = |total_decoded| {
                let progress =
                    Progress::new(String::from(decode_job_name), total_decoded, total_size);
                c_controller
                    .send_progress(progress)
                    .expect("Progress channel closed");
            };

            // decode
            let decode_success = decode_audio(id, format, decoder, Some(decoder_progress_callback));

            // UPDATE PROGRESS
            let progress = Progress::new(String::from(job_name), 80, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            if let Err(e) = &decode_success {
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                c_controller
                    .0
                    .static_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            }

            let decoded = decode_success.unwrap();

            // Final progress if not able to get file size from metadata.
            if total_size == 0 {
                let progress = Progress::new(String::from(decode_job_name), 0, total_size);
                c_controller
                    .send_progress(progress)
                    .expect("Progress channel closed");

                // In case of inexact sizes: progress task will be removed on next ui draw.
            } else if total_size != decoded.len() {
                let progress = Progress::new(String::from(decode_job_name), 1, 1);
                c_controller
                    .send_progress(progress)
                    .expect("Progress channel closed");
            }

            assert!(!decoded.is_empty(), "Invalid file: 0-size audio");

            let progress = Progress::new(String::from(job_name), 100, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");
            run_static_audio_transcription(decoded, c_controller.clone(), configs)
        });

        let worker = (WorkerType::Static, st_thread);
        self.send_thread_handle(worker)
            .expect("Thread channel closed");
    }

    // TODO: add progress
    pub fn start_recording(&self, run_fft: Arc<AtomicBool>, ctx: &egui::Context) {
        let job_name = "Recorder Setup";
        // Update state.
        self.0.recorder_running.store(true, Ordering::Release);
        self.0.save_recording_ready.store(false, Ordering::Release);

        let ctx = ctx.clone();

        let controller = self.clone();

        // Send a request for configs.
        self.0
            .recording_configs_request_sender
            .send(())
            .expect("Recording configs request channel closed");

        let rec_thread = thread::spawn(move || {
            let configs =
                get_requested_configs(controller.clone(), AudioConfigType::Recording, ctx);

            if let Err(e) = &configs {
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                controller
                    .0
                    .recorder_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            }

            let AudioConfigs::Recording(confs) = configs.unwrap() else {
                let e = WhisperAppError::new(WhisperAppErrorType::ParameterError, String::from("Invalid configs provided to recording stream. Either invalid data passed, or data race condition"));
                let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                controller
                    .0
                    .recorder_running
                    .store(false, Ordering::Release);
                panic!("{}", msg);
            };

            run_recording(controller.clone(), confs, run_fft);

            Ok(String::from("Finished Recording."))
        });

        let worker = (WorkerType::Recording, rec_thread);
        self.send_thread_handle(worker)
            .expect("Thread channel closed");
    }

    pub fn start_download(&self, url: String, file_name: String, file_directory: PathBuf) {
        let c_file_name = file_name.clone();

        let c_file_directory = file_directory.clone();

        let controller = self.clone();
        let c_controller = controller.clone();
        let download_thread = thread::spawn(move || {
            let client = controller.0.client.clone();
            let handle = controller.0.handle.clone();
            let stream_downloader = async_download_request(&client, url.as_str(), None);

            controller.0.downloading.store(true, Ordering::Release);

            let file_directory = c_file_directory.as_path();
            let stream = handle.block_on(stream_downloader);
            if let Err(e) = stream.as_ref() {
                controller.0.downloading.store(false, Ordering::Release);
                panic!("{}", e);
            }

            let mut stream = stream.unwrap();

            let total_size = stream.total_size;

            let job_name = format!("Downloading: {}", c_file_name);

            stream.progress_callback = Some(move |n| {
                let progress = Progress::new(job_name.clone(), n, total_size);
                let sent = c_controller.send_progress(progress);
                if let Err(e) = sent {
                    c_controller.0.downloading.store(false, Ordering::Release);
                    panic!("{}", e);
                }
            });

            let download = stream.download(file_directory, c_file_name.as_str());
            let success = handle.block_on(download);
            controller.0.downloading.store(false, Ordering::Release);

            if let Err(e) = success {
                panic!("{}", e);
            }
            Ok(format!(
                "Model: {}, successfully downloaded to {:?}",
                file_name,
                c_file_directory.as_os_str()
            ))
        });

        let worker = (WorkerType::Downloading, download_thread);

        self.send_thread_handle(worker)
            .expect("Thread channel closed");
    }

    pub fn save_audio_recording(&self, output_path: &PathBuf) {
        let to = output_path.to_path_buf();
        let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get storage dir");
        let tmp_file_path = get_temp_file_path(data_dir.as_path());
        assert!(tmp_file_path.exists(), "Audio buffer file missing!");
        let from = tmp_file_path.clone();
        let copy_thread = thread::spawn(move || {
            let success = copy_data(&from, &to);
            match success {
                Ok(_) => Ok(format!("File: {:?} saved successfully.", to.file_stem())),
                Err(e) => {
                    panic!("{}", e)
                }
            }
        });

        let worker = (WorkerType::Saving, copy_thread);
        self.send_thread_handle(worker)
            .expect("Thread channel closed.");
    }

    pub fn save_transcription(&self, output_path: &PathBuf, transcription: &[String]) {
        let p = output_path.clone();
        let transcription_text = transcription.join("\n");
        let c_controller = self.clone();
        let save_thread = thread::spawn(move || {
            let job_name = format!("Saving: {:?}", p.file_name());
            let total_size = transcription_text.len();

            let file_name = p.file_stem().expect("Invalid filename");
            let directory = p.parent().expect("Failed to get saving directory");
            let directory = directory.as_os_str();

            let progress_callback = move |n: usize| {
                let progress = Progress::new(job_name.clone(), n, total_size);
                c_controller
                    .send_progress(progress)
                    .expect("Progress channel closed.");
            };

            let write_success =
                save_transcription(p.as_path(), &transcription_text, Some(progress_callback));
            match write_success {
                Ok(_) => Ok(format!("{:?} saved to {:?}", file_name, directory)),
                Err(e) => {
                    panic!("{}", e)
                }
            }
        });

        let worker = (WorkerType::Saving, save_thread);
        self.send_thread_handle(worker)
            .expect("Thread channel closed.")
    }
}

// TODO: refactor this to recv_timeout.
fn get_requested_configs(
    controller: WhisperAppController,
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
                let try_for_configs = controller.0.configs_receiver.try_recv();
                if let Err(e) = &try_for_configs {
                    match e {
                        TryRecvError::Empty => {
                            thread::sleep(constants::SLEEP_DURATION);
                            continue;
                        }
                        TryRecvError::Disconnected => {
                            panic!("Config Channel has been closed")
                        }
                    }
                }
                if !try_for_configs.is_ok() {
                    panic!("{:?}", try_for_configs);
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

fn recording_impl<
    T: Default
    + Clone
    + Copy
    + FromPrimitive
    + NumCast
    + Bounded
    + Zero
    + sdl2::audio::AudioFormatNum
    + Sample
    + Sync
    + Send
    + 'static,
>(
    controller: WhisperAppController,
    run_fft: Arc<AtomicBool>,
    desired_audio: &AudioSpecDesired,
    spec: WavSpec,
    channel: (Sender<Vec<T>>, Receiver<Vec<T>>),
    filter: bool,
    f_central: f32,
) {
    let c_controller_write = controller.clone();
    let c_controller_fft = controller.clone();
    let audio_subsystem = &controller.0.audio_wrapper.audio_subsystem;

    let (sender, receiver) = channel;

    let mic = init_microphone(audio_subsystem, &desired_audio, sender);

    let audio_spec = mic.spec().clone();

    let channels = audio_spec.channels as u16;
    let sample_rate = (audio_spec.freq as i16) as u32;
    let mut spec = spec;
    spec.channels = channels;
    spec.sample_rate = sample_rate;

    // TODO: these need to be refactored to reset the running flag.
    let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data dir");
    let mut writer = get_tmp_file_writer(data_dir.as_path(), &spec).expect("Failed to open writer");

    // FFT channel.
    let (fft_s, fft_r) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);

    mic.resume();
    // TODO: add some way to flag the gui.

    let _ = thread::scope(|s| {
        let _write_thread = s.spawn(|| {
            loop {
                if !c_controller_write
                    .0
                    .recorder_running
                    .load(Ordering::Acquire)
                {
                    writer.finalize().expect("Failed to close writer.");
                    c_controller_write
                        .0
                        .save_recording_ready
                        .store(true, Ordering::Release);
                    break;
                }
                let output = receiver.recv();
                assert!(output.is_ok(), "Recording channel closed");
                let mut output = output.unwrap();

                // Filter audio (if desired)
                if filter {
                    let f_sample = sample_rate as f32;
                    let len = output.len();
                    let mut float_audio = vec![0.0; len];
                    to_f32_normalized(&output, &mut float_audio);
                    bandpass_filter(&mut float_audio, f_sample, f_central);
                    from_f32_normalized(&float_audio, &mut output);
                }

                // Propagate the data to the fft
                if run_fft.load(Ordering::Acquire) {
                    let fft_propagation = fft_s.send(output.clone());
                    assert!(fft_propagation.is_ok(), "FFT Channel Closed");
                }

                write_audio_sample(&output, &mut writer, None::<fn(usize)>);
            }
        });

        let _fft_thread = s.spawn(|| {
            loop {
                if !c_controller_fft.0.recorder_running.load(Ordering::Acquire) {
                    break;
                }

                let output = fft_r.recv();
                assert!(output.is_ok(), "I16 FFT Channel Closed");
                let output = output.unwrap();
                // TODO: profile this -> It might be just as fast/free to cast_to_f32 safely
                if TypeId::of::<T>() == TypeId::of::<f32>() {
                    let output = unsafe {
                        let mut out = std::mem::ManuallyDrop::new(output);
                        Vec::from_raw_parts(out.as_mut_ptr() as *mut f32, out.len(), out.capacity())
                    };
                    let mut guard = c_controller_fft
                        .0
                        .fft_buffer
                        .lock()
                        .expect("Poisoned fft buffer");
                    frequency_analysis(&output, &mut guard, sample_rate as f64);
                    debug_assert!(
                        guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                        "Failed to normalize"
                    )
                } else {
                    let len = output.len();
                    let mut fft_data = vec![0.0; len];
                    to_f32_normalized(&output, &mut fft_data);

                    let mut guard = c_controller_fft
                        .0
                        .fft_buffer
                        .lock()
                        .expect("Poisoned fft buffer");
                    frequency_analysis(&fft_data, &mut guard, sample_rate as f64);
                    debug_assert!(
                        guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                        "Failed to normalize"
                    )
                }
            }
        });
    });
    mic.pause();
}

fn run_recording(
    controller: WhisperAppController,
    configs: RecorderConfigs,
    run_fft: Arc<AtomicBool>,
) {
    let sample_rate_request = configs.sample_rate.sample_rate();
    let channels_request = configs.channel.num_channels();
    let buffer_size_request = configs.buffer_size.size();
    let desired_audio = microphone::get_desired_audio_spec(
        sample_rate_request,
        channels_request,
        buffer_size_request,
    );

    let filter = configs.filter;
    let f_central = f_central(configs.f_lower, configs.f_higher);
    // Get the audio channel.
    match configs.format {
        RecordingFormat::I16 => {
            let sender = controller.0.record_audio_i16_sender.clone();
            let receiver = controller.0.record_audio_i16_receiver.clone();
            let spec = WavSpec {
                channels: 2,
                sample_rate: 41000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            };
            recording_impl(
                controller.clone(),
                run_fft,
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );
            controller.clear_audio_i16();
        }
        RecordingFormat::I32 => {
            let sender = controller.0.record_audio_i32_sender.clone();
            let receiver = controller.0.record_audio_i32_receiver.clone();
            let spec = WavSpec {
                channels: 2,
                sample_rate: 41000,
                bits_per_sample: 32,
                sample_format: SampleFormat::Int,
            };
            recording_impl(
                controller.clone(),
                run_fft,
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );
            controller.clear_audio_i32();
        }
        RecordingFormat::F32 => {
            let sender = controller.0.record_audio_f32_sender.clone();
            let receiver = controller.0.record_audio_f32_receiver.clone();
            let spec = WavSpec {
                channels: 2,
                sample_rate: 41000,
                bits_per_sample: 32,
                sample_format: SampleFormat::Float,
            };
            recording_impl(
                controller.clone(),
                run_fft,
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );
            controller.clear_audio_f32();
        }
    }
}

// TODO: add a progress job to this setup.
fn run_realtime_audio_transcription(
    controller: WhisperAppController,
    configs: Arc<whisper_realtime::configs::Configs>,
) -> Result<String, Box<dyn Any + Send>> {
    // Clone the controller
    let c_controller_audio = controller.clone();
    let c_controller_write = controller.clone();

    // Init model.
    let model = init_model(configs.clone());
    let c_model = model.clone();

    // Clone configs
    let c_configs = configs.clone();

    // Audio buffer.
    let audio = init_audio_ring_buffer(Some(whisper_realtime::constants::INPUT_BUFFER_CAPACITY));
    let c_audio_reader = audio.clone();
    let c_audio_transcriber = audio.clone();

    // Text sender
    let c_text_sender = controller.0.transcription_text_sender.clone();
    // Recording sender for writing to tmp file
    let (s, r) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    let audio_write_sender: Sender<Vec<f32>> = s.clone();
    let audio_write_receiver = r.clone();

    // State Flags - This should likely be refactored.
    let c_realtime_is_ready = controller.0.realtime_ready.clone();
    let c_realtime_is_running = controller.0.realtime_running.clone();

    let mic_stream = init_realtime_microphone(
        &controller.0.audio_wrapper.audio_subsystem,
        controller.0.record_audio_f32_sender.clone(),
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

            // TODO: these need to reset the running flag before panicking.
            let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data dir");
            let mut writer =
                get_tmp_file_writer(data_dir.as_path(), &spec).expect("Failed to open writer.");

            loop {
                if !c_realtime_is_running_write_thread.load(Ordering::Acquire) {
                    writer.finalize().expect("Failed to close writer.");
                    // Update state - can save recording.
                    c_controller_write
                        .0
                        .save_recording_ready
                        .store(true, Ordering::Release);

                    break;
                }
                let output = audio_write_receiver.recv();
                assert!(output.is_ok(), "F32 Audio Channel Closed");
                let output = output.unwrap();
                write_audio_sample(&output, &mut writer, None::<fn(usize)>);
            }

            // CLEAR THE MSG QUEUE -> This runs a loop until the channel is empty.
            c_controller_write.clear_audio_f32();
        });

        let _audio_thread = s.spawn(move || {
            loop {
                if !c_realtime_is_running_audio_read_thread.load(Ordering::Acquire) {
                    break;
                }

                let output = c_controller_audio.receive_audio_f32();
                assert!(output.is_ok(), "Realtime Audio Channel closed");

                let mut audio_data = output.unwrap();

                // NOTE: This blocks.
                c_audio_reader.push_audio(&mut audio_data);

                // Propagate the data to the writing thread.
                let write_propagation = audio_write_sender.send(audio_data.clone());
                assert!(write_propagation.is_ok(), "F32 Write Channel closed");
            }

            // CLEAR THE MSG QUEUE -> this runs a loop to eat msgs until the channel is fully cleared
            // or has no producers.
            c_controller_audio.clear_audio_f32();
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

// TODO: add progress job.
fn run_static_audio_transcription(
    audio: Vec<f32>,
    controller: WhisperAppController,
    configs: Arc<whisper_realtime::configs::Configs>,
) -> Result<String, Box<dyn Any + Send>> {
    let model = init_model(configs.clone());
    let audio = SupportedAudioSample::F32(audio);
    let audio = Arc::new(Mutex::new(audio));

    let data_sender = controller.0.transcription_text_sender.clone();
    let data_sender = Some(data_sender);
    let channels = SupportedChannels::MONO;

    // Init Whisper.
    let ctx = init_whisper_ctx(model.clone(), configs.use_gpu);
    let mut state = ctx.create_state().expect("Failed to create WhisperState");
    let mut transcriber =
        StaticTranscriber::new_with_configs(audio, data_sender, configs, channels);

    // Progress callback.
    let p_controller = controller.clone();
    let transcriber_job_name = "Transcribing Audio";
    let total_size = 100;

    // NTS: possibly move memory.
    let progress_callback = move |n: i32| {
        let progress = Progress::new(String::from(transcriber_job_name), n as usize, total_size);
        p_controller
            .send_progress(progress)
            .expect("Progress channel closed");
    };

    let progress_callback = Some(progress_callback);

    let transcription = transcriber.process_audio(&mut state, progress_callback);
    Ok(transcription)
}

type WhisperAppThread = (WorkerType, JoinHandle<Result<String, Box<dyn Any + Send>>>);

// TODO: refactor & remove configs msg queues. Config windows don't paint if not seen leading to timeouts.
// TODO: add Atomic Enum for fft visualization types.
#[derive(Debug)]
struct WhisperAppContext {
    // GPU AVAILABLE
    gpu_support: Arc<AtomicBool>,
    // ASYNC (DOWNLOADS)
    client: reqwest::Client,
    handle: Handle,
    // SYSTEM THEME
    system_theme: Mutex<Option<eframe::Theme>>,
    // SDL AUDIO
    audio_wrapper: Arc<SdlAudioWrapper>,
    // STATE
    realtime_ready: Arc<AtomicBool>,
    static_ready: Arc<AtomicBool>,
    save_recording_ready: Arc<AtomicBool>,

    // WORKER FLAGS
    downloading: Arc<AtomicBool>,
    // RECORDER FLAGS.
    recorder_running: Arc<AtomicBool>,
    realtime_running: Arc<AtomicBool>,
    static_running: Arc<AtomicBool>,

    // FFT buffer
    fft_buffer: Mutex<[f32; constants::NUM_BUCKETS]>,
    // [REMOVE]
    // Configs Channels (BOUNDED):
    // Request configs
    realtime_configs_request_sender: Sender<()>,
    realtime_configs_request_receiver: Receiver<()>,

    // [REMOVE]
    static_configs_request_sender: Sender<()>,
    static_configs_request_receiver: Receiver<()>,

    // [REMOVE]
    recording_configs_request_sender: Sender<()>,
    recording_configs_request_receiver: Receiver<()>,

    // [REMOVE]
    // Send-Recv Configs channel (BOUNDED):
    configs_sender: Sender<AudioConfigs>,
    configs_receiver: Receiver<AudioConfigs>,

    // Recording channels (BOUNDED):
    record_audio_i16_sender: Sender<Vec<i16>>,
    record_audio_i16_receiver: Receiver<Vec<i16>>,

    // TODO: Remove if no wav support.
    record_audio_i32_sender: Sender<Vec<i32>>,
    record_audio_i32_receiver: Receiver<Vec<i32>>,

    record_audio_f32_sender: Sender<Vec<f32>>,
    record_audio_f32_receiver: Receiver<Vec<f32>>,

    // GUI CHANNELS (UNBOUNDED):
    // Transcription channel for passing text output
    transcription_text_sender: Sender<Result<(String, bool), WhisperRealtimeError>>,
    transcription_text_receiver: Receiver<Result<(String, bool), WhisperRealtimeError>>,

    // NOTE: these might actually need to be bounded
    progress_sender: Sender<Progress>,
    progress_receiver: Receiver<Progress>,

    console_sender: Sender<ConsoleMessage>,
    console_receiver: Receiver<ConsoleMessage>,

    // THREAD HANDLING
    thread_handle_sender: Sender<WhisperAppThread>,
}

impl WhisperAppContext {
    fn new(
        client: reqwest::Client,
        handle: Handle,
        audio_wrapper: Arc<SdlAudioWrapper>,
        system_theme: Option<eframe::Theme>,
        thread_handle_sender: Sender<WhisperAppThread>,
    ) -> Self {
        let gpu_enabled = check_gpu_target();
        let gpu_support = Arc::new(AtomicBool::new(gpu_enabled));

        let system_theme = Mutex::new(system_theme);

        // STATE
        let downloading = Arc::new(AtomicBool::new(false));
        let realtime_ready = Arc::new(AtomicBool::new(false));
        let static_ready = Arc::new(AtomicBool::new(false));
        let save_recording_ready = Arc::new(AtomicBool::new(false));

        let realtime_running = Arc::new(AtomicBool::new(false));
        let static_running = Arc::new(AtomicBool::new(false));
        let recorder_running = Arc::new(AtomicBool::new(false));

        // TODO: Figure out how to reset these more elegantly.
        // Atm, no rwlock to keep blocking at a minimum -> currently not possible to mutate
        // CONFIGS
        let (realtime_configs_request_sender, realtime_configs_request_receiver) = bounded(1);
        let (static_configs_request_sender, static_configs_request_receiver) = bounded(1);
        let (recording_configs_request_sender, recording_configs_request_receiver) = bounded(1);
        let (configs_sender, configs_receiver) = bounded(1);
        // Recording
        let (record_audio_i16_sender, record_audio_i16_receiver) =
            bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
        // TODO: remove if no hound spt.
        let (record_audio_i32_sender, record_audio_i32_receiver) =
            bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
        let (record_audio_f32_sender, record_audio_f32_receiver) =
            bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);

        // GUI
        let (transcription_text_sender, transcription_text_receiver) = unbounded();
        let (progress_sender, progress_receiver) = unbounded();
        let (console_sender, console_receiver) = unbounded();

        // FFT BUFFER
        let fft_buffer = Mutex::new([0.0; constants::NUM_BUCKETS]);

        Self {
            gpu_support,
            client,
            handle,
            system_theme,
            audio_wrapper,
            realtime_ready,
            static_ready,
            save_recording_ready,
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

// TODO: MIGRATE THESE TO A SEPARATE FILE
#[cfg(all(target_os = "windows", feature = "cuda"))]
fn check_gpu_target() -> bool {
    use nvml_wrapper::Nvml;
    let nvml = Nvml::init();
    if let Err(_) = &nvml {
        return false;
    };

    let nvml = nvml.unwrap();
    let device_count = nvml.device_count();
    if let Err(_) = &device_count {
        return false;
    };

    let device_count = device_count.unwrap();
    if device_count < 1 {
        return false;
    };

    let cuda_version = nvml.sys_cuda_driver_version();
    if let Err(_) = &cuda_version {
        return false;
    }

    let cuda_version = cuda_version.unwrap();
    let sys_major_version = cuda_driver_version_major(cuda_version);
    let sys_minor_version = cuda_driver_version_minor(cuda_version);
    sys_major_version >= constants::MIN_CUDA_MAJOR && sys_minor_version >= constants::MIN_CUDA_MINOR
}

#[cfg(all(target_os = "linux", feature = "cuda"))]
fn check_gpu_target() -> bool {
    use nvml_wrapper::Nvml;
    let nvml = Nvml::init();
    if let Err(_) = &nvml {
        return false;
    };

    let nvml = nvml.unwrap();
    let device_count = nvml.device_count();
    if let Err(_) = &device_count {
        return false;
    };

    let device_count = device_count.unwrap();
    if device_count < 1 {
        return false;
    };

    let cuda_version = nvml.sys_cuda_driver_version();
    if let Err(_) = &cuda_version {
        return false;
    }

    let cuda_version = cuda_version.unwrap();
    let sys_major_version = cuda_driver_version_major(cuda_version);
    let sys_minor_version = cuda_driver_version_minor(cuda_version);
    sys_major_version >= constants::MIN_CUDA_MAJOR && sys_minor_version >= constants::MIN_CUDA_MINOR
}

#[cfg(all(target_os = "linux", feature = "hipblas"))]
fn check_gpu_target() -> bool {
    let hip = env::var("HIP_PATH").is_ok();
    let common_paths = [
        "/opt/rocm/lib/libhipblas.so",
        "/opt/rocm/hip/lib/libhipblas.so",
        "opt/rocm/lib/librocblas.so",
        "opt/rocm/hipblas",
    ];
    let found_path = common_paths.iter().any(|&path| Path::new(path).exists());
    let blas = env::var("HIP_BLAS_PATH").is_ok();

    (hip & blas) | found_path
}

#[cfg(all(feature = "metal", target_arch = "x86_64"))]
fn check_gpu_target() -> bool {
    use metal;
    let available_devices = metal::Device::all();

    // Using raytracing as a base-minimum for running the gpu at a reasonable speed.
    available_devices.iter().any(|d| d.supports_raytracing());
}

// Apple Silicon is fully supported by Whisper.cpp
#[cfg(all(feature = "metal", target_arch = "aarch64"))]
fn check_gpu_target() -> bool {
    true
}

#[cfg(not(feature = "_gpu"))]
fn check_gpu_target() -> bool {
    false
}

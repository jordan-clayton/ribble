use std::{
    any::TypeId,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock, TryLockError,
    },
    thread::{self, JoinHandle},
};

use arboard::Clipboard;
use crossbeam::channel::{bounded, unbounded, Receiver, SendError, Sender, TryRecvError};
use hound::{Sample, SampleFormat, WavSpec};
use realfft::num_traits::{Bounded, FromPrimitive, NumCast, Zero};
use sdl2::{audio::AudioSpecDesired, log::log};
use tokio::runtime::Handle;
use whisper_realtime::{
    downloader::{
        request::{async_download_request, reqwest},
        traits::AsyncDownload,
    },
    errors::WhisperRealtimeError,
    microphone,
    transcriber::{
        realtime_transcriber::RealtimeTranscriber,
        static_transcriber::{StaticTranscriber, SupportedAudioSample, SupportedChannels},
        traits::Transcriber,
    },
};

use crate::{
    controller::utils::{
        gpu_init::check_gpu_target,
        transcriber_utilities::{
            init_audio_ring_buffer, init_microphone, init_model, init_realtime_microphone,
            init_whisper_ctx,
        },
    },
    ui::tabs::whisper_tab::FocusTab,
    utils::{
        audio_analysis::{
            bandpass_filter, f_central, frequency_analysis, from_f32_normalized,
            normalized_waveform, power_analysis, to_f32_normalized, AnalysisType,
            AtomicAnalysisType,
        },
        console_message::{ConsoleMessage, ConsoleMessageType},
        constants,
        errors::{extract_error_message, WhisperAppError, WhisperAppErrorType},
        file_mgmt::{
            copy_data, decode_audio, delete_temporary_audio_file, get_audio_reader,
            get_temp_file_path, get_tmp_file_writer, save_transcription, write_audio_sample,
        },
        progress::ProgressBar,
        recorder_configs::{RecorderConfigs, RecordingFormat},
        sdl_audio_wrapper::SdlAudioWrapper,
        workers::{AtomicAudioWorkerState, AudioWorkerState},
    },
};

// TODO: rewrite all of this; the current implementation has some major architectural problems that need revising.
// MAJOR: Implement An EngineKernel trait, store this in a weakref in each of the engines so that the controller's internal state
// (the kernel) can provide resources upon request in each of the engines.
// Until an event loop becomes absolutely necessary, this is the cleanest and fastest way to go.

// The following
// Controller:
// -> use a named inner instead of a tuple
// -> refactor state flags
// -> refactor ready flags -> some models will be packed in
// -> Migrate sync primitives to parking lot
// -> Remove poisoning-related state; doesn't exist in parking lot
// -> Reduce access to run_visualizer: set to true whenever there's a visualizer tab open.
// -> Remove FocusTabs once state has been removed from the UI.
// TODO: DECOMPOSE STATE INTO SUB-MODULES, Progress, Console, Visualizer, Recorder, Transcriber, Worker.
// TODO: implement drop and cleanup on drop
// And make the following changes once decoupled.
// Progress:
// -> Allocate a Slab for insertion/removal. slab is Sync + Send, so there's no need for additional synchronization primitives
// -> Add accessor methods to insert (returns ID), remove(id), and list (all current progress jobs)
// ConsoleMessages:
// -> Allocate a dequeue or similar with a fixed (resizeable as per user preferences) number of buckets.
// -> Upon reaching the size limit, pop the previous dequeue
// -> This will require a synchronization primitive (RWLock).
#[derive(Clone)]
pub struct WhisperAppController(Arc<WhisperAppContext>);

impl std::fmt::Debug for WhisperAppController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller")
            .field("Context", &self.0)
            .finish()
    }
}

// TODO: redo all of this; a lot of these methods are shortened by ribble_core.
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

    pub fn is_poisoned(&self) -> bool {
        self.0.poisoned.load(Ordering::Relaxed)
    }

    pub fn mark_poisoned(&self) {
        self.0.poisoned.store(true, Ordering::Relaxed)
    }

    pub fn gpu_enabled(&self) -> bool {
        self.0.gpu_support.load(Ordering::Relaxed)
    }

    // This will fall back to Mocha on a failed system_theme grab.
    pub fn get_system_theme(&self) -> Option<eframe::Theme> {
        let guard = self.0.system_theme.try_read();
        match guard {
            Ok(t) => t.clone(),
            Err(TryLockError::WouldBlock) => None,
            Err(TryLockError::Poisoned(e)) => {
                let t = e.into_inner();
                t.clone()
            }
        }
    }

    pub fn set_system_theme(&mut self, theme: Option<eframe::Theme>) {
        let guard = self.0.system_theme.try_write();
        match guard {
            Ok(mut t) => {
                *t = theme;
            }
            Err(TryLockError::WouldBlock) => {}
            Err(TryLockError::Poisoned(p)) => {
                let mut t = p.into_inner();
                *t = theme;
            }
        }
    }

    pub fn audio_running(&self) -> bool {
        let realtime_running = self.realtime_running();
        let static_running = self.static_running();
        let recorder_running = self.recorder_running();
        realtime_running || static_running || recorder_running
    }

    pub fn audio_worker_state(&self) -> AudioWorkerState {
        self.0.audio_worker_state.load(Ordering::Acquire)
    }
    pub fn is_working(&self) -> bool {
        let audio_running = self.audio_running();
        let downloading = self.is_downloading();
        audio_running || downloading
    }

    // VISUALIZER
    pub fn run_visualizer(&self) -> bool {
        self.0.run_visualizer.load(Ordering::Acquire)
    }
    pub fn set_run_visualizer(&self, visualize: bool) {
        self.0.run_visualizer.store(visualize, Ordering::Release);
    }

    pub fn get_analysis_type(&self) -> AnalysisType {
        self.0.analysis_type.load(Ordering::Acquire)
    }
    pub fn rotate_analysis_type(&self, clockwise: bool) {
        let at = self.0.analysis_type.load(Ordering::Relaxed);
        if clockwise {
            self.set_analysis_type(at.rotate_clockwise());
        } else {
            self.set_analysis_type(at.rotate_counterclockwise());
        }
    }

    pub fn set_analysis_type(&self, analysis_type: AnalysisType) {
        self.0.analysis_type.store(analysis_type, Ordering::Relaxed);
    }

    // READY
    pub fn realtime_ready(&self) -> bool {
        self.0.realtime_ready.load(Ordering::Acquire)
    }

    pub fn set_realtime_ready(&self, ready: bool) {
        self.0.realtime_ready.store(ready, Ordering::Relaxed);
    }

    pub fn static_ready(&self) -> bool {
        self.0.static_ready.load(Ordering::Acquire)
    }

    pub fn set_static_ready(&self, ready: bool) {
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

    pub fn send_focus_tab(&self, tab: FocusTab) -> Result<(), SendError<FocusTab>> {
        self.0.focus_tab_sender.send(tab)
    }

    pub fn recv_focus_tab(&self) -> Result<FocusTab, TryRecvError> {
        self.0.focus_tab_receiver.try_recv()
    }

    pub fn send_progress(&self, progress: ProgressBar) -> Result<(), SendError<ProgressBar>> {
        self.0.progress_sender.send(progress)
    }

    pub fn recv_progress(&self) -> Result<ProgressBar, TryRecvError> {
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

    pub fn read_visualizer_buffer(&self, dest: &mut [f32; constants::NUM_BUCKETS]) {
        let guard = self.0.visualizer_buffer.try_read();
        match guard {
            Ok(data) => {
                dest.copy_from_slice(data.as_slice());
            }
            Err(TryLockError::WouldBlock) => {
                return;
            }
            Err(TryLockError::Poisoned(poison)) => {
                let data = poison.into_inner();
                dest.copy_from_slice(data.as_slice());
                self.0.visualizer_buffer.clear_poison();
            }
        }
    }

    pub fn read_transcription_buffer(&self, dest: &mut Vec<String>) {
        let guard = self.0.transcription_buffer.try_read();
        match guard {
            Ok(transcription) => {
                let len = transcription.len();
                dest.resize(len, String::from(""));
                dest.clone_from_slice(transcription.as_slice());
            }
            Err(TryLockError::WouldBlock) => {
                return;
            }
            Err(TryLockError::Poisoned(poison)) => {
                let transcription = poison.into_inner();
                let len = transcription.len();
                dest.resize(len, String::from(""));
                dest.clone_from_slice(transcription.as_slice());
                self.0.transcription_buffer.clear_poison();
            }
        }
    }

    fn clear_transcription_buffer(&self) {
        let guard = self.0.transcription_buffer.write();
        match guard {
            Ok(mut text_buffer) => {
                text_buffer.clear();
            }
            Err(poison) => {
                let mut text_buffer = poison.into_inner();
                text_buffer.clear();
                self.0.transcription_buffer.clear_poison();
            }
        }
    }

    pub fn send_thread_handle(
        &self,
        thread: WhisperAppThread,
    ) -> Result<(), SendError<WhisperAppThread>> {
        self.0.thread_handle_sender.send(thread)
    }

    pub fn start_realtime_transcription(
        &mut self,
        configs: whisper_realtime::configs::Configs,
        filtering: (bool, f32, f32),
    ) {
        let job_name = "Realtime Init";

        // UPDATE PROGRESS BAR
        let progress = ProgressBar::new(String::from(job_name), 1, 100);
        self.send_progress(progress)
            .expect("Progress channel should be open");

        // Update state
        self.0
            .audio_worker_state
            .store(AudioWorkerState::Loading, Ordering::Release);
        self.0.realtime_running.store(true, Ordering::Release);
        self.0.save_recording_ready.store(false, Ordering::Release);

        // UPDATE PROGRESS BAR
        let progress = ProgressBar::new(String::from(job_name), 17, 100);
        self.send_progress(progress)
            .expect("Progress channel should be open");

        let controller = self.clone();
        let c_controller = controller.clone();

        // UPDATE PROGRESS BAR
        let progress = ProgressBar::new(String::from(job_name), 33, 100);
        self.send_progress(progress)
            .expect("Progress channel should be open");

        let rt_thread = thread::spawn(move || {
            // UPDATE PROGRESS BAR
            let progress = ProgressBar::new(String::from(job_name), 50, 100);

            if let Err(e) = c_controller.send_progress(progress) {
                let err = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Progress channel closed. Info: {}", e.to_string()),
                    true,
                );
                return Err(err);
            }

            // UPDATE PROGRESS BAR
            let progress = ProgressBar::new(String::from(job_name), 76, 100);
            if let Err(e) = c_controller.send_progress(progress) {
                let err = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Progress channel closed. Info: {}", e.to_string()),
                    true,
                );
                return Err(err);
            }

            let rt_configs = Arc::new(configs);

            // UPDATE PROGRESS BAR
            let progress = ProgressBar::new(String::from(job_name), 100, 100);
            if let Err(e) = c_controller.send_progress(progress) {
                let err = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Progress channel closed. Info: {}", e.to_string()),
                    true,
                );
                return Err(err);
            }

            // Run
            run_realtime_audio_transcription(c_controller.clone(), rt_configs, filtering)
        });

        // Send to the background controller to join.
        self.send_thread_handle(rt_thread)
            .expect("Thread channel should be open.");
    }

    pub fn start_static_transcription(
        &self,
        audio_file: &Path,
        configs: whisper_realtime::configs::Configs,
    ) {
        let job_name = "Static Init";
        let audio_file = audio_file.to_path_buf();

        let progress = ProgressBar::new(String::from(job_name), 1, 100);
        self.send_progress(progress)
            .expect("Progress channel should be open.");

        self.0.static_running.store(true, Ordering::Release);
        self.0
            .audio_worker_state
            .store(AudioWorkerState::Loading, Ordering::Release);

        let progress = ProgressBar::new(String::from(job_name), 10, 100);
        self.send_progress(progress)
            .expect("Progress channel should be open.");

        let controller = self.clone();
        let c_controller = controller.clone();
        let progress = ProgressBar::new(String::from(job_name), 20, 100);
        self.send_progress(progress)
            .expect("Progress channel should be open.");

        let st_thread = thread::spawn(move || {
            let audio_file = audio_file.as_path();
            let progress = ProgressBar::new(String::from(job_name), 30, 100);
            if let Err(e) = c_controller.send_progress(progress) {
                let err = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Progress channel closed. Info: {}", e.to_string()),
                    true,
                );
                return Err(err);
            }

            let progress = ProgressBar::new(String::from(job_name), 50, 100);
            if let Err(e) = c_controller.send_progress(progress) {
                let err = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Progress channel closed. Info: {}", e.to_string()),
                    true,
                );
                return Err(err);
            }

            // Wrap for static transcriber.
            let st_configs = Arc::new(configs);

            // Load the file & decode.
            let audio_reader = get_audio_reader(audio_file);
            if let Err(e) = &audio_reader {
                c_controller
                    .0
                    .static_running
                    .store(false, Ordering::Release);
                return Err(e.clone());
            }

            let (id, format, decoder) = audio_reader.unwrap();
            let progress = ProgressBar::new(String::from(job_name), 60, 100);
            if let Err(e) = c_controller.send_progress(progress) {
                let err = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Progress channel closed. Info: {}", e.to_string()),
                    true,
                );
                return Err(err);
            }

            // Second progress job -> decode audio.
            let decode_job_name = "Decoding audio";
            let total_size = if let Ok(m) = std::fs::metadata(audio_file) {
                m.len() as usize
            } else {
                0
            };

            let decoder_progress_callback = |total_decoded| {
                let progress =
                    ProgressBar::new(String::from(decode_job_name), total_decoded, total_size);
                c_controller
                    .send_progress(progress)
                    .expect("Progress channel should be open");
            };

            // decode
            let decode_success = decode_audio(id, format, decoder, Some(decoder_progress_callback));

            // UPDATE PROGRESS
            let progress = ProgressBar::new(String::from(job_name), 80, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel should be open");

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
                let progress = ProgressBar::new(String::from(decode_job_name), 0, total_size);
                c_controller
                    .send_progress(progress)
                    .expect("Progress channel should be open");

                // In case of inexact sizes: progress task will be removed on next ui draw.
            } else if total_size != decoded.len() {
                let progress = ProgressBar::new(String::from(decode_job_name), 1, 1);
                c_controller
                    .send_progress(progress)
                    .expect("Progress channel should be open");
            }

            assert!(!decoded.is_empty(), "Invalid file: 0-size audio");

            let progress = ProgressBar::new(String::from(job_name), 100, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel should be open");
            run_static_audio_transcription(decoded, c_controller.clone(), st_configs)
        });

        self.send_thread_handle(st_thread)
            .expect("Thread channel should be open");
    }

    pub fn start_recording(&self, configs: RecorderConfigs) {
        // Update state.
        self.0.recorder_running.store(true, Ordering::Release);
        self.0.save_recording_ready.store(false, Ordering::Release);
        self.0
            .audio_worker_state
            .store(AudioWorkerState::Loading, Ordering::Release);

        let controller = self.clone();

        let rec_thread = thread::spawn(move || run_recording(controller, configs));

        self.send_thread_handle(rec_thread)
            .expect("Thread channel should be open");
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
                let err = WhisperAppError::new(
                    WhisperAppErrorType::WhisperRealtime,
                    format!("Download failed. Info: {}", e.to_string()),
                    false,
                );
                return Err(err);
            }

            let mut stream = stream.unwrap();

            let total_size = stream.total_size;

            let job_name = format!("Downloading: {}", c_file_name);

            stream.progress_callback = Some(move |n| {
                let progress = ProgressBar::new(job_name.clone(), n, total_size);
                let _ = c_controller.send_progress(progress);
            });

            let download = stream.download(file_directory, c_file_name.as_str());
            let success = handle.block_on(download);
            controller.0.downloading.store(false, Ordering::Release);

            if let Err(e) = success {
                let err = WhisperAppError::new(
                    WhisperAppErrorType::WhisperRealtime,
                    format!("Download failed. Info: {}", e.to_string()),
                    false,
                );
                return Err(err);
            }
            Ok(format!(
                "Model: {}, successfully downloaded to {:?}",
                file_name,
                c_file_directory.as_os_str()
            ))
        });

        self.send_thread_handle(download_thread)
            .expect("Thread channel should be open.");
    }

    pub fn save_audio_recording(&self, output_path: &PathBuf) {
        let to = output_path.to_path_buf();
        let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get storage dir");
        let tmp_file_path = get_temp_file_path(data_dir.as_path());
        debug_assert!(tmp_file_path.exists(), "Audio buffer file missing!");
        let from = tmp_file_path.clone();
        let copy_thread = thread::spawn(move || {
            let success = copy_data(&from, &to);
            match success {
                Ok(_) => Ok(format!("File: {:?} saved successfully.", to.file_stem())),
                Err(e) => {
                    let err = WhisperAppError::new(
                        WhisperAppErrorType::IOError,
                        format!("Failed to copy file. Info: {}", e.to_string()),
                        false,
                    );
                    return Err(err);
                }
            }
        });

        self.send_thread_handle(copy_thread)
            .expect("Thread channel should be open.");
    }

    pub fn copy_to_clipboard(&self) {
        let controller = self.clone();
        let copy_thread = thread::spawn(move || {
            let mut clipboard = Clipboard::new().unwrap();
            let transcription_buffer = controller.0.transcription_buffer.read();
            let transcription_text = match transcription_buffer {
                Ok(text_buffer) => text_buffer.join("\n"),
                Err(poison) => {
                    let text_buffer = poison.into_inner();
                    controller.0.transcription_buffer.clear_poison();
                    text_buffer.join("\n")
                }
            };

            let text_len = transcription_text.len();

            let result = clipboard.set_text(transcription_text);
            match result {
                Ok(_) => Ok(format!("Copied {} bytes to clipboard.", text_len)),
                Err(e) => {
                    // Wrap in App error
                    let err = WhisperAppError::new(
                        WhisperAppErrorType::IOError,
                        format!("Failed to copy to clipboard. Error: {}", e.to_string()),
                        false,
                    );
                    Err(err)
                }
            }
        });

        self.send_thread_handle(copy_thread)
            .expect("Thread channel should be open");
    }

    pub fn save_transcription(&self, output_path: &PathBuf) {
        let p = output_path.clone();
        let c_controller = self.clone();
        let save_thread = thread::spawn(move || {
            let transcription_text = {
                let transcription_buffer = c_controller.0.transcription_buffer.read();
                match transcription_buffer {
                    Ok(text_buffer) => text_buffer.join("\n"),
                    Err(poison) => {
                        let text_buffer = poison.into_inner();
                        text_buffer.join("\n")
                    }
                }
            };

            let job_name = format!("Saving: {:?}", p.file_name());
            let total_size = transcription_text.len();

            let file_name = p.file_stem();
            let directory = p.parent().and_then(|path| Some(path.as_os_str()));

            let progress_callback = move |n: usize| {
                let progress = ProgressBar::new(job_name.clone(), n, total_size);
                let _ = c_controller.send_progress(progress);
            };

            let write_success =
                save_transcription(p.as_path(), &transcription_text, Some(progress_callback));
            match write_success {
                Ok(_) => Ok(format!("{:?} saved to {:?}", file_name, directory)),
                Err(e) => Err(e),
            }
        });

        self.send_thread_handle(save_thread)
            .expect("Thread channel should be open.")
    }

    pub fn cleanup(&self) -> std::io::Result<()> {
        #[cfg(debug_assertions)]
        log("Cleanup called");
        self.0.realtime_running.store(false, Ordering::Relaxed);
        self.0.static_running.store(false, Ordering::Relaxed);
        self.0.recorder_running.store(false, Ordering::Relaxed);
        delete_temporary_audio_file()
    }
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
    desired_audio: &AudioSpecDesired,
    spec: WavSpec,
    channel: (Sender<Vec<T>>, Receiver<Vec<T>>),
    filter: bool,
    f_central: f32,
) -> Result<String, WhisperAppError> {
    let job_name = "Recorder Setup";

    let progress = ProgressBar::new(String::from(job_name), 1, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }
    let c_controller_write = controller.clone();
    let c_controller_visualizer_thread = controller.clone();
    let audio_subsystem = &controller.0.audio_wrapper.audio_subsystem;

    let progress = ProgressBar::new(String::from(job_name), 10, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let (sender, receiver) = channel;

    let mic = init_microphone(audio_subsystem, &desired_audio, sender);

    let progress = ProgressBar::new(String::from(job_name), 30, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let audio_spec = mic.spec().clone();

    let channels = audio_spec.channels as u16;
    let sample_rate = (audio_spec.freq as i16) as u32;
    let mut spec = spec;
    spec.channels = channels;
    spec.sample_rate = sample_rate;

    let progress = ProgressBar::new(String::from(job_name), 40, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let data_dir = eframe::storage_dir(constants::APP_ID);
    if data_dir.is_none() {
        controller
            .0
            .recorder_running
            .store(false, Ordering::Release);
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            String::from("Failed to get data dir"),
            true,
        );
        return Err(err);
    }
    let data_dir = data_dir.unwrap();

    let progress = ProgressBar::new(String::from(job_name), 60, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let writer = get_tmp_file_writer(data_dir.as_path(), &spec);
    if let Err(e) = &writer {
        controller
            .0
            .recorder_running
            .store(false, Ordering::Release);
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Failed to open wav writer. Info: {}", e.to_string()),
            false,
        );
        return Err(err);
    }

    let mut writer = writer.unwrap();

    let progress = ProgressBar::new(String::from(job_name), 80, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Visualizer channel.
    let (visualizer_s, visualizer_r) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);

    // Focus the visualizer.
    if let Err(e) = controller.send_focus_tab(FocusTab::Visualizer) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Focus tab channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    };

    mic.resume();

    controller
        .0
        .audio_worker_state
        .store(AudioWorkerState::Running, Ordering::Release);

    let progress = ProgressBar::new(String::from(job_name), 100, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let result: thread::Result<()> = thread::scope(|s| {
        let _write_thread = s.spawn(|| {
            loop {
                if !c_controller_write.recorder_running() {
                    writer.finalize().expect("Failed to close writer.");
                    c_controller_write
                        .0
                        .save_recording_ready
                        .store(true, Ordering::Release);
                    break;
                }
                let output = receiver.recv();
                debug_assert!(output.is_ok(), "Recording channel should be open");
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

                // Propagate the data to the visualizer
                if c_controller_write.0.run_visualizer.load(Ordering::Acquire) {
                    let visualizer_propagation = visualizer_s.send(output.clone());
                    debug_assert!(visualizer_propagation.is_ok(), "Visualizer Channel Closed");
                }

                write_audio_sample(&output, &mut writer, None::<fn(usize)>);
            }
            #[cfg(debug_assertions)]
            log(&String::from("Recorder: writing thread done."));
        });

        let _visualizer_thread = s.spawn(|| {
            while c_controller_visualizer_thread.recorder_running() {
                let new_audio = visualizer_r.recv();
                if let Ok(output) = new_audio {
                    if TypeId::of::<T>() == TypeId::of::<f32>() {
                        let o = unsafe {
                            let mut out = std::mem::ManuallyDrop::new(output);
                            Vec::from_raw_parts(
                                out.as_mut_ptr() as *mut f32,
                                out.len(),
                                out.capacity(),
                            )
                        };

                        match c_controller_visualizer_thread.get_analysis_type() {
                            AnalysisType::Waveform => {
                                let mut guard = c_controller_visualizer_thread
                                    .0
                                    .visualizer_buffer
                                    .write()
                                    .expect("Poisoned visualizer buffer");
                                normalized_waveform(&o, &mut guard);
                                debug_assert!(
                                    guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                                    "Failed to normalize."
                                );
                            }
                            AnalysisType::Power => {
                                let mut guard = c_controller_visualizer_thread
                                    .0
                                    .visualizer_buffer
                                    .write()
                                    .expect("Poisoned visualizer buffer");
                                power_analysis(&o, &mut guard);
                                debug_assert!(
                                    guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                                    "Failed to normalize."
                                );
                            }
                            AnalysisType::SpectrumDensity => {
                                let mut guard = c_controller_visualizer_thread
                                    .0
                                    .visualizer_buffer
                                    .write()
                                    .expect("Poisoned visualizer buffer");
                                frequency_analysis(&o, &mut guard, sample_rate as f64);
                                debug_assert!(
                                    guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                                    "Failed to normalize."
                                );
                            }
                        }
                    } else {
                        let len = output.len();
                        let mut visualizer_data = vec![0.0; len];
                        to_f32_normalized(&output, &mut visualizer_data);

                        match c_controller_visualizer_thread.get_analysis_type() {
                            AnalysisType::Waveform => {
                                let mut guard = c_controller_visualizer_thread
                                    .0
                                    .visualizer_buffer
                                    .write()
                                    .expect("Poisoned fft buffer");
                                normalized_waveform(&visualizer_data, &mut guard);
                                debug_assert!(
                                    guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                                    "Failed to normalize"
                                );
                            }
                            AnalysisType::Power => {
                                let mut guard = c_controller_visualizer_thread
                                    .0
                                    .visualizer_buffer
                                    .write()
                                    .expect("Poisoned fft buffer");
                                power_analysis(&visualizer_data, &mut guard);
                                debug_assert!(
                                    guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                                    "Failed to normalize"
                                );
                            }
                            AnalysisType::SpectrumDensity => {
                                let mut guard = c_controller_visualizer_thread
                                    .0
                                    .visualizer_buffer
                                    .write()
                                    .expect("Poisoned fft buffer");
                                frequency_analysis(
                                    &visualizer_data,
                                    &mut guard,
                                    sample_rate as f64,
                                );
                                debug_assert!(
                                    guard.iter().all(|n| *n >= 0.0 && *n <= 1.0),
                                    "Failed to normalize"
                                );
                            }
                        }
                    }
                }
            }
            #[cfg(debug_assertions)]
            log(&String::from("Recorder: visualizing thread done."));
        });
        Ok(())
    });

    mic.pause();
    match result {
        Ok(_) => Ok(String::from("Recording finished.")),
        Err(e) => {
            let e_msg = extract_error_message(e);

            let err = WhisperAppError::new(
                WhisperAppErrorType::ThreadError,
                format!("Recording thread panicked. Info: {}", e_msg),
                false,
            );
            Err(err)
        }
    }
}

fn run_recording(
    controller: WhisperAppController,
    configs: RecorderConfigs,
) -> Result<String, WhisperAppError> {
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
    let result = match configs.format {
        RecordingFormat::I16 => {
            let sender = controller.0.record_audio_i16_sender.clone();
            let receiver = controller.0.record_audio_i16_receiver.clone();
            let spec = WavSpec {
                channels: 2,
                sample_rate: 41000,
                bits_per_sample: 16,
                sample_format: SampleFormat::Int,
            };
            let res = recording_impl(
                controller.clone(),
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );

            clear_message_queue(controller.0.record_audio_i16_receiver.clone());
            res
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
            let res = recording_impl(
                controller.clone(),
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );
            clear_message_queue(controller.0.record_audio_i32_receiver.clone());
            res
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
            let res = recording_impl(
                controller.clone(),
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );
            clear_message_queue(controller.0.record_audio_f32_receiver.clone());
            res
        }
    };
    controller
        .0
        .audio_worker_state
        .store(AudioWorkerState::Idle, Ordering::Release);
    result
}

fn run_realtime_audio_transcription(
    controller: WhisperAppController,
    configs: Arc<whisper_realtime::configs::Configs>,
    filtering: (bool, f32, f32),
) -> Result<String, WhisperAppError> {
    let job_name = "Realtime Setup";

    let progress = ProgressBar::new(String::from(job_name), 1, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Audio filtering
    let filter = filtering.0;
    let f_higher = filtering.1;
    let f_lower = filtering.2;
    let f_central = f_central(f_lower, f_higher);

    let progress = ProgressBar::new(String::from(job_name), 10, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Clear the text buffer.
    controller.clear_transcription_buffer();
    // Clone the controller
    let c_controller_audio_thread = controller.clone();
    let c_controller_write_thread = controller.clone();
    let c_controller_visualizer_thread = controller.clone();
    let c_controller_transcription_reader_thread = controller.clone();

    let progress = ProgressBar::new(String::from(job_name), 20, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Init model.
    let model = init_model(configs.clone());
    let c_model = model.clone();

    // Clone configs
    let c_configs = configs.clone();

    // Audio buffer.
    let audio = init_audio_ring_buffer(Some(whisper_realtime::constants::INPUT_BUFFER_CAPACITY));
    let c_audio_reader = audio.clone();
    let c_audio_transcriber = audio.clone();

    let progress = ProgressBar::new(String::from(job_name), 30, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Text sender
    let c_text_sender = controller.0.transcription_text_sender.clone();
    // Recording sender for writing to tmp file
    let (s_writer, r_writer) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    let audio_write_sender: Sender<Vec<f32>> = s_writer.clone();
    let audio_write_receiver = r_writer.clone();
    // Audio sender for transcription visualizer
    let (s_visualizer, r_visualizer) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    let audio_visualizer_sender: Sender<Vec<f32>> = s_visualizer.clone();
    let audio_visualizer_receiver = r_visualizer.clone();

    // State Flags - This should likely be refactored.
    let c_realtime_is_running = controller.0.realtime_running.clone();

    let progress = ProgressBar::new(String::from(job_name), 40, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let mic_stream = init_realtime_microphone(
        &controller.0.audio_wrapper.audio_subsystem,
        controller.0.record_audio_f32_sender.clone(),
    );
    let audio_spec = Arc::new(mic_stream.spec().clone());
    let sample_rate = audio_spec.freq as f64;
    let sample_rate_bandpass = sample_rate as f32;

    let c_audio_spec = audio_spec.clone();
    let c_mic_stream = mic_stream.clone();

    let progress = ProgressBar::new(String::from(job_name), 50, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Init Whisper.
    let ctx = init_whisper_ctx(c_model.clone(), c_configs.use_gpu);

    let state = ctx.create_state();
    if let Err(e) = &state {
        let err = WhisperAppError::new(
            WhisperAppErrorType::WhisperRealtime,
            format!("Failed to create Whisper State. Info: {}", e.to_string()),
            false,
        );
        return Err(err);
    }

    let progress = ProgressBar::new(String::from(job_name), 80, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let mut state = state.unwrap();

    // Focus the transcriber tab.

    if let Err(e) = controller.send_focus_tab(FocusTab::Transcription) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Focus tab channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    };

    let progress = ProgressBar::new(String::from(job_name), 100, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // NOTE: this can still panic. The panic will kill the thread and the joiner will
    // catch and close the app on next update.
    let transcription_thread = thread::scope(|s| {
        c_mic_stream.resume();
        let c_realtime_is_running_transcription_reader_thread = c_realtime_is_running.clone();
        let _visualizer_thread = s.spawn(move || {
            while c_controller_visualizer_thread.realtime_running() {
                let audio = audio_visualizer_receiver.recv();
                if let Ok(output) = audio {
                    if output.is_empty() {
                        break;
                    }
                    match c_controller_visualizer_thread.get_analysis_type() {
                        AnalysisType::Waveform => {
                            let guard = c_controller_visualizer_thread.0.visualizer_buffer.write();
                            match guard {
                                Ok(mut buffer) => {
                                    normalized_waveform(&output, &mut buffer);
                                }
                                Err(poisoned) => {
                                    let mut buffer = poisoned.into_inner();
                                    c_controller_visualizer_thread
                                        .0
                                        .visualizer_buffer
                                        .clear_poison();
                                    normalized_waveform(&output, &mut buffer);
                                }
                            }
                        }
                        AnalysisType::Power => {
                            let guard = c_controller_visualizer_thread.0.visualizer_buffer.write();
                            match guard {
                                Ok(mut buffer) => {
                                    power_analysis(&output, &mut buffer);
                                }
                                Err(poisoned) => {
                                    let mut buffer = poisoned.into_inner();
                                    c_controller_visualizer_thread
                                        .0
                                        .visualizer_buffer
                                        .clear_poison();
                                    power_analysis(&output, &mut buffer);
                                }
                            }
                        }
                        AnalysisType::SpectrumDensity => {
                            let guard = c_controller_visualizer_thread.0.visualizer_buffer.write();
                            match guard {
                                Ok(mut buffer) => {
                                    frequency_analysis(&output, &mut buffer, sample_rate);
                                }
                                Err(poisoned) => {
                                    let mut buffer = poisoned.into_inner();
                                    c_controller_visualizer_thread
                                        .0
                                        .visualizer_buffer
                                        .clear_poison();
                                    frequency_analysis(&output, &mut buffer, sample_rate);
                                }
                            }
                        }
                    }
                }
            }
            #[cfg(debug_assertions)]
            log(&"Visualizer closed properly");
        });

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

            while c_controller_write_thread.realtime_running() {
                let audio = audio_write_receiver.recv();
                if let Ok(output) = audio {
                    write_audio_sample(&output, &mut writer, None::<fn(usize)>);
                }
            }

            writer.finalize().expect("Failed to close writer.");
            #[cfg(debug_assertions)]
            log(&"Writer closed properly");
        });

        let _audio_thread = s.spawn(move || {
            loop {
                if !c_controller_audio_thread.realtime_running() {
                    // Pump the Visualizer thread with a zero-length vector to signal "Done"
                    // Loop will break on next iteration.
                    let visualize_propagation = audio_visualizer_sender.send(vec![]);
                    debug_assert!(
                        visualize_propagation.is_ok(),
                        "F32 Visualizer Channel closed"
                    );
                    break;
                }

                let output = c_controller_audio_thread.0.record_audio_f32_receiver.recv();
                debug_assert!(output.is_ok(), "Realtime Audio Channel closed");

                let mut audio_data = output.unwrap();

                // Filter the audio
                if filter {
                    bandpass_filter(&mut audio_data, sample_rate_bandpass, f_central);
                }

                // NOTE: This blocks.
                c_audio_reader.push_audio(&mut audio_data);

                // Propagate the data to the writing thread.
                let write_propagation = audio_write_sender.send(audio_data.clone());

                if c_controller_audio_thread.run_visualizer() {
                    let visualize_propagation = audio_visualizer_sender.send(audio_data.clone());
                    debug_assert!(
                        visualize_propagation.is_ok(),
                        "F32 Visualizer Channel closed"
                    );
                }
                debug_assert!(write_propagation.is_ok(), "F32 Visualizer Channel closed");
            }

            // CLEAR THE MSG QUEUE -> this runs a loop to eat msgs until the channel is fully cleared
            // or has no producers.
            clear_message_queue(
                c_controller_audio_thread
                    .0
                    .record_audio_f32_receiver
                    .clone(),
            );

            // Closed properly
            #[cfg(debug_assertions)]
            log(&"Audio reader thread closed properly");
        });

        let transcription_reader_thread = s.spawn(move || {
            let res = loop {
                if !c_controller_transcription_reader_thread.realtime_running() {
                    break Ok(());
                }
                let text = c_controller_transcription_reader_thread
                    .0
                    .transcription_text_receiver
                    .recv();
                match text {
                    Ok(result) => {
                        match result {
                            Ok(text_packet) => {
                                if text_packet.0 == constants::GO_MSG {
                                    c_controller_transcription_reader_thread
                                        .0
                                        .audio_worker_state
                                        .store(AudioWorkerState::Running, Ordering::Release);
                                }

                                if text_packet.0 == constants::STOP_MSG {
                                    continue;
                                }

                                // Consume newlines
                                if text_packet.0 == "\n" {
                                    continue;
                                }

                                let guard = c_controller_transcription_reader_thread
                                    .0
                                    .transcription_buffer
                                    .write();
                                match guard {
                                    Ok(mut text_buffer) => {
                                        if text_packet.1 {
                                            text_buffer.push(text_packet.0);
                                        } else {
                                            let last_entry_index = text_buffer.len() - 1;
                                            text_buffer[last_entry_index] = text_packet.0;
                                        }
                                    }
                                    Err(poison) => {
                                        let mut text_buffer = poison.into_inner();
                                        c_controller_transcription_reader_thread
                                            .0
                                            .transcription_buffer
                                            .clear_poison();
                                        if text_packet.1 {
                                            text_buffer.push(text_packet.0)
                                        } else {
                                            let last_entry_index = text_buffer.len() - 1;
                                            text_buffer[last_entry_index] = text_packet.0;
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                let err = WhisperAppError::new(
                                    WhisperAppErrorType::WhisperRealtime,
                                    format!("Transcription failure. Info: {}", e.to_string()),
                                    false,
                                );
                                c_realtime_is_running_transcription_reader_thread
                                    .store(false, Ordering::Release);
                                break Err(err);
                            }
                        }
                    }
                    Err(e) => {
                        let err = WhisperAppError::new(
                            WhisperAppErrorType::IOError,
                            format!("Transcription channel closed. Info: {}", e.to_string()),
                            false,
                        );
                        c_realtime_is_running_transcription_reader_thread
                            .store(false, Ordering::Release);
                        break Err(err);
                    }
                }
            };

            // Clear the text channel
            clear_message_queue(
                c_controller_transcription_reader_thread
                    .0
                    .transcription_text_receiver
                    .clone(),
            );
            // Closed properly
            #[cfg(debug_assertions)]
            log(&"Transcription reader thread closed properly");
            res
        });
        let transcription_runner_thread = s
            .spawn(move || {
                let mut transcriber = RealtimeTranscriber::new_with_configs(
                    c_audio_transcriber,
                    c_text_sender,
                    c_configs.clone(),
                    None,
                );
                #[cfg(debug_assertions)]
                log("Transcriber should be created.");

                let output =
                    transcriber.process_audio(&mut state, c_realtime_is_running, None::<fn(i32)>);
                #[cfg(debug_assertions)]
                log(&"Transcription runner thread closed properly");
                output
            })
            .join();

        let reader = transcription_reader_thread.join().unwrap_or_else(|e| {
            let e_msg = extract_error_message(e);
            let fatal = e_msg.contains("channel should be open");

            let err = WhisperAppError::new(
                WhisperAppErrorType::ThreadError,
                format!("Transcription reader thread panicked. Info: {}", e_msg),
                fatal,
            );
            Err(err)
        });
        (transcription_runner_thread, reader)
    });

    c_mic_stream.pause();
    #[cfg(debug_assertions)]
    log("Realtime transcription finished");

    controller
        .0
        .save_recording_ready
        .store(true, Ordering::Release);
    finalize_transcription(controller.clone(), transcription_thread)
}

fn run_static_audio_transcription(
    audio: Vec<f32>,
    controller: WhisperAppController,
    configs: Arc<whisper_realtime::configs::Configs>,
) -> Result<String, WhisperAppError> {
    let job_name = "Static Setup";

    let progress = ProgressBar::new(String::from(job_name), 1, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Clear the text buffer.
    controller.clear_transcription_buffer();

    let progress = ProgressBar::new(String::from(job_name), 10, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let c_controller_runner_thread = controller.clone();
    let c_controller_reader_thread = controller.clone();

    let progress = ProgressBar::new(String::from(job_name), 20, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }
    let model = init_model(configs.clone());

    let progress = ProgressBar::new(String::from(job_name), 30, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let audio = SupportedAudioSample::F32(audio);
    let audio = Arc::new(Mutex::new(audio));

    let progress = ProgressBar::new(String::from(job_name), 40, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let data_sender = controller.0.transcription_text_sender.clone();
    let data_sender = Some(data_sender);
    let channels = SupportedChannels::MONO;

    let progress = ProgressBar::new(String::from(job_name), 50, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // Init Whisper.
    let ctx = init_whisper_ctx(model.clone(), configs.use_gpu);
    let state = ctx.create_state();
    if let Err(e) = &state {
        let err = WhisperAppError::new(
            WhisperAppErrorType::WhisperRealtime,
            format!("Failed to create Whisper State. Info: {}", e.to_string()),
            false,
        );
        return Err(err);
    }

    let progress = ProgressBar::new(String::from(job_name), 60, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let mut state = state.unwrap();

    // Progress callback.
    let p_controller = controller.clone();
    let transcriber_job_name = "Transcribing Audio";
    let total_size = 100;

    let progress_callback = move |n: i32| {
        let progress = ProgressBar::new(String::from(transcriber_job_name), n as usize, total_size);
        let _ = p_controller.send_progress(progress);
    };

    let progress = ProgressBar::new(String::from(job_name), 80, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    let progress_callback = Some(progress_callback);

    // Focus the transcriber tab.
    if let Err(e) = controller.send_focus_tab(FocusTab::Transcription) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Focus tab channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    };

    let progress = ProgressBar::new(String::from(job_name), 100, 100);
    if let Err(e) = controller.send_progress(progress) {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            format!("Progress channel closed. Info: {}", e.to_string()),
            true,
        );
        return Err(err);
    }

    // NOTE: this can still panic. The panic will kill the thread and the joiner will
    // catch and close the app on next update.
    let transcription_thread = thread::scope(|s| {
        // Transcription reader thread.
        let reader_thread = s.spawn(move || {
            while c_controller_reader_thread.static_running() {
                let text = c_controller_reader_thread
                    .0
                    .transcription_text_receiver
                    .recv();
                match text {
                    Ok(result) => match result {
                        Ok(text_packet) => {
                            // Consume the stop message (will break the reader loop)
                            if text_packet.0 == constants::STOP_MSG {
                                continue;
                            }

                            let guard = c_controller_reader_thread.0.transcription_buffer.write();
                            match guard {
                                Ok(mut text_buffer) => {
                                    if text_packet.1 {
                                        text_buffer.push(text_packet.0)
                                    } else {
                                        let last_entry_index = text_buffer.len() - 1;
                                        text_buffer[last_entry_index] = text_packet.0;
                                    }
                                }
                                Err(poison) => {
                                    let mut text_buffer = poison.into_inner();
                                    c_controller_reader_thread
                                        .0
                                        .transcription_buffer
                                        .clear_poison();
                                    if text_packet.1 {
                                        text_buffer.push(text_packet.0);
                                    } else {
                                        let last_entry_index = text_buffer.len() - 1;
                                        text_buffer[last_entry_index] = text_packet.0;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            c_controller_reader_thread
                                .0
                                .static_running
                                .store(false, Ordering::Release);

                            let err = WhisperAppError::new(
                                WhisperAppErrorType::WhisperRealtime,
                                format!("Transcription failure. Info: {}", e.to_string()),
                                false,
                            );
                            return Err(err);
                        }
                    },
                    Err(e) => {
                        c_controller_reader_thread
                            .0
                            .static_running
                            .store(false, Ordering::Release);

                        let err = WhisperAppError::new(
                            WhisperAppErrorType::IOError,
                            format!("Transcription channel closed. Info: {}", e),
                            true,
                        );
                        return Err(err);
                    }
                }
            }

            // Clear the text channel
            clear_message_queue(
                c_controller_reader_thread
                    .0
                    .transcription_text_receiver
                    .clone(),
            );
            #[cfg(debug_assertions)]
            log(&String::from("Transcription reader finished"));
            Ok(())
        });

        let transcription_runner_thread = s
            .spawn(move || {
                let mut transcriber =
                    StaticTranscriber::new_with_configs(audio, data_sender, configs, channels);

                c_controller_runner_thread
                    .0
                    .audio_worker_state
                    .store(AudioWorkerState::Running, Ordering::Release);
                let static_running = c_controller_runner_thread.0.static_running.clone();
                let output =
                    transcriber.process_audio(&mut state, static_running, progress_callback);

                // Update state if transcription not already stopped.
                c_controller_runner_thread
                    .0
                    .static_running
                    .store(false, Ordering::Release);
                // Final progress update -> Whisper finishes before the final callback.
                let progress = ProgressBar::new(String::from(transcriber_job_name), 1, 1);

                // Pump the reader thread to wake it up.
                c_controller_runner_thread
                    .0
                    .transcription_text_sender
                    .send(Ok((String::from(constants::STOP_MSG), true)))
                    .expect("Transcription channel should be open.");

                c_controller_runner_thread
                    .send_progress(progress)
                    .expect("Progress channel should be open.");

                #[cfg(debug_assertions)]
                log(&String::from("Transcription runner finished"));
                output
            })
            .join();

        let reader = match reader_thread.join() {
            Ok(res) => match res {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            },
            Err(e) => {
                let e_msg = extract_error_message(e);
                let fatal = e_msg.contains("channel should be open");
                let err = WhisperAppError::new(
                    WhisperAppErrorType::ThreadError,
                    format!("Transcription reader thread panicked. Info: {}", e_msg),
                    fatal,
                );
                Err(err)
            }
        };
        (transcription_runner_thread, reader)
    });

    #[cfg(debug_assertions)]
    log("Static transcription finished");
    finalize_transcription(controller.clone(), transcription_thread)
}

fn finalize_transcription(
    controller: WhisperAppController,
    transcription_results: (thread::Result<String>, Result<(), WhisperAppError>),
) -> Result<String, WhisperAppError> {
    let (transcription, reader) = transcription_results;
    // Get info from the reader thread.
    if let Err(e) = reader {
        if e.fatal() {
            return Err(e);
        } else {
            let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
            let send_msg = controller.send_console_message(msg);
            if let Err(e) = send_msg {
                let fatal = WhisperAppError::new(
                    WhisperAppErrorType::IOError,
                    format!("Console channel closed. Info: {}", e),
                    true,
                );
                return Err(fatal);
            }
        }
    }

    controller
        .0
        .audio_worker_state
        .store(AudioWorkerState::Idle, Ordering::Release);
    // Extract the transcription if possible.
    let result = match transcription {
        Ok(t) => {
            let guard = controller.0.transcription_buffer.write();
            let mut buffer = match guard {
                Ok(buffer) => buffer,
                Err(poison) => {
                    controller.0.transcription_buffer.clear_poison();
                    poison.into_inner()
                }
            };
            buffer.clear();
            buffer.push(t);
            Ok(String::from("Transcription Complete"))
        }
        Err(_) => {
            let err = WhisperAppError::new(
                WhisperAppErrorType::WhisperRealtime,
                String::from("Transcription failed."),
                false,
            );
            Err(err)
        }
    };
    result
}

fn clear_message_queue<T>(queue: Receiver<T>) {
    while let Ok(_) = queue.try_recv() {
        continue;
    }
}

type WhisperAppThread = JoinHandle<Result<String, WhisperAppError>>;

#[derive(Debug)]
struct WhisperAppContext {
    poisoned: Arc<AtomicBool>,
    // GPU AVAILABLE
    gpu_support: Arc<AtomicBool>,
    // ASYNC (DOWNLOADS)
    client: reqwest::Client,
    handle: Handle,
    system_theme: RwLock<Option<eframe::Theme>>,
    // SDL AUDIO
    audio_wrapper: Arc<SdlAudioWrapper>,
    // STATE
    audio_worker_state: Arc<AtomicAudioWorkerState>,
    realtime_ready: Arc<AtomicBool>,
    static_ready: Arc<AtomicBool>,
    save_recording_ready: Arc<AtomicBool>,
    downloading: Arc<AtomicBool>,
    recorder_running: Arc<AtomicBool>,
    realtime_running: Arc<AtomicBool>,
    static_running: Arc<AtomicBool>,

    // VISUALIZER
    run_visualizer: Arc<AtomicBool>,
    analysis_type: AtomicAnalysisType,
    visualizer_buffer: RwLock<[f32; constants::NUM_BUCKETS]>,

    // AUDIO
    record_audio_i16_sender: Sender<Vec<i16>>,
    record_audio_i16_receiver: Receiver<Vec<i16>>,

    record_audio_i32_sender: Sender<Vec<i32>>,
    record_audio_i32_receiver: Receiver<Vec<i32>>,

    record_audio_f32_sender: Sender<Vec<f32>>,
    record_audio_f32_receiver: Receiver<Vec<f32>>,

    // TRANSCRIPTION
    transcription_text_sender: Sender<Result<(String, bool), WhisperRealtimeError>>,
    transcription_text_receiver: Receiver<Result<(String, bool), WhisperRealtimeError>>,
    transcription_buffer: RwLock<Vec<String>>,

    // NOTE: these might actually need to be bounded
    progress_sender: Sender<ProgressBar>,
    progress_receiver: Receiver<ProgressBar>,

    console_sender: Sender<ConsoleMessage>,
    console_receiver: Receiver<ConsoleMessage>,

    // THREAD HANDLING
    thread_handle_sender: Sender<WhisperAppThread>,

    // TAB MGMT
    focus_tab_sender: Sender<FocusTab>,
    focus_tab_receiver: Receiver<FocusTab>,
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

        let system_theme = RwLock::new(system_theme);

        // STATE
        let poisoned = Arc::new(AtomicBool::new(false));
        let downloading = Arc::new(AtomicBool::new(false));
        let realtime_ready = Arc::new(AtomicBool::new(false));
        let static_ready = Arc::new(AtomicBool::new(false));
        let save_recording_ready = Arc::new(AtomicBool::new(false));

        let realtime_running = Arc::new(AtomicBool::new(false));
        let static_running = Arc::new(AtomicBool::new(false));
        let recorder_running = Arc::new(AtomicBool::new(false));

        let audio_worker_state = Arc::new(AtomicAudioWorkerState::new(AudioWorkerState::Idle));

        // Visualizer
        let visualizer_buffer = RwLock::new([0.0; constants::NUM_BUCKETS]);
        let run_visualizer = Arc::new(AtomicBool::new(false));
        let analysis_type = AtomicAnalysisType::new(AnalysisType::Power);

        // Recording
        let (record_audio_i16_sender, record_audio_i16_receiver) =
            bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
        let (record_audio_i32_sender, record_audio_i32_receiver) =
            bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
        let (record_audio_f32_sender, record_audio_f32_receiver) =
            bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);

        // Transcription
        let (transcription_text_sender, transcription_text_receiver) = unbounded();
        let transcription_buffer = RwLock::new(vec![]);

        // Other GUI
        let (progress_sender, progress_receiver) = unbounded();
        let (console_sender, console_receiver) = unbounded();
        let (focus_tab_sender, focus_tab_receiver) = unbounded();

        Self {
            poisoned,
            gpu_support,
            client,
            handle,
            system_theme,
            audio_wrapper,
            audio_worker_state,
            realtime_ready,
            static_ready,
            save_recording_ready,
            downloading,
            recorder_running,
            realtime_running,
            static_running,
            run_visualizer,
            analysis_type,
            visualizer_buffer,
            record_audio_i16_sender,
            record_audio_i16_receiver,
            record_audio_i32_sender,
            record_audio_i32_receiver,
            record_audio_f32_sender,
            record_audio_f32_receiver,
            transcription_text_sender,
            transcription_text_receiver,
            transcription_buffer,
            progress_sender,
            progress_receiver,
            console_sender,
            console_receiver,
            thread_handle_sender,
            focus_tab_sender,
            focus_tab_receiver,
        }
    }
}

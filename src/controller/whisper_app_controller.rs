use std::{
    any::Any,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering}, Mutex, TryLockError,
    },
    thread::{self, JoinHandle},
};
// TODO: clean up imports.
use std::any::TypeId;
use std::path::Path;
use std::sync::RwLock;

use arboard::Clipboard;
use crossbeam::channel::{
    bounded, Receiver, Sender, SendError, TryRecvError, unbounded,
};
use hound::{Sample, SampleFormat, WavSpec};
#[cfg(feature = "cuda")]
use nvml_wrapper::{cuda_driver_version_major, cuda_driver_version_minor};
use realfft::num_traits::{Bounded, FromPrimitive, NumCast, Zero};
use sdl2::audio::AudioSpecDesired;
use sdl2::log::log;
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
        console_message::{ConsoleMessage, ConsoleMessageType},
        constants,
        file_mgmt::{
            copy_data, get_temp_file_path, get_tmp_file_writer, save_transcription,
            write_audio_sample,
        },
        progress::Progress,
        sdl_audio_wrapper::SdlAudioWrapper,
    },
};
// TODO: nest imports
use crate::controller::utils::transcriber_utilities::init_microphone;
use crate::ui::tabs::whisper_tab::FocusTab;
use crate::utils::audio_analysis::{
    AnalysisType, AtomicAnalysisType, bandpass_filter, f_central, frequency_analysis,
    from_f32_normalized, normalized_waveform, power_analysis, to_f32_normalized,
};
use crate::utils::errors::{WhisperAppError, WhisperAppErrorType};
use crate::utils::file_mgmt::{decode_audio, delete_temporary_audio_file, get_audio_reader};
use crate::utils::recorder_configs::{RecorderConfigs, RecordingFormat};
use crate::utils::workers::{AtomicAudioWorkerState, AudioWorkerState, WorkerType};

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
        return realtime_running || static_running || recorder_running;
    }

    // STATE
    pub fn app_running(&self) -> bool {
        self.0.application_running.load(Ordering::Acquire)
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
    pub fn rotate_analysis_type(&self) {
        let at = self.0.analysis_type.load(Ordering::Relaxed);
        self.set_analysis_type(at.rotate_clockwise())
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

    pub fn read_fft_buffer(&self, dest: &mut [f32; constants::NUM_BUCKETS]) {
        let guard = self.0.visualizer_buffer.try_read();
        match guard {
            Ok(g) => {
                dest.copy_from_slice(g.as_slice());
            }
            Err(TryLockError::WouldBlock) => {
                return;
            }
            Err(_g) => {
                // TODO: proper error handling -> this is recoverable.
                panic!("Visualizer RwLock poisoned.");
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
            Err(_g) => {
                // TODO: proper error handling -> this is recoverable.
                panic!("Transcription RwLock poisoned.");
            }
        }
    }

    fn clear_transcription_buffer(&self) {
        let guard = self.0.transcription_buffer.write();
        match guard {
            Ok(mut text_buffer) => {
                text_buffer.clear();
                log(&format!(
                    "Text buffer should be length 0, length: {}",
                    text_buffer.len()
                ))
            }
            Err(poison) => {
                let mut text_buffer = poison.into_inner();
                text_buffer.clear();
            }
        }
    }

    pub fn send_thread_handle(
        &self,
        thread: WhisperAppThread,
    ) -> Result<(), SendError<WhisperAppThread>> {
        self.0.thread_handle_sender.send(thread)
    }

    // TODO: add focus-to-transcription tab.
    pub fn start_realtime_transcription(&mut self, configs: whisper_realtime::configs::Configs, filtering: (bool, f32, f32)) {
        let job_name = "Realtime Setup";

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 1, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        // Update state
        self.0.audio_worker_state.store(AudioWorkerState::Loading, Ordering::Release);
        self.0.realtime_running.store(true, Ordering::Release);
        self.0.save_recording_ready.store(false, Ordering::Release);

        // UPDATE PROGRESS BAR
        let progress = Progress::new(String::from(job_name), 17, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

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

            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(job_name), 76, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            let rt_configs = Arc::new(configs);

            // UPDATE PROGRESS BAR
            let progress = Progress::new(String::from(job_name), 100, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            // Run
            run_realtime_audio_transcription(c_controller.clone(), rt_configs, filtering)
        });

        let thread = (WorkerType::Realtime, rt_thread);

        // Send to the background controller to join.
        self.send_thread_handle(thread)
            .expect("Thread channel closed");
    }

    pub fn start_static_transcription(
        &self,
        audio_file: &Path,
        configs: whisper_realtime::configs::Configs,
    ) {
        let job_name = "Static Setup";
        let audio_file = audio_file.to_path_buf();

        let progress = Progress::new(String::from(job_name), 1, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

        self.0.static_running.store(true, Ordering::Release);
        self.0.audio_worker_state.store(AudioWorkerState::Loading, Ordering::Release);

        let progress = Progress::new(String::from(job_name), 10, 100);
        self.send_progress(progress)
            .expect("Progress channel closed");

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

            let progress = Progress::new(String::from(job_name), 50, 100);
            c_controller
                .send_progress(progress)
                .expect("Progress channel closed");

            // Wrap for static transcriber.
            let st_configs = Arc::new(configs);

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
            run_static_audio_transcription(decoded, c_controller.clone(), st_configs)
        });

        let worker = (WorkerType::Static, st_thread);
        self.send_thread_handle(worker)
            .expect("Thread channel closed");
    }

    // TODO: add progress
    pub fn start_recording(&self, configs: RecorderConfigs) {
        let job_name = "Recorder Setup";
        // Update state.
        self.0.recorder_running.store(true, Ordering::Release);
        self.0.save_recording_ready.store(false, Ordering::Release);
        self.0.audio_worker_state.store(AudioWorkerState::Loading, Ordering::Release);

        let controller = self.clone();

        let rec_thread = thread::spawn(move || {
            run_recording(controller, configs);

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

    // TODO: proper error handling.
    pub fn copy_to_clipboard(&self) {
        let controller = self.clone();
        let copy_thread = thread::spawn(move || {
            let mut clipboard = Clipboard::new().unwrap();
            let transcription_buffer = controller.0.transcription_buffer.read();
            let transcription_text = match transcription_buffer {
                Ok(text_buffer) => text_buffer.join("\n"),
                Err(poison) => {
                    let text_buffer = poison.into_inner();
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
                    );
                    // TODO: remove box once error refactor.
                    let err: Box<dyn Any + Send> = Box::new(err);
                    Err(err)
                }
            }
        });

        let worker = (WorkerType::IO, copy_thread);
        self.send_thread_handle(worker)
            .expect("Thread channel closed");
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

    pub fn cleanup(&self) -> std::io::Result<()> {
        #[cfg(debug_assertions)]
        log("Cleanup called");
        self.0.realtime_running.store(false, Ordering::Relaxed);
        self.0.static_running.store(false, Ordering::Relaxed);
        self.0.recorder_running.store(false, Ordering::Relaxed);
        self.0.application_running.store(false, Ordering::Relaxed);
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
) {
    let c_controller_write = controller.clone();
    let c_controller_visualizer_thread = controller.clone();
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

    // Visualizer channel.
    let (visualizer_s, visualizer_r) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);

    // Focus the visualizer.
    controller
        .send_focus_tab(FocusTab::Visualizer)
        .expect("Focus tab channel closed");

    mic.resume();
    controller.0.audio_worker_state.store(AudioWorkerState::Running, Ordering::Release);

    let _ = thread::scope(|s| {
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

                // Propagate the data to the visualizer
                if c_controller_write.0.run_visualizer.load(Ordering::Acquire) {
                    let visualizer_propagation = visualizer_s.send(output.clone());
                    assert!(visualizer_propagation.is_ok(), "Visualizer Channel Closed");
                }

                write_audio_sample(&output, &mut writer, None::<fn(usize)>);
            }

            log(&String::from("Recorder: writing thread done."));
        });

        let _visualizer_thread = s.spawn(|| {
            while c_controller_visualizer_thread.recorder_running() {
                let new_audio = visualizer_r.recv();
                if let Ok(output) = new_audio {
                    // TODO: test this -> It might be just as fast/free to cast_to_f32 safely
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
            log(&String::from("Recorder: visualizing thread done."));
        });
    });

    mic.pause();
}

fn run_recording(controller: WhisperAppController, configs: RecorderConfigs) {
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
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );

            clear_message_queue(controller.0.record_audio_i16_receiver.clone());
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
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );
            clear_message_queue(controller.0.record_audio_i32_receiver.clone());
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
                &desired_audio,
                spec,
                (sender, receiver),
                filter,
                f_central,
            );
            clear_message_queue(controller.0.record_audio_f32_receiver.clone());
        }
    }
    controller.0.audio_worker_state.store(AudioWorkerState::Idle, Ordering::Release);
}

// TODO: add a progress job to this setup.
fn run_realtime_audio_transcription(
    controller: WhisperAppController,
    configs: Arc<whisper_realtime::configs::Configs>,
    filtering: (bool, f32, f32),
) -> Result<String, Box<dyn Any + Send>> {
    // Audio filtering
    let filter = filtering.0;
    let f_higher = filtering.1;
    let f_lower = filtering.2;
    let f_central = f_central(f_lower, f_higher);

    // Clear the text buffer.
    controller.clear_transcription_buffer();
    // Clone the controller
    let c_controller_audio_thread = controller.clone();
    let c_controller_write_thread = controller.clone();
    let c_controller_visualizer_thread = controller.clone();
    let c_controller_transcription_reader_thread = controller.clone();

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
    let (s_writer, r_writer) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    let audio_write_sender: Sender<Vec<f32>> = s_writer.clone();
    let audio_write_receiver = r_writer.clone();
    // Audio sender for transcription visualizer
    let (s_visualizer, r_visualizer) = bounded(whisper_realtime::constants::INPUT_BUFFER_CAPACITY);
    let audio_visualizer_sender: Sender<Vec<f32>> = s_visualizer.clone();
    let audio_visualizer_receiver = r_visualizer.clone();

    // State Flags - This should likely be refactored.
    let c_realtime_is_ready = controller.0.realtime_ready.clone();
    let c_realtime_is_running = controller.0.realtime_running.clone();

    let mic_stream = init_realtime_microphone(
        &controller.0.audio_wrapper.audio_subsystem,
        controller.0.record_audio_f32_sender.clone(),
    );
    let audio_spec = Arc::new(mic_stream.spec().clone());
    let sample_rate = audio_spec.freq as f64;
    let sample_rate_bandpass = sample_rate as f32;

    let c_audio_spec = audio_spec.clone();
    let c_mic_stream = mic_stream.clone();

    // Init Whisper.
    let ctx = init_whisper_ctx(c_model.clone(), c_configs.use_gpu);
    let mut state = ctx.create_state().expect("Failed to create WhisperState");

    // Focus the transcriber tab.
    controller
        .send_focus_tab(FocusTab::Transcription)
        .expect("Focus tab channel closed");

    // TODO: refactor panics.
    let transcription = thread::scope(|s| {
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
                            let mut guard = c_controller_visualizer_thread
                                .0
                                .visualizer_buffer
                                .write()
                                .expect("Visualizer RwLock Poisoned");
                            normalized_waveform(&output, &mut guard);
                        }
                        AnalysisType::Power => {
                            let mut guard = c_controller_visualizer_thread
                                .0
                                .visualizer_buffer
                                .write()
                                .expect("Visualizer RwLock Poisoned");
                            power_analysis(&output, &mut guard);
                        }
                        AnalysisType::SpectrumDensity => {
                            let mut guard = c_controller_visualizer_thread
                                .0
                                .visualizer_buffer
                                .write()
                                .expect("Visualizer RwLock Poisoned");
                            frequency_analysis(&output, &mut guard, sample_rate);
                        }
                    }
                }
            }
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
                assert!(output.is_ok(), "Realtime Audio Channel closed");

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
            clear_message_queue(c_controller_audio_thread.0.record_audio_f32_receiver.clone());

            // Closed properly
            log(&"Audio reader thread closed properly");
        });

        let _transcription_reader_thread = s.spawn(move || {
            while c_controller_transcription_reader_thread.realtime_running() {
                let text = c_controller_transcription_reader_thread
                    .0
                    .transcription_text_receiver
                    .recv();
                match text {
                    Ok(result) => {
                        match result {
                            Ok(text_packet) => {
                                if text_packet.0 == constants::GO_MSG {
                                    c_controller_transcription_reader_thread.0.audio_worker_state.store(AudioWorkerState::Running, Ordering::Release);
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
                                        // Mutex is poisoned.  TODO: proper handling.
                                        let mut text_buffer = poison.into_inner();
                                        if text_packet.1 {
                                            text_buffer.push(text_packet.0)
                                        } else {
                                            let last_entry_index = text_buffer.len() - 1;
                                            text_buffer[last_entry_index] = text_packet.0;
                                        }
                                    }
                                }
                            }
                            Err(_e) => {
                                // TODO: error msg.
                                c_realtime_is_running_transcription_reader_thread
                                    .store(false, Ordering::Release);
                            }
                        }
                    }
                    Err(_e) => {
                        // TODO: error msg - transcription channel is closed.
                        c_realtime_is_running_transcription_reader_thread
                            .store(false, Ordering::Release);
                        break;
                    }
                }
            }

            // TODO: factor out.
            // Clear the text channel
            clear_message_queue(c_controller_transcription_reader_thread.0.transcription_text_receiver.clone());
            // Closed properly
            log(&"Transcription reader thread closed properly");
        });
        let transcription_runner_thread = s
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
                log(&"Transcription runner thread closed properly");
                output
            })
            .join();
        transcription_runner_thread
    });

    c_mic_stream.pause();

    let result = match transcription {
        Ok(t) => {
            let mut guard = controller
                .0
                .transcription_buffer
                .write()
                .expect("RwLock Poisoned");
            guard.clear();
            guard.push(t);
            Ok(String::from("Transcription Complete"))
        }
        Err(e) => {
            // TODO: write an app error to propagate
            Err(e)
        }
    };


    // Update state
    controller
        .0
        .save_recording_ready
        .store(true, Ordering::Release);
    controller.0.audio_worker_state.store(AudioWorkerState::Idle, Ordering::Release);
    result
}

// TODO: add progress job.
fn run_static_audio_transcription(
    audio: Vec<f32>,
    controller: WhisperAppController,
    configs: Arc<whisper_realtime::configs::Configs>,
) -> Result<String, Box<dyn Any + Send>> {
    // Clear the text buffer.
    controller.clear_transcription_buffer();

    let c_controller_runner_thread = controller.clone();
    let c_controller_reader_thread = controller.clone();
    let model = init_model(configs.clone());
    let audio = SupportedAudioSample::F32(audio);
    let audio = Arc::new(Mutex::new(audio));

    let data_sender = controller.0.transcription_text_sender.clone();
    let data_sender = Some(data_sender);
    let channels = SupportedChannels::MONO;

    // Init Whisper.
    let ctx = init_whisper_ctx(model.clone(), configs.use_gpu);
    let mut state = ctx.create_state().expect("Failed to create WhisperState");

    // Progress callback.
    let p_controller = controller.clone();
    let transcriber_job_name = "Transcribing Audio";
    let total_size = 100;

    let progress_callback = move |n: i32| {
        let progress = Progress::new(String::from(transcriber_job_name), n as usize, total_size);
        p_controller
            .send_progress(progress)
            .expect("Progress channel closed");
    };

    let progress_callback = Some(progress_callback);
    // Focus transcription tab
    controller
        .send_focus_tab(FocusTab::Transcription)
        .expect("Transcription channel closed");

    let transcription_thread = thread::scope(|s| {
        let transcription_runner_thread = s
            .spawn(move || {
                let mut transcriber =
                    StaticTranscriber::new_with_configs(audio, data_sender, configs, channels);

                c_controller_runner_thread.0.audio_worker_state.store(AudioWorkerState::Running, Ordering::Release);

                let output = transcriber.process_audio(&mut state, progress_callback);
                // TODO: check library for whether set internally.
                c_controller_runner_thread
                    .0
                    .static_running
                    .store(false, Ordering::Release);
                // Final progress update -> Whisper finishes before the final callback.

                let progress = Progress::new(String::from(transcriber_job_name), 1, 1);
                c_controller_runner_thread
                    .send_progress(progress)
                    .expect("Progress channel closed");
                log(&String::from("Transcription runner finished"));
                output
            })
            .join();

        // Transcription reader thread.
        let _reader_thread = s.spawn(move || {
            while c_controller_reader_thread.static_running() {
                let text = c_controller_reader_thread
                    .0
                    .transcription_text_receiver
                    .recv();
                match text {
                    Ok(result) => {
                        match result {
                            Ok(text_packet) => {
                                if text_packet.0 == constants::GO_MSG {
                                    // TODO: SET STATE ENUM FROM PROGRESS TO GO
                                    continue;
                                }

                                let guard =
                                    c_controller_reader_thread.0.transcription_buffer.write();
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
                                        if text_packet.1 {
                                            text_buffer.push(text_packet.0);
                                        } else {
                                            let last_entry_index = text_buffer.len() - 1;
                                            text_buffer[last_entry_index] = text_packet.0;
                                        }
                                    }
                                }
                            }
                            Err(_e) => {
                                // TODO: send error msg
                                c_controller_reader_thread
                                    .0
                                    .static_running
                                    .store(false, Ordering::Release);
                            }
                        }
                    }
                    Err(_e) => {
                        // TODO: proper error -> transcription channel closed.
                        // Might be able to recover by re-initializing the controller.
                        c_controller_reader_thread
                            .0
                            .static_running
                            .store(false, Ordering::Release);
                    }
                }
            }

            // Clear the text channel
            clear_message_queue(c_controller_reader_thread.0.transcription_text_receiver.clone());
            log(&String::from("Transcription reader finished"));
        });
        transcription_runner_thread
    });

    let result = match transcription_thread {
        Ok(t) => {
            let mut guard = controller
                .0
                .transcription_buffer
                .write()
                .expect("RwLock poisoned");
            guard.clear();
            guard.push(t);
            Ok(String::from("Transcription Complete"))
        }
        Err(e) => {
            // TODO: Proper error
            Err(e)
        }
    };
    controller.0.audio_worker_state.store(AudioWorkerState::Idle, Ordering::Release);

    result
}

fn clear_message_queue<T>(queue: Receiver<T>) {
    while let Ok(_) = queue.try_recv() {
        continue;
    }
}

// TODO: JoinHandle errors should be WhisperAppErrors.
type WhisperAppThread = (WorkerType, JoinHandle<Result<String, Box<dyn Any + Send>>>);

#[derive(Debug)]
struct WhisperAppContext {
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
    application_running: Arc<AtomicBool>,
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
    progress_sender: Sender<Progress>,
    progress_receiver: Receiver<Progress>,

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
        let downloading = Arc::new(AtomicBool::new(false));
        let realtime_ready = Arc::new(AtomicBool::new(false));
        let static_ready = Arc::new(AtomicBool::new(false));
        let save_recording_ready = Arc::new(AtomicBool::new(false));

        let realtime_running = Arc::new(AtomicBool::new(false));
        let static_running = Arc::new(AtomicBool::new(false));
        let recorder_running = Arc::new(AtomicBool::new(false));

        let application_running = Arc::new(AtomicBool::new(true));
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
            gpu_support,
            client,
            handle,
            system_theme,
            audio_wrapper,
            audio_worker_state,
            application_running,
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

    (hip && blas) || found_path
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

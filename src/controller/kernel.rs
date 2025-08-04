use crate::controller::console::ConsoleEngine;
use crate::controller::downloader::DownloadEngine;
use crate::controller::model_bank::RibbleModelBank;
use crate::controller::progress::ProgressEngine;
use crate::controller::recorder::RecorderEngine;
use crate::controller::transcriber::TranscriberEngine;
use crate::controller::visualizer::VisualizerEngine;
use crate::controller::worker::WorkerEngine;
use crate::controller::writer::WriterEngine;
use crate::controller::{AmortizedDownloadProgress, AmortizedProgress, Bus, CompletedRecordingJobs, ConsoleMessage, ModelFile, OfflineTranscriberFeedback, Progress, RotationDirection, DEFAULT_PROGRESS_SLAB_CAPACITY, NUM_VISUALIZER_BUCKETS, SMALL_UTILITY_QUEUE_SIZE, UTILITY_QUEUE_SIZE};
use crate::controller::{AnalysisType, FileDownload};
use crate::utils::errors::RibbleError;
use crate::utils::preferences::UserPreferences;
use crate::utils::recorder_configs::{RibbleRecordingConfigs, RibbleRecordingExportFormat};
use crate::utils::vad_configs::VadConfigs;

use crate::controller::audio_backend_proxy::AudioBackendProxy;
use arc_swap::ArcSwap;
use ribble_whisper::transcriber::{TranscriptionSnapshot, WhisperControlPhrase};
use ribble_whisper::utils::get_channel;
use ribble_whisper::whisper::configs::WhisperRealtimeConfigs;
use ribble_whisper::whisper::model::ModelId;
use ron::ser::PrettyConfig;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// NOTE: At the moment, it's a lot easier to just work with the concrete proxy.
// Until generics are absolutely required for testing/swapping different audio backends,
// avoid using them here.
pub(super) struct Kernel {
    data_directory: PathBuf,
    user_preferences: ArcSwap<UserPreferences>,
    // NOTE: pass this -in- to the RecorderEngine/TranscriberEngine as arguments
    audio_backend: Arc<AudioBackendProxy>,
    transcriber_engine: TranscriberEngine,
    recorder_engine: RecorderEngine,
    console_engine: ConsoleEngine,
    progress_engine: ProgressEngine,
    visualizer_engine: VisualizerEngine,
    _worker_engine: WorkerEngine,
    writer_engine: WriterEngine,
    download_engine: DownloadEngine,
    // Since model bank needs to be accessed elsewhere (e.g. TranscriberEngine), this needs to be
    // in a shared pointer.
    model_bank: Arc<RibbleModelBank>,
    bus: Bus,
}

impl Kernel {
    const CONFIGS_FILE: &'static str = "ribble_configs.ron";
    const MODEL_BANK_DIR_SLUG: &'static str = "models";
    const TEMP_AUDIO_DIR_SLUG: &'static str = "recordings";

    // NOTE: this needs to take in the audio capture request sender from the app (main thread)
    // because of SDL invariants.
    pub(super) fn new(
        data_directory: &Path,
        audio_backend: AudioBackendProxy,
    ) -> Result<Self, RibbleError> {
        if !data_directory.is_absolute() {
            return Err(RibbleError::Core(format!(
                "Data directory not canonicalized: {data_directory:#?}"
            )));
        }

        let KernelState {
            transcriber_configs,
            offline_transcriber_feedback,
            vad_configs,
            recording_configs,
            user_preferences,
        } = Self::deserialize_user_data(data_directory);
        let (console_sender, console_receiver) = get_channel(UTILITY_QUEUE_SIZE);
        let (progress_sender, progress_receiver) = get_channel(SMALL_UTILITY_QUEUE_SIZE);
        let (work_sender, work_receiver) = get_channel(UTILITY_QUEUE_SIZE);
        let (write_sender, write_receiver) = get_channel(UTILITY_QUEUE_SIZE);
        let (visualizer_sender, visualizer_receiver) = get_channel(UTILITY_QUEUE_SIZE);
        let (download_sender, download_receiver) = get_channel(SMALL_UTILITY_QUEUE_SIZE);
        let bus = Bus::new(
            console_sender,
            progress_sender,
            work_sender,
            write_sender,
            visualizer_sender,
            download_sender,
        );

        let recorder_engine = RecorderEngine::new(recording_configs, &bus);
        let console_engine = ConsoleEngine::new(
            console_receiver,
            user_preferences.console_message_size(),
            &bus,
        );
        let progress_engine =
            ProgressEngine::new(DEFAULT_PROGRESS_SLAB_CAPACITY, progress_receiver);
        let visualizer_engine = VisualizerEngine::new(visualizer_receiver);
        let worker_engine = WorkerEngine::new(work_receiver, &bus)?;

        let recording_directory = data_directory.join(Self::TEMP_AUDIO_DIR_SLUG);
        let writer_engine = WriterEngine::new(recording_directory, write_receiver, &bus);
        let download_engine = DownloadEngine::new(download_receiver, &bus);

        let model_directory = data_directory.join(Self::MODEL_BANK_DIR_SLUG);
        let model_bank = Arc::new(RibbleModelBank::new(model_directory.as_path(), &bus)?);

        // In case the user has mucked around with the model directory and the previous ID is
        // invalid in the configs, catch it before constructing the TranscriberEngine and set the
        // configs ID to None.
        let model_id = transcriber_configs
            .model_id()
            .as_ref()
            .and_then(|model_id| {
                if !model_bank.contains_model(*model_id) {
                    None
                } else {
                    Some(*model_id)
                }
            });

        let transcriber_configs = transcriber_configs.with_model_id(model_id);

        // NOTE: to avoid already modifying the transcriber engine, just construct it last after
        // the ID check has been run.
        let transcriber_engine = TranscriberEngine::new(
            transcriber_configs,
            vad_configs,
            offline_transcriber_feedback,
            &bus,
        );

        Ok(Self {
            data_directory: data_directory.to_path_buf(),
            user_preferences: ArcSwap::from(Arc::new(user_preferences)),
            audio_backend: Arc::new(audio_backend),
            transcriber_engine,
            recorder_engine,
            console_engine,
            progress_engine,
            visualizer_engine,
            _worker_engine: worker_engine,
            writer_engine,
            download_engine,
            model_bank,
            bus,
        })
    }
    // USER PREFERENCES
    pub(super) fn read_user_preferences(&self) -> Arc<UserPreferences> {
        self.user_preferences.load_full()
    }

    pub(super) fn write_user_preferences(&self, new_prefs: UserPreferences) {
        // Atomic Swap the new in for the old
        let old_prefs = *self.user_preferences.swap(Arc::new(new_prefs));

        // Check the messages buffer size to see if a resize needs to happen.
        let new_message_size = new_prefs.console_message_size();
        if old_prefs.console_message_size() != new_message_size {
            self.resize_console_message_buffer(new_message_size);
        }
    }

    pub(super) fn get_app_theme(&self) -> Option<catppuccin_egui::Theme> {
        self.user_preferences.load().system_theme().app_theme()
    }

    pub(super) fn get_system_visuals(&self) -> Option<egui::Visuals> {
        self.user_preferences.load().system_theme().visuals()
    }

    pub(super) fn get_system_gradient(&self) -> Option<egui_colorgradient::Gradient> {
        self.user_preferences.load().system_theme().gradient()
    }

    // TODO: perhaps these methods should be trait methods if the controller needs to be testable.
    // MODEL MANAGEMENT
    pub(super) fn download_model(&self, url: &str) {
        self.model_bank.download_new_model(url);
    }
    pub(super) fn copy_new_model_to_bank(&self, file_path: PathBuf) {
        self.model_bank.copy_model_to_bank(file_path);
    }

    pub(super) fn get_model_key(&self, model_key: &str) -> ModelId {
        self.model_bank.create_model_key(model_key)
    }

    pub(super) fn get_model_directory(&self) -> &Path {
        self.model_bank.model_directory()
    }

    // (ID, File name)
    pub(super) fn try_read_model_list(&self, copy_buffer: &mut Vec<(ModelId, ModelFile)>) {
        self.model_bank.try_read_model_list(copy_buffer);
    }

    // TRANSCRIBER
    pub(super) fn read_transcription_configs(&self) -> Arc<WhisperRealtimeConfigs> {
        self.transcriber_engine.read_transcription_configs()
    }
    pub(super) fn write_transcription_configs(&self, new_configs: WhisperRealtimeConfigs) {
        self.transcriber_engine
            .write_transcription_configs(new_configs);
    }
    pub(super) fn read_vad_configs(&self) -> Arc<VadConfigs> {
        self.transcriber_engine.read_vad_configs()
    }
    pub(super) fn write_vad_configs(&self, new_configs: VadConfigs) {
        self.transcriber_engine.write_vad_configs(new_configs);
    }
    pub(super) fn read_offline_transcriber_feedback(&self) -> OfflineTranscriberFeedback {
        self.transcriber_engine.read_offline_transcriber_feedback()
    }
    pub(super) fn write_offline_transcriber_feedback(
        &self,
        new_feedback: OfflineTranscriberFeedback,
    ) {
        self.transcriber_engine
            .write_offline_transcriber_feedback(new_feedback);
    }
    pub(super) fn realtime_running(&self) -> bool {
        self.transcriber_engine.realtime_running()
    }
    pub(super) fn offline_running(&self) -> bool {
        self.transcriber_engine.offline_running()
    }
    pub(super) fn transcriber_running(&self) -> bool {
        self.transcriber_engine.transcriber_running()
    }

    pub(super) fn stop_realtime(&self) {
        self.transcriber_engine.stop_realtime()
    }
    pub(super) fn stop_offline(&self) {
        self.transcriber_engine.stop_offline()
    }
    pub(super) fn read_transcription_snapshot(&self) -> Arc<TranscriptionSnapshot> {
        self.transcriber_engine.read_transcription_snapshot()
    }
    pub(super) fn read_latest_control_phrase(&self) -> Arc<WhisperControlPhrase> {
        self.transcriber_engine.read_latest_control_phrase()
    }

    pub(super) fn read_current_audio_file_path(&self) -> Arc<Option<PathBuf>> {
        self.transcriber_engine.read_current_audio_file_path()
    }

    pub(super) fn start_realtime_transcription(&self) {
        let bank = Arc::clone(&self.model_bank);
        let backend = Arc::clone(&self.audio_backend);
        self.transcriber_engine
            .start_realtime_transcription(backend, bank);
    }

    pub(super) fn set_audio_file_path(&self, path: PathBuf) {
        self.transcriber_engine.set_current_audio_file_path(path);
    }
    pub(super) fn clear_audio_file_path(&self) {
        self.transcriber_engine.clear_current_audio_file_path();
    }

    // NOTE: The WriterEngine will update its own state if its recording cache is empty.
    // NOTE TWICE: This does not guarantee there won't be a file issue if the recording is missing.
    pub(super) fn try_retranscribe_latest(&self) {
        if let Some(path) = self.try_get_latest_recording() {
            self.set_audio_file_path(path);
            self.start_realtime_transcription();
        }
    }

    pub(super) fn start_offline_transcription(&self) {
        let bank = Arc::clone(&self.model_bank);
        self.transcriber_engine.start_offline_transcription(bank);
    }

    pub(super) fn save_transcription(&self, out_path: PathBuf) {
        self.transcriber_engine.save_transcription(out_path);
    }

    // RECORDER
    pub(super) fn recorder_running(&self) -> bool {
        self.recorder_engine.recorder_running()
    }

    pub(super) fn read_recorder_configs(&self) -> Arc<RibbleRecordingConfigs> {
        self.recorder_engine.read_recorder_configs()
    }
    pub(super) fn write_recorder_configs(&self, new_configs: RibbleRecordingConfigs) {
        self.recorder_engine.write_recorder_configs(new_configs);
    }

    pub(super) fn start_recording(&self) {
        let backend = Arc::clone(&self.audio_backend);
        self.recorder_engine.start_recording(backend);
    }
    pub(super) fn stop_recording(&self) {
        self.recorder_engine.stop_recording();
    }

    // WRITER (RECORDINGS + Export)
    pub(super) fn is_clearing_recordings(&self) -> bool {
        self.writer_engine.is_clearing()
    }
    pub(super) fn clear_recording_cache(&self) {
        self.writer_engine.clear_cache()
    }
    pub(super) fn latest_recording_exists(&self) -> bool {
        self.writer_engine.latest_exists()
    }
    pub(super) fn try_get_latest_recording(&self) -> Option<PathBuf> {
        self.writer_engine.try_get_latest()
    }

    pub(super) fn get_num_recordings(&self) -> usize {
        self.writer_engine.get_num_completed()
    }
    pub(super) fn try_get_completed_recordings(
        &self,
        copy_buffer: &mut Vec<(Arc<str>, CompletedRecordingJobs)>,
    ) {
        self.writer_engine.try_get_completed_jobs(copy_buffer)
    }

    // NOTE: this consumes a shared string -> clone higher up and consume it
    pub(super) fn try_get_recording_path(&self, file_name: Arc<str>) -> Option<PathBuf> {
        self.writer_engine.get_recording_path(file_name)
    }

    // NOTE: recording_file_name is internal -- It's the left-half of the (String, CompletedRecordingJobs) tuple.
    pub(super) fn export_recording(
        &self,
        out_path: PathBuf,
        recording_file_name: Arc<str>,
        output_format: RibbleRecordingExportFormat,
    ) {
        self.writer_engine
            .export_recording(out_path, recording_file_name, output_format);
    }

    // CONSOLE
    pub(super) fn try_get_current_messages(&self, copy_buffer: &mut Vec<Arc<ConsoleMessage>>) {
        self.console_engine.try_get_current_messages(copy_buffer);
    }
    pub(super) fn resize_console_message_buffer(&self, new_size: usize) {
        self.console_engine.resize(new_size)
    }

    // PROGRESS
    pub(super) fn try_get_current_jobs(&self, copy_buffer: &mut Vec<Progress>) {
        self.progress_engine.try_get_current_jobs(copy_buffer);
    }

    pub(super) fn try_get_amortized_jobs(&self) -> Option<AmortizedProgress> {
        self.progress_engine.try_get_amortized_progress()
    }

    // DOWNLOADER
    pub(super) fn try_get_current_downloads(&self, copy_buffer: &mut Vec<(usize, FileDownload)>) {
        self.download_engine.try_get_current_downloads(copy_buffer);
    }

    pub(super) fn try_get_amortized_download_progress(&self) -> Option<AmortizedDownloadProgress> {
        self.download_engine.try_get_amortized_download_progress()
    }

    pub(super) fn abort_download(&self, download_id: usize) {
        self.download_engine.abort_download(download_id);
    }

    // VISUALIZER
    pub(super) fn set_visualizer_visibility(&self, is_visible: bool) {
        self.visualizer_engine.set_visualizer_visibility(is_visible);
    }
    pub(super) fn try_read_visualization_buffer(
        &self,
        copy_buffer: &mut [f32; NUM_VISUALIZER_BUCKETS],
    ) {
        self.visualizer_engine
            .try_read_visualization_buffer(copy_buffer);
    }

    pub(super) fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.visualizer_engine.get_visualizer_analysis_type()
    }
    pub(super) fn set_visualizer_analysis_type(&self, new_type: AnalysisType) {
        self.visualizer_engine
            .set_visualizer_analysis_type(new_type);
    }
    pub(super) fn rotate_visualizer_type(&self, direction: RotationDirection) {
        self.visualizer_engine.rotate_visualizer_type(direction);
    }

    pub(super) fn serialize_user_data(&self) {
        let transcriber_configs = *self.transcriber_engine.read_transcription_configs();
        let offline_transcriber_feedback =
            self.transcriber_engine.read_offline_transcriber_feedback();
        let vad_configs = *self.transcriber_engine.read_vad_configs();
        let recording_configs = *self.recorder_engine.read_recorder_configs();
        let user_preferences = *self.user_preferences.load_full();

        let state = KernelState {
            transcriber_configs,
            offline_transcriber_feedback,
            vad_configs,
            recording_configs,
            user_preferences,
        };

        let canonicalized = self.data_directory.to_path_buf().join(Self::CONFIGS_FILE);
        match File::create(canonicalized.as_path()) {
            Ok(configs_file) => {
                let writer = BufWriter::new(configs_file);
                match ron::Options::default().to_io_writer_pretty(writer, &state, PrettyConfig::default()) {
                    Ok(_) => {
                        log::info!("User data serialized to: {}", canonicalized.display());
                    }
                    Err(e) => {
                        log::warn!("Failed to serialize user data: {e}");
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to create user data file: {e}");
            }
        }
    }

    fn deserialize_user_data(data_directory: &Path) -> KernelState {
        let canonicalized = data_directory.to_path_buf().join(Self::CONFIGS_FILE);
        match File::open(&canonicalized) {
            Ok(configs_file) => {
                let reader = BufReader::new(configs_file);
                ron::de::from_reader(reader).unwrap_or_else(|e| {
                    log::warn!("Error deserializing user data: {e}");
                    KernelState::default()
                })
            }
            Err(e) => {
                log::warn!("Error deserializing user data: {e}");
                KernelState::default()
            }
        }
    }
}

impl Drop for Kernel {
    fn drop(&mut self) {
        log::info!("Dropping Kernel.");
        log::info!("Launching sentinels to close engines.");
        self.bus.try_close_bus();
        log::info!("Serializing user data.");
        self.serialize_user_data();
        log::info!("Kernel cleanup completed.");
    }
}

// Basic serializable/deserializable app state.
// At this time, there isn't much to keep track of, but this may
// start to grow as features get added.
//
// As of now, everything does implement default, so if the resource is missing/gets lost, there's a
// fallback.
#[derive(Default, serde::Serialize, serde::Deserialize)]
struct KernelState {
    #[serde(default)]
    transcriber_configs: WhisperRealtimeConfigs,
    #[serde(default)]
    offline_transcriber_feedback: OfflineTranscriberFeedback,
    #[serde(default)]
    vad_configs: VadConfigs,
    #[serde(default)]
    recording_configs: RibbleRecordingConfigs,
    #[serde(default)]
    user_preferences: UserPreferences,
}

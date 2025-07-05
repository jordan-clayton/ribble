use crate::controller::console::ConsoleEngine;
use crate::controller::downloader::DownloadEngine;
use crate::controller::progress::ProgressEngine;
use crate::controller::recorder::RecorderEngine;
use crate::controller::transcriber::TranscriberEngine;
use crate::controller::visualizer::{AnalysisType, RotationDirection, VisualizerEngine, NUM_VISUALIZER_BUCKETS};
use crate::controller::worker::WorkerEngine;
use crate::controller::writer::WriterEngine;
use crate::controller::{Bus, CompletedRecordingJobs, ConsoleMessage, OfflineTranscriberFeedback, Progress, DEFAULT_PROGRESS_SLAB_CAPACITY, SMALL_UTILITY_QUEUE_SIZE, UTILITY_QUEUE_SIZE};
use crate::utils::errors::RibbleError;
use crate::utils::model_bank::{RibbleModelBank, RibbleModelBankIter};
use crate::utils::preferences::UserPreferences;
use crate::utils::recorder_configs::{RibbleRecordingConfigs, RibbleRecordingExportFormat};
use crate::utils::vad_configs::VadConfigs;
use arc_swap::ArcSwap;
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::transcriber::{TranscriptionSnapshot, WhisperControlPhrase};
use ribble_whisper::utils::get_channel;
use ribble_whisper::whisper::configs::WhisperRealtimeConfigs;
use ribble_whisper::whisper::model::{ConcurrentModelBank, Model, ModelId};
use ron::ser::PrettyConfig;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// TODO: NOTE TO SELF, store this in the controller instead of the old spaghetti.
// The spaghetti is now portioned onto different plates, to make things easier to manage.
pub(super) struct Kernel<A: AudioBackend<ArcChannelSink<f32>>> {
    data_directory: PathBuf,
    user_preferences: ArcSwap<UserPreferences>,
    // NOTE: pass this -in- to the RecorderEngine/TranscriberEngine as arguments
    audio_backend: A,
    transcriber_engine: TranscriberEngine,
    recorder_engine: RecorderEngine,
    console_engine: ConsoleEngine,
    progress_engine: ProgressEngine,
    visualizer_engine: VisualizerEngine,
    worker_engine: WorkerEngine,
    writer_engine: WriterEngine,
    download_engine: DownloadEngine,
    // Since model bank needs to be accessed elsewhere (e.g. TranscriberEngine), this needs to be
    // in a shared pointer.
    model_bank: Arc<RibbleModelBank>,
}

impl<A: AudioBackend<ArcChannelSink<f32>>> Kernel<A> {
    const CONFIGS_FILE: &'static str = "ribble_configs.ron";
    const MODEL_BANK_DIR_SLUG: &'static str = "models";
    const TEMP_AUDIO_DIR_SLUG: &'static str = "recordings";

    // NOTE: this needs to take in the audio capture request sender from the app (main thread)
    // because of SDL invariants.
    pub(super) fn new(data_directory: &Path, audio_backend: A) -> Result<Self, RibbleError> {
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

        let transcriber_engine = TranscriberEngine::new(
            transcriber_configs,
            vad_configs,
            offline_transcriber_feedback,
            &bus,
        );

        let recorder_engine = RecorderEngine::new(recording_configs, &bus);
        let console_engine = ConsoleEngine::new(
            console_receiver,
            user_preferences.console_message_size(),
            &bus,
        );
        let progress_engine = ProgressEngine::new(DEFAULT_PROGRESS_SLAB_CAPACITY, progress_receiver);
        let visualizer_engine = VisualizerEngine::new(visualizer_receiver);
        let worker_engine = WorkerEngine::new(work_receiver, &bus);

        let recording_directory = data_directory.join(Self::CONFIGS_FILE);
        let writer_engine = WriterEngine::new(recording_directory, write_receiver, &bus);
        let download_engine = DownloadEngine::new(download_receiver, &bus);

        let model_directory = data_directory.join(Self::MODEL_BANK_DIR_SLUG);
        let model_bank = Arc::new(RibbleModelBank::new(model_directory.as_path())?);

        Ok(Self {
            data_directory: data_directory.to_path_buf(),
            user_preferences: ArcSwap::from(Arc::new(user_preferences)),
            audio_backend,
            transcriber_engine,
            recorder_engine,
            console_engine,
            progress_engine,
            visualizer_engine,
            worker_engine,
            writer_engine,
            download_engine,
            model_bank,
        })
    }

    // TODO: perhaps these methods should be trait methods if the controller needs to be testable.
    // MODEL MANAGEMENT
    pub(super) fn download_model(&self, url: String) {
        todo!("Add download method to RibbleModelBank")
    }
    pub(super) fn copy_new_model(&self, file_path: &Path) {
        todo!("Refactor new Model method in RibbleModelBank")
    }

    pub(super) fn rename_model(&self, model_id: ModelId, new_name: String) -> Result<Option<ModelId>, RibbleError> {
        Ok(self.model_bank.rename_model(model_id, new_name)?)
    }

    pub(super) fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleError> {
        Ok(self.model_bank.model_exists_in_storage(model_id)?)
    }

    pub(super) fn delete_model(&self, model_id: ModelId) -> Result<Option<Model>, RibbleError> {
        Ok(self.model_bank.remove_model(model_id)?)
    }

    pub(super) fn refresh_model_bank(&self) -> Result<(), RibbleError> {
        Ok(self.model_bank.refresh_model_bank()?)
    }

    pub(super) fn get_model_list(&self) -> RibbleModelBankIter {
        self.model_bank.iter()
    }

    // TRANSCRIBER
    pub(super) fn read_transcription_configs(&self) -> Arc<WhisperRealtimeConfigs> {
        self.transcriber_engine.read_transcription_configs()
    }
    pub(super) fn write_transcription_configs(&self, new_configs: WhisperRealtimeConfigs) {
        self.transcriber_engine.write_transcription_configs(new_configs);
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
    pub(super) fn write_offline_transcriber_feedback(&self, new_feedback: OfflineTranscriberFeedback) {
        self.transcriber_engine.write_offline_transcriber_feedback(new_feedback);
    }
    pub(super) fn realtime_running(&self) -> bool {
        self.transcriber_engine.realtime_running()
    }
    pub(super) fn offline_running(&self) -> bool {
        self.transcriber_engine.offline_running()
    }
    pub(super) fn transcriber_running(&self) -> bool {
        self.transcriber_running()
    }

    pub(super) fn stop_realtime(&self) {
        self.transcriber_engine.stop_realtime()
    }
    pub(super) fn stop_offline(&self) {
        self.transcriber_engine.stop_realtime()
    }
    pub(super) fn read_transcription_snapshot(&self) -> Arc<TranscriptionSnapshot> {
        self.transcriber_engine.read_transcription_snapshot()
    }
    pub(super) fn read_latest_control_phrase(&self) -> Arc<WhisperControlPhrase> {
        self.transcriber_engine.read_latest_control_phrase()
    }

    pub(super) fn start_realtime_transcription(&self) {
        let bank = Arc::clone(&self.model_bank);

        self.transcriber_engine
            .start_realtime_transcription(&self.audio_backend, bank);
    }

    pub(super) fn start_offline_transcription(&self, audio_file: &Path) {
        let bank = Arc::clone(&self.model_bank);
        self.transcriber_engine
            .start_offline_transcription(audio_file, bank);
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
        self.recorder_engine.start_recording(&self.audio_backend);
    }

    // WRITER (RECORDINGS + Export)
    pub(super) fn is_clearing_recordings(&self) -> bool {
        self.writer_engine.is_clearing()
    }
    pub(super) fn clear_recording_cache(&self) {
        self.writer_engine.clear_cache()
    }
    pub(super) fn try_get_latest_recording(&self) -> Option<PathBuf> {
        self.writer_engine.try_get_latest()
    }
    pub(super) fn try_get_completed_recordings(&self, copy_buffer: &mut Vec<(String, CompletedRecordingJobs)>) {
        self.writer_engine.try_get_completed_jobs(copy_buffer)
    }

    pub(super) fn try_get_recording_path(&self, file_name: &str) -> Option<PathBuf> {
        self.writer_engine.get_recording_path(file_name)
    }

    // NOTE: recording_file_name is internal -- It's the left-half of the (String, CompletedRecordingJobs) tuple.
    pub(super) fn export_recording(&self, out_path: &Path, recording_file_name: &str, output_format: RibbleRecordingExportFormat) {
        self.writer_engine.export(out_path, recording_file_name, output_format);
    }

    // CONSOLE
    pub(super) fn try_get_current_message(&self, copy_buffer: &mut Vec<Arc<ConsoleMessage>>) {
        self.console_engine.try_get_current_message(copy_buffer);
    }
    pub(super) fn resize_console_message_buffer(&self, new_size: usize) {
        self.console_engine.resize(new_size)
    }

    // PROGRESS
    pub(super) fn try_get_current_jobs(&self, copy_buffer: &mut Vec<Progress>) {
        self.progress_engine.try_get_current_jobs(copy_buffer);
    }

    // VISUALIZER
    pub(super) fn set_visualizer_visibility(&self, is_visible: bool) {
        self.set_visualizer_visibility(is_visible);
    }
    pub(super) fn try_read_visualization_buffer(&self, copy_buffer: &mut [f32; NUM_VISUALIZER_BUCKETS]) {
        self.visualizer_engine.try_read_visualization_buffer(copy_buffer);
    }

    pub(super) fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.visualizer_engine.get_visualizer_analysis_type()
    }
    pub(super) fn rotate_visualizer_type(&self, direction: RotationDirection) {
        self.visualizer_engine.rotate_visualizer_type(direction);
    }

    // TODO: return a result or log.
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

        let configs_file = File::open(self.data_directory.as_path());
        if let Ok(file) = configs_file {
            let writer = BufWriter::new(file);
            if ron::Options::default().to_io_writer_pretty(writer, &state, PrettyConfig::default()).is_err() {
                todo!("LOGGING");
            }
        } else {
            todo!("LOGGING");
        }
    }

    fn deserialize_user_data(data_directory: &Path) -> KernelState {
        let canonicalized = data_directory.to_path_buf().join(Self::CONFIGS_FILE);
        match File::open(&canonicalized) {
            Ok(file) => {
                let reader = BufReader::new(file);
                match ron::de::from_reader(reader) {
                    Ok(state) => state,
                    Err(_) => {
                        todo!("LOGGING");
                        KernelState::default()
                    }
                }
            }
            Err(_) => {
                todo!("LOGGING");
                KernelState::default()
            }
        }
    }
}

impl<A: AudioBackend<ArcChannelSink<f32>>> Drop for Kernel<A> {
    fn drop(&mut self) {
        self.serialize_user_data();
    }
}

// Basic serializeable/deserializable app state.
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

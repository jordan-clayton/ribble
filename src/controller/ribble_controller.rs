use crate::controller::kernel::Kernel;
use crate::controller::visualizer::{AnalysisType, RotationDirection, NUM_VISUALIZER_BUCKETS};
use crate::controller::{AmortizedProgress, CompletedRecordingJobs, ConsoleMessage, OfflineTranscriberFeedback, Progress};
use crate::utils::errors::RibbleError;
use crate::utils::model_bank::RibbleModelBankIter;
use crate::utils::recorder_configs::{RibbleRecordingConfigs, RibbleRecordingExportFormat};
use crate::utils::vad_configs::VadConfigs;
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::transcriber::{TranscriptionSnapshot, WhisperControlPhrase};
use ribble_whisper::whisper::configs::WhisperRealtimeConfigs;
use ribble_whisper::whisper::model::{Model, ModelId};
use std::path::{Path, PathBuf};
use std::sync::Arc;

// NOTE: if deciding to swap the backend, make the sink generic, S: Sink<f32>
// NOTE TWICE: Possibly look at making the Kernel generic and implement methods on it.
// TODO: Heavily consider removing the generics here until it's absolutely necessary.
// It's a bit of a pain and the app is currently non-generic--it's much easier to deal with concrete types.
#[derive(Clone)]
pub(crate) struct RibbleController<A: AudioBackend<ArcChannelSink<f32>>> {
    kernel: Arc<Kernel<A>>,
    max_whisper_threads: usize,
}

impl<A: AudioBackend<ArcChannelSink<f32>>> RibbleController<A> {
    const RECOMMENDED_MAX_WHISPER_THREADS: usize = 8;
    // NOTE: The AudioBackendProxy will need to be constructed higher up in the app and passed in.
    pub(crate) fn new(data_directory: &Path, audio_backend: A) -> Result<Self, RibbleError> {
        let kernel = Arc::new(Kernel::new(data_directory, audio_backend)?);
        let available_threads = std::thread::available_parallelism()?.get();
        let max_whisper_threads = available_threads.min(Self::RECOMMENDED_MAX_WHISPER_THREADS);
        Ok(Self { kernel, max_whisper_threads })
    }

    pub(crate) fn serialize_user_data(&self) {
        self.kernel.serialize_user_data();
    }

    // TODO: either add to the kernel or create a second state struct for hardware configurations.
    // As of right now, the number of available threads
    pub(crate) fn max_whisper_threads(&self) -> usize {
        self.max_whisper_threads
    }

    // MODEL MANAGEMENT
    pub(crate) fn download_model(&self, url: String) {
        todo!("Finish Kernel::download_model")
    }
    pub(crate) fn copy_new_model(&self, file_path: &Path) {
        todo!("Finish Kernel::copy_new_model")
    }

    pub(crate) fn rename_model(&self, model_id: ModelId, new_name: String) -> Result<Option<ModelId>, RibbleError> {
        Ok(self.kernel.rename_model(model_id, new_name)?)
    }

    pub(crate) fn model_exists_in_storage(&self, model_id: ModelId) -> Result<bool, RibbleError> {
        Ok(self.kernel.model_exists_in_storage(model_id)?)
    }

    pub(crate) fn delete_model(&self, model_id: ModelId) -> Result<Option<Model>, RibbleError> {
        Ok(self.kernel.remove_model(model_id)?)
    }

    // TODO: expect this to be a void method after refactoring ->
    // The ModelBank needs handles for downloading/work threads.
    pub(crate) fn refresh_model_bank(&self) -> Result<(), RibbleError> {
        Ok(self.kernel.refresh_model_bank()?)
    }

    pub(crate) fn get_model_list(&self) -> RibbleModelBankIter {
        self.kernel.iter()
    }
    pub(crate) fn get_model_directory(&self) -> &Path {
        self.kernel.get_model_directory()
    }

    // TRANSCRIBER
    pub(crate) fn read_transcription_configs(&self) -> Arc<WhisperRealtimeConfigs> {
        self.kernel.read_transcription_configs()
    }
    pub(crate) fn write_transcription_configs(&self, new_configs: WhisperRealtimeConfigs) {
        self.kernel.write_transcription_configs(new_configs);
    }
    pub(crate) fn read_vad_configs(&self) -> Arc<VadConfigs> {
        self.kernel.read_vad_configs()
    }
    pub(crate) fn write_vad_configs(&self, new_configs: VadConfigs) {
        self.kernel.write_vad_configs(new_configs);
    }
    pub(crate) fn read_offline_transcriber_feedback(&self) -> OfflineTranscriberFeedback {
        self.kernel.read_offline_transcriber_feedback()
    }
    pub(crate) fn write_offline_transcriber_feedback(&self, new_feedback: OfflineTranscriberFeedback) {
        self.kernel.write_offline_transcriber_feedback(new_feedback);
    }
    pub(crate) fn realtime_running(&self) -> bool {
        self.kernel.realtime_running()
    }
    pub(crate) fn offline_running(&self) -> bool {
        self.kernel.offline_running()
    }
    pub(crate) fn transcriber_running(&self) -> bool {
        self.transcriber_running()
    }

    pub(crate) fn stop_realtime(&self) {
        self.kernel.stop_realtime()
    }
    pub(crate) fn stop_offline(&self) {
        self.kernel.stop_realtime()
    }

    // It's easiest from the transcription windows to just kill both.
    pub(crate) fn stop_transcription(&self) {
        self.kernel.stop_realtime();
        self.kernel.stop_offline();
    }

    pub(crate) fn read_transcription_snapshot(&self) -> Arc<TranscriptionSnapshot> {
        self.kernel.read_transcription_snapshot()
    }
    pub(crate) fn read_latest_control_phrase(&self) -> Arc<WhisperControlPhrase> {
        self.kernel.read_latest_control_phrase()
    }

    pub(crate) fn read_current_audio_file_path(&self) -> Arc<Option<PathBuf>> {
        self.kernel.read_current_audio_file_path()
    }

    pub(crate) fn start_realtime_transcription(&self) {
        self.kernel
            .start_realtime_transcription();
    }

    pub(crate) fn set_audio_file_path(&self, path: PathBuf) {
        self.kernel.set_audio_file_path(path);
    }
    pub(crate) fn clear_audio_file_path(&self) {
        self.kernel.clear_audio_file_path();
    }

    pub(crate) fn start_offline_transcription(&self) {
        self.kernel
            .start_offline_transcription();
    }

    pub(crate) fn try_retranscribe_latest(&self) {
        self.kernel.try_retranscribe_latest();
    }

    // RECORDER
    pub(crate) fn recorder_running(&self) -> bool {
        self.kernel.recorder_running()
    }

    pub(crate) fn read_recorder_configs(&self) -> Arc<RibbleRecordingConfigs> {
        self.kernel.read_recorder_configs()
    }
    pub(crate) fn write_recorder_configs(&self, new_configs: RibbleRecordingConfigs) {
        self.kernel.write_recorder_configs(new_configs);
    }

    pub(crate) fn start_recording(&self) {
        self.kernel.start_recording();
    }

    // WRITER (RECORDINGS + Export)
    pub(crate) fn is_clearing_recordings(&self) -> bool {
        self.kernel.is_clearing()
    }
    pub(crate) fn clear_recording_cache(&self) {
        self.kernel.clear_cache()
    }
    pub(crate) fn try_get_latest_recording(&self) -> Option<PathBuf> {
        self.kernel.try_get_latest()
    }

    // NOTE: if lock-contention is ever an issue (if this method even gets used),
    // swap to a try_get and respond accordingly in the UI.
    pub(crate) fn get_num_recordings(&self) -> usize {
        self.kernel.get_num_recordings()
    }

    pub(crate) fn latest_recording_exists(&self) -> bool {
        self.kernel.latest_recording_exists()
    }
    pub(crate) fn try_get_completed_recordings(&self, copy_buffer: &mut Vec<(String, CompletedRecordingJobs)>) {
        self.kernel.try_get_completed_jobs(copy_buffer)
    }

    pub(crate) fn try_get_recording_path(&self, file_name: &str) -> Option<PathBuf> {
        self.kernel.get_recording_path(file_name)
    }

    // NOTE: recording_file_name is internal -- It's the left-half of the (String, CompletedRecordingJobs) tuple.
    pub(crate) fn export_recording(&self, out_path: &Path, recording_file_name: &str, output_format: RibbleRecordingExportFormat) {
        self.kernel.export(out_path, recording_file_name, output_format);
    }

    // CONSOLE
    pub(crate) fn try_get_current_message(&self, copy_buffer: &mut Vec<Arc<ConsoleMessage>>) {
        self.kernel.try_get_current_message(copy_buffer);
    }
    pub(crate) fn resize_console_message_buffer(&self, new_size: usize) {
        self.kernel.resize(new_size)
    }

    // PROGRESS
    pub(crate) fn try_get_current_jobs(&self, copy_buffer: &mut Vec<Progress>) {
        self.kernel.try_get_current_jobs(copy_buffer);
    }

    pub(crate) fn try_get_amortized_progress(&self) -> Option<AmortizedProgress> {
        self.kernel.try_get_amortized_jobs()
    }

    // VISUALIZER
    pub(crate) fn set_visualizer_visibility(&self, is_visible: bool) {
        self.set_visualizer_visibility(is_visible);
    }
    pub(crate) fn try_read_visualization_buffer(&self, copy_buffer: &mut [f32; NUM_VISUALIZER_BUCKETS]) {
        self.kernel.try_read_visualization_buffer(copy_buffer);
    }

    pub(crate) fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.kernel.get_visualizer_analysis_type()
    }
    pub(crate) fn rotate_visualizer_type(&self, direction: RotationDirection) {
        self.kernel.rotate_visualizer_type(direction);
    }
}
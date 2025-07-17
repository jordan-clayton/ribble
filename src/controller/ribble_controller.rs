use crate::controller::audio_backend_proxy::AudioBackendProxy;
use crate::controller::kernel::Kernel;
use crate::controller::{
    AmortizedDownloadProgress, AmortizedProgress, AnalysisType, CompletedRecordingJobs,
    ConsoleMessage, FileDownload, NUM_VISUALIZER_BUCKETS, OfflineTranscriberFeedback, Progress,
    RotationDirection,
};
use crate::utils::errors::RibbleError;
use crate::utils::preferences::UserPreferences;
use crate::utils::recorder_configs::{RibbleRecordingConfigs, RibbleRecordingExportFormat};
use crate::utils::vad_configs::VadConfigs;
use ribble_whisper::transcriber::{TranscriptionSnapshot, WhisperControlPhrase};
use ribble_whisper::utils::Sender;
use ribble_whisper::whisper::configs::WhisperRealtimeConfigs;
use ribble_whisper::whisper::model::ModelId;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn set_base_directory() -> PathBuf {
    use directories::BaseDirs;
    match BaseDirs::new() {
        Some(base_dirs) => base_dirs.home_dir().to_path_buf(),
        None => PathBuf::from("/"),
    }
}

#[derive(Clone)]
pub(crate) struct RibbleController {
    kernel: Arc<Kernel>,
    base_dir: Arc<PathBuf>,
    // NOTE: Since the controller has to be used and operated by views in the gui,
    // This needs to include some GUI-ish code.
    toasts_sender: Sender<egui_notify::Toast>,
    max_whisper_threads: usize,
}

impl RibbleController {
    const RECOMMENDED_MAX_WHISPER_THREADS: usize = 8;
    // NOTE: The data_directory needs to be absolute -> there's a guard for this in the
    // kernel and it will return Err() if the path is relative.
    pub(crate) fn new(
        data_directory: &Path,
        audio_backend: AudioBackendProxy,
        toasts_sender: Sender<egui_notify::Toast>,
    ) -> Result<Self, RibbleError> {
        let kernel = Arc::new(Kernel::new(data_directory, audio_backend)?);
        let available_threads = std::thread::available_parallelism()?.get();
        let max_whisper_threads = available_threads.min(Self::RECOMMENDED_MAX_WHISPER_THREADS);

        let base_dir = Arc::new(set_base_directory());

        Ok(Self {
            kernel,
            base_dir,
            toasts_sender,
            max_whisper_threads,
        })
    }

    pub(crate) fn base_dir(&self) -> &Path {
        self.base_dir.as_path()
    }

    // Only try_send the toast -> this shouldn't ever really block
    pub(crate) fn send_toast(&self, toast: egui_notify::Toast) {
        if self.toasts_sender.try_send(toast).is_err() {
            todo!(
                "LOGGING -> this should only error out when the queue is full (wouldblock) or the app has closed."
            );
        }
    }

    pub(crate) fn serialize_user_data(&self) {
        self.kernel.serialize_user_data();
    }

    pub(crate) fn max_whisper_threads(&self) -> usize {
        self.max_whisper_threads
    }

    pub(crate) fn read_user_preferences(&self) -> Arc<UserPreferences> {
        self.kernel.read_user_preferences()
    }

    pub(crate) fn write_user_preferences(&self, new_prefs: UserPreferences) {
        self.kernel.write_user_preferences(new_prefs);
    }

    pub(crate) fn get_system_visuals(&self) -> Option<egui::Visuals> {
        self.kernel.get_system_visuals()
    }

    // NOTE: this will allocate and should not be called every frame,
    // Either use an internal setter, or swap on transition.
    pub(crate) fn get_system_gradient(&self) -> Option<egui_colorgradient::Gradient> {
        self.kernel.get_system_gradient()
    }

    // MODEL MANAGEMENT
    pub(crate) fn download_model(&self, url: &str) {
        self.kernel.download_model(url);
    }
    // Consume the PathBuf instead of taking a path by reference
    // Since the copy-operation has to happen on a thread, there
    // needs to be at least one allocation.
    // Since obtaining a dynamic path involves an allocation, might as well just take it instead of
    // re-allocating.
    pub(crate) fn copy_new_model(&self, file_path: PathBuf) {
        self.kernel.copy_new_model_to_bank(file_path);
    }

    // (Id, File name)
    pub(crate) fn try_read_model_list(&self, copy_buffer: &mut Vec<(ModelId, Arc<str>)>) {
        self.kernel.try_read_model_list(copy_buffer);
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
    pub(crate) fn write_offline_transcriber_feedback(
        &self,
        new_feedback: OfflineTranscriberFeedback,
    ) {
        self.kernel.write_offline_transcriber_feedback(new_feedback);
    }
    pub(crate) fn realtime_running(&self) -> bool {
        self.kernel.realtime_running()
    }
    pub(crate) fn offline_running(&self) -> bool {
        self.kernel.offline_running()
    }
    pub(crate) fn transcriber_running(&self) -> bool {
        self.kernel.transcriber_running()
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
        self.kernel.start_realtime_transcription();
    }

    pub(crate) fn set_audio_file_path(&self, path: PathBuf) {
        self.kernel.set_audio_file_path(path);
    }
    pub(crate) fn clear_audio_file_path(&self) {
        self.kernel.clear_audio_file_path();
    }

    pub(crate) fn start_offline_transcription(&self) {
        self.kernel.start_offline_transcription();
    }

    pub(crate) fn try_retranscribe_latest(&self) {
        self.kernel.try_retranscribe_latest();
    }

    pub(crate) fn save_transcription(&self, out_path: PathBuf) {
        self.kernel.save_transcription(out_path);
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

    pub(crate) fn stop_recording(&self) {
        self.kernel.stop_recording();
    }

    // WRITER (RECORDINGS + Export)
    pub(crate) fn is_clearing_recordings(&self) -> bool {
        self.kernel.is_clearing_recordings()
    }
    pub(crate) fn clear_recording_cache(&self) {
        self.kernel.clear_recording_cache()
    }
    pub(crate) fn try_get_latest_recording(&self) -> Option<PathBuf> {
        self.kernel.try_get_latest_recording()
    }

    // NOTE: if lock-contention is ever an issue (if this method even gets used),
    // swap to a try_get and respond accordingly in the UI.
    pub(crate) fn get_num_recordings(&self) -> usize {
        self.kernel.get_num_recordings()
    }

    pub(crate) fn latest_recording_exists(&self) -> bool {
        self.kernel.latest_recording_exists()
    }
    pub(crate) fn try_get_completed_recordings(
        &self,
        copy_buffer: &mut Vec<(Arc<str>, CompletedRecordingJobs)>,
    ) {
        self.kernel.try_get_completed_recordings(copy_buffer)
    }

    pub(crate) fn try_get_recording_path(&self, file_name: Arc<str>) -> Option<PathBuf> {
        self.kernel.try_get_recording_path(file_name)
    }

    // NOTE: recording_file_name is internal -- It's the left-half of the (Arc<str>, CompletedRecordingJobs) tuple.
    pub(crate) fn export_recording(
        &self,
        out_path: PathBuf,
        recording_file_name: Arc<str>,
        output_format: RibbleRecordingExportFormat,
    ) {
        self.kernel
            .export_recording(out_path, recording_file_name, output_format);
    }

    // CONSOLE
    pub(crate) fn try_get_current_messages(&self, copy_buffer: &mut Vec<Arc<ConsoleMessage>>) {
        self.kernel.try_get_current_messages(copy_buffer);
    }

    // Resizing happens on a background thread, so it's okay to call this with some level of
    // frequency. -> If using a slider in UI, consider caching the value, mutating that, and then
    // writing on a drag-finished event.
    // There is a tiny, tiny chance that the short-queue gets slammed -> if so, increase the size,
    // or handle priority better/classify jobs better.
    pub(crate) fn resize_console_message_buffer(&self, new_size: usize) {
        self.kernel.resize_console_message_buffer(new_size);
    }

    // PROGRESS
    pub(crate) fn try_get_current_jobs(&self, copy_buffer: &mut Vec<Progress>) {
        self.kernel.try_get_current_jobs(copy_buffer);
    }

    pub(crate) fn try_get_amortized_progress(&self) -> Option<AmortizedProgress> {
        self.kernel.try_get_amortized_jobs()
    }

    // DOWNLOADER

    pub(crate) fn try_get_current_downloads(&self, copy_buffer: &mut Vec<(usize, FileDownload)>) {
        self.kernel.try_get_current_downloads(copy_buffer);
    }

    pub(crate) fn try_get_amortized_download_progress(&self) -> Option<AmortizedDownloadProgress> {
        self.kernel.try_get_amortized_download_progress()
    }

    pub(crate) fn abort_download(&self, download_id: usize) {
        self.kernel.abort_download(download_id);
    }

    // VISUALIZER
    pub(crate) fn set_visualizer_visibility(&self, is_visible: bool) {
        self.kernel.set_visualizer_visibility(is_visible);
    }
    pub(crate) fn try_read_visualization_buffer(
        &self,
        copy_buffer: &mut [f32; NUM_VISUALIZER_BUCKETS],
    ) {
        self.kernel.try_read_visualization_buffer(copy_buffer);
    }

    pub(crate) fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.kernel.get_visualizer_analysis_type()
    }

    pub(crate) fn set_visualizer_analysis_type(&self, new_type: AnalysisType) {
        self.kernel.set_visualizer_analysis_type(new_type);
    }

    pub(crate) fn rotate_visualizer_type(&self, direction: RotationDirection) {
        self.kernel.rotate_visualizer_type(direction);
    }
}

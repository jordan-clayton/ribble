use crate::controller::console::ConsoleEngine;
use crate::controller::progress::ProgressEngine;
use crate::controller::recorder::RecorderEngine;
use crate::controller::transcriber::TranscriberEngine;
use crate::controller::visualizer::{AnalysisType, VisualizerEngine};
use crate::controller::worker::RibbleWorkerEngine;
use crate::utils::console_message::NewConsoleMessage;
use crate::utils::errors::WhisperAppError;
use crate::utils::progress::Progress;
use crossbeam::channel::Receiver;
use ribble_whisper::audio::microphone::AudioBackend;
use std::path::Path;
use std::sync::Arc;

// TODO: add to this as necessary; these are "resource requests" and "reporting" for engine components to communicate via the Controller kernel.
// e.g. AudioWorkerRunningState (not sold on that, but it's prooobably a good idea for the recorder icon).
pub trait EngineKernel: Send + Sync {
    // TODO: change return type once VAD configs is designed
    fn get_vad_config(&self);
    // TODO: change return type once BandpassConfigs is designed
    fn get_bandpass_config(&self);

    // TODO: not 100% sure about this -just yet-.
    // Also not 100% sure about how to handle the temporary audio file.
    fn get_audio_file_path(&self) -> Option<&Path>;
    fn get_audio_backend(&self) -> &AudioBackend;

    // TODO: this might need a trait-bound--revisit once the RecordingEngine is finished
    // TODO: The error type needs to change once errors are refactored
    fn request_writer<T>(&self, audio_stream: Receiver<Arc<[T]>>, path: &Path) -> Result<(), WhisperAppError>;
    fn add_progress_job(&self, job: Progress) -> usize;
    fn update_progress_job(&self, id: usize, delta: u64);
    fn remove_progress_job(&self, id: usize);
    fn send_console_message(&self, message: NewConsoleMessage);
    fn finalize_transcription(&self, transcription: String);
    // TODO: this likely should not be exposed--> I don't see a use for it that doesn't indicate a coupling problem.
    // For now, leave it.
    fn visualizer_running(&self) -> bool;
    fn update_visualizer_data(&self, buffer: &[f32]);

    // TODO: this likely should not be exposed--> I don't see a use for it that doesn't indicate a coupling problem.
    // For now, leave it.
    fn get_visualizer_analysis_type(&self) -> AnalysisType;
}

// TODO: NOTE TO SELF, store this in the controller instead of the old spaghetti.
// The spaghetti is now portioned onto different plates, so to speak, so that the complexity
// becomes a little easier to reason about.
// TODO: Consider implementing a DownloadEngine to bury the implementation.
// TODO: Ibid if bringing in integrity-checking.
// NOTE: if it becomes absolutely necessary (e.g. testing), factor the engine components out into traits.
// The EngineKernel is mockable, but the Engine components are not (yet).
pub struct Kernel {
    // TODO: additional state (non-engines), VadConfigs, BandpassConfigs, file paths
    // Also, the audio backend.
    audio_backend: AudioBackend,
    transcriber_engine: TranscriberEngine,
    recorder_engine: RecorderEngine,
    console_engine: ConsoleEngine,
    progress_engine: ProgressEngine,
    visualizer_engine: VisualizerEngine,
    worker_engine: RibbleWorkerEngine,
}

// TODO: implement trait
// NOTE: most of these are blocking calls (as of now with concrete components).
// Anything that involves writing involves trying to grab a write lock.
impl EngineKernel for Kernel {
    fn get_vad_config(&self) {
        todo!()
    }

    fn get_bandpass_config(&self) {
        todo!()
    }

    fn get_audio_file_path(&self) -> Option<&Path> {
        todo!()
    }

    fn get_audio_backend(&self) -> &AudioBackend {
        &self.audio_backend
    }

    fn request_writer<T>(&self, audio_stream: Receiver<Arc<[T]>>, path: &Path) -> Result<(), WhisperAppError> {
        todo!("Implement kernel method that handles this request")
        // NOTE TO SELF: migrate the _write_thread from the TranscriberEngine scoped thread loop
        // to a kernel method that spawns a joinhandle.
    }

    fn add_progress_job(&self, job: Progress) -> usize {
        self.progress_engine.add_progress_job(job)
    }

    fn update_progress_job(&self, id: usize, delta: u64) {
        self.progress_engine.update_progress_job(id, delta);
    }

    fn remove_progress_job(&self, id: usize) {
        self.progress_engine.remove_progress_job(id);
    }

    fn send_console_message(&self, message: NewConsoleMessage) {
        self.console_engine.add_console_message(message);
    }

    fn finalize_transcription(&self, transcription: String) {
        self.transcriber_engine.finalize_transcription(transcription);
    }

    fn visualizer_running(&self) -> bool {
        self.visualizer_engine.visualizer_running()
    }

    fn update_visualizer_data(&self, buffer: &[f32]) {
        self.visualizer_engine.update_visualizer_data(buffer);
    }


    fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.visualizer_engine.get_visualizer_analysis_type()
    }
}

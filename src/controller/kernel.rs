use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use crate::controller::console::ConsoleEngine;
use crate::controller::progress::ProgressEngine;
use crate::controller::transcriber::TranscriberEngine;
use crate::utils::console_message::NewConsoleMessage;
use crate::utils::constants::NUM_BUCKETS;
use crate::utils::progress::Progress;
use ribble_whisper::audio::microphone::AudioBackend;
use crate::controller::recorder::RecorderEngine;
use crate::controller::visualizer::VisualizerEngine;
use crate::controller::worker::RibbleWorkerEngine;
use crate::utils::audio_analysis::AnalysisType;

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
    fn add_progress_job(&self, job: Progress) -> usize;
    fn update_progress_job(&self, id: usize, delta: u64);
    fn remove_progress_job(&self, id: usize);
    fn send_console_message(&self, message: NewConsoleMessage);
    fn finalize_transcription(&self, transcription: String);
    fn visualizer_running_flag(&self) -> Arc<AtomicBool>;
    fn update_visualizer(&self, buffer: &[f32; NUM_BUCKETS]);
    fn get_visualizer_analysis_type(&self) -> AnalysisType;
}

// TODO: NOTE TO SELF, store this in the controller instead of the old spaghetti.
// The spaghetti is now portioned onto different plates, so to speak, so that the complexity
// becomes a little easier to reason about.
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
// NOTE: most of these are blocking calls.
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
   
    fn add_progress_job(&self, job: Progress) -> usize {
        self.progress_engine.add_progress_job(job)
    }

    fn update_progress_job(&self, id: usize, delta: u64) {
        self.progress_engine.update_progress_job(id, delta)
    }

    fn remove_progress_job(&self, id: usize) {
        self.progress_engine.remove_progress_job(id)
    }

    fn send_console_message(&self, message: NewConsoleMessage) {
        todo!()
    }

    fn finalize_transcription(&self, transcription: String) {
        todo!()
    }

    fn visualizer_running_flag(&self) -> Arc<AtomicBool> {
        todo!()
    }

    fn update_visualizer(&self, buffer: &[f32; NUM_BUCKETS]) {
        todo!()
    }

    fn get_visualizer_analysis_type(&self) -> AnalysisType {
        todo!()
    }
}

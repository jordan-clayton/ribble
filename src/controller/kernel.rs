use crate::controller::console::{ConsoleEngine, ConsoleMessage};
use crate::controller::progress::{Progress, ProgressEngine};
use crate::controller::recorder::RecorderEngine;
use crate::controller::transcriber::TranscriberEngine;
use crate::controller::visualizer::{AnalysisType, VisualizerEngine};
use crate::controller::worker::WorkerEngine;
use crate::utils::audio_backend_proxy::AudioBackendProxy;
use crate::utils::errors::{RibbleAppError, RibbleError};
use crate::utils::model_bank::RibbleModelBank;
use crate::utils::pcm_f32::IntoPcmF32;
use crate::utils::vad_configs::VadConfigs;
use arc_swap::ArcSwap;
use ribble_whisper::audio::audio_backend::{AudioBackend, CaptureSpec};
use ribble_whisper::audio::microphone::Sdl2Capture;
use ribble_whisper::audio::recorder::SampleSink;
use ribble_whisper::utils::Receiver;
use ribble_whisper::whisper::model::ModelRetriever;
use std::path::Path;
use std::sync::Arc;

pub(crate) enum TranscriberMethod {
    Realtime,
    Offline,
}

// TODO: add to this as necessary; these are "resource requests" and "reporting" for engine components to communicate via the Controller kernel.
// e.g. AudioWorkerRunningState (not sold on that, but it's prooobably a good idea for the recorder icon).
//
// TODO: If/when this trait gets too bloated, split into smaller interfaces so that types (e.g.
// Worker, Recorder) are left with things they don't ever use.
// TODO TWICE: Remove the EngineKernel -> it's not the right way to communicate.

pub(crate) trait EngineKernel: Send + Sync {
    type Retriever: ModelRetriever;

    fn get_vad_configs(&self, transcriber_method: TranscriberMethod) -> VadConfigs;
    // TODO: change return type once BandpassConfigs is designed
    fn get_bandpass_config(&self);

    // TODO: not 100% sure about this -just yet-.
    // Also not 100% sure about how to handle the temporary audio file.
    fn get_audio_file_path(&self) -> Option<&Path>;

    fn request_audio_capture<S: SampleSink>(
        &self,
        spec: CaptureSpec,
        sink: S,
    ) -> Result<Sdl2Capture<S>, RibbleError>;

    // TODO: this might need a trait-bound--revisit once the RecordingEngine is finished
    // TODO: The error type needs to change once errors are refactored
    // TODO: Integer/Floating point -> not sure how best to handle -> possibly use the trait bound,
    // Or: Make VisualizerSample into just a Sample -> not entirely sure
    // Might be easiest to just send a RecorderConfigs or similar.
    fn request_writer<T>(
        &self,
        audio_stream: Receiver<Arc<[T]>>,
        path: &Path,
    ) -> Result<(), RibbleAppError>;
    fn add_progress_job(&self, job: Progress) -> usize;
    fn update_progress_job(&self, id: usize, delta: u64);
    fn remove_progress_job(&self, id: usize);
    fn send_console_message(&self, message: ConsoleMessage);
    // TODO: this likely should not be exposed--> I don't see a use for it that doesn't indicate a coupling problem.
    // For now, leave it.
    fn visualizer_running(&self) -> bool;
    fn update_visualizer_data<T: IntoPcmF32>(&self, buffer: Arc<[T]>, sample_rate: f64);

    // TODO: this likely should not be exposed--> I don't see a use for it that doesn't indicate a coupling problem.
    // For now, leave it.
    fn get_visualizer_analysis_type(&self) -> AnalysisType;
    fn get_model_retriever(&self) -> Arc<Self::Retriever>;

    // TODO: possibly add a "fatal"/abort app mechanism for "unrecoverable" or "should be unrecoverable" errors.
    // If any background threads are panicking, there's an implementation error.
    // In this instance, the app should probably crash because important work can no longer be done.
    fn cleanup_progress_jobs(&self, ids: &[usize]);
}

// TODO: NOTE TO SELF, store this in the controller instead of the old spaghetti.
// The spaghetti is now portioned onto different plates, to make things easier to manage.
// TODO: Consider implementing a DownloadEngine to bury the implementation. -> It might be sufficient to just keep that in the controller & send to the WorkerEngine.
// TODO: Ibid if bringing in integrity-checking
// NOTE: if it becomes absolutely necessary (e.g. testing), factor the engine components out into traits.
// The EngineKernel is mockable, but the Engine components are not (yet)--but likely don't need to
// be mocked and can be used as is..

// TODO: remove the monomorphization -> do it at the function level.
//
// Instead, write delegate methods here that the controller will call
pub struct Kernel {
    // The configs are Serializable -> derive serialize on the struct and write a routine for
    // save/load -> possibly pass the configs in via some large structure.

    realtime_vad_configs: ArcSwap<VadConfigs>,
    offline_vad_configs: ArcSwap<VadConfigs>,

    audio_backend: AudioBackendProxy,

    // The Whisper configs are also serializeable -> derive serialize on the struct and write
    // a routine in Transcriber engine or pass the configs in.
    transcriber_engine: TranscriberEngine<RibbleModelBank, Kernel>,
    recorder_engine: RecorderEngine<RibbleModelBank, Kernel>,
    console_engine: ConsoleEngine,
    progress_engine: ProgressEngine,
    visualizer_engine: VisualizerEngine,
    worker_engine: WorkerEngine<RibbleModelBank, Kernel>,
    // Since model bank needs to be accessed elsewhere (e.g. TranscriberEngine), this needs to be
    // in a shared pointer.
    model_bank: Arc<RibbleModelBank>,
}

// TODO: impl Kernel -> the main guts, controller delegates to these methods
// NOTE: the constructor's going to get a liiiiittle gnarly.
// Use the Bus abstraction to try and keep things sane-ish.

// TODO: implement trait
// NOTE: most of these are blocking calls (as of now with concrete components).
// Anything that involves writing is almost guaranteed to involve trying to grab a write lock.
impl EngineKernel for Kernel {
    type Retriever = RibbleModelBank;
    fn get_vad_configs(&self, transcriber_method: TranscriberMethod) -> VadConfigs {
        match transcriber_method {
            TranscriberMethod::Realtime => *self.realtime_vad_configs.load_full(),
            TranscriberMethod::Offline => *self.offline_vad_configs.load_full(),
        }
    }

    fn get_bandpass_config(&self) {
        todo!()
    }

    fn get_audio_file_path(&self) -> Option<&Path> {
        todo!()
    }

    fn request_audio_capture<S: SampleSink>(
        &self,
        spec: CaptureSpec,
        sink: S,
    ) -> Result<Sdl2Capture<S>, RibbleError> {
        self.audio_backend.open_capture(spec, sink).into()
    }

    fn request_writer<T>(
        &self,
        audio_stream: Receiver<Arc<[T]>>,
        path: &Path,
    ) -> Result<(), RibbleAppError> {
        todo!("Implement kernel method that handles this request")
        // NOTE TO SELF: migrate the _write_thread from the TranscriberEngine scoped thread loop
        // to a kernel method that spawns a joinhandle to send to the WorkerEngine.
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

    fn send_console_message(&self, message: ConsoleMessage) {
        self.console_engine.add_console_message(message);
    }

    fn visualizer_running(&self) -> bool {
        self.visualizer_engine.visualizer_running()
    }

    // TODO: this should just be a single message queue.
    fn update_visualizer_data<T: IntoPcmF32>(&self, buffer: Arc<[T]>, sample_rate: f64) {
        self.visualizer_engine
            .update_visualizer_data(buffer, sample_rate);
    }

    fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.visualizer_engine.get_visualizer_analysis_type()
    }

    fn get_model_retriever(&self) -> Arc<Self::Retriever> {
        Arc::clone(&self.model_bank)
    }

    fn cleanup_progress_jobs(&self, ids: &[usize]) {
        for id in ids {
            self.progress_engine.remove_progress_job(*id);
        }
    }
}

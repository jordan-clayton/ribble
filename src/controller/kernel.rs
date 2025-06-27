use crate::controller::console::ConsoleEngine;
use crate::controller::console::ConsoleMessage;
use crate::controller::progress::Progress;
use crate::controller::progress::ProgressEngine;
use crate::controller::recorder::RecorderEngine;
use crate::controller::transcriber::TranscriberEngine;
use crate::controller::visualizer::{AnalysisType, VisualizerEngine};
use crate::controller::worker::WorkerEngine;
use crate::utils::errors::RibbleAppError;
use crate::utils::model_bank::RibbleModelBank;
use crate::utils::pcm_f32::IntoPcmF32;
use crossbeam::channel::Receiver;
use ribble_whisper::audio::microphone::AudioBackend;
use ribble_whisper::whisper::model::{ModelId, ModelRetriever};
use std::path::{Path, PathBuf};
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
    fn finalize_transcription(&self, transcription: String);
    // TODO: this likely should not be exposed--> I don't see a use for it that doesn't indicate a coupling problem.
    // For now, leave it.
    fn visualizer_running(&self) -> bool;
    fn update_visualizer_data<T: IntoPcmF32>(&self, buffer: Arc<[T]>, sample_rate: f64);

    // TODO: this likely should not be exposed--> I don't see a use for it that doesn't indicate a coupling problem.
    // For now, leave it.
    fn get_visualizer_analysis_type(&self) -> AnalysisType;
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
// The EngineKernel is mockable, but the Engine components are not (yet).

// NOTE: With the following implementation, the Kernel and the EngineKernel need to be of the same
// ModelRetriever type so that the TranscriberEngine<M: ModelRetriever> can monomorphize
// (because RealtimeTranscriber does static dispatch)

// RibbleModelBank can also be generic, but that introduces a complexity problem I'm not keen to solve right now
// Instead, when mocking, mock the Kernel itself and any necessary swappable traits ->
// ConcurrentModelBank is mainly just a trait to keep the code structure.

// TODO: All engines that take a WeakRef to the kernel, must also have the same trait bounds
// as the kernel itself.
// The trait bounds must be known at compile time for RealtimeTranscriber to work.
// Since all engines with a Weak<dyn EngineKernel> have to know what type ModelRetriever is
//

// NOTE: This might be a compilation pain point.
// All engines that have a Weak<dyn Kernel<M>> need to monomorphize.
// EngineKernel must have a trait bound for TranscriberEngine to compile, so this means
// that all engines must have either: Weak<dyn EngineKernel<M>>, or the engine must be *Engine<M>.
// TranscriberEngine must statically dispatch, other engines could be dynamic, but
// runtime dispatch is not a major architectural requirement for this project.

// Ideas thus far:
// - Delegate object:
// Pros: Transcribers get a concrete type and compile
// Cons: mild pointer overhead, lose freedom of generics

// - Monomorphize in the Kernel implementation structs + Engine Kernel
// Pros: Transcribers get a concrete type and compile
// Cons: RibbleModelBankState needs to be exposed, or a delegate object -> crusty + extra overhead
// More cons: It's really crusty to try and Monomorphize EngineKernel

// - Implement ModelRetriever for Kernel, monomorphize TranscriberEngine with type Kernel

// Pros: Reasonable indirection (delegate to RibbleModelBank -> Move inner state to RibbleModelbank)
// Cons: A little couple-y and lose the freedom of generics (not that it really matters)
// More cons: lots of shared references to the kernel.

// - Implement ModelRetriever for Kernel, make a concrete struct that takes a:
// Closure / Arc<dyn EngineKernel> to pass-through

// (HOPEFULLY) WORKING SOLUTION THUS FAR.
// Pros: Full separation of concerns, keep freedom of dynamic dispatch ->
// dynamic hot-swappable kernels are possibly good for future features.

// Cons: Slightly more indirection:
// e.g. Arc deref -> Struct::method() -> *Arc dyn dereference -> function_call() -> RibbleModelBank
// The last function_call() -> RibbleModelBank might be inlined, so this will still be okay.


pub struct Kernel {
    // TODO: additional state (non-engines), VadConfigs, BandpassConfigs, file paths,
    // Also, the audio backend.
    // Also twice: the ModelRetriever.
    audio_backend: AudioBackend,
    transcriber_engine: TranscriberEngine,
    recorder_engine: RecorderEngine,
    console_engine: ConsoleEngine,
    progress_engine: ProgressEngine,
    visualizer_engine: VisualizerEngine,
    worker_engine: WorkerEngine,
    model_bank: RibbleModelBank,
}

// TODO: implement trait
// NOTE: most of these are blocking calls (as of now with concrete components).
// Anything that involves writing is almost guaranteed to involve trying to grab a write lock.
impl EngineKernel for Kernel {
    // I'm not sure that it's entirely possible to get this to monomorphize into a single VAD type
    // It is more likely that enum-based dispatch or dynamic dispatch will be required.
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

    fn finalize_transcription(&self, transcription: String) {
        self.transcriber_engine
            .finalize_transcription(transcription);
    }

    fn visualizer_running(&self) -> bool {
        self.visualizer_engine.visualizer_running()
    }

    fn update_visualizer_data<T: IntoPcmF32>(&self, buffer: Arc<[T]>, sample_rate: f64) {
        self.visualizer_engine
            .update_visualizer_data(buffer, sample_rate);
    }

    fn get_visualizer_analysis_type(&self) -> AnalysisType {
        self.visualizer_engine.get_visualizer_analysis_type()
    }

    fn cleanup_progress_jobs(&self, ids: &[usize]) {
        for id in ids {
            self.progress_engine.remove_progress_job(*id);
        }
    }
}

impl ModelRetriever for Kernel {
    fn retrieve_model_path(&self, model_id: ModelId) -> Option<PathBuf> {
        self.model_bank.retrieve_model_path(model_id)
    }
}

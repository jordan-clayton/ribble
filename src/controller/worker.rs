use crate::controller::kernel::EngineKernel;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::console_message::NewConsoleMessage;
use crate::utils::errors::{WhisperAppError, WhisperAppErrorType};
use crossbeam::channel::{self, Receiver, SendError, Sender};
use ribble_whisper::utils::errors::RibbleWhisperError;
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;

struct RibbleWorkerInner {
    engine_kernel: Weak<dyn EngineKernel>,
    incoming: Receiver<RibbleWorkerHandle>,
}
// TODO: this will need refactoring to correct the error type mismatch
// TODO: twice, this needs a priority system or similar, rather than just joining the first job it comes upon.
// Since a lot of jobs are infinite loops, this information needs to be exposed to the WorkerEngine--
// And there needs to be a mechanism to address that.
// E.g. TranscriberEngine spawns a transcription loop, which itself spawns a write loop 
// -> the write loop should be terminated around the same time the transcription loop is, but 
// they're independent.

// It's possibly okay to handle those linearly, but this also invites the possibility that the 
// queue might get slammed (esp with infinite loops) -> think about how best to solve this (might be good to use 2 threads).
impl RibbleWorkerInner {
    fn handle_result(
        &self,
        message: Result<RibbleMessage, WhisperAppError>,
    ) -> Result<(), RibbleWhisperError> {
        match message {
            Ok(message) => self.handle_message(message),
            Err(err) => self.handle_error(err),
        }
    }
    fn handle_message(&self, message: RibbleMessage) -> Result<(), RibbleWhisperError> {
        let kernel = self
            .engine_kernel
            .upgrade()
            .ok_or(RibbleWhisperError::ParameterError(
                "Kernel not yet attached to WorkerEngine".to_string(),
            ))?;
        match message {
            RibbleMessage::Console(msg) => Ok(kernel.send_console_message(msg)),
            // NOTE: if for some reason a Progress message needs to be returned via thread,
            // this will panic and need refactoring.
            RibbleMessage::Progress(_) => unreachable!(),
            RibbleMessage::TranscriptionOutput(msg) => Ok(kernel.finalize_transcription(msg)),
        }
    }
    fn handle_error(&self, error: WhisperAppError) -> Result<(), RibbleWhisperError> {
        let kernel = self
            .engine_kernel
            .upgrade()
            .ok_or(RibbleWhisperError::ParameterError(
                "Kernel not yet attached to WorkerEngine".to_string(),
            ))?;
        let error_msg = NewConsoleMessage::Error(error.to_string());
        Ok(kernel.send_console_message(error_msg))
    }
}

pub struct RibbleWorkerEngine {
    outgoing: Sender<RibbleWorkerHandle>,
    inner: Arc<RibbleWorkerInner>,
    // TODO: swap the error type here once errors have been reimplemented.
    work_thread: Option<JoinHandle<Result<(), RibbleWhisperError>>>,
}

impl RibbleWorkerEngine {
    pub(crate) fn new() -> Self {
        // TODO: factor out a constant for the number of workers this can hold
        let (outgoing, incoming) = channel::bounded::<RibbleWorkerHandle>(100);

        let inner = Arc::new(RibbleWorkerInner {
            engine_kernel: Weak::new(),
            incoming,
        });
        let thread_inner = Arc::clone(&inner);

        let work_thread = Some(std::thread::spawn(move || {
            while let Ok(work) = thread_inner.incoming.recv() {
                match work.join() {
                    Ok(res) => thread_inner.handle_result(res)?,
                    Err(err) => {
                        // Map the error as best as possible
                        // TODO: this will need to be reimplemented once errors have been refactored
                        let ribble_error = WhisperAppError::new(
                            WhisperAppErrorType::ThreadError,
                            format!("Thread panicked! Error: {:?}", err),
                            false,
                        );
                        thread_inner.handle_error(ribble_error)?;
                    }
                }
            }
            Ok(())
        }));

        Self {
            outgoing,
            inner,
            work_thread,
        }
    }
    pub(crate) fn set_engine_kernel(&self, kernel: Weak<dyn EngineKernel>) {
        *self.inner.engine_kernel = kernel;
    }

    // TODO: Refactor this once errors have been refactored.
    pub(crate) fn spawn(
        &self,
        task: RibbleWorkerHandle,
    ) -> Result<(), SendError<RibbleWorkerHandle>> {
        self.outgoing.send(task)
    }
}

impl Drop for RibbleWorkerEngine {
    fn drop(&mut self) {
        if let Some(handle) = self.work_thread.take() {
            handle
                .join()
                .expect("The worker thread is not expected to panic and should run without issues.")
                .expect(
                    "The kernel should have been set before attempting to do any background work.",
                );
        }
    }
}

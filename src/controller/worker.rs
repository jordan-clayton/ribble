use crate::controller::console::ConsoleMessage;
use crate::controller::kernel::EngineKernel;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::errors::{RibbleAppError, RibbleError};
use crossbeam::channel::{self, Receiver, Sender, TrySendError};
use crossbeam::scope;
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;

struct WorkerInner {
    engine_kernel: Weak<dyn EngineKernel>,
    incoming: Receiver<RibbleWorkerHandle>,
    // Inner channel to forward incoming jobs to a joiner.
    // TODO: If double-buffering is not sufficient, look at implementing a priority system
    // Possibly look at rayon for work-stealing if thrashing starts to become an issue.
    forward_incoming: Receiver<RibbleWorkerHandle>,
    forward_outgoing: Sender<RibbleWorkerHandle>,
}
impl WorkerInner {
    fn handle_result(
        &self,
        message: Result<RibbleMessage, RibbleAppError>,
    ) -> Result<(), RibbleAppError> {
        match message {
            Ok(message) => self.handle_message(message),
            Err(err) => self.handle_error(err),
        }
    }
    fn handle_message(&self, message: RibbleMessage) -> Result<(), RibbleAppError> {
        let kernel = self
            .engine_kernel
            .upgrade()
            .ok_or(RibbleError::Core(
                "Kernel not yet attached to WorkerEngine".to_string(),
            ).into())?;
        match message {
            RibbleMessage::Console(msg) => Ok(kernel.send_console_message(msg)),
            // NOTE: if for some reason a Progress message needs to be returned via thread,
            // this will panic and need refactoring.
            RibbleMessage::Progress(_) => unreachable!(),
            RibbleMessage::TranscriptionOutput(msg) => Ok(kernel.finalize_transcription(msg)),
        }
    }
    fn handle_error(&self, mut error: RibbleAppError) -> Result<(), RibbleAppError> {
        let kernel = self
            .engine_kernel
            .upgrade()
            .ok_or(RibbleError::Core(
                "Kernel not yet attached to WorkerEngine".to_string(),
            ).into())?;

        let error = if error.needs_cleanup() {
            error.run_cleanup();
            error.into_error()
        } else {
            error.into_error()
        }?;

        let error_msg = ConsoleMessage::Error(error);
        Ok(kernel.send_console_message(error_msg))
    }
}

pub(super) struct WorkerEngine {
    outgoing: Sender<RibbleWorkerHandle>,
    inner: Arc<WorkerInner>,
    // TODO: swap the error type here once errors have been reimplemented.
    work_thread: Option<JoinHandle<Result<(), RibbleAppError>>>,
}

impl WorkerEngine {
    pub(super) fn new() -> Self {
        // TODO: factor out a constant for the number of workers this can hold
        let (outgoing, incoming) = channel::bounded::<RibbleWorkerHandle>(100);
        let (forward_outgoing, forward_incoming) = channel::bounded::<RibbleWorkerHandle>(100);

        let inner = Arc::new(WorkerInner {
            engine_kernel: Weak::new(),
            incoming,
            forward_outgoing,
            forward_incoming,
        });
        let thread_inner = Arc::clone(&inner);

        let work_thread = Some(std::thread::spawn(move || {
            let forwarder_inner = Arc::clone(&thread_inner);
            let worker_inner = Arc::clone(&thread_inner);
            let res = scope(|s| {

                // NOTE: at the moment, it's -probably- okay for this thread to block.
                // If this starts to become an issue once bounds are sorted out, look
                // at implementing a priority system + bounded joining.
                let _forwarder = s.spawn(move || {
                    while let Ok(work) = forwarder_inner.incoming.recv() {
                        // This can only return SendError if the entire struct is deallocated.
                        let _ = forwarder_inner.forward_outgoing.send(work);
                    }
                });
                let worker = s.spawn(move || {
                    while let Ok(work) = worker_inner.forward_incoming.recv() {
                        match work.join() {
                            Ok(res) => thread_inner.handle_result(res),
                            // TODO: it might actually better to just panic the app until the new implementation
                            // is sorted out -> In no places are the work threads expected to actually panic.
                            // All errors from ribble_whisper are handled with results -> so it might be
                            // better to treat as fatal and crash the app.
                            Err(err) => {
                                let ribble_error = RibbleError::ThreadPanic(format!("{:?}", err)).into();
                                thread_inner.handle_error(ribble_error)
                            }
                            // Since handle_result/handle_error only return Err when the kernel's 
                            // not set, unwrapping here will panic the worker thread and information
                            // should bubble up accordingly.
                        }?;
                    }
                    Ok(())
                });
                worker.join().map_err(|e| {
                    RibbleError::ThreadPanic(format!("{}", e)).into()
                })
            }).map_err(|e| {
                RibbleError::ThreadPanic(format!("{:?}", e)).into()
            })??;
            res
        }));

        Self {
            outgoing,
            inner,
            work_thread,
        }
    }
    pub(super) fn set_engine_kernel(&self, kernel: Weak<dyn EngineKernel>) {
        *self.inner.engine_kernel = kernel;
    }

    // TODO: determine whether or not to handle this in WorkerEngine, or the Controller.
    // (Probably best to do in the controller)
    pub(super) fn spawn(
        &self,
        task: RibbleWorkerHandle,
    ) -> Result<(), TrySendError<RibbleWorkerHandle>> {
        self.outgoing.try_send(task)
    }
}

impl Drop for WorkerEngine {
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

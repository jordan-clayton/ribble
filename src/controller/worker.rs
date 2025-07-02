use crate::controller::Bus;
use crate::controller::console::ConsoleMessage;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::errors::RibbleError;
use crossbeam::scope;
use ribble_whisper::utils::{Receiver, Sender, get_channel};
use std::sync::Arc;
use std::thread::JoinHandle;

// There is no functional difference between members of this enum (at the moment).
// Right now, it's just semantic & the long_queue is twice the size.
pub(crate) enum WorkRequest {
    Short(RibbleWorkerHandle),
    Long(RibbleWorkerHandle),
}

struct WorkerInner {
    incoming_requests: Receiver<WorkRequest>,
    console_message_sender: Sender<ConsoleMessage>,
    // Inner channel to forward incoming jobs to a joiner.
    // TODO: If double-buffering is not sufficient, look at implementing a priority system
    // Possibly look at rayon for work-stealing if thrashing starts to become an issue.
    short_incoming: Receiver<RibbleWorkerHandle>,
    short_outgoing: Sender<RibbleWorkerHandle>,
    long_incoming: Receiver<RibbleWorkerHandle>,
    long_outgoing: Sender<RibbleWorkerHandle>,
}
impl WorkerInner {
    const MAX_SHORT_JOBS: usize = 10;
    const MAX_LONG_JOBS: usize = 2 * Self::MAX_SHORT_JOBS;
    fn new(incoming_requests: Receiver<WorkRequest>, bus: Bus) -> Self {
        let (short_outgoing, short_incoming) = get_channel(Self::MAX_SHORT_JOBS);
        let (long_outgoing, long_incoming) = get_channel(Self::MAX_LONG_JOBS);
        Self {
            incoming_requests,
            console_message_sender: bus.console_message_sender(),
            short_incoming,
            short_outgoing,
            long_incoming,
            long_outgoing,
        }
    }
    fn handle_result(
        &self,
        message: Result<RibbleMessage, RibbleError>,
    ) -> Result<(), RibbleError> {
        match message {
            Ok(message) => self.handle_message(message),
            Err(err) => self.handle_error(err),
        }
    }
    // TODO: determine why this is returning an error...?
    fn handle_message(&self, message: RibbleMessage) -> Result<(), RibbleError> {
        match message {
            RibbleMessage::Console(msg) => Ok({
                if self.console_message_sender.send(msg).is_err() {
                    todo!("LOG THIS");
                }
            }),
            // NOTE: if for some reason a Progress message needs to be returned via thread,
            // this will panic and need refactoring.
            // TODO: Just remove the RibbleMessage and use ConsoleMessage.
            RibbleMessage::Progress(_) => unreachable!(),
        }
    }
    // TODO: determine why this is returning an error.
    fn handle_error(&self, error: RibbleError) -> Result<(), RibbleError> {
        let error_msg = ConsoleMessage::Error(error);
        Ok({
            if self.console_message_sender.send(error_msg).is_err() {
                todo!("LOG THIS");
            }
        })
    }
}

pub(super) struct WorkerEngine {
    inner: Arc<WorkerInner>,
    // TODO: swap the error type here once errors have been reimplemented.
    work_thread: Option<JoinHandle<Result<(), RibbleError>>>,
}

impl WorkerEngine {
    pub(super) fn new(incoming_request: Receiver<WorkRequest>, bus: Bus) -> Self {
        let inner = Arc::new(WorkerInner::new(incoming_request, bus));
        let thread_inner = Arc::clone(&inner);

        let work_thread = Some(std::thread::spawn(move || {
            let forwarder_inner = Arc::clone(&thread_inner);
            let long_job_inner = Arc::clone(&thread_inner);
            let short_job_inner = Arc::clone(&thread_inner);

            let res = scope(|s| {
                // NOTE: at the moment, it's -probably- okay for this thread to block.
                // If this starts to become an issue once bounds are sorted out, look
                // at implementing a priority system + bounded joining.
                let _forwarder = s.spawn(move || {
                    while let Ok(request) = forwarder_inner.incoming_requests.recv() {
                        match request {
                            WorkRequest::Long(work) => {
                                if forwarder_inner.long_outgoing.send(work).is_err() {
                                    todo!("LOGGING.");
                                }
                            }
                            WorkRequest::Short(work) => {
                                if forwarder_inner.short_outgoing.send(work).is_err() {
                                    todo!("LOGGING.");
                                }
                            }
                        }
                    }
                });

                // TODO: remove the INTO once the errors are fixed.
                let _short_worker = s.spawn(move || {
                    while let Ok(work) = short_job_inner.short_incoming.recv() {
                        // TODO: get rid of the ? operator => these don't need to return an error.
                        match work.join() {
                            Ok(res) => thread_inner.handle_result(res),
                            // TODO: it might actually better to just panic the app until the new implementation
                            // is sorted out -> In no places are the work threads expected to actually panic.
                            // All errors from ribble_whisper are handled with results -> so it might be
                            // better to treat as fatal and crash the app.
                            Err(err) => {
                                let ribble_error = RibbleError::ThreadPanic(format!("{:?}", err));
                                thread_inner.handle_error(ribble_error)
                            } // Since handle_result/handle_error only return Err when the kernel's
                              // not set, unwrapping here will panic the worker thread and information
                              // should bubble up accordingly.
                        }?;
                    }
                });

                // TODO: remove the INTO once the errors are fixed.
                let _long_worker = s.spawn(move || {
                    while let Ok(work) = short_job_inner.long_incoming.recv() {
                        // TODO: get rid of the ? operator => these don't need to return an error.
                        match work.join() {
                            Ok(res) => thread_inner.handle_result(res),
                            Err(err) => {
                                let ribble_error = RibbleError::ThreadPanic(format!("{:?}", err));
                                thread_inner.handle_error(ribble_error)
                            } // Since handle_result/handle_error only return Err when the kernel's
                              // not set, unwrapping here will panic the worker thread and information
                              // should bubble up accordingly.
                        }?;
                    }
                });

                Ok(())
                    .join()
                    .map_err(|e| RibbleError::ThreadPanic(format!("{}", e)))
            })
            .map_err(|e| RibbleError::ThreadPanic(format!("{:?}", e)))??;
            res
        }));

        Self { inner, work_thread }
    }
}

impl Drop for WorkerEngine {
    fn drop(&mut self) {
        if let Some(handle) = self.work_thread.take() {
            handle
                .join()
                .expect("The worker thread is not expected to panic and should run without issues.")
                .expect("I'm not quite sure as to what the error conditions for this should be.");
        }
    }
}

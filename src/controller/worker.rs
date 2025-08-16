use crate::controller::{Bus, ConsoleMessage, RibbleMessage, RibbleWork, WorkRequest};
use crate::utils::errors::RibbleError;
use crossbeam::scope;
use ribble_whisper::utils::{Receiver, Sender, get_channel};
use std::error::Error;
use std::sync::Arc;
use std::thread::JoinHandle;

struct WorkerInner {
    incoming_requests: Receiver<WorkRequest>,
    console_message_sender: Sender<ConsoleMessage>,
    // If double-buffering is insufficient, swap to priority or look at work-stealing.
    short_incoming: Receiver<RibbleWork>,
    short_outgoing: Sender<RibbleWork>,
    long_incoming: Receiver<RibbleWork>,
    long_outgoing: Sender<RibbleWork>,
}
impl WorkerInner {
    const MAX_SHORT_JOBS: usize = 16;
    const MAX_LONG_JOBS: usize = 2 * Self::MAX_SHORT_JOBS;
    fn new(incoming_requests: Receiver<WorkRequest>, bus: &Bus) -> Self {
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
    fn handle_result(&self, message: Result<RibbleMessage, RibbleError>) {
        match message {
            Ok(message) => self.handle_message(message),
            Err(err) => self.handle_error(err),
        }
    }
    fn handle_message(&self, message: RibbleMessage) {
        match message {
            RibbleMessage::Console(msg) => {
                if let Err(e) = self.console_message_sender.send(msg) {
                    log::warn!(
                        "Console engine closed. Cannot send new messages.\nError source: {:#?}",
                        e.source()
                    );
                }
            }

            RibbleMessage::BackgroundWork(msg) => {
                if let Err(e) = msg {
                    let err_msg = ConsoleMessage::Error(e);
                    if let Err(e) = self.console_message_sender.send(err_msg) {
                        log::warn!(
                            "Console engine closed. Cannot send new error messages.\nError source: {:#?}",
                            e.source()
                        );
                    }
                }
            }
        }
    }

    fn handle_error(&self, error: RibbleError) {
        // Log the error message internally.
        log::error!("{}", &error);
        // Propagate to the console.
        let error_msg = ConsoleMessage::Error(error);
        if let Err(e) = self.console_message_sender.send(error_msg) {
            log::warn!(
                "Console engine closed. Cannot send new error messages.\nError source: {:#?}",
                e.source()
            );
        }
    }
}

pub(super) struct WorkerEngine {
    _inner: Arc<WorkerInner>,
    work_thread: Option<JoinHandle<()>>,
}

impl WorkerEngine {
    pub(super) fn new(
        incoming_request: Receiver<WorkRequest>,
        bus: &Bus,
    ) -> Result<Self, RibbleError> {
        let inner = Arc::new(WorkerInner::new(incoming_request, bus));
        let thread_inner = Arc::clone(&inner);

        let mut work_thread = Some(std::thread::spawn(move || {
            let forwarder_inner = Arc::clone(&thread_inner);
            let long_job_inner = Arc::clone(&thread_inner);
            let short_job_inner = Arc::clone(&thread_inner);

            // This scope block first maps the error and then unwraps it to
            // propagate the panic up to the full thread.
            scope(|s| {
                // NOTE: at the moment, it's -probably- okay for this thread to block.
                // If this starts to become an issue once bounds are sorted out, look
                // at implementing a priority system + bounded joining.

                // -> There's also a sentinel value to prevent deadlocking at drop.
                let _forwarder = s.spawn(move |_| {
                    while let Ok(request) = forwarder_inner.incoming_requests.recv() {
                        match request {
                            WorkRequest::Long(work) => {
                                if let Err(e) = forwarder_inner.long_outgoing.send(RibbleWork::Work(work)) {
                                    log::warn!("Worker long queue somehow closed. Cannot forward in new requests.\nError source: {:#?}", e.source());
                                }
                            }
                            WorkRequest::Short(work) => {
                                if let Err(e) = forwarder_inner.short_outgoing.send(RibbleWork::Work(work)) {
                                    log::warn!("Worker short queue somehow closed. Cannot forward in new requests.\nError source: {:#?}", e.source());
                                }
                            }
                            WorkRequest::Shutdown => {
                                // Forward the sentinels to each of the smaller threads.
                                if let Err(e) = forwarder_inner.long_outgoing.send(RibbleWork::Sentinel) {
                                    log::warn!("Worker long queue somehow closed. Cannot forward in new requests.\nError source: {:#?}", e.source());
                                }
                                if let Err(e) = forwarder_inner.short_outgoing.send(RibbleWork::Sentinel) {
                                    log::warn!("Worker short queue somehow closed. Cannot forward in new requests.\nError source: {:#?}", e.source());
                                }

                                // Then break the WorkRequest loop to allow the engine to close.
                                break;
                            }
                        }
                    }
                });

                let _short_worker = s.spawn(move |_| {
                    while let Ok(work) = short_job_inner.short_incoming.recv() {
                        match work {
                            RibbleWork::Work(work) => {
                                match work.join() {
                                    Ok(res) => short_job_inner.handle_result(res),
                                    Err(err) => {
                                        let ribble_error = RibbleError::ThreadPanic(format!("{err:#?}"));
                                        short_job_inner.handle_error(ribble_error);
                                    }
                                };
                            }
                            RibbleWork::Sentinel => break,
                        }
                    }
                });

                let _long_worker = s.spawn(move |_| {
                    while let Ok(work) = long_job_inner.long_incoming.recv() {
                        match work {
                            RibbleWork::Work(work) => {
                                match work.join() {
                                    Ok(res) => long_job_inner.handle_result(res),
                                    Err(err) => {
                                        let ribble_error = RibbleError::ThreadPanic(format!("{err:#?}"));
                                        long_job_inner.handle_error(ribble_error)
                                    } // Since handle_result/handle_error only return Err when the kernel's
                                    // not set, unwrapping here will panic the worker thread and information
                                    // should bubble up accordingly.
                                };
                            }
                            RibbleWork::Sentinel => break,
                        }
                    }
                });
            })
                .map_err(|e| RibbleError::ThreadPanic(format!("{e:?}"))).unwrap();
        }));

        // If the worker thread fails to spin up, return an error at construction time.
        if work_thread
            .as_ref()
            .is_some_and(|thread| thread.is_finished())
        {
            let inner = work_thread.take().unwrap();
            return match inner.join() {
                Ok(_) => {
                    let err = RibbleError::Core(
                        "Worker thread returned before construction finished.".to_string(),
                    );
                    Err(err)
                }
                Err(e) => {
                    let err = RibbleError::ThreadPanic(format!(
                        "Worker thread panicked at construction.\nError: {e:#?}"
                    ));
                    Err(err)
                }
            };
        }

        Ok(Self {
            _inner: inner,
            work_thread,
        })
    }
}

impl Drop for WorkerEngine {
    fn drop(&mut self) {
        log::info!("Dropping WorkerEngine.");
        if let Some(handle) = self.work_thread.take() {
            log::info!("Joining WorkerEngine work thread.");
            handle.join().expect(
                "The worker thread is not expected to panic and should run without issues.",
            );
            log::info!("WorkerEngine work thread joined.");
        }
    }
}

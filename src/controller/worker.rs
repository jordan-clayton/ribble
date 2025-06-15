use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::console_message::NewConsoleMessage;
use crate::utils::errors::{WhisperAppError, WhisperAppErrorType};
use crossbeam::channel::{self, Receiver, SendError, Sender};
use parking_lot::Mutex;
use std::sync::Arc;
use std::thread::JoinHandle;


// Types for returning output up to the controller to be pushed to its appropriate spot.
type ConsoleCallback = Box<dyn Fn(NewConsoleMessage) + Send + Sync>;
type TranscriptionCallback = Box<dyn Fn(String) + Send + Sync>;

struct RibbleWorkerInner {
    incoming: Receiver<RibbleWorkerHandle>,
    on_console: Mutex<Option<ConsoleCallback>>,
    on_transcription_finish: Mutex<Option<TranscriptionCallback>>,
}

impl RibbleWorkerInner {
    fn handle_result(&self, message: Result<RibbleMessage, WhisperAppError>) {
        match message {
            Ok(message) => self.handle_message(message),
            Err(err) => self.handle_error(err),
        }
    }
    fn handle_message(&self, message: RibbleMessage) {
        match message {
            RibbleMessage::Console(msg) => {
                let try_callback = self.on_console.lock();
                if let Some(callback) = try_callback {
                    callback(msg)
                }
            }
            // NOTE: if for some reason a Progress message needs to be returned via thread,
            // this will panic and need refactoring.
            RibbleMessage::Progress(_) => unreachable!(),
            RibbleMessage::TranscriptionOutput(msg) => {
                let try_callback = self.on_transcription_finish.lock();
                if let Some(callback) = try_callback {
                    callback(msg)
                }
            }
        }
    }
    fn handle_error(&self, error: WhisperAppError) {
        let try_callback = self.on_console.lock();
        if let Some(callback) = try_callback {
            // TODO: once errors have been re-implemented, just send the error object to the enum constructor
            let error_msg = NewConsoleMessage::Error(error.to_string());
            callback(error_msg)
        }
    }
}

pub struct RibbleWorkerEngine {
    outgoing: Sender<RibbleWorkerHandle>,
    inner: Arc<RibbleWorkerInner>,
    work_thread: Option<JoinHandle<()>>,
}

impl RibbleWorkerEngine {
    pub(crate) fn new() -> Self {
        // TODO: factor out a constant for the number of workers this can hold
        let (outgoing, incoming) = channel::bounded::<RibbleWorkerHandle>(100);

        let inner = Arc::new(RibbleWorkerInner {
            incoming,
            on_console: Mutex::new(None),
            on_transcription_finish: Mutex::new(None),
        });
        let thread_inner = Arc::clone(&inner);

        let work_thread = Some(std::thread::spawn(move || {
            while let Ok(work) = thread_inner.incoming.recv() {
                match work.join() {
                    Ok(res) => thread_inner.handle_result(res),
                    Err(err) => {
                        // Map the error as best as possible
                        // TODO: this will need to be reimplemented once errors have been refactored
                        let ribble_error = WhisperAppError::new(
                            WhisperAppErrorType::ThreadError,
                            format!("Thread panicked! Error: {:?}", err),
                            false,
                        );
                        thread_inner.handle_error(ribble_error);
                    }
                }
            }
        }));

        Self {
            outgoing,
            inner,
            work_thread,
        }
    }
    pub(crate) fn set_on_console(&self, callback: Option<impl Fn(NewConsoleMessage) + Send + Sync + 'static>) {
        let lock = self.inner.on_console.lock();
        *lock = callback.map(|cb| Box::new(cb) as Box<_>);
    }
    pub(crate) fn set_on_transcription_finish(
        &self,
        callback: Option<impl Fn(String) + Send + Sync + 'static>,
    ) {
        let lock = self.inner.on_transcription_finish.lock();
        *lock = callback.map(|cb| Box::new(cb) as Box<_>);
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
            let _ = handle.join();
        }
    }
}

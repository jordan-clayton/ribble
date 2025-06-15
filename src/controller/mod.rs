use std::cell::UnsafeCell;
use std::thread::JoinHandle;
use crate::utils::console_message::NewConsoleMessage;
use crate::utils::errors::WhisperAppError;
use crate::utils::progress::Progress;

pub(crate) mod utils;
pub(crate) mod whisper_app_controller;

// TODO: modify visibility if needed
mod worker;
mod transcriber;
mod visualizer;
mod recorder;
mod progress;
mod console;

type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, WhisperAppError>>;

pub(crate) enum RibbleMessage {
    Console(NewConsoleMessage),
    Progress(Progress),
    TranscriptionOutput(String),
}

// TODO: put this somewhere else if it makes sense to
pub struct UIThreadOnly<T>{
    inner: UnsafeCell<T>
}

// NOTE: these must only be called from a single (UI thread), otherwise
// thread-safety cannot be guaranteed
unsafe impl<T> Sync for UIThreadOnly<T>{}
unsafe impl<T> Send for UIThreadOnly<T>{}

impl<T> UIThreadOnly<T>{
    pub(crate) fn new(inner: T) -> Self{
        Self{inner}
    }

    // TODO: remove if not necessary
    pub(crate) unsafe fn get_ref(&self) -> &T {
      &*self.inner.get()
    }

    pub(crate) unsafe fn get_mut(&self) -> &mut T{
       &mut *self.inner.get()
    }
    pub(crate) fn get_ptr(&self) -> *mut T{
       self.inner.get()
    }
}

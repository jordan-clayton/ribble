use crate::utils::errors::RibbleAppError;
use ribble_whisper::utils::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;

// TODO: remove these excess utils; most of them are not necessary for the application.
pub(crate) mod utils;
pub(crate) mod whisper_app_controller;

// TODO: determine visibility and fix this later.
pub(crate) mod console;
mod downloader;
mod kernel;
pub(crate) mod progress;
mod recorder;
// TODO: move transcriber feedback out of this module and stick in user_preferences.
pub(crate) mod transcriber;
mod visualizer;
mod worker;
mod writer;

type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, RibbleAppError>>;

pub(crate) enum RibbleMessage {
    Console(console::ConsoleMessage),
    // TODO: possibly rename this; use it for cleanup if a job fails.
    // Send the messages in over the queue.
    Progress(progress::ProgressMessage),
}

struct Bus {
    console_sender: Sender<console::ConsoleMessage>,
    // TODO: this is not correct yet -> this will need a ProgressMessage or similar.
    // It will also most likely require a hashmap instead of a slab because IDs are lost across the
    // channel.
    progress_sender: Sender<progress::ProgressMessage>,
    worker_sender: Sender<RibbleWorkerHandle>,
    // TODO: future thing -> possibly stick this in a data structure with the sample rate.
    // Pre-computing and re-initializing the FFT thingy might get a little sticky.
    visualizer_sender: Sender<Arc<[f32]>>,
}

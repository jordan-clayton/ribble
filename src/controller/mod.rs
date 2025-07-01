use crate::utils::errors::RibbleAppError;
use ribble_whisper::utils::Sender;
use std::sync::Arc;
use std::thread::JoinHandle;

// TODO: remove these excess utils; most of them are not necessary for the application.
pub(crate) mod utils;
pub(crate) mod whisper_app_controller;

// TODO: determine visibility and fix this later.
pub(crate) mod console;
mod kernel;
pub(crate) mod progress;
mod recorder;
mod transcriber;
mod visualizer;
mod worker;
mod writer;
mod downloader;

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
    visualizer_sender: Sender<Arc<[f32]>>,
}

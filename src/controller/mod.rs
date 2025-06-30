use crate::utils::errors::RibbleAppError;
use console::ConsoleMessage;
use progress::ProgressMessage;
use std::thread::JoinHandle;
use ribble_whisper::utils::{Sender, Receiver}
use std::sync::Arc;

// TODO: remove these excess utils; most of them are not necessary for the application.
pub(crate) mod utils;
pub(crate) mod whisper_app_controller;

// TODO: modify visibility if needed
mod console;
mod kernel;
mod progress;
mod recorder;
mod transcriber;
mod visualizer;
mod worker;
mod writer;

type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, RibbleAppError>>;

pub(crate) enum RibbleMessage {
    Console(ConsoleMessage),
    // Uh, I don't think this is ever seeing use? If so, change the RibbleMessage definition
    Progress(ProgressMessage),
}

struct Bus {
    console_sender: Sender<ConsoleMessage>,
    // TODO: this is not correct yet -> this will need a ProgressMessage or similar.
    // It will also most likely require a hashmap instead of a slab because IDs are lost across the
    // channel.
    progress_sender: Sender<ProgressMessage>,
    worker_sender: Sender<RibbleWorkerHandle>,
    visualizer_sender: Sender<Arc<[f32]>>
}

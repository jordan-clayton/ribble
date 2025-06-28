use crate::utils::errors::RibbleAppError;
use console::ConsoleMessage;
use progress::Progress;
use std::thread::JoinHandle;

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

type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, RibbleAppError>>;

pub(crate) enum RibbleMessage {
    Console(ConsoleMessage),
    Progress(Progress),
    TranscriptionOutput(String),
}


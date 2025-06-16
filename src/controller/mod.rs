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
mod kernel;

type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, WhisperAppError>>;

pub(crate) enum RibbleMessage {
    Console(NewConsoleMessage),
    Progress(Progress),
    TranscriptionOutput(String),
}
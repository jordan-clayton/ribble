use ribble_whisper::utils::errors::RibbleWhisperError;
use std::error::Error;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RibbleError {
    // RibbleWhisper has its own to_string impls.
    #[error("Ribble Whisper: {0}")]
    RibbleWhisper(#[from] RibbleWhisperError),
    // TODO: This might actually be fine, but if errors need to be clearer, refactor accordingly.
    #[error("Core: {0}")]
    Core(String),
    #[error("Thread Panic: {0}")]
    ThreadPanic(String),
    #[error("Visualizer Error: {0}")]
    VisualizerError(#[from] realfft::FftError),
    #[error("IO Error: {0}")]
    IOError(#[from] std::io::Error),
    #[error("WavError: {0}")]
    WavError(#[from] hound::Error),
    #[error("DirectoryWatcher: {0}")]
    DirectoryWatcher(#[from] notify_debouncer_full::notify::Error),
    #[error("Egui: {0}")]
    Egui(#[from] egui::load::LoadError),
    #[error("Eframe: {0}")]
    Eframe(#[from] eframe::Error),
    #[error("Logger: {0}")]
    Logger(#[from] flexi_logger::FlexiLoggerError),
}

impl Error for RibbleError {}

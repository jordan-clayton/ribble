use ribble_whisper::utils::errors::RibbleWhisperError;
use strum::{AsRefStr, Display, EnumDiscriminants, EnumIter, EnumString, IntoStaticStr};
use thiserror::Error;


// NOTE: THIS NEEDS TO BE TESTED, but it should be perfectly fine to just call .into() to convert
// to a discriminant without consuming the error.
#[derive(Debug, Error, EnumDiscriminants)]
#[strum_discriminants(name(RibbleErrorCategory))]
#[strum_discriminants(derive(Display, EnumIter, AsRefStr, IntoStaticStr, EnumString))]
#[strum_discriminants(strum(prefix = "RibbleError: "))]
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
    // This needs to be manually mapped; Eframe errors aren't Sync or Send
    #[error("Eframe: {0}")]
    Eframe(String),
    #[error("Logger: {0}")]
    Logger(#[from] flexi_logger::FlexiLoggerError),
    #[error("Crash-Handler: {0}")]
    CrashHandler(#[from] crash_handler::Error),
    #[error("Conversion Error: {0}")]
    ConversionError(&'static str),
}
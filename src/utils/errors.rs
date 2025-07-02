use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RibbleError {
    // RibbleWhisper has its own to_string impls.
    #[error("Ribble Whisper: {0}")]
    RibbleWhisper(#[from] ribble_whisper::utils::errors::RibbleWhisperError),
    // TODO: this is a placeholder, replace with clearer errors.
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
}

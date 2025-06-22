use ribble_whisper::utils::errors::RibbleWhisperError;
use std::any::Any;
use std::fmt::Formatter;
use strum::{Display, EnumIter};
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum RibbleError {
    // RibbleWhisper has its own to_string impls.
    #[error("{}")]
    RibbleWhisper(#[from] ribble_whisper::utils::errors),
    // TODO: this is a placeholder, replace with clearer errors.
    #[error("Core: {}")]
    Core(String),
    #[error("Thread Panic: {}")]
    ThreadPanic(String),
    #[error("Visualizer Error: {}")]
    VisualizerError(#[from] realfft::FftError),
}

#[derive(Debug)]
pub(crate) enum Cleanup {
    Dirty(Box<dyn FnMut()>),
    Cleaned,
}

impl Cleanup {
    pub(crate) fn needs_cleanup(&self) -> bool {
        !self.is_cleaned()
    }
    pub(crate) fn is_cleaned(&self) -> bool {
        matches!(self, Cleanup::Cleaned)
    }
}

// TODO: Possibly just call this RibbleError & change the Enum to RibbleInnerError or something?
// TODO: might also not be able to derive Error without at least implementing toString. Not sure.
#[derive(Debug, Error)]
pub(crate) struct RibbleAppError {
    error: RibbleError,
    cleanup: Cleanup,
}

impl RibbleAppError {
    pub(crate) fn new(error: RibbleError) -> Self {
        Self { error, cleanup: Cleanup::Cleaned }
    }

    // Sets an optional cleanup closure/function.
    // Setting this will require cleanup to be called before consuming the inner error.
    pub(crate) fn with_cleanup(self, cleanup: impl FnMut()) -> RibbleAppError {
        RibbleAppError { error: self.error, cleanup: Cleanup::Dirty(Box::new(cleanup)) }
    }

    pub(crate) fn needs_cleanup(&self) -> bool {
        self.cleanup.needs_cleanup()
    }

    // Consumes and returns the ribble error if the cleanup closure has been called
    // Returns the original RibbleAppError if cleanup hasn't been called yet
    // (so that cleanup can, actually happen).
    pub(crate) fn into_error(self) -> Result<RibbleError, Self> {
        if self.cleanup.is_cleaned() {
            Ok(self.error)
        } else {
            Err(self)
        }
    }

    pub(crate) fn to_string(&self) -> String {
        self.error.to_string()
    }

    // TODO: For now, let this panic until bugs are stamped out, then look at handling gracefully.
    pub(crate) fn run_cleanup(&mut self) {
        if self.cleanup.is_cleaned() {
            return;
        }
        self.cleanup();
        self.cleanup = Cleanup::Cleaned;
    }
}

impl From<RibbleError> for RibbleAppError {
    fn from(error: RibbleError) -> Self {
        Self { error, cleanup: Cleanup::Cleaned }
    }
}

// TODO: look here if there are problems.
impl From<RibbleWhisperError> for RibbleAppError {
    fn from(error: RibbleWhisperError) -> Self {
        Self { error: RibbleError::RibbleWhisper(error), cleanup: Cleanup::Cleaned }
    }
}


// TODO: handle this... better.
#[derive(Clone, Debug)]
pub struct WhisperAppError {
    error_type: WhisperAppErrorType,
    reason: String,
    fatal: bool,
}

impl WhisperAppError {
    pub fn new(error_type: WhisperAppErrorType, reason: String, fatal: bool) -> Self {
        Self {
            error_type,
            reason,
            fatal,
        }
    }

    pub fn fatal(&self) -> bool {
        self.fatal
    }
}

impl std::fmt::Display for WhisperAppError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "WhisperAppError: {}. Cause: {}",
            self.error_type.to_string(),
            self.reason
        )
    }
}

#[derive(Clone, Copy, Debug, Display, EnumIter)]
pub enum WhisperAppErrorType {
    WhisperRealtime,
    IOError,
    ParameterError,
    ThreadError,
    GUIError,
    Unknown,
}

pub fn extract_error_message(error: Box<dyn Any + Send>) -> String {
    match error.downcast_ref::<&'static str>() {
        None => match error.downcast_ref::<String>() {
            None => String::from("Unknown error"),
            Some(s) => s.clone(),
        },
        Some(s) => String::from(*s),
    }
}

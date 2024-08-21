use std::fmt::Formatter;

use strum::{Display, EnumIter};

#[derive(Clone, Debug)]
pub struct WhisperAppError {
    error_type: WhisperAppErrorType,
    reason: String,
}

impl WhisperAppError {
    pub fn new(error_type: WhisperAppErrorType, reason: String) -> Self {
        Self { error_type, reason }
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
    IOError,
    ParameterError,
    ThreadError,
    GUIError,
    Unknown,
}

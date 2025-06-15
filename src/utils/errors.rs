use std::any::Any;
use std::fmt::Formatter;

use strum::{Display, EnumIter};

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

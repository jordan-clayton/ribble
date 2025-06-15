use crate::utils::console_message::NewConsoleMessage;
use crate::utils::errors::WhisperAppError;
use crate::utils::progress::Progress;

pub(crate) mod audio_analysis;
pub(crate) mod console_message;
pub(crate) mod constants;
pub(crate) mod errors;
pub(crate) mod file_mgmt;
pub(crate) mod preferences;
pub(crate) mod progress;
pub(crate) mod recorder_configs;
pub(crate) mod sdl_audio_wrapper;
pub(crate) mod threading;
pub(crate) mod workers;



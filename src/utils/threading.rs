// TODO: Flatten this.
use crate::utils::{
    console_message::ConsoleMessageType
    ,
    errors::{extract_error_message, WhisperAppError, WhisperAppErrorType},
};

// TODO: remove or mark inline. Check call sites for use and determine why I wrote this.
// It does not make sense to call this function in like 8 different places.
// Something's up.
// TODO: once this is... solved, remove and nuke this entire file.
pub fn get_max_threads() -> std::ffi::c_int {
    match std::thread::available_parallelism() {
        Ok(n) => n.get() as std::ffi::c_int,
        Err(_) => 2,
    }
}
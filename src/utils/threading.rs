// TODO: just remove this entire file.
use std::thread::JoinHandle;

use crossbeam::channel::SendError;
// TODO: remove SDL.
use sdl2::log::log;

// TODO: Flatten this.
use crate::controller::console::ConsoleMessage;
// TODO: Flatten this.
use crate::{
    controller::whisper_app_controller::WhisperAppController,
    utils::{
        console_message::ConsoleMessageType,
        constants,
        errors::{extract_error_message, WhisperAppError, WhisperAppErrorType},
    },
};

// TODO: remove or mark inline. Check call sites for use and determine why I wrote this.
// It does not make sense to call this function in like 8 different places.
// Something's up.
pub fn get_max_threads() -> std::ffi::c_int {
    match std::thread::available_parallelism() {
        Ok(n) => n.get() as std::ffi::c_int,
        Err(_) => 2,
    }
}

// TODO: Remove this function.
pub fn join_threads_loop(
    msg_queue: crossbeam::channel::Receiver<JoinHandle<Result<String, WhisperAppError>>>,
    controller: WhisperAppController,
) {
    loop {
        let msg = msg_queue.recv();
        match msg {
            Ok(handle) => {
                let res = handle.join();

                // Thread panic.
                if let Err(e) = res {
                    let e_msg = extract_error_message(e);

                    let msg = ConsoleMessage::new(
                        ConsoleMessageType::Error,
                        format!("Worker thread panicked.  Info: {}", e_msg),
                    );
                    if let Err(_) = send_console_msg(msg.clone(), controller.clone()) {
                        // Print to stderr
                        eprintln!("{}", msg);
                        // This will crash the app. Channels are required for the app to operate.
                        controller.mark_poisoned();
                    }
                    continue;
                }

                let res = res.unwrap();

                if let Err(e) = res {
                    let msg = ConsoleMessage::new(
                        ConsoleMessageType::Error,
                        format!("{}", e.to_string()),
                    );
                    if let Err(_) = send_console_msg(msg.clone(), controller.clone()) {
                        // Print to stderr
                        eprintln!("{}", msg);
                        controller.mark_poisoned();
                    };

                    if e.fatal() {
                        // Print to stderr
                        eprintln!("{}", msg);
                        controller.mark_poisoned();
                    }
                    continue;
                }

                let res = res.unwrap();

                // Check for finished.
                if res == constants::CLOSE_APP {
                    break;
                }

                let msg = ConsoleMessage::new(ConsoleMessageType::Status, res);
                if let Err(_) = send_console_msg(msg, controller.clone()) {
                    // Print to stderr
                    let msg = WhisperAppError::new(
                        WhisperAppErrorType::IOError,
                        String::from("Console message channel closed."),
                        true,
                    );
                    eprintln!("{}", msg);
                    controller.mark_poisoned();
                }
            }
            // Channel has closed.
            Err(_) => {
                break;
            }
        }
    }
    #[cfg(debug_assertions)]
    log("Joiner thread closed.");
}

// TODO: this does not need to be a function.
// Just emit the console message through the controller.
fn send_console_msg(
    msg: ConsoleMessage,
    controller: WhisperAppController,
) -> Result<(), SendError<ConsoleMessage>> {
    controller.send_console_message(msg)
}

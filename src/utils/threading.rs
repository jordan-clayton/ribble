use std::any::Any;
use std::thread::JoinHandle;

use whisper_realtime::errors::{WhisperRealtimeError, WhisperRealtimeErrorType};

use crate::utils::configs::WorkerType;
use crate::utils::console_message::{ConsoleMessage, ConsoleMessageType};
use crate::utils::constants;
use crate::whisper_app_context::WhisperAppController;

pub fn get_max_threads() -> std::ffi::c_int {
    match std::thread::available_parallelism() {
        Ok(n) => n.get() as std::ffi::c_int,
        Err(_) => 2,
    }
}

// TODO: refactor this once errors implemented - Likely better to implement a "Console Msg".
pub fn join_threads(msg_queue: crossbeam::channel::Receiver<(WorkerType, JoinHandle<Result<String, Box<dyn Any + Send>>>)>, controller: WhisperAppController) {
    loop {
        let msg = msg_queue.recv();
        match msg {
            Ok(m) => {
                let (worker, handle) = m;
                let res = handle.join();
                if let Err(e) = res {
                    let msg = ConsoleMessage::new(ConsoleMessageType::ERROR, format!("{:?}", e));
                    send_console_msg(msg, controller.clone());
                    continue;
                }
                let res = res.unwrap();

                if let Err(e) = res {
                    let msg = ConsoleMessage::new(ConsoleMessageType::ERROR, format!("{:?}", e));
                    send_console_msg(msg, controller.clone());
                    continue;
                }

                let res = res.unwrap();

                if worker == WorkerType::DOWNLOADING || worker == WorkerType::RECORDING {
                    let msg = ConsoleMessage::new(ConsoleMessageType::STATUS, res);
                    send_console_msg(msg, controller.clone());
                } else {
                    // Transcription thread -> send to transcription window.
                    let sender = controller.transcription_text_sender();
                    sender.send(Ok((String::from(constants::CLEAR_MSG), true))).expect("Transcription Channel Closed");
                    sender.send(Ok((res, true))).expect("Transcription Channel Closed");
                }
            }
            // Channel has closed.
            Err(_) => {
                break;
            }
        }
    }
}

// TODO: refactor once console messaging implemented.
fn send_console_msg(msg: ConsoleMessage, controller: WhisperAppController) {
    let error = WhisperRealtimeError::new(WhisperRealtimeErrorType::Unknown, format!("Thread panicked. Error: {}", msg));
    controller.send_error(error).expect("Error channel closed");
}


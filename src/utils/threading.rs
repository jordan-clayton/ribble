use std::any::Any;
use std::thread::JoinHandle;

use crate::controller::whisper_app_controller::WhisperAppController;
use crate::utils::console_message::{ConsoleMessage, ConsoleMessageType};
use crate::utils::constants;
use crate::utils::workers::WorkerType;

pub fn get_max_threads() -> std::ffi::c_int {
    match std::thread::available_parallelism() {
        Ok(n) => n.get() as std::ffi::c_int,
        Err(_) => 2,
    }
}

// TODO: revisit once proper error handling.
pub fn join_threads_loop(
    msg_queue: crossbeam::channel::Receiver<(
        WorkerType,
        JoinHandle<Result<String, Box<dyn Any + Send>>>,
    )>,
    controller: WhisperAppController,
) {
    loop {
        let msg = msg_queue.recv();
        match msg {
            Ok(m) => {
                let (worker, handle) = m;
                let res = handle.join();
                if let Err(e) = res {
                    let msg = ConsoleMessage::new(ConsoleMessageType::Error, format!("{:?}", e));
                    send_console_msg(msg, controller.clone());
                    continue;
                }
                let res = res.unwrap();

                if let Err(e) = res {
                    let msg = ConsoleMessage::new(ConsoleMessageType::Error, format!("{:?}", e));
                    send_console_msg(msg, controller.clone());
                    continue;
                }

                let res = res.unwrap();

                if worker == WorkerType::ThreadManagement {
                    if res == constants::CLOSE_MSG {
                        break;
                    }
                }

                let msg = ConsoleMessage::new(ConsoleMessageType::Status, res);
                send_console_msg(msg, controller.clone());
            }
            // Channel has closed.
            Err(_) => {
                break;
            }
        }
    }
}

// TODO: figure out a way to handle without panicking.
// Possibly a flag/MSG to catch in the Update loop.
fn send_console_msg(msg: ConsoleMessage, controller: WhisperAppController) {
    controller
        .send_console_message(msg)
        .expect("Error channel closed");
}

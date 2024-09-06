use std::{
    any::Any,
    thread::JoinHandle,
};

use sdl2::log::log;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    utils::{
        console_message::{ConsoleMessage, ConsoleMessageType},
        workers::WorkerType,
    },
};

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
    while controller.app_running() {
        let msg = msg_queue.recv();
        if !controller.app_running() {
            break;
        }
        match msg {
            Ok(m) => {
                let (_worker, handle) = m;
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

                let msg = ConsoleMessage::new(ConsoleMessageType::Status, res);
                send_console_msg(msg, controller.clone());
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

// TODO: figure out a way to handle without panicking.
// Possibly a flag/MSG to catch in the Update loop.
fn send_console_msg(msg: ConsoleMessage, controller: WhisperAppController) {
    controller
        .send_console_message(msg)
        .expect("Error channel closed");
}

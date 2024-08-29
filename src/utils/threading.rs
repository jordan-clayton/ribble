use std::any::Any;
use std::thread::JoinHandle;

use crate::controller::whisper_app_controller::WhisperAppController;
use crate::utils::configs::WorkerType;
use crate::utils::console_message::{ConsoleMessage, ConsoleMessageType};
use crate::utils::constants;

pub fn get_max_threads() -> std::ffi::c_int {
    match std::thread::available_parallelism() {
        Ok(n) => n.get() as std::ffi::c_int,
        Err(_) => 2,
    }
}

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

                if worker == WorkerType::Realtime || worker == WorkerType::Static {
                    // Transcription thread -> send to transcription window.
                    let sender = controller.transcription_text_sender();
                    sender
                        .send(Ok((String::from(constants::CLEAR_MSG), true)))
                        .expect("Transcription Channel Closed");
                    sender
                        .send(Ok((res, true)))
                        .expect("Transcription Channel Closed");
                } else {
                    let msg = ConsoleMessage::new(ConsoleMessageType::Status, res);
                    send_console_msg(msg, controller.clone());
                }
            }
            // Channel has closed.
            Err(_) => {
                break;
            }
        }
    }
}

fn send_console_msg(msg: ConsoleMessage, controller: WhisperAppController) {
    controller
        .send_console_message(msg)
        .expect("Error channel closed");
}

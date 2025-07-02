use crate::utils::errors::RibbleError;
use ribble_whisper::utils::Sender;
use std::thread::JoinHandle;

// TODO: remove these excess utils; most of them are not necessary for the application.
pub(crate) mod utils;
pub(crate) mod whisper_app_controller;

// TODO: fix visibility issues later -> common (message formats, etc.) need to go in different
// files.
pub(crate) mod console;
pub(crate) mod downloader;
mod kernel;
pub(crate) mod progress;
mod recorder;
// TODO: move transcriber feedback out of this module and stick in user_preferences.
pub(crate) mod transcriber;
mod visualizer;
pub(crate) mod worker;
mod writer;

type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, RibbleError>>;

pub(crate) enum RibbleMessage {
    Console(console::ConsoleMessage),
    // TODO: possibly rename this; use it for cleanup if a job fails.
    // Send the messages in over the queue.
    Progress(progress::ProgressMessage),
}

pub(super) struct Bus {
    console_message_sender: Sender<console::ConsoleMessage>,
    // TODO: this is not correct yet -> this will need a ProgressMessage or similar.
    // It will also most likely require a hashmap instead of a slab because IDs are lost across the
    // channel.
    progress_message_sender: Sender<progress::ProgressMessage>,
    work_request_sender: Sender<worker::WorkRequest>,
    write_request_sender: Sender<writer::WriteRequest>,
    // TODO: future thing -> possibly stick this in a data structure with the sample rate.
    // Pre-computing and re-initializing the FFT thingy might get a little sticky.
    visualizer_sample_sender: Sender<visualizer::VisualizerSample>,
    download_request_sender: Sender<downloader::DownloadRequest>,
}

impl Bus {
    pub(super) fn new(
        console_message_sender: Sender<console::ConsoleMessage>,
        progress_message_sender: Sender<progress::ProgressMessage>,
        work_request_sender: Sender<worker::WorkRequest>,
        write_request_sender: Sender<writer::WriteRequest>,
        visualizer_sample_sender: Sender<visualizer::VisualizerSample>,
        download_request_sender: Sender<downloader::DownloadRequest>,
    ) -> Self {
        Self {
            console_message_sender,
            progress_message_sender,
            work_request_sender,
            write_request_sender,
            visualizer_sample_sender,
            download_request_sender,
        }
    }

    pub(super) fn console_message_sender(&self) -> Sender<console::ConsoleMessage> {
        self.console_message_sender.clone()
    }

    pub(super) fn progress_message_sender(&self) -> Sender<progress::ProgressMessage> {
        self.progress_message_sender.clone()
    }
    pub(super) fn write_request_sender(&self) -> Sender<writer::WriteRequest> {
        self.write_request_sender.clone()
    }
    pub(super) fn work_request_sender(&self) -> Sender<worker::WorkRequest> {
        self.work_request_sender.clone()
    }
    pub(super) fn visualizer_sample_sender(&self) -> Sender<visualizer::VisualizerSample> {
        self.visualizer_sample_sender.clone()
    }
    pub(super) fn download_request_sender(&self) -> Sender<downloader::DownloadRequest> {
        self.download_request_sender.clone()
    }
}

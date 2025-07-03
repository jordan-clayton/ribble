use crate::controller::Bus;
use crate::controller::console::ConsoleMessage;
use crate::controller::progress::{Progress, ProgressMessage};
use crate::controller::worker::WorkRequest;
use crate::controller::{RibbleMessage, RibbleWorkerHandle};
use crate::utils::errors::RibbleError;
use ribble_whisper::downloader::SyncDownload;
use ribble_whisper::downloader::downloaders::sync_download_request;
use ribble_whisper::utils::callback::RibbleWhisperCallback;
use ribble_whisper::utils::{Receiver, Sender, get_channel};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;

pub(crate) struct DownloadRequest {
    url: String,
    // NOTE: this should probably just take the file_name from the slug
    // HANDLE THIS LOGIC HIGHER UP IN THE CONTROLLER.
    // e.g. Url::parse(url)?, url.path_segments_mut()?.pop() => returns the last bit of the URL.
    // OTHERWISE, take it in as an argument from the user.
    file_name: String,
    directory: PathBuf,
    // This is a pipe for sending back the file_name when
    // the download is completed so the caller can respond.
    // (e.g. Place the new entry in a ModelBank, refresh the bank, etc.)
    return_sender: Option<Sender<String>>,
}

// NOTE: when making a downlaod request in the model bank, spawn a wrapper thread that spawns the
// smaller download thread first and then waits for the file_name to be returned (possibly with
// timeout).
// THEN: on receipt of the string (or an err if the thread panics and memory gets deallocated),
// respond accordingly (e.g. put the new model in the model bank)

impl DownloadRequest {
    pub(crate) fn new() -> Self {
        Self {
            url: Default::default(),
            file_name: Default::default(),
            directory: Default::default(),
            return_sender: None,
        }
    }

    pub(crate) fn with_url(mut self, url: String) -> Self {
        self.url = url;
        self
    }
    pub(crate) fn with_file_name(mut self, file_name: String) -> Self {
        self.file_name = file_name;
        self
    }
    pub(crate) fn with_directory(mut self, directory: &Path) -> Self {
        self.directory = directory.to_path_buf();
        self
    }
    pub(crate) fn with_return_sender(mut self, sender: Sender<String>) -> Self {
        self.return_sender = Some(sender);
        self
    }

    fn decompose(self) -> (String, String, PathBuf, Option<Sender<String>>) {
        (self.url, self.file_name, self.directory, self.return_sender)
    }

    pub(crate) fn url(&self) -> &String {
        &self.url
    }
    pub(crate) fn file_name(&self) -> &String {
        &self.file_name
    }
    pub(crate) fn directory(&self) -> &Path {
        self.directory.as_path()
    }
}

struct DownloadEngineState {
    incoming_jobs: Receiver<DownloadRequest>,
    worker_sender: Sender<WorkRequest>,
    progress_sender: Sender<ProgressMessage>,
}

impl DownloadEngineState {
    fn new(incoming_jobs: Receiver<DownloadRequest>, bus: &Bus) -> Self {
        Self {
            incoming_jobs,
            worker_sender: bus.work_request_sender(),
            progress_sender: bus.progress_message_sender(),
        }
    }

    fn start_download(&self, job: DownloadRequest) -> RibbleWorkerHandle {
        std::thread::spawn(move || {
            let (url, file_name, dest_dir, return_sender) = job.decompose();

            let sync_downloader = sync_download_request(&url)?;
            let progress_job = Progress::new_determinate(
                "Downloading model.",
                sync_downloader.total_size() as u64,
            );
            let (id_sender, id_receiver) = get_channel(1);
            let progress_message = ProgressMessage::Request {
                job: progress_job,
                source: id_sender,
            };
            if self.progress_sender.send(progress_message).is_err() {
                todo!("LOGGING");
            }

            let progress_id = match id_receiver.recv() {
                Ok(id) => Some(id),
                Err(_) => {
                    todo!("LOGGING");
                    None
                }
            };

            let callback_progress_sender = self.progress_sender.clone();
            let progress_closure = move |n: usize| {
                if progress_id.is_none() {
                    return;
                }

                let update = ProgressMessage::Increment {
                    job_id: progress_id.unwrap(),
                    delta: n as u64,
                };
                if callback_progress_sender.send(update).is_err() {
                    todo!("LOGGING");
                }
            };

            let progress_callback = RibbleWhisperCallback::new(progress_closure);
            let mut sync_downloader = sync_downloader.with_progress_callback(progress_callback);

            // NOTE: This is a blocking call.
            sync_downloader.download(dest_dir.as_path(), &file_name)?;

            // Remove the progress job now that the file's downloaded.
            if let Some(id) = progress_id {
                let finished = ProgressMessage::Remove { job_id: id };
                if self.progress_sender.send(finished).is_err() {
                    todo!("LOGGING");
                }
            }

            let console_message =
                ConsoleMessage::Status(format!("Successfully downloaded {}", &file_name));
            let ribble_message = RibbleMessage::Console(console_message);

            // Send back the file-name to signal "this has been downloaded properly"
            // i.e. so that the caller can decide what to do (put the new model in the model bank).
            if let Some(sender) = return_sender {
                if sender.send(file_name).is_err() {
                    todo!("LOGGING");
                }
            }

            Ok(ribble_message)
        })
    }
}

pub(super) struct DownloadEngine {
    inner: Arc<DownloadEngineState>,
    work_thread: Option<JoinHandle<Result<(), RibbleError>>>,
}

impl DownloadEngine {
    // TODO: refactor this to take in a bus once the bus impl is done.
    pub(super) fn new(incoming_jobs: Receiver<DownloadRequest>, bus: &Bus) -> Self {
        let inner = Arc::new(DownloadEngineState::new(incoming_jobs, bus));
        let thread_inner = Arc::clone(&inner);

        let worker = std::thread::spawn(move || {
            while let Ok(download_job) = thread_inner.incoming_jobs.recv() {
                let work_request = WorkRequest::Short(thread_inner.start_download(download_job));
                if thread_inner.worker_sender.send(work_request).is_err() {
                    // TODO: do some sort of logging -> worker engine is deallocated and this should
                    // only be the case when the app is closing.
                    break;
                }
            }
            Ok(())
        });

        let work_thread = Some(worker);
        Self { inner, work_thread }
    }
}

impl Drop for DownloadEngine {
    fn drop(&mut self) {
        if let Some(handle) = self.work_thread.take() {
            handle.join()
                .expect("The DownloadEngine worker thread is not expected to ever panic.")
                .expect("I genuinely don't know what sort of error condition might cause things to fail.")
        }
    }
}

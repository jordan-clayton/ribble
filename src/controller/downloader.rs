use crate::controller::Progress;
use crate::controller::RibbleMessage;
use crate::controller::WorkRequest;
use crate::controller::{Bus, DownloadRequest};
use crate::controller::{ConsoleMessage, ProgressMessage};
use crate::utils::errors::RibbleError;
use ribble_whisper::downloader::SyncDownload;
use ribble_whisper::downloader::downloaders::sync_download_request;
use ribble_whisper::utils::callback::RibbleWhisperCallback;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{Receiver, Sender, get_channel};
use std::sync::Arc;
use std::thread::JoinHandle;

//
// TODO: Downloads are now cancellable -> expose methods to display this/expose functions in the
// UI.

// OKAY, so, downloading has been refactored (a lot simpler & more useful):
// - Temporary files by default -> no clobbering if a download gets cancelled.
// - File-names are, by and large, solved; this will, however, need a dummy "fallback" filename
// - These come from either: Content-Disposition, or the end of the URL, (or the fallback).
//
// - The ModelBank has a watcher on the dir and will refresh itself accordingly -> all that needs
//   to happen is the download.
//
// - All that needs to be done:
//      - fallback name
//      - bank of in-progress downloads + add abort callback tied to atomic boolean
//      - Methods to expose this bank: Either lock + clone, or take in FnMut for-each.
//      - Perhaps try both methods: profile and see what's faster.

const FALLBACK_NAME: &'static str = "fallback";

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

    // TODO: factor in logic to capture metadata -> the inner Progress is atomic an can be shared.
    // TODO TWICE: maybe rename from "start_download" to just "download_file"
    fn start_download(&self, job: DownloadRequest) -> Result<RibbleMessage, RibbleError> {
        let (url, file_name, dest_dir, return_sender) = job.decompose();

        let sync_downloader = sync_download_request(&url, FALLBACK_NAME)?;
        if sync_downloader.content_name() == FALLBACK_NAME {
            return Err(RibbleWhisperError::ParameterError(format!(
                "File not found, likely invalid url.\nURL:{url}"
            ))
            .into());
        }

        let progress_job =
            Progress::new_determinate("Downloading model.", sync_downloader.total_size() as u64);

        let progress_view = progress_job
            .progress_view()
            .expect("This method always returns some with a determinate progress job");

        // TODO: construct download metadata
        // Use the FileDownload struct and use a shared atomic bool.
        // Expose a method here that takes in an index/key and set visibility via lookup

        let (id_sender, id_receiver) = get_channel(1);
        let progress_message = ProgressMessage::Request {
            job: progress_job,
            id_return_sender: id_sender,
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

        // TODO: add an abort callback -> share an atomic boolean that can be cancelled from the
        // UI.
        // REMEMBER: The abort callback returns true if it -should- abort.
        let mut sync_downloader = sync_downloader.with_progress_callback(progress_callback);

        // NOTE: This is a blocking call.
        // NOTE TWICE: match on this and extract the DownloadAborted error if it was aborted
        //  -> clean up the (temporary) file ->>>> CHANGE THE DOWNLOAD API: extract the file-name
        //  from reqwest and store internally, expose a "temporary file_name" -> download to the
        //  temporary file_name first, match internally in the API to determine whether or not to
        //  re-name the temporary file to the finished one -> if it's aborted, just delete the
        //  temporary file.
        let download_path = sync_downloader.download(dest_dir.as_path())?;
        // TODO: determine whether or not to re-extract the file-name from the download path, or
        // just use the content_name in the ribble message

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
        // TODO: remove this logique; the model bank has a directory-watcher that updates
        // accordingly.
        if let Some(sender) = return_sender {
            if sender.send(file_name).is_err() {
                todo!("LOGGING");
            }
        }

        Ok(ribble_message)
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
                let download_inner = Arc::clone(&thread_inner);
                let start_download =
                    std::thread::spawn(move || download_inner.start_download(download_job));

                let work_request = WorkRequest::Short(start_download);
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

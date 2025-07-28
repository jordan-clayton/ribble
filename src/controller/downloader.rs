use crate::controller::{
    AmortizedDownloadProgress, Bus, ConsoleMessage, DownloadRequest, FileDownload, Progress,
    ProgressMessage, RibbleMessage, WorkRequest,
};
use crate::utils::errors::RibbleError;
use parking_lot::RwLock;
use ribble_whisper::downloader::downloaders::sync_download_request;
use ribble_whisper::downloader::SyncDownload;
use ribble_whisper::utils::callback::{RibbleAbortCallback, RibbleWhisperCallback};
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{get_channel, Receiver, Sender};
use slab::Slab;
use std::error::Error;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

const FALLBACK_NAME: &str = "invalid_download";

struct DownloadEngineState {
    // NOTE: if hashing is required, implment hash on FileDownload and use an IndexSet/ pre-hash
    // the content_name and use it as key in IndexMap.
    // Vectors might be a little fragile and Slab insert/remove is going to be way faster.
    file_downloads: RwLock<Slab<FileDownload>>,
    incoming_jobs: Receiver<DownloadRequest>,
    worker_sender: Sender<WorkRequest>,
    progress_sender: Sender<ProgressMessage>,
}

impl DownloadEngineState {
    fn new(incoming_jobs: Receiver<DownloadRequest>, bus: &Bus) -> Self {
        let file_downloads = RwLock::new(Slab::new());
        Self {
            file_downloads,
            incoming_jobs,
            worker_sender: bus.work_request_sender(),
            progress_sender: bus.progress_message_sender(),
        }
    }

    // NOTE: these just take in the resources by value -> they're owned by the DownloadRequest::DownloadJob
    // There's no real need to take it in via reference.
    fn run_download(&self, url: String, dest_dir: PathBuf) -> Result<RibbleMessage, RibbleError> {
        let sync_downloader = sync_download_request(&url, FALLBACK_NAME)?;
        let content_name = sync_downloader.content_name();
        if content_name == FALLBACK_NAME {
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
        let abort_download = Arc::new(AtomicBool::new(false));

        // Make a FileDownload to store in the slab -> for exposing in the UI.
        let file_download =
            FileDownload::new(content_name, progress_view, Arc::clone(&abort_download));

        // Place it in the bank of downloads -> the progress updated by the progress_callback is
        // shared, so the state will be accessible from the UI (outside progress-bars, that is).
        let download_id = self.file_downloads.write().insert(file_download);

        let (id_sender, id_receiver) = get_channel(1);
        let progress_message = ProgressMessage::Request {
            job: progress_job,
            id_return_sender: id_sender,
        };

        if let Err(e) = self.progress_sender.send(progress_message) {
            log::warn!(
                "Progress engine closed, cannot send initial download progress request.\nError source: {:#?}",
                e.source()
            );
        }

        let progress_id = match id_receiver.recv() {
            Ok(id) => Some(id),
            Err(e) => {
                log::warn!(
                    "Progress engine did not return an ID due to rendezvous fail.\nError source: {:#?}",
                    e.source()
                );
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

            if let Err(e) = callback_progress_sender.try_send(update) {
                log::warn!(
                    "Cannot send update request, channel either closed or too small.\nError: {}\nError source: {:#?}",
                    &e,
                    e.source()
                );
            }
        };

        let progress_callback = RibbleWhisperCallback::new(progress_closure);
        let abort_closure = move || abort_download.load(Ordering::Acquire);
        let abort_callback = RibbleAbortCallback::new(abort_closure);

        let mut sync_downloader = sync_downloader
            .with_progress_callback(progress_callback)
            .with_abort_callback(abort_callback);

        // NOTE: This is a blocking call; download the file.
        let download_path = sync_downloader.download(dest_dir.as_path());
        // Regardless of whether  it ends because of an error, the download needs to get
        // removed--there's no "resume"/"pause" because it's a blocking download, so there's no way
        // to expose that feature right now.
        // This may change at a later date, but for now it's good enough.

        // Try to remove the FileDownload struct -> since the download is done, this thread
        // shouldn't panic.
        // It -should- be impossible for this to panic, because the DownloadEngineState owns its
        // file_downloads; if it is gone, log the error to diagnose issues.
        if self
            .file_downloads
            .write()
            .try_remove(download_id)
            .is_none()
        {
            log::warn!("File download metadata missing after download.");
        };

        let download_path = download_path?;

        // Re-bind the content_name to avoid borrowing issues.
        let content_name = sync_downloader.content_name();

        // Remove the progress job now that the file's downloaded.
        if let Some(id) = progress_id {
            let finished = ProgressMessage::Remove { job_id: id };
            if let Err(e) = self.progress_sender.send(finished) {
                log::warn!(
                    "Progress engine closed, cannot send removal request.\nError source: {:#?}",
                    e.source()
                );
            }
        }

        // Print both the content name and the fully returned path in the Console message.
        let console_message = ConsoleMessage::Status(format!(
            "Successfully downloaded {content_name} to {}",
            download_path.display()
        ));

        let ribble_message = RibbleMessage::Console(console_message);

        Ok(ribble_message)
    }

    // Since the download is already happening on a thread, only the abort flag needs to be set;
    // The metadata will be removed automatically and does not need to be manually removed.
    // NOTE: this will still block at the moment -> it's tricky to coordinate another thread with
    // the download thread in place; if this becomes an issue, then look at spawning another
    // thread.
    fn abort_download(&self, download_id: usize) {
        let read_guard = self.file_downloads.read();
        if let Some(download) = read_guard.get(download_id) {
            download.abort_download();
        } else {
            log::warn!("Download metadata missing for id: {download_id}");
        }
    }
}

pub(super) struct DownloadEngine {
    inner: Arc<DownloadEngineState>,
    work_thread: Option<JoinHandle<()>>,
}

impl DownloadEngine {
    pub(super) fn new(incoming_jobs: Receiver<DownloadRequest>, bus: &Bus) -> Self {
        let inner = Arc::new(DownloadEngineState::new(incoming_jobs, bus));
        let thread_inner = Arc::clone(&inner);

        let worker = std::thread::spawn(move || {
            while let Ok(download_job) = thread_inner.incoming_jobs.recv() {
                match download_job {
                    DownloadRequest::DownloadJob { url, directory } => {
                        let download_inner = Arc::clone(&thread_inner);
                        let start_download =
                            std::thread::spawn(move || download_inner.run_download(url, directory));

                        let work_request = WorkRequest::Short(start_download);
                        if let Err(e) = thread_inner.worker_sender.send(work_request) {
                            log::warn!(
                        "Worker Engine closed. Can no longer send requests.\nError source: {:#?}",
                        e.source()
                    );
                            break;
                        }
                    }
                    DownloadRequest::Shutdown => break,
                }
            }
        });

        let work_thread = Some(worker);
        Self { inner, work_thread }
    }

    // FileDownload is a cheap clone (mostly copy); this should be harmlesss to call in the UI.
    pub(super) fn try_get_current_downloads(&self, copy_buffer: &mut Vec<(usize, FileDownload)>) {
        if let Some(guard) = self.inner.file_downloads.try_read() {
            copy_buffer.clear();
            // Filter out aborted downloads and return a cloned list.
            copy_buffer.extend(
                guard
                    .iter()
                    .filter(|(_, download)| !download.should_abort.load(Ordering::Acquire))
                    .map(|(key, val)| (key, val.clone())),
            );
        }
    }

    pub(super) fn try_get_amortized_download_progress(&self) -> Option<AmortizedDownloadProgress> {
        if let Some(jobs) = self.inner.file_downloads.try_read() {
            // This will coerce into NoJobs if the accumulator ends up (0, 0) (i.e. no jobs).
            let download_progress: AmortizedDownloadProgress = jobs
                .iter()
                .fold((0usize, 0usize), |(current, total), (_, file_download)| {
                    // If the download is aborted, don't accumulated it.
                    if file_download.should_abort.load(Ordering::Acquire) {
                        (current, total)
                    } else {
                        let progress = file_download.progress();
                        (
                            current + progress.current_position() as usize,
                            total + progress.total_size() as usize,
                        )
                    }
                })
                .into();
            Some(download_progress)
        } else {
            None
        }
    }

    // NOTE: Metadata removal is handled internally; this only sets the flag to stop the download.
    pub(super) fn abort_download(&self, download_id: usize) {
        self.inner.abort_download(download_id);
    }
}

impl Drop for DownloadEngine {
    fn drop(&mut self) {
        log::info!("Dropping DownloadEngine.");
        if let Some(handle) = self.work_thread.take() {
            log::info!("Joining DownloadEngine work thread.");
            handle
                .join()
                .expect("The DownloadEngine worker thread is not expected to ever panic.");
            log::info!("DownloadEngine work thread joined.");
        }
    }
}

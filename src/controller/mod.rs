use crate::utils::errors::RibbleError;
use atomic_enum::atomic_enum;
use egui::{RichText, Visuals};
use ribble_whisper::utils::Sender;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use strum::{AsRefStr, Display, EnumIter, EnumString};

// TODO: remove these excess utils; most of them are not necessary for the application.
pub(crate) mod utils;
pub(crate) mod whisper_app_controller;

mod console;
mod downloader;
mod kernel;
mod progress;
mod recorder;
mod transcriber;
mod visualizer;
mod worker;
mod writer;
pub(crate) mod ribble_controller;

type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, RibbleError>>;

const UTILITY_QUEUE_SIZE: usize = 32;
const SMALL_UTILITY_QUEUE_SIZE: usize = 16;
const UI_UPDATE_QUEUE_SIZE: usize = 8;
// TODO: determine whether or not this is necessary, whether it should be changed.
// Right now, there are no hard limits on how large this can get.
const DEFAULT_PROGRESS_SLAB_CAPACITY: usize = 8;

// CONSOLE CONSTANTS
pub const DEFAULT_NUM_CONSOLE_MESSAGES: usize = 32;
pub const MIN_NUM_CONSOLE_MESSAGES: usize = 16;
pub const MAX_NUM_CONSOLE_MESSAGES: usize = 64;

pub(crate) enum RibbleMessage {
    Console(ConsoleMessage),
    BackgroundWork(Result<(), RibbleError>),
}

struct Bus {
    console_message_sender: Sender<ConsoleMessage>,
    // TODO: this is not correct yet -> this will need a ProgressMessage or similar.
    // It will also most likely require a hashmap instead of a slab because IDs are lost across the
    // channel.
    progress_message_sender: Sender<ProgressMessage>,
    work_request_sender: Sender<WorkRequest>,
    write_request_sender: Sender<writer::WriteRequest>,
    // TODO: future thing -> possibly stick this in a data structure with the sample rate.
    // Pre-computing and re-initializing the FFT thingy might get a little sticky.
    visualizer_sample_sender: Sender<visualizer::VisualizerSample>,
    download_request_sender: Sender<DownloadRequest>,
}

impl Bus {
    fn new(
        console_message_sender: Sender<ConsoleMessage>,
        progress_message_sender: Sender<ProgressMessage>,
        work_request_sender: Sender<WorkRequest>,
        write_request_sender: Sender<writer::WriteRequest>,
        visualizer_sample_sender: Sender<visualizer::VisualizerSample>,
        download_request_sender: Sender<DownloadRequest>,
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

    fn console_message_sender(&self) -> Sender<ConsoleMessage> {
        self.console_message_sender.clone()
    }

    fn progress_message_sender(&self) -> Sender<ProgressMessage> {
        self.progress_message_sender.clone()
    }
    fn write_request_sender(&self) -> Sender<writer::WriteRequest> {
        self.write_request_sender.clone()
    }
    fn work_request_sender(&self) -> Sender<WorkRequest> {
        self.work_request_sender.clone()
    }
    fn visualizer_sample_sender(&self) -> Sender<visualizer::VisualizerSample> {
        self.visualizer_sample_sender.clone()
    }
    fn download_request_sender(&self) -> Sender<DownloadRequest> {
        self.download_request_sender.clone()
    }
}

// Minimal: progress bar only (fastest)
// Progressive: progress bar + snapshotting when new segments are decoded.
#[atomic_enum]
#[repr(C)]
#[derive(Default, PartialEq, Eq, EnumIter, EnumString, AsRefStr)]
pub(crate) enum OfflineTranscriberFeedback {
    #[default]
    Minimal = 0,
    Progressive,
}

#[derive(Debug, Display)]
pub(crate) enum ConsoleMessage {
    Error(RibbleError),
    Status(String),
}

impl ConsoleMessage {
    // NOTE TO SELF: call ui.label(msg.to_console_text(&visuals)) in the console tab when drawing
    pub(crate) fn to_console_text(&self, visuals: &Visuals) -> RichText {
        let (color, msg) = match self {
            ConsoleMessage::Error(msg) => (visuals.error_fg_color, msg.to_string()),
            ConsoleMessage::Status(msg) => (visuals.text_color(), msg.to_owned()),
        };
        // This has to make at least 1 heap allocation to coerce into a string
        // Test, but expect this to just move the string created above.
        RichText::new(msg).color(color).monospace()
    }
}

// There is no functional difference between members of this enum (at the moment).
// Right now, it's just semantic & the long_queue is twice the size.
enum WorkRequest {
    Short(RibbleWorkerHandle),
    Long(RibbleWorkerHandle),
}

struct DownloadRequest {
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


impl DownloadRequest {
    fn new() -> Self {
        Self {
            url: Default::default(),
            file_name: Default::default(),
            directory: Default::default(),
            return_sender: None,
        }
    }

    fn decompose(self) -> (String, String, PathBuf, Option<Sender<String>>) {
        (self.url, self.file_name, self.directory, self.return_sender)
    }

    fn with_url(mut self, url: String) -> Self {
        self.url = url;
        self
    }
    fn with_file_name(mut self, file_name: String) -> Self {
        self.file_name = file_name;
        self
    }
    fn with_directory(mut self, directory: &Path) -> Self {
        self.directory = directory.to_path_buf();
        self
    }
    fn with_return_sender(mut self, sender: Sender<String>) -> Self {
        self.return_sender = Some(sender);
        self
    }

    fn url(&self) -> &String {
        &self.url
    }
    fn file_name(&self) -> &String {
        &self.file_name
    }
    fn directory(&self) -> &Path {
        self.directory.as_path()
    }
}

enum ProgressMessage {
    Request {
        job: Progress,
        id_return_sender: Sender<usize>,
    },

    Increment {
        job_id: usize,
        delta: u64,
    },
    Decrement {
        job_id: usize,
        delta: u64,
    },
    Set {
        job_id: usize,
        pos: u64,
    },
    Reset {
        job_id: usize,
    },
    Remove {
        job_id: usize,
    },
}

#[derive(Debug)]
struct AtomicProgress {
    pos: AtomicU64,
    capacity: AtomicU64,
}

impl AtomicProgress {
    fn new() -> Self {
        Self {
            pos: AtomicU64::new(0),
            capacity: AtomicU64::new(0),
        }
    }
    fn with_capacity(self, capacity: u64) -> Self {
        self.capacity.store(capacity, Ordering::Release);
        self
    }

    // TODO: remove if unused.
    fn set(&self, pos: u64) {
        self.pos.store(pos, Ordering::Release);
    }
    fn inc(&self, delta: u64) {
        self.pos.fetch_add(delta, Ordering::Release);
    }
    fn dec(&self, delta: u64) {
        self.pos.fetch_sub(delta, Ordering::Release);
    }

    fn reset(&self) {
        self.pos.store(0, Ordering::Release);
    }
    // Progress in the range [0, 1] where 1 means 100% completion
    fn normalized(&self) -> f32 {
        (self.pos.load(Ordering::Acquire) as f64 / self.capacity.load(Ordering::Acquire) as f64)
            as f32
    }
    fn is_finished(&self) -> bool {
        self.pos.load(Ordering::Acquire) == self.capacity.load(Ordering::Acquire)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum Progress {
    Determinate {
        job_name: &'static str,
        progress: Arc<AtomicProgress>,
    },
    Indeterminate {
        job_name: &'static str,
    },
}

impl Progress {
    pub(crate) fn new_indeterminate(job_name: &'static str) -> Self {
        Self::Indeterminate { job_name }
    }
    pub(crate) fn new_determinate(job_name: &'static str, total_size: u64) -> Self {
        let progress = AtomicProgress::new().with_capacity(total_size);
        let progress = Arc::new(progress);
        Self::Determinate { job_name, progress }
    }

    pub(crate) fn inc(&self, delta: u64) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.inc(delta);
        }
    }
    pub(crate) fn dec(&self, delta: u64) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.dec(delta);
        }
    }
    pub(crate) fn set(&self, pos: u64) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.set(pos);
        }
    }
    pub(crate) fn reset(&self) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.reset();
        }
    }

    pub(crate) fn progress(&self) -> f32 {
        match self {
            Progress::Determinate {
                job_name: _,
                progress,
            } => progress.normalized(),
            Progress::Indeterminate { .. } => 1.0,
        }
    }

    // TODO: remove if never called
    pub(crate) fn is_finished(&self) -> bool {
        match self {
            Progress::Determinate {
                job_name: _,
                progress,
            } => progress.is_finished(),
            Progress::Indeterminate { .. } => false,
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct CompletedRecordingJobs {
    // This can probably just be accumulated.
    file_size_estimate: usize,
    total_duration: Duration,
    channels: usize,
    sample_rate: usize,
}

impl CompletedRecordingJobs {
    pub(crate) fn new(file_size_estimate: usize, total_duration: Duration, channels: usize, sample_rate: usize) -> Self {
        Self { file_size_estimate, total_duration, channels, sample_rate }
    }

    pub(crate) fn file_size_estimate(&self) -> usize {
        self.file_size_estimate
    }
    pub(crate) fn total_duration(&self) -> Duration {
        self.total_duration
    }
    pub(crate) fn channels(&self) -> usize {
        self.channels
    }
    pub(crate) fn sample_rate(&self) -> usize {
        self.sample_rate
    }
}
use crate::utils::errors::RibbleError;
use crate::utils::recorder_configs::RibbleRecordingConfigs;
use atomic_enum::atomic_enum;
use egui::{RichText, Visuals};
use ribble_whisper::utils::{Receiver, Sender};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};

pub mod audio_backend_proxy;
pub mod console;
pub mod downloader;
pub mod kernel;
pub mod model_bank;
pub mod progress;
pub mod recorder;
pub mod ribble_controller;
pub mod transcriber;
pub mod visualizer;

pub mod worker;
pub mod writer;

// TODO: perhaps make this a "resolution" parameter.
// It's also more than likely fine to double this, if not quadruple.
// TODO: test performance with higher resolutions.
pub const NUM_VISUALIZER_BUCKETS: usize = 32;

pub const UTILITY_QUEUE_SIZE: usize = 32;

pub const SMALL_UTILITY_QUEUE_SIZE: usize = 16;
pub const UI_UPDATE_QUEUE_SIZE: usize = 8;
// TODO: determine whether or not this is necessary, whether it should be changed.
// Right now, there are no hard limits on how large this can get.
const DEFAULT_PROGRESS_SLAB_CAPACITY: usize = 8;
// CONSOLE CONSTANTS
pub const DEFAULT_NUM_CONSOLE_MESSAGES: usize = 32;

pub const MIN_NUM_CONSOLE_MESSAGES: usize = 16;
pub const MAX_NUM_CONSOLE_MESSAGES: usize = 64;
type RibbleWorkerHandle = JoinHandle<Result<RibbleMessage, RibbleError>>;

pub enum RibbleMessage {
    Console(ConsoleMessage),
    BackgroundWork(Result<(), RibbleError>),
}

pub struct Bus {
    console_message_sender: Sender<ConsoleMessage>,
    progress_message_sender: Sender<ProgressMessage>,
    work_request_sender: Sender<WorkRequest>,
    write_request_sender: Sender<WriteRequest>,
    // TODO: future thing -> possibly stick this in a data structure with the sample rate.
    // Pre-computing and re-initializing the FFT thingy might get a little sticky.
    visualizer_sample_sender: Sender<VisualizerPacket>,
    download_request_sender: Sender<DownloadRequest>,
}

impl Bus {
    fn new(
        console_message_sender: Sender<ConsoleMessage>,
        progress_message_sender: Sender<ProgressMessage>,
        work_request_sender: Sender<WorkRequest>,
        write_request_sender: Sender<WriteRequest>,
        visualizer_sample_sender: Sender<VisualizerPacket>,
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
    fn write_request_sender(&self) -> Sender<WriteRequest> {
        self.write_request_sender.clone()
    }
    fn work_request_sender(&self) -> Sender<WorkRequest> {
        self.work_request_sender.clone()
    }
    fn visualizer_sample_sender(&self) -> Sender<VisualizerPacket> {
        self.visualizer_sample_sender.clone()
    }
    fn download_request_sender(&self) -> Sender<DownloadRequest> {
        self.download_request_sender.clone()
    }

    // NOTE: this needs to be explicitly called (by the kernel or authoritative owner),
    // The kernel will not be able to drop its own engines if they're stuck waiting on queues
    // Since this is a shared bus, it's unknown if the senders/receivers are dropped.
    fn try_close_bus(&self) {
        let Bus {
            console_message_sender,
            progress_message_sender,
            work_request_sender,
            write_request_sender,
            visualizer_sample_sender,
            download_request_sender,
        } = self;

        Self::fire_sentinel_message(console_message_sender, ConsoleMessage::Shutdown);
        Self::fire_sentinel_message(progress_message_sender, ProgressMessage::Shutdown);
        Self::fire_sentinel_message(work_request_sender, WorkRequest::Shutdown);
        Self::fire_sentinel_message(write_request_sender, WriteRequest::Shutdown);
        Self::fire_sentinel_message(visualizer_sample_sender, VisualizerPacket::Shutdown);
        Self::fire_sentinel_message(download_request_sender, DownloadRequest::Shutdown);
    }

    fn fire_sentinel_message<T: Send>(sender: &Sender<T>, msg: T) {
        // NOTE: this could deadlock if the queues aren't large enough.
        if let Err(e) = sender.send(msg) {
            log::warn!(
                "Failed to send sentinel message due to receiver drop.\n\
           Error source: {:#?}",
                e.source()
            );
        }
    }
}

// Minimal: progress bar only (fastest)
// Progressive: progress bar + snapshotting when new segments are decoded.
#[atomic_enum]
#[repr(C)]
#[derive(
    Default, PartialEq, Eq, EnumIter, EnumString, AsRefStr, serde::Serialize, serde::Deserialize,
)]
pub enum OfflineTranscriberFeedback {
    #[default]
    Minimal = 0,
    Progressive,
}

#[derive(Debug, Display)]
pub enum ConsoleMessage {
    Error(RibbleError),
    Status(String),
    Shutdown,
}

impl ConsoleMessage {
    // NOTE TO SELF: call ui.label(msg.to_console_text(&visuals)) in the console tab when drawing
    pub fn to_console_text(&self, visuals: &Visuals) -> RichText {
        let (color, msg) = match self {
            ConsoleMessage::Error(msg) => (visuals.error_fg_color, msg.to_string()),
            ConsoleMessage::Status(msg) => (visuals.text_color(), msg.to_owned()),
            ConsoleMessage::Shutdown => (visuals.text_color(), "Shutting down.".to_owned()),
        };
        // This has to make at least 1 heap allocation to coerce into a string
        // Test, but expect this to just move the string created above.
        RichText::new(msg).color(color).monospace()
    }
}

enum RibbleWork {
    Work(RibbleWorkerHandle),
    Sentinel,
}
// There is no functional difference between members of this enum (at the moment).
// Right now, it's just semantic & the long_queue is twice the size.
enum WorkRequest {
    Short(RibbleWorkerHandle),
    Long(RibbleWorkerHandle),
    Shutdown,
}

// TODO: use this for presenting in the UI.
// It has everything needed for viewing
// TODO: think about how/where to add the "abort"
// The UI and the DownloadEngine both interop here.
#[derive(Clone, Debug)]
pub struct FileDownload {
    name: Arc<str>,
    progress: ProgressView,
    should_abort: Arc<AtomicBool>,
}

impl FileDownload {
    fn new(name: &str, progress: ProgressView, should_abort: Arc<AtomicBool>) -> Self {
        Self {
            name: Arc::from(name),
            progress,
            should_abort,
        }
    }

    pub fn name(&self) -> Arc<str> {
        Arc::clone(&self.name)
    }
    pub fn progress(&self) -> ProgressView {
        self.progress.clone()
    }

    fn abort_download(&self) {
        self.should_abort.store(true, Ordering::Release);
    }
}

// NOTE: if it somehow becomes necessary to send information (e.g. the returned PathBuf) back from the DownloadRequest to the
// requester, then use a queue.
enum DownloadRequest {
    DownloadJob {
        url: String,
        directory: PathBuf,
    },
    Shutdown,
}

impl DownloadRequest {
    fn new_job(url: &str, directory: &Path) -> Self {
        Self::DownloadJob {
            url: url.to_string(),
            directory: directory.to_path_buf(),
        }
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
    Shutdown,
}

#[derive(Debug)]
pub struct AtomicProgress {
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

    fn current_position(&self) -> u64 {
        self.pos.load(Ordering::Acquire)
    }
    fn total_size(&self) -> u64 {
        self.capacity.load(Ordering::Acquire)
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

pub enum AmortizedProgress {
    NoJobs,
    Determinate { current: usize, total_size: usize },
    Indeterminate,
}

#[derive(Copy, Clone, Default)]
pub enum AmortizedDownloadProgress {
    #[default]
    NoJobs,
    Total {
        current: usize,
        total_size: usize,
    },
}
impl AmortizedDownloadProgress {
    pub fn decompose(self) -> Option<(usize, usize)> {
        match self {
            AmortizedDownloadProgress::Total {
                current,
                total_size,
            } => Some((current, total_size)),
            AmortizedDownloadProgress::NoJobs => None,
        }
    }
}

impl From<(usize, usize)> for AmortizedDownloadProgress {
    fn from(value: (usize, usize)) -> Self {
        match value {
            (0, 0) => AmortizedDownloadProgress::NoJobs,
            (current, total_size) => AmortizedDownloadProgress::Total {
                current,
                total_size,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProgressView {
    inner: Arc<AtomicProgress>,
}

impl ProgressView {
    pub fn new(progress: Arc<AtomicProgress>) -> Self {
        Self { inner: progress }
    }

    // Returns the progress, normalized between 0 and 1
    pub fn current_progress(&self) -> f32 {
        self.inner.normalized()
    }

    pub fn current_position(&self) -> u64 {
        self.inner.current_position()
    }
    pub fn total_size(&self) -> u64 {
        self.inner.total_size()
    }
    pub fn is_finished(&self) -> bool {
        self.inner.is_finished()
    }
}

#[derive(Debug, Clone)]
pub enum Progress {
    Determinate {
        job_name: &'static str,
        progress: Arc<AtomicProgress>,
    },
    Indeterminate {
        job_name: &'static str,
    },
}

impl Progress {
    fn new_indeterminate(job_name: &'static str) -> Self {
        Self::Indeterminate { job_name }
    }
    fn new_determinate(job_name: &'static str, total_size: u64) -> Self {
        let progress = AtomicProgress::new().with_capacity(total_size);
        let progress = Arc::new(progress);
        Self::Determinate { job_name, progress }
    }

    pub fn job_name(&self) -> &'static str {
        match self {
            Progress::Determinate {
                job_name,
                progress: _,
            }
            | Progress::Indeterminate { job_name } => job_name,
        }
    }

    fn inc(&self, delta: u64) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.inc(delta);
        }
    }
    fn dec(&self, delta: u64) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.dec(delta);
        }
    }
    fn set(&self, pos: u64) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.set(pos);
        }
    }
    fn reset(&self) {
        if let Self::Determinate {
            job_name: _,
            progress,
        } = self
        {
            progress.reset();
        }
    }

    pub fn current_progress(&self) -> Option<usize> {
        match self {
            Progress::Determinate { progress, .. } => Some(progress.current_position() as usize),
            Progress::Indeterminate { .. } => None,
        }
    }
    pub fn total_size(&self) -> Option<usize> {
        match self {
            Progress::Determinate { progress, .. } => Some(progress.total_size() as usize),
            Progress::Indeterminate { .. } => None,
        }
    }

    pub fn progress(&self) -> Option<f32> {
        match self {
            Progress::Determinate {
                job_name: _,
                progress,
            } => Some(progress.normalized()),
            Progress::Indeterminate { .. } => None,
        }
    }

    // TODO: remove if never called
    pub fn is_finished(&self) -> bool {
        match self {
            Progress::Determinate {
                job_name: _,
                progress,
            } => progress.is_finished(),
            Progress::Indeterminate { .. } => false,
        }
    }

    pub fn progress_view(&self) -> Option<ProgressView> {
        match self {
            Progress::Determinate {
                job_name: _,
                progress,
            } => Some(ProgressView::new(Arc::clone(progress))),
            Progress::Indeterminate { .. } => None,
        }
    }
}

#[derive(Copy, Clone)]
pub struct CompletedRecordingJobs {
    // This can probably just be accumulated.
    file_size_estimate: usize,
    total_duration: Duration,
    channels: usize,
    sample_rate: usize,
}

impl CompletedRecordingJobs {
    pub fn new(
        file_size_estimate: usize,
        total_duration: Duration,
        channels: usize,
        sample_rate: usize,
    ) -> Self {
        Self {
            file_size_estimate,
            total_duration,
            channels,
            sample_rate,
        }
    }

    pub fn file_size_estimate(&self) -> usize {
        self.file_size_estimate
    }
    pub fn total_duration(&self) -> Duration {
        self.total_duration
    }
    pub fn channels(&self) -> usize {
        self.channels
    }
    pub fn sample_rate(&self) -> usize {
        self.sample_rate
    }
}

pub enum RotationDirection {
    Clockwise,
    CounterClockwise,
}

#[atomic_enum]
#[derive(Default, PartialEq, EnumIter, Display, IntoStaticStr, AsRefStr)]
pub enum AnalysisType {
    #[strum(to_string = "Amplitude")]
    #[default]
    AmplitudeEnvelope = 0,
    Waveform,
    Power,
    #[strum(to_string = "Spectrum Density")]
    SpectrumDensity,
}

impl AnalysisType {
    // NOTE: this is obviously a little un-maintainable and not the greatest solution if the AnalysisTypes grow.
    // If it becomes untenable, look into a macro-based solution.
    // TODO: write a quick test to stamp out bugs here
    pub fn rotate(&self, direction: RotationDirection) -> Self {
        match (self, direction) {
            (AnalysisType::AmplitudeEnvelope, RotationDirection::Clockwise) => {
                AnalysisType::Waveform
            }
            (AnalysisType::AmplitudeEnvelope, RotationDirection::CounterClockwise) => {
                AnalysisType::SpectrumDensity
            }
            (AnalysisType::Waveform, RotationDirection::Clockwise) => AnalysisType::Power,
            (AnalysisType::Waveform, RotationDirection::CounterClockwise) => {
                AnalysisType::AmplitudeEnvelope
            }
            (AnalysisType::Power, RotationDirection::Clockwise) => AnalysisType::SpectrumDensity,
            (AnalysisType::Power, RotationDirection::CounterClockwise) => AnalysisType::Waveform,
            (AnalysisType::SpectrumDensity, RotationDirection::Clockwise) => {
                AnalysisType::AmplitudeEnvelope
            }
            (AnalysisType::SpectrumDensity, RotationDirection::CounterClockwise) => {
                AnalysisType::Power
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum ModelFile {
    Packed(usize),
    File(Arc<str>),
}

impl ModelFile {
    pub const PACKED_NAMES: [&'static str; 2] = ["ggml-tiny.q0.bin", "ggml-base.q0.bin"];
}

impl Display for ModelFile {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelFile::Packed(id) => {
                write!(f, "ModelFile::Packed: {}", Self::PACKED_NAMES[*id])
            }
            ModelFile::File(name) => {
                write!(f, "ModelFile::File: {name}")
            }
        }
    }
}

pub(in crate::controller) enum WriteRequest {
    WriteJob {
        receiver: Receiver<Arc<[f32]>>,
        spec: RibbleRecordingConfigs,
    },
    Shutdown,
}

impl WriteRequest {
    pub(in crate::controller) fn new_job(
        receiver: Receiver<Arc<[f32]>>,
        spec: RibbleRecordingConfigs,
    ) -> Self {
        Self::WriteJob { receiver, spec }
    }

    pub(in crate::controller) fn unpack(
        self,
    ) -> Option<(Receiver<Arc<[f32]>>, RibbleRecordingConfigs)> {
        match self {
            WriteRequest::WriteJob { receiver, spec } => Some((receiver, spec)),
            WriteRequest::Shutdown => None,
        }
    }
}

pub enum VisualizerPacket {
    VisualizerSample {
        sample: Arc<[f32]>,
        sample_rate: f64,
    },
    Shutdown,
}

impl VisualizerPacket {
    pub(in crate::controller) fn new(sample: Arc<[f32]>, sample_rate: f64) -> Self {
        Self::VisualizerSample {
            sample,
            sample_rate,
        }
    }
}
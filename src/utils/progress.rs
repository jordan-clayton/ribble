use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

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
        (self.pos.load(Ordering::Acquire) as f64 / self.capacity.load(Ordering::Acquire) as f64) as f32
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
    pub(crate) fn indeterminate(job_name: &'static str) -> Self {
        Self::Indeterminate {job_name}
    }
    pub(crate) fn determinate(job_name: &'static str, total_size: u64) -> Self{
        let progress = AtomicProgress::new().with_capacity(total_size);
        let progress = Arc::new(progress);
        Self::Determinate {job_name, progress}
    }

    pub(crate) fn inc(&self, delta: u64) {
        if let Self::Determinate {job_name: _, progress} = self{
            progress.inc(delta);
        }
    }
    pub(crate) fn dec(&self, delta: u64) {
        if let Self::Determinate {job_name: _, progress} = self{
            progress.dec(delta);
        }
    }
    pub(crate) fn set(&self, pos: u64){
        if let Self::Determinate {job_name: _, progress} = self {
            progress.set(pos);
        }
    }
    pub(crate) fn reset(&self){
        if let Self::Determinate {job_name: _, progress} = self{
            progress.reset();
        }
    }

    pub(crate) fn progress(&self) -> f32{
        match self{
            Progress::Determinate { job_name: _, progress } => {progress.normalized()}
            Progress::Indeterminate { .. } => {1.0}
        }
    }

    // TODO: remove if never called
    pub(crate) fn is_finished(&self) -> bool{
        match self{
            Progress::Determinate { job_name: _, progress } => {progress.is_finished()}
            Progress::Indeterminate { .. } => {false}
        }
    }
}

// TODO: this likely doesn't need to be hashable.
// It should be thread-safe though.
#[derive(Debug, Clone)]
pub struct ProgressBar {
    job_name: String,
    // TODO: move to AtomicUsize and encapsulate within an arc
    progress: usize,
    // TODO: move to AtomicUsize
    total_size: usize,
}

impl ProgressBar {
    pub fn new(job_name: String, progress: usize, total_size: usize) -> Self {
        Self {
            job_name,
            progress,
            total_size,
        }
    }
    pub fn finished(&self) -> bool {
        self.progress == self.total_size
    }
    pub fn progress(&self) -> usize {
        self.progress
    }

    pub fn total_size(&self) -> usize {
        self.total_size
    }

    pub fn job_name(&self) -> &String {
        &self.job_name
    }
}

impl Hash for ProgressBar {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.job_name.hash(state);
    }
}

impl PartialEq for ProgressBar {
    fn eq(&self, other: &Self) -> bool {
        self.job_name == other.job_name
    }
}

impl Eq for ProgressBar {}

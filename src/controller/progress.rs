use parking_lot::RwLock;
use ribble_whisper::utils::Sender;
use slab::Slab;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) enum ProgressMessage {
    Request {
        job: Progress,
        source: Sender<usize>,
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

// TODO: migrate to an inner state struct and use RAII bg threads to get new messages.
pub(super) struct ProgressEngine {
    current_jobs: Arc<RwLock<Slab<Progress>>>,
}

impl ProgressEngine {
    // Require capacity at construction time.
    // This will dynamically resize according to its needs
    // It's fine to send 0 as an initial capacity; it'll just suffer some initial allocation overhead.
    pub(super) fn new(initial_capacity: usize) -> Self {
        let slab = Slab::with_capacity(initial_capacity);
        let current_jobs = Arc::new(RwLock::new(slab));
        Self { current_jobs }
    }

    pub(super) fn add_progress_job(&self, job: Progress) -> usize {
        self.current_jobs.write().insert(job)
    }
    pub(super) fn update_progress_job(&self, id: usize, delta: u64) {
        if let Some(progress) = self.current_jobs.write().get(id) {
            progress.inc(delta);
        }
    }
    pub(super) fn remove_progress_job(&self, id: usize) {
        self.current_jobs.write().remove(id);
    }
    pub(super) fn try_get_current_jobs(&self, copy_buffer: &mut Vec<Progress>) {
        if let Some(jobs) = self.current_jobs.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(jobs.iter().cloned())
        }
    }
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

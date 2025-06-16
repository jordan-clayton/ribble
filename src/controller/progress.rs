use crate::utils::progress::Progress;
use parking_lot::RwLock;
use slab::Slab;
use std::sync::Arc;

// NOTE: if the kernel is required here, migrate to an Arc<inner> state struct
pub struct ProgressEngine {
    current_jobs: Arc<RwLock<Slab<Progress>>>,
}

impl ProgressEngine {
    // Require capacity at construction time.
    // This will dynamically resize according to its needs
    // It's fine to send 0 as an initial capacity; it'll just suffer some initial allocation overhead.
    pub(crate) fn new(initial_capacity: usize) -> Self {
        let slab = Slab::with_capacity(initial_capacity);
        let current_jobs = Arc::new(RwLock::new(slab));
        Self { current_jobs }
    }

    pub(crate) fn add_progress_job(&self, job: Progress) -> usize {
        self.current_jobs.write().insert(job)
    }
    pub(crate) fn update_progress_job(&self, id: usize, delta: u64) {
        if let Some(progress) = self.current_jobs.write().get(id) {
            progress.inc(delta);
        }
    }
    pub(crate) fn remove_progress_job(&self, id: usize) {
        self.current_jobs.write().remove(id);
    }
    pub(crate) fn get_current_jobs(&self, copy_buffer: &mut Vec<Progress>) {
        if let Some(jobs) = self.current_jobs.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(jobs.iter().cloned())
        }
    }
}

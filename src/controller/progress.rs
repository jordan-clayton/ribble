use crate::controller::{AmortizedProgress, Progress, ProgressMessage};
use crate::utils::errors::RibbleError;
use parking_lot::RwLock;
use ribble_whisper::utils::Receiver;
use slab::Slab;
use std::sync::Arc;
use std::thread::JoinHandle;

struct ProgressEngineState {
    current_jobs: RwLock<Slab<Progress>>,
    incoming_progress_messages: Receiver<ProgressMessage>,
}

impl ProgressEngineState {
    fn new(initial_capacity: usize, message_receiver: Receiver<ProgressMessage>) -> Self {
        let slab = Slab::with_capacity(initial_capacity);
        let current_jobs = RwLock::new(slab);
        Self {
            current_jobs,
            incoming_progress_messages: message_receiver,
        }
    }

    fn add_progress_job(&self, job: Progress) -> usize {
        self.current_jobs.write().insert(job)
    }
    fn increment_progress_job(&self, id: usize, delta: u64) {
        if let Some(progress) = self.current_jobs.read().get(id) {
            progress.inc(delta);
        }
    }
    fn decrement_progress_job(&self, id: usize, delta: u64) {
        if let Some(progress) = self.current_jobs.read().get(id) {
            progress.dec(delta);
        }
    }

    fn set_progress_job_position(&self, id: usize, pos: u64) {
        if let Some(progress) = self.current_jobs.read().get(id) {
            progress.set(pos);
        }
    }
    fn reset_progress_job(&self, id: usize) {
        if let Some(progress) = self.current_jobs.read().get(id) {
            progress.reset();
        }
    }
    fn remove_progress_job(&self, id: usize) {
        if self.current_jobs.write().try_remove(id).is_none() {
            todo!("LOGGING: This should never, ever be none unless there's a stale ID.");
        }
    }
}

pub(super) struct ProgressEngine {
    inner: Arc<ProgressEngineState>,
    work_thread: Option<JoinHandle<Result<(), RibbleError>>>,
}

impl ProgressEngine {
    // Require capacity at construction time.
    // This will dynamically resize according to its needs
    // It's fine to send 0 as an initial capacity; it'll just suffer some initial allocation overhead.
    pub(super) fn new(
        initial_capacity: usize,
        message_receiver: Receiver<ProgressMessage>,
    ) -> Self {
        let inner = Arc::new(ProgressEngineState::new(initial_capacity, message_receiver));
        let thread_inner = Arc::clone(&inner);
        let worker = std::thread::spawn(move || {
            while let Ok(message) = thread_inner.incoming_progress_messages.recv() {
                match message {
                    ProgressMessage::Request {
                        job,
                        id_return_sender,
                    } => {
                        let id = thread_inner.add_progress_job(job);
                        if id_return_sender.send(id).is_err() {
                            todo!("LOGGING");
                        }
                    }
                    ProgressMessage::Increment { job_id, delta } => {
                        thread_inner.increment_progress_job(job_id, delta);
                    }
                    ProgressMessage::Decrement { job_id, delta } => {
                        thread_inner.decrement_progress_job(job_id, delta);
                    }
                    ProgressMessage::Set { job_id, pos } => {
                        thread_inner.set_progress_job_position(job_id, pos);
                    }
                    ProgressMessage::Reset { job_id } => {
                        thread_inner.reset_progress_job(job_id);
                    }
                    ProgressMessage::Remove { job_id } => {
                        thread_inner.remove_progress_job(job_id);
                    }
                }
            }
            Ok(())
        });

        let work_thread = Some(worker);
        Self { inner, work_thread }
    }

    pub(super) fn try_get_current_jobs(&self, copy_buffer: &mut Vec<Progress>) {
        if let Some(jobs) = self.inner.current_jobs.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(jobs.iter().map(|(_, progress)| progress.clone()))
        }
    }

    pub(super) fn try_get_amortized_progress(&self) -> Option<AmortizedProgress> {
        if let Some(jobs) = self.inner.current_jobs.try_read() {
            if jobs.is_empty() {
                Some(AmortizedProgress::NoJobs)
            } else {
                let mut current = 0usize;
                let mut total_size = 0usize;
                for (_, job) in jobs.iter() {
                    if let Progress::Determinate {
                        job_name: _,
                        progress,
                    } = job
                    {
                        current += progress.current_position() as usize;
                        total_size += progress.total_size() as usize;
                    } else {
                        return Some(AmortizedProgress::Indeterminate);
                    }
                }

                Some(AmortizedProgress::Determinate {
                    current,
                    total_size,
                })
            }
        } else {
            None
        }
    }
}

impl Drop for ProgressEngine {
    fn drop(&mut self) {
        // TODO: determine whether or not to just have a void JoinHandle.
        if let Some(thread) = self.work_thread.take() {
            thread.join()
                .expect("The progress worker thread is expected to never panic.").expect(
                "I'm not quite sure what error conditions might ever happen with the thread.--This is being determined"
            );
        }
    }
}

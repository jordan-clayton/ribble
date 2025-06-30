// Basic idea: spawn a thread on construction time that waits for new requests for writing.
// Upon one, send a worker job (via message queues once the kernel stuff is refactored out).
// Store a limited number of temporary file recordings (keep an accumulator modulo num recordings).
use crate::controller::RibbleWorkerHandle;
use crate::utils::errors::RibbleAppError;
use crate::utils::recorder_configs::RibbleRecordingFormat;
use hound::{SampleFormat, WavSpec, WavWriter};
use parking_lot::RwLock;
use ribble_whisper::utils::get_channel;
use ribble_whisper::utils::{Receiver, Sender};
use std::collections::VecDeque;
use std::fs::File;
use std::io::BufWriter;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub(super) type WriteRequest = Receiver<f32>;

struct WriteJobs {
    completed: bool,
    writer: WavWriter<BufWriter<File>>,
    // Try-Wait on this -> if none have data, possibly accumulate and decide whether to sleep.
    // Perhaps this is actually better to just send 1-at-a-time - It's probably fine to have this
    // handled by the WorkerEngine.
    receiver: Receiver<Arc<[f32]>>,
    // When a job is first entered in, record the time so that it can be diffed on finish.
    start_time: Instant,
}

#[derive(Copy, Clone)]
struct CompletedJobs {
    file_size: usize,
    total_duration: Duration,
}

struct WriterEngineState {
    // These should have 1:1 index mappings.
    write_jobs: RwLock<VecDeque<WriteJobs>>,
    completed_jobs: RwLock<VecDeque<CompletedJobs>>,
    incoming_jobs: Receiver<WriteRequest>,
    worker_engine_channel: Sender<RibbleWorkerHandle>,
}

impl WriterEngineState {
    const MAX_JOBS: usize = 5;
    const TMP_FILE: &str = "tmp_recording";
    fn new(
        incoming_jobs: Receiver<WriteRequest>,
        worker_engine_channel: Sender<RibbleWorkerHandle>,
    ) -> Self {
        let write_jobs = RwLock::new(VecDeque::with_capacity(Self::MAX_JOBS));
        let completed_jobs = RwLock::new(VecDeque::with_capacity(Self::MAX_JOBS));
        Self {
            write_jobs,
            completed_jobs,
            incoming_jobs,
            worker_engine_channel,
        }
    }
}

pub(super) struct WriterEngine {
    // TODO: The method for export should accept the RibbleRecordingFormat, don't store it in the
    // job.
    inner: Arc<WriterEngineState>,
    request_polling_thread: Option<JoinHandle<Result<(), RibbleAppError>>>,
}

impl WriterEngine {
    fn new(
        incoming_jobs: Receiver<WriteRequest>,
        worker_engine_channel: Sender<RibbleWorkerHandle>,
    ) -> Self {
        let inner = Arc::new(WriterEngineState::new(incoming_jobs, worker_engine_channel));
        // TODO: polling thread logic -> borrow it from the TranscriberEngine
        Self {
            inner,
            request_polling_thread: None,
        }
    }
    pub(super) fn export(&self, _output_format: RibbleRecordingFormat) {
        todo!("Finish the export function.")
    }
    pub(super) fn try_get_completed_jobs(&self, copy_buffer: &mut Vec<CompletedJobs>) {
        if let Some(jobs) = self.inner.completed_jobs.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(jobs.iter().copied());
        }
    }
}

impl Drop for WriterEngine {
    fn drop(&mut self) {
        if let Some(handle) = self.request_polling_thread.take() {
            handle
                .join()
                .expect("The Writer thread is not expected to panic and should run without issues.")
                .expect("I'm not sure what the errors for this might be")
        }
    }
}

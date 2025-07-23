use crate::controller::{Bus, ConsoleMessage};
use crate::controller::{RibbleMessage, WorkRequest, MAX_NUM_CONSOLE_MESSAGES, MIN_NUM_CONSOLE_MESSAGES};
use crate::utils::errors::RibbleError;
use parking_lot::RwLock;
use ribble_whisper::utils::{Receiver, Sender};
use std::collections::VecDeque;
use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

struct ConsoleEngineState {
    incoming_messages: Receiver<ConsoleMessage>,
    queue: RwLock<VecDeque<Arc<ConsoleMessage>>>,
    // Because of the way VecDeque allocates, capacity needs to be tracked such that the length is
    // essentially fixed.
    // In practice, expect the real capacity to be slightly greater (likely the next power of two),
    // but the length of the elements will remain fixed to the user-specified limit.
    queue_capacity: AtomicUsize,
}

impl ConsoleEngineState {
    fn new(incoming_messages: Receiver<ConsoleMessage>, capacity: usize) -> Self {
        let capacity = capacity.max(MIN_NUM_CONSOLE_MESSAGES);
        let queue = RwLock::new(VecDeque::with_capacity(capacity));
        let queue_capacity = AtomicUsize::new(capacity);
        Self {
            incoming_messages,
            queue,
            queue_capacity,
        }
    }

    fn add_console_message(&self, message: ConsoleMessage) {
        // Get a write lock for pushing to the buffer
        let mut queue = self.queue.write();

        // If the buffer is at capacity, pop the first element
        let capacity = self.queue_capacity.load(Ordering::Acquire);

        debug_assert!(capacity > 0, "Redundancy error, capacity is zero");
        if queue.len() == capacity {
            queue.pop_front();
        }
        queue.push_back(Arc::new(message));
        debug_assert!(
            queue.len() <= capacity,
            "Queue length greater than capacity, pop logic is incorrect. Len: {}",
            queue.len()
        );
    }

    fn resize(&self, new_size: usize) {
        // Clamp the size between min/max
        let new_size = new_size
            .max(MIN_NUM_CONSOLE_MESSAGES)
            .min(MAX_NUM_CONSOLE_MESSAGES);
        // Determine whether to shrink or grow.
        let capacity = self.queue_capacity.load(Ordering::Acquire);
        if new_size > capacity {
            let diff = new_size.saturating_sub(capacity);
            // This is likely a little unnecessary, but stranger things have happened.
            // For now, leave the check as a debug assert and remove after testing.
            debug_assert!(diff > 0);
            self.grow(diff)
        } else if new_size < capacity {
            let diff = capacity.saturating_sub(new_size);
            debug_assert!(diff > 0);
            self.shrink(diff);
        } else {
            return;
        }
        self.queue_capacity.store(new_size, Ordering::Release);
    }

    // Since diff is pre-calculated and reserve is (additional), this should never, ever panic
    // except for in cases of a memory allocation error.
    fn grow(&self, diff: usize) {
        // Get a write lock to resize the buffer.
        self.queue.write().reserve(diff);
    }

    // Like above, diff is pre-calculated and this method clamps to the length of the buffer.
    // Expect that this will never panic.
    fn shrink(&self, diff: usize) {
        let mut queue = self.queue.write();
        let drain = diff.min(queue.len());
        queue.drain(..drain);
    }
}

// This is modelled akin to "history states" such that only a predefined list of
// console messages are retained.
// NOTE: if it becomes important to retain the entire history of the program for logging purposes,
// implement a double-buffer strategy to retain popped states.
pub(super) struct ConsoleEngine {
    inner: Arc<ConsoleEngineState>,
    work_request_sender: Sender<WorkRequest>,
    work_thread: Option<JoinHandle<()>>,
}

// Provide access to inner
impl ConsoleEngine {
    pub(super) fn new(
        incoming_messages: Receiver<ConsoleMessage>,
        capacity: usize,
        bus: &Bus,
    ) -> Self {
        let inner = Arc::new(ConsoleEngineState::new(incoming_messages, capacity));
        let thread_inner = Arc::clone(&inner);

        let worker = std::thread::spawn(move || {
            while let Ok(console_message) = thread_inner.incoming_messages.recv() {
                thread_inner.add_console_message(console_message);
            }
        });

        let work_thread = Some(worker);

        Self {
            inner,
            work_request_sender: bus.work_request_sender(),
            work_thread,
        }
    }

    // Implementing Clone for ConsoleMessage would get expensive; it's cheaper to just use
    // shared pointers
    pub(super) fn try_get_current_messages(&self, copy_buffer: &mut Vec<Arc<ConsoleMessage>>) {
        if let Some(buffer) = self.inner.queue.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(buffer.iter().cloned())
        }
    }

    // Since resizing can block, this dispatches a very short-lived thread to perform the resize in
    // the background.
    pub(super) fn resize(&self, new_size: usize) {
        let thread_inner = Arc::clone(&self.inner);
        let work = std::thread::spawn(move || {
            thread_inner.resize(new_size);
            Ok(RibbleMessage::BackgroundWork(Ok(())))
        });

        let work_request = WorkRequest::Short(work);
        if let Err(e) = self.work_request_sender.try_send(work_request) {
            log::error!("Cannot send resize request, channel closed or too small.\n\
            Error: {}\n\
            Error source: {}", &e, e.source());
        }
    }
}

impl Drop for ConsoleEngine {
    fn drop(&mut self) {
        if let Some(handle) = self.work_thread.take() {
            handle
                .join()
                .expect("The Console thread should never panic.");
        }
    }
}

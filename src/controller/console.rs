use crate::utils::console_message::NewConsoleMessage;
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// NOTE: hold off on adding a reference to the kernel if it's not required in the interface.
struct ConsoleState {
    queue: RwLock<VecDeque<NewConsoleMessage>>,
    // Because of the way VecDeque allocates, capacity needs to be tracked such that the length is
    // essentially fixed.
    // In practice, expect the real capacity to be slightly greater (likely the next power of two),
    // but the length of the elements will remain fixed to the user-specified limit.
    capacity: AtomicUsize,
}

// This is modelled akin to "history states" such that only a predefined list of
// console messages are retained.
// NOTE: if it becomes important to retain the entire history of the program for logging purposes,
// implement a double-buffer strategy to retain popped states.
pub struct ConsoleEngine {
    inner: Arc<ConsoleState>,
}

// Provide access to inner
impl ConsoleEngine {
    // It is fine to resize the inner queue, but this should take an initial nonzero capacity
    // The backing buffer is fine to be zero size, but the capacity is monitored to not exceed
    // a pre-defined user limit.
    pub(crate) fn new(capacity: NonZero<usize>) -> Self {
        let capacity = capacity.get();
        let console_queue = RwLock::new(VecDeque::with_capacity(capacity));
        let inner = ConsoleState {
            queue: console_queue,
            capacity: AtomicUsize::new(capacity),
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    pub(crate) fn add_console_message(&self, message: NewConsoleMessage) {
        // Get a write lock for pushing to the buffer
        let mut queue = self.inner.queue.write();

        // If the buffer is at capacity, pop the first element
        let capacity = self.inner.capacity.load(Ordering::Acquire);
        debug_assert!(capacity > 0, "Redundancy error, capacity is zero");
        if queue.len() == capacity {
            queue.pop_front();
        }
        queue.push_back(message);
        debug_assert!(
            queue.len() <= capacity,
            "Queue length greater than capacity, pop logic is incorrect. Len: {}",
            queue.len()
        );
        drop(queue);
    }

    pub(crate) fn try_get_current_message(&self, copy_buffer: &mut Vec<NewConsoleMessage>) {
        if let Some(buffer) = self.inner.queue.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(buffer.iter().cloned())
        }
    }

    pub(crate) fn resize(&self, new_size: NonZero<usize>) {
        let new_size = new_size.get();
        // Determine whether to shrink or grow.
        let capacity = self.inner.capacity.load(Ordering::Acquire);
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
        self.inner.capacity.store(new_size, Ordering::Release);
    }

    // Since diff is pre-calculated and reserve is (additional), this should never, ever panic
    // except for in cases of a memory allocation error.
    fn grow(&self, diff: usize) {
        // Get a write lock to resize the buffer.
        self.inner.queue.write().reserve(diff);
    }

    // Like above, diff is pre-calculated and this method clamps to the length of the buffer.
    // Expect that this will never panic.
    fn shrink(&self, diff: usize) {
        let mut queue = self.inner.queue.write();
        let drain = diff.min(queue.len());
        queue.drain(..drain);
    }
}

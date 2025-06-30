use crate::utils::constants::{MAX_NUM_MESSAGES, MIN_NUM_MESSAGES};
use crate::utils::errors::RibbleError;
use egui::{RichText, Visuals};
use parking_lot::RwLock;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use strum::Display;

// NOTE: hold off on adding a reference to the kernel if it's not required in the interface.
// TODO: if a kernel is not required, refactor these state parameters back into ConsoleEngine.
//
// TODO: spawn a background thread and use message queues instead.
struct ConsoleState {
    queue: RwLock<VecDeque<Arc<ConsoleMessage>>>,
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
pub(super) struct ConsoleEngine {
    inner: Arc<ConsoleState>,
}

// Provide access to inner
impl ConsoleEngine {
    // These are going to be
    const DEFAULT_NUM_MESSAGES: usize = 32;
    // It is fine to resize the inner queue, but this should take an initial nonzero capacity
    // The backing buffer is fine to be zero size, but the capacity is monitored to not exceed
    // a pre-defined user limit.
    //
    // TODO: -> Have this return a tuple (Self::Engine, Self::Port(sender))
    pub(super) fn new(capacity: usize) -> Self {
        let capacity = capacity.max(MIN_NUM_MESSAGES);

        let console_queue = RwLock::new(VecDeque::with_capacity(capacity));
        let inner = ConsoleState {
            queue: console_queue,
            capacity: AtomicUsize::new(capacity),
        };
        Self {
            inner: Arc::new(inner),
        }
    }

    // TODO: remove this method -> migrate the logic to a background thread.
    pub(super) fn add_console_message(&self, message: ConsoleMessage) {
        // Get a write lock for pushing to the buffer
        let mut queue = self.inner.queue.write();

        // If the buffer is at capacity, pop the first element
        let capacity = self.inner.capacity.load(Ordering::Acquire);
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
        drop(queue);
    }

    // Implementing Clone for ConsoleMessage would get expensive; it's cheaper to just use
    // shared pointers
    pub(super) fn try_get_current_message(&self, copy_buffer: &mut Vec<Arc<ConsoleMessage>>) {
        if let Some(buffer) = self.inner.queue.try_read() {
            copy_buffer.clear();
            copy_buffer.extend(buffer.iter().cloned())
        }
    }

    pub(super) fn resize(&self, new_size: usize) {
        // Clamp the size between min/max
        let new_size = new_size.max(MIN_NUM_MESSAGES).min(MAX_NUM_MESSAGES);
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

// TODO: implement drop

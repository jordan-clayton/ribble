use mimalloc::MiMalloc;

use crate::runner::RibbleRunner;
use crate::utils::errors::RibbleError;
use image::GenericImageView;

// This should be a faster allocator, good for short strings and allocation churn.
static GLOBAL: MiMalloc = MiMalloc;

mod controller;
mod ui;
mod utils;
mod runner;

// TODO: add a crash handler + native window on crash for segfaults.
// SEE: research notes.
fn main() -> Result<(), RibbleError> {
    let ribble = RibbleRunner::new()?;
    ribble.run()
}
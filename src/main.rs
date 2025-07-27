use mimalloc::MiMalloc;

use crate::runner::RibbleRunner;
use crate::utils::errors::RibbleError;

// This should be a faster allocator, good for short strings and allocation churn.
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod controller;
mod ui;
mod utils;
mod runner;

fn main() -> Result<(), RibbleError> {
    let ribble = RibbleRunner::new()?;
    ribble.run()
}
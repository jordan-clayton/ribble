#![windows_subsystem = "windows"] // Hide console window in release mode on windows
use mimalloc::MiMalloc;
use ribble::runner::RibbleRunner;

// This should be a faster allocator, good for short strings and allocation churn.
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

use ribble::utils::errors::RibbleError;

fn main() -> Result<(), RibbleError> {
    let ribble = RibbleRunner::new()?;
    ribble.run()
}


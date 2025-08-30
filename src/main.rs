#![cfg_attr(
    not(debug_assertions),
    allow(dead_code, unused_imports)
)] // Turn off warnings in release mode
// There is a lot of unused application code--mostly constructions/artifacts of the design process
// They may end up useful in the future/somewhere else, so for now they'll remain in the project.
#![windows_subsystem = "windows"] // Hide console window in release mode on windows.
use mimalloc::MiMalloc;

use crate::runner::RibbleRunner;
use crate::utils::errors::RibbleError;

// This should be a faster allocator, good for short strings and allocation churn.
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

mod controller;
mod runner;
mod ui;
mod utils;

fn main() -> Result<(), RibbleError> {
    let ribble = RibbleRunner::new()?;
    ribble.run()
}


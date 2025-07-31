#![windows_subsystem = "windows"] // Hide console window in release mode on windows
use mimalloc::MiMalloc;

use crate::runner::RibbleRunner;
use crate::utils::errors::RibbleError;

// TODO: remove this after profiling, this is so gross.
#[cfg(all(feature = "bencher", test))]
pub(crate) mod benches;

#[cfg(not(feature = "bencher"))]
mod controller;
#[cfg(feature = "bencher")]
pub(crate) mod controller;

mod runner;
#[cfg(not(feature = "bencher"))]
mod ui;
#[cfg(feature = "bencher")]
pub(crate) mod ui;

#[cfg(not(feature = "bencher"))]
mod utils;

#[cfg(feature = "bencher")]
pub(crate) mod utils;


// This should be a faster allocator, good for short strings and allocation churn.
#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;


fn main() -> Result<(), RibbleError> {
    let ribble = RibbleRunner::new()?;
    ribble.run()
}


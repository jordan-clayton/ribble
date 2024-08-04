use std::time::Duration;

use directories::ProjectDirs;

// This should probably be split into separate str qualifiers -> needs to be used for eframe.
// I DO NOT LIKE THE WAY THIS IS IMPLEMENTED.
fn proj_dir() -> Option<ProjectDirs> { ProjectDirs::from("com", "Jordan", "WhisperGUI") }

pub const APP_ID: &str = "WhisperGUI";
pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "Jordan";

// TODO: pick an appropriate livelock timeout.
pub const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

pub const SLEEP_DURATION: Duration = Duration::from_millis(1);
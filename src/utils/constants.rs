use std::time::Duration;

pub const APP_ID: &str = "WhisperGUI";
pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "Jordan";

pub const CLEAR_MSG: &str = "[CLEAR]";

// TODO: pick an appropriate livelock timeout.
pub const CHANNEL_TIMEOUT: Duration = Duration::from_millis(10);

pub const SLEEP_DURATION: Duration = Duration::from_millis(1);

pub const FROM_COLOR: egui::Rgba = egui::Rgba::from_rgba_premultiplied(0.0, 0.0, 0.0, 0.7);
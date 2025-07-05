use std::time::Duration;
pub const BLANK_SEPARATOR: f32 = 8.0;

// TODO: move GUI constants to a GUI common file.

// Uh, 1, this doesn't make a lot of sense; 2, just use the one from ribble_whisper
pub const MAX_REALTIME_TIMEOUT: u64 = (Duration::new(7200, 0).as_millis() / 1000) as u64;

pub const TOOLTIP_DELAY: f32 = 0.5;
pub const TOOLTIP_GRACE_TIME: f32 = 0.0;
// NOTE: these are in seconds due to slider scaling.
// TODO: Rethink this. Values > 0 should be fine, though it might be wiser to cap on 10s.
// Or, just let this be handled by ribble_whisper; 10s is perfectly reasonable as a window size.
pub const MIN_AUDIO_CHUNK_SIZE: f32 = 2.0;
pub const MAX_AUDIO_CHUNK_SIZE: f32 = 30.0;

// TODO: remove, this doesn't exist in ribble_core anymore.
pub const MIN_PHRASE_TIMEOUT: f32 = 0.5;
pub const MAX_PHRASE_TIMEOUT: f32 = 10.0;

// TODO: Move to VisualizerEngine
// This can go in the GUI constants lib

// This was used as a smoothing factor to prevent the bars from aggressively jumping.
pub const SMOOTH_FACTOR: f32 = 8.0;

// TODO: move to gui common -> fix.
pub const MAX_VISUALIZER_HEIGHT: f32 = 800.0;
pub const MIN_VISUALIZER_HEIGHT: f32 = 30.0;
pub const VISUALIZER_HEIGHT_EXPANSION: f32 = 20.0;
pub const VISUALIZER_MAX_HEIGHT_PROPORTION: f32 = 0.90;
pub const VISUALIZER_MIN_HEIGHT_PROPORTION: f32 = 0.10;
pub const VISUALIZER_MAX_WIDTH: f32 = 16.0;
pub const VISUALIZER_MIN_WIDTH: f32 = 8.0;
// TODO: move to VisualizerEngine

// TODO: look into what the heck these things are doing/for
// Lord-y.
pub const TREE_KEY: &str = "Tree";
pub const CLOSED_TABS_KEY: &str = "Closed Tabs";
// TODO: look into ron, choose a more appropriate format for serialization
// Ron is totally fine -- just rename this file -- > get rid of the old "data.ron"
pub const OLD_DATA_STORAGE_FILE: &str = "data.ron";
pub const APP_ID: &str = "Ribble";

pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "Jordan";

// TODO: REMOVE
pub const TEMP_FILE: &str = "tmp.wav";

pub const DEFAULT_BUTTON_LABEL: &str = "Reset to default";

pub const RECORDING_ANIMATION_TIMESCALE: f64 = 2.0;

pub const FROM_COLOR: egui::Rgba = egui::Rgba::from_rgba_premultiplied(0.0, 0.0, 0.0, 0.7);

pub const DESATURATION_MULTIPLIER: f32 = 0.5;

// TODO: this could probably go.
pub const MAX_WHISPER_THREADS: std::ffi::c_int = 8;

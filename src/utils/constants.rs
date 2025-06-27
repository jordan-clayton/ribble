use std::time::Duration;

// This has been tested to work with version 12.4.
// TODO: CUDA constants can probably be removed.
pub const CUDA_MAJOR: i32 = 12;
pub const CUDA_MINOR: i32 = 4;
pub const BLANK_SEPARATOR: f32 = 8.0;

// TODO: implement a User Preferences pane for these sorts of things.
pub const DEFAULT_CONSOLE_HISTORY_SIZE: usize = 35;

// 2 hours, in ms.
pub const MAX_REALTIME_TIMEOUT: u64 = (Duration::new(7200, 0).as_millis() / 1000) as u64;

pub const TOOLTIP_DELAY: f32 = 0.5;
pub const TOOLTIP_GRACE_TIME: f32 = 0.0;
// NOTE: these are in seconds due to slider scaling.
// TODO: Rethink this. Values > 0 should be fine, though it might be wiser to cap on 10s.
// Or, just let this be handled by ribble_core; 10s is perfectly reasonable as a window size.
pub const MIN_AUDIO_CHUNK_SIZE: f32 = 2.0;
pub const MAX_AUDIO_CHUNK_SIZE: f32 = 30.0;

// TODO: remove, this doesn't exist in ribble_core anymore.
pub const MIN_PHRASE_TIMEOUT: f32 = 0.5;
pub const MAX_PHRASE_TIMEOUT: f32 = 10.0;

// TODO: cross-check this with ribble_core, 200ms, is probably sufficient for all use cases.
// Possibly expose this in an AdvancedRealtimeConfigs pane.
pub const MIN_VAD_SEC: f32 = 0.1;
pub const MAX_VAD_SEC: f32 = 1.0;

// TODO: these likely need renaming
pub const MIN_VAD_PROBABILITY: f32 = 0.5;
pub const MAX_VAD_PROBABILITY: f32 = 0.9;

// FFT CONSTANTS
pub const FRAME_CONVERGENCE_ITERATIONS: usize = 1000;
pub const FRAME_CONVERGENCE_TOLERANCE: usize = 2;
// TODO: This should really be renamed.
pub const NUM_BUCKETS: usize = 32;
pub const SMOOTH_FACTOR: f32 = 8.0;

// Default range is 20Hz - 20kHz
pub const DEFAULT_F_LOWER: f32 = 20f32;
pub const DEFAULT_F_HIGHER: f32 = 20000f32;
pub const MIN_F_LOWER: f32 = 10.0;
pub const MAX_F_LOWER: f32 = 100.0;
pub const MIN_F_HIGHER: f32 = 330.0;
pub const MAX_F_HIGHER: f32 = 80000.0;
pub const MAX_VISUALIZER_HEIGHT: f32 = 800.0;
pub const MIN_VISUALIZER_HEIGHT: f32 = 30.0;
pub const VISUALIZER_HEIGHT_EXPANSION: f32 = 20.0;
pub const VISUALIZER_MAX_HEIGHT_PROPORTION: f32 = 0.90;
pub const VISUALIZER_MIN_HEIGHT_PROPORTION: f32 = 0.10;
pub const VISUALIZER_MAX_WIDTH: f32 = 16.0;
pub const VISUALIZER_MIN_WIDTH: f32 = 8.0;
pub const POWER_OVERLAP: f32 = 0.5;
pub const AMPLITUDE_OVERLAP: f32 = 0.25;
pub const POWER_GAIN: f32 = 30.0;
pub const WAVEFORM_GAIN: f32 = POWER_GAIN / 2.0;

// TODO: look into what the heck these things are doing/for
pub const TREE_KEY: &str = "Tree";
pub const CLOSED_TABS_KEY: &str = "Closed Tabs";
// TODO: look into ron, choose a more appropriate format for serialization
pub const RON_FILE: &str = "data.ron";
pub const APP_ID: &str = "Ribble";

// TODO: this is not a good way to solve whatever this is solving.
// Use an enumeration
pub const CLOSE_APP: &str = "[CLOSE APP]";
pub const QUALIFIER: &str = "com";
pub const ORGANIZATION: &str = "Jordan";

// TODO: These are now part of ribble_whisper.
pub const GO_MSG: &str = "[START SPEAKING]\n";
pub const STOP_MSG: &str = "\n[END TRANSCRIPTION]\n";
pub const TEMP_FILE: &str = "tmp.wav";

pub const DEFAULT_BUTTON_LABEL: &str = "Reset to default";

pub const RECORDING_ANIMATION_TIMESCALE: f64 = 2.0;

pub const FROM_COLOR: egui::Rgba = egui::Rgba::from_rgba_premultiplied(0.0, 0.0, 0.0, 0.7);

pub const DESATURATION_MULTIPLIER: f32 = 0.5;

// TODO: this could probably go.
pub const MAX_WHISPER_THREADS: std::ffi::c_int = 8;

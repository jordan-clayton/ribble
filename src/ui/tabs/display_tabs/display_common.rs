use catppuccin_egui::Theme;
use egui::{Rgba, Widget};

use crate::{
    ui::widgets::recording_icon::recording_icon,
    utils::{constants, workers::AudioWorkerState},
};

// TODO: This should live with widgets-and also maybe clean up the implementation? 
// This should really return a compound widget: | icon | label | ---> space
// Check the old implementation to see what's up here.

// Also, maybe remove this entirely? This is a bit of a GUI question, and it doesn't make sense for
// the state to be shared across two separate screens.
pub fn get_header_recording_icon(
    state: AudioWorkerState,
    transcriber: bool,
    theme: &Theme,
) -> (impl Widget, &'static str) {
    let time_scale = Some(constants::RECORDING_ANIMATION_TIMESCALE);
    match state {
        AudioWorkerState::Idle => {
            let icon = recording_icon(Rgba::from(theme.green), false, time_scale);
            let msg = "Ready.";
            (icon, msg)
        }
        AudioWorkerState::Loading => {
            let icon = recording_icon(Rgba::from(theme.green), true, time_scale);
            let msg = if transcriber {
                "Preparing to transcribe."
            } else {
                "Preparing to record."
            };
            (icon, msg)
        }
        AudioWorkerState::Running => {
            let icon = recording_icon(Rgba::from(theme.red), true, time_scale);
            let msg = if transcriber {
                "Transcription in progress."
            } else {
                "Recording in progress."
            };
            (icon, msg)
        }
        AudioWorkerState::Error => {
            let icon = recording_icon(Rgba::from(theme.yellow), true, time_scale);
            let msg = "Not ready.";
            (icon, msg)
        }
    }
}

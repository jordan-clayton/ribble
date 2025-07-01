use crate::controller::transcriber::OfflineTranscriberFeedback;
use strum::{AsRefStr, Display, EnumIter, EnumString};

// TODO: This needs refactoring & also is probably a good spot for "UserPreferences.".
pub fn get_app_theme(system_theme: Option<eframe::Theme>) -> catppuccin_egui::Theme {
    match system_theme {
        None => catppuccin_egui::MOCHA,
        Some(t) => match t {
            eframe::Theme::Dark => catppuccin_egui::MOCHA,
            eframe::Theme::Light => catppuccin_egui::LATTE,
        },
    }
}


// TODO: user preferences:
// App theme (Dark/Light/System/Custom: explicit catpuccin themes),
// Possibly even just grey? Not sure.
// console messages size (triggers change in ConsoleEngine), Offline Transcriber feedback settings
// Possibly make console messages size some predefined enum (16, 32, 64)

// Toggling themes globally:
// Egui defaults:
// ctx.set_theme(egui::Theme::...); (Dark, Light, System)
// Catpuccin:
// catppuccin_egui::set_theme(ctx, catpuccin_theme);

#[derive(
    Copy,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    EnumIter,
    EnumString,
    AsRefStr,
    Display
)]
pub(crate) enum RibbleAppTheme {
    System,
    Light,
    Dark,
    Latte,
    Frappe,
    Macchiato,
    Mocha,
}

impl RibbleAppTheme {
    // TODO: look into egui-theme-lerp crate, check out catppuccin source code for methods to extract the visuals
    // This should probably return a ThemeAnimator (if lerping) + New theme -> or just handle in the gui.
    pub(crate) fn set_theme(ctx: &egui::Context, new_theme: RibbleAppTheme) {
        todo!("Theme lerping logic.")
    }
}

// TODO: methods for changing this.
pub(crate) struct UserPreferences {
    console_message_size: usize,
    system_theme: RibbleAppTheme,
    transcriber_feedback_settings: OfflineTranscriberFeedback,
}
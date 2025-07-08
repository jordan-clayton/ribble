use crate::controller::DEFAULT_NUM_CONSOLE_MESSAGES;
use strum::{AsRefStr, Display, EnumIter, EnumString};
// TODO: determine whether or not to just move this to the controller module.
// Toggling themes globally:
// Egui defaults:
// ctx.set_theme(egui::Theme::...); (Dark, Light, System)
// Catpuccin:
// catppuccin_egui::set_theme(ctx, catpuccin_theme);

#[derive(
    Default,
    Copy,
    Clone,
    serde::Serialize,
    serde::Deserialize,
    EnumIter,
    EnumString,
    AsRefStr,
    Display,
)]
pub(crate) enum RibbleAppTheme {
    #[default]
    System,
    Light,
    Dark,
    Latte,
    Frappe,
    Macchiato,
    Mocha,
}

impl RibbleAppTheme {
    // NOTE: if the user prefs is set to System, the information needs to be queried from the 
    // viewport.
    // Rather than taking in an egui::context, it's easiest to just get the input state in the paint 
    // loop.
    pub(crate) fn visuals(&self) -> Option<egui::Visuals> {
        match self {
            RibbleAppTheme::System => { None }
            RibbleAppTheme::Light => { Some(egui::Visuals::light()) }
            RibbleAppTheme::Dark => { Some(egui::Visuals::dark()) }
            // NOTE: atm, catppuccin::Theme::visuals(...) isn't public, so this has to be done
            // through one of the public functions.
            // Since set_style_theme() only extracts the visuals anyway, it's fine to just use
            // egui's default visuals (dark mode) -- these should all be consistent with which style they modify.
            RibbleAppTheme::Latte => {
                let mut style = egui::Style::default();
                catppuccin_egui::set_style_theme(&mut style, catppuccin_egui::LATTE);
                Some(style.visuals)
            }
            RibbleAppTheme::Frappe => {
                let mut style = egui::Style::default();
                catppuccin_egui::set_style_theme(&mut style, catppuccin_egui::FRAPPE);
                Some(style.visuals)
            }
            RibbleAppTheme::Macchiato => {
                let mut style = egui::Style::default();
                catppuccin_egui::set_style_theme(&mut style, catppuccin_egui::MACCHIATO);
                Some(style.visuals)
            }
            RibbleAppTheme::Mocha => {
                let mut style = egui::Style::default();
                catppuccin_egui::set_style_theme(&mut style, catppuccin_egui::MACCHIATO);
                Some(style.visuals)
            }
        }
    }
}

#[derive(Copy, Clone)]
pub(crate) struct UserPreferences {
    console_message_size: usize,
    system_theme: RibbleAppTheme,
}

impl UserPreferences {
    pub(crate) fn new() -> Self {
        Self {
            console_message_size: 0,
            system_theme: Default::default(),
        }
    }

    pub(crate) fn with_console_message_size(mut self, new_size: usize) -> Self {
        // TODO: if going to limit the console messages to a predefined minimum,
        // The constants should go somewhere else
        self.console_message_size = new_size.max(1);
        self
    }
    pub(crate) fn with_system_theme(mut self, new_theme: RibbleAppTheme) -> Self {
        self.system_theme = new_theme;
        self
    }

    pub(crate) fn console_message_size(&self) -> usize {
        self.console_message_size
    }
    pub(crate) fn system_theme(&self) -> RibbleAppTheme {
        self.system_theme
    }
}

impl Default for UserPreferences {
    fn default() -> Self {
        Self::new().with_console_message_size(DEFAULT_NUM_CONSOLE_MESSAGES)
    }
}

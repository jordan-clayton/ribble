use crate::controller::DEFAULT_NUM_CONSOLE_MESSAGES;
use egui::epaint::Hsva;
use egui_colorgradient::{ColorInterpolator, Gradient};
use strum::{AsRefStr, Display, EnumIter, EnumString};

#[derive(
    Default,
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
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
            RibbleAppTheme::System => None,
            RibbleAppTheme::Light => Some(egui::Visuals::light()),
            RibbleAppTheme::Dark => Some(egui::Visuals::dark()),
            // NOTE: atm, catppuccin::Theme::visuals(...) isn't public, so this has to be done
            // through one of the public functions.
            // Since set_style_theme() only extracts the visuals anyway, it's fine to just use
            // egui's default visuals (dark mode) -- these should all be consistent with which style they modify.
            //
            // TODO: fork catppuccin_egui and expose the method directly to cut down on
            // indirection.
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
                catppuccin_egui::set_style_theme(&mut style, catppuccin_egui::MOCHA);
                Some(style.visuals)
            }
        }
    }

    pub(crate) fn gradient(&self) -> Option<Gradient> {
        match self {
            RibbleAppTheme::System => None,
            RibbleAppTheme::Light => Some(catppuccin_egui::LATTE),
            RibbleAppTheme::Dark => Some(catppuccin_egui::MOCHA),
            RibbleAppTheme::Latte => Some(catppuccin_egui::LATTE),
            RibbleAppTheme::Frappe => Some(catppuccin_egui::FRAPPE),
            RibbleAppTheme::Macchiato => Some(catppuccin_egui::MACCHIATO),
            RibbleAppTheme::Mocha => Some(catppuccin_egui::MOCHA),
        }
        .and_then(|theme| {
            let color_stops = [
                theme.mauve,
                theme.pink,
                theme.flamingo,
                theme.maroon,
                theme.peach,
                theme.yellow,
                theme.green,
                theme.teal,
                theme.sky,
                theme.sapphire,
                theme.blue,
                theme.lavender,
            ];

            let max_idx = color_stops.len() - 1;

            let iter = color_stops.iter().enumerate().map(|(idx, &color)| {
                let stop = idx as f32 / max_idx as f32;
                let color: Hsva = color.into();
                (stop, color)
            });

            let gradient = Gradient::new(egui_colorgradient::InterpolationMethod::Linear, iter);
            Some(gradient)
        })
    }

    pub(crate) fn color_interpolator(&self) -> Option<ColorInterpolator> {
        self.gradient().and_then(|grad| Some(grad.interpolator()))
    }
}

#[derive(Copy, Clone, serde::Serialize, serde::Deserialize)]
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

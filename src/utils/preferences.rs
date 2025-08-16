use crate::controller::{DEFAULT_NUM_CONSOLE_MESSAGES, MIN_NUM_CONSOLE_MESSAGES};
use egui::Visuals;
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
            // There are plans to fork catppuccin_egui at a later date; for now, things are fine.

            RibbleAppTheme::Latte => {
                Self::tweak_catppuccin_visuals(catppuccin_egui::LATTE)
            }
            RibbleAppTheme::Frappe => {
                Self::tweak_catppuccin_visuals(catppuccin_egui::FRAPPE)
            }
            RibbleAppTheme::Macchiato => {
                Self::tweak_catppuccin_visuals(catppuccin_egui::MACCHIATO)
            }
            RibbleAppTheme::Mocha => {
                Self::tweak_catppuccin_visuals(catppuccin_egui::MOCHA)
            }
        }
    }

    // There are some contrast issues with the faint-bg color.
    fn tweak_catppuccin_visuals(theme: catppuccin_egui::Theme) -> Option<Visuals> {
        let mut style = egui::Style::default();
        catppuccin_egui::set_style_theme(&mut style, theme);
        let mut visuals = style.visuals;
        // This is for striping, but catppuccin uses surface0 for widget bgs.
        // Mantle is a bit too dark, but will work in a pinch

        // Blending between the two seems to be closest to egui default visuals.
        let from: egui::Rgba = theme.base.into();
        let to: egui::Rgba = theme.surface0.into();
        let color: egui::Rgba = egui::lerp(from..=to, 0.5);
        visuals.faint_bg_color = color.into();
        Some(visuals)
    }

    pub(crate) fn app_theme(&self) -> Option<catppuccin_egui::Theme> {
        match self {
            RibbleAppTheme::System => None,
            RibbleAppTheme::Light => Some(catppuccin_egui::LATTE),
            RibbleAppTheme::Dark => Some(catppuccin_egui::MOCHA),
            RibbleAppTheme::Latte => Some(catppuccin_egui::LATTE),
            RibbleAppTheme::Frappe => Some(catppuccin_egui::FRAPPE),
            RibbleAppTheme::Macchiato => Some(catppuccin_egui::MACCHIATO),
            RibbleAppTheme::Mocha => Some(catppuccin_egui::MOCHA),
        }
    }

    pub(crate) fn gradient(&self) -> Option<Gradient> {
        self.app_theme().map(|theme| {
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
                (stop, color)
            });

            Gradient::new(egui_colorgradient::InterpolationMethod::Linear, iter)
        })
    }

    pub(crate) fn color_interpolator(&self) -> Option<ColorInterpolator> {
        self.gradient().map(|grad| grad.interpolator())
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
        self.console_message_size = new_size.max(MIN_NUM_CONSOLE_MESSAGES);
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

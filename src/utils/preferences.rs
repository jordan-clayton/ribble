use strum::{AsRefStr, Display, EnumIter, EnumString};

// TODO: user preferences:
// App theme (Dark/Light/System/Custom: explicit catpuccin themes),
// Possibly even just grey? Not sure.
// console messages size (triggers change in ConsoleEngine), Offline Transcriber feedback settings

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

// TODO: this should not be an enum method; this should be an app method.
impl RibbleAppTheme {
    // TODO: look into egui-theme-lerp crate, check out catppuccin source code for methods to extract the visuals
    // This should probably return a ThemeAnimator (if lerping) + New theme -> or just handle in the gui.
    pub(crate) fn set_theme(ctx: &egui::Context, new_theme: RibbleAppTheme) {
        todo!("Theme lerping logic.")
    }
}

#[derive(Copy, Clone)]
pub(crate) struct UserPreferences {
    console_message_size: usize,
    system_theme: RibbleAppTheme,
}

impl UserPreferences {
    // TODO: dedup this constant
    const DEFAULT_CONSOLE_MESSAGE_SIZE: usize = 32;
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
        Self::new().with_console_message_size(Self::DEFAULT_CONSOLE_MESSAGE_SIZE)
    }
}

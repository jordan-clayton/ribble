use egui::{RichText, Visuals};
use std::fmt::Formatter;
use strum::Display;

// TODO: rename this back to ConsoleMessage when ready to refactor the UI.
// TODO: if it is the case that status messages can all be 'static str, make that change here--
// -- it's likely not going to be the case in practice without significant ergonomics tradeoffs,
// (e.g. downloading should include the filename, which require strings).
#[derive(Clone, Debug, Display)]
pub(crate) enum NewConsoleMessage {
    #[strum(to_string = "Error: {}")]
    Error(String),
    #[strum(to_string = "Status: {}")]
    Status(String),
}

impl NewConsoleMessage {
    pub(crate) fn message(&self) -> &str {
        match self {
            Self::Error(msg) | Self::Status(msg) => msg
        }
    }

    pub(crate) fn into_inner(self) -> String {
        match self {
            Self::Error(msg) | Self::Status(msg) => msg
        }
    }

    // NOTE TO SELF: call ui.label(msg.to_console_text(&visuals)) in the console tab when drawing
    pub(crate) fn to_console_text(&self, visuals: &Visuals) -> RichText {
        let color = match self {
            NewConsoleMessage::Error(_) => { visuals.error_fg_color }
            NewConsoleMessage::Status(_) => { visuals.text_color() }
        };

        RichText::new(self.message()).color(color).monospace()
    }
}

#[derive(Clone, Debug)]
pub struct ConsoleMessage {
    msg_type: ConsoleMessageType,
    msg: String,
}

impl std::fmt::Display for ConsoleMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.msg_type.to_string(), self.msg)
    }
}

impl ConsoleMessage {
    pub fn new(msg_type: ConsoleMessageType, msg: String) -> Self {
        Self { msg_type, msg }
    }
}

#[derive(Clone, Debug, Display)]
pub enum ConsoleMessageType {
    Error,
    Status,
}

use std::collections::VecDeque;

use egui::{ScrollArea, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    ui::tabs::tab_view,
    utils::{console_message::ConsoleMessage, constants},
    whisper_app_context::WhisperAppController,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ErrorConsoleDisplayTab {
    title: String,
    #[serde(default = "console_history")]
    #[serde(skip)]
    console_queue: VecDeque<ConsoleMessage>,
}

impl ErrorConsoleDisplayTab {
    pub fn new() -> Self {
        let errors = console_history();
        Self {
            title: String::from("Errors"),
            console_queue: errors,
        }
    }
}

impl Default for ErrorConsoleDisplayTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for ErrorConsoleDisplayTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let Self {
            title: _,
            console_queue: errors,
        } = self;

        // Get errors
        let new_error = controller.recv_console_message();
        let mut len = errors.len();
        if let Ok(message) = new_error {
            errors.push_back(message);
            len += 1;

            if len > constants::DEFAULT_CONSOLE_HISTORY_SIZE {
                errors.pop_front();
                len -= 1;
            }
        }

        ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            // Print errors.
            let style = ui.style_mut();
            let bg_color = style.visuals.extreme_bg_color;
            style.visuals.panel_fill = bg_color;

            for (i, error) in errors.range(..).enumerate() {
                ui.monospace(error.to_string());
                if i < len - 1 {
                    ui.add_space(constants::BLANK_SEPARATOR);
                }
            }
        });
    }

    // TODO: determine whether needed
    fn context_menu(
        &mut self,
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        surface: SurfaceIndex,
        node: NodeIndex,
    ) {}

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

fn console_history() -> VecDeque<ConsoleMessage> {
    VecDeque::with_capacity(constants::DEFAULT_CONSOLE_HISTORY_SIZE)
}

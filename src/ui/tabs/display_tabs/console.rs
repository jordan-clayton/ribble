use std::collections::VecDeque;

use egui::{CentralPanel, Frame, ScrollArea, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::tab_view,
    utils::{console_message::ConsoleMessage, constants},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConsoleTab {
    title: String,
    #[serde(default = "console_history")]
    #[serde(skip)]
    console_queue: VecDeque<ConsoleMessage>,
}

impl ConsoleTab {
    pub fn new() -> Self {
        let errors = console_history();
        Self {
            title: String::from("Console"),
            console_queue: errors,
        }
    }
}

impl Default for ConsoleTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for ConsoleTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let Self {
            title: _,
            console_queue: console_messages,
        } = self;

        // Get errors
        let mut len = console_messages.len();
        while let Ok(message) = controller.recv_console_message() {
            console_messages.push_back(message);
            len += 1;

            if len > constants::DEFAULT_CONSOLE_HISTORY_SIZE {
                console_messages.pop_front();
                len -= 1;
            }
        }

        let visuals = ui.visuals();
        let bg_col = visuals.extreme_bg_color;
        let frame = Frame::default().fill(bg_col);
        CentralPanel::default().frame(frame).show_inside(ui, |ui| {
            ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for message in console_messages.range(..) {
                    ui.monospace(message.to_string());
                    ui.add_space(constants::BLANK_SEPARATOR);
                }
            });
        });
    }

    fn context_menu(
        &mut self,
        _ui: &mut Ui,
        _controller: &mut WhisperAppController,
        _surface: SurfaceIndex,
        _node: NodeIndex,
    ) {
    }

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

use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::ui::tabs::tab_view;
use crate::whisper_app_context::WhisperAppController;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ErrorConsoleDisplayTab {
    title: String,
}

impl ErrorConsoleDisplayTab {
    pub fn new() -> Self {
        Self {
            title: String::from("Errors"),
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
        todo!()
    }

    fn context_menu(
        &mut self,
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        surface: SurfaceIndex,
        node: NodeIndex,
    ) {
        todo!()
    }

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

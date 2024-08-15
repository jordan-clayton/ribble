use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::ui::tabs::tab_view;
use crate::whisper_app_context::WhisperAppController;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProgressDisplayTab {
    title: String,
}

impl ProgressDisplayTab {
    pub fn new() -> Self {
        Self { title: String::from("Progress") }
    }
}

impl Default for ProgressDisplayTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for ProgressDisplayTab {
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        todo!()
    }

    fn context_menu(&mut self, ui: &mut Ui, controller: &mut WhisperAppController, surface: SurfaceIndex, node: NodeIndex) {
        todo!()
    }

    fn closeable(&mut self) -> bool {
        todo!()
    }

    fn allowed_in_windows(&mut self) -> bool {
        todo!()
    }
}
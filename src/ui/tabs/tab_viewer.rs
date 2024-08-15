use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::whisper_app_context::WhisperAppController;

use super::{tab_view::TabView, whisper_tab::WhisperTab};

pub struct WhisperTabViewer {
    controller: WhisperAppController,
}

impl WhisperTabViewer {
    pub fn new(controller: WhisperAppController) -> Self {
        Self { controller }
    }
}

impl egui_dock::TabViewer for WhisperTabViewer {
    type Tab = WhisperTab;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.title()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        tab.ui(ui, &mut self.controller)
    }

    fn context_menu(&mut self, ui: &mut Ui, tab: &mut Self::Tab, surface: SurfaceIndex, node: NodeIndex) {
        tab.context_menu(ui, &mut self.controller, surface, node)
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.closeable()
    }

    fn allowed_in_windows(&self, tab: &mut Self::Tab) -> bool {
        tab.allowed_in_windows()
    }
}

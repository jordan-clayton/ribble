use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::ui::tabs::tab_view;
use crate::utils::configs::RecorderConfigs;
use crate::whisper_app_context::WhisperAppController;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RecordingConfigsTab {
    title: String,
    recorder_configs: RecorderConfigs,
}

impl RecordingConfigsTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: RecorderConfigs) -> Self {
        Self { title: String::from("Recording Configs"), recorder_configs: configs }
    }
}

impl Default for RecordingConfigsTab {
    fn default() -> Self {
        let configs = RecorderConfigs::default();
        Self::new_with_configs(configs)
    }
}

impl tab_view::TabView for RecordingConfigsTab {
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
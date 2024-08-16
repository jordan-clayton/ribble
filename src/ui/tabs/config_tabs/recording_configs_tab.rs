use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::ui::tabs::tab_view;
use crate::utils::configs::RecorderConfigs;
use crate::whisper_app_context::WhisperAppController;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecordingConfigsTab {
    title: String,
    recorder_configs: RecorderConfigs,
}

impl RecordingConfigsTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: RecorderConfigs) -> Self {
        Self {
            title: String::from("Recording Configs"),
            recorder_configs: configs,
        }
    }
}

impl Default for RecordingConfigsTab {
    fn default() -> Self {
        let configs = RecorderConfigs::default();
        Self::new_with_configs(configs)
    }
}

impl tab_view::TabView for RecordingConfigsTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }

    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let Self {
            title: _,
            recorder_configs,
        } = self;

        let RecorderConfigs {
            sample_rate,
            buffer_size,
            channel,
            format,
        } = recorder_configs;

        let recorder_running = controller.recorder_running();

        let recorder_ready = controller.ready();
        todo!()
        // Grid of configs + button for default.
    }

    // TODO: determine if this is required.
    fn context_menu(
        &mut self,
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        surface: SurfaceIndex,
        node: NodeIndex,
    ) {
    }

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

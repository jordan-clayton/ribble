use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use whisper_realtime::configs::Configs;

use crate::ui::tabs::tab_view;
use crate::whisper_app_context::WhisperAppController;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct StaticConfigsTab {
    title: String,
    static_configs: Configs,
    // Shared datacache (state flags and wot).
}

// TODO
impl StaticConfigsTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: Configs) -> Self {
        Self { title: String::from("Static Configs"), static_configs: configs }
    }
}

impl Default for StaticConfigsTab {
    fn default() -> Self {
        let configs = Configs::default();
        Self::new_with_configs(configs)
    }
}


impl tab_view::TabView for StaticConfigsTab {
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, _ui: &mut Ui, _controller: &mut WhisperAppController) {
        todo!()
    }

    // Right-click tab -> What should be shown.
    fn context_menu(&mut self, _ui: &mut Ui, _controller: &mut WhisperAppController, _surface: SurfaceIndex, _node: NodeIndex) {
        todo!()
    }

    fn closeable(&mut self) -> bool {
        false
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

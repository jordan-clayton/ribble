use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use whisper_realtime::configs::Configs;

use super::super::tab_view;

#[derive(serde::Serialize, serde::Deserialize)]
pub struct RealtimeConfigsTab {
    title: String,
    realtime_configs: Configs,
}

// TODO
impl RealtimeConfigsTab {
    fn new(configs: Configs) -> Self {
        Self { title: String::from("Realtime Configs"), realtime_configs: configs }
    }
}

impl Default for RealtimeConfigsTab {
    fn default() -> Self {
        let configs = Configs::default();
        Self::new(configs)
    }
}


impl tab_view::TabView for RealtimeConfigsTab {
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, _ui: &mut Ui) {
        todo!()
    }

    // Right-click tab -> What should be shown.
    fn context_menu(&mut self, _ui: &mut Ui, _surface: SurfaceIndex, _node: NodeIndex) {
        todo!()
    }

    fn closeable(&mut self) -> bool {
        false
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}
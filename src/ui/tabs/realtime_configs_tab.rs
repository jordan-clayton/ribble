use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use std::sync::Arc;
use whisper_realtime::configs::Configs;

use super::tab_view;


// TODO: this will likely need to take an Arc'd Configs
#[derive(Clone)]
pub struct RealtimeTab{
    title: String,
    configs: Arc<Configs>,
    // Shared datacache.
}

// TODO: Bindings for whisper_realtime realtime transcription

impl RealtimeTab{
    fn new(configs: Arc<Configs>) -> Self{
        Self{title: String::from("Realtime Configs"), configs}
    }


}

impl Default for RealtimeTab{
    fn default() -> Self{
        let configs= Arc::new(Configs::default());
        Self::new(configs)
    }
}


impl tab_view::TabView for RealtimeTab{

    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, _ui: &mut Ui ) {
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
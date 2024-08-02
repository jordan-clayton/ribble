use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use std::sync:: mpsc;

use super::tab_view;

pub struct TranscriptionTab {
    title: String,
    text_buffer: Vec<String>,
    data_channel: mpsc::Receiver<String>,
    // Shared datacache.
}

// TODO: implementation for when to clear the buffer.
// TODO: function to clear the buffer.
// TODO: function to save the buffer to file.
impl TranscriptionTab {
    fn new(channel: mpsc::Receiver<String> ) -> Self{
        Self{title: String::from("Transcription"), text_buffer: vec![], data_channel: channel}
    }


}


impl tab_view::TabView for TranscriptionTab {

    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, _ui: &mut Ui ) {
        todo!()
    }

    // Right-click tab -> context actions for text operations
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

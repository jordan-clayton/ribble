use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::whisper_app_context::WhisperAppController;

// Port of egui_dock::TabViewer interface, removing type parameter
// To be able to have heterogeneous tabs.

//TODO: port the remainder of the impl.
pub trait TabView {
    fn title(&mut self) -> WidgetText;
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController);

    fn context_menu(&mut self, ui: &mut Ui, controller: &mut WhisperAppController, surface: SurfaceIndex, node: NodeIndex);

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        false
    }
}
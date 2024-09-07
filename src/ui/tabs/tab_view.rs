use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::controller::whisper_app_controller::WhisperAppController;

// Port of egui_dock::TabViewer trait major features used in drawing app ui.

pub trait TabView {
    fn id(&mut self) -> String;
    fn title(&mut self) -> WidgetText;
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController);

    fn context_menu(
        &mut self,
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        surface: SurfaceIndex,
        node: NodeIndex,
    );

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        false
    }
}

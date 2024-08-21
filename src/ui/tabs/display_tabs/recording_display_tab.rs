use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{ui::tabs::tab_view, whisper_app_context::WhisperAppController};

// TODO: Two buffers, current + target
// On new data, change the target.
// On each frame, smooth current toward target by SMOOTHING FACTOR & paint to scrn.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecordingDisplayTab {
    title: String,
    // For determining whether to run/display the fft.
    visualize: bool,
}

// TODO: this will need a proper constructor.
impl RecordingDisplayTab {
    pub fn new() -> Self {
        Self {
            title: String::from("Recording"),
            visualize: true,
        }
    }
}

impl Default for RecordingDisplayTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for RecordingDisplayTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Split view:  Visualizer | Buttons: Output path, Visualizer toggle, Start and stop recording, etc.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        todo!()
    }

    // TODO: determine if actually useful.
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

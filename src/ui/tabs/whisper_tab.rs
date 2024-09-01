use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{
        controller_tabs::{
            r#static::StaticConfigsTab, realtime::RealtimeConfigsTab,
            recording::RecordingConfigsTab,
        },
        display_tabs::{
            console::ErrorConsoleDisplayTab, progress::ProgressDisplayTab,
            transcription::TranscriptionTab,
            visualizer::RecordingDisplayTab,
        },
        tab_view::TabView,
    },
};

// This is a concession made to keep the implementation as decoupled as possible.
// Generics are not possible due to the sized type bound required for egui_dock::TabViewer
// Each member of WhisperTab must implement TabView, a port of the egui_dock::TabViewer interface
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum WhisperTab {
    RealtimeConfigs(RealtimeConfigsTab),
    StaticConfigs(StaticConfigsTab),
    RecordingConfigs(RecordingConfigsTab),
    TranscriptionDisplay(TranscriptionTab),
    RecordingDisplay(RecordingDisplayTab),
    ProgressDisplay(ProgressDisplayTab),
    ErrorDisplay(ErrorConsoleDisplayTab),
}

impl WhisperTab {
    fn decay(&mut self) -> &mut dyn TabView {
        match self {
            WhisperTab::RealtimeConfigs(rc) => rc,
            WhisperTab::StaticConfigs(sc) => sc,
            WhisperTab::RecordingConfigs(rec) => rec,
            WhisperTab::TranscriptionDisplay(td) => td,
            WhisperTab::RecordingDisplay(rd) => rd,
            WhisperTab::ProgressDisplay(pd) => pd,
            WhisperTab::ErrorDisplay(ed) => ed,
        }
    }
}

impl TabView for WhisperTab {
    fn id(&mut self) -> String {
        self.decay().id()
    }
    fn title(&mut self) -> WidgetText {
        self.decay().title()
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        self.decay().ui(ui, controller)
    }

    fn context_menu(
        &mut self,
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        surface: SurfaceIndex,
        node: NodeIndex,
    ) {
        self.decay().context_menu(ui, controller, surface, node);
    }

    fn closeable(&mut self) -> bool {
        self.decay().closeable()
    }

    fn allowed_in_windows(&mut self) -> bool {
        self.decay().allowed_in_windows()
    }
}

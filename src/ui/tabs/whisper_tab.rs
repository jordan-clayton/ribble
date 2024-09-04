use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{
        controller_tabs::{r#static::StaticTab, realtime::RealtimeTab, recording::RecordingTab},
        display_tabs::{
            console::ConsoleTab, progress::ProgressTab, transcription::TranscriptionTab,
            visualizer::VisualizerTab,
        },
        tab_view::TabView,
    },
};

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum WhisperTab {
    Realtime(RealtimeTab),
    Static(StaticTab),
    Recording(RecordingTab),
    Transcription(TranscriptionTab),
    Visualizer(VisualizerTab),
    Progress(ProgressTab),
    Console(ConsoleTab),
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum FocusTab {
    Realtime,
    Static,
    Recording,
    Transcription,
    Visualizer,
    Progress,
    Console,
}

impl FocusTab {
    pub fn id(&self) -> String {
        match self {
            FocusTab::Realtime => String::from("Realtime"),
            FocusTab::Static => String::from("Static"),
            FocusTab::Recording => String::from("Recording"),
            FocusTab::Transcription => String::from("Transcription"),
            FocusTab::Visualizer => String::from("Visualizer"),
            FocusTab::Progress => String::from("Progress"),
            FocusTab::Console => String::from("Progress"),
        }
    }
}

impl WhisperTab {
    fn decay(&mut self) -> &mut dyn TabView {
        match self {
            WhisperTab::Realtime(rc) => rc,
            WhisperTab::Static(sc) => sc,
            WhisperTab::Recording(rec) => rec,
            WhisperTab::Transcription(td) => td,
            WhisperTab::Visualizer(rd) => rd,
            WhisperTab::Progress(pd) => pd,
            WhisperTab::Console(ed) => ed,
        }
    }

    pub fn matches(&self, tab_type: FocusTab) -> bool {
        match self {
            WhisperTab::Realtime(_) => tab_type == FocusTab::Realtime,
            WhisperTab::Static(_) => tab_type == FocusTab::Static,
            WhisperTab::Recording(_) => tab_type == FocusTab::Recording,
            WhisperTab::Transcription(_) => tab_type == FocusTab::Transcription,
            WhisperTab::Visualizer(_) => tab_type == FocusTab::Visualizer,
            WhisperTab::Progress(_) => tab_type == FocusTab::Progress,
            WhisperTab::Console(_) => tab_type == FocusTab::Console,
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

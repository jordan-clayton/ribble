// Encapsulation struct of trait T for covariant implementation type S.

use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::ui::tabs::{
    config_tabs::{realtime_configs_tab, recording_configs_tab, static_configs_tab},
    display_tabs::{
        error_console_display_tab, progress_display_tab, recording_display_tab,
        transcription_display_tab,
    },
    tab_view::TabView,
};
use crate::whisper_app_context::WhisperAppController;

// This is a concession made to keep the implementation as decoupled as possible.
// Generics are not possible due to the sized type bound required for egui_dock::TabViewer
// Each member of WhisperTab must implement TabView, a port of the egui_dock::TabViewer interface
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub enum WhisperTab {
    RealtimeConfigs(realtime_configs_tab::RealtimeConfigsTab),
    StaticConfigs(static_configs_tab::StaticConfigsTab),
    RecordingConfigs(recording_configs_tab::RecordingConfigsTab),
    TranscriptionDisplay(transcription_display_tab::TranscriptionTab),
    RecordingDisplay(recording_display_tab::RecordingDisplayTab),
    ProgressDisplay(progress_display_tab::ProgressDisplayTab),
    ErrorDisplay(error_console_display_tab::ErrorConsoleDisplayTab),
}

impl WhisperTab {
    // TODO: remove if unused
    fn as_tab_view(&mut self) -> &mut dyn TabView {
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
        id(self)
    }
    fn title(&mut self) -> WidgetText {
        title(self)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        draw_ui(self, ui, controller)
    }

    fn context_menu(
        &mut self,
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        surface: SurfaceIndex,
        node: NodeIndex,
    ) {
        context_menu(self, ui, controller, surface, node);
    }

    fn closeable(&mut self) -> bool {
        closeable(self)
    }

    fn allowed_in_windows(&mut self) -> bool {
        allowed_in_windows(self)
    }
}

// IMPL functions:
// To enforce that all members of the struct implement tabview
fn id(tab: &mut impl TabView) -> String {
    tab.id()
}
fn title(tab: &mut impl TabView) -> WidgetText {
    tab.title()
}

fn draw_ui(tab: &mut impl TabView, ui: &mut Ui, controller: &mut WhisperAppController) {
    tab.ui(ui, controller);
}

fn context_menu(
    tab: &mut impl TabView,
    ui: &mut Ui,
    controller: &mut WhisperAppController,
    surface: SurfaceIndex,
    node: NodeIndex,
) {
    tab.context_menu(ui, controller, surface, node)
}

fn closeable(tab: &mut impl TabView) -> bool {
    tab.closeable()
}

fn allowed_in_windows(tab: &mut impl TabView) -> bool {
    tab.allowed_in_windows()
}

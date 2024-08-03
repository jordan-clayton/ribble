// Encapsulation struct of trait T for covariant implementation type S.

use egui::{Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use super::{tab_view::TabView, realtime_configs_tab, transcription_tab, static_configs_tab};


// This is a concession made to keep the implementation as decoupled as possible.
// Generics are not possible due to the sized type bound required for egui_dock::TabViewer
// Each member of WhisperTab must implement TabView, a port of the egui_dock::TabViewer interface
pub enum WhisperTab{
    RealtimeConfigs(realtime_configs_tab::RealtimeTab),
    StaticConfigs(static_configs_tab::StaticTab),
    // TranscriptionConfigs
    Transcription(transcription_tab::TranscriptionTab)
    // Recording
    // Progress
    // Errors
}

impl WhisperTab {
    // TODO: remove if unused
    fn as_tab_view(&mut self) -> &mut dyn TabView{
       match self{
           WhisperTab::RealtimeConfigs(rt) => rt,
           WhisperTab::StaticConfigs(st) => st
           WhisperTab::Transcription(tt) => tt,
       }
    }
}


impl TabView for WhisperTab {
    fn title(&mut self) -> WidgetText {
        title(self)
    }

    fn ui(&mut self, ui: &mut Ui) {
        draw_ui(self, ui)
    }

    fn context_menu(&mut self, ui: &mut Ui, surface: SurfaceIndex, node: NodeIndex) {
        context_menu(self, ui, surface, node);
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
fn title(tab: &mut impl TabView) -> WidgetText{
    tab.title()
}

fn draw_ui(tab: &mut impl TabView, ui: &mut Ui){
   tab.ui(ui)
}

fn context_menu(tab: &mut impl TabView, ui: &mut Ui, surface: SurfaceIndex, node: NodeIndex){
    tab.context_menu(ui, surface, node)
}

fn closeable(tab: &mut impl TabView) -> bool{
   tab.closeable()
}

fn allowed_in_windows(tab: &mut impl TabView) -> bool{
    tab.allowed_in_windows()
}


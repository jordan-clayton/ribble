use std::collections::HashMap;

use egui::{Id, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{tab_view::TabView, whisper_tab::WhisperTab},
};

pub struct WhisperTabViewer<'a> {
    controller: WhisperAppController,
    closed_tabs: &'a mut HashMap<String, WhisperTab>,
    added_tabs: &'a mut Vec<(SurfaceIndex, NodeIndex, WhisperTab)>,
}

impl<'a> WhisperTabViewer<'a> {
    pub fn new(
        controller: WhisperAppController,
        closed_tabs: &'a mut HashMap<String, WhisperTab>,
        added_tabs: &'a mut Vec<(SurfaceIndex, NodeIndex, WhisperTab)>,
    ) -> Self {
        Self {
            controller,
            closed_tabs,
            added_tabs,
        }
    }
}

impl egui_dock::TabViewer for WhisperTabViewer<'_> {
    type Tab = WhisperTab;

    fn title(&mut self, tab: &mut Self::Tab) -> WidgetText {
        tab.title()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        tab.ui(ui, &mut self.controller)
    }

    fn context_menu(
        &mut self,
        ui: &mut Ui,
        tab: &mut Self::Tab,
        surface: SurfaceIndex,
        node: NodeIndex,
    ) {
        tab.context_menu(ui, &mut self.controller, surface, node)
    }

    fn id(&mut self, tab: &mut Self::Tab) -> Id {
        Id::new(tab.id())
    }

    fn closeable(&mut self, tab: &mut Self::Tab) -> bool {
        tab.closeable()
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        self.closed_tabs.insert(tab.id(), tab.clone());
        true
    }

    // TODO: fix sizing
    fn add_popup(&mut self, ui: &mut Ui, surface: SurfaceIndex, node: NodeIndex) {
        let closed_tabs: Vec<String> = self.closed_tabs.keys().cloned().collect();
        ui.style_mut().visuals.button_frame = false;

        for key in closed_tabs {
            if ui.button(&key).clicked() {
                let tab = self
                    .closed_tabs
                    .remove(&key)
                    .expect("Failed to get tab key");
                self.added_tabs.push((surface, node, tab));
            }
        }
    }

    fn allowed_in_windows(&self, tab: &mut Self::Tab) -> bool {
        tab.allowed_in_windows()
    }
}

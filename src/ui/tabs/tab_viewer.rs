use std::collections::HashMap;

use eframe::epaint::text::TextWrapMode;
use egui::{Id, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex, TabStyle};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{tab_view::TabView, whisper_tab::WhisperTab},
    utils::preferences::get_app_theme,
};

pub struct WhisperTabViewer<'a> {
    controller: WhisperAppController,
    closed_tabs: &'a mut HashMap<String, WhisperTab>,
    added_tabs: &'a mut Vec<(SurfaceIndex, NodeIndex, WhisperTab)>,
    n_open_tabs: usize,
}

impl<'a> WhisperTabViewer<'a> {
    pub fn new(
        controller: WhisperAppController,
        closed_tabs: &'a mut HashMap<String, WhisperTab>,
        added_tabs: &'a mut Vec<(SurfaceIndex, NodeIndex, WhisperTab)>,
        n_open_tabs: usize,
    ) -> Self {
        Self {
            controller,
            closed_tabs,
            added_tabs,
            n_open_tabs,
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
        tab.closeable() && self.n_open_tabs > 1
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> bool {
        self.closed_tabs.insert(tab.id(), tab.clone());
        if let WhisperTab::Visualizer(_) = tab {
            self.controller.set_run_visualizer(false);
        }
        true
    }

    fn add_popup(&mut self, ui: &mut Ui, surface: SurfaceIndex, node: NodeIndex) {
        let closed_tabs: Vec<String> = self.closed_tabs.keys().cloned().collect();
        ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
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
    fn tab_style_override(&self, _tab: &Self::Tab, global_style: &TabStyle) -> Option<TabStyle> {
        let system_theme = self.controller.get_system_theme();
        let theme = get_app_theme(system_theme);
        let mut tab_style = global_style.clone();
        let mut focus_style = tab_style.focused.clone();
        focus_style.outline_color = theme.lavender;
        focus_style.bg_fill = theme.surface0;
        tab_style.focused = focus_style;
        Some(tab_style)
    }

    fn allowed_in_windows(&self, tab: &mut Self::Tab) -> bool {
        tab.allowed_in_windows()
    }

    fn scroll_bars(&self, _tab: &Self::Tab) -> [bool; 2] {
        [false, false]
    }
}

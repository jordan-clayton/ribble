use std::collections::HashMap;

use catppuccin_egui::Theme;
use egui::Visuals;
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex, TabIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{
        controller_tabs::{r#static, realtime, recording},
        display_tabs::{
            console, progress, transcription,
            visualizer,
        },
        tab_viewer,
    },
    utils::preferences,
};
use crate::ui::tabs::whisper_tab::WhisperTab;

pub struct WhisperApp {
    // These need to be serialized
    tree: DockState<WhisperTab>,
    closed_tabs: HashMap<String, WhisperTab>,
    controller: WhisperAppController,
}

impl WhisperApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mut controller: WhisperAppController) -> Self {
        let storage = cc.storage;
        let system_theme = cc.integration_info.system_theme;
        controller.set_system_theme(system_theme);
        match storage {
            None => Self::default_layout(controller),
            Some(s) => {
                let stored_state = eframe::get_value(s, eframe::APP_KEY);
                match stored_state {
                    None => Self::default_layout(controller),
                    Some(state) => {
                        let (tree, closed_tabs) = state;
                        Self {
                            tree,
                            closed_tabs,
                            controller,
                        }
                    }
                }
            }
        }
    }

    fn default_layout(controller: WhisperAppController) -> Self {
        let closed_tabs = HashMap::new();

        let td = WhisperTab::Transcription(
            transcription::TranscriptionTab::default(),
        );
        let rd = WhisperTab::Visualizer(
            visualizer::VisualizerTab::default(),
        );
        let pd = WhisperTab::Progress(
            progress::ProgressTab::default(),
        );
        let ed = WhisperTab::Console(
            console::ConsoleTab::default(),
        );
        let rc = WhisperTab::Realtime(
            realtime::RealtimeTab::default(),
        );
        let st =
            WhisperTab::Static(r#static::StaticTab::default());
        let rec = WhisperTab::Recording(
            recording::RecordingTab::default(),
        );
        let mut tree: DockState<WhisperTab> = DockState::new(vec![td, rd]);

        let surface = tree.main_surface_mut();

        let [top, _] = surface.split_below(NodeIndex::root(), 0.7, vec![pd, ed]);

        let [_, _] = surface.split_right(top, 0.6, vec![rc, st, rec]);

        Self {
            tree,
            closed_tabs,
            controller,
        }
    }
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let system_theme = frame.info().system_theme;
        self.controller.set_system_theme(system_theme.clone());

        let theme = preferences::get_app_theme(system_theme);

        catppuccin_egui::set_theme(ctx, theme);

        // Catppuccin uses the same color for faint_bg and inactive widgets.
        // This causes issues with striped layouts.
        let mut visuals = ctx.style().visuals.clone();
        tweak_visuals(&mut visuals, theme);
        ctx.set_visuals(visuals);

        // Repaint continuously when running a worker.
        if self.controller.is_working() {
            ctx.request_repaint();
        }

        // Tab focus.
        let mut focus_tabs: Vec<(SurfaceIndex, NodeIndex, TabIndex)> = vec![];
        let mut missing_tabs: Vec<String> = vec![];
        while let Ok(focus_tab) = self.controller.recv_focus_tab() {
            let mut found = false;
            let surfaces = self.tree.iter_surfaces();
            for (surface_index, surface) in surfaces.enumerate() {
                let tree = surface.node_tree();
                if tree.is_none() {
                    continue;
                }
                let tree = tree.unwrap();

                for (node_index, node) in tree.iter().enumerate() {
                    let tabs = node.tabs();
                    if tabs.is_none() {
                        continue;
                    }
                    let tabs = tabs.unwrap();
                    for (tab_index, tab) in tabs.iter().enumerate() {
                        if tab.matches(focus_tab) {
                            focus_tabs.push((surface_index.into(), node_index.into(), tab_index.into()));
                            found = true;
                        }
                    }
                }
            }

            if !found {
                missing_tabs.push(focus_tab.id())
            }
        }

        for location in focus_tabs {
            self.tree.set_active_tab(location);
        }

        for key in missing_tabs {
            let tab = self.closed_tabs.remove(&key);
            if let Some(t) = tab {
                self.tree.push_to_focused_leaf(t);
            }
        }

        let mut closed_tabs = self.closed_tabs.clone();
        let show_add = !closed_tabs.is_empty();
        let mut added_tabs = vec![];

        let mut tab_viewer = tab_viewer::WhisperTabViewer::new(
            self.controller.clone(),
            &mut closed_tabs,
            &mut added_tabs,
        );

        DockArea::new(&mut self.tree)
            .style(Style::from_egui(ctx.style().as_ref()))
            // Quick-fix for tabs being non-recoverable if a window is closed.
            .show_window_close_buttons(false)
            .show_add_buttons(show_add)
            .show_add_popup(show_add)
            .show(ctx, &mut tab_viewer);

        self.closed_tabs = closed_tabs;

        added_tabs.drain(..).for_each(|(surface, node, tab)| {
            self.tree.set_focused_node_and_surface((surface, node));
            self.tree.push_to_focused_leaf(tab);
        });
    }

    // TODO: Restore once testing finished.
    // fn save(&mut self, storage: &mut dyn Storage) {
    //     eframe::set_value(storage, eframe::APP_KEY, &(&self.tree, &self.closed_tabs));
    // }

    // TODO: set back to true once testing done
    fn persist_egui_memory(&self) -> bool {
        false
    }
}

// This is a fix to deal with surface0 being used for both widgets
// and faint_bg_color. Sliders and checkboxes get lost when using
// striped layouts.
fn tweak_visuals(visuals: &mut Visuals, theme: Theme) {
    visuals.faint_bg_color = theme.mantle
}


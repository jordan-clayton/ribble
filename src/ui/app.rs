use std::collections::HashMap;

use eframe::{Storage, Theme};
use egui_dock::{DockArea, DockState, NodeIndex, Style};

use crate::ui::tabs::config_tabs::recording_configs_tab;
use crate::ui::tabs::display_tabs::{error_console_display_tab, recording_display_tab};
use crate::utils::preferences;
use crate::utils::sdl_audio_wrapper::SdlAudioWrapper;
use crate::whisper_app_context::WhisperAppController;

use super::tabs::{
    config_tabs::{realtime_configs_tab, static_configs_tab},
    display_tabs::{progress_display_tab, transcription_display_tab},
    tab_viewer, whisper_tab,
};

// TODO: finish App implementation.
// TODO: eframe::save & periodic configs serialization.

pub struct WhisperApp {
    // These need to be serialized
    tree: DockState<whisper_tab::WhisperTab>,
    closed_tabs: HashMap<String, whisper_tab::WhisperTab>,
    controller: WhisperAppController,
}

// ** TODO, add toasts -> on click should focus the error tab.

impl WhisperApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        audio_wrapper: std::sync::Arc<SdlAudioWrapper>,
    ) -> Self {
        let storage = cc.storage;
        let system_theme = cc.integration_info.system_theme;
        match storage {
            None => Self::default_layout(audio_wrapper, system_theme),
            Some(s) => {
                let stored_state = eframe::get_value(s, eframe::APP_KEY);
                match stored_state {
                    None => Self::default_layout(audio_wrapper, system_theme),
                    Some(state) => {
                        let (tree, mut closed_tabs) = state;
                        let controller = WhisperAppController::new(audio_wrapper, system_theme);
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

    fn default_layout(
        audio_wrapper: std::sync::Arc<SdlAudioWrapper>,
        system_theme: Option<Theme>,
    ) -> Self {
        let controller = WhisperAppController::new(audio_wrapper, system_theme);

        let mut closed_tabs = HashMap::new();

        let td = whisper_tab::WhisperTab::TranscriptionDisplay(
            transcription_display_tab::TranscriptionTab::default(),
        );
        let rd = whisper_tab::WhisperTab::RecordingDisplay(
            recording_display_tab::RecordingDisplayTab::default(),
        );
        let pd = whisper_tab::WhisperTab::ProgressDisplay(
            progress_display_tab::ProgressDisplayTab::default(),
        );
        let ed = whisper_tab::WhisperTab::ErrorDisplay(
            error_console_display_tab::ErrorConsoleDisplayTab::default(),
        );
        let rc = whisper_tab::WhisperTab::RealtimeConfigs(
            realtime_configs_tab::RealtimeConfigsTab::default(),
        );
        let st =
            whisper_tab::WhisperTab::StaticConfigs(static_configs_tab::StaticConfigsTab::default());
        let rec = whisper_tab::WhisperTab::RecordingConfigs(
            recording_configs_tab::RecordingConfigsTab::default(),
        );
        let mut tree: DockState<whisper_tab::WhisperTab> = DockState::new(vec![td, rd]);

        let surface = tree.main_surface_mut();

        let [top, _] = surface.split_below(NodeIndex::root(), 0.7, vec![pd, ed]);

        let [_, _] = surface.split_right(top, 0.7, vec![rc, st, rec]);

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

        // Repaint continuously when running a worker.
        if self.controller.is_working() {
            ctx.request_repaint();
        }

        // let mut closed_tabs = clone_closed_tabs(&self.closed_tabs);
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
            .show_add_buttons(show_add)
            .show_add_popup(show_add)
            .show(ctx, &mut tab_viewer);

        self.closed_tabs = closed_tabs;

        added_tabs.drain(..).for_each(|(surface, node, tab)| {
            self.tree.set_focused_node_and_surface((surface, node));
            self.tree.push_to_focused_leaf(tab);
        });
    }

    fn save(&mut self, storage: &mut dyn Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &(&self.tree, &self.closed_tabs));
    }

    // TODO: set to false when testing default layout.
    fn persist_egui_memory(&self) -> bool {
        true
    }
}

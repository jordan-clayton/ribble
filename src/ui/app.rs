use eframe::Storage;
use egui_dock::{DockArea, DockState, NodeIndex, Style};

use crate::ui::tabs::config_tabs::recording_configs_tab;
use crate::ui::tabs::display_tabs::{error_console_display_tab, recording_display_tab};
use crate::utils::sdl_audio_wrapper::SdlAudioWrapper;
use crate::whisper_app_context::WhisperAppController;

use super::tabs::{
    config_tabs::{
        realtime_configs_tab,
        static_configs_tab
        ,
    },
    display_tabs::{
        progress_display_tab
        ,
        transcription_display_tab
        ,
    },
    tab_viewer, whisper_tab};

// TODO: shared data cache containing:
// Atomic state variables,
// A Mutex-Guarded shared audio buffer

// POSSIBLY: Arc<Channels> for inter-tab communication.
// Might need to store & join thread-handles.

// TODO: finish App implementation.
// TODO: eframe::save & periodic configs serialization.

pub struct WhisperApp
{
    tree: DockState<whisper_tab::WhisperTab>,
    tab_viewer: tab_viewer::WhisperTabViewer,
    controller: WhisperAppController,

}

// Preliminary default design.

// _______________________________________________________________________________________________________
// |transcription tab/Recording tab  || Realtime Configs tab / Static Configs Tab / Recording Configs tab |
// |______________________________________________________________________________________________________
// | Errors / Progress bars toasts?                                                                       |
// _______________________________________________________________________________________________________

// Main tabs
//  Transcription tab: Text view + buttons for starting + saving transcription.
//  Recording tab: Visualization + buttons for saving

// Configs tabs
// Rt configs tab: For toggling Rt configs, button to download model + File dialog: external model.
// St configs tab: For toggling Rt configs, button to download model + File dialog: external model.
// Recording tab: Recording time, file paths, etc.

// Bottom tabs
// Progress bar for progress operations.
// Error console for detailed error messaging.

// ** Also, add toasts -> on click should focus the error tab.
impl WhisperApp {
    pub fn new(cc: &eframe::CreationContext<'_>, audio_wrapper: std::sync::Arc<SdlAudioWrapper>) -> Self {
        let storage = cc.storage;
        match storage {
            None => Self::default_layout(audio_wrapper),
            Some(s) => {
                let stored_tree = eframe::get_value(s, eframe::APP_KEY);
                match stored_tree {
                    None => Self::default_layout(audio_wrapper),
                    Some(tree) => {
                        let controller = WhisperAppController::new(audio_wrapper);
                        let tab_viewer = tab_viewer::WhisperTabViewer::new(controller.clone());
                        Self { tree, tab_viewer, controller }
                    }
                }
            }
        }
    }

    fn default_layout(audio_wrapper: std::sync::Arc<SdlAudioWrapper>) -> Self {
        let controller = WhisperAppController::new(audio_wrapper);

        let tab_viewer = tab_viewer::WhisperTabViewer::new(controller.clone());

        // TODO: cleanup where necessary
        let td = whisper_tab::WhisperTab::TranscriptionDisplay(transcription_display_tab::TranscriptionTab::new());
        let rd = whisper_tab::WhisperTab::RecordingDisplay(recording_display_tab::RecordingDisplayTab::new());
        let pd = whisper_tab::WhisperTab::ProgressDisplay(progress_display_tab::ProgressDisplayTab::new());
        let ed = whisper_tab::WhisperTab::ErrorDisplay(error_console_display_tab::ErrorConsoleDisplayTab::new());
        let rc = whisper_tab::WhisperTab::RealtimeConfigs(realtime_configs_tab::RealtimeConfigsTab::default());
        let st = whisper_tab::WhisperTab::StaticConfigs(static_configs_tab::StaticConfigsTab::default());
        let rec = whisper_tab::WhisperTab::RecordingConfigs(recording_configs_tab::RecordingConfigsTab::new());
        let mut tree: DockState<whisper_tab::WhisperTab> = DockState::new(vec![
            td, rd,
        ]);

        let surface = tree.main_surface_mut();

        let [top, _] = surface.split_below(
            NodeIndex::root(),
            0.7,
            vec![
                pd, ed,
            ],
        );

        let [_, _] = surface.split_right(
            top,
            0.7,
            vec![
                rc,
                st,
                rec,
            ],
        );

        Self { tree, tab_viewer, controller }
    }
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // Repaint continuously when running a worker.
        if self.controller.is_working() {
            ctx.request_repaint();
        }

        DockArea::new(&mut self.tree)
            .style(Style::from_egui(ctx.style().as_ref()))
            .show(ctx, &mut self.tab_viewer);
    }


    fn save(&mut self, storage: &mut dyn Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.tree);
    }

    // TODO: set to false when testing default layout.
    fn persist_egui_memory(&self) -> bool { true }
}
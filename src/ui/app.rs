use eframe::Storage;
use egui_dock::{DockArea, DockState, NodeIndex, Style};

use crate::ui::tabs::config_tabs::recording_configs_tab;
use crate::ui::tabs::display_tabs::{error_console_display_tab, recording_display_tab};

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
impl Default for WhisperApp
{
    fn default() -> Self {

        // Initialize ctx

        // Pass to tab viewer struct.

        // Call tab viewer struct


        // TODO: Implement this properly -> should construct the main shared data struct & handle serialization.
        let (_, recv) = std::sync::mpsc::channel();

        let td = whisper_tab::WhisperTab::TranscriptionDisplay(transcription_display_tab::TranscriptionTab::new(recv));
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

        let tab_viewer = tab_viewer::WhisperTabViewer::default();


        Self { tree, tab_viewer }
    }
}

impl WhisperApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let storage = cc.storage;
        match storage {
            None => Self::default(),
            Some(s) => {
                let stored_tree = eframe::get_value(s, eframe::APP_KEY);
                match stored_tree {
                    None => Self::default(),
                    Some(tree) => {
                        let tab_viewer = tab_viewer::WhisperTabViewer::default();
                        Self { tree, tab_viewer }
                    }
                }
            }
        }
    }
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {

        // TODO: Once State Struct
        // if self.struct.load_atomic_running_boolean{
        //      ctx.request_repaint()
        // }

        DockArea::new(&mut self.tree)
            .style(Style::from_egui(ctx.style().as_ref()))
            .show(ctx, &mut self.tab_viewer);
    }


    // TODO: Implement save.
    fn save(&mut self, storage: &mut dyn Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.tree)
    }

    fn persist_egui_memory(&self) -> bool { true }
}
use egui_dock::{DockArea, DockState, NodeIndex, Style};

use super::tabs::{realtime_configs_tab, static_configs_tab, tab_renderer, transcription_tab, whisper_tab};

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

        // TODO: Implement this properly -> should construct the main shared data struct & handle serialization.
        let (_, recv) = std::sync::mpsc::channel();

        let tt = whisper_tab::WhisperTab::Transcription(transcription_tab::TranscriptionTab::new(recv));
        let rt = whisper_tab::WhisperTab::RealtimeConfigs(realtime_configs_tab::RealtimeTab::default());
        let st = whisper_tab::WhisperTab::StaticConfigs(static_configs_tab::StaticTab::default());
        let mut tree: DockState<whisper_tab::WhisperTab> = DockState::new(vec![
            tt
            // Recording tab.
        ]);

        let mut surface = tree.main_surface_mut();

        let [top, _] = surface.split_below(
            NodeIndex::root(),
            0.7,
            vec![
                // Progress
                // Errors
            ],
        );

        let [_, _] = surface.split_right(
            top,
            0.7,
            vec![
                rt,
                st,
                // recording configs.
            ],
        );

        // Could also add windows?


        Self { tree }
    }
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        DockArea::new(&mut self.tree)
            .stype(Style::from_egui(ctx.style().as_ref()))
            .show(ctx, &mut tab_renderer::WhisperTabViewer {});
    }
}
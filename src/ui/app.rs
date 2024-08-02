use egui_dock::DockState;

use super::tabs::{whisper_tab, realtime_configs_tab};

// TODO: shared data cache containing:
// Atomic state variables,
// A Mutex-Guarded shared audio buffer

// POSSIBLY: Arc<Channels> for inter-tab communication.
// Might need to store & join thread-handles.

// TODO: finish App implementation.
struct App
{
    tree: DockState<whisper_tab::WhisperTab>,
}

impl Default for App
 {
   fn default() -> Self{
       let dock_state: DockState<whisper_tab::WhisperTab> = DockState::new(vec![
          whisper_tab::WhisperTab::RealtimeConfigs(realtime_configs_tab::RealtimeTab::default())
       ]);

       Self{tree: dock_state}
   }
}
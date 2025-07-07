use enum_dispatch::enum_dispatch;
use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};
// NOTE: these need to be brought into scope so that enum_dispatch can run its macros
use crate::ui::new_tabs::tabs::*;

// These are "Panes" used for the egui_tiles Tree
// NOTE: VadConfigs is now in Transcriber (collapsible)
// NOTE: *Model Tabs are now modals.
// NOTE: downloads should get a full view tab + cancel mechanism.
#[derive(serde::Serialize, serde::Deserialize, strum::EnumCount, Clone)]
#[enum_dispatch]
pub(in crate::ui) enum RibbleTab {
    TranscriberTab,
    RecordingTab,
    TranscriptionTab,
    VisualizerTab,
    ProgressTab,
    ConsoleTab,
    UserPreferencesTab,
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub(in crate::ui) enum RibbleTabId {
    Transcriber,
    Recording,
    Transcription,
    Visualizer,
    Console,
    Progress,
    UserPreferences,
}

impl From<RibbleTabId> for RibbleTab {
    fn from(value: RibbleTabId) -> Self {
        match value {
            // TODO: implement default methods for tabs to make this easier.
            RibbleTabId::Transcriber => RibbleTab::TranscriberTab(TranscriberTab::default()),
            RibbleTabId::Recording => RibbleTab::RecordingTab(RecordingTab {}),
            RibbleTabId::Transcription => RibbleTab::TranscriptionTab(TranscriptionTab {}),
            RibbleTabId::Visualizer => { RibbleTab::VisualizerTab(VisualizerTab {}) }
            RibbleTabId::Console => RibbleTab::ConsoleTab(ConsoleTab {}),
            RibbleTabId::Progress => RibbleTab::ProgressTab(ProgressTab {}),
            RibbleTabId::UserPreferences => RibbleTab::UserPreferencesTab(UserPreferencesTab {})
        }
    }
}

#[derive(EnumIter, EnumString, AsRefStr, IntoStaticStr, Debug)]
pub(in crate::ui) enum ClosableRibbleTab {
    Visualizer,
    Progress,
    Console,
    // POSSIBLY add this as a horizontal instead of a tab at a focused area.
    // If doing that, implement some sort of cog-widget at the top of the tab bar and remove from this list.
    #[strum(serialize = "User Preferences")]
    UserPreferences,
}

// TODO: this might be cleaned up if default is implemented on each tab -> should be doable, espec. if stateless/ZST
impl From<ClosableRibbleTab> for RibbleTab {
    fn from(value: ClosableRibbleTab) -> Self {
        match value {
            ClosableRibbleTab::Visualizer => { RibbleTab::VisualizerTab(VisualizerTab {}) }
            ClosableRibbleTab::Progress => { RibbleTab::ProgressTab(ProgressTab {}) }
            ClosableRibbleTab::Console => { RibbleTab::ConsoleTab(ConsoleTab {}) }
            ClosableRibbleTab::UserPreferences => { RibbleTab::UserPreferencesTab(UserPreferencesTab {}) }
        }
    }
}
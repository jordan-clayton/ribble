use enum_dispatch::enum_dispatch;
use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};
// NOTE: these need to be brought into scope so that enum_dispatch can run its macros
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::tabs::*;
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::ArcChannelSink;

#[enum_dispatch(RibbleTab)]
pub(in crate::ui) trait TabView {
    fn tile_id(&self) -> RibbleTabId;
    fn tab_title(&mut self) -> egui::WidgetText;
    /// # Arguments:
    /// * ui: egui::Ui, for drawing,
    /// * tile_id: egui_tiles::TileId, this Pane's id
    /// * controller: RibbleController, for accessing internal data.
    /// TODO: add an argument so that a pane can request to be closed.
    /// OR: use an enumeration: PaneResponse::UiResponse(..), PaneResponse::Close,
    fn pane_ui<A: AudioBackend<ArcChannelSink<f32>>>(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        // If there's significant atomic overhead, swap to a reference.
        // It shouldn't be an issue though.
        controller: RibbleController<A>,
    ) -> egui::Response;

    /// Should this tab be closable?
    fn is_tab_closable(&self) -> bool;

    /// Fires whenever a tab is closed
    /// * return true if the tab should still be closed.
    /// * return false if the tab should remain open
    fn on_tab_close<A: AudioBackend<ArcChannelSink<f32>>>(
        &mut self,
        _controller: RibbleController<A>,
    ) -> bool {
        self.is_tab_closable()
    }
}

// These are "Panes" used for the egui_tiles Tree
// NOTE: VadConfigs is now in Transcriber (collapsible)
// NOTE: *Model Tabs are now modals.
// NOTE: downloads should get a full view tab + cancel mechanism.
#[derive(serde::Serialize, serde::Deserialize, strum::EnumCount, Clone)]
#[enum_dispatch]
pub(in crate::ui) enum RibbleTab {
    TranscriberTab(TranscriberTab),
    RecordingTab(RecordingTab),
    TranscriptionTab(TranscriptionTab),
    VisualizerTab(VisualizerTab),
    ProgressTab(ProgressTab),
    ConsoleTab(ConsoleTab),
    UserPreferencesTab(UserPreferencesTab),
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
            RibbleTabId::Visualizer => RibbleTab::VisualizerTab(VisualizerTab::default()),
            RibbleTabId::Console => RibbleTab::ConsoleTab(ConsoleTab::default()),
            RibbleTabId::Progress => RibbleTab::ProgressTab(ProgressTab::default()),
            RibbleTabId::UserPreferences => RibbleTab::UserPreferencesTab(UserPreferencesTab {}),
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
            ClosableRibbleTab::Visualizer => RibbleTab::VisualizerTab(VisualizerTab::default()),
            ClosableRibbleTab::Progress => RibbleTab::ProgressTab(ProgressTab::default()),
            ClosableRibbleTab::Console => RibbleTab::ConsoleTab(ConsoleTab::default()),
            ClosableRibbleTab::UserPreferences => {
                RibbleTab::UserPreferencesTab(UserPreferencesTab::default())
            }
        }
    }
}

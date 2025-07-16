use enum_dispatch::enum_dispatch;
use strum::{AsRefStr, EnumIter, EnumString, IntoStaticStr};
// NOTE: these need to be brought into scope so that enum_dispatch can run its macros
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::panes::*;

#[enum_dispatch(RibblePane)]
pub(in crate::ui) trait PaneView {
    fn pane_id(&self) -> RibblePaneId;

    fn pane_title(&self) -> egui::WidgetText;
    /// # Arguments:
    /// * ui: egui::Ui, for drawing,
    /// * tile_id: egui_tiles::TileId, this Pane's id
    /// * controller: RibbleController, for accessing internal data.
    /// TODO: add an argument so that a pane can request to be closed.
    /// OR: use an enumeration: PaneResponse::UiResponse(..), PaneResponse::Close,
    /// ACTUALLY: there's a mechanism in the egui upgrade that makes this a lot easier.
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        // If there's significant atomic overhead, swap to a reference.
        // It shouldn't be an issue though.
        controller: RibbleController,
    ) -> egui::Response;

    /// Should this tab be closable?
    fn is_pane_closable(&self) -> bool;

    /// Fires whenever a tab is closed
    /// * return true if the tab should still be closed.
    /// * return false if the tab should remain open
    fn on_pane_close(&mut self, _controller: RibbleController) -> bool {
        self.is_pane_closable()
    }
}

// These are "Panes" used for the egui_tiles Tree
// NOTE: VadConfigs is now in Transcriber (collapsible)
// NOTE: *Model Tabs are now modals.
// NOTE: downloads should get a full view tab + cancel mechanism.
#[derive(serde::Serialize, serde::Deserialize, strum::EnumCount, Clone)]
#[enum_dispatch]
pub(in crate::ui) enum RibblePane {
    TranscriberPane(TranscriberPane),
    RecordingPane(RecordingPane),
    TranscriptionPane(TranscriptionPane),
    VisualizerPane(VisualizerPane),
    ProgressPane(ProgressPane),
    ConsolePane(ConsolePane),
    DownloadsPane(DownloadsPane),
    UserPreferencesPane(UserPreferencesPane),
}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub(in crate::ui) enum RibblePaneId {
    Transcriber,
    Recording,
    Transcription,
    Visualizer,
    Console,
    Progress,
    Downloads,
    UserPreferences,
}

impl From<RibblePaneId> for RibblePane {
    fn from(value: RibblePaneId) -> Self {
        match value {
            // TODO: implement default methods for tabs to make this easier.
            RibblePaneId::Transcriber => RibblePane::TranscriberPane(TranscriberPane::default()),
            RibblePaneId::Recording => RibblePane::RecordingPane(RecordingPane::default()),
            RibblePaneId::Transcription => RibblePane::TranscriptionPane(TranscriptionPane {}),
            RibblePaneId::Visualizer => RibblePane::VisualizerPane(VisualizerPane::default()),
            RibblePaneId::Console => RibblePane::ConsolePane(ConsolePane::default()),
            RibblePaneId::Downloads => RibblePane::DownloadsPane(DownloadsPane::default()),
            RibblePaneId::Progress => RibblePane::ProgressPane(ProgressPane::default()),
            RibblePaneId::UserPreferences => {
                RibblePane::UserPreferencesPane(UserPreferencesPane::default())
            }
        }
    }
}

// TODO: rename this to ClosableRibblePane
// Use in an actual menu-bar with a "window/panes menu"
// Rethink the "opened-pane" implementation; it can probably be way simpler.
// Also, perhaps split at the root, or split a leaf.
#[derive(EnumIter, EnumString, AsRefStr, IntoStaticStr, Debug)]
pub(in crate::ui) enum ClosableRibbleTab {
    Visualizer,
    Progress,
    Console,
    Downloads,
    Recording,
    // POSSIBLY add this as a horizontal instead of a tab at a focused area.
    // If doing that, implement some sort of cog-widget at the top of the tab bar and remove from this list.
    #[strum(serialize = "User Preferences")]
    UserPreferences,
}

// TODO: this might be cleaned up if default is implemented on each tab -> should be doable, espec. if stateless/ZST
impl From<ClosableRibbleTab> for RibblePane {
    fn from(value: ClosableRibbleTab) -> Self {
        match value {
            ClosableRibbleTab::Visualizer => RibblePane::VisualizerPane(VisualizerPane::default()),
            ClosableRibbleTab::Progress => RibblePane::ProgressPane(ProgressPane::default()),
            ClosableRibbleTab::Console => RibblePane::ConsolePane(ConsolePane::default()),
            ClosableRibbleTab::Downloads => RibblePane::DownloadsPane(DownloadsPane::default()),
            ClosableRibbleTab::Recording => RibblePane::RecordingPane(RecordingPane::default()),
            ClosableRibbleTab::UserPreferences => {
                RibblePane::UserPreferencesPane(UserPreferencesPane::default())
            }
        }
    }
}

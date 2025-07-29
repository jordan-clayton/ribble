// NOTE: these need to be brought into scope so that enum_dispatch can run its macros
use crate::controller::ribble_controller::RibbleController;
use crate::ui::panes::pane_list::*;
use enum_dispatch::enum_dispatch;
use strum::{AsRefStr, Display, EnumIter, EnumString, IntoStaticStr};

#[enum_dispatch(RibblePane)]
pub(in crate::ui) trait PaneView {
    fn pane_id(&self) -> RibblePaneId;

    fn pane_title(&self) -> egui::WidgetText;
    /// # Arguments:
    /// * ui: egui::Ui, for drawing
    /// * should_close: &mut bool, set to true if the user (can and) has closed the pane.
    /// * controller: RibbleController, for accessing internal data.
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        should_close: &mut bool,
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
// NOTE: *Model Tabs are now modals/dialogs.
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

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Display, AsRefStr)]
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

#[derive(EnumIter, EnumString, AsRefStr, IntoStaticStr, Debug)]
pub(in crate::ui) enum ClosableRibbleViewPane {
    Recording,
    Console,
    Downloads,
    Progress,
    Visualizer,
}

impl From<ClosableRibbleViewPane> for RibblePane {
    fn from(value: ClosableRibbleViewPane) -> Self {
        match value {
            ClosableRibbleViewPane::Recording => RibblePane::RecordingPane(RecordingPane::default()),
            ClosableRibbleViewPane::Console => RibblePane::ConsolePane(ConsolePane::default()),
            ClosableRibbleViewPane::Downloads => RibblePane::DownloadsPane(DownloadsPane::default()),
            ClosableRibbleViewPane::Progress => RibblePane::ProgressPane(ProgressPane::default()),
            ClosableRibbleViewPane::Visualizer => RibblePane::VisualizerPane(VisualizerPane::default()),
        }
    }
}

impl From<ClosableRibbleViewPane> for RibblePaneId {
    fn from(value: ClosableRibbleViewPane) -> Self {
        match value {
            ClosableRibbleViewPane::Recording => RibblePaneId::Recording,
            ClosableRibbleViewPane::Console => RibblePaneId::Console,
            ClosableRibbleViewPane::Downloads => RibblePaneId::Downloads,
            ClosableRibbleViewPane::Progress => RibblePaneId::Progress,
            ClosableRibbleViewPane::Visualizer => RibblePaneId::Visualizer
        }
    }
}

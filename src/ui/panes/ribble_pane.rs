// NOTE: these need to be brought into scope so that enum_dispatch can run its macros
use crate::controller::ribble_controller::RibbleController;
use crate::ui::panes::pane_list::*;
use crate::utils::errors::RibbleError;
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
// TODO: DECIDE WHAT TO DO HERE.
// NOTE: currently this Enumeration is size ~246 bytes because of the double buffering in
// VisualizerEngine.
// THERE IS A NOTE IN VISUALIZERENGINE to fix this.

// At this time, it's not decided whether the resolution should be tweakable or fixed,
// and whether to use vectors in the view

// It -might- be more efficient to just leave this be with stack-allocation
#[derive(serde::Serialize, serde::Deserialize, strum::EnumCount, Clone)]
#[enum_dispatch]
pub(in crate::ui) enum RibblePane {
    Transcriber(TranscriberPane),
    Recording(RecordingPane),
    Transcription(TranscriptionPane),
    Visualizer(VisualizerPane),
    Progress(ProgressPane),
    Console(ConsolePane),
    Downloads(DownloadsPane),
    UserPreferences(UserPreferencesPane),
}

// Since data is just caching, define equality based on the discriminant.
impl PartialEq for RibblePane {
    fn eq(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Eq for RibblePane {}

#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash, Display, AsRefStr, EnumIter)]
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

impl RibblePaneId {
    pub(in crate::ui) fn is_closable(&self) -> bool {
        ClosableRibbleViewPane::try_from(*self).is_ok()
    }
}

impl From<RibblePaneId> for RibblePane {
    fn from(value: RibblePaneId) -> Self {
        match value {
            RibblePaneId::Transcriber => RibblePane::Transcriber(TranscriberPane::default()),
            RibblePaneId::Recording => RibblePane::Recording(RecordingPane::default()),
            RibblePaneId::Transcription => RibblePane::Transcription(TranscriptionPane {}),
            RibblePaneId::Visualizer => RibblePane::Visualizer(VisualizerPane::default()),
            RibblePaneId::Console => RibblePane::Console(ConsolePane::default()),
            RibblePaneId::Downloads => RibblePane::Downloads(DownloadsPane::default()),
            RibblePaneId::Progress => RibblePane::Progress(ProgressPane::default()),
            RibblePaneId::UserPreferences => {
                RibblePane::UserPreferences(UserPreferencesPane::default())
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
    UserPreferences,
}

impl From<ClosableRibbleViewPane> for RibblePane {
    fn from(value: ClosableRibbleViewPane) -> Self {
        match value {
            ClosableRibbleViewPane::Recording => RibblePane::Recording(RecordingPane::default()),
            ClosableRibbleViewPane::Console => RibblePane::Console(ConsolePane::default()),
            ClosableRibbleViewPane::Downloads => RibblePane::Downloads(DownloadsPane::default()),
            ClosableRibbleViewPane::Progress => RibblePane::Progress(ProgressPane::default()),
            ClosableRibbleViewPane::Visualizer => RibblePane::Visualizer(VisualizerPane::default()),
            ClosableRibbleViewPane::UserPreferences => RibblePane::UserPreferences(UserPreferencesPane::default())
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
            ClosableRibbleViewPane::Visualizer => RibblePaneId::Visualizer,
            ClosableRibbleViewPane::UserPreferences => RibblePaneId::UserPreferences
        }
    }
}

impl TryFrom<RibblePaneId> for ClosableRibbleViewPane {
    type Error = RibbleError;

    fn try_from(value: RibblePaneId) -> Result<Self, Self::Error> {
        match value {
            RibblePaneId::Transcriber => { Err(RibbleError::ConversionError("Transcriber Pane is not closable.")) }
            RibblePaneId::Recording => { Ok(ClosableRibbleViewPane::Recording) }
            RibblePaneId::Transcription => { Err(RibbleError::ConversionError("Transcription Pane is not closable.")) }
            RibblePaneId::Visualizer => { Ok(ClosableRibbleViewPane::Visualizer) }
            RibblePaneId::Progress => { Ok(ClosableRibbleViewPane::Progress) }
            RibblePaneId::Console => { Ok(ClosableRibbleViewPane::Console) }
            RibblePaneId::Downloads => { Ok(ClosableRibbleViewPane::Downloads) }
            RibblePaneId::UserPreferences => { Ok(ClosableRibbleViewPane::UserPreferences) }
        }
    }
}
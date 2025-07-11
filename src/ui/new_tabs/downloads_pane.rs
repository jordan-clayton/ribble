use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::ribble_pane::{PaneView, RibblePaneId};

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub(in crate::ui) struct DownloadsPane {
    // TODO: this will require a redundant buffer of "Downloads" (metadata).
    // Return to this once the Downloader has been exteeeended.
}

impl PaneView for DownloadsPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Downloads
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Downloads".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        todo!();
        // Basic idea:
        // List-tile style view, shows the file_name, progress (can be shared with Progress
        // Engine-progress is atomic)
        // Expose a button to cancel the download -> send this information to the controller &
        // downloadEngine will take care of things.
        // Show each as a deterministic progress bar
        // Voila.
    }

    fn is_pane_closable(&self) -> bool {
        true
    }
}

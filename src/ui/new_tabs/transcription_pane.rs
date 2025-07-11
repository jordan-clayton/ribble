use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::PaneView;
use crate::ui::new_tabs::ribble_pane::RibblePaneId;

#[derive(Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct TranscriptionPane {}

impl PaneView for TranscriptionPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Transcription
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Transcription".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        todo!()
    }

    fn is_pane_closable(&self) -> bool {
        todo!()
    }
}

use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;
use crate::ui::new_tabs::ribble_tab::RibbleTabId;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct TranscriptionTab {}

impl TabView for TranscriptionTab {
    fn tile_id(&self) -> RibbleTabId {
        RibbleTabId::Transcription
    }

    fn tab_title(&self) -> egui::WidgetText {
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

    fn is_tab_closable(&self) -> bool {
        todo!()
    }
}

use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::SampleSink;
use crate::ui::new_tabs::ribble_tab::RibbleTabId;

pub(crate) struct OfflineTab {}

impl TabView for OfflineTab {
    fn tab_id(&self) -> RibbleTabId {
        RibbleTabId::Offline
    }

    fn tab_title(&mut self) -> egui::WidgetText {
        todo!()
    }

    fn pane_ui<S, A>(&mut self, ui: &mut egui::Ui, tile_id: egui_tiles::TileId, controller: RibbleController<A>) -> egui_tiles::UiResponse
    where
        S: SampleSink,
        A: AudioBackend<S>,
    {
        todo!()
    }


    fn is_tab_closable(&self) -> bool {
        true
    }
}

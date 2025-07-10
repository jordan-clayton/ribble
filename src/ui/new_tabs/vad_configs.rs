use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::SampleSink;

pub(crate) struct VadConfigsTab {}

// I don't think this is getti
impl TabView for VadConfigsTab {
    fn tab_title(&self) -> egui::WidgetText {}

    fn pane_ui<S, A>(
        &mut self,
        ui: &mut egui::Ui,
        controller: RibbleController<A>,
    ) -> egui_tiles::UiResponse
    where
        S: SampleSink,
        A: AudioBackend<S>,
    {
        todo!()
    }

    fn is_tab_closable(&self) -> bool {
        todo!()
    }
}


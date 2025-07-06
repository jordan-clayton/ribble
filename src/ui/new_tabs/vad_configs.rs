use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::SampleSink;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;

pub(crate) struct VadConfigsTab {}

impl TabView for VadConfigsTab {
    fn tab_title(&mut self) -> egui::WidgetText {
        todo!()
    }

    fn pane_ui<S, A>(&mut self, ui: &mut egui::Ui, controller: RibbleController<A>) -> egui_tiles::UiResponse
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
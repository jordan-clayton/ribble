use crate::controller::ConsoleMessage;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;
use crate::ui::new_tabs::ribble_tab::RibbleTabId;
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::ArcChannelSink;
use std::sync::Arc;

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ConsoleTab {
    // NOTE: These are shared ConsoleMessages (held in the ConsoleEngine).
    // It's cheaper to clone an Arc, versus String clones.
    #[serde(skip)]
    #[serde(default)]
    message_buffer: Vec<Arc<ConsoleMessage>>,
}

impl Default for ConsoleTab {
    fn default() -> Self {
        Self {
            message_buffer: vec![],
        }
    }
}

impl TabView for ConsoleTab {
    fn tile_id(&self) -> RibbleTabId {
        RibbleTabId::Console
    }

    fn tab_title(&mut self) -> egui::WidgetText {
        "Console".into()
    }

    fn pane_ui<A>(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        controller: RibbleController<A>,
    ) -> egui::Response
    where
        A: AudioBackend<ArcChannelSink<f32>>,
    {
        // Try to read the current messages (non-blocking).
        controller.try_get_current_messages(&mut self.message_buffer);

        // Set the background color
        let visuals = ui.visuals();
        let bg_col = visuals.extreme_bg_color;
        egui::Frame::new().fill(bg_col).show(ui, |ui| {
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.with_layout(
                        egui::Layout::top_down(egui::Align::LEFT).with_cross_justify(true),
                        |ui| {
                            for msg in self.message_buffer.iter() {
                                ui.label(msg.to_console_text(visuals));
                            }
                        },
                    );
                });
        });

        let pane_id = egui::Id::from("console_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // Add a context menu to make this close-able.
        resp.context_menu(|ui| {
            let mut should_close = false;
            if ui
                .selectable_value(&mut should_close, true, "Close tab.")
                .clicked()
            {
                if should_close {
                    todo!("HANDLE CLOSING THE PANE");
                }
                ui.close_menu();
            };
        });

        resp
    }

    fn is_tab_closable(&self) -> bool {
        true
    }
}

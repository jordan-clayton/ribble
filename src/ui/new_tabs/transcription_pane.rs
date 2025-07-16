use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::PaneView;
use crate::ui::new_tabs::ribble_pane::RibblePaneId;

#[derive(Copy, Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
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
        _tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        let bg_col = ui.visuals().extreme_bg_color;

        let transcription_snapshot = controller.read_transcription_snapshot();

        let control_phrase = controller.read_latest_control_phrase();

        let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
        let header_width = ui.max_rect().width();
        let desired_size = egui::Vec2::new(header_width, header_height);
        let layout = egui::Layout::left_to_right(egui::Align::Center)
            .with_main_justify(true)
            .with_main_wrap(true);

        // TODO: Logic for copying to clipboard + exporting a textfile.

        egui::Frame::default().show(ui, |ui| {
            ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                ui.heading("Transcription:");
                // NOTE: this should aaaactually probably cache it instead,
                // Migrate this to a match to strip out the debug messages.
                ui.label(control_phrase.to_string());
            });

            egui::Frame::default().fill(bg_col).show(ui, |ui| {
                egui::ScrollArea::both()
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        // Show the full transcription state first.
                        ui.monospace(transcription_snapshot.confirmed());
                        // Then print the segment buffer.

                        for segment in transcription_snapshot.string_segments().iter() {
                            ui.monospace(segment);
                        }
                    })
            });
        });

        let pane_id = egui::Id::new("transcription_pane");
        ui.interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab)
    }

    // NOTE: if this makes sense to close, change this.
    // But it seems a little illogical.
    fn is_pane_closable(&self) -> bool {
        false
    }
}

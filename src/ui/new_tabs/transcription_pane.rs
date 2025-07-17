use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::PaneView;
use crate::ui::new_tabs::ribble_pane::RibblePaneId;

#[derive(Copy, Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct TranscriptionPane {}

// Clipboard: https://unicodeplus.com/U+1F4CB
const COPY_ICON: &'static str = "📋";
// Floppy Disk: https://unicodeplus.com/U+1F4BE
const SAVE_ICON: &'static str = "💾";

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
        let transcriber_running = controller.transcriber_running();

        let control_phrase = controller.read_latest_control_phrase();

        let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
        let header_width = ui.max_rect().width();
        let desired_size = egui::Vec2::new(header_width, header_height);
        let layout = egui::Layout::left_to_right(egui::Align::Center)
            .with_main_justify(true)
            .with_main_wrap(true);

        // NOTE: It might be wise to implement a "transcription_is_empty()" or similar on TranscriptionSnapshot
        let transcription_empty = transcription_snapshot.confirmed().is_empty()
            && transcription_snapshot.string_segments().is_empty();

        egui::Frame::default().show(ui, |ui| {
            ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                ui.heading("Transcription:");
                // NOTE: this should aaaactually probably cache it instead,
                // Migrate this to a match to strip out the debug messages.
                ui.label(control_phrase.to_string());

                ui.add_enabled_ui(!(transcriber_running || transcription_empty), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button(COPY_ICON).clicked() {
                            let full_transcription =
                                transcription_snapshot.as_ref().clone().into_string();
                            ui.ctx().copy_text(full_transcription);
                            // TODO: send toast "Copied to clipboard."
                        }

                        // SAVE BUTTON
                        if ui.button(SAVE_ICON).clicked() {
                            let file_dialog = rfd::FileDialog::new()
                                .add_filter("txt", &["txt"])
                                .set_directory(controller.base_dir());

                            if let Some(out_path) = file_dialog.save_file() {
                                controller.save_transcription(out_path);
                                // TODO: send toast "Saving file"
                            }
                        }
                    });
                });
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

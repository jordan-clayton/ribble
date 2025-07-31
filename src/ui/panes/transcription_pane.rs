use crate::controller::ribble_controller::RibbleController;
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::{PaneView, PANE_INNER_MARGIN};
use egui_notify::Toast;

#[derive(Copy, Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct TranscriptionPane {}

// Clipboard: https://unicodeplus.com/U+1F4CB
const COPY_ICON: &str = "📋";
// Floppy Disk: https://unicodeplus.com/U+1F4BE
const SAVE_ICON: &str = "💾";

// TODO: determine a reasonable button size.
const HEADING_BUTTON_SIZE: f32 = 16.0;

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
        // Since this pane should -never- close, don't expose a way to do so.
        _should_close: &mut bool,
        controller: RibbleController,
    ) -> egui::Response {
        let transcription_background_color = ui.visuals().code_bg_color;
        let header_color = ui.visuals().panel_fill;

        let transcription_snapshot = controller.read_transcription_snapshot();
        let transcriber_running = controller.transcriber_running();

        let control_phrase = controller.read_latest_control_phrase();


        // NOTE: It might be wise to implement a "transcription_is_empty()" or similar on TranscriptionSnapshot
        let transcription_empty = transcription_snapshot.confirmed().is_empty()
            && transcription_snapshot.string_segments().is_empty();


        // TODO: this might not work just yet - test out and remove this todo if it's right.
        // Create a (hopefully) lower-priority pane-sized interaction hitbox
        let pane_id = egui::Id::new("transcription_pane");
        // NOTE: This might fix things if it's an interact_bg and not "interact"
        let resp = ui.interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // Use an outer frame to set the total pane margins.
        // NOTE: the margins on these might be too thick.
        // This -will- show the default central panel background and may need some TLC.
        egui::Frame::default().inner_margin(PANE_INNER_MARGIN).show(ui, |ui| {
            // These could be Panels, but since they're not resizeable, these two frames basically
            // just do the same thing.
            egui::Frame::default().fill(header_color).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.columns_const(|[col1, col2]| {
                        col1.vertical_centered_justified(|ui| {
                            // This code needs to be duplicated or be a tuple-closure
                            // -> The calculation needs to be relative to the columns.
                            let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
                            let header_width = ui.max_rect().width();
                            let desired_size = egui::Vec2::new(header_width, header_height);
                            let layout = egui::Layout::left_to_right(egui::Align::Center);
                            ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                ui.heading("Transcription:");
                                // TODO: Remember to strip out debug messages in release mode in TranscriberEngine.
                                ui.label(control_phrase.to_string());
                            });
                        });

                        col2.vertical_centered_justified(|ui| {
                            let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
                            let header_width = ui.max_rect().width();
                            let desired_size = egui::Vec2::new(header_width, header_height);
                            let layout = egui::Layout::right_to_left(egui::Align::Center);
                            ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                // Disable the UI if there's no transcription
                                ui.add_enabled_ui(!(transcriber_running || transcription_empty), |ui| {
                                    // NOTE: This might cause lag with long transcriptions
                                    // If that's the case, spawn a short-lived thread to perform the string join.
                                    let copy_button = egui::RichText::new(COPY_ICON).size(HEADING_BUTTON_SIZE);
                                    let copy_tooltip = |ui: &mut egui::Ui| {
                                        ui.style_mut().interaction.selectable_labels = true;
                                        ui.label("Copy to clipboard.");
                                    };
                                    if ui.button(copy_button)
                                        .on_hover_ui(copy_tooltip)
                                        .on_disabled_hover_ui(copy_tooltip)
                                        .clicked() {
                                        //NOTE: this
                                        let full_transcription =
                                            transcription_snapshot.as_ref().clone().into_string();
                                        ui.ctx().copy_text(full_transcription);
                                        let toast = Toast::info("Copied to Clipboard");
                                        controller.send_toast(toast);
                                    }

                                    let save_button = egui::RichText::new(SAVE_ICON).size(HEADING_BUTTON_SIZE);
                                    let save_tooltip = |ui: &mut egui::Ui| {
                                        ui.style_mut().interaction.selectable_labels = true;
                                        ui.label("Save transcription.");
                                    };
                                    if ui.button(save_button)
                                        .on_hover_ui(save_tooltip)
                                        .on_disabled_hover_ui(save_tooltip)
                                        .clicked() {
                                        // TODO: support for other file formats (markdown, etc.)
                                        // At the moment, the transcription -only- outputs non Diarized text
                                        // And no timestamps (for offline transcription).
                                        // If/when timestamps/other metadata exists and it becomes relevant to support
                                        // other filetimes, do so.
                                        let file_dialog = rfd::FileDialog::new()
                                            .add_filter("txt", &["txt"])
                                            .set_directory(controller.base_dir());

                                        if let Some(out_path) = file_dialog.save_file() {
                                            controller.save_transcription(out_path);
                                            let toast = Toast::info("Saving file");
                                            controller.send_toast(toast);
                                        }
                                    }
                                });
                            }).response.on_hover_cursor(egui::CursorIcon::Default);
                        })
                    });
                });

                // Expect this frame to have the correct cursor when hovering over the text.
                egui::Frame::default().fill(transcription_background_color).show(ui, |ui| {
                    egui::ScrollArea::vertical()
                        // Turn off auto-shrink
                        .auto_shrink([false; 2])
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
        });


        resp
    }

    // NOTE: if this makes sense to close, change this.
    // But it seems a little illogical.
    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

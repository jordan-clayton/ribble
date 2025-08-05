use crate::controller::ribble_controller::RibbleController;
use crate::controller::CompletedRecordingJobs;
use crate::ui::{GRID_ROW_SPACING_COEFF, MODAL_HEIGHT_PROPORTION, PANE_INNER_MARGIN};
use egui::{Align, Frame, Grid, Id, Layout, Modal, ScrollArea, Sense, Ui, UiBuilder, Vec2};
use std::sync::Arc;

pub(in crate::ui) fn build_recording_modal<F>(ui: &mut Ui, modal_id_salt: &str, modal_grid_salt: &str, controller: &RibbleController,
                                              recordings: &[(Arc<str>, CompletedRecordingJobs)], mut on_tap: F) -> egui::ModalResponse<()>
where
    F: FnMut(Arc<str>),
{
    let modal_id = Id::new(modal_id_salt);
    Modal::new(modal_id).show(ui.ctx(), |ui| {
        let height = ui.ctx().screen_rect().height() * MODAL_HEIGHT_PROPORTION;
        ui.set_max_height(height);
        Frame::default().inner_margin(PANE_INNER_MARGIN).show(ui, |ui| {
            ui.columns_const(|[col1, col2]| {
                col1.vertical_centered_justified(|ui| {
                    ui.horizontal(|ui| {
                        ui.heading("Previous recordings:");
                    });
                });
                col2.vertical_centered_justified(|ui| {
                    let desired_size = Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
                    let layout = Layout::right_to_left(Align::Center);
                    ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                        if ui.button("Clear recordings").clicked() {
                            controller.clear_recording_cache();
                        }
                    });
                });
            });


            let gap_space = ui.spacing().interact_size.y;
            ui.add_space(gap_space);
            ScrollArea::vertical().show(ui, |ui| {
                Grid::new(modal_grid_salt).num_columns(1)
                    .striped(true)
                    .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                    .show(ui, |ui| {
                        let len = recordings.len();
                        for (i, (file_name, recording)) in recordings.iter().enumerate() {
                            let heading_text = format!("Recording {}", len - i);
                            let body_text = {
                                let secs = recording.total_duration().as_secs();
                                let seconds = secs % 60;
                                let minutes = (secs / 60) % 60;
                                let hours = (secs / 60) / 60;

                                // This is in bytes.
                                let file_size_estimate = recording.file_size_estimate();
                                let size_text = match unit_prefix::NumberPrefix::binary(file_size_estimate as f32) {
                                    unit_prefix::NumberPrefix::Standalone(number) => format!("{number:.0} B"),
                                    unit_prefix::NumberPrefix::Prefixed(prefix, number) => format!("{number:.2} {prefix}B"),
                                };

                                format!("Total time: {hours}:{minutes}:{seconds} | Approx size: {size_text}")
                            };

                            let tile_resp = ui.scope_builder(
                                UiBuilder::new().id_salt(i).sense(Sense::click()),
                                |ui| {
                                    let resp = ui.response();
                                    let text_col = ui.visuals().strong_text_color();
                                    let visuals = ui.style().interact(&resp);

                                    Frame::default().stroke(visuals.bg_stroke)
                                        .corner_radius(visuals.corner_radius)
                                        .show(ui, |ui| {
                                            // Fill up the entire left side of the grid (hopefully).
                                            ui.set_width(ui.available_width());
                                            // Disable the "interaction" here to only interact
                                            // with the uibuilder.
                                            ui.add_enabled_ui(false, |ui| {
                                                ui.vertical(|ui| {
                                                    // NOTE: There's a bug in egui, (7367); text overrides are being ignored.
                                                    // instead, use two labels with explicit richtext.
                                                    let heading = egui::RichText::new(heading_text).heading().color(text_col);
                                                    let body = egui::RichText::new(body_text).monospace().color(text_col);
                                                    ui.label(heading);
                                                    ui.label(body);
                                                });
                                            });
                                        });
                                },
                            ).response;

                            if tile_resp.clicked() {
                                on_tap(Arc::clone(file_name));
                            }

                            ui.end_row();
                        }
                    })
            });
        });
    })
}
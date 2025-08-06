use crate::controller::ribble_controller::RibbleController;
use crate::controller::FileDownload;
use crate::ui::panes::ribble_pane::{PaneView, RibblePaneId};
use crate::ui::GRID_ROW_SPACING_COEFF;
use crate::ui::PANE_INNER_MARGIN;
use irox_egui_extras::progressbar::ProgressBar;
use unit_prefix::NumberPrefix;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub(in crate::ui) struct DownloadsPane {
    #[serde(skip)]
    #[serde(default)]
    current_downloads: Vec<(usize, FileDownload)>,
}

// https://unicodeplus.com/U+1F5D9 -> "X" (Cancellation glyph)
const CANCELLATION_X: &str = "🗙";

impl PaneView for DownloadsPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Downloads
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Downloads".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        should_close: &mut bool,
        controller: RibbleController,
    ) -> egui::Response {
        controller.try_get_current_downloads(&mut self.current_downloads);
        if !self.current_downloads.is_empty() {
            ui.ctx().request_repaint();
        }

        let panel_col = ui.visuals().panel_fill;

        let pane_id = egui::Id::new("downloads_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        egui::Frame::default().inner_margin(PANE_INNER_MARGIN).fill(panel_col).show(ui, |ui| {
            ui.heading("Downloads:");
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    let layout = egui::Layout::right_to_left(egui::Align::Center);
                    let min_row_height = ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF;

                    egui::Grid::new("downloads_grid")
                        .num_columns(1)
                        .striped(true)
                        .min_row_height(min_row_height)
                        .show(ui, |ui| {
                            let desired_size = egui::Vec2::new(ui.available_width(), min_row_height);

                            for (download_id, download) in self.current_downloads.iter() {
                                let download_progress = download.progress();

                                let current_bytes = download_progress.current_position();
                                let total_size = download_progress.total_size();

                                let cur_bytes_text = NumberPrefix::binary(current_bytes as f32);
                                let total_bytes_text = NumberPrefix::binary(total_size as f32);
                                let bytes_format = match (cur_bytes_text, total_bytes_text) {
                                    (NumberPrefix::Standalone(cur), NumberPrefix::Standalone(tot)) => format!("{cur:.2}/{tot:.2} B"),
                                    (NumberPrefix::Standalone(cur), NumberPrefix::Prefixed(prefix, tot)) => format!("{cur:02} B/{tot:.2} {prefix}B"),
                                    (NumberPrefix::Prefixed(c_pref, cur), NumberPrefix::Prefixed(t_pref, tot)) => format!("{cur:.2} {c_pref}B/{tot:.2} {t_pref}B"),
                                    _ => unreachable!("Total size should never be less than current bytes. Cur: {current_bytes}, Tot: {total_size}"),
                                };

                                ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                    if ui.button(CANCELLATION_X)
                                        .on_hover_text("Cancel download.")
                                        .clicked() {
                                        // NOTE: at the moment, this is a read-blocking method.
                                        // The contention should be minimal, but if there's any jank,
                                        // run the action on a short-lived background thread instead.
                                        controller.abort_download(*download_id);
                                    }
                                    let mut pb = ProgressBar::new(download_progress.current_progress())
                                        .desired_width(ui.available_width())
                                        .text_left(download.name().to_string())
                                        .text_right(bytes_format);
                                    pb.animate = true;
                                    ui.add(pb);
                                });
                                ui.end_row();
                            }
                        });
                });
        });


        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            ui.selectable_value(should_close, self.is_pane_closable(), "Close pane");
        });

        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

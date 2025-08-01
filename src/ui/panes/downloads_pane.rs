use crate::controller::ribble_controller::RibbleController;
use crate::controller::FileDownload;
use crate::ui::panes::ribble_pane::{PaneView, RibblePaneId};
use crate::ui::panes::PANE_INNER_MARGIN;
use crate::ui::GRID_ROW_SPACING_COEFF;
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
                    let grid_width = ui.available_width();
                    let spacing = ui.spacing().item_spacing.x;
                    let button_size = ui.spacing().interact_size.x;

                    // This is a "best-effort" sort of deal
                    // TODO: test to make sure the size is mostly correct to size the grid properly.
                    // The pb should fill the space.
                    // If that doesn't work, try to come up with a ui.vertical/layout solution
                    let pb_width = grid_width - spacing - button_size;

                    // NOTE: a left-to-right solution like User-preferences would be much simpler
                    // TODO: if this is not working well, test out egui_extras::Table
                    // Alternatively this might be able to be done with two ui.layouts:
                    // one left to right, one right to left.
                    // Not sure just yet.
                    egui::Grid::new("downloads_grid")
                        .num_columns(2)
                        .striped(true)
                        .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                        .min_col_width(grid_width)
                        .show(ui, |ui| {
                            for (download_id, download) in self.current_downloads.iter() {
                                let download_progress = download.progress();

                                let current_bytes = download_progress.current_position();
                                let total_size = download_progress.total_size();

                                let cur_bytes_text = NumberPrefix::binary(current_bytes as f32);
                                let total_bytes_text = NumberPrefix::binary(total_size as f32);
                                let bytes_format = match (cur_bytes_text, total_bytes_text) {
                                    (NumberPrefix::Standalone(cur), NumberPrefix::Standalone(tot)) => format!("{cur}/{tot} B"),
                                    (NumberPrefix::Standalone(cur), NumberPrefix::Prefixed(prefix, tot)) => format!("{cur} B/{tot} {prefix}B"),
                                    (NumberPrefix::Prefixed(c_pref, cur), NumberPrefix::Prefixed(t_pref, tot)) => format!("{cur} {c_pref}B/{tot} {t_pref}B"),
                                    _ => unreachable!("Total size should never be less than current bytes. Cur: {current_bytes}, Tot: {total_size}"),
                                };


                                let mut pb = ProgressBar::new(download_progress.current_progress())
                                    .desired_width(pb_width)
                                    .text_left(download.name().to_string())
                                    .text_right(bytes_format);
                                pb.animate = true;
                                ui.add(pb);

                                let layout = egui::Layout::right_to_left(egui::Align::Center);
                                let desired_size = ui.spacing().interact_size;

                                // This will automatically allocate for "at least" the button,
                                // and will do a little more if interact_size is slightly larger.
                                // Expect this to result in what looks like a "full-justify"
                                // where the PB is on the left, and the close button is on the right.
                                ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                    if ui.button(CANCELLATION_X).clicked() {
                                        // NOTE: at the moment, this is a read-blocking method.
                                        // The contention should be minimal, but if there's any jank,
                                        // run the action on a short-lived background thread instead.
                                        controller.abort_download(*download_id);
                                    }
                                });

                                ui.end_row();
                            }
                        });
                });
        });


        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            ui.selectable_value(should_close, self.is_pane_closable(), "Close tab");
        });

        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

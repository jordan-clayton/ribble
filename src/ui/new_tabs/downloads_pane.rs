use crate::controller::FileDownload;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::ribble_pane::{PaneView, RibblePaneId};
use irox_egui_extras::progressbar::ProgressBar;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub(in crate::ui) struct DownloadsPane {
    #[serde(skip)]
    #[serde(default)]
    current_downloads: Vec<(usize, FileDownload)>,
}

// https://unicodeplus.com/U+1F5D9 -> "X" (Cancellation glyph)
const CANCELLATION_X: &'static str = "🗙";

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
        _tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        controller.try_get_current_downloads(&mut self.current_downloads);
        if !self.current_downloads.is_empty() {
            ui.ctx().request_repaint();
        }
        egui::Frame::default().show(ui, |ui| {
            ui.heading("Downloads:");
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    egui::Grid::new("Downloads Grid")
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for (download_id, download) in self.current_downloads.iter() {
                                let download_progress = download.progress();

                                let _current_bytes = download_progress.current_position();
                                let _total_size = download_progress.total_size();
                                let bytes_format = format!("TODO: BYTE SIZE");

                                let mut pb = ProgressBar::new(download_progress.current_progress())
                                    .text_left(download.name().to_string())
                                    .text_right(bytes_format);
                                pb.animate = true;

                                ui.add(pb);
                                if ui.button(CANCELLATION_X).clicked() {
                                    // NOTE: at the moment, this is a blocking method.
                                    // Writers should still get priority, but if there's any jank,
                                    // run the action on a short-lived background thread instead.
                                    //
                                    // TODO: Actually, yes, This should happen on a background thread
                                    // with a flag to prevent grandma clicks.
                                    controller.abort_download(*download_id);
                                }

                                ui.end_row();
                            }
                        });
                });
        });

        let pane_id = egui::Id::new("downloads_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        let mut should_close = false;
        resp.context_menu(|ui| {
            ui.selectable_value(&mut should_close, self.is_pane_closable(), "Close tab.");
        });

        if should_close {
            ui.close();
        }

        resp
    }

    fn is_pane_closable(&self) -> bool {
        true
    }
}

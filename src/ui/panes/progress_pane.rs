use crate::controller::Progress;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::panes::PaneView;
use crate::ui::panes::ribble_pane::RibblePaneId;
use irox_egui_extras::progressbar::ProgressBar;

#[derive(Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProgressPane {
    #[serde(default)]
    #[serde(skip)]
    current_jobs: Vec<Progress>,
}

// NOTE: if the progress bar impl thus far is insufficent (and requires custom painting/gradients),
// Factor out a widget and just paint it.

impl PaneView for ProgressPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Progress
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Progress".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        // Get the current list of jobs.
        controller.try_get_current_jobs(&mut self.current_jobs);

        // If there are progress bars, request a repaint.
        if !self.current_jobs.is_empty() {
            ui.ctx().request_repaint();
        }

        egui::Frame::default().show(ui, |ui| {
            ui.heading("Progress:");
            egui::ScrollArea::both()
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    egui::Grid::new("progress_grid")
                        .num_columns(1)
                        .striped(true)
                        .show(ui, |ui| {
                            for prog_job in self.current_jobs.iter() {
                                let mut pb = match prog_job.progress() {
                                    Some(progress) => ProgressBar::new(progress)
                                        .text_left(prog_job.job_name().to_string())
                                        .text_right(format!("{}%", progress * 100f32)),
                                    None => ProgressBar::indeterminate()
                                        .text_left(prog_job.job_name().to_string()),
                                };

                                pb.animate = true;
                                ui.add(pb);
                                ui.end_row();
                            }
                        });
                });
        });

        let pane_id = egui::Id::new("progress_pane");
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

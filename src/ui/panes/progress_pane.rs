use crate::controller::ribble_controller::RibbleController;
use crate::controller::Progress;
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::{PaneView, PANE_INNER_MARGIN};
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
        should_close: &mut bool,
        controller: RibbleController,
    ) -> egui::Response {
        let panel_color = ui.visuals().panel_fill;
        // Get the current list of jobs.
        controller.try_get_current_jobs(&mut self.current_jobs);

        // If there are progress bars, request a repaint.
        if !self.current_jobs.is_empty() {
            ui.ctx().request_repaint();
        }

        let pane_id = egui::Id::new("progress_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // TODO: this requires testing -> check that the bars fill.
        egui::Frame::default().fill(panel_color).inner_margin(PANE_INNER_MARGIN).show(ui, |ui| {
            ui.heading("Progress:");
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    // NOTE: since grids don't automatically expand to remaining space, this needs
                    // to be calculated manually.
                    // TODO: check this and see if the width should be queried in the grid method.
                    // This should work as anticipated though.
                    let grid_width = ui.available_width();
                    egui::Grid::new("progress_grid")
                        .num_columns(1)
                        .min_col_width(grid_width)
                        .striped(true)
                        .show(ui, |ui| {
                            // NOTE: this will need to be tested -> it is most likely to work though.
                            for prog_job in self.current_jobs.iter() {
                                let mut pb = match prog_job.progress() {
                                    Some(progress) => ProgressBar::new(progress)
                                        .desired_width(grid_width)
                                        .text_left(prog_job.job_name().to_string())
                                        .text_right(format!("{}%", progress * 100f32)),
                                    None => ProgressBar::indeterminate()
                                        .desired_width(grid_width)
                                        .text_left(prog_job.job_name().to_string()),
                                };

                                pb.animate = true;
                                ui.add(pb);
                                ui.end_row();
                            }
                        }).response.on_hover_cursor(egui::CursorIcon::Default);
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

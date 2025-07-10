use crate::controller::Progress;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;
use crate::ui::new_tabs::ribble_tab::RibbleTabId;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ProgressTab {
    #[serde(default)]
    #[serde(skip)]
    current_jobs: Vec<Progress>,
}

impl Default for ProgressTab {
    fn default() -> Self {
        Self {
            current_jobs: vec![],
        }
    }
}

impl TabView for ProgressTab {
    fn tile_id(&self) -> RibbleTabId {
        RibbleTabId::Progress
    }

    fn tab_title(&self) -> egui::WidgetText {
        "Progress".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        // Get the current list of jobs.
        controller.try_get_current_jobs(&mut self.current_jobs);
        let spacing = ui.spacing().interact_size.y;
        let len = self.current_jobs.len();

        // TODO: determine whether this should have a different background.
        // If so, stick this into a frame.
        egui::ScrollArea::both()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for (i, prog_job) in self.current_jobs.iter().enumerate() {
                    // TODO: test and see if top spacing is also required or not.
                    // If spacing/separators aren't needed, just iterate the jobs.
                    match prog_job {
                        Progress::Determinate { job_name, progress } => {
                            todo!("DRAW");
                        }
                        Progress::Indeterminate { job_name } => {
                            todo!("DRAW");
                        }
                    }

                    ui.add_space(spacing);
                    if i != len - 1 {
                        ui.separator();
                    }
                }
            });

        let pane_id = egui::Id::from("progress_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // Add a context menu to make this closable.
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

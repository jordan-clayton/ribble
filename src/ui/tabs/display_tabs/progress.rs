use std::collections::HashSet;

use egui::{Grid, ProgressBar, ScrollArea, Spinner, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController, ui::tabs::tab_view,
    utils::progress::Progress,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProgressTab {
    title: String,
    #[serde(skip)]
    jobs: HashSet<Progress>,
}

impl ProgressTab {
    pub fn new() -> Self {
        let jobs = HashSet::new();
        Self {
            title: String::from("Progress"),
            jobs,
        }
    }

    fn progress_widget(ui: &mut Ui, progress: &Progress) {
        let p = progress.progress();
        let total_size = progress.total_size();
        match total_size {
            0 => {
                ui.vertical(|ui| {
                    ui.label(progress.job_name());
                    ui.add(Spinner::new())
                });
            }
            _ => {
                let percent = (p as f32) / (total_size as f32);
                ui.vertical(|ui| {
                    ui.label(progress.job_name());
                    ui.add(ProgressBar::new(percent).show_percentage().animate(true));
                });
            }
        }
    }
}

impl Default for ProgressTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for ProgressTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let Self { title: _, jobs } = self;

        // Get any new progress.
        while let Ok(progress) = controller.recv_progress() {
            jobs.replace(progress);
        }

        let mut finished_jobs = vec![];
        ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            Grid::new("progress")
                .striped(true)
                .num_columns(1)
                .show(ui, |ui| {
                    for job in jobs.clone() {
                        // Draw it.
                        Self::progress_widget(ui, &job);
                        ui.end_row();

                        // Check for removal
                        if job.finished() {
                            finished_jobs.push(job)
                        }
                    }
                });
        });

        for job in finished_jobs {
            jobs.remove(&job);
        }
    }

    fn context_menu(
        &mut self,
        _ui: &mut Ui,
        _controller: &mut WhisperAppController,
        _surface: SurfaceIndex,
        _node: NodeIndex,
    ) {
    }

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

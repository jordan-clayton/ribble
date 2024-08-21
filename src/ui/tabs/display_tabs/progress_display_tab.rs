use std::collections::HashSet;

use egui::{Grid, ProgressBar, ScrollArea, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    ui::tabs::tab_view, utils::progress::Progress, whisper_app_context::WhisperAppController,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ProgressDisplayTab {
    title: String,
    #[serde(skip)]
    jobs: HashSet<Progress>,
}

impl ProgressDisplayTab {
    pub fn new() -> Self {
        let jobs = HashSet::new();
        Self {
            title: String::from("Progress"),
            jobs,
        }
    }

    fn progress_widget(ui: &mut Ui, progress: &Progress) {
        let percent = (progress.progress() as f32) / (progress.total_size() as f32);
        ui.vertical(|ui| {
            ui.label(progress.job_name());
            ui.add(ProgressBar::new(percent).show_percentage().animate(true));
        });
    }
}

impl Default for ProgressDisplayTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for ProgressDisplayTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let Self { title: _, jobs } = self;

        // Get any new progress.
        let message = controller.recv_progress();

        if let Ok(progress) = message {
            jobs.insert(progress);
        }

        let mut finished_jobs = vec![];
        ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
            Grid::new("progress").striped(true).show(ui, |ui| {
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

    // TODO: Determine if actually required.
    fn context_menu(
        &mut self,
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        surface: SurfaceIndex,
        node: NodeIndex,
    ) {
    }

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

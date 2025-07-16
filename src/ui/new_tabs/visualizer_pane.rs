use crate::controller::NUM_VISUALIZER_BUCKETS;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::PaneView;
use crate::ui::new_tabs::ribble_pane::RibblePaneId;
use crate::ui::widgets::soundbar::soundbar;

const SMOOTHING_CONSTANT: f32 = 8.0;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct VisualizerPane {
    #[serde(skip)]
    #[serde(default)]
    visualizer_buckets: [f32; NUM_VISUALIZER_BUCKETS],
    #[serde(skip)]
    #[serde(default)]
    presentation_buckets: [f32; NUM_VISUALIZER_BUCKETS],
}

impl PaneView for VisualizerPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Visualizer
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Visualizer".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        // If this is painting, a visualizer is in view, so set the visualizer to true to continue
        // processing audio data if it's coming in.
        controller.set_visualizer_visibility(true);
        // Check for audio running (otherwise, smooth to 0)
        let audio_running = controller.realtime_running() || controller.recorder_running();
        // If the audio is running (and thus the VisualizerEngine is active), try to read the buffer.
        if audio_running {
            controller.try_read_visualization_buffer(&mut self.visualizer_buckets);
            // Otherwise, just zero out the visualizer bucket.
        } else {
            self.visualizer_buckets.iter_mut().for_each(|v| *v = 0.0);
        }

        // Smooth the buffer to prevent the (unintended) jumpiness.
        let dt = ui.ctx().input(|i| i.stable_dt);
        smoothing(&self.visualizer_buckets, &mut self.presentation_buckets, dt);

        egui::Frame::default().show(ui, |ui| {
            // TODO: this needs to change to a gradient.
            let theme: catppuccin_egui::Theme = catppuccin_egui::MOCHA;
            let rect = ui.max_rect();
            // The new implementation is in the widgets module.
            ui.add(soundbar(rect, &self.presentation_buckets, theme));
        });

        let pane_id = egui::Id::new("visualizer_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // Add a context menu to make this close-able.
        resp.context_menu(|ui| {
            // TODO: ADD MENU ITEMS FOR CHANGING THE VISUALISER STYLE
            // THEN A SEPARATOR
            // THEN THE CLOSE BUTTON

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

    fn is_pane_closable(&self) -> bool {
        true
    }
    fn on_pane_close(&mut self, controller: RibbleController) -> bool {
        controller.set_visualizer_visibility(false);
        true
    }
}

fn smoothing(target: &[f32], current: &mut [f32], dt: f32) {
    assert_eq!(target.len(), current.len());
    for i in 0..target.len() {
        current[i] = current[i] + (target[i] - current[i]) * SMOOTHING_CONSTANT * dt;
    }
}

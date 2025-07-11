use crate::controller::NUM_VISUALIZER_BUCKETS;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::PaneView;
use crate::ui::new_tabs::ribble_pane::RibblePaneId;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct VisualizerPane {
    #[serde(skip)]
    #[serde(default)]
    visualizer_buckets: [f32; NUM_VISUALIZER_BUCKETS],
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
        tile_id: egui_tiles::TileId,
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

        // Smooth out the values and write into the presentation buckets.
        // This is most likely going to be a 1-instruction operation if SIMD
        // - and checking via memcpmp for a zeroed buffer isn't going to really save much.
        smoothing(&self.visualizer_buckets, &mut self.presentation_buckets);

        // Basic idea:
        // Get a sensing response for the size of the rect (to get mouse delta).
        // Frame with some sort of background color (maybe default?)
        // Decide on the number of buckets to actually show based on the width of the window.
        // - Instead of deciding on the width based on window size, make it a fixed proportion
        //   and calculate the number of buckets to support that width.
        // - Also incorporate padding (fixed proportion).

        // - Sample the buckets (linearly interpolate) to get an amplitude
        // - (Change the lerping hitbox --> it's currently weird)
        // - increase the height based on the proximity to the mouse -> use a wider falloff
        // - also, do it from the closest rect corner, not the center.

        todo!("FINISH DRAWING.");
        let pane_id = egui::Id::from("visualizer_pane");
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

// TODO: figure out smoothing business.
// This might actually need double buffering.
fn smoothing(src: &[f32], dst: &mut [f32]) {
    assert_eq!(src.len(), dst.len());
    todo!("Smoothing");
}

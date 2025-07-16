use crate::controller::ribble_controller::RibbleController;
use crate::controller::{AnalysisType, NUM_VISUALIZER_BUCKETS, RotationDirection};
use crate::ui::new_tabs::PaneView;
use crate::ui::new_tabs::ribble_pane::RibblePaneId;
use crate::ui::widgets::soundbar::soundbar;
use crate::utils::preferences::RibbleAppTheme;
use egui_colorgradient::ColorInterpolator;
use std::fmt::Debug;
use strum::IntoEnumIterator;

const SMOOTHING_CONSTANT: f32 = 8.0;

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct VisualizerPane {
    #[serde(skip)]
    #[serde(default)]
    visualizer_buckets: [f32; NUM_VISUALIZER_BUCKETS],
    #[serde(skip)]
    #[serde(default)]
    presentation_buckets: [f32; NUM_VISUALIZER_BUCKETS],
    // NOTE: this is the only view that's using the color_interpolator
    // If that changes, moved to a shared module in the kernel or otherwise and access via the
    // controller.
    #[serde(skip)]
    #[serde(default)]
    color_interpolator: Option<ColorInterpolator>,
    #[serde(skip)]
    #[serde(default)]
    current_theme: RibbleAppTheme,
}

impl Clone for VisualizerPane {
    fn clone(&self) -> Self {
        // TODO: fix this
        Self {
            visualizer_buckets: self.visualizer_buckets.clone(),
            presentation_buckets: self.presentation_buckets.clone(),
            color_interpolator: self.current_theme.color_interpolator(),
            current_theme: self.current_theme,
        }
    }
}

// For some reason, the egui_colorgradient structs don't implement Debug, despite all inner fields
// implementing debug.
impl Debug for VisualizerPane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VisualizerPane")
            .field("visualizer_buckets", &self.visualizer_buckets)
            .field("presentation_buckets", &self.presentation_buckets)
            // NOTE: there's no way to interrogate the private fields of the color interpolator.
            // If it becomes imperative to Debug-Display this, fork the repo or make a wrapper
            // struct.
            .field("color_interpolator", &self.color_interpolator.is_some())
            .field("current_theme", &self.current_theme)
            .finish()
    }
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

        // Check the theme to determine whether or not the gradient needs to be swapped.
        let theme = controller.read_user_preferences().system_theme();
        if theme != self.current_theme || self.color_interpolator.is_none() {
            self.current_theme = theme;
            self.color_interpolator = match theme.color_interpolator() {
                Some(interp) => Some(interp),
                None => match ui.ctx().system_theme() {
                    Some(theme) => match theme {
                        egui::Theme::Dark => RibbleAppTheme::Mocha.color_interpolator(),
                        egui::Theme::Light => RibbleAppTheme::Latte.color_interpolator(),
                    },
                    None => RibbleAppTheme::Mocha.color_interpolator(),
                },
            };
        }

        debug_assert!(
            self.color_interpolator.is_some(),
            "Failed to set color interpolator."
        );

        let mut visualizer_type = controller.get_visualizer_analysis_type();

        let inner = egui::Frame::default().show(ui, |ui| {
            let color_interpolator = self
                .color_interpolator
                .as_ref()
                .expect("The color interpolator is only None at construction.");

            // Add the analysis type header
            ui.heading(visualizer_type.as_ref());
            let rect = ui.max_rect();
            // The new implementation is in the widgets module.
            ui.add(soundbar(
                rect,
                &self.presentation_buckets,
                color_interpolator,
            ));
        });

        if inner.response.has_focus() {
            let (left, right) = ui.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                )
            });

            if left & (left ^ right) {
                controller.rotate_visualizer_type(RotationDirection::CounterClockwise);
            }

            if right & (left ^ right) {
                controller.rotate_visualizer_type(RotationDirection::Clockwise);
            }
        }

        let pane_id = egui::Id::new("visualizer_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // Add a context menu to make this close-able.
        // If this is no longer close-able, the close button will just nop.
        resp.context_menu(|ui| {
            for analysis_type in AnalysisType::iter() {
                if ui
                    .selectable_value(&mut visualizer_type, analysis_type, analysis_type.as_ref())
                    .clicked()
                {
                    controller.set_visualizer_analysis_type(visualizer_type);
                }
            }

            ui.separator();
            let mut should_close = false;
            if ui
                .selectable_value(&mut should_close, self.is_pane_closable(), "Close tab.")
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

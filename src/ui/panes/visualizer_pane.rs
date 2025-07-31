use crate::controller::ribble_controller::RibbleController;
use crate::controller::{AnalysisType, RotationDirection, NUM_VISUALIZER_BUCKETS};
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::{PaneView, PANE_INNER_MARGIN};
use crate::ui::widgets::soundbar::soundbar;
use crate::utils::preferences::RibbleAppTheme;
use egui_colorgradient::ColorInterpolator;
use std::fmt::Debug;
use strum::IntoEnumIterator;

const SMOOTHING_CONSTANT: f32 = 8.0;

// TODO: DETERMINE WHAT TO DO RE ENUM SIZE
// - Currently, VisualizerPane is 296-800 bytes because the slices are being stored on the stack.
// - This will, most definitely be faster, but RibblePanes then become 296 bytes by default,
//   Which seems a little wasteful for some which are ZST or much smaller.
// - Since VisualizerPane -has- to be mutable and there should be no locking in the UI, the options are as follows:
//      - Do nothing and accept that all panes are 296 bytes.
//      - Use vectors instead of fixed sized slices (can then make the visualizer resolution tweakable)

// I'm not 100% sure what to do here. Stack allocated buffers will be quicker and -very- cache friendly
// This should be more efficient.
// If I move to vectors, the size is ~80-90 bytes, so this will significantly save on memory,
// but add (maybe) noticeable indirection costs.

// RETURN TO THIS AFTER PROFILING FOR MEMORY.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct VisualizerPane {
    #[serde(skip)]
    #[serde(default = "make_buffer")]
    visualizer_buckets: [f32; NUM_VISUALIZER_BUCKETS],
    #[serde(skip)]
    #[serde(default = "make_buffer")]
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
    #[serde(skip)]
    #[serde(default)]
    has_focus: bool,
}

impl Default for VisualizerPane {
    fn default() -> Self {
        Self {
            visualizer_buckets: make_buffer(),
            presentation_buckets: make_buffer(),
            color_interpolator: None,
            current_theme: Default::default(),
            has_focus: false,
        }
    }
}

// Since [f32; 64] doesn't implement default, this has to exist on the benching branch while I test things.
fn make_buffer() -> [f32; NUM_VISUALIZER_BUCKETS] {
    [0.0; NUM_VISUALIZER_BUCKETS]
}


impl Clone for VisualizerPane {
    fn clone(&self) -> Self {
        Self {
            visualizer_buckets: self.visualizer_buckets,
            presentation_buckets: self.presentation_buckets,
            color_interpolator: Some(self.current_theme.color_interpolator().unwrap_or(RibbleAppTheme::Mocha.color_interpolator().unwrap())),
            current_theme: self.current_theme,
            has_focus: self.has_focus,
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
        should_close: &mut bool,
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

        // Check the theme to determine whether the gradient needs to be swapped.
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

        let pane_id = egui::Id::new("visualizer_pane");
        let pane_max_rect = ui.max_rect();
        let resp = ui
            .interact(pane_max_rect, pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        let bg_col = ui.style().visuals.extreme_bg_color;

        egui::Frame::default().inner_margin(PANE_INNER_MARGIN).fill(bg_col).show(ui, |ui| {
            ui.heading(format!("{visualizer_type}:"));
            ui.put(pane_max_rect, |ui: &mut egui::Ui| {
                let color_interpolator = self
                    .color_interpolator
                    .as_ref()
                    .expect("The color interpolator is only None at construction.");

                // The new implementation is in the "widgets" module.
                ui.add(soundbar(
                    pane_max_rect,
                    &self.presentation_buckets,
                    color_interpolator,
                ))
            });
        });

        if resp.clicked() {
            self.has_focus = true;
        }
        if resp.clicked_elsewhere() {
            self.has_focus = false;
        }

        // Once the sense interactions have settled, this may always return true.
        // Check once the response issue has been resolved.
        // NOTE: could also use up (CC) and down (C)
        // -> this should probably change if/when other visualizations (e.g. line plot) are implemented.
        if self.has_focus {
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
            // For closing the pane.
            ui.selectable_value(should_close, self.is_pane_closable(), "Close pane");
        });

        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
    // NOTE: this only gets called on close, so it must be that the pane is closable.
    fn on_pane_close(&mut self, controller: RibbleController) -> bool {
        controller.set_visualizer_visibility(false);
        true
    }
}

pub fn smoothing(target: &[f32], current: &mut [f32], dt: f32) {
    assert_eq!(target.len(), current.len());
    for i in 0..target.len() {
        current[i] = current[i] + (target[i] - current[i]) * SMOOTHING_CONSTANT * dt;
    }
}
pub trait VisualizerPaneTester {
    fn get_buckets(&mut self) -> &mut [f32; NUM_VISUALIZER_BUCKETS];
    fn smoothing(&mut self, dt: f32);
}
impl VisualizerPaneTester for VisualizerPane {
    fn get_buckets(&mut self) -> &mut [f32; NUM_VISUALIZER_BUCKETS] {
        &mut self.visualizer_buckets
    }

    fn smoothing(&mut self, dt: f32) {
        smoothing(&self.visualizer_buckets, &mut self.presentation_buckets, dt);
    }
}

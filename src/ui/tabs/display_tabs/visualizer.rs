use eframe::epaint::text::TextWrapMode;
use egui::{
    CentralPanel, FontId, Frame, RichText, Sense, TextStyle, TopBottomPanel, Ui, WidgetText,
};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::IntoEnumIterator;

use crate::ui::widgets::fft_visualizer::draw_fft;
use crate::utils::audio_analysis::AnalysisType;
use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::{tabs::tab_view, widgets::recording_icon::recording_icon},
    utils::{audio_analysis, constants, preferences},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VisualizerTab {
    title: String,
    #[serde(skip)]
    #[serde(default = "allocate_new_fft_buffer")]
    current: [f32; constants::NUM_BUCKETS],
    #[serde(skip)]
    #[serde(default = "allocate_new_fft_buffer")]
    target: [f32; constants::NUM_BUCKETS],
    // This is to avoid unnecessary calls to clear the array.
    #[serde(skip)]
    target_cleared: bool,
    pub visualize: bool,
}

impl VisualizerTab {
    pub fn new() -> Self {
        Self {
            title: String::from("Visualizer"),
            current: allocate_new_fft_buffer(),
            target: allocate_new_fft_buffer(),
            target_cleared: false,
            visualize: true,
        }
    }
}

impl Default for VisualizerTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for VisualizerTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let Self {
            title: _,
            current,
            target,
            target_cleared,
            visualize,
        } = self;

        controller.set_run_visualizer(*visualize);
        let mut accepting_speech = false;
        let realtime_running = controller.realtime_running();
        let recorder_running = controller.recorder_running();
        let mic_running = realtime_running || recorder_running;

        if mic_running {
            *target_cleared = false;
            // Update to the latest fft data.
            controller.read_fft_buffer(target);
        } else {
            if !*target_cleared {
                // Zero out the tgt array
                clear_array(target);
                *target_cleared = true;
            }
        }

        // Get the frame time.
        let dt = ui.ctx().input(|i| i.stable_dt);
        // Smooth the current position towards tgt
        audio_analysis::smoothing(current, target, dt);

        // Force a repaint if the amplitudes are not zero.
        if current.iter().any(|f| (*f - 0.0) >= f32::EPSILON) {
            ui.ctx().request_repaint();
        }

        let system_theme = controller.get_system_theme();
        let theme = preferences::get_app_theme(system_theme);
        let time_scale = Some(constants::RECORDING_ANIMATION_TIMESCALE);

        // TODO: refactor this -> atomic enum for state + which audio_worker
        TopBottomPanel::top("header")
            .resizable(false)
            .show_inside(ui, |ui| {
                let (icon, msg) = if accepting_speech {
                    (
                        recording_icon(egui::Rgba::from(theme.red), true, time_scale),
                        "Recording in progress.",
                    )
                } else if mic_running {
                    (
                        recording_icon(egui::Rgba::from(theme.green), true, time_scale),
                        "Preparing to record.",
                    )
                } else {
                    (
                        recording_icon(egui::Rgba::from(theme.green), false, time_scale),
                        "Ready to record.",
                    )
                };

                ui.horizontal(|ui| {
                    ui.add(icon);
                    ui.label(msg);
                });
                let space = ui.spacing().item_spacing.y;
                ui.add_space(space);
            });

        let visuals = ui.visuals();
        let bg_col = visuals.extreme_bg_color;
        let frame = Frame::default().fill(bg_col);

        let resp = CentralPanel::default().frame(frame).show_inside(ui, |ui| {
            let analysis_type = controller.get_analysis_type();
            ui.add_space(constants::BLANK_SEPARATOR);
            let header_style = TextStyle::Heading;
            let header_size = ui.text_style_height(&header_style);
            ui.label(RichText::new(analysis_type.to_string()).font(FontId::monospace(header_size)));
            draw_fft(ui, &current, Some(theme));
        });

        let response = resp.response.interact(Sense::click());
        response.context_menu(|ui| {
            ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
            let visualization_functions = AnalysisType::iter();
            let mut current = controller.get_analysis_type();
            for visual in visualization_functions {
                if ui
                    .selectable_value(&mut current, visual, visual.to_string())
                    .clicked()
                {
                    controller.set_analysis_type(visual);
                    ui.close_menu();
                }
            }
        });
        if response.clicked() {
            controller.rotate_analysis_type();
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

fn allocate_new_fft_buffer() -> [f32; constants::NUM_BUCKETS] {
    [0.0; constants::NUM_BUCKETS]
}

fn clear_array(array: &mut [f32]) {
    array.iter_mut().for_each(|f| *f = 0.0);
}

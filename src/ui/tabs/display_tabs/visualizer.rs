use eframe::epaint::text::TextWrapMode;
use egui::{lerp, Align, CentralPanel, FontId, Frame, Layout, RichText, Sense, TextStyle, TopBottomPanel, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::IntoEnumIterator;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::{
        tabs::{display_tabs::display_common::get_header_recording_icon, tab_view},
        widgets::{fft_visualizer::draw_fft, toggle_switch::toggle},
    },
    utils::{
        audio_analysis::AnalysisType,
        constants, preferences,
    },
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VisualizerTab {
    title: String,
    #[serde(skip)]
    #[serde(default = "allocate_new_visualizer_buffer")]
    current: [f32; constants::NUM_BUCKETS],
    #[serde(skip)]
    #[serde(default = "allocate_new_visualizer_buffer")]
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
            current: allocate_new_visualizer_buffer(),
            target: allocate_new_visualizer_buffer(),
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
        let realtime_running = controller.realtime_running();
        let transcription = realtime_running || controller.static_running();
        let recorder_running = controller.recorder_running();
        let mic_running = realtime_running || recorder_running;
        let audio_worker_state = controller.audio_worker_state();
        let visualizer_running = controller.run_visualizer();

        if visualizer_running && mic_running {
            *target_cleared = false;
            // Update to the latest fft data.
            controller.read_visualizer_buffer(target);
        } else {
            if !*target_cleared {
                // Zero out the tgt array
                clear_array(target);
                *target_cleared = true;
            }
        }

        // Get the frame time.
        let dt = ui.ctx().input(|i| i.stable_dt);
        // Smooth the current position towards tgt -- there's no need for the separate fn
        for (i, sample) in current.iter_mut().enumerate() {
            *sample = lerp(*sample..=target[i], dt);
            // Target should never be Nan/Inf -> this is checked for in the VisualizerEngine
        }
        //smoothing(current, target, dt);

        // Force a repaint if the amplitudes are not zero.
        if current.iter().any(|f| *f >= f32::EPSILON) {
            ui.ctx().request_repaint();
        }

        let system_theme = controller.get_system_theme();
        let theme = preferences::get_app_theme(system_theme);

        TopBottomPanel::top("visualizer_header")
            .resizable(false)
            .show_inside(ui, |ui| {
                ui.horizontal(|ui| {
                    let (icon, msg) = get_header_recording_icon(audio_worker_state, transcription, &theme);
                    ui.add(icon);
                    ui.label(msg);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add(toggle(visualize));
                        ui.label("Run Visualizer")
                    });
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
            controller.rotate_analysis_type(true);
        }
    }

    fn context_menu(
        &mut self,
        _ui: &mut Ui,
        _controller: &mut WhisperAppController,
        _surface: SurfaceIndex,
        _node: NodeIndex,
    ) {}

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

fn allocate_new_visualizer_buffer() -> [f32; constants::NUM_BUCKETS] {
    [0.0; constants::NUM_BUCKETS]
}

fn clear_array(array: &mut [f32]) {
    array.iter_mut().for_each(|f| *f = 0.0);
}

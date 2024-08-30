use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use egui::{Button, CentralPanel, Grid, SidePanel, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::{
        tabs::tab_view,
        widgets::{fft_visualizer, recording_icon::recording_icon},
    },
    utils::{constants, preferences, recording},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecordingDisplayTab {
    title: String,
    // For determining whether to run/display the fft.
    visualize: Arc<AtomicBool>,
    #[serde(skip)]
    #[serde(default = "allocate_new_fft_buffer")]
    current: [f32; constants::NUM_BUCKETS],
    #[serde(skip)]
    #[serde(default = "allocate_new_fft_buffer")]
    target: [f32; constants::NUM_BUCKETS],

    // This is to avoid unnecessary calls to clear the array.
    #[serde(skip)]
    target_cleared: bool,
}

impl RecordingDisplayTab {
    pub fn new() -> Self {
        Self {
            title: String::from("Visualizer"),
            visualize: Arc::new(AtomicBool::new(true)),
            current: allocate_new_fft_buffer(),
            target: allocate_new_fft_buffer(),
            target_cleared: false,
        }
    }
}

impl Default for RecordingDisplayTab {
    fn default() -> Self {
        Self::new()
    }
}

impl tab_view::TabView for RecordingDisplayTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Split view:  Visualizer | Buttons: Output path, Visualizer toggle, Start and stop recording, etc.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let Self {
            title: _,
            visualize,
            current,
            target,
            target_cleared,
        } = self;

        let mut run_visualizer = visualize.load(Ordering::Acquire);
        // TODO: figure this out - either msg or similar.
        let mut accepting_speech = false;
        let recorder_running = controller.recorder_running();
        let save_recording_ready = controller.save_recording_ready();

        // Check whether mic is occupied by another process.
        let mic_occupied = controller.audio_running() ^ recorder_running;

        if run_visualizer && recorder_running {
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
        recording::smoothing(current, target, dt);

        // Button panel
        SidePanel::right("recording_panel").show_inside(ui, |ui| {
            ui.add_enabled_ui(!mic_occupied, |ui| {
                Grid::new("inner_recording_panel").striped(true).show(ui, |ui| {
                    // Start recording button
                    if ui.add_enabled(!recorder_running, Button::new("Start Recording")).clicked() {
                        controller.start_recording(visualize.clone(), &ui.ctx().clone());
                        // TODO: remove once proper implemented
                        accepting_speech = true;
                    }

                    ui.end_row();

                    // Stop recording button
                    if ui.add_enabled(recorder_running, Button::new("Stop Recording")).clicked() {
                        controller.stop_recording();
                    }

                    ui.end_row();

                    // Save recording button.
                    if ui.add_enabled(save_recording_ready, Button::new("Save")).clicked() {
                        // Open File dialog at HOME directory, fallback to root.
                        let base_dirs = directories::BaseDirs::new();
                        let dir = if let Some(dir) = base_dirs {
                            dir.home_dir().to_path_buf()
                        } else {
                            PathBuf::from("/")
                        };

                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("wave", &["wav"])
                            .set_directory(dir).save_file() {
                            controller.save_audio_recording(&p);
                        }
                    }
                    ui.end_row();

                    // Run visual toggle
                    if ui.checkbox(&mut run_visualizer, "Run visualizer").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Click to toggle the frequency waveform visualizer. Disable to improve performance.");
                    }).clicked() {
                        visualize.store(run_visualizer, Ordering::Release);
                    }
                    ui.end_row();
                });
            });
        });

        CentralPanel::default().show_inside(ui, |ui| {
            // Visualization.
            // Header
            let system_theme = controller.get_system_theme();
            let theme = preferences::get_app_theme(system_theme);
            let time_scale = Some(constants::RECORDING_ANIMATION_TIMESCALE);

            let (icon, msg) = if accepting_speech {
                (
                    recording_icon(egui::Rgba::from(theme.red), true, time_scale),
                    "Recording in progress.",
                )
            } else if recorder_running {
                (
                    recording_icon(egui::Rgba::from(theme.green), true, time_scale),
                    "Preparing to record.",
                )
            } else if mic_occupied {
                (
                    recording_icon(egui::Rgba::from(theme.yellow), true, time_scale),
                    "Microphone in use.",
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

            ui.separator();

            // FFT visualizer
            fft_visualizer::draw_fft(ui, &current, Some(theme));
        });
    }

    // TODO: determine if actually useful.
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

fn allocate_new_fft_buffer() -> [f32; constants::NUM_BUCKETS] {
    [0.0; constants::NUM_BUCKETS]
}

fn clear_array(array: &mut [f32]) {
    array.iter_mut().for_each(|f| *f = 0.0);
}

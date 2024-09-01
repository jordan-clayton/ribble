use std::path::PathBuf;

use egui::{Button, Checkbox, ComboBox, Grid, ScrollArea, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::IntoEnumIterator;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    utils::{
        constants,
        recorder_configs::{
            BufferSize, Channel, RecorderConfigs, RecordingFormat, SampleRate,
        },
    },
};
use crate::ui::tabs::tab_view;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecordingTab {
    title: String,
    recorder_configs: RecorderConfigs,
}

impl RecordingTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: RecorderConfigs) -> Self {
        Self {
            title: String::from("Recording"),
            recorder_configs: configs,
        }
    }
}

impl Default for RecordingTab {
    fn default() -> Self {
        let configs = RecorderConfigs::default();
        Self::new_with_configs(configs)
    }
}

impl tab_view::TabView for RecordingTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }

    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let c_configs = self.recorder_configs.clone();
        let Self {
            title: _,
            recorder_configs,
        } = self;

        let RecorderConfigs {
            sample_rate,
            buffer_size,
            channel,
            format,
            filter,
            f_lower,
            f_higher,
        } = recorder_configs;

        let recorder_running = controller.recorder_running();
        let save_recording_ready = controller.save_recording_ready();

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_enabled_ui(!recorder_running, |ui| {
                ui.heading("Configuration");
                Grid::new("recording_configs")
                    .striped(true)
                    .show(ui, |ui| {

                        // SAMPLE RATE
                        ui.label("Sample rate:").on_hover_ui(|ui| {
                            ui.label("Set the desired sample rate (Hz). Note: this must be supported by your audio device, or it will fall back to default.");
                        });
                        let sample_rates = SampleRate::iter();

                        ComboBox::from_id_source("sample_rate").selected_text(sample_rate.to_string()).show_ui(ui, |ui| {
                            for s in sample_rates {
                                ui.selectable_value(sample_rate, s, format!("{}: {}", s.to_string(), {
                                    let rate = s.sample_rate();
                                    match rate {
                                        None => { String::from("") }
                                        Some(n) => { format!("{} Hz", n) }
                                    }
                                }));
                            }
                        });
                        ui.end_row();
                        // BUFFER SIZE
                        ui.label("Buffer size:").on_hover_ui(|ui| {
                            ui.label("Set the desired audio frame size. Large buffer sizes may introduce lag. Recommended: Medium (1024) for Mono, Large(2048) for Stereo");
                        });

                        let buffer_sizes = BufferSize::iter();
                        //Name: Size
                        ComboBox::from_id_source("buffer_size").selected_text(buffer_size.to_string()).show_ui(ui, |ui| {
                            for b in buffer_sizes {
                                ui.selectable_value(buffer_size, b, format!("{}: {}", b.to_string(), {
                                    let size = b.size();
                                    match size {
                                        None => { String::from("") }
                                        Some(n) => { format!("{} Bytes", n) }
                                    }
                                }));
                            }
                        });
                        ui.end_row();
                        // CHANNEL
                        ui.label("Channels").on_hover_ui(|ui| {
                            ui.label("Select the number of audio channels. Must be supported by your device, or this will fall back to system defaults.");
                        });
                        let channels = Channel::iter();
                        ComboBox::from_id_source("channels").selected_text(channel.to_string()).show_ui(ui, |ui| {
                            for c in channels {
                                ui.selectable_value(channel, c, c.to_string());
                            }
                        });
                        ui.end_row();

                        // RECORDING FORMAT
                        ui.label("Audio Format").on_hover_ui(|ui| {
                            ui.label("Select WAV audio format. Must be supported by your device, or this will fallback to system defaults.");
                        });

                        let formats = RecordingFormat::iter();
                        ComboBox::from_id_source("recording_format").selected_text(format.to_string()).show_ui(ui, |ui| {
                            for f in formats {
                                ui.selectable_value(format, f, f.to_string().to_lowercase()).on_hover_ui(|ui| {
                                    ui.label(f.tooltip());
                                });
                            }
                        });

                        ui.end_row();

                        // RUN BANDPASS FILTER
                        ui.label("Bandpass Filter:").on_hover_ui(|ui| {
                            ui.label("Run a bandpass filter to clean up recording?");
                        });
                        ui.add(Checkbox::without_text(filter));
                        ui.end_row();

                        // BANDPASS THRESHOLDS
                        ui.add_enabled_ui(*filter, |ui| {
                            // High Threshold
                            ui.label("High frequency cutoff:").on_hover_ui(|ui| {
                                ui.label("Frequencies higher than this threshold will be filtered out.");
                            });
                        });

                        ui.add_enabled_ui(*filter, |ui| {
                            ui.add(Slider::new(f_higher, constants::MIN_F_HIGHER..=constants::MAX_F_HIGHER).suffix("Hz"));
                        });
                        ui.end_row();

                        // Low Threshold
                        ui.add_enabled_ui(*filter, |ui| {
                            ui.label("Low frequency cutoff:").on_hover_ui(|ui| {
                                ui.label("Frequencies lower than this threshold will be filtered out.");
                            });
                        });
                        ui.add_enabled_ui(*filter, |ui| {
                            ui.add(Slider::new(f_lower, constants::MIN_F_LOWER..=constants::MAX_F_LOWER).suffix("Hz"));
                        });
                        ui.end_row();

                        // DEFAULTS.
                        ui.label("Reset To Defaults:");
                        if ui.button("Reset").clicked() {
                            let default = RecorderConfigs::default();
                            let RecorderConfigs {
                                sample_rate: default_sample_rate, buffer_size: default_buffer_size, channel: default_channel, format: default_format, filter: default_filter, f_lower: default_f_lower, f_higher: default_f_higher
                            } = default;
                            *sample_rate = default_sample_rate;
                            *buffer_size = default_buffer_size;
                            *channel = default_channel;
                            *format = default_format;
                            *filter = default_filter;
                            *f_lower = default_f_lower;
                            *f_higher = default_f_higher;
                        }
                        ui.end_row();
                    });
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            let mic_occupied = controller.is_working() ^ recorder_running;
            ui.separator();
            ui.add_enabled_ui(!mic_occupied, |ui| {
                ui.heading("Recording");
                ui.vertical_centered_justified(|ui| {
                    if ui.add_enabled(!recorder_running, Button::new("Start Recording")).clicked() {
                        // TODO: refactor visualizer toggle into controller.
                        // TODO: refactor start_recording
                    }

                    ui.add_space(constants::BLANK_SEPARATOR);

                    if ui.add_enabled(recorder_running, Button::new("Stop Recording")).clicked() {
                        controller.stop_recording();
                    }
                });
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();
            ui.add_enabled_ui(save_recording_ready, |ui| {
                ui.heading("Saving");
                ui.vertical_centered_justified(|ui| {
                    // Save button
                    if ui.button("Save").clicked() {
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
                    ui.add_space(constants::BLANK_SEPARATOR);
                });
            });
        });
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

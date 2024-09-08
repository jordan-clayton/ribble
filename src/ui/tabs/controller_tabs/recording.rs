use std::path::PathBuf;

use egui::{Button, ComboBox, Grid, Pos2, ScrollArea, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::IntoEnumIterator;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{
        controller_tabs::controller_common::{
            f_higher_stack, f_lower_stack, toggle_bandpass_filter_stack,
        },
        tab_view,
    },
    utils::{
        constants,
        recorder_configs::{BufferSize, Channel, RecorderConfigs, RecordingFormat, SampleRate},
    },
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecordingTab {
    title: String,
    recorder_configs: RecorderConfigs,
    #[serde(skip)]
    last_mouse_pos: Pos2,
}

impl RecordingTab {
    pub fn new_with_configs(configs: RecorderConfigs) -> Self {
        Self {
            title: String::from("Recording"),
            recorder_configs: configs,
            last_mouse_pos: Default::default(),
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
            last_mouse_pos
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


        let style = ui.style_mut();
        style.interaction.show_tooltips_only_when_still = true;
        style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
        style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;

        // Workaround for egui's default tooltip behaviour.
        // This will drop the tooltip on mouse movement.
        // get the pointer state.
        let new_mouse_pos = ui.ctx().input(|i| { i.pointer.latest_pos().unwrap_or_default() });

        let diff = (new_mouse_pos - *last_mouse_pos).abs();
        *last_mouse_pos = new_mouse_pos;

        let pointer_still = diff.x <= f32::EPSILON && diff.y <= f32::EPSILON;

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_enabled_ui(!recorder_running, |ui| {
                ui.heading("Configuration");
                Grid::new("recording_configs")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {

                        // SAMPLE RATE
                        ui.label("Sample rate:");
                        let sample_rates = SampleRate::iter();

                        let resp = ComboBox::from_id_source("sample_rate").selected_text(sample_rate.to_string()).show_ui(ui, |ui| {
                            for s in sample_rates {
                                ui.selectable_value(sample_rate, s, format!("{}: {}", s.to_string(), {
                                    let rate = s.sample_rate();
                                    match rate {
                                        None => { String::from("") }
                                        Some(n) => { format!("{} Hz", n) }
                                    }
                                }));
                            }
                        }).response;

                        if pointer_still {
                            resp.on_hover_ui(|ui| {
                                ui.label("Set the desired audio sample rate. Impacts performance but improves audio quality. Falls back to system defaults if unsupported.");
                            });
                        }

                        ui.end_row();
                        // BUFFER SIZE
                        ui.label("Buffer size:");

                        let buffer_sizes = BufferSize::iter();
                        //Name: Size
                        let resp = ComboBox::from_id_source("buffer_size").selected_text(buffer_size.to_string()).show_ui(ui, |ui| {
                            for b in buffer_sizes {
                                ui.selectable_value(buffer_size, b, format!("{}: {}", b.to_string(), {
                                    let size = b.size();
                                    match size {
                                        None => { String::from("") }
                                        Some(n) => { format!("{} Bytes", n) }
                                    }
                                }));
                            }
                        }).response;

                        if pointer_still {
                            resp.on_hover_ui(|ui| {
                                ui.label("Set the desired audio frame size. Large buffer sizes may introduce lag.\nRecommended: Medium for Mono, Large for Stereo.");
                            });
                        }
                        ui.end_row();
                        // CHANNEL
                        ui.label("Channels");
                        let channels = Channel::iter();
                        let resp = ComboBox::from_id_source("channels").selected_text(channel.to_string()).show_ui(ui, |ui| {
                            for c in channels {
                                ui.selectable_value(channel, c, c.to_string());
                            }
                        }).response;
                        if pointer_still {
                            resp.on_hover_ui(|ui| {
                                ui.label("Stereo or Mono. Falls back to system defaults if unsupported.");
                            });
                        }
                        ui.end_row();

                        // RECORDING FORMAT
                        ui.label("Audio Format");

                        let formats = RecordingFormat::iter();
                        let resp = ComboBox::from_id_source("recording_format").selected_text(format.to_string()).show_ui(ui, |ui| {
                            for f in formats {
                                ui.selectable_value(format, f, f.to_string().to_lowercase()).on_hover_ui(|ui| {
                                    ui.label(f.tooltip());
                                });
                            }
                        }).response;

                        if pointer_still {
                            resp.on_hover_ui(|ui| {
                                ui.label("Select WAV audio format. Falls back to system defaults if unsupported.");
                            });
                        }

                        ui.end_row();

                        // RUN BANDPASS FILTER
                        toggle_bandpass_filter_stack(ui, filter, pointer_still);
                        ui.end_row();

                        // BANDPASS THRESHOLDS
                        f_higher_stack(ui, *filter, f_higher, pointer_still);
                        ui.end_row();

                        // Low Threshold
                        f_lower_stack(ui, *filter, f_lower, pointer_still);
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
                        controller.start_recording(c_configs);
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

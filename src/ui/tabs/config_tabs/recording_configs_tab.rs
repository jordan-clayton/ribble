use egui::{Checkbox, ComboBox, Grid, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::IntoEnumIterator;

use crate::{
    utils::{
        configs::{
            AudioConfigs, BufferSize, Channel, RecorderConfigs, RecordingFormat, SampleRate,
        },
        constants,
    },
    whisper_app_context::WhisperAppController,
};
use crate::ui::tabs::tab_view;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RecordingConfigsTab {
    title: String,
    recorder_configs: RecorderConfigs,
}

impl RecordingConfigsTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: RecorderConfigs) -> Self {
        Self {
            title: String::from("Recording Configuration"),
            recorder_configs: configs,
        }
    }
}

impl Default for RecordingConfigsTab {
    fn default() -> Self {
        let configs = RecorderConfigs::default();
        Self::new_with_configs(configs)
    }
}

impl tab_view::TabView for RecordingConfigsTab {
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

        // Check for config-copy requests.
        let req = controller.recv_recording_configs_req();
        if let Ok(_) = req {
            controller
                .send_configs(AudioConfigs::Recording(c_configs))
                .expect("Configs channel closed.");
        }

        // *** flag for enabled
        let recorder_running = controller.recorder_running();

        // Grid of configs + button for default.
        ui.add_enabled_ui(!recorder_running, |ui| {
            Grid::new("recording_configs")
                .striped(true)
                .show(ui, |ui| {
                    // SAMPLE RATE
                    ui.label("Sample rate:").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Set the desired sample rate (Hz). Note: this must be supported by your audio device, or it will fall back to default.");
                    });
                    let sample_rates = SampleRate::iter();
                    ui.horizontal(|ui| {
                        // Rate: Value
                        ComboBox::from_id_source("sample_rate").selected_text(sample_rate.to_string()).show_ui(ui, |ui| {
                            for s in sample_rates {
                                ui.selectable_value(sample_rate, s, format!("{}: {}", s.to_string(), {
                                    let rate = s.sample_rate();
                                    match rate {
                                        None => { String::from("") }
                                        Some(n) => { format!("{}", n) }
                                    }
                                }));
                            }
                        });
                        ui.label("Hz");
                    });
                    ui.end_row();
                    // BUFFER SIZE
                    ui.label("Buffer size:").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Set the desired audio frame size. Large buffer sizes may introduce lag. Recommended: Medium (1024) for Mono, Large(2048) for Stereo");
                    });

                    let buffer_sizes = BufferSize::iter();
                    ui.horizontal(|ui| {
                        //Name: Size
                        ComboBox::from_id_source("buffer_size").selected_text(buffer_size.to_string()).show_ui(ui, |ui| {
                            for b in buffer_sizes {
                                ui.selectable_value(buffer_size, b, format!("{}: {}", b.to_string(), {
                                    let size = b.size();
                                    match size {
                                        None => { String::from("") }
                                        Some(n) => { format!("{}", n) }
                                    }
                                }));
                            }
                        });
                        ui.label("Bytes");
                    });
                    ui.end_row();
                    // CHANNEL
                    ui.label("Channels").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Select the number of audio channels. Must be supported by your device, or this will fall back to system defaults.");
                    });
                    let channels = Channel::iter();
                    for c in channels {
                        ui.selectable_value(channel, c, c.to_string());
                    }
                    ui.end_row();

                    // RECORDING FORMAT
                    ui.label("Audio Format").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Select WAV audio format. Must be supported by your device, or this will fallback to system defaults.");
                    });

                    let formats = RecordingFormat::iter();
                    ComboBox::from_id_source("recording_format").selected_text(format.to_string()).show_ui(ui, |ui| {
                        for f in formats {
                            ui.selectable_value(format, f, f.to_string()).on_hover_ui(|ui| {
                                ui.label(f.tooltip());
                            });
                        }
                    });

                    ui.end_row();

                    // RUN BANDPASS FILTER
                    ui.label("Filter:").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Run a bandpass filter to clean up recording?");
                    });
                    ui.add(Checkbox::without_text(filter));
                    ui.end_row();

                    // BANDPASS THRESHOLDS
                    ui.add_enabled_ui(*filter, |ui| {
                        // High Threshold
                        ui.label("High frequency cutoff:").on_hover_ui(|ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            ui.label("Frequencies higher than this threshold will be filtered out.");
                        });
                        ui.add(Slider::new(f_higher, constants::MIN_HIGH_FREQUENCY..=constants::MAX_HIGH_FREQUENCY).suffix("Hz"));
                        ui.end_row();
                        // Low Threshold
                        ui.label("Low frequency cutoff:").on_hover_ui(|ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            ui.label("Frequencies lower than this threshold will be filtered out.");
                        });

                        ui.add(Slider::new(f_lower, constants::MIN_HIGH_FREQUENCY..=constants::MAX_HIGH_FREQUENCY).suffix("Hz"));
                        ui.end_row();
                    })
                });
        });
    }

    // TODO: determine if this is required.
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

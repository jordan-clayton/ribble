use std::path::PathBuf;

use egui::{Button, Grid, Label, Pos2, RichText, ScrollArea, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::{IntoEnumIterator, VariantArray};
use whisper_realtime::{
    configs::Configs,
    model::{Model, ModelType},
};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::{
        tabs::controller_tabs::controller_common::{
            f_higher_stack, f_lower_stack, model_stack, n_threads_stack, save_transcription_button,
            set_language_stack, set_translate_stack, toggle_bandpass_filter_stack, use_gpu_stack,
        },
        tabs::tab_view,
        tabs::whisper_tab::FocusTab,
    },
    utils::{constants, preferences::get_app_theme, threading::get_max_threads},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RealtimeTab {
    title: String,
    realtime_configs: Configs,
    #[serde(skip)]
    #[serde(default = "get_max_threads")]
    max_threads: std::ffi::c_int,
    filter: bool,
    f_lower: f32,
    f_higher: f32,
    #[serde(skip)]
    last_mouse_pos: Pos2,
}

impl RealtimeTab {
    pub fn new_with_configs(realtime_configs: Configs) -> Self {
        let max_threads = get_max_threads();
        Self {
            title: String::from("Realtime"),
            realtime_configs,
            max_threads,
            filter: false,
            f_lower: constants::DEFAULT_F_LOWER,
            f_higher: constants::DEFAULT_F_HIGHER,
            last_mouse_pos: Default::default(),
        }
    }
}

impl Default for RealtimeTab {
    fn default() -> Self {
        let configs = Configs::default();
        Self::new_with_configs(configs)
    }
}

impl tab_view::TabView for RealtimeTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    //noinspection DuplicatedCode
    // Main UI design.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let c_configs = self.realtime_configs.clone();
        let Self {
            title: _,
            realtime_configs,
            max_threads,
            filter,
            f_lower,
            f_higher,
            last_mouse_pos,
        } = self;

        let Configs {
            n_threads,
            set_translate,
            language,
            use_gpu,
            model,
            realtime_timeout,
            audio_sample_ms,
            vad_sample_ms,
            phrase_timeout,
            voice_probability_threshold,
            naive_vad_freq_threshold: _,
            naive_vad_energy_threshold: _,
            naive_window_len: _,
            naive_window_step: _,
            print_special: _,
            print_progress: _,
            print_realtime: _,
            print_timestamps: _,
        } = realtime_configs;

        // Update ready state
        let data_dir =
            eframe::storage_dir(constants::APP_ID).expect("Failed to get data directory");
        let m_model = Model::new_with_type_and_dir(*model, data_dir);
        let downloaded = m_model.is_downloaded();
        controller.set_realtime_ready(downloaded);

        let realtime_running = controller.realtime_running();
        let realtime_ready = controller.realtime_ready();
        let available_models: Vec<ModelType> = if *use_gpu {
            ModelType::VARIANTS.to_vec()
        } else {
            ModelType::iter()
                .filter(|m| *m < ModelType::Small)
                .collect()
        };

        let system_theme = controller.get_system_theme();
        let theme = get_app_theme(system_theme);

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
            ui.add_enabled_ui(!realtime_running, |ui| {
                ui.heading("Configuration");
                Grid::new("realtime_configs")
                    .striped(true)
                    .num_columns(2)
                    .show(ui, |ui| {
                        // Model
                        model_stack(ui, model, &m_model, downloaded, controller.clone(), available_models.as_slice(), Some(theme), pointer_still);
                        ui.end_row();
                        // Num_threads
                        n_threads_stack(ui, n_threads, *max_threads, pointer_still);
                        ui.end_row();
                        // Use gpu
                        let gpu_enabled = controller.gpu_enabled();
                        use_gpu_stack(ui, use_gpu, gpu_enabled, pointer_still);
                        ui.end_row();
                        // INPUT Language -> Set to auto for language detection
                        set_language_stack(ui, language, pointer_still);
                        ui.end_row();
                        // Translate (TO ENGLISH)
                        set_translate_stack(ui, set_translate, pointer_still);
                        ui.end_row();

                        // Filter audio
                        toggle_bandpass_filter_stack(ui, filter, pointer_still);
                        ui.end_row();

                        f_higher_stack(ui, *filter, f_higher, pointer_still);
                        ui.end_row();

                        f_lower_stack(ui, *filter, f_lower, pointer_still);
                        ui.end_row();

                        // Transcriber Timeout
                        ui.label("Transcription Timeout:");

                        let mut rt_timeout = *realtime_timeout as u64 / 1000;

                        // MAX_REALTIME_TIMEOUT is in seconds.
                        ui.horizontal(|ui| {
                            let mut resp = ui.add(Slider::new(&mut rt_timeout, 0..=constants::MAX_REALTIME_TIMEOUT)
                                .step_by(1.0)
                                .drag_value_speed(1.0)
                            );

                            if pointer_still {
                                resp = resp.on_hover_ui(|ui| {
                                    ui.label("Sets timeout limit for realtime audio transcription. Set to 0 to disable.");
                                });
                            }
                            if resp.changed() {
                                *realtime_timeout = (rt_timeout * 1000) as u128;
                            };

                            resp.context_menu(|ui| {
                                if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                                    *realtime_timeout = whisper_realtime::constants::REALTIME_AUDIO_TIMEOUT;
                                    ui.close_menu();
                                }
                            });

                            ui.label({
                                let millis = *realtime_timeout;
                                let seconds = millis / 1000;
                                let h = seconds / (60 * 60);
                                let m = (seconds / 60) % 60;
                                let s = seconds % 60;
                                format!("{h:02}h : {m:02}m : {s:02}s")
                            });
                        });

                        ui.end_row();

                        // Audio chunk size
                        ui.label("Audio Sample Size:");

                        let mut sample_ms = *audio_sample_ms as f32 / 1000.0;
                        let mut resp =
                            ui.add(Slider::new(&mut sample_ms, constants::MIN_AUDIO_CHUNK_SIZE..=constants::MAX_AUDIO_CHUNK_SIZE)
                                .step_by(0.5).suffix("s"));

                        if pointer_still {
                            resp = resp.on_hover_ui(|ui| {
                                ui.label("Set the sample window size for realtime processing. Affects accuracy.\nRecommended: 10s");
                            });
                        }
                        if resp.changed() {
                            *audio_sample_ms = (sample_ms * 1000.0) as usize;
                        }

                        resp.context_menu(|ui| {
                            if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                                *audio_sample_ms = whisper_realtime::constants::AUDIO_SAMPLE_MS;
                                ui.close_menu();
                            }
                        });
                        ui.end_row();

                        let mut slider_phrase_timeout = *phrase_timeout as f32 / 1000.0;

                        ui.label("Phrase Timeout:");
                        let mut resp = ui.add(Slider::new(&mut slider_phrase_timeout, constants::MIN_PHRASE_TIMEOUT..=constants::MAX_PHRASE_TIMEOUT)
                            .step_by(0.5).suffix("s"));
                        if pointer_still {
                            resp = resp.on_hover_ui(|ui| {
                                ui.label("Set the approximate duration of a complete phrase. Affects transcription accuracy.\nRecommended: 3s");
                            });
                        }
                        if resp.changed() {
                            *phrase_timeout = (slider_phrase_timeout * 1000.0) as usize;
                        }

                        resp.context_menu(|ui| {
                            if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                                *phrase_timeout = whisper_realtime::constants::PHRASE_TIMEOUT;
                                ui.close_menu();
                            }
                        });
                        ui.end_row();

                        // Voice Activity Detection chunk size (UI in Seconds), internally ms.
                        ui.label("Voice Activity Sample Size");

                        let mut vad_sec = *vad_sample_ms as f32 / 1000.0;

                        let mut resp = ui.add(Slider::new(&mut vad_sec, constants::MIN_VAD_SEC..=constants::MAX_VAD_SEC).step_by(0.05).suffix("s"));
                        if pointer_still {
                            resp = resp.on_hover_ui(|ui| {
                                ui.label("Set the sample size for voice detection. Affects accuracy, smaller is usually better.\nRecommended: 0.3s");
                            })
                        }
                        if resp.changed() {
                            *vad_sample_ms = (vad_sec * 1000.0) as usize;
                        }

                        resp.context_menu(|ui| {
                            if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                                *vad_sample_ms = whisper_realtime::constants::VAD_SAMPLE_MS;
                                ui.close_menu();
                            }
                        });

                        ui.end_row();
                        // Voice Activity probability threshold
                        // Label
                        ui.label("VAD Probability Threshold:");
                        let mut resp = ui.add(Slider::new(voice_probability_threshold, constants::MIN_VAD_PROBABILITY..=constants::MAX_VAD_PROBABILITY)
                            .custom_formatter(|n, _| {
                                let p = n * 100f64;
                                format!("{p:0.2}")
                            }).suffix("%")
                            .custom_parser(|s| {
                                let str = s.chars().filter(|c| c.is_numeric() || *c == '.').collect::<String>();
                                str.parse::<f64>().ok()
                            })
                        );

                        if pointer_still {
                            resp = resp.on_hover_ui(|ui| {
                                ui.label("Set the minimum threshold for detecting speech. Affects accuracy.\nRecommended: 65%-80%");
                            });
                        }
                        resp.context_menu(|ui| {
                            if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                                *voice_probability_threshold = whisper_realtime::constants::VOICE_PROBABILITY_THRESHOLD;
                                ui.close_menu();
                            }
                        });

                        ui.end_row();
                        // Reset defaults button.
                        ui.label("Reset all to default:");
                        if ui.button("Reset").clicked() {
                            let default = Configs::default();
                            let Configs {
                                n_threads: default_n_threads,
                                set_translate: default_set_translate,
                                language: default_language,
                                use_gpu: default_use_gpu,
                                model: default_model,
                                realtime_timeout: default_realtime_timeout,
                                audio_sample_ms: default_audio_sample_ms,
                                vad_sample_ms: default_vad_sample_ms,
                                phrase_timeout: default_phrase_timeout,
                                voice_probability_threshold: default_voice_probability_threshold, ..
                            } = default;

                            let default_n_threads = get_max_threads().min(default_n_threads);

                            *n_threads = default_n_threads;
                            *set_translate = default_set_translate;
                            *language = default_language;
                            *use_gpu = default_use_gpu && gpu_enabled;
                            *model = default_model;
                            *realtime_timeout = default_realtime_timeout;
                            *audio_sample_ms = default_audio_sample_ms;
                            *vad_sample_ms = default_vad_sample_ms;
                            *phrase_timeout = default_phrase_timeout;
                            *voice_probability_threshold = default_voice_probability_threshold;
                            *filter = false;
                            *f_lower = constants::DEFAULT_F_LOWER;
                            *f_higher = constants::DEFAULT_F_HIGHER;
                        }
                        ui.end_row();
                    });
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();
            let mic_occupied = controller.is_working() ^ realtime_running;
            ui.add_enabled_ui(!mic_occupied, |ui| {
                // Transcription section
                ui.add_enabled(realtime_ready, Label::new(RichText::new("Transcription").heading()));
                ui.vertical_centered_justified(|ui| {
                    if ui
                        .add_enabled(!realtime_running && realtime_ready, Button::new("Start"))
                        .clicked()
                    {
                        controller.start_realtime_transcription(c_configs, (*filter, *f_higher, *f_lower));
                    }

                    ui.add_space(constants::BLANK_SEPARATOR);

                    if ui
                        .add_enabled(realtime_running, Button::new("Stop"))
                        .clicked()
                    {
                        controller.stop_transcriber(true);
                    }
                });
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();

            // Saving
            let can_save_audio = controller.save_recording_ready();
            // Static transcription check
            let static_running = controller.static_running();

            ui.add_enabled_ui(!realtime_running && !static_running, |ui| {
                ui.heading("Saving");
                ui.vertical_centered_justified(|ui| {
                    save_transcription_button(ui, controller.clone());
                    ui.add_space(constants::BLANK_SEPARATOR);
                    if ui.add(Button::new("Copy to Clipboard")).clicked() {
                        controller.copy_to_clipboard();
                    }
                    ui.add_space(constants::BLANK_SEPARATOR);
                    // Save audio
                    if ui
                        .add_enabled(can_save_audio, Button::new("Save Recording"))
                        .clicked()
                    {
                        // Open File dialog at HOME directory, fallback to root.
                        let base_dirs = directories::BaseDirs::new();
                        let dir = if let Some(dir) = base_dirs {
                            dir.home_dir().to_path_buf()
                        } else {
                            PathBuf::from("/")
                        };

                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("wave (.wav)", &["wav"])
                            .set_directory(dir)
                            .save_file()
                        {
                            controller.save_audio_recording(&p);
                        }
                    }
                });
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();


            ui.add_enabled_ui(!realtime_running && (can_save_audio), |ui| {
                ui.heading("Re-Transcribe");
                ui.vertical_centered_justified(|ui| {
                    if ui
                        .add_enabled(can_save_audio, Button::new("Open Static tab."))
                        .clicked()
                    {
                        controller.send_focus_tab(FocusTab::Static).expect("Focus channel closed.");
                    }
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

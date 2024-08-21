use egui::{Button, Checkbox, ComboBox, Grid, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::{IntoEnumIterator, VariantArray};
use whisper_realtime::{
    configs::Configs,
    model::{Model, ModelType},
};

use crate::{
    ui::tabs::tab_view,
    utils::{configs::AudioConfigs, constants, threading::get_max_threads},
    whisper_app_context::WhisperAppController,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RealtimeConfigsTab {
    title: String,
    realtime_configs: Configs,
    #[serde(skip)]
    #[serde(default = "get_max_threads")]
    max_threads: std::ffi::c_int,
}

impl RealtimeConfigsTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: Configs) -> Self {
        let max_threads = get_max_threads();
        Self {
            title: String::from("Realtime Configuration"),
            realtime_configs: configs,
            max_threads,
        }
    }
}

impl Default for RealtimeConfigsTab {
    fn default() -> Self {
        let configs = Configs::default();
        Self::new_with_configs(configs)
    }
}

// TODO: refactor duplicated code fragments into shared functions.
// TODO: disable ui when running the static transcriber - It won't cause problems internally, but will cause confusion for the user.
impl tab_view::TabView for RealtimeConfigsTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let c_configs = self.realtime_configs.clone();
        let Self {
            title: _,
            realtime_configs,
            max_threads,
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

        // Check for config-copy requests
        let req = controller.recv_realtime_configs_req();
        if let Ok(_) = req {
            controller
                .send_configs(AudioConfigs::Realtime(c_configs))
                .expect("Configs channel closed.");
        }

        let downloading = controller.is_downloading();
        let realtime_ready = controller.realtime_ready();
        let realtime_running = controller.realtime_running();

        ui.add_enabled_ui(!realtime_running, |ui| {
            Grid::new("realtime_configs").striped(true).show(ui, |ui| {
                // Model
                ui.label("Model:").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Select the desired model for transcribing");
                });

                let available_models: Vec<ModelType> = if *use_gpu {
                    ModelType::VARIANTS.to_vec()
                } else {
                    ModelType::iter().filter(|m| *m < ModelType::Small).collect()
                };


                ui.horizontal(|ui| {
                    ComboBox::from_id_source("model_type")
                        .selected_text(model.to_string())
                        .show_ui(ui, |ui| {
                            for m in available_models {
                                ui.selectable_value(model, m, m.to_string());
                            }
                        });

                    let dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data dir.");
                    let m_model = Model::new_with_type_and_dir(*model, dir);
                    let model_downloaded = m_model.is_downloaded();

                    if model_downloaded {
                        if !realtime_ready {
                            controller.update_realtime_ready(true);
                        }
                        // Okay icon
                        // ui.add(okay icon);
                        ui.label("-Okay icon- here");
                    } else {
                        if realtime_ready {
                            controller.update_realtime_ready(false);
                        }
                        // Warning icon
                        // ui.add(warning icon);
                        ui.label("-Warning icon- here");
                    }

                    // Open button
                    if ui.button("Open")
                        .on_hover_ui(|ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            ui.label(format!("Open compatible {} model", model.to_string()));
                        }).clicked()
                    {
                        // TODO: controller fn for opening model:
                        // -Open file dialog: get path
                        // -Copy the file to the models directory.
                        // controller.open_model(...);
                    }

                    // Download button
                    if ui.add_enabled(!downloading, Button::new("Download")).on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label(format!("Download compatible {} model", model.to_string()));
                    }).clicked() {
                        // TODO: finish controller download fn
                        // controller.download_model( ... either model or name);
                    }
                });

                ui.end_row();

                // Num_threads
                ui.label("Threads:").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Select the number of threads to allocate for transcription");
                    ui.label(format!("Recommended: {}", std::cmp::min(7, *max_threads)));
                });

                ui.add(Slider::new(n_threads, 1..=std::cmp::min(*max_threads, constants::MAX_WHISPER_THREADS)));
                ui.end_row();

                // Use gpu
                if cfg!(feature = "_gpu") {
                    ui.label("Hardware Accelerated (GPU):").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Enable hardware acceleration. REQUIRED for models larger than Small");
                    });
                    ui.add(Checkbox::without_text(use_gpu));
                    ui.end_row();
                }
                // INPUT Language -> Set to auto for language detection
                ui.label("Language:").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Select input language. Set to Auto for auto-detection");
                });


                ComboBox::from_id_source("language").selected_text(
                    *constants::LANGUAGE_CODES.get(language).expect("Failed to get language")
                ).show_ui(ui, |ui| {
                    for (k, v) in constants::LANGUAGE_OPTIONS.iter() {
                        ui.selectable_value(language, v.clone(), *k);
                    }
                });

                ui.end_row();
                // Translate (TO ENGLISH)
                ui.label("Translate").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Translate transcription (to English ONLY)");
                });

                ui.add(Checkbox::without_text(set_translate));

                ui.end_row();
                // Transcriber Timeout
                ui.label("Transcription Timeout").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Set realtime timeout? Set to 0 to disable");
                });

                let mut rt_timeout = *realtime_timeout as u64;

                ui.horizontal(|ui| {
                    if ui.add(Slider::new(&mut rt_timeout, 0..=constants::MAX_REALTIME_TIMEOUT as u64)
                        // Step by seconds: TODO: consider changing as needed
                        .step_by(1000f64))
                        .changed() {
                        *realtime_timeout = rt_timeout as u128;
                    };

                    ui.label({
                        let h = *realtime_timeout / (60 * 60);
                        let m = (*realtime_timeout / 60) % 60;
                        let s = *realtime_timeout % 60;
                        format!("{h:02}H : {m:02}M : {s:02}S:")
                    });
                });


                ui.end_row();
                // Audio chunk size (in ms) Min: 2s? Max: 30s
                ui.label("Audio Sample Size").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Realtime audio is processed in chunks, (in ms). Tweak this value to improve transcription accuracy. Recommended: 10s / 10000ms");
                });

                ui.horizontal(|ui| {
                    ui.add(Slider::new(audio_sample_ms, constants::MIN_AUDIO_CHUNK_SIZE..=constants::MAX_AUDIO_CHUNK_SIZE)
                        .step_by(100f64));
                    ui.label({
                        // TODO: if precision is weird, use f64
                        let s = (*audio_sample_ms as f32) / 1000f32;
                        format!("{s:.3} seconds")
                    })
                });
                ui.end_row();

                // Phrase timeout (in ms) Min: 2s? Max: 10s?
                ui.label("Phrase Timeout Size").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Phrase timeout is the estimated length of time per sentence. Tweak this value to improve accuracy and reduce accidental output duplication. Recommended: 3s / 3000ms");
                });

                ui.horizontal(|ui| {
                    ui.add(Slider::new(phrase_timeout, constants::MAX_PHRASE_TIMEOUT..=constants::MAX_PHRASE_TIMEOUT)
                        .step_by(100f64));
                    ui.label({
                        let s = (*phrase_timeout as f32) / 1000f32;
                        format!("{s:.3} seconds")
                    })
                });
                ui.end_row();
                // Voice Activity Detection chunk size (in ms)
                ui.label("Voice Activity Sample Size").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Voice activity is processed in small sample chunks. Tweak this value to improve detection accuracy. Recommended: 300ms");
                });

                ui.horizontal(|ui| {
                    ui.add(Slider::new(vad_sample_ms, constants::MIN_VAD_SAMPLE_MS..=constants::MAX_VAD_SAMPLE_MS).step_by(10f64));
                    ui.label({
                        let ms = (*vad_sample_ms as f32) / 1000f32;
                        format!("{ms:04} ms")
                    })
                });

                ui.end_row();
                // Voice Activity probability threshold
                // Label
                ui.label("Voice Detection Probability").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Set the minimum probability threshold for detecting speech. Tweak to improve detection accuracy. Recommended: 65%-80%");
                });

                // This is represented in percentages to be slightly more intuitive.
                ui.add(Slider::new(voice_probability_threshold, constants::MIN_VAD_PROBABILITY..=constants::MAX_VAD_PROBABILITY)
                    .custom_formatter(|n, _| {
                        let p = n * 100f64;
                        format!("{p:.20}")
                    }).suffix("%")
                    .custom_parser(|s| {
                        let str = s.chars().filter(|c| c.is_numeric() || *c == '.').collect::<String>();
                        str.parse::<f64>().ok()
                    })
                );


                ui.end_row();
                // Reset defaults button.
                ui.label("Reset To Defaults");
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
                        voice_probability_threshold: default_voice_probability_threshold,
                        naive_vad_freq_threshold: _,
                        naive_vad_energy_threshold: _,
                        naive_window_len: _,
                        naive_window_step: _,
                        print_special: _,
                        print_progress: _,
                        print_realtime: _,
                        print_timestamps: _,
                    } = default;

                    *n_threads = default_n_threads;
                    *set_translate = default_set_translate;
                    *language = default_language;
                    *use_gpu = default_use_gpu;
                    *model = default_model;
                    *realtime_timeout = default_realtime_timeout;
                    *audio_sample_ms = default_audio_sample_ms;
                    *vad_sample_ms = default_vad_sample_ms;
                    *phrase_timeout = default_phrase_timeout;
                    *voice_probability_threshold = default_voice_probability_threshold;
                }
                ui.end_row();
            });
        });
    }

    // TODO: Determine if needed.
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

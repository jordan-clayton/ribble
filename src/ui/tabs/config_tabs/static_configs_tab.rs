use egui::{Button, Checkbox, ComboBox, Grid, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::VariantArray;
use whisper_realtime::configs::Configs;
use whisper_realtime::model::{Model, ModelType};

use crate::ui::tabs::tab_view;
use crate::utils::configs::AudioConfigs;
use crate::utils::constants;
use crate::utils::threading::get_max_threads;
use crate::whisper_app_context::WhisperAppController;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StaticConfigsTab {
    title: String,
    static_configs: Configs,
    #[serde(skip)]
    #[serde(default = "get_max_threads")]
    max_threads: std::ffi::c_int,
}

// TODO
impl StaticConfigsTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: Configs) -> Self {
        let max_threads = get_max_threads();
        Self {
            title: String::from("Static Configuration"),
            static_configs: configs,
            max_threads,
        }
    }
}

impl Default for StaticConfigsTab {
    fn default() -> Self {
        let configs = Configs::default();
        Self::new_with_configs(configs)
    }
}

// TODO: refactor duplicated code fragments into shared functions.
// TODO: disable ui when running the static transcriber - It won't cause problems internally, but will cause confusion for the user.
impl tab_view::TabView for StaticConfigsTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        let c_configs = self.static_configs.clone();
        let Self {
            title: _,
            static_configs,
            max_threads,
        } = self;

        let Configs {
            n_threads,
            set_translate,
            language,
            use_gpu,
            model,
            realtime_timeout: _,
            audio_sample_ms: _,
            vad_sample_ms: _,
            phrase_timeout: _,
            voice_probability_threshold: _,
            naive_vad_freq_threshold: _,
            naive_vad_energy_threshold: _,
            naive_window_len: _,
            naive_window_step: _,
            print_special: _,
            print_progress: _,
            print_realtime: _,
            print_timestamps: _,
        } = static_configs;

        // Check for config-copy requests
        let req = controller.recv_static_configs_req();
        if let Ok(_) = req {
            controller.send_configs(AudioConfigs::Static(c_configs)).expect("Configs channel closed.");
        }

        let downloading = controller.is_downloading();
        let static_ready = controller.static_ready();
        let static_running = controller.static_running();

        ui.add_enabled_ui(!static_running, |ui| {
            Grid::new("static_configs").striped(true).show(ui, |ui| {
                // Model
                ui.label("Model:").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Select the desired model for transcribing");
                });

                let available_models: Vec<ModelType> = ModelType::VARIANTS.to_vec();

                ui.horizontal(|ui| {
                    ComboBox::from_id_source("modeltype")
                        .selected_text(model.to_string())
                        .show_ui(ui, |ui| {
                            for m in available_models {
                                ui.selectable_value(model, m, m.to_string());
                            }
                        });

                    let dir =
                        eframe::storage_dir(constants::APP_ID).expect("Failed to get data dir.");
                    let mut m_model = Model::new_with_type_and_dir(*model, dir);
                    let mut model_downloaded = m_model.is_downloaded();

                    if model_downloaded {
                        if !static_ready {
                            controller.update_realtime_ready(true);
                        }
                        // Okay icon
                        // ui.add(okay icon);
                        ui.label("-Okay icon- here");
                    } else {
                        if static_ready {
                            controller.update_realtime_ready(false);
                        }
                        // Warning icon
                        // ui.add(warning icon);
                        ui.label("-Warning icon- here");
                    }

                    // Open button
                    if ui
                        .button("Open")
                        .on_hover_ui(|ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            ui.label(format!("Open compatible {} model", model.to_string()));
                        })
                        .clicked()
                    {
                        // TODO: controller fn for opening model:
                        // -Open file dialog: get path
                        // -Copy the file to the models directory.
                        // controller.open_model(...);
                    }

                    // Download button
                    if ui
                        .add_enabled(!downloading, Button::new("Download"))
                        .on_hover_ui(|ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            ui.label(format!("Download compatible {} model", model.to_string()));
                        })
                        .clicked()
                    {
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

                ui.add(Slider::new(
                    n_threads,
                    1..=std::cmp::min(*max_threads, constants::MAX_WHISPER_THREADS),
                ));

                ui.end_row();

                // Use gpu
                if cfg!(feature = "_gpu") {
                    ui.label("Hardware Accelerated (GPU):").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label(
                            "Enable hardware acceleration. REQUIRED for models larger than Small",
                        );
                    });
                    ui.add(Checkbox::without_text(use_gpu));

                    ui.end_row();
                }

                // INPUT Language -> Set to auto for language detection
                ui.label("Language:").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Select input language. Set to Auto for auto-detection");
                });

                ComboBox::from_id_source("language")
                    .selected_text(
                        *constants::LANGUAGE_CODES
                            .get(language)
                            .expect("Failed to get language"),
                    )
                    .show_ui(ui, |ui| {
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
                        realtime_timeout: _,
                        audio_sample_ms: _,
                        vad_sample_ms: _,
                        phrase_timeout: _,
                        voice_probability_threshold: _,
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
                }
                ui.end_row();
            });
        });
    }

    // Right-click tab -> What should be shown.
    // TODO: Determine whether necessary to implement
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

use std::any::Any;
use std::path::PathBuf;
use std::thread;

use egui::{Button, Checkbox, ComboBox, Grid, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::VariantArray;
use whisper_realtime::{
    configs::Configs,
    model::{Model, ModelType},
};

use crate::{
    ui::tabs::tab_view,
    utils::{configs::AudioConfigs, constants, threading::get_max_threads},
    whisper_app_context::WhisperAppController,
};
use crate::ui::tabs::tabs_common::{download_button, FileFilter, open_file_button};
use crate::utils::configs::WorkerType;
use crate::utils::file_mgmt::copy_data;

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
            controller
                .send_configs(AudioConfigs::Static(c_configs))
                .expect("Configs channel closed.");
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

                // TODO: Move to configs_common
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
                    let model_downloaded = m_model.is_downloaded();

                    if model_downloaded {
                        if !static_ready {
                            controller.update_static_ready(true);
                        }
                        // Okay icon
                        // ui.add(okay icon);
                        ui.label("-Okay icon- here");
                    } else {
                        if static_ready {
                            controller.update_static_ready(false);
                        }
                        // Warning icon
                        // ui.add(warning icon);
                        ui.label("-Warning icon- here");
                    }

                    // Open button -->TODO:  Refactor into void fn and move to configs_common
                    let filters = vec![FileFilter {
                        file_type: "ggml model",
                        filters: vec!["bin"],
                    }];
                    let s_filters = Some(filters.as_slice());
                    let open_file_tooltip =
                        Some(format!("Open compatible {} model", model.to_string()));
                    let c_controller = controller.clone();
                    let model_path_open = m_model.file_path();

                    let open_file_callback = move |path: &Option<PathBuf>| {
                        if let Some(p) = path {
                            let from = p.clone();
                            let to = model_path_open.clone();
                            let copy_thread = thread::spawn(move || {
                                let success = copy_data(&from, &to);
                                match success {
                                    Ok(_) => Ok(format!(
                                        "File: {:?}, successfully copied to: {:?}",
                                        from.as_os_str(),
                                        to.as_os_str()
                                    )),
                                    Err(e) => {
                                        panic!("{}", e)
                                    }
                                }
                            });

                            let worker = (WorkerType::Downloading, copy_thread);

                            c_controller
                                .send_thread_handle(worker)
                                .expect("Thread channel closed");
                        }
                    };

                    ui.add(open_file_button(
                        s_filters,
                        open_file_tooltip,
                        true,
                        open_file_callback,
                    ));

                    let c_controller = controller.clone();
                    let url = m_model.url();
                    let file_name = m_model.model_file_name().to_owned();
                    let directory = m_model.model_directory();
                    let download_callback = move || {
                        c_controller.start_download(url, file_name, directory);
                    };

                    let download_tooltip =
                        Some(format!("Download compatible {} model", model.to_string()));
                    // Download button
                    ui.add_enabled(
                        !downloading,
                        download_button(
                            download_tooltip,
                            true,
                            "Download Model",
                            download_callback,
                        ),
                    );
                });

                ui.end_row();

                // TODO: Move to configs_common
                // Num_threads
                ui.label("Threads:").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Select the number of threads to allocate for transcription");
                    ui.label(format!("Recommended: {}", std::cmp::min(7, *max_threads)));
                });

                // TODO: Move to configs_common
                ui.add(Slider::new(
                    n_threads,
                    1..=std::cmp::min(*max_threads, constants::MAX_WHISPER_THREADS),
                ));

                ui.end_row();

                // TODO: Move to configs_common
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

                // TODO: Move to configs_common
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

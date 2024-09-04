use std::path::PathBuf;

use egui::{Button, Grid, Label, RichText, ScrollArea, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::VariantArray;
use whisper_realtime::{configs::Configs, model::ModelType};
use whisper_realtime::model::Model;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::tab_view,
    utils::threading::get_max_threads,
};
use crate::ui::tabs::controller_tabs::controller_common;
use crate::ui::widgets::icons::{ok_icon, warning_icon};
use crate::utils::{constants, file_mgmt};
use crate::utils::preferences::get_app_theme;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StaticTab {
    title: String,
    static_configs: Configs,
    #[serde(skip)]
    #[serde(default = "get_max_threads")]
    max_threads: std::ffi::c_int,
    #[serde(skip)]
    audio_path: Option<PathBuf>,
}

impl StaticTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: Configs) -> Self {
        let max_threads = get_max_threads();
        Self {
            title: String::from("Static"),
            static_configs: configs,
            max_threads,
            audio_path: None,
        }
    }
}

impl Default for StaticTab {
    fn default() -> Self {
        let configs = Configs::default();
        Self::new_with_configs(configs)
    }
}

impl tab_view::TabView for StaticTab {
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
            audio_path
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
        let (has_file, file_path) = if audio_path.is_some() {
            let path = audio_path.clone().unwrap();
            let valid_file = path.exists() & path.is_file();
            (valid_file, path)
        } else {
            let data_dir =
                eframe::storage_dir(constants::APP_ID).expect("Failed to get storage dir");
            let path = file_mgmt::get_temp_file_path(&data_dir);
            let valid_file = path.exists() & controller.save_recording_ready();
            (valid_file, path)
        };

        // Set static ready based on the current model + audio file.
        let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data directory");
        let m_model = Model::new_with_type_and_dir(*model, data_dir);
        let downloaded = m_model.is_downloaded();
        let is_ready = downloaded & has_file;
        controller.set_static_ready(is_ready);

        let static_running = controller.static_running();
        let static_ready = controller.static_ready();
        let available_models: Vec<ModelType> = ModelType::VARIANTS.to_vec();
        let system_theme = controller.get_system_theme();
        let theme = get_app_theme(system_theme);


        let file_name = file_path.file_name();

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_enabled_ui(!static_running, |ui| {
                ui.heading("Configuration");
                Grid::new("static_configs_grid")
                    .striped(true)
                    .show(ui, |ui| {
                        // MODEL ROW, see configs common.
                        // Contains dropdown to select model type, to open a downloaded model + download a model.
                        controller_common::model_row(
                            ui,
                            model,
                            &m_model,
                            downloaded,
                            controller.clone(),
                            available_models.as_slice(),
                            Some(theme),
                        );
                        ui.end_row();
                        // Num_threads
                        controller_common::n_threads_row(ui, n_threads, *max_threads);
                        ui.end_row();
                        let gpu_enabled = controller.gpu_enabled();
                        controller_common::use_gpu_row(ui, use_gpu, gpu_enabled);
                        ui.end_row();
                        // INPUT Language -> Set to auto for language detection
                        controller_common::set_language_row(ui, language);
                        ui.end_row();
                        // Translate (TO ENGLISH)
                        controller_common::set_translate_row(ui, set_translate);
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
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();
            let mic_occupied = controller.is_working() ^ static_running;
            ui.add_enabled_ui(!mic_occupied, |ui| {
                // Transcription section
                ui.add_enabled(static_ready, Label::new(RichText::new("Transcription").heading()));
                ui.vertical_centered_justified(|ui| {
                    if ui
                        .add_enabled(!static_running && static_ready, Button::new("Start"))
                        .clicked()
                    {
                        controller.start_static_transcription(file_path.as_path(), c_configs);
                    }

                    ui.add_space(constants::BLANK_SEPARATOR);

                    if ui
                        .add_enabled(static_running, Button::new("Stop"))
                        .clicked()
                    {
                        controller.stop_transcriber(false);
                    }
                });
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();

            ui.heading("Audio File");
            ui.vertical_centered_justified(|ui| {
                if ui.add_enabled(!static_running, Button::new("Open")).clicked() {
                    // Open File dialog at HOME directory, fallback to root.
                    let base_dirs = directories::BaseDirs::new();
                    let dir = if let Some(dir) = base_dirs {
                        dir.home_dir().to_path_buf()
                    } else {
                        PathBuf::from("/")
                    };
                    if let Some(p) = rfd::FileDialog::new()
                        .add_filter("Wave (.wav)", &["wav"])
                        .add_filter("mpeg (.mp3, mp4, m4v)", &["mp3, mp4, m4v"])
                        .set_directory(dir)
                        .pick_file()
                    {
                        *audio_path = Some(p);
                    }
                }

                if has_file {
                    ui.add_space(constants::BLANK_SEPARATOR);
                    if file_name.is_some() {
                        let name = file_name.unwrap();
                        if name == constants::TEMP_FILE {
                            ui.horizontal(|ui| {
                                ui.add(ok_icon(None, Some(theme)));
                                ui.label("Saved recording found!");
                            });
                        } else {
                            let name = file_name.unwrap();
                            let name = name.to_str().unwrap_or("Audio File");
                            ui.horizontal(|ui| {
                                ui.add(ok_icon(None, Some(theme)));
                                ui.label(name);
                            });
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.add(warning_icon(None, Some(theme)));
                            ui.label("Invalid file");
                        });
                    }
                }
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();
            // Saving
            let realtime_running = controller.realtime_running();
            ui.add_enabled_ui(!static_running && !realtime_running, |ui| {
                ui.heading("Saving");
                ui.vertical_centered_justified(|ui| {
                    // TODO: factor out to common.
                    if ui.add(Button::new("Save Transcription")).clicked() {
                        // Open File dialog at HOME directory, fallback to root.
                        let base_dirs = directories::BaseDirs::new();
                        let dir = if let Some(dir) = base_dirs {
                            dir.home_dir().to_path_buf()
                        } else {
                            PathBuf::from("/")
                        };

                        if let Some(p) = rfd::FileDialog::new()
                            .add_filter("text (.txt)", &["txt"])
                            .set_directory(dir)
                            .save_file()
                        {
                            controller.save_transcription(&p);
                        }
                    }
                    ui.add_space(constants::BLANK_SEPARATOR);
                    if ui.add(Button::new("Copy to Clipboard")).clicked() {
                        controller.copy_to_clipboard();
                    }
                });
            });
        });
    }

    // Right-click tab -> What should be shown.
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

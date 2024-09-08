use std::path::PathBuf;

use egui::{Button, Grid, Label, Pos2, RichText, ScrollArea, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use strum::VariantArray;
use whisper_realtime::{
    configs::Configs,
    model::{Model, ModelType},
};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::{
        tabs::controller_tabs::controller_common::{
            model_stack, n_threads_stack, save_transcription_button, set_language_stack,
            set_translate_stack, use_gpu_stack,
        },
        widgets::icons::{ok_icon, warning_icon},
    },
    ui::tabs::tab_view,
    utils::{constants, file_mgmt, preferences::get_app_theme, threading::get_max_threads},
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct StaticTab {
    title: String,
    static_configs: Configs,
    #[serde(skip)]
    #[serde(default = "get_max_threads")]
    max_threads: std::ffi::c_int,
    #[serde(skip)]
    audio_path: Option<PathBuf>,
    #[serde(skip)]
    last_mouse_pos: Pos2,
}

impl StaticTab {
    pub fn new_with_configs(configs: Configs) -> Self {
        let max_threads = get_max_threads();
        Self {
            title: String::from("Static"),
            static_configs: configs,
            max_threads,
            audio_path: None,
            last_mouse_pos: Default::default(),
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
            audio_path,
            last_mouse_pos,
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
        let data_dir =
            eframe::storage_dir(constants::APP_ID).expect("Failed to get data directory");
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
            ui.add_enabled_ui(!static_running, |ui| {
                ui.heading("Configuration");
                Grid::new("static_configs_grid")
                    .striped(true)
                    .num_columns(2)
                    .show(ui, |ui| {
                        // MODEL ROW, see configs common.
                        // Contains dropdown to select model type, to open a downloaded model + download a model.
                        model_stack(
                            ui,
                            model,
                            &m_model,
                            downloaded,
                            controller.clone(),
                            available_models.as_slice(),
                            Some(theme),
                            pointer_still,
                        );
                        ui.end_row();
                        // Num_threads
                        n_threads_stack(ui, n_threads, *max_threads, pointer_still);
                        ui.end_row();
                        let gpu_enabled = controller.gpu_enabled();
                        use_gpu_stack(ui, use_gpu, gpu_enabled, pointer_still);
                        ui.end_row();
                        // INPUT Language -> Set to auto for language detection
                        set_language_stack(ui, language, pointer_still);
                        ui.end_row();
                        // Translate (TO ENGLISH)
                        set_translate_stack(ui, set_translate, pointer_still);
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
                ui.add_enabled(
                    static_ready,
                    Label::new(RichText::new("Transcription").heading()),
                );
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
                if ui
                    .add_enabled(!static_running, Button::new("Open"))
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
                    save_transcription_button(ui, controller.clone());
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

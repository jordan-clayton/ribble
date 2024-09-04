use std::path::PathBuf;

use egui::{Button, Grid, Label, RichText, ScrollArea, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use sdl2::log::log;
use strum::{IntoEnumIterator, VariantArray};
use whisper_realtime::{configs::Configs, model::ModelType};
use whisper_realtime::model::Model;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::tab_view,
    utils::{constants, threading::get_max_threads},
};
use crate::ui::tabs::controller_tabs::controller_common;
use crate::ui::tabs::whisper_tab::FocusTab;
use crate::utils::preferences::get_app_theme;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct RealtimeTab {
    title: String,
    realtime_configs: Configs,
    #[serde(skip)]
    #[serde(default = "get_max_threads")]
    max_threads: std::ffi::c_int,
}

impl RealtimeTab {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn new_with_configs(configs: Configs) -> Self {
        let max_threads = get_max_threads();
        Self {
            title: String::from("Realtime"),
            realtime_configs: configs,
            max_threads,
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
        let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data directory");
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

        ScrollArea::vertical().show(ui, |ui| {
            ui.add_enabled_ui(!realtime_running, |ui| {
                ui.heading("Configuration");
                Grid::new("realtime_configs")
                    .striped(true)
                    .show(ui, |ui| {
                        // Model
                        controller_common::model_row(ui, model, &m_model, downloaded, controller.clone(), available_models.as_slice(), Some(theme));
                        ui.end_row();
                        // Num_threads
                        controller_common::n_threads_row(ui, n_threads, *max_threads);
                        ui.end_row();
                        // Use gpu
                        let gpu_enabled = controller.gpu_enabled();
                        controller_common::use_gpu_row(ui, use_gpu, gpu_enabled);
                        ui.end_row();
                        // INPUT Language -> Set to auto for language detection
                        controller_common::set_language_row(ui, language);
                        ui.end_row();
                        // Translate (TO ENGLISH)
                        controller_common::set_translate_row(ui, set_translate);
                        ui.end_row();
                        // Transcriber Timeout
                        ui.label("Transcription Timeout").on_hover_ui(|ui| {
                            ui.label("Set realtime timeout? Set to 0 to disable");
                        });

                        let mut rt_timeout = *realtime_timeout as u64 / 1000;

                        // MAX_REALTIME_TIMEOUT is in seconds.
                        // TODO: test the step-by + drag velocity
                        ui.horizontal(|ui| {
                            if ui.add(Slider::new(&mut rt_timeout, 0..=constants::MAX_REALTIME_TIMEOUT)
                                .step_by(1.0)
                                .drag_value_speed(1.0)
                            )
                                .changed() {
                                *realtime_timeout = (rt_timeout * 1000) as u128;
                                #[cfg(debug_assertions)]
                                log(&format!("realtime_timeout: {}", realtime_timeout));
                            };

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

                        // Audio chunk size (in ms) Min: 2s? Max: 30s
                        ui.label("Audio Sample Size").on_hover_ui(|ui| {
                            ui.label("Realtime audio is processed in chunks, (in ms). Tweak this value to improve transcription accuracy. Recommended: 10s / 10000ms");
                        });

                        let mut sample_ms = *audio_sample_ms as f32 / 1000.0;
                        if ui.add(Slider::new(&mut sample_ms, constants::MIN_AUDIO_CHUNK_SIZE..=constants::MAX_AUDIO_CHUNK_SIZE)
                            .step_by(0.5).suffix("s")).changed() {
                            *audio_sample_ms = (sample_ms * 1000.0) as usize;

                            #[cfg(debug_assertions)]
                            log(&format!("audio_sample_ms: {}", audio_sample_ms));
                        }
                        ui.end_row();

                        let mut slider_phrase_timeout = *phrase_timeout as f32 / 1000.0;

                        ui.label("Phrase Timeout").on_hover_ui(|ui| {
                            ui.label("Estimated length of time per sentence/phrase. Tweak this value to improve accuracy and reduce accidental output duplication. Recommended: 3s");
                        });
                        if ui.add(Slider::new(&mut slider_phrase_timeout, constants::MIN_PHRASE_TIMEOUT..=constants::MAX_PHRASE_TIMEOUT)
                            .step_by(0.5).suffix("s")).changed() {
                            *phrase_timeout = (slider_phrase_timeout * 1000.0) as usize;
                            #[cfg(debug_assertions)]
                            log(&format!("phrase_timeout: {}", phrase_timeout));
                        }
                        ui.end_row();

                        // Voice Activity Detection chunk size (UI in Seconds), internally ms.
                        ui.label("Voice Activity Sample Size").on_hover_ui(|ui| {
                            ui.label("Voice activity is processed in small sample chunks. Tweak this value to improve detection accuracy. Recommended: 0.3s");
                        });

                        let mut vad_sec = *vad_sample_ms as f32 / 1000.0;

                        ui.horizontal(|ui| {
                            if ui.add(Slider::new(&mut vad_sec, constants::MIN_VAD_SEC..=constants::MAX_VAD_SEC).step_by(0.05).suffix("s")).changed() {
                                *vad_sample_ms = (vad_sec * 1000.0) as usize;
                                #[cfg(debug_assertions)]
                                log(&format!("vad_ms: {}", vad_sample_ms));
                            }
                        });
                        ui.end_row();
                        // Voice Activity probability threshold
                        // Label
                        ui.label("VAD Probability Threshold").on_hover_ui(|ui| {
                            ui.label("Set the minimum probability threshold for detecting speech. Tweak to improve detection accuracy. Recommended: 65%-80%");
                        });
                        ui.add(Slider::new(voice_probability_threshold, constants::MIN_VAD_PROBABILITY..=constants::MAX_VAD_PROBABILITY)
                            .custom_formatter(|n, _| {
                                let p = n * 100f64;
                                format!("{p:0.2}")
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
                        controller.start_realtime_transcription(c_configs);
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

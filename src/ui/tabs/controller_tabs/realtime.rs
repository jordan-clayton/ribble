use std::path::PathBuf;

use egui::{Button, Grid, ScrollArea, Slider, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};
use sdl2::log::log;
use strum::{IntoEnumIterator, VariantArray};
use whisper_realtime::{configs::Configs, model::ModelType};
use whisper_realtime::model::Model;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::tab_view,
    utils::{constants, recorder_configs::AudioConfigs, threading::get_max_threads},
};
use crate::ui::tabs::controller_tabs::controller_common;
use crate::utils::file_mgmt;
use crate::utils::preferences::get_app_theme;

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
            title: String::from("Realtime"),
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

impl tab_view::TabView for RealtimeConfigsTab {
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

        // Check for config-copy requests
        let req = controller.recv_realtime_configs_req();
        if let Ok(_) = req {
            controller
                .send_configs(AudioConfigs::Realtime(c_configs))
                .expect("Configs channel closed.");
        }

        // Update ready state
        let data_dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data directory");
        let m_model = Model::new_with_type_and_dir(*model, data_dir);
        let downloaded = m_model.is_downloaded();
        controller.update_realtime_ready(downloaded);

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
        // TODO: pass this to the controller_common model row function.
        let theme = get_app_theme(system_theme);

        // Layout test
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
                        // TODO: testing ui.
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

                        // NOTE: THIS CODING IS CURRENTLY DUPLICATED FOR PHRASE TIMEOUT. REQUIREMENTS MAY CHANGE & FACTORING OUT HAS LITTLE UTILITY AT THIS TIME.
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
                ui.add_space(constants::BLANK_SEPARATOR);
            });
            ui.separator();
            let mic_occupied = controller.is_working() ^ realtime_running;
            ui.add_enabled_ui(!mic_occupied, |ui| {
                // Transcription section
                ui.heading("Transcription");
                ui.vertical_centered_justified(|ui| {
                    if ui
                        .add_enabled(!realtime_running && realtime_ready, Button::new("Start"))
                        .clicked()
                    {
                        // TODO: factor out ctx.
                        let ctx = ui.ctx();
                        controller.start_realtime_transcription(&ctx);
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

            ui.heading("Saving");
            ui.add_enabled_ui(!realtime_running && !static_running, |ui| {
                ui.vertical_centered_justified(|ui| {
                    if ui.add(Button::new("Save Transcription")).clicked() {
                        log(&"Start save routine".to_string());
                    }
                    ui.add_space(constants::BLANK_SEPARATOR);
                    if ui.add(Button::new("Copy to Clipboard")).clicked() {
                        log(&"Start clipboard routine".to_string());
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

            ui.heading("Re-Transcribe");
            ui.add_enabled_ui(!realtime_running && can_save_audio, |ui| {
                ui.vertical_centered_justified(|ui| {
                    // TODO: refactor this.
                    if ui
                        .add_enabled(can_save_audio, Button::new("Open Static tab."))
                        .clicked()
                    {
                        let data_dir = eframe::storage_dir(constants::APP_ID)
                            .expect("Failed to get storage dir");
                        let path = file_mgmt::get_temp_file_path(&data_dir);
                        assert!(path.exists(), "Temporary file missing");
                        // TODO: change the -TAB- to static.
                        // Set a flag
                    }
                });
            });
        });

        // TopBottomPanel::top("realtime_configs_panel").resizable(false).show_inside(ui, |ui| {
        //     ui.add_enabled_ui(!realtime_running, |ui| {
        //         ui.heading("Configuration");
        //         Grid::new("realtime_configs")
        //             .striped(true)
        //             .show(ui, |ui| {
        //                 // Model
        //                 controller_common::model_row(ui, model, &m_model, downloaded, controller.clone(), available_models.as_slice(), Some(theme));
        //                 ui.end_row();
        //                 // Num_threads
        //                 controller_common::n_threads_row(ui, n_threads, *max_threads);
        //                 ui.end_row();
        //                 // Use gpu
        //                 let gpu_enabled = controller.gpu_enabled();
        //                 controller_common::use_gpu_row(ui, use_gpu, gpu_enabled);
        //                 ui.end_row();
        //                 // INPUT Language -> Set to auto for language detection
        //                 controller_common::set_language_row(ui, language);
        //                 ui.end_row();
        //                 // Translate (TO ENGLISH)
        //                 controller_common::set_translate_row(ui, set_translate);
        //                 ui.end_row();
        //                 // Transcriber Timeout
        //                 // TODO: testing ui.
        //                 ui.label("Transcription Timeout").on_hover_ui(|ui| {
        //                     ui.style_mut().interaction.selectable_labels = true;
        //                     ui.label("Set realtime timeout? Set to 0 to disable");
        //                 });
        //
        //                 let mut rt_timeout = *realtime_timeout as u64;
        //
        //                 ui.horizontal(|ui| {
        //                     if ui.add(Slider::new(&mut rt_timeout, 0..=constants::MAX_REALTIME_TIMEOUT as u64)
        //                         // Step by seconds: TODO: consider changing as needed
        //                         .step_by(1000f64))
        //                         .changed() {
        //                         *realtime_timeout = rt_timeout as u128;
        //                     };
        //
        //                     ui.label({
        //                         let h = *realtime_timeout / (60 * 60);
        //                         let m = (*realtime_timeout / 60) % 60;
        //                         let s = *realtime_timeout % 60;
        //                         format!("{h:02}H : {m:02}M : {s:02}S:")
        //                     });
        //                 });
        //                 ui.end_row();
        //
        //                 // Audio chunk size (in ms) Min: 2s? Max: 30s
        //                 ui.label("Audio Sample Size").on_hover_ui(|ui| {
        //                     ui.style_mut().interaction.selectable_labels = true;
        //                     ui.label("Realtime audio is processed in chunks, (in ms). Tweak this value to improve transcription accuracy. Recommended: 10s / 10000ms");
        //                 });
        //
        //                 // NOTE: THIS CODING IS CURRENTLY DUPLICATED FOR PHRASE TIMEOUT. REQUIREMENTS MAY CHANGE & FACTORING OUT HAS LITTLE UTILITY AT THIS TIME.
        //                 ui.horizontal(|ui| {
        //                     ui.add(Slider::new(audio_sample_ms, constants::MIN_AUDIO_CHUNK_SIZE..=constants::MAX_AUDIO_CHUNK_SIZE)
        //                         .step_by(100f64));
        //                     ui.label({
        //                         // TODO: if precision is weird, use f64
        //                         let s = (*audio_sample_ms as f32) / 1000f32;
        //                         format!("{s:.3} seconds")
        //                     })
        //                 });
        //                 ui.end_row();
        //
        //                 // Phrase timeout (in ms) Min: 2s? Max: 10s?
        //                 ui.label("Phrase Timeout Size").on_hover_ui(|ui| {
        //                     ui.style_mut().interaction.selectable_labels = true;
        //                     ui.label("Phrase timeout is the estimated length of time per sentence. Tweak this value to improve accuracy and reduce accidental output duplication. Recommended: 3s / 3000ms");
        //                 });
        //                 ui.horizontal(|ui| {
        //                     ui.add(Slider::new(phrase_timeout, constants::MAX_PHRASE_TIMEOUT..=constants::MAX_PHRASE_TIMEOUT)
        //                         .step_by(100f64));
        //                     ui.label({
        //                         let s = (*phrase_timeout as f32) / 1000f32;
        //                         format!("{s:.3} seconds")
        //                     })
        //                 });
        //                 ui.end_row();
        //                 // Voice Activity Detection chunk size (in ms)
        //                 ui.label("Voice Activity Sample Size").on_hover_ui(|ui| {
        //                     ui.style_mut().interaction.selectable_labels = true;
        //                     ui.label("Voice activity is processed in small sample chunks. Tweak this value to improve detection accuracy. Recommended: 300ms");
        //                 });
        //                 ui.horizontal(|ui| {
        //                     ui.add(Slider::new(vad_sample_ms, constants::MIN_VAD_SAMPLE_MS..=constants::MAX_VAD_SAMPLE_MS).step_by(10f64));
        //                     ui.label({
        //                         let ms = (*vad_sample_ms as f32) / 1000f32;
        //                         format!("{ms:04} ms")
        //                     })
        //                 });
        //                 ui.end_row();
        //                 // Voice Activity probability threshold
        //                 // Label
        //                 ui.label("Voice Detection Probability").on_hover_ui(|ui| {
        //                     ui.style_mut().interaction.selectable_labels = true;
        //                     ui.label("Set the minimum probability threshold for detecting speech. Tweak to improve detection accuracy. Recommended: 65%-80%");
        //                 });
        //                 // This is represented in percentages to be slightly more intuitive.
        //                 ui.add(Slider::new(voice_probability_threshold, constants::MIN_VAD_PROBABILITY..=constants::MAX_VAD_PROBABILITY)
        //                     .custom_formatter(|n, _| {
        //                         let p = n * 100f64;
        //                         format!("{p:.20}")
        //                     }).suffix("%")
        //                     .custom_parser(|s| {
        //                         let str = s.chars().filter(|c| c.is_numeric() || *c == '.').collect::<String>();
        //                         str.parse::<f64>().ok()
        //                     })
        //                 );
        //                 ui.end_row();
        //                 // Reset defaults button.
        //                 ui.label("Reset To Defaults");
        //                 if ui.button("Reset").clicked() {
        //                     let default = Configs::default();
        //                     let Configs {
        //                         n_threads: default_n_threads,
        //                         set_translate: default_set_translate,
        //                         language: default_language,
        //                         use_gpu: default_use_gpu,
        //                         model: default_model,
        //                         realtime_timeout: default_realtime_timeout,
        //                         audio_sample_ms: default_audio_sample_ms,
        //                         vad_sample_ms: default_vad_sample_ms,
        //                         phrase_timeout: default_phrase_timeout,
        //                         voice_probability_threshold: default_voice_probability_threshold,
        //                         naive_vad_freq_threshold: _,
        //                         naive_vad_energy_threshold: _,
        //                         naive_window_len: _,
        //                         naive_window_step: _,
        //                         print_special: _,
        //                         print_progress: _,
        //                         print_realtime: _,
        //                         print_timestamps: _,
        //                     } = default;
        //
        //                     *n_threads = default_n_threads;
        //                     *set_translate = default_set_translate;
        //                     *language = default_language;
        //                     *use_gpu = default_use_gpu;
        //                     *model = default_model;
        //                     *realtime_timeout = default_realtime_timeout;
        //                     *audio_sample_ms = default_audio_sample_ms;
        //                     *vad_sample_ms = default_vad_sample_ms;
        //                     *phrase_timeout = default_phrase_timeout;
        //                     *voice_probability_threshold = default_voice_probability_threshold;
        //                 }
        //                 ui.end_row();
        //             });
        //         ui.add_space(constants::BLANK_SEPARATOR);
        //     });
        // });
        //
        // CentralPanel::default().show_inside(ui, |ui| {
        //     let mic_occupied = controller.is_working() ^ realtime_running;
        //     ui.add_enabled_ui(!mic_occupied, |ui| {
        //         // Transcription section
        //         ui.heading("Transcription");
        //         ui.vertical_centered_justified(|ui| {
        //             if ui
        //                 .add_enabled(!realtime_running && realtime_ready, Button::new("Start"))
        //                 .clicked()
        //             {
        //                 // TODO: factor out ctx.
        //                 let ctx = ui.ctx();
        //                 controller.start_realtime_transcription(&ctx);
        //             }
        //
        //             ui.add_space(constants::BLANK_SEPARATOR);
        //
        //             if ui
        //                 .add_enabled(realtime_running, Button::new("Stop"))
        //                 .clicked()
        //             {
        //                 controller.stop_transcriber(true);
        //             }
        //         });
        //         ui.add_space(constants::BLANK_SEPARATOR);
        //     });
        //     ui.separator();
        //
        //     // Saving
        //     let can_save_audio = controller.save_recording_ready();
        //     // Static transcription check
        //     let static_running = controller.static_running();
        //
        //     ui.heading("Saving");
        //     ui.add_enabled_ui(!realtime_running && !static_running, |ui| {
        //         ui.vertical_centered_justified(|ui| {
        //             if ui.add(Button::new("Save Transcription")).clicked() {
        //                 log(&"Start save routine".to_string());
        //             }
        //             ui.add_space(constants::BLANK_SEPARATOR);
        //             if ui.add(Button::new("Copy to Clipboard")).clicked() {
        //                 log(&"Start clipboard routine".to_string());
        //             }
        //             ui.add_space(constants::BLANK_SEPARATOR);
        //             // Save audio
        //             if ui
        //                 .add_enabled(can_save_audio, Button::new("Save Recording"))
        //                 .clicked()
        //             {
        //                 // Open File dialog at HOME directory, fallback to root.
        //                 let base_dirs = directories::BaseDirs::new();
        //                 let dir = if let Some(dir) = base_dirs {
        //                     dir.home_dir().to_path_buf()
        //                 } else {
        //                     PathBuf::from("/")
        //                 };
        //
        //                 if let Some(p) = rfd::FileDialog::new()
        //                     .add_filter("wave (.wav)", &["wav"])
        //                     .set_directory(dir)
        //                     .save_file()
        //                 {
        //                     controller.save_audio_recording(&p);
        //                 }
        //             }
        //         });
        //         ui.add_space(constants::BLANK_SEPARATOR);
        //     });
        //     ui.separator();
        //
        //     ui.heading("Re-Transcribe");
        //     ui.add_enabled_ui(!realtime_running && can_save_audio, |ui| {
        //         ui.vertical_centered_justified(|ui| {
        //             // TODO: refactor this.
        //             if ui
        //                 .add_enabled(can_save_audio, Button::new("Open Static tab."))
        //                 .clicked()
        //             {
        //                 let data_dir = eframe::storage_dir(constants::APP_ID)
        //                     .expect("Failed to get storage dir");
        //                 let path = file_mgmt::get_temp_file_path(&data_dir);
        //                 assert!(path.exists(), "Temporary file missing");
        //                 // TODO: change the -TAB- to static.
        //                 // Set a flag
        //             }
        //         });
        //     });
        // });
    }

    // TODO: Determine if needed.
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

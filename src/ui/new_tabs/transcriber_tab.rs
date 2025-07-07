use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;
use crate::ui::new_tabs::ribble_tab::RibbleTabId;
use crate::ui::widgets::toggle_switch::toggle;
use crate::utils::vad_configs::{VadFrameSize, VadStrictness, VadType};
use ribble_whisper::audio::audio_backend::AudioBackend;
use ribble_whisper::audio::recorder::SampleSink;
use ribble_whisper::whisper::configs::Language;
use strum::IntoEnumIterator;
use crate::controller::{CompletedRecordingJobs, OfflineTranscriberFeedback};

#[derive(serde::Serialize, serde::Deserialize)]
pub(in crate::ui) struct TranscriberTab {
    #[serde(default = "true")]
    pub realtime: bool,
    #[serde(default)]
    pub recordings_buffer: Vec<(String, CompletedRecordingJobs)>,
    // TODO: maybe define these modals in separate files instead of tabs?
    // or just write it at the bottom of the draw method.
    #[serde(default = "false")]
    pub recording_modal: bool,
    #[serde(default = "false")]
    pub download_modal: bool,
    #[serde(default = "false")]
    pub copy_modal: bool,
}

impl TabView for TranscriberTab {
    fn tab_id(&self) -> RibbleTabId {
        RibbleTabId::Transcriber
    }

    fn tab_title(&mut self) -> egui::WidgetText {
        if self.realtime {
            "Real-time Transcription".into()
        } else {
            "File Transcription".into()
        }
    }

    fn pane_ui<S, A>(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        controller: RibbleController<A>,
    ) -> egui_tiles::UiResponse
    where
        S: SampleSink,
        A: AudioBackend<S>,
    {
        let transcription_running = controller.transcription_running();
        let audio_worker_running = controller.recorder_running() || transcription_running;

        let configs = *controller.read_transcription_configs();
        let vad_configs = *controller.read_vad_configs();
        // RUN TRANSCRIPTION
        // NOTE: branch when building the buttons.
        let can_run_transcription = configs.model_id().is_some() && !audio_worker_running;

        // TODO: move this to a ScrollArea once done.
        // HEADING
        let header_text = if self.realtime {
            "File Transcription"
        } else {
            "Real-time Transcription"
        };

        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true),
            |ui| {
                ui.heading(header_text);
                ui.horizontal_wrapped(|ui| {
                    ui.label("Switch mode:");
                    ui.add_enabled(!audio_worker_running, toggle(&mut self.realtime));
                });
            },
        );

        let button_spacing = ui.spacing().button_padding.y;
        // -- Scroll Area starts here
        // FUNCTIONS
        if self.realtime {
            // RUNNER BUTTONS: START + STOP + Re-Transcribe
            ui.vertical_centered_justified(|ui| {
                if ui
                    .add_enabled(can_run_transcription, egui::Button::new("Start Real-time"))
                    .clicked()
                {
                    controller.start_realtime_transcription();
                }
                ui.add_space(button_spacing);
                if ui
                    .add_enabled(transcription_running, egui::Button::new("Stop"))
                    .clicked()
                {
                    controller.stop_transcription();
                }
                ui.add_space(button_spacing);
                let latest_recording_exists = controller.latest_recording_exists();

                if ui
                    .add_enabled(
                        latest_recording_exists && can_run_transcription,
                        egui::Button::new("Re-transcribe Latest"),
                    ).on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Offline transcribe latest cached recording.\n\
                    Generally more accurate due to full audio context.");
                })
                    .clicked()
                {
                    self.realtime = false;
                    controller.try_retranscribe_latest();
                }
            });
        } else {
            // Get the audio file information.
            let current_audio_path = controller.read_current_audio_file_path();
            let current_file = match current_audio_path.as_ref() {
                Some(path) => {
                    path.file_name()
                }
                None => None
            };

            // RUNER BUTTONS: START + STOP
            ui.vertical_centered_justified(|ui| {
                let has_file = current_file.is_some();

                if ui.add_enabled(can_run_transcription && has_file, egui::Button::new("Start")).clicked() {
                    controller.start_offline_transcription();
                }

                if ui
                    .add_enabled(transcription_running, egui::Button::new("Stop"))
                    .clicked()
                {
                    controller.stop_transcription();
                }
            });
            ui.add_space(button_spacing);
            ui.separator();
            // AUDIO FILE: LOAD FILE, LOAD RECORDING, CLEAR
            ui.heading("Audio File");
            ui.vertical_centered_justified(|ui| {
                // AUDIO FILE
                egui::Grid::new("audio_file").num_columns(2).show(ui, |ui| {
                    ui.label("Current audio file:");
                    ui.horizontal(|ui| {
                        ui.label(current_file);
                        if ui.button("Clear").clicked() {
                            controller.clear_audio_file_path();
                        }
                    });
                });
                // TODO: determine whether to add button spacing between the grid or not.
                ui.add_space(button_spacing);
                if ui.add_enabled(!transcription_running, egui::Button::new("Open file")).clicked() {
                    let mut file_dialog = rfd::FileDialog::new()
                        .add_filter("all supported", &["wav", "mpa", "mp2", "mp3", "mp4", "m4v", "ogg", "mkv", "aif", "aiff", "aifc", "caf", "alac", "flac"])
                        .add_filter("wav", &["wav"])
                        .add_filter("mpeg", &["mpa", "mp2", "mp3", "mp4", "m4v"])
                        .add_filter("aiff", &["aif", "aiff", "aifc"])
                        .add_filter("caf", &["caf"])
                        .add_filter("mkv", &["mkv"])
                        .add_filter("alac", &["alac"])
                        .add_filter("flac", &["flac"]);

                    match directories::BaseDirs::new() {
                        None => {
                            file_dialog = file_dialog.set_directory("/");
                        }
                        Some(dirs) => {
                            file_dialog = file_dialog.set_directory(dirs.home_dir());
                        }
                    }

                    if let Some(p) = file_dialog.pick_file() {
                        controller.set_audio_file_path(p);
                    }
                }
                ui.add_space(button_spacing);
                if ui.add_enabled(!transcription_running, egui::Button::new("Load recording")).clicked() {
                    *self.recording_modal = true;
                    todo!("Define recording modal");
                }
            });
            ui.add_space(button_spacing);
            ui.separator();

            ui.heading("Feedback Mode");
            // FEEDBACK MODE -> possibly hide this, but it seems important to have accessible.
            egui::Grid::new("offline_feedback").num_columns(2).show(ui, |ui| {
                let mut offline_feedback = controller.read_offline_transcriber_feedback();
                ui.label("Feedback mode.").on_hover_ui(|ui| {
                    ui.style_mut().interaction.selectable_labels = true;
                    ui.label("Set the feedback mode for file transcription.\n\
                    Progressive: Enables live updates. Significantly degrades performance.\n\
                    Minimal: Disables live updates. Significant increases performance.");
                });
                egui::ComboBox::from_id_salt("feedback_mode")
                    .selected_text(offline_feedback.as_ref()).show_ui(ui, |ui| {
                    for feedback_mode in OfflineTranscriberFeedback::iter() {
                        if ui.selectable_value(&mut offline_feedback, feedback_mode, feedback_mode.as_ref()).clicked() {
                            controller.write_offline_transcriber_feedback(offline_feedback);
                        }
                    }
                });
                ui.end_row();
            });
        }

        ui.add_space(button_spacing);
        ui.separator();
        // CONFIGS GRIDS
        ui.heading("Configs.");
        // Disable the configs interaction if the main runner is running
        ui.collapsing("Transcription Configs", |ui| {
            ui.add_enabled_ui(!audio_worker_running, |ui| {
                let row_height = ui.spacing().interact_size.y;
                // NOTE: THIS IS A LITTLE FRAGILE -> This should reflect the number of rows in the grid
                // Realtime has +2 extra features
                let total_rows = if self.realtime {
                    10
                } else {
                    8
                };
                egui::ScrollArea::vertical().show_rows(
                    ui,
                    row_height,
                    total_rows,
                    |ui, row_range| {
                        egui::Grid::new("realtime_configs")
                            .num_columns(2)
                            .striped(true)
                            .start_row(row_range.start)
                            .show(ui, |ui| {
                                // TODO handle model stuff:
                                // Get the list of models from the model bank.
                                // Unpack in the combobox loop to get the (key, Model)
                                // Only need to get the model's "name".

                                // ROW: MODEL
                                ui.label("Model:");
                                // NOTE: this might be too many buttons, lol, test and see.
                                ui.horizontal_wrapped(|ui| {
                                    // TODO: combobox of selectable values
                                    if ui.button("Open Model").clicked() {
                                        *self.copy_modal = true;
                                    }
                                    if ui.button("Download Model").clicked() {
                                        *self.download_modal = true;
                                    }

                                    if ui.button("Open Models Folder").clicked() {
                                        let model_directory = controller.get_model_directory();
                                        // Try and open it in the default file explorer.
                                        let _ = opener::reveal(model_directory);
                                    }
                                    if ui.button("Refresh Models").clicked() {
                                        // TODO: refactor this once model bank finished.
                                        let _ = controller.refresh_model_bank();
                                    }
                                });
                                ui.end_row();
                                // ROW: NUM THREADS
                                let mut n_threads = configs.n_threads();
                                let thread_range = 1..=controller.max_whisper_threads();
                                ui.label("No. threads:").on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label("Set the number of threads to allocate to whisper. Recommended 7");
                                });
                                // TODO: if this gets too janky, consider using caching and dirty-writes.
                                if ui.add(egui::Slider::new(&mut n_threads, thread_range).integer()).is_pointer_button_down_on() {
                                    let new_configs = configs.with_n_threads(n_threads);
                                    controller.write_transcription_configs(new_configs)
                                }
                                ui.end_row();
                                // NOTE: if it becomes imperative to expose past prompt tokens,
                                // do so around here, but it shouldn't be relevant.
                                // ROW: SET TRANSLATE
                                ui.label("Translate (En):").on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label("Translate the transcription (English only).");
                                });
                                let mut translate = configs.translate();
                                if ui.add(egui::Checkbox::without_text(&mut translate)).clicked() {
                                    let new_configs = configs.set_translate(translate);
                                    controller.write_transcription_configs(new_configs)
                                }
                                ui.end_row();
                                // ROW: LANGUAGE
                                ui.label("Language:").on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label("Set the input audio language.\n\
                                    Set to Auto for auto-detection.");
                                });

                                let salt = "select language";
                                // NOTE TO SELF: implement Language::default();
                                // It's fine for now: Default = None = Auto anyway.
                                let mut language = configs.language().unwrap_or(Language::Auto);
                                egui::ComboBox::from_id_salt(salt)
                                    .selected_text(language.as_ref()).show_ui(ui, |ui| {
                                    for lang in Language::iter() {
                                        if ui.selectable_value(&mut language, lang, lang.as_ref()).clicked() {
                                            let new_configs = configs.with_language(Some(language));
                                            controller.write_transcription_configs(new_configs);
                                        }
                                    }
                                });
                                ui.end_row();
                                // ROW: SET GPU
                                ui.label("Hardware Acceleration:").on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label("Toggles transcription hardware acceleration via the GPU.\n\
                                    Real-time transcription may not be feasible without hardware acceleration.");
                                });
                                let mut using_gpu = configs.using_gpu();
                                if ui.add(egui::Checkbox::without_text(&mut using_gpu)).clicked() {
                                    let new_configs = configs.set_gpu(using_gpu);
                                    controller.write_transcription_configs(new_configs);
                                }
                                ui.end_row();
                                // ROW: USE NO CONTEXT
                                ui.label("Use Context:").on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label("Use previous context to inform transcription.\nImproves accuracy but may introduce real-time artefacts.");
                                });

                                let mut using_context = !configs.using_no_context();
                                if ui.add(egui::Checkbox::without_text(&mut using_context)).clicked() {
                                    let new_configs = configs.set_use_no_context(!using_context);
                                    controller.write_transcription_configs(new_configs);
                                }
                                ui.end_row();
                                // ROW: SET FLASH ATTENTION
                                ui.label("Use Flash Attention:").on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label("Toggles Flash Attention (if supported).\nSignificantly increases performance.");
                                });

                                let mut using_flash_attention = configs.using_flash_attention();
                                if ui.add(egui::Checkbox::without_text(&mut using_flash_attention)).clicked() {
                                    let new_configs = configs.set_flash_attention(using_flash_attention);
                                    controller.write_transcription_configs(new_configs);
                                }
                                ui.end_row();
                                // -- REALTIME specific configs.
                                if self.realtime {
                                    // ROW: REALTIME TIMEOUT -> PREDEFINE (NONE, 15 MIN, 30 MIN, 1HR, 2HR)
                                    // TODO: define AudioTimeout enum.
                                    // ROW: SAMPLE LEN -> PREDEFINE (3s, 5s, 10s, 20s?)
                                    // TODO: define AudioSampleLen enum.
                                    // -> larger sizes will be more accurate but less responsive (Longer whisper, might be costly)
                                    // -> smaller sizes will be less accurate but more responsive (shorter whisper, might burn cycles)
                                    // NOTE: don't expose the vad ms unless absolutely necessary.

                                }
                                // ROW: RESET TO DEFAULTS.
                                ui.label("Reset settings:");
                                if ui.button("Reset").clicked() {
                                    // Since real-time configs expose some more parameters,
                                    // Only reset the whisper configs if resetting from offline mode.
                                    let new_configs = if self.realtime {
                                        Default::default();
                                    } else {
                                        configs.with_whisper_configs(Default::default());
                                    };

                                    controller.write_transcription_configs(new_configs);
                                }
                                ui.end_row();
                            });
                    },
                );
            });
        });

        ui.add_space(button_spacing);
        ui.separator();

        ui.collapsing("Voice Activitiy Detector Configs.", |ui| {
            ui.add_enabled_ui(!audio_worker_running, |ui| {
                egui::Grid::new("vad_configs").num_columns(2).show(ui, |ui| {
                    // VAD TYPE
                    ui.label("VAD algorithm:").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Select which voice detection algorithm to use.\n\
                        Set to Auto for system defaults.");
                    });
                    let vad_type_salt = "vad_type";
                    let mut vad_type = vad_configs.vad_type();
                    egui::ComboBox::from_id_salt(vad_type_salt)
                        .selected_text(vad_type.as_ref()).show_ui(ui, |ui| {
                        for vad in VadType::iter() {
                            if ui.selectable_value(&mut vad_type, vad, vad.as_ref())
                                .on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label(vad.tooltip());
                                })
                                .clicked() {
                                let new_vad_configs = vad_configs.with_vad_type(vad_type);
                                controller.write_vad_configs(new_vad_configs);
                            }
                        }
                    });
                    ui.end_row();
                    // FRAME SIZE
                    ui.label("Frame size:").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Sets the length of the audio frame used to detect voice. \n\
                        Larger sizes may introduce latency but provide better results.\n\
                        Set to Auto for system defaults.");
                    });

                    let frame_size_salt = "vad_frame_size";
                    let mut frame_size = vad_configs.frame_size();
                    egui::ComboBox::from_id_salt(frame_size_salt)
                        .selected_text(frame_size.as_ref()).show_ui(ui, |ui| {
                        for size in VadFrameSize::iter() {
                            if ui.selectable_value(&mut frame_size, size, frame_size.as_ref()).clicked() {
                                let new_vad_configs = vad_configs.with_frame_size(frame_size);
                                controller.write_vad_configs(new_vad_configs);
                            }
                        }
                    });
                    ui.end_row();
                    // STRICTNESS
                    ui.label("Strictness:").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Sets the voice-detection thresholds.\n\
                        Higher strictness can improve performance, but may increase false negatives.\n\
                        Set to Auto for system defaults.");
                    });
                    let strictness_salt = "vad_strictness";
                    let mut vad_strictness = vad_configs.strictness();
                    egui::ComboBox::from_id_salt(strictness_salt)
                        .selected_text(vad_strictness.as_ref())
                        .show_ui(ui, |ui| {
                            for strictness in VadStrictness::iter() {
                                if ui.selectable_value(&mut vad_strictness, strictness, strictness.as_ref()).clicked() {
                                    let new_vad_configs = vad_configs.with_strictness(vad_strictness);
                                    controller.write_vad_configs(new_vad_configs);
                                }
                            }
                        });
                    ui.end_row();
                    // USE OFFLINE
                    let mut vad_use_offline = vad_configs.use_vad_offline();
                    ui.label("Use offline:").on_hover_ui(|ui| {
                        ui.style_mut().interaction.selectable_labels = true;
                        ui.label("Run VAD for file transcription.\n\
                        Significantly improves performance but may cause transcription artifacts.");
                    });
                    if ui.add(egui::Checkbox::without_text(&mut vad_use_offline)).clicked() {
                        let new_vad_configs = vad_configs.with_use_vad_offline(vad_use_offline);
                        controller.write_vad_configs(new_vad_configs);
                    }
                    ui.end_row();
                    ui.label("Reset settings:");
                    if ui.button("Reset").clicked() {
                        controller.write_vad_configs(Default::default());
                    }
                    ui.end_row();
                })
            });

            // MODALS -> this doesn't need to be in the scroll area.
            if self.recording_modal {
                let modal = egui::Modal::new(egui::Id::from("recording_modal")).show(ui.ctx(), |ui| {
                    // Basically just a scrollable list of recordings.
                    controller.try_get_completed_recordings(&mut self.recordings_buffer);
                    // List-tile style picker with a button to clear at either the top or the bottom.
                    // Use selectable labels.
                    // Not sure about how tall to make this? Maybe it auto sizes.
                    // Maybe also branch based on the length of the recordings buuufer?
                    // If it's empty, just ui.label( No saved recordings ).

                    todo!("Recording modal.");
                });

                if modal.should_close() {
                    *self.recording_modal = false;
                }
            }

            if self.download_modal {
                let modal = egui::Modal::new(egui::Id::from("download_modal")).show(ui.ctx(), |ui| {
                    todo!("download modal.");
                });

                if modal.should_close() {
                    *self.download_modal = false;
                }
            }

            if self.copy_modal {
                let modal = egui::Modal::new(egui::Id::from("copy_modal")).show(ui.ctx(), |ui| {
                    todo!("Copy Modal.")
                });

                if modal.should_close() {
                    *self.copy_modal = false;
                }
            }
        });
    }

    fn is_tab_closable(&self) -> bool {
        true
    }
}
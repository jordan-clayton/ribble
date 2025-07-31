use crate::controller::ribble_controller::RibbleController;
use crate::controller::{CompletedRecordingJobs, ModelFile, OfflineTranscriberFeedback};
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::PaneView;
use crate::ui::widgets::toggle_switch::toggle;
use crate::utils::realtime_settings::{AudioSampleLen, RealtimeTimeout, VadSampleLen};
use crate::utils::vad_configs::{VadFrameSize, VadStrictness, VadType};
use ribble_whisper::whisper::configs::Language;
use ribble_whisper::whisper::model::{DefaultModelType, ModelId};
use std::sync::Arc;
use strum::IntoEnumIterator;

// Icon button for opening a link to huggingface/a readme explainer
const LINK_ICON: &str = "🌐";

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TranscriberPane {
    #[serde(default = "set_realtime")]
    realtime: bool,
    #[serde(skip)]
    #[serde(default)]
    recordings_buffer: Vec<(Arc<str>, CompletedRecordingJobs)>,
    #[serde(skip)]
    #[serde(default)]
    model_list: Vec<(ModelId, ModelFile)>,
    #[serde(skip)]
    #[serde(default)]
    recording_modal: bool,
    #[serde(skip)]
    #[serde(default)]
    download_modal: bool,
    #[serde(skip)]
    #[serde(default)]
    model_url: String,
}

// This is for serde until it supports literals.
fn set_realtime() -> bool {
    true
}

impl Default for TranscriberPane {
    fn default() -> Self {
        Self {
            realtime: true,
            recordings_buffer: vec![],
            model_list: vec![],
            recording_modal: false,
            download_modal: false,
            model_url: Default::default(),
        }
    }
}

impl PaneView for TranscriberPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Transcriber
    }

    fn pane_title(&self) -> egui::WidgetText {
        if self.realtime {
            "Real-time Transcription".into()
        } else {
            "File Transcription".into()
        }
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _should_close: &mut bool,
        controller: RibbleController,
    ) -> egui::Response {
        let transcription_running = controller.transcriber_running();
        let audio_worker_running = controller.recorder_running() || transcription_running;

        let configs = *controller.read_transcription_configs();
        let vad_configs = *controller.read_vad_configs();
        let current_model = (*configs
            .model_id())
            .and_then(|id| self.model_list.iter().find(|(k, _)| *k == id)).cloned();

        // RUN TRANSCRIPTION
        let can_run_transcription = current_model.is_some() && !audio_worker_running;

        // HEADING
        let header_text = if self.realtime {
            "Real-time Transcription"
        } else {
            "File Transcription"
        };

        // TODO: this might not work just yet - test out and remove this todo if it's right.
        // Create a (hopefully) lower-priority pane-sized interaction hitbox
        // Handle dragging the UI.
        let pane_id = egui::Id::new("transcriber_pane");
        // Return the interaction response.
        let resp = ui.interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // MAIN PANEL FRAME
        egui::Frame::default().show(ui, |ui| {
            // TODO: cop the trick used in the main app to get this to align properly.
            let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
            let header_width = ui.max_rect().width();
            let desired_size = egui::Vec2::new(header_width, header_height);
            let layout = egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true).with_main_wrap(true);

            ui.allocate_ui_with_layout(
                desired_size,
                layout,
                |ui| {
                    ui.heading(header_text);
                    ui.horizontal_wrapped(|ui| {
                        ui.label("Switch mode:");
                        ui.add_enabled(!audio_worker_running, toggle(&mut self.realtime));
                    });
                },
            );

            let button_spacing = ui.spacing().button_padding.y;
            egui::ScrollArea::both().show(ui, |ui| {
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
                            // NOTE: this might be a little too TOCTOU prone.
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
                        egui::Grid::new("audio_file").num_columns(2).striped(true).show(ui, |ui| {
                            ui.label("Current audio file:");
                            ui.horizontal(|ui| {
                                ui.label(format!("{current_file:#?}"));
                                if ui.button("Clear").clicked() {
                                    controller.clear_audio_file_path();
                                }
                            });
                        });

                        // TODO: determine whether to add button spacing between the grid or not.
                        ui.add_space(button_spacing);
                        if ui.add_enabled(!transcription_running, egui::Button::new("Open file")).clicked() {
                            let file_dialog = rfd::FileDialog::new()
                                .add_filter("all supported",
                                            &["wav", "mpa", "mp2", "mp3", "mp4", "m4v", "ogg", "mkv", "aif", "aiff", "aifc", "caf", "alac", "flac"])
                                .add_filter("wav", &["wav"])
                                .add_filter("mpeg", &["mpa", "mp2", "mp3", "mp4", "m4v"])
                                .add_filter("aiff", &["aif", "aiff", "aifc"])
                                .add_filter("caf", &["caf"])
                                .add_filter("mkv", &["mkv"])
                                .add_filter("alac", &["alac"])
                                .add_filter("flac", &["flac"])
                                .set_directory(controller.base_dir());

                            if let Some(path) = file_dialog.pick_file() {
                                controller.set_audio_file_path(path);
                            }
                        }
                        ui.add_space(button_spacing);
                        if ui.add_enabled(!transcription_running, egui::Button::new("Load recording")).clicked() {
                            self.recording_modal = true;
                        }
                    });
                    ui.add_space(button_spacing);
                    ui.separator();

                    ui.heading("Feedback Mode");
                    // FEEDBACK MODE -> possibly hide this, but it seems important to have accessible.
                    egui::Grid::new("offline_feedback").num_columns(2).striped(true).show(ui, |ui| {
                        let mut offline_feedback = controller.read_offline_transcriber_feedback();
                        ui.label("Feedback mode.").on_hover_ui(|ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            ui.label("Set the feedback mode for file transcription.\n\
                            Progressive: Enables live updates. Significantly degrades performance.\n\
                            Minimal: Disables live updates. Significant increases performance.");
                        });
                        egui::ComboBox::from_id_salt("feedback_mode_combobox")
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
                ui.heading("Configs:");
                // Disable the configs interaction if the main runner is running
                ui.collapsing("Transcription Configs", |ui| {
                    ui.add_enabled_ui(!audio_worker_running, |ui| {
                        // NOTE: THIS IS A LITTLE FRAGILE -> This should reflect the number of rows in the grid
                        // Realtime has +3 extra features

                        // Also, the wrapping might cause things to get a little crusty -> test
                        // this out.
                        let row_height = ui.spacing().interact_size.y;
                        let total_rows = if self.realtime {
                            12
                        } else {
                            9
                        };
                        egui::ScrollArea::vertical().show_rows(
                            ui,
                            row_height,
                            total_rows,
                            |ui, row_range| {
                                egui::Grid::new("realtime_configs_grid")
                                    .num_columns(2)
                                    .striped(true)
                                    .start_row(row_range.start)
                                    .show(ui, |ui| {
                                        // ROW: MODEL
                                        ui.label("Model:");
                                        // NOTE: this might be too many buttons, test and see.
                                        ui.horizontal_wrapped(|ui| {
                                            // Try-Get the model list from the controller.
                                            controller.try_read_model_list(&mut self.model_list);
                                            // Get a clone of the model_id to modify
                                            let mut model_id = *configs.model_id();

                                            let model_id_combobox = match current_model {
                                                Some((_, file)) => {
                                                    match file {
                                                        ModelFile::Packed(idx) => {
                                                            egui::ComboBox::from_id_salt("model_id_combobox")
                                                                .selected_text(ModelFile::PACKED_NAMES[idx])
                                                        }
                                                        ModelFile::File(name) => {
                                                            egui::ComboBox::from_id_salt("model_id_combobox")
                                                                .selected_text(name.as_ref())
                                                        }
                                                    }
                                                }
                                                None => {
                                                    egui::ComboBox::from_id_salt("model_id_combobox")
                                                        .selected_text("Select a model.")
                                                }
                                            };

                                            model_id_combobox.show_ui(ui, |ui| {
                                                for (m_id, model_file) in self.model_list.iter() {
                                                    match model_file {
                                                        ModelFile::Packed(idx) => {
                                                            if ui.selectable_value(&mut model_id, Some(*m_id), ModelFile::PACKED_NAMES[*idx])
                                                                .clicked() {
                                                                let new_configs = configs.with_model_id(model_id);
                                                                controller.write_transcription_configs(new_configs);
                                                            }
                                                        }
                                                        ModelFile::File(file_name) => {
                                                            if ui.selectable_value(&mut model_id, Some(*m_id), file_name.as_ref())
                                                                .clicked() {
                                                                let new_configs = configs.with_model_id(model_id);
                                                                controller.write_transcription_configs(new_configs);
                                                            }
                                                        }
                                                    };
                                                }
                                            });

                                            if ui.button("Open Model").clicked() {
                                                let file_dialog = rfd::FileDialog::new()
                                                    .add_filter("ggml-model", &[".bin"])
                                                    .set_directory(controller.base_dir());

                                                // If there is path, it is a ".bin".
                                                // At the moment, there's no integrity checking
                                                // mechanisms
                                                if let Some(path) = file_dialog.pick_file() {
                                                    controller.copy_new_model(path);
                                                }
                                            }
                                            if ui.button("Download Model").clicked() {
                                                self.download_modal = true;
                                            }
                                        });
                                        ui.end_row();

                                        // ROW: OPEN MODEL FOLDER
                                        ui.label("Models Folder");
                                        if ui.button("Open Models Folder").clicked() {
                                            let model_directory = controller.get_model_directory();
                                            // Try and open it in the default file explorer.
                                            // There's a debouncer in the model-bank that will
                                            // keep the list mostly up to date.
                                            let _ = opener::reveal(model_directory);
                                        }
                                        ui.end_row();

                                        // ROW: NUM THREADS
                                        let mut n_threads = configs.n_threads();
                                        let thread_range = 1..=controller.max_whisper_threads();
                                        ui.label("No. threads:").on_hover_ui(|ui| {
                                            ui.style_mut().interaction.selectable_labels = true;
                                            ui.label("Set the number of threads to allocate to whisper. Recommended: 7.");
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
                                            Set to Auto for automatic language-detection.");
                                        });

                                        // NOTE TO SELF: implement Language::default() in Ribble-Whisper;
                                        // It's fine for now: Default = None = Auto anyway.
                                        let mut language = configs.language().unwrap_or(Language::Auto);
                                        egui::ComboBox::from_id_salt("select_language_combobox")
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
                                            ui.label("Use previous context to inform transcription.\n\
                                            Improves accuracy but may introduce real-time artefacts.");
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
                                            ui.label("Toggles Flash Attention (if supported).\n\
                                            Significantly increases performance.");
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
                                            let mut realtime_timeout: RealtimeTimeout = configs.realtime_timeout().into();

                                            // NOTE: when the app gets constructed with default
                                            // settings, the time should be 3600 ms = 1 hr, so this
                                            // should always map 1:1 with the enum members.
                                            #[cfg(debug_assertions)]
                                            {
                                                let test_timeout: u128 = realtime_timeout.into();
                                                assert_eq!(test_timeout, configs.realtime_timeout());
                                            }
                                            ui.label("Timeout:").on_hover_ui(|ui| {
                                                ui.style_mut().interaction.selectable_labels = true;
                                                ui.label("Set the timeout for real-time transcription.\n\
                                                Set to infinite for continuous sessions, but note that performance may degrade.");
                                            });

                                            let rt_timeout_id = egui::Id::new("realtime_timeout_id");

                                            egui::ComboBox::from_id_salt(rt_timeout_id)
                                                .selected_text(realtime_timeout.as_ref())
                                                .show_ui(ui, |ui| {
                                                    for timeout_len in RealtimeTimeout::iter() {
                                                        if ui.selectable_value(&mut realtime_timeout, timeout_len, timeout_len.as_ref())
                                                            .clicked() {
                                                            let new_timeout: u128 = realtime_timeout.into();
                                                            let new_configs = configs.with_realtime_timeout(new_timeout);
                                                            controller.write_transcription_configs(new_configs);
                                                        }
                                                    }
                                                });
                                            ui.end_row();

                                            // ROW: SAMPLE LEN
                                            let mut audio_sample_len: AudioSampleLen = configs.audio_sample_len_ms().into();

                                            // As with realtime-timeout above, the following should
                                            // always have a clean 1:1 mapping between configs and
                                            // enum members.
                                            #[cfg(debug_assertions)]
                                            {
                                                let test_len: usize = audio_sample_len.into();
                                                assert_eq!(test_len, configs.audio_sample_len_ms());
                                            }

                                            ui.label("Audio Sample size:").on_hover_ui(|ui| {
                                                ui.style_mut().interaction.selectable_labels = true;
                                                ui.label("Sets the audio sampling buffer size.\n\
                                                Smaller sizes: lower latency, lower accuracy, higher power draw.\n\
                                                Larger sizes: higher latency, higher accuracy, lower power draw.");
                                            });

                                            let a_sample_id = egui::Id::new("audio_sample_len");

                                            egui::ComboBox::from_id_salt(a_sample_id)
                                                .selected_text(audio_sample_len.as_ref())
                                                .show_ui(ui, |ui| {
                                                    for sample_len in AudioSampleLen::iter() {
                                                        if ui.selectable_value(&mut audio_sample_len, sample_len, sample_len.as_ref())
                                                            .clicked() {
                                                            let new_audio_ms: usize = audio_sample_len.into();
                                                            let new_configs = configs.with_audio_sample_len(new_audio_ms);
                                                            controller.write_transcription_configs(new_configs);
                                                        }
                                                    }
                                                });
                                            ui.end_row();

                                            // ROW: VAD SAMPLE LEN
                                            let mut vad_sample_len: VadSampleLen = configs.vad_sample_len().into();
                                            // As with the previous assertions, the enum-usize
                                            // mapping should be 1:1, since ribble_whisper's
                                            // defaults map to at least 1 enum member.
                                            // This is just a sanity check that will fail on a
                                            // clean start if that assumption is false.
                                            #[cfg(debug_assertions)]
                                            {
                                                let test_len: usize = vad_sample_len.into();
                                                assert_eq!(test_len, configs.vad_sample_len());
                                            }

                                            ui.label("VAD Sample size:").on_hover_ui(|ui| {
                                                ui.style_mut().interaction.selectable_labels = true;
                                                ui.label("Sets the voice-activity sampling buffer size.\n\
                                                Smaller sizes: lower latency, lower accuracy, higher power draw.\n\
                                                Larger sizes: higher latency, higher accuracy, lower power draw.");
                                            });

                                            egui::ComboBox::from_id_salt("vad_sample_len_combobox")
                                                .selected_text(vad_sample_len.as_ref())
                                                .show_ui(ui, |ui| {
                                                    for sample_len in VadSampleLen::iter() {
                                                        if ui.selectable_value(&mut vad_sample_len, sample_len, sample_len.as_ref())
                                                            .clicked() {
                                                            let new_vad_ms: usize = vad_sample_len.into();
                                                            let new_configs = configs.with_vad_sample_len(new_vad_ms);
                                                            controller.write_transcription_configs(new_configs);
                                                        }
                                                    }
                                                });

                                            ui.end_row();
                                        }

                                        // ROW: RESET TO DEFAULTS.
                                        ui.label("Reset settings:");
                                        if ui.button("Reset").clicked() {
                                            // Since real-time configs expose some more parameters,
                                            // Only reset the whisper configs if resetting from offline mode.
                                            let new_configs = if self.realtime {
                                                Default::default()
                                            } else {
                                                configs.with_whisper_configs(Default::default())
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
                        egui::Grid::new("vad_configs_grid").striped(true)
                            .num_columns(2)
                            .show(ui, |ui| {
                                // VAD TYPE
                                ui.label("VAD algorithm:").on_hover_ui(|ui| {
                                    ui.style_mut().interaction.selectable_labels = true;
                                    ui.label("Select which voice detection algorithm to use.\n\
                                Set to Auto for system defaults.");
                                });

                                let mut vad_type = vad_configs.vad_type();
                                egui::ComboBox::from_id_salt("vad_type_combobox")
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
                                    ui.label("Sets the length of the audio frame used to detect voice.\n\
                                    Larger sizes may introduce latency but provide better results.\n\
                                    Set to Auto for system defaults.");
                                });

                                let mut frame_size = vad_configs.frame_size();
                                egui::ComboBox::from_id_salt("vad_frame_size_combobox")
                                    .selected_text(frame_size.as_ref()).show_ui(ui, |ui| {
                                    for size in VadFrameSize::iter() {
                                        if ui.selectable_value(&mut frame_size, size, size.as_ref()).clicked() {
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
                                let mut vad_strictness = vad_configs.strictness();
                                egui::ComboBox::from_id_salt("vad_strictness_combobox")
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
                });
            });
        });

        // MODALS -> this doesn't need to be in the scroll area.
        if self.recording_modal {
            let modal_id = egui::Id::new("transcriber_recordings_modal");
            let modal = egui::Modal::new(modal_id).show(ui.ctx(), |ui| {

                // Try-get the latest list of recordings.
                // TODO: maaaaybe this should have a debouncer.
                controller.try_get_completed_recordings(&mut self.recordings_buffer);

                // TODO: abstract better constants/variables here -> should probably be a percentage of the
                // window size
                ui.set_width_range(70f32..=100f32);
                let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
                let header_width = ui.max_rect().width();
                let desired_size = egui::Vec2::new(header_width, header_height);


                ui.allocate_ui_with_layout(desired_size, egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true), |ui| {
                    ui.heading("Previous recordings:");
                    if ui.button("Clear recordings").clicked() {
                        // This guards against grandma clicks.
                        controller.clear_recording_cache();
                    }
                });

                // NOTE: if this is a sufficient size, cache it higher up in the ui function.
                // (Unless the sizing gets modified in configs).
                // Maybe spacing isn't really needed.
                let gap_space = ui.spacing().interact_size.y;
                ui.add_space(gap_space);

                // If it's possible to know the size in advance, use show-rows.
                egui::ScrollArea::both().show(ui, |ui| {
                    egui::Grid::new("transcriber recording_list_grid").num_columns(1).striped(true).show(ui, |ui| {
                        let len = self.recordings_buffer.len();
                        for (i, (file_name, recording)) in self.recordings_buffer.iter().enumerate() {
                            let heading_text = format!("Recording: {}", len - i);

                            // TODO: if this is expensive/not all that valuable, just do the duration.
                            // NOTE: atm this code is identical to the recording modal
                            // If this diverges, keep the code here.
                            // Otherwise, look at factoring into a common function.
                            let body_text = {
                                let secs = recording.total_duration().as_secs();
                                let seconds = secs % 60;
                                let minutes = (secs / 60) % 60;
                                let hours = (secs / 60) / 60;

                                // This is in bytes.
                                let file_size_estimate = recording.file_size_estimate();
                                let size_text = match unit_prefix::NumberPrefix::binary(file_size_estimate as f32) {
                                    unit_prefix::NumberPrefix::Standalone(number) => format!("{number:.0} B"),
                                    unit_prefix::NumberPrefix::Prefixed(prefix, number) => format!("{number:.2} {prefix}B"),
                                };

                                format!("Total time: {hours}:{minutes}:{seconds} | Approx size: {size_text}")
                            };

                            // NOTE: this might actually panic if called from more than one spot
                            // Look into factoring out this modal.
                            let tile_id = egui::Id::new(heading_text.as_str());
                            let resp = ui.interact(ui.max_rect(), tile_id, egui::Sense::click());
                            let visuals = ui.style().interact(&resp);

                            // TODO: TEST THIS OUT AND MAKE SURE THINGS WORK OUT
                            // THE GOAL: highlight color + OUTLINE
                            // NOTE: atm this code is identical to the recording modal
                            // If this diverges, keep the code here.
                            // Otherwise, look at factoring into a common function.
                            egui::Frame::default().fill(visuals.bg_fill).stroke(visuals.fg_stroke).show(ui, |ui| {
                                ui.vertical(|ui| {
                                    ui.label(heading_text);
                                    ui.small(body_text);
                                });
                            });

                            if resp.clicked() {
                                // Try to load the recording - an unsuccessful recording will just
                                // get the updated list.

                                // NOTE: controller.try_get_recording_path() will internally
                                // prune out nonexistent paths -> it's possibly not necessary to set
                                // up a debouncer just yet.
                                //
                                // If the file doesn't exist, this will return None
                                if let Some(path) = controller.try_get_recording_path(Arc::clone(file_name)) {
                                    // Close the modal
                                    // Since this ui cursor doesn't have .close(), just set the ref
                                    self.recording_modal = false;
                                    // Set the audio
                                    controller.set_audio_file_path(path);
                                    // Swap to offline-mode for re-transcription

                                    self.realtime = false;
                                } else {
                                    // The writer engine will prune out its nonexistent file-paths,
                                    // so perhaps maybe a "toast" is sufficient here to say "sorry
                                    // cannot find recording".
                                    //
                                    // Otherwise, a debouncer will be necessary to maintain the state
                                    // of the directory.

                                    log::warn!("Temporary recording file missing: {file_name}");
                                    let toast = egui_notify::Toast::warning("Failed to find saved recording.");
                                    controller.send_toast(toast);
                                }
                            }
                            ui.end_row();
                        }
                    });
                });
            });

            // If a user clicks outside the modal, this will close it.
            if modal.should_close() {
                self.recording_modal = false;
            }
        }

        if self.download_modal {
            let modal = egui::Modal::new(egui::Id::new("download_models_modal"))
                .show(ui.ctx(), |ui| {
                    // TODO: like above: abstract better constants/variables here -> should probably be a percentage of the
                    // window size
                    ui.set_width_range(70f32..=100f32);

                    ui.heading("Download Models:");

                    // NOTE: this might not be necessary; remove it if it looks weird.
                    let gap_space = ui.spacing().interact_size.y;
                    ui.add_space(gap_space);

                    egui::ScrollArea::both().show(ui, |ui| {
                        egui::Grid::new("download_models_grid")
                            .num_columns(2)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.label("Url:");
                                ui.horizontal(|ui| {
                                    let empty = self.model_url.is_empty();
                                    ui.add(
                                        egui::TextEdit::singleline(&mut self.model_url)
                                            .hint_text("Download url"),
                                    );

                                    // Download runner button
                                    if ui
                                        .add_enabled(!empty, egui::Button::new("Download"))
                                        .clicked()
                                    {
                                        // TODO: possibly validate (try-parse) the url.
                                        self.download_modal = false;
                                        self.model_url.clear();
                                        controller.download_model(&self.model_url);
                                    }
                                    if ui
                                        .button(LINK_ICON)
                                        .on_hover_ui(|ui| {
                                            ui.style_mut().interaction.selectable_labels = true;
                                            ui.label("Launch the browser to open a model repository.");
                                        })
                                        .clicked()
                                    {
                                        self.download_modal = false;
                                        self.model_url.clear();
                                        // TODO: Change this to open a MODELS.md or similar containing
                                        // explanations + links for stuff.
                                        let _ = opener::open_browser(
                                            "https://huggingface.co/ggerganov/whisper.cpp/tree/main",
                                        );
                                    }
                                });

                                ui.end_row();
                            });

                        // Collapsible default-models.
                        // NOTE: These will just pull from the huggingface ggml repository.
                        // Consider looking into mirroring/stable storage.
                        ui.collapsing("Default models:", |ui| {
                            egui::Grid::new("default_models_grid")
                                .num_columns(2)
                                .striped(true)
                                .show(ui, |ui| {
                                    for model_type in DefaultModelType::iter() {
                                        ui.label(model_type.as_ref());
                                        if ui.button("Download").clicked() {
                                            self.download_modal = false;
                                            let url = model_type.url();
                                            controller.download_model(&url);
                                        }
                                        ui.end_row();
                                    }
                                });
                            // Tooltip for default moddels
                        })
                            .header_response
                            .on_hover_ui(|ui| {
                                ui.style_mut().interaction.selectable_labels = true;
                                ui.label("A selection of downloadable models sourced from huggingface.");
                            });
                    });
                });

            if modal.should_close() {
                self.download_modal = false;
                self.model_url.clear();
            }
        }

        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

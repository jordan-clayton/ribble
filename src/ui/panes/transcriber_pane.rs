use crate::controller::ribble_controller::RibbleController;
use crate::controller::{CompletedRecordingJobs, ModelFile, OfflineTranscriberFeedback};
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::PaneView;
use crate::ui::widgets::recording_modal::build_recording_modal;
use crate::ui::widgets::toggle_switch::toggle;
use crate::ui::{
    DEFAULT_TOAST_DURATION, GRID_ROW_SPACING_COEFF, MODAL_HEIGHT_PROPORTION, PANE_INNER_MARGIN,
};
use crate::utils::audio_gain::MAX_AUDIO_GAIN_DB;
use crate::utils::buffering_strategy::RibbleBufferingStrategy;
use crate::utils::realtime_settings::{AudioSampleLen, RealtimeTimeout, VadSampleLen};
use crate::utils::vad_configs::{VadFrameSize, VadStrictness, VadType};
use egui::Ui;
use ribble_whisper::whisper::configs::{Language, RealtimeBufferingStrategy};
use ribble_whisper::whisper::model::{DefaultModelType, ModelId};
use std::error::Error;
use std::sync::Arc;
use strum::IntoEnumIterator;

// Icon button for opening a link to huggingface/a models explainer
// NOTE: Not 100% committed to setting up a MODELS.md or similar -> it might be sufficent to just
// include information in the README.
const LINK_ICON: &str = "🌐";
const LINK_BUTTON_SIZE: f32 = 18.0;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(in crate::ui) struct TranscriberPane {
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
        ui: &mut Ui,
        _should_close: &mut bool,
        controller: RibbleController,
    ) -> egui::Response {
        let transcription_running = controller.transcriber_running();
        let audio_worker_running = controller.recorder_running() || transcription_running;

        let configs = *controller.read_transcription_configs();
        let vad_configs = *controller.read_vad_configs();

        // The query to get the model list is lazy (happens closer to the UI paint),
        // but this can cause issues if there is a model set.
        let model_id = *configs.model_id();
        // Check for both conditions to try and fill the list once

        // If the model has not yet been set (or doesn't exist anymore),
        // then the check for current_model will fail as expected,
        // the user should make a new model selection.
        if self.model_list.is_empty() && model_id.is_some() {
            controller.try_read_model_list(&mut self.model_list);
        }

        let current_model = (*configs.model_id())
            .and_then(|id| self.model_list.iter().find(|(k, _)| *k == id))
            .cloned();

        // RUN TRANSCRIPTION
        let can_run_transcription = current_model.is_some() && !audio_worker_running;

        // HEADING
        let header_text = if self.realtime {
            "Real-time Transcription"
        } else {
            "File Transcription"
        };

        let pane_id = egui::Id::new("transcriber_pane");
        // Return the interaction response.
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        let pane_col = ui.visuals().panel_fill;
        let latest_recording_exists = controller.latest_recording_exists();

        // MAIN PANEL FRAME
        egui::Frame::default().fill(pane_col).inner_margin(PANE_INNER_MARGIN).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.columns_const(|[col1, col2]| {
                    col1.vertical_centered_justified(|ui| {
                        let layout = egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true);
                        let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
                        let header_width = ui.max_rect().width();
                        let desired_size = egui::Vec2::new(header_width, header_height);
                        ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                            ui.heading(header_text);
                        });
                    });
                    col2.vertical_centered_justified(|ui| {
                        let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
                        let header_width = ui.max_rect().width();
                        let desired_size = egui::Vec2::new(header_width, header_height);
                        let layout = egui::Layout::right_to_left(egui::Align::Center).with_main_wrap(true);

                        let tooltip = if self.realtime { "Switch to file transcription." } else { "Switch to real-time transcription." };

                        ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                            ui.add_enabled(!transcription_running, toggle(&mut self.realtime))
                                .on_hover_cursor(egui::CursorIcon::Default)
                                .on_hover_text(tooltip);
                            ui.label("Realtime mode: ");
                        })
                    }).response.on_hover_cursor(egui::CursorIcon::Default);
                });
            });


            let button_spacing = ui.spacing().button_padding.y;

            // Since the top UI works better with the sticky, these can no longer can be scoped to
            // the upper offline-transcription branch split.
            //
            // Get the audio file information.
            let current_audio_path = controller.read_current_audio_file_path();

            let current_file = current_audio_path.as_deref().and_then(|path| path.file_name());

            // RUNNER FUNCTIONS:
            // REALTIME STICKY RUNNER BUTTONS
            // to true.
            if self.realtime {
                // RUNNER BUTTONS: START + STOP + Re-Transcribe
                ui.vertical_centered_justified(|ui| {
                    if ui
                        .add_enabled(can_run_transcription, egui::Button::new("Start"))
                        .on_hover_cursor(egui::CursorIcon::Default)
                        .clicked()
                    {
                        controller.start_realtime_transcription();
                    }
                    ui.add_space(button_spacing);
                    let stop_hover_text = "Immediately stop real-time transcription.";
                    if ui
                        .add_enabled(transcription_running, egui::Button::new("Stop"))
                        .on_hover_cursor(egui::CursorIcon::Default)
                        .on_hover_text(stop_hover_text)
                        .on_disabled_hover_text(stop_hover_text)
                        .clicked()
                    {
                        controller.stop_transcription();
                    }

                    let slow_stop_hover_text = "Stop streaming audio and transcribe any remaining samples.\n\
                                May result in better accuracy on older/lower-end hardware.";

                    if ui
                        .add_enabled(transcription_running && !controller.slow_stopping(),
                                     egui::Button::new("Slow Stop"))
                        .on_hover_cursor(egui::CursorIcon::Default)
                        .on_hover_text(slow_stop_hover_text)
                        .on_disabled_hover_text(slow_stop_hover_text)
                        .clicked() {
                        controller.slow_stop();
                    }
                });
            } else {
                // OFFLINE STICKY RUNNER BUTTONS

                // RUNER BUTTONS: START + STOP
                ui.vertical_centered_justified(|ui| {
                    let has_file = current_file.is_some();

                    if ui.add_enabled(can_run_transcription && has_file, egui::Button::new("Start"))
                        .on_hover_cursor(egui::CursorIcon::Default)
                        .clicked() {
                        controller.start_offline_transcription();
                    }

                    let stop_hover_text = "Immediately stop file transcription.";

                    if ui
                        .add_enabled(transcription_running, egui::Button::new("Stop"))
                        .on_hover_text(stop_hover_text)
                        .on_disabled_hover_text(stop_hover_text)
                        .on_hover_cursor(egui::CursorIcon::Default)
                        .clicked()
                    {
                        controller.stop_transcription();
                    }
                });
            }

            ui.add_space(button_spacing);
            ui.separator();

            egui::ScrollArea::both()
                .auto_shrink([false; 2]).show(ui, |ui| {
                // REALTIME NON STICKY RUNNER BUTTONS
                if self.realtime {
                    ui.heading("Cleanup:");

                    ui.vertical_centered_justified(|ui| {
                        if ui
                            .add_enabled(
                                latest_recording_exists && can_run_transcription,
                                egui::Button::new("Re-transcribe Last Recording"),
                            )
                            .on_hover_cursor(egui::CursorIcon::Default)
                            .on_hover_text("Offline transcribe latest cached recording.\n\
                    Generally more accurate due to full audio context.")
                            .clicked()
                        {
                            // NOTE: this might be a little too TOCTOU prone.
                            self.realtime = false;
                            controller.try_retranscribe_latest();
                        }
                    });
                } else {
                    // OFFLINE NON_STICKY RUNNER BUTTONS
                    // AUDIO FILE: LOAD FILE, LOAD RECORDING, CLEAR
                    ui.heading("Audio File:");
                    let audio_file_label_text = match current_file {
                        None => "None".to_string(),
                        Some(file) => {
                            file.to_string_lossy().to_string()
                        }
                    };
                    ui.vertical_centered_justified(|ui| {
                        // AUDIO FILE
                        egui::Grid::new("audio_file")
                            .num_columns(3)
                            .striped(true)
                            .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                            .show(ui, |ui| {
                                ui.label("Current audio file:");
                                // This will wrap--that's probably fine.
                                ui.label(audio_file_label_text);
                                // THIS COULD BE AN "X" instead of clear.
                                let desired_size = egui::Vec2::new(ui.available_width(), ui.spacing().interact_size.y);
                                let layout = egui::Layout::right_to_left(egui::Align::Center);
                                ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                    if ui.button("Clear")
                                        .on_hover_cursor(egui::CursorIcon::Default)
                                        .clicked() {
                                        controller.clear_audio_file_path();
                                    }
                                });
                            });

                        ui.add_space(button_spacing);
                        if ui.add_enabled(!transcription_running, egui::Button::new("Open file"))
                            .on_hover_cursor(egui::CursorIcon::Default)
                            .clicked() {
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
                        if ui.add_enabled(!transcription_running && latest_recording_exists, egui::Button::new("Load recording"))
                            .on_hover_cursor(egui::CursorIcon::Default)
                            .clicked() {
                            self.recording_modal = true;
                        }
                    });
                    ui.add_space(button_spacing);
                    ui.separator();

                    ui.heading("Feedback Mode");
                    // FEEDBACK MODE -> possibly hide this, but it seems important to have accessible.
                    egui::Grid::new("offline_feedback")
                        .num_columns(3)
                        .striped(true)
                        .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                        .show(ui, |ui| {
                            let mut offline_feedback = controller.read_offline_transcriber_feedback();
                            ui.label("Feedback mode:").on_hover_text("Set the feedback mode for file transcription.");

                            // This is a "null" column to try and get the combobox spacing a little
                            // more "nice".
                            let size = ui.spacing().interact_size;

                            ui.allocate_space(size);

                            ui.add_enabled_ui(!transcription_running, |ui| {
                                egui::ComboBox::from_id_salt("feedback_mode_combobox")
                                    .selected_text(offline_feedback.as_ref()).show_ui(ui, |ui| {
                                    for feedback_mode in OfflineTranscriberFeedback::iter() {
                                        if ui.selectable_value(&mut offline_feedback, feedback_mode, feedback_mode.as_ref())
                                            .on_hover_text(feedback_mode.tooltip()).clicked() {
                                            controller.write_offline_transcriber_feedback(offline_feedback);
                                        }
                                    }
                                }).response.on_hover_cursor(egui::CursorIcon::Default);
                            });
                            ui.end_row();
                        });
                }
                ui.add_space(button_spacing);
                ui.separator();


                // CONFIGS GRIDS
                ui.heading("Configs:");
                // Disable the configs interaction if the main runner is running
                let transcription_configs_dropdown = ui.collapsing("Transcription Configs", |ui| {
                    ui.add_enabled_ui(!transcription_running, |ui| {
                        egui::Grid::new("transcription_configs_grid")
                            .num_columns(2)
                            .striped(true)
                            .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                            .show(ui, |ui| {
                                // ROW: MODEL
                                ui.label("Model:");
                                // NOTE: do not use the horizontal layout hack to get the grid to paint fully here
                                // Otherwise it will also affect the scrolling.
                                ui.horizontal_wrapped(|ui| {
                                    controller.try_read_model_list(&mut self.model_list);
                                    // Get a clone of the model_id to modify
                                    let mut model_id = *configs.model_id();

                                    let model_id_combobox = match current_model {
                                        Some((_, file)) => {
                                            let salt = "model_id_combobox";
                                            match file {
                                                #[cfg(any(
                                                    debug_assertions,
                                                    feature = "pack-in-models"
                                                ))]
                                                ModelFile::Packed(idx) => {
                                                    egui::ComboBox::from_id_salt(salt)
                                                        .selected_text(ModelFile::PACKED_NAMES[idx])
                                                }
                                                ModelFile::File(name) => {
                                                    egui::ComboBox::from_id_salt(salt)
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
                                                #[cfg(any(
                                                    debug_assertions,
                                                    feature = "pack-in-models"
                                                ))]
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
                                    }).response
                                        .on_hover_cursor(egui::CursorIcon::Default);
                                });
                                ui.end_row();

                                ui.label("Load Model:").on_hover_text("Copy a downloaded model into Ribble.");
                                // NOTE: this does not need to be in a horizontal; this is just a hack to get
                                // the grid lines to paint to the edge of the pane.

                                ui.horizontal(|ui| {
                                    if ui.button("Load")
                                        .on_hover_cursor(egui::CursorIcon::Default)
                                        .clicked() {
                                        let file_dialog = rfd::FileDialog::new()
                                            .add_filter("ggml-model", &["bin"])
                                            .set_directory(controller.base_dir());

                                        // If there is path, it is a ".bin".
                                        // At the moment, there's no integrity checking
                                        // mechanisms
                                        if let Some(path) = file_dialog.pick_file() {
                                            // Try and set the Model ID if it's valid - the hash is expected to be stable.

                                            // Since this is happening over a background thread,
                                            // there isn't yet a great way to "await" this or get the file_name
                                            // if the result is successful.

                                            // Instead, expect it -will- be a successful operation,
                                            // Get an identical hash - if it's valid, the id will align
                                            // and be confirmed in the next repaint.

                                            // NOTE: implement a broadcast system.
                                            if let Some(file_name) = path.as_path().file_name() {
                                                let key = controller.get_model_key(&file_name.to_string_lossy());
                                                controller.write_transcription_configs(configs.with_model_id(Some(key)))
                                            }
                                            controller.copy_new_model(path);
                                        }
                                    }

                                    // GRID HACK.
                                    ui.add_space(ui.available_width());
                                });
                                ui.end_row();

                                ui.label("Download Model:").on_hover_text("Open the the downloads menu.");
                                if ui.button("Open menu").clicked() {
                                    self.download_modal = true;
                                }

                                ui.end_row();

                                // ROW: OPEN MODEL FOLDER
                                ui.label("Models Folder");
                                if ui.button("Open")
                                    .on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
                                    let model_directory = controller.get_model_directory();
                                    // Try and open it in the default file explorer.
                                    // There's a debouncer in the model-bank that will
                                    // keep the list mostly up to date.
                                    if let Err(e) = opener::open(model_directory) {
                                        log::warn!("Failed to open model directory. Error: {}\n\
                                        Error source: {:#?}", &e, e.source());
                                        let mut toast = egui_notify::Toast::error("Failed to open models directory");
                                        toast.duration(Some(DEFAULT_TOAST_DURATION));
                                        controller.send_toast(toast);
                                    }
                                }
                                ui.end_row();

                                // ROW: NUM THREADS
                                let mut n_threads = configs.n_threads();
                                let thread_range = 1..=controller.max_whisper_threads();
                                ui.label("No. threads:").on_hover_text("Set the number of threads to allocate to whisper. Recommended: 7.");

                                let slider = ui.add(egui::Slider::new(&mut n_threads, thread_range).integer());
                                // TODO: factor this out into a function: it's highly duplicated and error-prone code
                                let keyboard_input = check_keyboard(ui);

                                if slider.drag_stopped() || (slider.changed() && keyboard_input) {
                                    let new_configs = configs.with_n_threads(n_threads);
                                    controller.write_transcription_configs(new_configs)
                                }

                                ui.end_row();

                                // NOTE: if it becomes imperative to expose past prompt tokens,
                                // do so around here, but it shouldn't be relevant.
                                // ROW: SET TRANSLATE
                                ui.label("Translate (En):").on_hover_text("Translate the transcription (English only).");
                                let mut translate = configs.translate();
                                if ui.add(egui::Checkbox::without_text(&mut translate))
                                    .on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
                                    let new_configs = configs.with_translate(translate);
                                    controller.write_transcription_configs(new_configs)
                                }
                                ui.end_row();

                                // ROW: LANGUAGE
                                ui.label("Language:").on_hover_text("Set the input audio language.\n\
                                            Set to Auto for automatic language-detection.");

                                // NOTE TO SELF: implement Language::default() in Ribble-Whisper;
                                // It's fine for now: Default = None = Auto anyway.
                                let mut language = configs.language().unwrap_or(Language::Auto);

                                // NOTE: The other codes are all lowercase, but "auto" doesn't fit
                                // well with the rest of the UI.

                                let lang_selected_text = match language {
                                    Language::Auto => "Auto",
                                    _ => language.as_ref()
                                };
                                egui::ComboBox::from_id_salt("select_language_combobox")
                                    .selected_text(lang_selected_text).show_ui(ui, |ui| {
                                    for lang in Language::iter() {
                                        if ui.selectable_value(&mut language, lang, lang.as_ref()).clicked() {
                                            let new_configs = configs.with_language(Some(language));
                                            controller.write_transcription_configs(new_configs);
                                        }
                                    }
                                }).response
                                    .on_hover_cursor(egui::CursorIcon::Default);
                                ui.end_row();

                                // ROW: SET GPU
                                ui.label("Hardware Acceleration:").on_hover_text("Toggles transcription hardware acceleration via the GPU.\n\
                                            Real-time transcription may not be feasible without hardware acceleration."
                                );
                                let mut using_gpu = configs.using_gpu();
                                if ui.add(egui::Checkbox::without_text(&mut using_gpu))
                                    .on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
                                    let new_configs = configs.with_use_gpu(using_gpu);
                                    controller.write_transcription_configs(new_configs);
                                }
                                ui.end_row();

                                // ROW: SET FLASH ATTENTION
                                ui.label("Use Flash Attention:").on_hover_text("Toggles Flash Attention (if supported).\n\
                                            Significantly increases performance.");

                                let mut using_flash_attention = configs.using_flash_attention();
                                if ui.add(egui::Checkbox::without_text(&mut using_flash_attention))
                                    .on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
                                    let new_configs = configs.with_use_flash_attention(using_flash_attention);
                                    controller.write_transcription_configs(new_configs);
                                }
                                ui.end_row();

                                // ROW: USE CONTEXT - (is no-context in ribble-whisper)
                                // (no-context = true => use context = false)
                                ui.label("Use context:").on_hover_text("Retain context between decode passes.\n\
                                    Tends to improve accuracy but may introduce hallucinations.\n\
                                    This feature is ignored in real-time.");
                                let mut use_context = !configs.using_no_context();
                                if ui.add(egui::Checkbox::without_text(&mut use_context))
                                    .on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
                                    let new_configs = configs.with_use_no_context(!use_context);
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
                                        let test_timeout: usize = realtime_timeout.into();
                                        assert_eq!(test_timeout, configs.realtime_timeout());
                                    }
                                    ui.label("Timeout:").on_hover_text("Set the timeout for real-time transcription.\n\
                                                Set to infinite for continuous sessions, but note that performance may degrade.");


                                    egui::ComboBox::from_id_salt("realtime_timeout_combobox")
                                        .selected_text(realtime_timeout.as_ref())
                                        .show_ui(ui, |ui| {
                                            for timeout_len in RealtimeTimeout::iter() {
                                                if ui.selectable_value(&mut realtime_timeout, timeout_len, timeout_len.as_ref())
                                                    .clicked() {
                                                    let new_timeout: usize = realtime_timeout.into();
                                                    let new_configs = configs.with_realtime_timeout(new_timeout);
                                                    controller.write_transcription_configs(new_configs);
                                                }
                                            }
                                        }).response.on_hover_cursor(egui::CursorIcon::Default);
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

                                    ui.label("Audio Sample size:").on_hover_text("Sets the audio sampling buffer size.\n\
                                                Smaller sizes: lower latency, lower accuracy, higher power draw.\n\
                                                Larger sizes: higher latency, higher accuracy, lower power draw.");

                                    egui::ComboBox::from_id_salt("audio_sample_len")
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
                                        }).response.on_hover_cursor(egui::CursorIcon::Default);
                                    ui.end_row();

                                    // ROW: BUFFERING STRATEGY
                                    let mut buffering_strategy: RibbleBufferingStrategy = configs.realtime_buffering_strategy().into();
                                    ui.label("Buffering Strategy:").on_hover_text("Sets the buffering strategy for real-time transcription.\n\
                                                                        May improve accuracy, reduces performance costs, but increases latency.\n\
                                                                        Buffering is recommended for older/lower-end hardware.");
                                    egui::ComboBox::from_id_salt("realtime_buffering_strategy")
                                        .selected_text(buffering_strategy.as_ref())
                                        .show_ui(ui, |ui| {
                                            for strategy in RibbleBufferingStrategy::iter() {
                                                if ui.selectable_value(&mut buffering_strategy, strategy, strategy.as_ref())
                                                    .on_hover_text(strategy.tooltip())
                                                    .clicked() {
                                                    let new_strategy: RealtimeBufferingStrategy = buffering_strategy.into();
                                                    let new_configs = configs.with_buffering_strategy(new_strategy);
                                                    controller.write_transcription_configs(new_configs);
                                                }
                                            }
                                        }).response.on_hover_cursor(egui::CursorIcon::Default);

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

                                    ui.label("VAD Sample size:").on_hover_text("Sets the voice-activity sampling buffer size.\n\
                                                Smaller sizes: lower latency, lower accuracy, higher power draw.\n\
                                                Larger sizes: higher latency, higher accuracy, lower power draw.");

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
                                        }).response.on_hover_cursor(egui::CursorIcon::Default);

                                    ui.end_row();
                                }

                                // ROW: RESET TO DEFAULTS.
                                ui.label("Reset to defaults:");
                                if ui.button("Reset")
                                    .on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
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
                    });
                });
                transcription_configs_dropdown
                    .header_response
                    .on_hover_cursor(egui::CursorIcon::Default);


                ui.add_space(button_spacing);
                ui.separator();

                let vd_configs = ui.collapsing("Voice Activity Detector Configs", |ui| {
                    ui.add_enabled_ui(!transcription_running, |ui| {
                        egui::Grid::new("vad_configs_grid").striped(true)
                            .num_columns(2)
                            .striped(true)
                            .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                            .show(ui, |ui| {
                                // VAD TYPE
                                ui.label("VAD algorithm:").on_hover_text("Select which voice detection algorithm to use.\n\
                                Set to Auto for system defaults.");

                                let mut vad_type = vad_configs.vad_type();
                                ui.horizontal(|ui| {
                                    egui::ComboBox::from_id_salt("vad_type_combobox")
                                        .selected_text(vad_type.as_ref()).show_ui(ui, |ui| {
                                        for vad in VadType::iter() {
                                            if ui.selectable_value(&mut vad_type, vad, vad.as_ref())
                                                .on_hover_text(vad.tooltip())
                                                .clicked() {
                                                let new_vad_configs = vad_configs.with_vad_type(vad_type);
                                                controller.write_vad_configs(new_vad_configs);
                                            }
                                        }
                                    }).response.on_hover_cursor(egui::CursorIcon::Default);

                                    // This is just an empty to paint the grid color to the edge of the screen.
                                    ui.add_space(ui.available_width());
                                });

                                ui.end_row();

                                // FRAME SIZE
                                // Silero v5 requires fixed buffer size based on sample rate, so this has to conditionally render.
                                // NOTE: this will need to be maintained if swapping the
                                // VadType::Auto to WebRtc
                                // --this is not a great solution right now wrt maintainability, but it will do.
                                if !matches!(vad_configs.vad_type(), VadType::Silero | VadType::Auto) {
                                    ui.label("Frame size:").on_hover_text("Sets the length of the audio frame used to detect voice.\n\
                                    Larger sizes may introduce latency but provide better results.\n\
                                    Set to Auto for system defaults.");

                                    let mut frame_size = vad_configs.frame_size();
                                    egui::ComboBox::from_id_salt("vad_frame_size_combobox")
                                        .selected_text(frame_size.as_ref()).show_ui(ui, |ui| {
                                        for size in VadFrameSize::iter() {
                                            if ui.selectable_value(&mut frame_size, size, size.as_ref()).clicked() {
                                                let new_vad_configs = vad_configs.with_frame_size(frame_size);
                                                controller.write_vad_configs(new_vad_configs);
                                            }
                                        }
                                    }).response.on_hover_cursor(egui::CursorIcon::Default);
                                    ui.end_row();
                                }

                                // STRICTNESS
                                ui.label("Strictness:").on_hover_text("Sets the voice-detection thresholds.\n\
                                    Higher strictness can improve performance, but may increase false negatives.\n\
                                    Set to Auto for system defaults.");
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
                                    }).response.on_hover_cursor(egui::CursorIcon::Default);
                                ui.end_row();

                                // USE OFFLINE
                                let mut vad_use_offline = vad_configs.use_vad_offline();
                                ui.label("File VAD:").on_hover_text("Run VAD for file transcription.\n\
                                    Significantly improves performance but may cause transcription artifacts.");
                                if ui.add(egui::Checkbox::without_text(&mut vad_use_offline)).on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
                                    let new_vad_configs = vad_configs.with_use_vad_offline(vad_use_offline);
                                    controller.write_vad_configs(new_vad_configs);
                                }
                                ui.end_row();
                                ui.label("Reset settings:");
                                if ui.button("Reset")
                                    .on_hover_cursor(egui::CursorIcon::Default)
                                    .clicked() {
                                    controller.write_vad_configs(Default::default());
                                }
                                ui.end_row();
                            })
                    });
                });
                vd_configs.header_response.on_hover_cursor(egui::CursorIcon::Default);
                ui.separator();
                // AUDIO GAIN SETTINGS
                let audio_gain_configs = ui.collapsing("Audio gain", |ui| {
                    let audio_gain_configs = *controller.read_audio_gain_configs();

                    ui.add_enabled_ui(!transcription_running, |ui| {
                        egui::Grid::new("audio_gain_configs_grid").striped(true).num_columns(2)
                            .show(ui, |ui| {
                                let mut db = audio_gain_configs.db();
                                let db_range = 0.0..=MAX_AUDIO_GAIN_DB;
                                ui.label("Audio gain:").on_hover_text("Apply audio gain to boost recording volume.\n\
                                Recommended: ~6dB all-purpose, 20dB if using Silero VAD.");
                                ui.horizontal(|ui| {
                                    let slider = ui.add(egui::Slider::new(&mut db, db_range));
                                    let keyboard_input = check_keyboard(ui);
                                    if slider.drag_stopped() || (slider.changed() && keyboard_input) {
                                        let new_configs = audio_gain_configs.with_decibels(db);
                                        controller.write_audio_gain_configs(new_configs);
                                    }
                                    // Tiny hack to paint the grid color to the edge of the pane.
                                    ui.add_space(ui.available_width());
                                });
                                ui.end_row();

                                ui.label("File gain:").on_hover_text("Apply gain to files before transcribing?\n\
                                Audio will be normalized to the highest peak.");

                                let mut use_offline = audio_gain_configs.use_offline();
                                if ui.add(egui::Checkbox::without_text(&mut use_offline)).clicked() {
                                    let new_configs = audio_gain_configs.with_use_offline(use_offline);
                                    controller.write_audio_gain_configs(new_configs);
                                }
                                ui.end_row();
                            });
                    });
                });
                audio_gain_configs.header_response.on_hover_cursor(egui::CursorIcon::Default);
            });
        });

        // MODALS -> this doesn't need to be in the scroll area.
        if self.recording_modal {
            controller.try_read_recording_metadata(&mut self.recordings_buffer);
            // NOTE: this is a very cheap clone, so it should be fine to just cache and pass into the closure.
            let err_ctx = ui.ctx().clone();
            let handle_recordings = |file_name| match controller
                .try_get_recording_path(Arc::clone(&file_name))
            {
                Some(path) => {
                    controller.set_audio_file_path(path);
                    self.realtime = false;
                    self.recording_modal = false;
                }
                None => {
                    log::warn!("Temporary recording file missing: {file_name}");
                    let mut toast = egui_notify::Toast::warning("Failed to find saved recording.");
                    toast.duration(Some(DEFAULT_TOAST_DURATION));
                    controller.send_toast(toast);
                    err_ctx.request_repaint();
                }
            };

            let modal = build_recording_modal(
                ui,
                "transcriber_recording_modal",
                "transcriber_recording_grid",
                &controller,
                &self.recordings_buffer,
                handle_recordings,
            );

            // If a user clicks outside the modal, this will close it.
            if modal.should_close() {
                self.recording_modal = false;
            }
        }

        if self.download_modal {
            let modal = egui::Modal::new(egui::Id::new("download_models_modal"))
                .show(ui.ctx(), |ui| {
                    let height = ui.ctx().screen_rect().height() * MODAL_HEIGHT_PROPORTION;
                    ui.set_max_height(height);
                    egui::Frame::default().inner_margin(PANE_INNER_MARGIN).show(ui, |ui| {
                        ui.heading("Download Models:");

                        // NOTE: this might not be necessary; remove it if it looks weird.
                        let gap_space = ui.spacing().interact_size.y;
                        ui.add_space(gap_space);

                        egui::ScrollArea::vertical()
                            .show(ui, |ui| {
                                egui::Grid::new("download_models_grid")
                                    .num_columns(2)
                                    .striped(true)
                                    .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                                    .show(ui, |ui| {
                                        ui.label("Url:");
                                        ui.horizontal_centered(|ui| {
                                            // Set the interact size to be slightly larger (to match the button size)
                                            ui.spacing_mut().interact_size.y *= 1.25;

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
                                                self.download_modal = false;
                                                controller.download_model(&self.model_url);
                                                self.model_url.clear();
                                            }

                                            // The "link" icon is a little small ->
                                            let link_button = egui::RichText::new(LINK_ICON).size(LINK_BUTTON_SIZE);

                                            if ui
                                                .button(link_button)
                                                .on_hover_text("Launch the browser to open a model repository.")
                                                .clicked()
                                            {
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
                                        .min_row_height(ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF)
                                        .show(ui, |ui| {
                                            for model_type in DefaultModelType::iter() {
                                                ui.label(model_type.as_ref());
                                                ui.horizontal(|ui| {
                                                    if ui.button("Download").clicked() {
                                                        self.download_modal = false;
                                                        let url = model_type.url();
                                                        controller.download_model(&url);
                                                    }
                                                    // This is hacky, but it will extend the striping to the edge
                                                    // of the grid.
                                                    ui.add_space(ui.available_width());
                                                });
                                                ui.end_row();
                                            }
                                        });
                                    // Tooltip for default moddels
                                })
                                    .header_response
                                    .on_hover_text("A selection of downloadable models sourced from huggingface.");
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

fn check_keyboard(ui: &mut Ui) -> bool {
    ui.input(|i| {
        i.keys_down.iter().any(|key| {
            // NOTE: there is no "is_numeric() or similar in egui, afaik.
            // To avoid unnecessary/excess caching (and allow the slider to work as intended),
            // Check for a (numeric) key input on the slider, and write on a
            // change -> enter isn't strictly necessary and writes are atomic anyway.
            matches!(
                key,
                egui::Key::Num0
                    | egui::Key::Num1
                    | egui::Key::Num2
                    | egui::Key::Num3
                    | egui::Key::Num4
                    | egui::Key::Num5
                    | egui::Key::Num6
                    | egui::Key::Num7
                    | egui::Key::Num8
                    | egui::Key::Num9
            )
        })
    })
}

use std::path::PathBuf;

use catppuccin_egui::Theme;
use egui::{Button, CentralPanel, Grid, Layout, ScrollArea, SidePanel, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::utils::{file_mgmt, preferences};
use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::{
        tabs::tab_view,
        widgets::{recording_icon::recording_icon, toggle_switch::toggle},
    },
    utils::{
        console_message::{ConsoleMessage, ConsoleMessageType},
        constants,
    },
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionTab {
    title: String,
    #[serde(skip)]
    text_buffer: Vec<String>,
    realtime_mode: bool,

    // Default to false
    #[serde(skip)]
    processing_speech: bool,
    // Possibly just use a file path.
    #[serde(skip)]
    audio_file: Option<PathBuf>,
}

impl TranscriptionTab {
    fn new(text_buffer: Vec<String>, realtime_mode: bool, accepting_speech: bool) -> Self {
        Self {
            title: String::from("Transcription"),
            text_buffer,
            realtime_mode,
            processing_speech: accepting_speech,
            audio_file: None,
        }
    }

    // For transcription display window.
    fn header(
        ui: &mut Ui,
        theme: Theme,
        processing_msg: &str,
        processing: bool,
        running: bool,
        ready: bool,
    ) {
        let (icon, msg) = if processing {
            (
                recording_icon(egui::Rgba::from(theme.red), true),
                processing_msg,
            )
        } else if running {
            (
                recording_icon(egui::Rgba::from(theme.green), true),
                "Preparing to transcribe.",
            )
        } else if ready {
            (
                recording_icon(egui::Rgba::from(theme.green), false),
                "Ready to transcribe.",
            )
        } else {
            (
                recording_icon(egui::Rgba::from(theme.yellow), false),
                "Not ready.",
            )
        };

        ui.horizontal(|ui| {
            ui.add(icon);
            ui.label(msg);
        });
    }

    // Static toolbar
    fn static_panel(
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        audio_path: &mut Option<PathBuf>,
        transcription: &[String],
        static_running: bool,
    ) {
        let audio_running = controller.audio_running();
        // Check whether mic is occupied by another process.
        let mic_occupied = audio_running ^ static_running;

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

        let mut file_name = file_path
            .file_name()
            .expect("Invalid filename")
            .to_str()
            .expect("Invalid path unicode");
        if file_name == constants::TEMP_FILE {
            file_name = if has_file {
                "Current recording".into()
            } else {
                ""
            }
        }

        let can_start = !static_running && has_file;

        ui.add_enabled_ui(!mic_occupied, |ui| {
            Grid::new("inner_static_panel")
                .striped(true)
                .show(ui, |ui| {
                    if ui.add_enabled(can_start, Button::new("Start")).clicked() {
                        let ctx = ui.ctx().clone();
                        controller.start_static_transcription(&file_path, &ctx);
                    }

                    ui.end_row();

                    if ui
                        .add_enabled(static_running, Button::new("Stop"))
                        .clicked()
                    {
                        controller.stop_transcriber(false);
                    }

                    ui.end_row();

                    // Open file button - this will already be disabled if realtime/recording are running.
                    // TODO: determine what... uh, files.
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(!static_running, Button::new("Open"))
                            .on_hover_ui(|ui| {
                                ui.style_mut().interaction.selectable_labels = true;
                                ui.label("Open a compatible audio file to transcribe");
                            })
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
                                .add_filter("Wave", &["wav"])
                                .add_filter("mpeg", &["mpg, mp3, mp4, m4v"])
                                .set_directory(dir)
                                .pick_file()
                            {
                                *audio_path = Some(p);
                            }
                        }
                        ui.label(file_name);
                    });
                    ui.end_row();

                    Self::transcription_save_button(ui, controller, transcription, static_running);
                    ui.end_row();
                })
        });
    }

    // Realtime toolbar
    // i.e. Buttons for starting/stopping/saving/etc
    fn realtime_panel(
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        transcription: &[String],
        realtime_running: bool,
    ) {
        let audio_running = controller.audio_running();
        // Check whether mic is occupied by another process.
        let mic_occupied = audio_running ^ realtime_running;
        let ctx = ui.ctx().clone();

        ui.add_enabled_ui(!mic_occupied, |ui| {
            Grid::new("inner_realtime_panel")
                .striped(true)
                .show(ui, |ui| {
                    if ui
                        .add_enabled(!realtime_running, Button::new("Start"))
                        .clicked()
                    {
                        controller.start_realtime_transcription(&ctx);
                    }
                    ui.end_row();

                    if ui
                        .add_enabled(realtime_running, Button::new("Stop"))
                        .clicked()
                    {
                        controller.stop_transcriber(true);
                    }
                    ui.end_row();

                    let can_save = controller.save_recording_ready() & !realtime_running;
                    // Re-transcription
                    if ui
                        .add_enabled(can_save, Button::new("Re-Transcribe Recording"))
                        .clicked()
                    {
                        let data_dir = eframe::storage_dir(constants::APP_ID)
                            .expect("Failed to get storage dir");
                        let path = file_mgmt::get_temp_file_path(&data_dir);
                        assert!(path.exists(), "Temporary file missing");
                        controller.start_static_transcription(path.as_path(), &ctx);
                    }
                    ui.end_row();

                    Self::transcription_save_button(
                        ui,
                        controller,
                        transcription,
                        realtime_running,
                    );
                    ui.end_row();

                    // Save audio.
                    if ui
                        .add_enabled(can_save, Button::new("Save recording"))
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
                            .add_filter("wave", &["wav"])
                            .set_directory(dir)
                            .save_file()
                        {
                            controller.save_audio_recording(&p);
                        }
                    }
                    ui.end_row();
                });
        });
    }

    fn transcription_save_button(
        ui: &mut Ui,
        controller: &mut WhisperAppController,
        transcription: &[String],
        running: bool,
    ) {
        let can_save = !running & !transcription.is_empty();
        if ui
            .add_enabled(can_save, Button::new("Save transcription"))
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
                .add_filter("text", &["txt"])
                .set_directory(dir)
                .save_file()
            {
                controller.save_transcription(&p, transcription);
            }
        }
    }
}

impl Default for TranscriptionTab {
    fn default() -> Self {
        let text_buffer = vec![];
        let realtime_mode = true;
        let accepting_speech = false;
        Self::new(text_buffer, realtime_mode, accepting_speech)
    }
}

impl tab_view::TabView for TranscriptionTab {
    fn id(&mut self) -> String {
        self.title.clone()
    }
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        // Destructure mut borrow
        let Self {
            title: _,
            text_buffer,
            realtime_mode,
            processing_speech,
            audio_file,
        } = self;

        let realtime_running = controller.realtime_running();
        let realtime_ready = controller.realtime_ready();
        let static_running = controller.static_running();
        let static_ready = controller.static_ready();

        // UPDATE STATE
        // Handle new text.
        let new_text_message = controller.recv_transcription_text();
        if let Ok(message) = new_text_message {
            match message {
                Ok(t) => {
                    // Special case: [CLEAR TRANSCRIPTION]
                    let text = t.0;
                    if &text == constants::CLEAR_MSG {
                        text_buffer.clear();
                    }
                    // Special case: [START SPEAKING]
                    // Set the speech flag.
                    else if &text == constants::GO_MSG {
                        *processing_speech = true;
                    } else {
                        // Append to the text buffer.
                        if t.1 {
                            text_buffer.push(text);
                        } else {
                            let last = text_buffer.len() - 1;
                            text_buffer[last] = text;
                        }
                    }
                }
                Err(e) => {
                    let msg = ConsoleMessage::new(ConsoleMessageType::Error, e.to_string());
                    controller
                        .send_console_message(msg)
                        .expect("Error channel closed");
                }
            }
        }

        // Keep processing_speech state consistent with the state of the transcription worker.
        *processing_speech = if *realtime_mode {
            *processing_speech & realtime_running
        } else {
            *processing_speech & static_running
        };

        // UPDATE SCREEN
        // Button panel
        SidePanel::right("transcription_panel").show_inside(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Default::default()), |ui| {
                // Toggle button.
                ui.add(toggle(realtime_mode));
                // Label
                let label = if *realtime_mode { "Realtime" } else { "Static" };

                ui.label(label);
            });

            // Scrollable panel section.
            ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                if *realtime_mode {
                    Self::realtime_panel(ui, controller, text_buffer.as_slice(), realtime_running);
                } else {
                    Self::static_panel(
                        ui,
                        controller,
                        audio_file,
                        text_buffer.as_slice(),
                        static_running,
                    );
                }
            })
        });

        CentralPanel::default().show_inside(ui, |ui| {
            // Get the theme.
            let system_theme = controller.get_system_theme();
            let theme = preferences::get_app_theme(system_theme);
            let (processing_msg, running, ready) = if *realtime_mode {
                ("Speak now.", realtime_running, realtime_ready)
            } else {
                ("Transcription in progress.", static_running, static_ready)
            };

            Self::header(
                ui,
                theme,
                processing_msg,
                *processing_speech,
                running,
                ready,
            );

            // Transcription
            ScrollArea::vertical()
                .auto_shrink(false)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for chunk in text_buffer {
                        ui.monospace(chunk);
                    }
                })
        });
    }

    // Right-click tab -> context actions for text operations
    // TODO: Determine if necessary.
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

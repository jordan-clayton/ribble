use egui::{CentralPanel, Layout, ScrollArea, SidePanel, Ui, Widget, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::ui::tabs::tab_view;
use crate::ui::widgets::recording_icon::recording_icon;
use crate::ui::widgets::toggle_switch;
use crate::utils::{constants, preferences};
use crate::whisper_app_context::WhisperAppController;

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct TranscriptionTab {
    title: String,
    #[serde(skip)]
    text_buffer: Vec<String>,
    realtime_mode: bool,

    // Default to false
    #[serde(skip)]
    accepting_speech: bool,

    // Possibly just use a file path.
    // #[serde(skip)]
    // file_loaded: bool,
}


// TODO: function to save the buffer to file.
impl TranscriptionTab {
    fn new(text_buffer: Vec<String>, realtime_mode: bool, accepting_speech: bool) -> Self {
        Self { title: String::from("Transcription"), text_buffer, realtime_mode, accepting_speech }
    }

    // Static header
    fn static_header(ui: &mut Ui, controller: &WhisperAppController) {
        let system_theme = controller.get_system_theme();
        let theme = preferences::get_app_theme(system_theme);

        let mut icon = None;
        let mut msg = "";

        if controller.static_running() {
            icon = Some(recording_icon(egui::Rgba::from(theme.red), true));
            msg = "Transcribing in progress."
        } else if controller.static_ready() {
            icon = Some(recording_icon(egui::Rgba::from(theme.green), false));
            msg = "Ready to transcribe."
        } else {
            icon = Some(recording_icon(egui::Rgba::from(theme.yellow), false));
            msg = "Not ready."
        }

        let icon = icon.expect("Recording icon not set");

        ui.horizontal(|ui| {
            ui.add(icon);
            ui.label(msg);
        });
    }

    // Static toolbar
    fn static_toolbar(ui: &mut Ui, controller: &mut WhisperAppController) {
        todo!()
    }

    // Realtime header
    fn realtime_header(ui: &mut Ui, controller: &WhisperAppController, accepting_speech: bool) {
        let system_theme = controller.get_system_theme();
        let theme = preferences::get_app_theme(system_theme);

        let mut icon = None;
        let mut msg = "";

        if accepting_speech {
            icon = Some(recording_icon(egui::Rgba::from(theme.red), true));
            msg = "Speak now.";
        } else if controller.realtime_running() {
            icon = Some(recording_icon(egui::Rgba::from(theme.green), true));
            msg = "Preparing to transcribe."
        } else if controller.realtime_ready() {
            icon = Some(recording_icon(egui::Rgba::from(theme.green), false));
            msg = "Ready to transcribe."
        } else {
            icon = Some(recording_icon(egui::Rgba::from(theme.yellow), false));
            msg = "Not ready."
        };

        let icon = icon.expect("Recording icon not set");

        ui.horizontal(|ui| {
            ui.add(icon);
            ui.label(msg);
        });
    }

    // Realtime toolbar
    fn realtime_toolbar(ui: &mut Ui, controller: &mut WhisperAppController) {
        todo!()
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
    fn title(&mut self) -> WidgetText {
        WidgetText::from(&self.title)
    }

    // Main UI design.
    fn ui(&mut self, ui: &mut Ui, controller: &mut WhisperAppController) {
        // Destructure mut borrow
        let Self {
            title: _, text_buffer, realtime_mode, accepting_speech,
        } = self;

        // UPDATE STATE
        // Handle new text.
        let new_text_message = controller.receive_transcription_text();
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
                    if &text == "" {
                        *accepting_speech = true;
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
                    controller.send_error(e).expect("Error channel closed");
                }
            }
        }

        // UPDATE SCREEN
        // Button panel
        SidePanel::right("realtime_panel").show_inside(ui, |ui| {
            ui.with_layout(Layout::right_to_left(Default::default()), |ui| {
                // Toggle button.
                ui.add(toggle_switch::toggle(realtime_mode));
                // Label
                let label = if *realtime_mode {
                    "Realtime"
                } else {
                    "Static"
                };

                ui.label(label);
            });

            // Scrollable panel section.
            ScrollArea::vertical()
                .auto_shrink(false)
                .show(ui, |ui| {
                    if *realtime_mode {
                        Self::realtime_toolbar(ui, controller);
                    } else {
                        Self::static_toolbar(ui, controller);
                    }
                })
        });

        CentralPanel::default().show_inside(ui, |ui| {
            // Status
            if *realtime_mode {
                Self::realtime_header(ui, controller, *accepting_speech);
            } else {
                Self::static_header(ui, controller);
            }

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
    fn context_menu(&mut self, _ui: &mut Ui, controller: &mut WhisperAppController, _surface: SurfaceIndex, _node: NodeIndex) {
        todo!()
    }

    fn closeable(&mut self) -> bool {
        false
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}
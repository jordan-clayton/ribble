use catppuccin_egui::Theme;
use egui::{
    CentralPanel, Frame, ScrollArea, TopBottomPanel, Ui, WidgetText,
};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::{
        tabs::tab_view,
        widgets::recording_icon::recording_icon,
    },
    utils::{
        console_message::{ConsoleMessage, ConsoleMessageType},
        constants,
    },
};
use crate::utils::preferences;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionTab {
    title: String,
    #[serde(skip)]
    text_buffer: Vec<String>,
    // TODO: remove
    realtime_mode: bool,
    // Default to false
    // TODO: refactor this to something... better.
    #[serde(skip)]
    processing_speech: bool,
}

impl TranscriptionTab {
    fn new(text_buffer: Vec<String>, realtime_mode: bool, accepting_speech: bool) -> Self {
        Self {
            title: String::from("Transcription"),
            text_buffer,
            realtime_mode,
            processing_speech: accepting_speech,
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
        let time_scale = Some(constants::RECORDING_ANIMATION_TIMESCALE);
        let (icon, msg) = if processing {
            (
                recording_icon(egui::Rgba::from(theme.red), true, time_scale),
                processing_msg,
            )
        } else if running {
            (
                recording_icon(egui::Rgba::from(theme.green), true, time_scale),
                "Preparing to transcribe.",
            )
        } else if ready {
            (
                recording_icon(egui::Rgba::from(theme.green), false, time_scale),
                "Ready to transcribe.",
            )
        } else {
            (
                recording_icon(egui::Rgba::from(theme.yellow), false, time_scale),
                "Not ready.",
            )
        };

        ui.horizontal(|ui| {
            ui.add(icon);
            ui.label(msg);
        });
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
        } = self;

        let realtime_running = controller.realtime_running();
        let realtime_ready = controller.realtime_ready();
        let static_running = controller.static_running();
        let static_ready = controller.static_ready();

        // TODO: refactor to rwloc
        // UPDATE STATE
        // Handle new text.
        while let Ok(message) = controller.recv_transcription_text() {
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

        TopBottomPanel::top("header")
            .resizable(false)
            .show_inside(ui, |ui| {
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
                let space = ui.spacing().item_spacing.y;
                ui.add_space(space);
            });

        // Transcription
        let visuals = ui.visuals();
        let bg_col = visuals.extreme_bg_color;
        let transcription_frame = Frame::default().fill(bg_col);

        // TODO: look into Complex layouts if necessary
        CentralPanel::default()
            .frame(transcription_frame)
            .show_inside(ui, |ui| {
                ScrollArea::vertical()
                    .auto_shrink(false)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for segment in text_buffer {
                            ui.monospace(segment);
                        }
                    });
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
    ) {}

    fn closeable(&mut self) -> bool {
        true
    }

    fn allowed_in_windows(&mut self) -> bool {
        true
    }
}

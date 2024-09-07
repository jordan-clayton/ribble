use egui::{CentralPanel, Frame, ScrollArea, TopBottomPanel, Ui, WidgetText};
use egui_dock::{NodeIndex, SurfaceIndex};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{display_tabs::display_common::get_header_recording_icon, tab_view},
    utils::preferences,
};

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct TranscriptionTab {
    title: String,
    #[serde(skip)]
    text_buffer: Vec<String>,
}

impl TranscriptionTab {
    fn new(text_buffer: Vec<String>) -> Self {
        Self {
            title: String::from("Transcription"),
            text_buffer,
        }
    }
}

impl Default for TranscriptionTab {
    fn default() -> Self {
        let text_buffer = vec![];
        Self::new(text_buffer)
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
        } = self;

        let audio_worker_state = controller.audio_worker_state();

        // Update state.
        controller.read_transcription_buffer(text_buffer);

        TopBottomPanel::top("transcription_header")
            .resizable(false)
            .show_inside(ui, |ui| {
                let system_theme = controller.get_system_theme();
                let theme = preferences::get_app_theme(system_theme);
                let (icon, msg) = get_header_recording_icon(audio_worker_state, true, &theme);
                ui.horizontal(|ui| {
                    ui.add(icon);
                    ui.label(msg);
                });

                let space = ui.spacing().item_spacing.y;
                ui.add_space(space);
            });

        // Transcription
        let visuals = ui.visuals();
        let bg_col = visuals.extreme_bg_color;
        let transcription_frame = Frame::default().fill(bg_col);

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

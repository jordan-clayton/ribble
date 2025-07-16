use crate::controller::ribble_controller::RibbleController;
use crate::controller::{MAX_NUM_CONSOLE_MESSAGES, MIN_NUM_CONSOLE_MESSAGES};
use crate::ui::new_tabs::PaneView;
use crate::ui::new_tabs::ribble_pane::RibblePaneId;
use crate::utils::preferences::RibbleAppTheme;
use strum::IntoEnumIterator;

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct UserPreferencesPane {
    #[serde(skip)]
    #[serde(default)]
    num_console_messages: Option<usize>,
}

impl PaneView for UserPreferencesPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::UserPreferences
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Settings".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        let prefs = *controller.read_user_preferences();

        if self.num_console_messages.is_none() {
            self.num_console_messages = Some(prefs.console_message_size())
        }

        let mut console_message_size = self
            .num_console_messages
            .expect("Console messages can only be None at construction time");

        let mut theme = prefs.system_theme();

        egui::Frame::new().show(ui, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                egui::Grid::new("prefs_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        // SET SYSTEM THEME
                        ui.label("System theme:");
                        egui::ComboBox::from_id_salt("user_theme")
                            .selected_text(theme.as_ref())
                            .show_ui(ui, |ui| {
                                for ribble_theme in RibbleAppTheme::iter() {
                                    if ui
                                        .selectable_value(
                                            &mut theme,
                                            ribble_theme,
                                            ribble_theme.as_ref(),
                                        )
                                        .clicked()
                                    {
                                        let new_prefs = prefs.with_system_theme(theme);
                                        controller.write_user_preferences(new_prefs);
                                    }
                                }
                            });

                        ui.end_row();

                        // SET CONSOLE HISTORY
                        // Writes on drag-finished.
                        ui.label("Console history:").on_hover_ui(|ui| {
                            ui.style_mut().interaction.selectable_labels = true;
                            ui.label("Set the number of console messages to retain.");
                        });

                        if ui
                            .add(egui::Slider::new(
                                &mut console_message_size,
                                MIN_NUM_CONSOLE_MESSAGES..=MAX_NUM_CONSOLE_MESSAGES,
                            ))
                            .drag_stopped()
                        {
                            // Write the new number of console messages -> the console buffer is
                            // handled internally.
                            let new_prefs = prefs.with_console_message_size(console_message_size);
                            controller.write_user_preferences(new_prefs);
                        }

                        ui.end_row();
                    });
            });
        });

        let pane_id = egui::Id::new("user_prefs_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            let mut should_close = false;
            if ui
                .selectable_value(&mut should_close, self.is_pane_closable(), "Close tab.")
                .clicked()
            {
                if should_close {
                    todo!("HANDLE CLOSING THE PANE");
                }
                ui.close_menu();
            };
        });

        resp
    }

    fn is_pane_closable(&self) -> bool {
        true
    }
}

use crate::controller::ribble_controller::RibbleController;
use crate::controller::{MAX_NUM_CONSOLE_MESSAGES, MIN_NUM_CONSOLE_MESSAGES};
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::{PaneView, PANE_INNER_MARGIN};
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
        should_close: &mut bool,
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

        let pane_id = egui::Id::new("user_prefs_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        let pane_color = ui.visuals().panel_fill;

        // TODO: this should a mechanism to be able to reset the layout.
        // -Not sure how this is going to happen just yet (try to avoid exposing the information to other tabs)
        // -Possibly make an "invalidate caches" or "set-to-default" in the main screen drop-down button.
        egui::Frame::default().fill(pane_color).inner_margin(PANE_INNER_MARGIN).show(ui, |ui| {
            ui.heading("Settings:");
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("user_prefs_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        // SET SYSTEM THEME
                        ui.label("System theme:");
                        // This is just so that the grid paints to the edge.
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt("user_theme_combobox")
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
                            // add a spacer to take up the remaining available width
                            ui.add_space(ui.available_width());
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


        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            ui.selectable_value(should_close, self.is_pane_closable(), "Close pane");
        });

        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

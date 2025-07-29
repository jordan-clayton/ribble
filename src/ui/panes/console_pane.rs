use crate::controller::ribble_controller::RibbleController;
use crate::controller::ConsoleMessage;
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::PaneView;
use std::sync::Arc;

#[derive(Clone, Default, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ConsolePane {
    // NOTE: These are shared ConsoleMessages (held in the ConsoleEngine).
    // It's cheaper to clone an Arc, versus String clones.
    #[serde(skip)]
    #[serde(default)]
    message_buffer: Vec<Arc<ConsoleMessage>>,
}

impl PaneView for ConsolePane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Console
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Console".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        should_close: &mut bool,
        controller: RibbleController,
    ) -> egui::Response {
        // Try to read the current messages (non-blocking).
        controller.try_get_current_messages(&mut self.message_buffer);

        let pane_id = egui::Id::new("console_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            // This cursor needs to be on the "bg -> not quite sure how to resolve this just yet"
            .on_hover_cursor(egui::CursorIcon::Grab);
        let bg_col = ui.visuals().extreme_bg_color;

        ui.heading("Console:");
        egui::Frame::default().fill(bg_col).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                // Fill space -inside- the scroll area.
                .auto_shrink([false; 2])
                .show(ui, |ui| {
                    ui.with_layout(
                        egui::Layout::top_down(egui::Align::LEFT).with_cross_justify(true),
                        |ui| {
                            for msg in self.message_buffer.iter() {
                                ui.label(msg.to_console_text(ui.visuals()));
                            }
                        },
                    );
                });
        });


        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            ui.selectable_value(should_close, self.is_pane_closable(), "Close tab.");
        });


        resp
    }

    fn is_pane_closable(&self) -> bool {
        true
    }
}

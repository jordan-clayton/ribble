use crate::controller::ribble_controller::RibbleController;
use crate::controller::ConsoleMessage;
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::{PaneView, PANE_INNER_MARGIN};
use std::sync::Arc;

// TODO: TEST THIS.
const SPACING_COEFF: f32 = 1.5;
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
            .on_hover_cursor(egui::CursorIcon::Grab);

        let console_bg_color = ui.visuals().extreme_bg_color;
        let panel_col = ui.visuals().panel_fill;

        // TODO: test this.
        // Set up a debug menu item to fire messages to the console.

        // I'm leaning towards an explicit margin that exposes the panel color.
        // The alternative is to wrap frames (outer = color, (inner = margin)) with explicit margins
        // to paint across the entire box.
        egui::Frame::default().fill(panel_col).inner_margin(PANE_INNER_MARGIN).show(ui, |ui| {
            ui.heading("Console:");
            egui::Frame::default().fill(console_bg_color).show(ui, |ui| {
                // NOTE: if swapping to a ScrollArea::both(), set the TextModeWrap in the label to Extend.
                egui::ScrollArea::vertical()
                    .stick_to_bottom(true)
                    // Fill space -inside- the scroll area.
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        // Set the spacing between messages to be a little bit larger.
                        let mut item_spacing = ui.spacing().item_spacing;
                        item_spacing.y *= SPACING_COEFF;
                        ui.style_mut().spacing.item_spacing = item_spacing;
                        // This should just... print things?
                        for msg in self.message_buffer.iter() {
                            ui.label(msg.to_console_text(ui.visuals()));
                        }
                    });
            });
        });

        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            ui.selectable_value(should_close, self.is_pane_closable(), "Close tab");
        });
        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

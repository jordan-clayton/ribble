use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::TabView;
use crate::ui::new_tabs::ribble_tab::RibbleTabId;

// NOTE: I'm not sure that any state is actually going to be stored in this tab.
// It might just be loaded from the controller.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct UserPreferencesTab {}

impl TabView for UserPreferencesTab {
    fn tile_id(&self) -> RibbleTabId {
        RibbleTabId::UserPreferences
    }

    fn tab_title(&self) -> egui::WidgetText {
        "Settings".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        controller: RibbleController,
    ) -> egui::Response {
        let prefs = controller.get_user_preferences();
        // Simple drawing:
        // Theme switcher -> can just be a drop-down

        // Console size -> can be a slider, but only call the explicit
        // resize function after the drag has finished/the number is set explicitly.

        todo!()
    }

    fn is_tab_closable(&self) -> bool {
        todo!()
    }
}

impl Default for UserPreferencesTab {
    fn default() -> Self {
        Self {}
    }
}

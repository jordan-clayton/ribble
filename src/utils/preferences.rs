pub fn get_app_theme(system_theme: Option<eframe::Theme>) -> catppuccin_egui::Theme {
    match system_theme {
        None => {
            catppuccin_egui::MOCHA
        }
        Some(t) => {
            match t {
                eframe::Theme::Dark => {
                    catppuccin_egui::MOCHA
                }
                eframe::Theme::Light => {
                    catppuccin_egui::LATTE
                }
            }
        }
    }
}
use directories::ProjectDirs;
use eframe;
use egui::ViewportBuilder;
use crate::ui::app::WhisperApp;
use crate::utils::constants;

mod ui;
mod utils;

fn main() -> eframe::Result<()>{
    let proj_dirs = ProjectDirs::from(constants::QUALIFIER, constants::ORGANIZATION, constants::APP_ID).expect("Failed to get proj dir");
    let data_dir = proj_dirs.data_dir();
    let mut native_options = eframe::NativeOptions::default();
    let viewport = build_viewport();

    // TODO: switch to true once default layout is done.
    native_options.persist_window = false;
    native_options.persistence_path = Some(data_dir.to_path_buf());
    native_options.viewport = viewport;


    eframe::run_native(constants::APP_ID, native_options, Box::new(|cc|Ok(Box::<WhisperApp>::default())))
}

// TODO: MacOS might require different configs to look more "Apple-y".
fn build_viewport() -> ViewportBuilder{
    let mut viewport = ViewportBuilder::default();
    viewport.app_id = Some(String::from(constants::APP_ID));
    // TODO: change if using a different title.
    viewport.title = Some(String::from(constants::APP_ID));
    // TODO: Add an icon.
    // TODO: add include_bytes() for assets.
    // let icon = eframe::icon_data::from_png_bytes().expect("invalid icon png");
    // let icon = Arc::new(icon);
    // viewport.icon = icon;
    viewport
}

use std::thread;

use directories::ProjectDirs;
use eframe;
use egui::ViewportBuilder;

use crate::{
    ui::app::WhisperApp,
    utils::{
        constants,
        errors::{WhisperAppError, WhisperAppErrorType},
        sdl_audio_wrapper::SdlAudioWrapper,
        threading::join_threads_loop,
    },
    whisper_app_context::WhisperAppController,
};

mod ui;
mod utils;
mod whisper_app_context;

fn main() -> Result<(), WhisperAppError> {
    let proj_dirs = ProjectDirs::from(
        constants::QUALIFIER,
        constants::ORGANIZATION,
        constants::APP_ID,
    )
        .expect("Failed to get proj dir");
    let data_dir = proj_dirs.data_dir();
    let mut native_options = eframe::NativeOptions::default();
    let viewport = build_viewport();

    // TODO: switch to true once default layout is done.
    native_options.persist_window = false;
    native_options.persistence_path = Some(data_dir.to_path_buf());
    native_options.viewport = viewport;

    // SDL.
    let sdl = sdl2::init().expect("Failed to initialize SDL");
    let audio_subsystem = sdl.audio().expect("Failed to initialize audio");

    let audio_wrapper = SdlAudioWrapper { audio_subsystem };
    let audio_wrapper = std::sync::Arc::new(audio_wrapper);

    // Bg thread queue
    let (sender, receiver) = crossbeam::channel::unbounded();
    let c_receiver = receiver.clone();

    // App controller - Theme is set upon app construction.
    let controller = WhisperAppController::new(audio_wrapper, None, sender);

    let c_controller = controller.clone();

    // Bg thread to join threads spawned by the app.
    let joiner_thread = thread::spawn(move || {
        join_threads_loop(c_receiver, c_controller);
    });

    let app = eframe::run_native(
        constants::APP_ID,
        native_options,
        Box::new(|cc| Ok(Box::new(WhisperApp::new(cc, controller)))),
    );

    let t = joiner_thread.join();
    if let Err(e) = app {
        let err = WhisperAppError::new(
            WhisperAppErrorType::GUIError,
            format!("Failed to set up GFX ctx, Error: {}", e),
        );
        return Err(err);
    }

    if let Err(e) = t {
        let err = WhisperAppError::new(
            WhisperAppErrorType::ThreadError,
            format!("Thread panicked. Error: {:?}", e),
        );
        return Err(err);
    }

    Ok(())
}

// TODO: MacOS might require different configs to look more "Apple-y".
fn build_viewport() -> ViewportBuilder {
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

use std::io::ErrorKind;
use std::thread;

use directories::ProjectDirs;
use eframe;
use eframe::emath::vec2;
use egui::ViewportBuilder;
use egui_extras::install_image_loaders;
use sdl2::log::log;
use whisper_realtime::downloader::request::reqwest;

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::app::WhisperApp,
    utils::{
        constants,
        errors::{WhisperAppError, WhisperAppErrorType},
        sdl_audio_wrapper::SdlAudioWrapper,
        threading::join_threads_loop,
    },
};
use crate::utils::workers::WorkerType;

mod controller;
mod ui;
mod utils;

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

    // Async runtime + downloading
    let client = reqwest::Client::new();
    let rt = tokio::runtime::Runtime::new();
    if let Err(e) = rt.as_ref() {
        let err = WhisperAppError::new(
            WhisperAppErrorType::Unknown,
            format!("Failed to build tokio runtime. Error: {}", e),
        );
        return Err(err);
    }
    let rt = rt.unwrap();

    let handle = rt.handle();

    // App controller - Theme is set upon app construction.
    let controller =
        WhisperAppController::new(client.clone(), handle.clone(), audio_wrapper, None, sender);

    let c_controller = controller.clone();
    let app_controller = controller.clone();

    // Bg thread to join threads spawned by the app.
    let joiner_thread = thread::spawn(move || {
        join_threads_loop(c_receiver, c_controller);
    });

    let app = eframe::run_native(
        constants::APP_ID,
        native_options,
        Box::new(|cc| {
            // Support for svg.
            install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(WhisperApp::new(cc, app_controller)))
        }),
    );

    // Pump a bogus thread to the joiner to get it to stop in case it's blocked.
    let finished = thread::spawn(|| {
        Ok(String::from("Finished"))
    });

    let _ = controller.send_thread_handle((WorkerType::IO, finished));

    #[cfg(debug_assertions)]
    log("App closed");
    // if failed to delete temporary file.
    if let Err(e) = controller.cleanup() {
        if e.kind() != ErrorKind::NotFound {
            let err = WhisperAppError::new(WhisperAppErrorType::IOError, format!("Failed to delete scratch buffer. Error: {}", e));
            eprintln!("{}", err);
        }
    }

    let t = joiner_thread.join();
    #[cfg(debug_assertions)]
    log("Threads joined");
    if let Err(e) = app {
        let err = WhisperAppError::new(
            WhisperAppErrorType::GUIError,
            format!("Failed to set up GFX ctx, Error: {}", e),
        );
        eprintln!("{}", err);
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
    // TODO: App title?
    // TODO: App icon?
    ViewportBuilder::default()
        .with_app_id(constants::APP_ID)
        .with_title(constants::APP_ID)
        .with_resizable(true)
        // Default window height.
        // TODO: make a constant
        .with_inner_size(vec2(1024.0, 768.0))
}

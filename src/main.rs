use mimalloc::MiMalloc;

use std::{io::ErrorKind, thread};

use directories::ProjectDirs;
use eframe::{emath::vec2, run_native, NativeOptions};
use egui::ViewportBuilder;
use egui_extras::install_image_loaders;
use sdl2::log::log;
use whisper_realtime::downloader::request::reqwest;

use crate::ui::widgets::icons::load_icon;
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

// Faster allocator, good for short strings and allocation churn.
static GLOBAL: MiMalloc = MiMalloc;

mod controller;
mod ui;
mod utils;

fn main() -> Result<(), WhisperAppError> {
    let proj_dirs = ProjectDirs::from(
        constants::QUALIFIER,
        constants::ORGANIZATION,
        constants::APP_ID,
    );

    if proj_dirs.is_none() {
        let err = WhisperAppError::new(
            WhisperAppErrorType::IOError,
            String::from("Failed to get project directory"),
            true,
        );
        return Err(err);
    }

    let proj_dirs = proj_dirs.unwrap();

    let data_dir = proj_dirs.data_dir();

    // Create the project directory if it doesn't exist

    if !data_dir.exists() {
        if let Err(e) = std::fs::create_dir_all(data_dir) {
            let err = WhisperAppError::new(
                WhisperAppErrorType::IOError,
                format!(
                    "Failed to create project directory, info: {}",
                    e.to_string()
                ),
                true,
            );
            return Err(err);
        }
    }

    let mut native_options = NativeOptions::default();
    let viewport = build_viewport();

    native_options.persist_window = true;
    native_options.persistence_path = Some(data_dir.to_path_buf());
    native_options.viewport = viewport;

    // SDL.
    let sdl = sdl2::init();
    if let Err(e) = &sdl {
        let err = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Failed to initialize sdl. Info: {}", e),
            true,
        );
        return Err(err);
    }

    let sdl = sdl.unwrap();

    let audio_subsystem = sdl.audio();
    if let Err(e) = &audio_subsystem {
        let err = WhisperAppError::new(
            WhisperAppErrorType::ParameterError,
            format!("Failed to initialize sdl. Info: {}", e),
            true,
        );
        return Err(err);
    }

    let audio_subsystem = audio_subsystem.unwrap();

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
            format!("Failed to build tokio runtime. Info: {}", e),
            true,
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

    let app = run_native(
        constants::APP_ID,
        native_options,
        Box::new(|cc| {
            // Support for svg.
            install_image_loaders(&cc.egui_ctx);
            Ok(Box::new(WhisperApp::new(cc, app_controller)))
        }),
    );

    // Send a "Finished" message to close the background joiner thread.
    let finished = thread::spawn(|| Ok(String::from(constants::CLOSE_APP)));

    let _ = controller.send_thread_handle(finished);

    #[cfg(debug_assertions)]
    log("App closed");
    // if failed to delete temporary file.
    if let Err(e) = controller.cleanup() {
        if e.kind() != ErrorKind::NotFound {
            let err = WhisperAppError::new(
                WhisperAppErrorType::IOError,
                format!("Failed to delete scratch buffer. Info: {}", e),
                true,
            );
            eprintln!("{}", err);
        }
    }

    let t = joiner_thread.join();
    #[cfg(debug_assertions)]
    log("Threads joined");
    if let Err(e) = app {
        let err = WhisperAppError::new(
            WhisperAppErrorType::GUIError,
            format!("Failed to set up GFX ctx, Info: {}", e),
            true,
        );
        eprintln!("{}", err);
    }

    if let Err(e) = t {
        let err = WhisperAppError::new(
            WhisperAppErrorType::ThreadError,
            format!("Thread panicked. Info: {:?}", e),
            true,
        );
        return Err(err);
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn build_viewport() -> ViewportBuilder {
    let image_bytes = include_bytes!("assets/whisper_app_icon_128x128@1x.png");
    let icon = load_icon(image_bytes);
    let mut viewport = ViewportBuilder::default()
        .with_app_id(constants::APP_ID)
        .with_title(constants::APP_ID)
        .with_resizable(true)
        .with_inner_size(vec2(1024.0, 768.0));

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }
    viewport
}

// TODO: MacOs may require more "Apple-like" configurations.
#[cfg(target_os = "macos")]
fn build_viewport() -> ViewportBuilder {
    let image_bytes = include_bytes!("assets/whisper_app_icon_1024x1024@1x.png");
    let icon = load_icon(image_bytes);
    let mut viewport = ViewportBuilder::default()
        .with_app_id(constants::APP_ID)
        .with_title(constants::APP_ID)
        .with_resizable(true)
        .with_titlebar_shown(false)
        .with_inner_size(vec2(1024.0, 768.0));

    if let Some(icon_data) = icon {
        viewport = viewport.with_icon(icon_data);
    }
    viewport
}

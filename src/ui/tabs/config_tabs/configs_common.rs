use std::path::PathBuf;
use std::thread;

use egui::{Button, Checkbox, ComboBox, Slider, Ui};
use whisper_realtime::model::{Model, ModelType};

use crate::utils::configs::AudioConfigType;
use crate::utils::configs::WorkerType;
use crate::utils::constants;
use crate::utils::errors::{WhisperAppError, WhisperAppErrorType};
use crate::utils::file_mgmt::copy_data;
use crate::whisper_app_context::WhisperAppController;

// I'm not 100% sold on this - It might be worth the heap allocation?
// TODO: Might also be better to avoid the branching & supply the boolean + callback.
pub fn model_row(
    ui: &mut Ui,
    model: &mut ModelType,
    configs_type: AudioConfigType,
    controller: WhisperAppController,
    available_models: &[ModelType],
) {
    let c_controller = controller.clone();
    let (ready, update_ready) = match configs_type {
        AudioConfigType::Realtime => {
            let ready = controller.realtime_ready();
            let f = move |r| c_controller.update_realtime_ready(r);
            let f: Box<dyn Fn(bool)> = Box::new(f);
            (ready, f)
        }
        AudioConfigType::Static => {
            let ready = controller.static_ready();
            let f = move |r| c_controller.update_static_ready(r);
            let f: Box<dyn Fn(bool)> = Box::new(f);
            (ready, f)
        }
        _ => {
            let err = WhisperAppError::new(
                WhisperAppErrorType::ParameterError,
                String::from("Invalid config type passed to model ui builder"),
            );
            panic!("{}", err);
        }
    };

    let downloading = controller.is_downloading();

    ui.label("Model:").on_hover_ui(|ui| {
        ui.style_mut().interaction.selectable_labels = true;
        ui.label("Select the desired model for transcribing");
    });

    ui.horizontal(|ui| {
        ComboBox::from_id_source("modeltype")
            .selected_text(model.to_string())
            .show_ui(ui, |ui| {
                for m in available_models {
                    ui.selectable_value(model, *m, m.to_string());
                }
            });

        let dir = eframe::storage_dir(constants::APP_ID).expect("Failed to get data dir.");
        let mut m_model = Model::new_with_type_and_dir(*model, dir);
        let model_downloaded = m_model.is_downloaded();

        if model_downloaded {
            if !ready {
                update_ready(true);
            }
            // Okay icon
            // ui.add(okay icon);
            ui.label("-Okay icon- here");
        } else {
            if ready {
                update_ready(false);
            }
            // Warning icon
            // ui.add(warning icon);
            ui.label("-Warning icon- here");
        }

        let model_path_open = m_model.file_path();

        if ui
            .button("Open Model")
            .on_hover_ui(|ui| {
                ui.style_mut().interaction.selectable_labels = true;
                ui.label(format!("Open a compatible {}, model.", model.to_string()));
            })
            .clicked()
        {
            // Open File dialog at HOME directory, fallback to root.
            let base_dirs = directories::BaseDirs::new();
            let dir = if let Some(dir) = base_dirs {
                dir.home_dir().to_path_buf()
            } else {
                PathBuf::from("/")
            };
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("ggml model", &["bin"])
                .set_directory(dir)
                .pick_file()
            {
                let from = p.clone();
                let to = model_path_open.clone();
                let copy_thread = thread::spawn(move || {
                    let success = copy_data(&from, &to);
                    match success {
                        Ok(_) => Ok(format!(
                            "File: {:?}, successfully copied to: {:?}",
                            from.as_os_str(),
                            to.as_os_str()
                        )),
                        Err(e) => {
                            panic!("{}", e)
                        }
                    }
                });

                let worker = (WorkerType::Downloading, copy_thread);
                controller
                    .send_thread_handle(worker)
                    .expect("Thread channel closed")
            }
        }
        if ui
            .add_enabled(!downloading, Button::new("Download"))
            .on_hover_ui(|ui| {
                ui.style_mut().interaction.selectable_labels = true;
                ui.label(format!("Download compatible {} model", model.to_string()));
            })
            .clicked()
        {
            let url = m_model.url();
            let file_name = m_model.model_file_name().to_owned();
            let directory = m_model.model_directory();
            controller.start_download(url, file_name, directory)
        }
    });

    ui.end_row();
}

pub fn n_threads_row(ui: &mut Ui, n_threads: &mut std::ffi::c_int, max_threads: std::ffi::c_int) {
    ui.label("Threads:").on_hover_ui(|ui| {
        ui.style_mut().interaction.selectable_labels = true;
        ui.label("Select the number of threads to allocate for transcription");
        ui.label(format!("Recommended: {}", std::cmp::min(7, max_threads)));
    });

    ui.add(Slider::new(
        n_threads,
        1..=std::cmp::min(max_threads, constants::MAX_WHISPER_THREADS),
    ));

    ui.end_row();
}

pub fn use_gpu_row(ui: &mut Ui, use_gpu: &mut bool, gpu_capable: bool) {
    *use_gpu = *use_gpu & gpu_capable;
    ui.label("Hardware Accelerated (GPU):").on_hover_ui(|ui| {
        ui.style_mut().interaction.selectable_labels = true;
        ui.label(
            "Enable hardware acceleration (if supported). REQUIRED to use large models in realtime mode.",
        );
    });
    ui.add_enabled(gpu_capable, Checkbox::without_text(use_gpu))
        .on_hover_ui(|ui| {
            ui.style_mut().interaction.selectable_labels = true;
            let status = if gpu_capable {
                "supported"
            } else {
                "unsupported"
            };
            ui.label(format!("Hardware acceleration is {}", status));
        });

    ui.end_row();
}

pub fn set_language_row(ui: &mut Ui, language: &mut Option<String>) {
    ui.label("Language:").on_hover_ui(|ui| {
        ui.style_mut().interaction.selectable_labels = true;
        ui.label("Select input language. Set to Auto for auto-detection");
    });

    ComboBox::from_id_source("language")
        .selected_text(
            *constants::LANGUAGE_CODES
                .get(language)
                .expect("Failed to get language"),
        )
        .show_ui(ui, |ui| {
            for (k, v) in constants::LANGUAGE_OPTIONS.iter() {
                ui.selectable_value(language, v.clone(), *k);
            }
        });

    ui.end_row();
}

pub fn set_translate_row(ui: &mut Ui, set_translate: &mut bool) {
    ui.label("Translate").on_hover_ui(|ui| {
        ui.style_mut().interaction.selectable_labels = true;
        ui.label("Translate transcription (to English ONLY)");
    });

    ui.add(Checkbox::without_text(set_translate));

    ui.end_row();
}

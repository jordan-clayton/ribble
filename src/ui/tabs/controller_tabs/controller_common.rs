use std::{path::PathBuf, thread};

use catppuccin_egui::Theme;
use egui::{Button, Checkbox, ComboBox, Slider, Ui};
use whisper_realtime::model::{Model, ModelType};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::widgets::icons::ok_icon,
    utils::{
        constants
        ,
        file_mgmt::copy_data

        ,
    },
};
use crate::ui::widgets::icons::warning_icon;
use crate::utils::workers::WorkerType;

pub fn model_row(
    ui: &mut Ui,
    model: &mut ModelType,
    m_model: &Model,
    downloaded: bool,
    controller: WhisperAppController,
    available_models: &[ModelType],
    theme: Option<Theme>,
) {
    let downloading = controller.is_downloading();

    let model_path = m_model.file_path();

    ui.horizontal(|ui| {
        ui.label("Model:").on_hover_ui(|ui| {
            ui.label("Select the desired model for transcribing");
        });

        if downloaded {
            ui.add(ok_icon(None, theme)).on_hover_ui(|ui| {
                ui.label("Model found.");
            });
        } else {
            ui.add(warning_icon(None, theme)).on_hover_ui(|ui| {
                ui.label("Model not found");
            });
        }
    });

    ui.horizontal(|ui| {
        ComboBox::from_id_source("modeltype")
            .selected_text(model.to_string())
            .show_ui(ui, |ui| {
                for m in available_models {
                    ui.selectable_value(model, *m, m.to_string());
                }
            });

        if ui
            .button("Open")
            .on_hover_ui(|ui| {
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
                .add_filter("ggml model (.bin)", &["bin"])
                .set_directory(dir)
                .pick_file()
            {
                let from = p.clone();
                let to = model_path.to_path_buf();
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
}

pub fn n_threads_row(ui: &mut Ui, n_threads: &mut std::ffi::c_int, max_threads: std::ffi::c_int) {
    ui.label("Threads:").on_hover_ui(|ui| {
        ui.label("Select the number of threads to allocate for transcription");
        ui.label(format!("Recommended: {}", std::cmp::min(7, max_threads)));
    });

    ui.add(Slider::new(
        n_threads,
        1..=std::cmp::min(max_threads, constants::MAX_WHISPER_THREADS),
    ));
}

pub fn use_gpu_row(ui: &mut Ui, use_gpu: &mut bool, gpu_capable: bool) {
    *use_gpu = *use_gpu & gpu_capable;
    ui.label("Hardware Accelerated (GPU):").on_hover_ui(|ui| {
        ui.label(
            "Enable hardware acceleration (if supported). REQUIRED to use large models in realtime mode.",
        );
    });
    ui.add_enabled(gpu_capable, Checkbox::without_text(use_gpu))
        .on_hover_ui(|ui| {
            let status = if gpu_capable {
                "supported"
            } else {
                "unsupported"
            };
            ui.label(format!("Hardware acceleration is {}", status));
        });
}

pub fn set_language_row(ui: &mut Ui, language: &mut Option<String>) {
    ui.label("Language:").on_hover_ui(|ui| {
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
}

pub fn set_translate_row(ui: &mut Ui, set_translate: &mut bool) {
    ui.label("Translate").on_hover_ui(|ui| {
        ui.label("Translate transcription (to English ONLY)");
    });

    ui.add(Checkbox::without_text(set_translate));
}

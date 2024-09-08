use std::{path::PathBuf, thread};

use catppuccin_egui::Theme;
use egui::{Button, Checkbox, ComboBox, Slider, Ui};
use whisper_realtime::model::{Model, ModelType};

use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::widgets::icons::{ok_icon, warning_icon},
    utils::{
        constants,
        errors::{WhisperAppError, WhisperAppErrorType},
        file_mgmt::copy_data, threading::get_max_threads},
};

pub fn save_transcription_button(ui: &mut Ui, controller: WhisperAppController) {
    if ui.add(Button::new("Save Transcription")).clicked() {
        // Open File dialog at HOME directory, fallback to root.
        let base_dirs = directories::BaseDirs::new();
        let dir = if let Some(dir) = base_dirs {
            dir.home_dir().to_path_buf()
        } else {
            PathBuf::from("/")
        };

        if let Some(p) = rfd::FileDialog::new()
            .add_filter("text (.txt)", &["txt"])
            .set_directory(dir)
            .save_file()
        {
            controller.save_transcription(&p);
        }
    }
}

pub fn model_stack(
    ui: &mut Ui,
    model: &mut ModelType,
    m_model: &Model,
    downloaded: bool,
    controller: WhisperAppController,
    available_models: &[ModelType],
    theme: Option<Theme>,
    pointer_still: bool,
) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;

    let downloading = controller.is_downloading();

    let model_path = m_model.file_path();

    ui.horizontal(|ui| {
        ui.label("Model:");

        if downloaded {
            let resp = ui.add(ok_icon(None, theme));
            if pointer_still {
                resp.on_hover_ui(|ui| {
                    ui.label("Model found.");
                });
            }
        } else {
            let resp = ui.add(warning_icon(None, theme));
            if pointer_still {
                resp.on_hover_ui(|ui| {
                    ui.label("Model not found.");
                });
            }
        }
    });

    ui.horizontal(|ui| {
        let mut resp = ComboBox::from_id_source("modeltype")
            .selected_text(model.to_string())
            .show_ui(ui, |ui| {
                for m in available_models {
                    ui.selectable_value(model, *m, m.to_string());
                }
            })
            .response;
        if pointer_still {
            resp = resp.on_hover_ui(|ui| {
                ui.label("Select the desired model for transcribing.");
            });
        }
        resp.context_menu(|ui| {
            if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                *model = ModelType::default();
                ui.close_menu();
            }
        });

        // OPEN BUTTON
        let mut resp = ui.button("Open");
        if pointer_still {
            resp = resp.on_hover_ui(|ui| {
                ui.label(format!("Open a compatible {} model.", model.to_string()));
            });
        }
        if resp.clicked() {
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
                            let err = WhisperAppError::new(
                                WhisperAppErrorType::IOError,
                                format!("Failed to copy file. Info: {}", e.to_string()),
                                false,
                            );
                            Err(err)
                        }
                    }
                });

                controller
                    .send_thread_handle(copy_thread)
                    .expect("Thread channel should be open.")
            }
        }

        // DOWNLOAD BUTTON
        let mut resp = ui.add_enabled(!downloading, Button::new("Download"));

        if pointer_still {
            resp = resp.on_hover_ui(|ui| {
                ui.label(format!("Download compatible {} model", model.to_string()));
            })
        }

        if resp.clicked() {
            let url = m_model.url();
            let file_name = m_model.model_file_name().to_owned();
            let directory = m_model.model_directory();
            controller.start_download(url, file_name, directory)
        }
    });
}

pub fn n_threads_stack(
    ui: &mut Ui,
    n_threads: &mut std::ffi::c_int,
    max_threads: std::ffi::c_int,
    pointer_still: bool,
) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;
    ui.label("Threads:");

    let mut resp = ui.add(Slider::new(
        n_threads,
        1..=std::cmp::min(max_threads, constants::MAX_WHISPER_THREADS),
    ));

    if pointer_still {
        resp = resp.on_hover_ui(|ui| {
            ui.label("Select the number of threads to allocate for transcription.");
            ui.label(format!("Recommended: {}", std::cmp::min(7, max_threads)));
        });
    }
    resp.context_menu(|ui| {
        if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
            *n_threads = get_max_threads().min(4);
            ui.close_menu();
        }
    });
}

pub fn use_gpu_stack(ui: &mut Ui, use_gpu: &mut bool, gpu_capable: bool, pointer_still: bool) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;
    *use_gpu = *use_gpu & gpu_capable;
    ui.label("Hardware Acceleration (GPU):");

    let mut resp = ui.add_enabled(gpu_capable, Checkbox::without_text(use_gpu));

    if pointer_still {
        resp = resp
            .on_hover_ui(|ui| {
                ui.label("Enable hardware acceleration. Required for large models in realtime.");
            })
            .on_disabled_hover_ui(|ui| {
                ui.label(
                    "Hardware acceleration is not supported. Realtime model selection limited.",
                );
            });
    }

    resp.context_menu(|ui| {
        if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
            *use_gpu = gpu_capable;
            ui.close_menu();
        }
    });
}

pub fn set_language_stack(ui: &mut Ui, language: &mut Option<String>, pointer_still: bool) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;
    ui.label("Language:");

    let mut resp = ComboBox::from_id_source("language")
        .selected_text(
            *constants::LANGUAGE_CODES
                .get(language)
                .expect("Language should be retrieved from LANGUAGE_CODES."),
        )
        .show_ui(ui, |ui| {
            for (k, v) in constants::LANGUAGE_OPTIONS.iter() {
                ui.selectable_value(language, v.clone(), *k);
            }
        })
        .response;

    if pointer_still {
        resp = resp.on_hover_ui(|ui| {
            ui.label("Select input language. Set to Auto for auto-detection.");
        });
    }

    resp.context_menu(|ui| {
        if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
            *language = Some(String::from("en"));
            ui.close_menu();
        }
    });
}

pub fn set_translate_stack(ui: &mut Ui, set_translate: &mut bool, pointer_still: bool) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;
    ui.label("Translate:");

    let mut resp = ui.add(Checkbox::without_text(set_translate));

    if pointer_still {
        resp = resp.on_hover_ui(|ui| {
            ui.label("Translate transcription into English.");
        });
    }

    resp.context_menu(|ui| {
        if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
            *set_translate = false;
            ui.close_menu();
        }
    });
}

pub fn toggle_bandpass_filter_stack(ui: &mut Ui, filter: &mut bool, pointer_still: bool) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;
    ui.label("Bandpass Filter:");
    let mut resp = ui.add(Checkbox::without_text(filter));

    if pointer_still {
        resp = resp.on_hover_ui(|ui| {
            ui.label("Run a bandpass filter to clean up audio.");
        });
    }

    resp.context_menu(|ui| {
        if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
            *filter = false;
            ui.close_menu();
        }
    });
}

pub fn f_higher_stack(ui: &mut Ui, filter: bool, f_higher: &mut f32, pointer_still: bool) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;
    ui.add_enabled_ui(filter, |ui| {
        // High Threshold
        ui.label("High frequency cutoff:");
    });

    ui.add_enabled_ui(filter, |ui| {
        let mut resp = ui.add(
            Slider::new(f_higher, constants::MIN_F_HIGHER..=constants::MAX_F_HIGHER).suffix("Hz"),
        );
        if pointer_still {
            resp = resp.on_hover_ui(|ui| {
                ui.label("Frequencies higher than this threshold will be filtered out.");
            });
        }

        resp.context_menu(|ui| {
            if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                *f_higher = constants::DEFAULT_F_HIGHER;
                ui.close_menu();
            }
        })
    });
}

pub fn f_lower_stack(ui: &mut Ui, filter: bool, f_lower: &mut f32, pointer_still: bool) {
    let style = ui.style_mut();
    style.interaction.show_tooltips_only_when_still = true;
    style.interaction.tooltip_grace_time = constants::TOOLTIP_GRACE_TIME;
    style.interaction.tooltip_delay = constants::TOOLTIP_DELAY;

    ui.add_enabled_ui(filter, |ui| {
        ui.label("Low frequency cutoff:");
    });
    ui.add_enabled_ui(filter, |ui| {
        let mut resp = ui.add(
            Slider::new(f_lower, constants::MIN_F_LOWER..=constants::MAX_F_LOWER).suffix("Hz"),
        );

        if pointer_still {
            resp = resp.on_hover_ui(|ui| {
                ui.label("Frequencies lower than this threshold will be filtered out.");
            });
        }

        resp.context_menu(|ui| {
            if ui.button(constants::DEFAULT_BUTTON_LABEL).clicked() {
                *f_lower = constants::DEFAULT_F_LOWER;
                ui.close_menu();
            }
        })
    });
}

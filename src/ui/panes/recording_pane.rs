use crate::controller::ribble_controller::RibbleController;
use crate::controller::CompletedRecordingJobs;
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::PaneView;
use crate::ui::widgets::recording_modal::build_recording_modal;
use crate::ui::{DEFAULT_TOAST_DURATION, GRID_ROW_SPACING_COEFF, PANE_INNER_MARGIN};
use crate::utils::recorder_configs::{
    RibbleChannels, RibbleExportFormat, RibblePeriod, RibbleSampleRate,
};
use std::sync::Arc;
use strum::IntoEnumIterator;

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct RecordingPane {
    #[serde(skip)]
    #[serde(default)]
    recordings_buffer: Vec<(Arc<str>, CompletedRecordingJobs)>,
    #[serde(skip)]
    #[serde(default)]
    recording_modal: bool,
}

impl PaneView for RecordingPane {
    fn pane_id(&self) -> RibblePaneId {
        RibblePaneId::Recording
    }

    fn pane_title(&self) -> egui::WidgetText {
        "Recording".into()
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        should_close: &mut bool,
        controller: RibbleController,
    ) -> egui::Response {
        // Runner flags
        let recorder_running = controller.recorder_running();
        let audio_worker_running = recorder_running || controller.transcriber_running();

        // Configs/Export
        let configs = *controller.read_recorder_configs();
        let mut export_format = controller.read_export_format();

        // Pane UI
        let pane_col = ui.visuals().panel_fill;

        // Lowest-priority pane response (for dragging + passthrough interactions)
        let pane_id = egui::Id::new("recording_pane");
        let resp = ui
            .interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag())
            .on_hover_cursor(egui::CursorIcon::Grab);

        egui::Frame::default()
            .fill(pane_col)
            .inner_margin(PANE_INNER_MARGIN)
            .show(ui, |ui| {
                ui.heading("Recording:");
                let button_spacing = ui.spacing().button_padding.y;

                ui.vertical_centered_justified(|ui| {
                    if ui
                        .add_enabled(
                            !audio_worker_running,
                            egui::Button::new("Start recording"),
                        )
                        .on_hover_cursor(egui::CursorIcon::Default)
                        .clicked()
                    {
                        controller.start_recording();
                    }
                    ui.add_space(button_spacing);

                    if ui
                        .add_enabled(recorder_running, egui::Button::new("Stop"))
                        .on_hover_cursor(egui::CursorIcon::Default)
                        .clicked()
                    {
                        controller.stop_recording();
                    }
                });

                ui.add_space(button_spacing);
                ui.separator();

                egui::ScrollArea::both()
                    .auto_shrink([false; 2])
                    .show(ui, |ui| {
                        ui.heading("Export: ");
                        ui.vertical_centered_justified(|ui| {
                            // This implies there is at least one recording that can be exported.
                            let latest_exists = controller.latest_recording_exists();

                            if ui
                                .add_enabled(latest_exists, egui::Button::new("Export recording"))
                                .on_hover_cursor(egui::CursorIcon::Default)
                                .clicked()
                            {
                                self.recording_modal = true;
                            }

                            ui.add_space(button_spacing);
                            ui.separator();
                        });

                        ui.heading("Configs:");
                        ui.vertical_centered_justified(|ui| {
                            let configs_dropdown = ui.collapsing("Recording Configs", |ui| {
                                ui.add_enabled_ui(!recorder_running, |ui| {
                                    egui::Grid::new("recording_configs_grid")
                                        .num_columns(2)
                                        .striped(true)
                                        .min_row_height(
                                            ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF,
                                        )
                                        .show(ui, |ui| {
                                            ui.label("Sample Rate:");
                                            let mut sample_rate = configs.sample_rate();
                                            ui.horizontal(|ui| {
                                                egui::ComboBox::from_id_salt("sample_rate_combobox")
                                                    .selected_text(sample_rate.as_ref())
                                                    .show_ui(ui, |ui| {
                                                        for rate in RibbleSampleRate::iter() {
                                                            if ui
                                                                .selectable_value(
                                                                    &mut sample_rate,
                                                                    rate,
                                                                    rate.as_ref(),
                                                                )
                                                                .clicked()
                                                            {
                                                                let new_configs = configs
                                                                    .with_sample_rate(sample_rate);
                                                                controller.write_recorder_configs(
                                                                    new_configs,
                                                                );
                                                            }
                                                        }
                                                    })
                                                    .response
                                                    .on_hover_cursor(egui::CursorIcon::Default);

                                                ui.add_space(ui.available_width());
                                            });

                                            ui.end_row();

                                            ui.label("Channels:");
                                            let mut channels = configs.num_channels();
                                            egui::ComboBox::from_id_salt("ribble_channels_combobox")
                                                .selected_text(channels.as_ref())
                                                .show_ui(ui, |ui| {
                                                    for ch_conf in RibbleChannels::iter() {
                                                        if ui
                                                            .selectable_value(
                                                                &mut channels,
                                                                ch_conf,
                                                                ch_conf.as_ref(),
                                                            )
                                                            .clicked()
                                                        {
                                                            let new_configs =
                                                                configs.with_num_channels(channels);
                                                            controller
                                                                .write_recorder_configs(new_configs);
                                                        }
                                                    }
                                                })
                                                .response
                                                .on_hover_cursor(egui::CursorIcon::Default);
                                            ui.end_row();

                                            ui.label("Buffer size:");
                                            let mut period = configs.period();
                                            egui::ComboBox::from_id_salt("buffer_size_combobox")
                                                .selected_text(period.as_ref())
                                                .show_ui(ui, |ui| {
                                                    for period_conf in RibblePeriod::iter() {
                                                        if ui
                                                            .selectable_value(
                                                                &mut period,
                                                                period_conf,
                                                                period_conf.as_ref(),
                                                            )
                                                            .clicked()
                                                        {
                                                            let new_configs =
                                                                configs.with_period(period);
                                                            controller
                                                                .write_recorder_configs(new_configs);
                                                        }
                                                    }
                                                })
                                                .response
                                                .on_hover_cursor(egui::CursorIcon::Default);

                                            ui.end_row();

                                            ui.label("Reset settings:");
                                            if ui
                                                .button("Reset")
                                                .on_hover_cursor(egui::CursorIcon::Default)
                                                .clicked()
                                            {
                                                controller.write_recorder_configs(Default::default());
                                            }
                                        });
                                });
                            });
                            configs_dropdown
                                .header_response
                                .on_hover_cursor(egui::CursorIcon::Default);

                            let export_dropdown = ui.collapsing("Export Configs", |ui| {
                                egui::Grid::new("recording_export_format")
                                    .num_columns(2)
                                    .min_row_height(
                                        ui.spacing().interact_size.y * GRID_ROW_SPACING_COEFF,
                                    )
                                    .show(ui, |ui| {
                                        ui.label("Export format").on_hover_text(
                                            "Set the (wav) audio file export format. Supported: I16, F32.",
                                        );
                                        // Recording Format Combobox.
                                        egui::ComboBox::from_id_salt("export_format_combobox")
                                            .selected_text(export_format.as_ref())
                                            .show_ui(ui, |ui| {
                                                for format in RibbleExportFormat::iter() {
                                                    // NOTE: at the moment, the RecordingExportFormat is not stored anywhere
                                                    // It will initialize to the default upon the pane loading
                                                    if ui
                                                        .selectable_value(
                                                            &mut export_format,
                                                            format,
                                                            format.as_ref(),
                                                        )
                                                        .on_hover_text(format.tooltip())
                                                        .clicked()
                                                    {
                                                        controller
                                                            .write_export_format(export_format);
                                                    };
                                                }
                                            })
                                            .response
                                            .on_hover_cursor(egui::CursorIcon::Default);
                                        ui.end_row();
                                    });
                            });

                            export_dropdown
                                .header_response
                                .on_hover_cursor(egui::CursorIcon::Default);
                        });
                    });
            });

        if self.recording_modal {
            controller.try_read_recording_metadata(&mut self.recordings_buffer);
            // NOTE: this is a very cheap clone, so it should be fine to just cache and pass into the closure.
            let err_ctx = ui.ctx().clone();
            let handle_recordings = |file_name| {
                if controller
                    .try_get_recording_path(Arc::clone(&file_name))
                    .is_some()
                {
                    if let Some(out_path) = rfd::FileDialog::new()
                        .add_filter("wav", &["wav"])
                        .set_directory(controller.base_dir())
                        .save_file()
                    {
                        self.recording_modal = false;
                        // This is a little bit of a tricky detail of RFD + GTK.
                        // The extension isn't always appended to the end of the file name,
                        // so there needs to be an explicit check to ensure.
                        // MacOs and Windows will both append the proper extension.
                        #[cfg(target_os = "linux")]
                        {
                            let out_path = if out_path.extension().is_some_and(|ext| ext == "wav") {
                                out_path
                            } else {
                                out_path.with_extension("wav")
                            };
                            controller.export_recording(out_path, file_name, export_format);
                        }

                        #[cfg(not(target_os = "linux"))]
                        {
                            controller.export_recording(out_path, file_name, export_format);
                        }
                    }
                } else if controller
                    .try_get_recording_path(Arc::clone(&file_name))
                    .is_none()
                {
                    log::warn!("Temporary recording file missing: {file_name}");
                    let mut toast = egui_notify::Toast::warning("Failed to find saved recording.");
                    toast.duration(Some(DEFAULT_TOAST_DURATION));
                    controller.send_toast(toast);
                    err_ctx.request_repaint();
                }
            };

            let modal = build_recording_modal(
                ui,
                "recorder_recording_modal",
                "recorder_recording_grid",
                &controller,
                &self.recordings_buffer,
                handle_recordings,
            );

            if modal.should_close() {
                self.recording_modal = false;
            }
        }

        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            ui.selectable_value(should_close, self.is_pane_closable(), "Close pane");
        });

        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

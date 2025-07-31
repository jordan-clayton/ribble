use crate::controller::ribble_controller::RibbleController;
use crate::controller::CompletedRecordingJobs;
use crate::ui::panes::ribble_pane::RibblePaneId;
use crate::ui::panes::PaneView;
use crate::utils::recorder_configs::{
    RibbleChannels, RibblePeriod, RibbleRecordingExportFormat, RibbleSampleRate,
};
use std::sync::Arc;
use strum::IntoEnumIterator;

#[derive(Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordingPane {
    #[serde(skip)]
    #[serde(default)]
    recordings_buffer: Vec<(Arc<str>, CompletedRecordingJobs)>,
    #[serde(skip)]
    #[serde(default)]
    recording_modal: bool,
    // TODO: if this becomes genuinely important to store,
    // stick it... somewhere, like the kernel.
    #[serde(skip)]
    #[serde(default)]
    export_format: RibbleRecordingExportFormat,
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
        let recorder_running = controller.recorder_running();
        let audio_worker_running = recorder_running || controller.transcriber_running();
        let configs = *controller.read_recorder_configs();


        // TODO: this might not work just yet - test out and remove this todo if it's right.
        // Create a (hopefully) lower-priority pane-sized interaction hitbox
        let pane_id = egui::Id::new("recording_pane");
        let resp = ui.interact(ui.max_rect(), pane_id, egui::Sense::click_and_drag());

        ui.heading("Recording:");
        egui::Frame::default().show(ui, |ui| {
            let button_spacing = ui.spacing().button_padding.y;
            ui.vertical_centered_justified(|ui| {
                if ui
                    .add_enabled(!audio_worker_running, egui::Button::new("Start recording"))
                    .clicked()
                {
                    controller.start_recording();
                }
            });
            ui.add_space(button_spacing);

            if ui
                .add_enabled(recorder_running, egui::Button::new("Stop"))
                .clicked()
            {
                controller.stop_recording();
            }

            ui.add_space(button_spacing);
            ui.separator();
            // This implies there is at least one recording that can be exported.
            let latest_exists = controller.latest_recording_exists();

            if ui
                .add_enabled(latest_exists, egui::Button::new("Export recording"))
                .clicked()
            {
                self.recording_modal = true;
            }

            ui.add_space(button_spacing);
            ui.separator();
            ui.collapsing("Recording Configs", |ui| {
                egui::Grid::new("recording_configs_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Sample Rate:");
                        let mut sample_rate = configs.sample_rate();
                        egui::ComboBox::from_id_salt("sample_rate_combobox")
                            .selected_text(sample_rate.as_ref())
                            .show_ui(ui, |ui| {
                                for rate in RibbleSampleRate::iter() {
                                    if ui
                                        .selectable_value(&mut sample_rate, rate, rate.as_ref())
                                        .clicked()
                                    {
                                        let new_configs = configs.with_sample_rate(sample_rate);
                                        controller.write_recorder_configs(new_configs);
                                    }
                                }
                            });

                        ui.end_row();

                        ui.label("Channels:");
                        let mut channels = configs.num_channels();
                        egui::ComboBox::from_id_salt("ribble_channels_combobox")
                            .selected_text(channels.as_ref())
                            .show_ui(ui, |ui| {
                                for ch_conf in RibbleChannels::iter() {
                                    if ui
                                        .selectable_value(&mut channels, ch_conf, ch_conf.as_ref())
                                        .clicked()
                                    {
                                        let new_configs = configs.with_num_channels(channels);
                                        controller.write_recorder_configs(new_configs);
                                    }
                                }
                            });
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
                                        let new_configs = configs.with_period(period);
                                        controller.write_recorder_configs(new_configs);
                                    }
                                }
                            });

                        ui.end_row();
                    });
            });
        });

        if self.recording_modal {
            let recording_id = egui::Id::new("recording_recordings_modal");
            let modal = egui::Modal::new(recording_id).show(ui.ctx(), |ui| {
                controller.try_get_completed_recordings(&mut self.recordings_buffer);
                // TODO: figure out the sizing here a little bit more nicely.
                ui.set_width_range(70f32..=100f32);

                let header_height = egui::TextStyle::Heading.resolve(ui.style()).size;
                let header_width = ui.max_rect().width();
                let desired_size = egui::Vec2::new(header_width, header_height);

                // NOTE: this is duplicated from the Transcriber view (similar modal)
                // If this implementation diverges, then keep it here.
                // Otherwise, look at making it a common function.
                ui.allocate_ui_with_layout(
                    desired_size,
                    egui::Layout::left_to_right(egui::Align::Center).with_main_justify(true),
                    |ui| {
                        ui.heading("Previous recordings:");
                        if ui.button("Clear recordings").clicked() {
                            // This internally guards against grandma clicks.
                            controller.clear_recording_cache();
                        }
                    },
                );

                // 1 row grid: Export Format: Dropdown.
                egui::Grid::new("export_format_grid")
                    .num_columns(2)
                    .striped(true)
                    .show(ui, |ui| {
                        ui.label("Export format");
                        egui::ComboBox::from_id_salt("export_format_combo")
                            .selected_text(self.export_format.as_ref())
                            .show_ui(ui, |ui| {
                                for format in RibbleRecordingExportFormat::iter() {
                                    ui.selectable_value(
                                        &mut self.export_format,
                                        format,
                                        format.as_ref(),
                                    ).on_hover_ui(|ui| {
                                        ui.style_mut().interaction.selectable_labels = true;
                                        ui.label(format.tooltip());
                                    });
                                }
                            });
                        ui.end_row();
                    });

                // Clickable grid: take from the transcriber impl
                // NOTE: this is copy-paste kung-fu -> extract into a common function if the
                // implementation isn't too divergent.
                // ALSO: since the implementation is kung-fu copy-pasted, this will require
                // identical changes if this doesn't get factored out.
                egui::ScrollArea::both().show(ui, |ui| {
                    egui::Grid::new("recording recording_list_grid")
                        .num_columns(1)
                        .striped(true)
                        .show(ui, |ui| {
                            let len = self.recordings_buffer.len();
                            for (i, (file_name, recording)) in self.recordings_buffer.iter().enumerate() {
                                let heading_text = format!("Recording: {}", len - i);

                                // TODO: if this is expensive/not all that valuable, just do the duration.
                                // NOTE: this is identical to the Transcriber pane impl
                                // This should probably exist in a common method, but keep it separate if it diverges.
                                let body_text = {
                                    let secs = recording.total_duration().as_secs();
                                    let seconds = secs % 60;
                                    let minutes = (secs / 60) % 60;
                                    let hours = (secs / 60) / 60;

                                    // This is in bytes.
                                    let file_size_estimate = recording.file_size_estimate();
                                    let size_text = match unit_prefix::NumberPrefix::binary(file_size_estimate as f32) {
                                        unit_prefix::NumberPrefix::Standalone(number) => format!("{number:.0} B"),
                                        unit_prefix::NumberPrefix::Prefixed(prefix, number) => format!("{number:.2} {prefix}B"),
                                    };

                                    format!("Total time: {hours}:{minutes}:{seconds} | Approx size: {size_text}")
                                };


                                let tile_id = egui::Id::new(heading_text.as_str());

                                let resp = ui.interact(ui.max_rect(), tile_id, egui::Sense::click());
                                let visuals = ui.style().interact(&resp);

                                // TODO: TEST THIS OUT AND MAKE SURE THINGS WORK OUT
                                // THE GOAL: highlight color + OUTLINE

                                // NOTE: this is identical to the Transcriber pane impl
                                // This should probably exist in a common method, but keep it separate if it diverges.
                                egui::Frame::default().fill(visuals.bg_fill).stroke(visuals.fg_stroke).show(ui, |ui| {
                                    ui.vertical(|ui| {
                                        ui.label(heading_text);
                                        ui.small(body_text);
                                    });
                                });

                                if resp.clicked() {
                                    // NOTE: There isn't a debouncer in the recording folder right
                                    // now.
                                    //
                                    // Invalid paths should at least pop a toast, but perhaps things
                                    // need to move to a debouncer.
                                    if let Some(_) = controller.try_get_recording_path(Arc::clone(file_name)) {
                                        // File dialog to save

                                        if let Some(out_path) = rfd::FileDialog::new()
                                            .add_filter("wav", &["wav"])
                                            .set_directory(controller.base_dir())
                                            .save_file() {
                                            self.recording_modal = false;
                                            controller.export_recording(out_path, Arc::clone(file_name), self.export_format);
                                        }
                                    } else {
                                        // The writer engine will prune out its nonexistent file-paths,
                                        // so perhaps maybe a "toast" is sufficient here to say "sorry
                                        // cannot find recording"

                                        // Otherwise, a debouncer will be necessary to maintain the state
                                        // of the directory.
                                        log::warn!("Temporary recording file missing: {file_name}");
                                        let toast = egui_notify::Toast::warning("Failed to find saved recording.");
                                        controller.send_toast(toast);
                                    }
                                }
                                ui.end_row();
                            }
                        });
                })
            });

            if modal.should_close() {
                self.recording_modal = false;
            }
        }


        // Add a context menu to make this closable -> NOTE: if the pane should not be closed, this
        // will just nop.
        resp.context_menu(|ui| {
            ui.selectable_value(should_close, self.is_pane_closable(), "Close tab");
        });

        resp
    }

    fn is_pane_closable(&self) -> bool {
        self.pane_id().is_closable()
    }
}

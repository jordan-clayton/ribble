use std::error::Error;
use std::path::Path;
use std::thread::JoinHandle;

use slab::Slab;

use crate::controller::audio_backend_proxy::{
    AudioBackendProxy, AudioCaptureRequest, SharedSdl2Capture,
};
use crate::controller::ribble_controller::RibbleController;
use crate::controller::{
    AmortizedDownloadProgress, AmortizedProgress, LatestError, UI_UPDATE_QUEUE_SIZE,
};
use crate::ui::panes::RibbleTree;
use crate::ui::panes::ribble_pane::{ClosableRibbleViewPane, RibblePaneId};
use crate::ui::widgets::pie_progress::pie_progress;
use crate::ui::widgets::recording_icon::recording_icon;
use crate::utils::errors::{RibbleError, RibbleErrorCategory};
use crate::utils::migration::{RibbleVersion, Version};
use crate::utils::preferences::RibbleAppTheme;
use eframe::Storage;
use eframe::glow::Context;
use egui_notify::{Toast, Toasts};
use egui_theme_lerp::ThemeAnimator;
use irox_egui_extras::progressbar::ProgressBar;
use ribble_whisper::audio::audio_backend::{
    AudioBackend, CaptureSpec, Sdl2Backend, default_backend,
};
use ribble_whisper::audio::microphone::{MicCapture, Sdl2Capture};
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::sdl2;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{Receiver, get_channel};
use std::sync::Arc;
use strum::IntoEnumIterator;

// Icon constants
const HAMBURGER: &str = "☰";
const NO_DOWNLOADS: &str = "📥";
const ERROR_ICON: &str = "❎";
const TOOLTIP_GRACE_TIME: f32 = 0.0;
const TOOLTIP_DELAY: f32 = 0.5;

// This is in seconds
const THEME_TRANSITION_TIME: f32 = 0.3;
const RECORDING_ICON_FLICKER_SPEED: f32 = 1.0;

const TOP_BAR_HEIGHT_COEFF: f32 = 1.5;
const BOTTOM_PROGRESS_COLUMN_RATIO: f32 = 0.9;

// NOTE: If this works for everything in the app, move it to a common place (mod) or make it public.
const TOP_BAR_BUTTON_SIZE: f32 = 20.0;

// TODO: keyboard shortcuts?

// NOTE: it might be the case that the local cache dir does need to come back. Not sure yet.
pub struct Ribble {
    version: RibbleVersion,
    tree: RibbleTree,
    _sdl: sdl2::Sdl,
    backend: Sdl2Backend,
    // This needs to be polled in the UI loop to handle
    capture_requests: Receiver<AudioCaptureRequest>,
    toasts_handle: Toasts,
    toasts_receiver: Receiver<Toast>,
    current_devices: Slab<Arc<Sdl2Capture<ArcChannelSink<f32>>>>,
    controller: RibbleController,

    theme_animator: ThemeAnimator,

    // NOTE: this logs internally on failure to serialize.
    periodic_serialize: Option<JoinHandle<()>>,

    cached_downloads_progress: AmortizedDownloadProgress,
    cached_progress: AmortizedProgress,
    #[cfg(debug_assertions)]
    debug_download_id: Option<usize>,
}

impl Ribble {
    pub(crate) fn new(
        version: RibbleVersion,
        data_directory: &Path,
        cc: &eframe::CreationContext<'_>,
    ) -> Result<Self, RibbleError> {
        // Pack these in the app struct so they live on the main thread.
        let (sdl_ctx, backend) = default_backend()?;

        // This channel allows the kernel to request a mic capture from SDL.
        let (request_sender, request_receiver) = get_channel(1);

        // This channel is for sending toasts between the GUI pane views and the main ctx via the controller.

        let (toasts_sender, toasts_receiver) = get_channel(UI_UPDATE_QUEUE_SIZE);
        // Make a new "Toasts" to initialize

        let toasts_handle = Toasts::default();

        // Send this to the kernel
        let backend_proxy = AudioBackendProxy::new(request_sender);
        // Deserialize/default construct the controller.
        let controller = RibbleController::new(data_directory, backend_proxy, toasts_sender)?;

        // Deserialize/default construct the app tree -> this has its own default layout.
        let tree = RibbleTree::new(data_directory, controller.clone());

        // Get the system visuals stored in user_prefs
        let system_visuals = match controller.read_system_visuals() {
            Some(visuals) => visuals,
            // None => "System" theme, extract the information from the creation context.
            // The default ThemePreference is ThemePreference::System (macOS, Windows),
            // So this will return Some(theme) for those platforms, None for Linux (default to Dark)
            None => Self::get_system_visuals(&cc.egui_ctx),
        };

        let theme_animator = ThemeAnimator::new(system_visuals.clone(), system_visuals.clone())
            .animation_time(THEME_TRANSITION_TIME);

        // -Set- the actual system visuals so that they are persisted.
        cc.egui_ctx.set_visuals(system_visuals);

        let current_devices = Slab::new();

        Ok(Self {
            version,
            tree,
            _sdl: sdl_ctx,
            backend,
            capture_requests: request_receiver,
            toasts_handle,
            toasts_receiver,
            current_devices,
            controller,
            theme_animator,
            periodic_serialize: None,
            // Since the data is guarded by sync locks, these need to be cached in the UI,
            // Or accept that some blocking might be necessary to get the read lock.
            cached_downloads_progress: AmortizedDownloadProgress::NoJobs,
            cached_progress: AmortizedProgress::NoJobs,
            #[cfg(debug_assertions)]
            debug_download_id: None,
        })
    }

    fn open_audio_device(
        &mut self,
        spec: CaptureSpec,
        sink: ArcChannelSink<f32>,
    ) -> Result<SharedSdl2Capture<ArcChannelSink<f32>>, RibbleWhisperError> {
        // Try to open capture
        // Give ownership to the Arc temporarily
        // -This is technically a major "warn", but there are mechanisms in place to ensure that
        // the device is only dropped on the main thread.
        let device = Arc::new(self.backend.open_capture(spec, sink)?);

        // Clone a reference to consume for the shared capture
        let shared_device = Arc::clone(&device);

        // Place it in the slab and get a device_id
        let device_id = self.current_devices.insert(device);

        let shared_capture = SharedSdl2Capture::new(device_id, shared_device);
        Ok(shared_capture)
    }

    // Until it's absolutely certain that this implementation works as intended,
    // this function is going to panic to ensure the device is always cleaned up on the main
    // thread.
    fn try_close_audio_device(&mut self, device_id: usize) {
        // This will panic if the device is not in the slab.
        // Use try_remove in case the device has already been removed.
        // This shouldn't ever happen--but this needs to be tested first.
        if let Some(shared_device) = self.current_devices.try_remove(device_id) {
            Self::consume_audio_device(shared_device)
        } else {
            log::warn!("Device id missing from opened device buffer.");
        }
    }

    fn consume_audio_device(device: Arc<Sdl2Capture<ArcChannelSink<f32>>>) {
        let _strong_count = Arc::strong_count(&device);

        // This will consume the inner from the Arc and leave the pointer empty.
        // It only returns Some(..) when the refcount is exactly 1
        let device = Arc::into_inner(device);

        assert!(
            device.is_some(),
            "Strong count > 1 when trying to close audio device. Count: {_strong_count}"
        );
        // The device will automatically be dropped by the end of this function.
    }

    fn get_system_visuals(ctx: &egui::Context) -> egui::Visuals {
        ctx.system_theme()
            .unwrap_or(egui::Theme::Dark)
            .default_visuals()
    }

    fn check_join_last_save(&mut self) {
        if let Some(handle) = self.periodic_serialize.take()
            && let Err(e) = handle.join()
        {
            log::error!("Error serializing app state: {e:#?}");
        }
    }

    fn serialize_app_state(&mut self) {
        self.check_join_last_save();
        let controller = self.controller.clone();

        // TODO: This seems to create more problems than it solves.
        // It might be better to allow the panes to close and let the user retrieve the pane view as
        // needed.
        //
        // Run a pass over the tree to make sure all non-closable panes are still in view.
        self.tree.check_insert_non_closable_panes();

        // NOTE: this is a proxy object that avoids cloning the entire tree/behavior.
        // It's not as cheap as cloning the controller and uses CoW.
        let tree_serializer = self.tree.tree_serializer();

        let worker = std::thread::spawn(move || {
            controller.serialize_user_data();
            tree_serializer.serialize();
        });

        self.periodic_serialize = Some(worker)
    }
}

impl Drop for Ribble {
    fn drop(&mut self) {
        log::info!("Dropping Ribble App, joining egui save thread.");
        self.check_join_last_save();
        log::info!("Egui save thread joined.");
    }
}

impl eframe::App for Ribble {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check requests for an audio handle and produce an AudioDevice for capture.
        while let Ok(request) = self.capture_requests.try_recv() {
            match request {
                AudioCaptureRequest::Open(spec, sink, sender) => {
                    let shared_capture = self.open_audio_device(spec, sink);

                    // If there's a problem with communicating to send a handle to the requesting thread,
                    // treat this as an error and close the app after logging.
                    if let Err(e) = sender.try_send(shared_capture) {
                        log::error!(
                            "Cannot return audio device to requesting thread.\n\
                        Error source: {:#?}",
                            e.source()
                        );
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                AudioCaptureRequest::Close(device_id) => {
                    self.try_close_audio_device(device_id);
                }
            }
        }

        // Grab any new toasts that haven't been drawn.
        while let Ok(toast) = self.toasts_receiver.try_recv() {
            self.toasts_handle.add(toast);
        }

        // Set the system theme.
        let system_theme = match self.controller.read_system_visuals() {
            None => Self::get_system_visuals(ctx),
            Some(visuals) => visuals,
        };

        // Check to see if the system theme has been changed (via user preferences).
        // If this should start the transition animation, swap the themes.
        let start_transition = if system_theme != self.theme_animator.theme_2 {
            // If the transition is already going on (or has completed), swap theme 2 into theme
            // 1 and set theme 2 to the new theme.
            if self.theme_animator.progress <= 1.0 {
                self.theme_animator.theme_1 = self.theme_animator.theme_2.clone();
            }
            self.theme_animator.theme_2 = system_theme;
            self.theme_animator.theme_1_to_2 = true;
            true
        } else {
            false
        };

        // Set the GUI constants.
        ctx.style_mut(|style| {
            style.interaction.show_tooltips_only_when_still = true;
            style.interaction.tooltip_grace_time = TOOLTIP_GRACE_TIME;
            style.interaction.tooltip_delay = TOOLTIP_DELAY;
        });

        egui::TopBottomPanel::top("top_panel")
            .resizable(false)
            .min_height(0.0)
            .show(ctx, |ui| {
                ui.columns_const(|[col1, col2]| {
                    // Recording icon + status message
                    col1.vertical_centered_justified(|ui| {
                        // This code needs to be duplicated or be a tuple-closure
                        // -> The calculation needs to be relative to the columns.
                        let header_height = ui.spacing().interact_size.y * TOP_BAR_HEIGHT_COEFF;
                        let header_width = ui.max_rect().width();
                        // Allocate a top "toolbar"-sized toolbar.
                        let desired_size = egui::Vec2::new(header_width, header_height);
                        let layout = egui::Layout::left_to_right(egui::Align::Center);
                        ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                            let real_time = self.controller.realtime_running();
                            let offline = self.controller.offline_running();
                            let recording = self.controller.recorder_running();
                            let idle = !(real_time | offline | recording);

                            // This maps the visuals to a catppuccin theme to make it easier
                            // to get Red-Green-Yellow that "mostly" matches with the user's selected theme.
                            let theme = match self.controller.read_app_theme() {
                                None => {
                                    match ui.ctx().system_theme().unwrap_or(egui::Theme::Dark) {
                                        egui::Theme::Dark => RibbleAppTheme::Mocha
                                            .app_theme()
                                            .expect("This theme has 1:1 mapping with catppuccin."),
                                        egui::Theme::Light => RibbleAppTheme::Latte
                                            .app_theme()
                                            .expect("This theme has 1:1 mapping with catppuccin."),
                                    }
                                }
                                Some(theme) => theme,
                            };

                            let (color, msg, animate) = if idle {
                                (theme.green, "Ready...", false)
                            } else if offline {
                                (theme.yellow, "Transcribing audio file...", true)
                            } else {
                                let device_running = !self.current_devices.is_empty();
                                let msg = if device_running {
                                    "Recording..."
                                } else {
                                    "Preparing to record..."
                                };
                                (theme.red, msg, device_running)
                            };
                            ui.add(recording_icon(
                                color.into(),
                                animate,
                                RECORDING_ICON_FLICKER_SPEED,
                            ));
                            ui.monospace(msg);
                        });
                    });
                    // Control buttons.
                    col2.vertical_centered_justified(|ui| {
                        let header_height = ui.spacing().interact_size.y * TOP_BAR_HEIGHT_COEFF;
                        let header_width = ui.max_rect().width();
                        // Allocate a top "toolbar"-sized toolbar.
                        let desired_size = egui::Vec2::new(header_width, header_height);
                        let layout = egui::Layout::right_to_left(egui::Align::Center);

                        // Allocate a top "toolbar"-sized toolbar.
                        ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                            // UH, this does not seem to be respected by egui 0.32.0
                            // Until this bug gets resolved, this needs to use a richtext object instead
                            // NOTE: The fix for this is coming in egui 0.32.1

                            // This -should- hopefully be restored at some point
                            // ui.style_mut().text_styles.insert(
                            //     egui::TextStyle::Button,
                            //     egui::FontId::new(TOP_BAR_BUTTON_SIZE, eframe::epaint::FontFamily::Proportional),
                            // );

                            // NOTE NOTE NOTE: if memory allocation churn becomes an issue, this is low-hanging fruit to cache
                            // (or just set it back when the error is fixed).

                            let settings_button =
                                egui::RichText::new(HAMBURGER).size(TOP_BAR_BUTTON_SIZE);

                            // Far right hamburger button.
                            ui.menu_button(settings_button, |ui| {
                                ui.menu_button("Window", |ui| {
                                    for pane in ClosableRibbleViewPane::iter().filter(|p| {
                                        !matches!(*p, ClosableRibbleViewPane::UserPreferences)
                                    }) {
                                        if ui.button(pane.as_ref()).clicked() {
                                            self.tree.add_new_pane(pane.into());
                                            ui.ctx().request_repaint();
                                        }
                                    }
                                });
                                if ui.button("Settings").clicked() {
                                    self.tree.add_new_pane(RibblePaneId::UserPreferences);
                                    ui.ctx().request_repaint();
                                }

                                // NOTE: To avoid allocating every frame, this is more of a "try to recover"
                                // This shouldn't ever appear since the empty tree is caught in the ui loop.
                                if self.tree.is_invalid() {
                                    ui.separator();
                                    if ui.button("Restore Layout").clicked() {
                                        if !self.tree.recovery_tree_exists() {
                                            let toast = Toast::warning("Layout file missing!");
                                            self.controller.send_toast(toast)
                                        }
                                        // This will try to deserialize the layout and check to make sure it
                                        // contains a copy of non-closable tabs.
                                        // It will fall back to the default layout on a total failure.
                                        self.tree.try_recover_layout();
                                    }
                                }

                                ui.separator();
                                if ui.button("Reset layout").clicked() {
                                    self.tree.reset_layout();
                                    ui.ctx().request_repaint();
                                }

                                ui.separator();

                                if ui.button("Quit").clicked() {
                                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                                }
                                #[cfg(debug_assertions)]
                                {
                                    ui.separator();
                                    ui.menu_button("Debug menu", |ui| {
                                        // This mightn't be necessary.
                                        // Recording works and doesn't induce a lot of overhead;
                                        // Running file transcription will produce testable conditions.
                                        if ui.button("Test Progress").clicked() {
                                            todo!("Test Progress");
                                        }
                                        if ui.button("Test Download").clicked() {
                                            if let Some(id) = self.debug_download_id.take() {
                                                self.controller.remove_debug_download(id);
                                            }
                                            self.debug_download_id =
                                                Some(self.controller.add_debug_download());
                                        }

                                        if ui.button("Test Indeterminate Download").clicked() {
                                            if let Some(id) = self.debug_download_id.take() {
                                                self.controller.remove_debug_download(id);
                                            }
                                            self.debug_download_id = Some(
                                                self.controller.add_debug_indeterminate_download(),
                                            );
                                        }

                                        // This mightn't be necessary.
                                        // The application is in a mostly-working state,
                                        // including the console so this doesn't really need to be implemented.
                                        if ui.button("Test Console").clicked() {
                                            todo!("Test Console");
                                        }

                                        // For testing "Latest Error".
                                        if ui.button("Add Placeholder Error").clicked() {
                                            self.controller.add_placeholder_error()
                                        }
                                        if ui.button("Clear Latest Error").clicked() {
                                            self.controller.clear_latest_error()
                                        }

                                        // For fuzzing the layout/tree and inducing fallback mechanisms
                                        if ui.button("Induce panic").clicked() {
                                            panic!("Panic triggered!");
                                        }

                                        // For testing the segfault handler.
                                        if ui.button("Induce segfault").clicked() {
                                            unsafe {
                                                std::ptr::null_mut::<i32>().write(42);
                                            }
                                        }

                                        // For testing tree fallback mechanisms on an empty tree.
                                        if ui.button("Clear Tree").clicked() {
                                            self.tree.clear_tree();
                                        }

                                        if ui.button("Test Tree Recovery").clicked() {
                                            self.tree.test_tree_recovery();
                                        }

                                        // For fuzzing the egui layout algorithm to induce
                                        // conditions that can cause an invalid egui_tiles::Tree.
                                        if ui.button("Crash egui, lose tree").clicked() {
                                            egui::Grid::new("sadness").num_columns(2).show(
                                                ui,
                                                |ui| {
                                                    ui.add_space(1.0);
                                                },
                                            );
                                        }
                                    });
                                }
                            });

                            // Downloads widget.
                            if let Some(downloads) =
                                self.controller.try_get_amortized_download_progress()
                            {
                                self.cached_downloads_progress = downloads;
                            }

                            let download_button = match self.cached_downloads_progress {
                                AmortizedDownloadProgress::NoJobs => {
                                    let no_downloads_button =
                                        egui::RichText::new(NO_DOWNLOADS).size(TOP_BAR_BUTTON_SIZE);
                                    ui.add(egui::Button::selectable(false, no_downloads_button))
                                }
                                AmortizedDownloadProgress::Total {
                                    current,
                                    total_size,
                                } => {
                                    let resp =
                                        ui.add(pie_progress(current as f32, total_size as f32));
                                    ui.ctx().request_repaint();
                                    resp
                                }
                            }
                            .on_hover_ui(|ui| {
                                ui.style_mut().interaction.selectable_labels = true;
                                ui.label("Show downloads");
                            });

                            if download_button.clicked() {
                                self.tree.add_new_pane(RibblePaneId::Downloads);
                                ui.ctx().request_repaint();
                            }
                        })
                    });
                });
            });

        egui::TopBottomPanel::bottom("bottom_panel")
            .min_height(0.0)
            .resizable(false)
            .show(ctx, |ui| {
                ui.columns_const(|[col1, col2, col3]| {
                    #[cfg(debug_assertions)]
                    {
                        col1.vertical_centered_justified(|ui| {
                            ui.horizontal(|ui| {
                                // FPS counter -> NOTE this is not mean frame time and is not smoothed out
                                // TODO: look at maybe implementing smoothing at some point.
                                // stable_dt is in seconds
                                let dt = ui.ctx().input(|i| i.stable_dt);
                                let dt_ms = dt * 1000.0;
                                let fps = 1.0 / dt;
                                ui.monospace(format!("FPS: {fps:.2}"));
                                ui.monospace(format!("Frame time: {dt_ms:.2} ms"));
                            });
                        });
                    }

                    col2.vertical_centered_justified(|ui| {
                        // LATEST ERROR.
                        // TODO: this might work better as a method; consider refactoring later.
                        let mut error_ui_closure = |ui: &mut egui::Ui, error: &LatestError| {
                            // Try and do a single widget here.
                            let mut layout_job = egui::text::LayoutJob::default();
                            let font_id = egui::TextStyle::Monospace.resolve(ui.style());
                            layout_job.append(
                                ERROR_ICON,
                                0.0,
                                egui::TextFormat {
                                    font_id: font_id.clone(),
                                    color: ui.visuals().error_fg_color,
                                    ..Default::default()
                                },
                            );

                            layout_job.append(" ", 0.0, Default::default());

                            layout_job.append(
                                error.category().as_ref(),
                                0.0,
                                egui::TextFormat {
                                    font_id,
                                    ..Default::default()
                                },
                            );

                            if ui
                                .label(layout_job)
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.tree.add_new_pane(RibblePaneId::Console);
                                ui.ctx().request_repaint();
                            }
                        };

                        match self.controller.read_latest_error().as_ref() {
                            None => {
                                #[cfg(debug_assertions)]
                                {
                                    let dummy_error = LatestError::new(
                                        1,
                                        RibbleErrorCategory::ConversionError,
                                        std::time::Instant::now(),
                                    );
                                    error_ui_closure(ui, &dummy_error);
                                }
                            }
                            Some(error) => {
                                error_ui_closure(ui, error);
                            }
                        }
                    });

                    col3.vertical_centered_justified(|ui| {
                        let interact_size = ui.spacing().interact_size;
                        let desired_size = egui::vec2(ui.available_width(), interact_size.y);
                        let layout = egui::Layout::right_to_left(egui::Align::Center);

                        ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                            if let Some(progress) = self.controller.try_get_amortized_progress() {
                                self.cached_progress = progress;
                            }

                            // Print the ribble version
                            ui.monospace(self.version.semver_string());
                            // NOTE: this technically spills over into the middle column
                            // but expect this to never overlap
                            let desired_size = egui::vec2(
                                ui.available_width() * BOTTOM_PROGRESS_COLUMN_RATIO,
                                interact_size.y,
                            );

                            // NOTE: THIS IS NOT CORRECT YET.
                            match self.cached_progress {
                                AmortizedProgress::NoJobs => {
                                    ui.horizontal(|ui| {
                                        #[cfg(debug_assertions)]
                                        {
                                            let (rect, response) = ui.allocate_exact_size(
                                                desired_size,
                                                egui::Sense::click(),
                                            );
                                            if ui.is_rect_visible(rect) {
                                                let color = egui::Color32::from_rgb(255, 0, 0);

                                                ui.painter().rect_stroke(
                                                    rect,
                                                    0.0,
                                                    egui::Stroke::new(1.0, color),
                                                    egui::StrokeKind::Middle,
                                                );
                                            }
                                            if response
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                self.tree.add_new_pane(RibblePaneId::Progress);
                                            }
                                            ui.monospace("Working:");
                                        }
                                    });
                                }
                                AmortizedProgress::Determinate {
                                    current,
                                    total_size,
                                } => {
                                    ui.horizontal(|ui| {
                                        let progress = current as f32 / total_size as f32;
                                        debug_assert!(!progress.is_nan());

                                        let pb = ProgressBar::new(progress)
                                            .desired_width(desired_size.x)
                                            .desired_height(desired_size.y * 0.5);

                                        let (rect, resp) = ui.allocate_exact_size(
                                            desired_size,
                                            egui::Sense::click(),
                                        );

                                        if ui.is_rect_visible(rect) {
                                            ui.put(rect, pb);
                                        }

                                        // Paint a progress bar
                                        if resp
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            log::info!("Progress bar clicked");
                                            self.tree.add_new_pane(RibblePaneId::Progress);
                                        }

                                        ui.monospace("Working:");
                                        ui.ctx().request_repaint();
                                    });
                                }
                                AmortizedProgress::Indeterminate => {
                                    ui.horizontal(|ui| {
                                        let pb = ProgressBar::indeterminate()
                                            .desired_width(desired_size.x)
                                            .desired_height(desired_size.y * 0.5);

                                        let (rect, resp) = ui.allocate_exact_size(
                                            desired_size,
                                            egui::Sense::click(),
                                        );

                                        if ui.is_rect_visible(rect) {
                                            ui.put(rect, pb);
                                        }

                                        // Paint a progress bar
                                        if resp
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            log::info!("Progress bar clicked");
                                            self.tree.add_new_pane(RibblePaneId::Progress);
                                        }

                                        ui.monospace("Working:");
                                        ui.ctx().request_repaint();
                                    });
                                }
                            }
                        });
                    });
                });
            });

        let mut frame = egui::Frame::central_panel(ctx.style().as_ref());
        frame.inner_margin = egui::Margin::ZERO;

        egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
            if self.theme_animator.anim_id.is_none() {
                self.theme_animator.create_id(ui);
            } else {
                // This implicitly set s the visuals
                self.theme_animator.animate(ctx);
            }

            if start_transition {
                self.theme_animator.start();
            }
            self.tree.ui(ui);
        });

        // Show any toasts that might be in the buffer.
        self.toasts_handle.show(ctx);

        // If there's any sort of "work" being done (transcribing, recording)
        // then request a repaint -> the downloads/progress will already request repaints if they
        // are showing work.
        if self.controller.recorder_running() || self.controller.transcriber_running() {
            ctx.request_repaint();
        }
    }

    // This will automatically save egui memory (window position, etc.) upon opening the storage file
    // Instead of one gigantic point of failure, Ribble stores its state across multiple files in the data directory.
    fn save(&mut self, _storage: &mut dyn Storage) {
        self.serialize_app_state();
    }

    // Called after the last "save" (will save internally again on drop)
    // If the user closes the window while background threads are still running,
    // the program will deadlock.
    fn on_exit(&mut self, _gl: Option<&Context>) {
        log::info!("Starting runtime cleanup.");
        self.controller.stop_work();
        // WAIT until the last SDL device gets dropped on the main thread before dropping everything.
        // Check requests for an audio handle and produce an AudioDevice for capture.

        // It should never be possible for there to be more than one audio device,
        // and this loop will never service requests for a new audio device.

        // However, since this does block, use a short timeout to prevent the program hanging on close.
        while let Ok(request) = self
            .capture_requests
            .recv_timeout(std::time::Duration::from_millis(100))
        {
            if let AudioCaptureRequest::Close(device_id) = request {
                self.try_close_audio_device(device_id);
                if self.current_devices.is_empty() {
                    log::info!("Dropped last audio device.");
                    break;
                }
            }
        }

        // It is unlikely for this branch to be taken, but if it is, try and consume the remaining devices.
        if !self.current_devices.is_empty() {
            log::warn!("Audio devices still in use. Background work might be deadlocked.");
            for device in self.current_devices.drain() {
                device.pause();

                #[cfg(debug_assertions)]
                {
                    // This method will (intentionally) panic and should be used to tease out remaining
                    // logic problems/race conditions.

                    // If this ever happens to actually panic, a better solution is required to ensure SDL
                    // devices are always successfully dropped on the main thread.
                    // (i.e. some way to short-circuit the background workers)
                    Self::consume_audio_device(device);
                }
            }
            log::info!("Remaining audio devices successfully dropped.");
        }

        log::info!("Finished runtime cleanup.");
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }
}

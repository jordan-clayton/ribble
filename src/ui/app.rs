use std::error::Error;
use std::path::Path;
use std::thread::JoinHandle;

use slab::Slab;

use crate::controller::audio_backend_proxy::{
    AudioBackendProxy, AudioCaptureRequest, SharedSdl2Capture,
};
use crate::controller::ribble_controller::RibbleController;
use crate::controller::{AmortizedDownloadProgress, AmortizedProgress, UI_UPDATE_QUEUE_SIZE};
use crate::ui::panes::ribble_pane::{ClosableRibbleViewPane, RibblePaneId};
use crate::ui::panes::RibbleTree;
use crate::ui::widgets::pie_progress::pie_progress;
use crate::ui::widgets::recording_icon::recording_icon;
use crate::utils::errors::RibbleError;
use crate::utils::migration::{RibbleVersion, Version};
use crate::utils::preferences::RibbleAppTheme;
use eframe::Storage;
use egui_notify::{Toast, Toasts};
use egui_theme_lerp::ThemeAnimator;
use irox_egui_extras::progressbar::ProgressBar;
use ribble_whisper::audio::audio_backend::{
    default_backend, AudioBackend, CaptureSpec, Sdl2Backend,
};
use ribble_whisper::audio::microphone::Sdl2Capture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::sdl2;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{get_channel, Receiver};
use std::sync::Arc;
use strum::IntoEnumIterator;

// TODO: implement a debug menu from which to test some stuff out:
// e.g. Panics, progress, downloads, etc.

// TODO: FIND AN APPROPRIATE SPOT FOR GUI/STYLING CONSTANTS
// Icon constants
const HAMBURGER: &str = "☰";
const NO_DOWNLOADS: &str = "📥";
const TOOLTIP_GRACE_TIME: f32 = 0.0;
const TOOLTIP_DELAY: f32 = 0.5;

// This is in seconds
const THEME_TRANSITION_TIME: f32 = 0.3;

// Relative progress bar size.
const BOTTOM_PROGRESS_RATIO: f32 = 0.3;
const RECORDING_ICON_FLICKER_SPEED: f32 = 1.0;

const TOP_BAR_HEIGHT_COEFF: f32 = 1.5;

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
        let system_visuals = match controller.get_system_visuals() {
            Some(visuals) => visuals,
            // None => "System" theme, extract the information from the creation context.
            // The default ThemePreference is ThemePreference::System (macOS, Windows),
            // So this will return Some(theme) for those platforms, None for Linux (default to Dark)
            None => Self::get_system_visuals(&cc.egui_ctx),
        };

        let theme_animator = ThemeAnimator::new(system_visuals.clone(), system_visuals.clone())
            .animation_time(THEME_TRANSITION_TIME);

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
    fn close_audio_device(&mut self, device_id: usize) {
        // This will panic if the device is not in the slab.
        let shared_device = self.current_devices.remove(device_id);
        let _strong_count = Arc::strong_count(&shared_device);

        // This will consume the inner from the Arc and leave the pointer empty.
        // It only returns Some(..) when the refcount is exactly 1
        let device = Arc::into_inner(shared_device);

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
        if let Some(handle) = self.periodic_serialize.take() {
            if let Err(e) = handle.join() {
                log::error!("Error serializing app state: {e:#?}");
            }
        }
    }

    // TODO: DETERMINE WHETHER OR NOT TO LET egui DO THIS, OR IMPLEMENT DIRTY WRITES.
    fn serialize_app_state(&mut self) {
        self.check_join_last_save();
        let controller = self.controller.clone();

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
        log::info!("Dropping Ribble App; joining/running Ribble save.");
        self.check_join_last_save();
        log::info!("Final app save called.");
        // NOTE: the kernel and the RibbleTree both serialize on drop.
        // This is just to join the last eframe::App save() call.
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
                        log::error!("Cannot return audio device to requesting thread.\n\
                        Error source: {:#?}", e.source());
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                AudioCaptureRequest::Close(device_id) => {
                    self.close_audio_device(device_id);
                }
            }
        }

        // Grab any new toasts that haven't been drawn.
        while let Ok(toast) = self.toasts_receiver.try_recv() {
            self.toasts_handle.add(toast);
        }

        // Set the system theme.
        let system_theme = match self.controller.get_system_visuals() {
            None => Self::get_system_visuals(ctx),
            Some(visuals) => visuals,
        };

        // Check to see if the system theme has been changed (via user preferences).
        // If this should start the transition animation, swap the themes.
        let start_transition = if system_theme != self.theme_animator.theme_2 {
            // If the old transition completed, swap the themes.
            // Otherwise, the in-progress transition will just change its destination theme.
            // TODO: this might look janky? Test to see whether this should just change the themes anyway.
            if self.theme_animator.progress == 1.0 {
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
                ui.horizontal(|ui| {
                    ui.columns_const(|[col1, col2]| {
                        // Recording icon + status message
                        col1.vertical_centered_justified(|ui| {
                            // This code needs to be duplicated or be a tuple-closure
                            // -> The calculation needs to be relative to the columns.
                            let header_height = ui.spacing().interact_size.y * TOP_BAR_HEIGHT_COEFF;
                            let header_width = ui.max_rect().width();
                            // Allocate a top "toolbar"-sized toolbar.
                            let desired_size = egui::Vec2::new(header_width, header_height);
                            let layout =
                                egui::Layout::left_to_right(egui::Align::Center);
                            ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                let real_time = self.controller.realtime_running();
                                let offline = self.controller.offline_running();
                                let recording = self.controller.recorder_running();
                                let idle = !(real_time | offline | recording);

                                // This maps the visuals to a catppuccin theme to make it easier
                                // to get Red-Green-Yellow that "mostly" matches with the user's selected theme.
                                let theme = match self.controller.get_app_theme() {
                                    None => {
                                        match ui
                                            .ctx()
                                            .system_theme()
                                            .unwrap_or(egui::Theme::Dark)
                                        {
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
                                ui.add(recording_icon(color.into(), animate, RECORDING_ICON_FLICKER_SPEED));
                                ui.monospace(msg);
                            });
                        });
                        // Control buttons.
                        col2.vertical_centered_justified(|ui| {
                            let header_height = ui.spacing().interact_size.y * TOP_BAR_HEIGHT_COEFF;
                            let header_width = ui.max_rect().width();
                            // Allocate a top "toolbar"-sized toolbar.
                            let desired_size = egui::Vec2::new(header_width, header_height);
                            let layout =
                                egui::Layout::right_to_left(egui::Align::Center);

                            // Allocate a top "toolbar"-sized toolbar.
                            ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                                // UH, this does not seem to be respected by egui 0.32.0
                                // Until this bug gets resolved, this needs to use a richtext object instead

                                // This -should- hopefully be restored at some point
                                // ui.style_mut().text_styles.insert(
                                //     egui::TextStyle::Button,
                                //     egui::FontId::new(TOP_BAR_BUTTON_SIZE, eframe::epaint::FontFamily::Proportional),
                                // );

                                // NOTE NOTE NOTE: if memory allocation churn becomes an issue, this is low-hanging fruit to cache
                                // (or just set it back when the error is fixed).

                                let settings_button = egui::RichText::new(HAMBURGER).size(TOP_BAR_BUTTON_SIZE);

                                // Far right hamburger button.
                                ui.menu_button(settings_button, |ui| {
                                    ui.menu_button("Window", |ui| {
                                        for pane in ClosableRibbleViewPane::iter()
                                            .filter(|p| !matches!(*p, ClosableRibbleViewPane::UserPreferences)) {
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

                                    ui.separator();

                                    if ui.button("Quit").clicked() {
                                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
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
                                        let no_downloads_button = egui::RichText::new(NO_DOWNLOADS).size(TOP_BAR_BUTTON_SIZE);
                                        ui.add(egui::Button::selectable(false, no_downloads_button))
                                    }
                                    AmortizedDownloadProgress::Total {
                                        current,
                                        total_size,
                                    } => {
                                        // TODO: this needs to be tested -> It is not known whether or not the drawing behaves as written.
                                        let resp = ui.add(pie_progress(current as f32, total_size as f32));
                                        ui.ctx().request_repaint();
                                        resp
                                    }
                                }.on_hover_ui(|ui| {
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
            });

        egui::TopBottomPanel::bottom("bottom_panel")
            .min_height(0.0)
            .resizable(false)
            .show(ctx, |ui| {
                let interact_size = ui.spacing().interact_size;
                // Allocate a bottom "toolbar"-sized toolbar.
                let desired_size = egui::vec2(ui.max_rect().width(), interact_size.y);
                let layout = egui::Layout::right_to_left(egui::Align::Center);

                ui.allocate_ui_with_layout(desired_size, layout, |ui| {
                    if let Some(progress) = self.controller.try_get_amortized_progress() {
                        self.cached_progress = progress;
                    }

                    // Print the ribble version
                    ui.monospace(self.version.semver_string());

                    match self.cached_progress {
                        AmortizedProgress::NoJobs => {
                            ui.horizontal(|ui| {
                                #[cfg(debug_assertions)]
                                {
                                    let rect = ui.max_rect();
                                    let desired_size = egui::vec2(rect.width() * BOTTOM_PROGRESS_RATIO, interact_size.y);
                                    let (rect, response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
                                    if ui.is_rect_visible(rect) {
                                        let color = egui::Color32::from_rgb(255, 0, 0);

                                        ui.painter().rect_stroke(rect, 0.0, egui::Stroke::new(1.0, color), egui::StrokeKind::Middle);
                                    }
                                    if response.clicked() {
                                        self.tree.add_new_pane(RibblePaneId::Progress);
                                    }

                                    ui.monospace("Debug:");
                                    // FPS counter -> NOTE this is not mean frame time and is not smoothed out
                                    // TODO: look at maybe implementing smoothing at some point.
                                    // stable_dt is in seconds
                                    let dt = ui.ctx().input(|i| { i.stable_dt });
                                    let dt_ms = dt * 1000.0;
                                    let fps = 1.0 / dt;
                                    ui.monospace(format!("FPS: {fps:.2}"));
                                    ui.monospace(format!("Frame time: {dt_ms:.2} ms"));
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
                                let rect = ui.max_rect();
                                let pb = ProgressBar::new(progress)
                                    .desired_width(rect.width() * BOTTOM_PROGRESS_RATIO)
                                    .desired_height(rect.height());
                                // Paint a progress bar
                                if ui.add(pb).clicked() {
                                    self.tree.add_new_pane(RibblePaneId::Progress);
                                }
                                ui.monospace("Working:");
                                ui.ctx().request_repaint();
                            });
                        }
                        AmortizedProgress::Indeterminate => {
                            ui.horizontal(|ui| {
                                let rect = ui.max_rect();
                                let pb = ProgressBar::indeterminate()
                                    .desired_width(rect.width() * BOTTOM_PROGRESS_RATIO)
                                    .desired_height(rect.height())
                                    .text("Working".to_string());

                                // Paint an indeterminate progress bar
                                if ui.add(pb).clicked() {
                                    self.tree.add_new_pane(RibblePaneId::Progress);
                                }
                                ui.monospace("Working:");
                                ui.ctx().request_repaint();
                            });
                        }
                    }
                });
            });

        // Remove any and all margins -> these will (are?) handled by the panes themselves
        // TODO: update this comment once that's implemented.
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
            // TODO: RE: egui_tiles --
            // The panel split doesn't have a lot of contrast and is difficult to see
            // especially on dark backgrounds.
            // This needs to be addressed: consider using some sort of extreme fg color or
            // the outline color
            self.tree.ui(ui);
        });

        // Show any toasts that might be in the buffer.
        self.toasts_handle.show(ctx);
    }

    // TODO: determine whether to actually use this method at all,
    // or whether to just spawn a separate thread and periodically run the save method.
    // It'll get a little bit spicy on close, seeing as this also gets called on shutdown,
    // And each individual resource also serializes itself on shutdown.

    // This is causing some weird issues -> it's failing to initialize the Storage
    // I'm -not- quite sure how to approach replicating this just yet ->
    // This app is non-send/sync, so it's a little tricky to do the threading -> look into this.
    fn save(&mut self, _storage: &mut dyn Storage) {
        self.serialize_app_state();
    }

    // TODO: determine whether or not to just periodically run serialization on the background thread itself and join on drop.
    // Would be easier; I'm not using egui's persistence and the tree saves itself.
    fn persist_egui_memory(&self) -> bool {
        true
    }
}

// This is a fix to deal with surface0 being used for both widgets
// and faint_bg_color. Sliders and checkboxes get lost when using
// striped layouts.
// NOTE: DON'T DELETE THIS JUST YET ->
// This color issue may still be an issue that hasn't been resolved in catppuccin_egui;
//
// fn tweak_visuals(visuals: &mut Visuals, theme: Theme) {
//     visuals.faint_bg_color = theme.mantle
// }

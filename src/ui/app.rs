use std::path::Path;
use std::thread::JoinHandle;

use slab::Slab;

use crate::controller::audio_backend_proxy::{
    AudioBackendProxy, AudioCaptureRequest, SharedSdl2Capture,
};
use crate::utils::errors::RibbleError;
use eframe::Storage;
use egui_theme_lerp::ThemeAnimator;
use ribble_whisper::audio::audio_backend::{
    AudioBackend, CaptureSpec, Sdl2Backend, default_backend,
};
use ribble_whisper::audio::microphone::Sdl2Capture;
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::utils::errors::RibbleWhisperError;
use ribble_whisper::utils::{Receiver, get_channel};
use std::sync::Arc;

use crate::controller::UI_UPDATE_QUEUE_SIZE;
use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::RibbleTree;
use egui_notify::{Toast, Toasts};

// NOTE: it might be the case that the local cache dir does need to come back. Not sure yet.
pub struct Ribble {
    tree: RibbleTree,
    sdl: sdl2::Sdl,
    backend: Sdl2Backend,
    // This needs to be polled in the UI loop to handle
    capture_requests: Receiver<AudioCaptureRequest>,
    toasts_handle: Toasts,
    toasts_receiver: Receiver<Toast>,
    current_devices: Slab<Arc<Sdl2Capture<ArcChannelSink<f32>>>>,
    controller: RibbleController,

    theme_animator: ThemeAnimator,

    // TODO: if only logging, remove the result.
    periodic_serialize: Option<JoinHandle<Result<(), RibbleError>>>,
}

impl Ribble {
    // TODO: FIND AN APPROPRIATE SPOT FOR GUI/STYLING CONSTANTS
    const TOOLTIP_GRACE_TIME: f32 = 0.0;
    const TOOLTIP_DELAY: f32 = 0.5;

    // This is in seconds
    const THEME_TRANSITION_TIME: f32 = 0.3;

    pub(crate) fn new(
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
        let tree = RibbleTree::new(data_directory, controller.clone())?;

        // Get the system visuals stored in user_prefs
        let system_visuals = match controller.get_system_visuals() {
            Some(visuals) => visuals,
            // None => "System" theme, extract the information from the creation context.
            // The default ThemePreference is ThemePreference::System (macOS, Windows),
            // So this will return Some(theme) for those platforms, None for Linux (default to Dark)
            None => Self::get_system_visuals(&cc.egui_ctx),
        };

        let theme_animator = ThemeAnimator::new(system_visuals.clone(), system_visuals.clone())
            .animation_time(Self::THEME_TRANSITION_TIME);

        let current_devices = Slab::new();

        Ok(Self {
            tree,
            sdl: sdl_ctx,
            backend,
            capture_requests: request_receiver,
            toasts_handle,
            toasts_receiver,
            current_devices,
            controller,
            theme_animator,
            periodic_serialize: None,
        })
    }

    fn open_audio_device(
        &mut self,
        spec: CaptureSpec,
        sink: ArcChannelSink<f32>,
    ) -> Result<SharedSdl2Capture<ArcChannelSink<f32>>, RibbleWhisperError> {
        // Try to open capture
        // Give ownership to the Arc temporarily
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
        // This is mainly for debugging purposes
        let strong_count = Arc::strong_count(&shared_device);

        // This will consume the inner from the Arc and leave the pointer empty.
        // It only returns Some(..) when the refcount is exactly 1
        let device = Arc::into_inner(shared_device);

        assert!(
            device.is_some(),
            "Strong count > 1 when trying to close audio device. Count: {strong_count}",
        );
        // The device will automatically be dropped by the end of this function.
    }

    fn get_system_visuals(ctx: &egui::Context) -> egui::Visuals {
        match ctx.system_theme() {
            None => egui::Theme::Dark,
            Some(theme) => theme,
        }
        .default_visuals()
    }

    fn check_join_last_save(&mut self) {
        if let Some(handle) = self.periodic_serialize.take() {
            // TODO: Add a way for the app to forward messages into the console engine.
            // OR: just log the error.
            if handle.join().is_err() {
                todo!("LOGGING");
            }
        }
    }

    // TODO: DETERMINE WHETHER OR NOT TO LET EGUI DO THIS, OR IMPLEMENT DIRTY WRITES.
    fn serialize_app_state(&mut self) {
        self.check_join_last_save();

        let controller = self.controller.clone();
        // NOTE: This tree clone is probably relatively expensive, but egui calls this on a background
        // thread anyway.
        //
        // It shouldn't really matter -> both items serialize on drop, so the app state will be
        // preserved when it's closed.
        let tree = self.tree.clone();

        let worker = std::thread::spawn(move || {
            // TODO: expect there to be a borrow issue; self cannot be shared across threads safely (technically) due to SDL,
            // so the internal references may not be allowed.
            controller.serialize_user_data();
            tree.serialize_tree();
            Ok(())
        });

        self.periodic_serialize = Some(worker)
    }
}

impl Drop for Ribble {
    fn drop(&mut self) {
        self.check_join_last_save();
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
                    if sender.send(shared_capture).is_err() {
                        // TODO: logging
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
            style.interaction.tooltip_grace_time = Self::TOOLTIP_GRACE_TIME;
            style.interaction.tooltip_delay = Self::TOOLTIP_DELAY;
        });

        // TODO: OTHER PANELS -> top info bar, bottom info bar.
        egui::CentralPanel::default().show(ctx, |ui| {
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
    }

    // TODO: determine whether to actually use this method at all,
    // or whether to just spawn a separate thread and periodically run the save method.
    // It'll get a little bit spicy on close, seeing as this also gets called on shutdown,
    // And each individual resource also serializes itself on shutdown.
    fn save(&mut self, _storage: &mut dyn Storage) {
        self.serialize_app_state();
    }

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

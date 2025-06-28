use std::collections::HashMap;
use std::thread::JoinHandle;

use catppuccin_egui::Theme;
use eframe::Storage;
use egui::{Event, Key, ViewportCommand, Visuals};
use egui_dock::{DockArea, DockState, NodeIndex, Style, SurfaceIndex, TabIndex};
use ribble_whisper::audio::recorder::ArcChannelSink;
use ribble_whisper::audio::audio_backend::{AudioBackend, Sdl2Backend, default_backend}
use crate::utils::audio_backend_proxy::{AudioCaptureRequest, AudioBackendProxy};
use crate::utils::errors::RibbleError;
use ribble_whisper::utils::{Sender, Receiver, get_channel};

use crate::controller::console::ConsoleMessage;
use crate::{
    controller::whisper_app_controller::WhisperAppController,
    ui::tabs::{
        controller_tabs::{realtime, recording, r#static},
        display_tabs::{console, progress, transcription, visualizer},
        tab_viewer,
        whisper_tab::{FocusTab, WhisperTab},
    },
    utils::{
        console_message::ConsoleMessageType,
        errors::WhisperAppError,
        file_mgmt::{load_app_state, save_app_state},
        preferences,
    },
};

pub struct Ribble {
    // TODO: rename tabs
    tree: DockState<WhisperTab>,
    // TODO: rewrite controller
    //controller: RibbleController,
    sdl: sdl2::Sdl,
    backend: Sdl2Backend,
    // This needs to be polled in the UI loop to handle
    capture_requests: Receiver<AudioCaptureRequest>,
    // TODO: background thread for saving, RAII
}

impl Ribble {
    // NOTE: this should really only take the system theme if it's... necessary?
    // I do not remember what the heck I was doing.
    pub fn new() -> Result<Self, RibbleError> {
        // Pack these in the app struct so they live on the main thread.
        let (sdl_ctx, backend) = default_backend()?;

        // This channel allows the kernel to request a mic capture from SDL.
        let (request_sender, request_receiver) = get_channel(1);

        // Send this to the kernel
        let backend_proxy = AudioBackendProxy::new(request_sender);


        todo!("App Constructor");
        // Deserialize the app tree
    }
}


impl eframe::App for Ribble {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        while let Ok(request) = self.capture_requests.try_recv() {
            request(&self.backend);
        }
        todo!("Finish draw loop.")
    }
}

// OLD IMPLEMENTATION -> REMOVE ONCE APP REWRITTEN
pub struct WhisperApp {
    tree: DockState<WhisperTab>,
    closed_tabs: HashMap<String, WhisperTab>,
    controller: WhisperAppController,
    last_save_join_handle: Option<JoinHandle<Result<(), WhisperAppError>>>,
}

impl WhisperApp {
    pub fn new(cc: &eframe::CreationContext<'_>, mut controller: WhisperAppController) -> Self {
        let system_theme = cc.integration_info.system_theme;
        controller.set_system_theme(system_theme);

        match load_app_state() {
            None => Self::default_layout(controller),
            Some(state) => {
                let (tree, closed_tabs) = state;
                Self {
                    tree,
                    closed_tabs,
                    controller,
                    last_save_join_handle: None,
                }
            }
        }
    }

    fn default_layout(controller: WhisperAppController) -> Self {
        let closed_tabs = HashMap::new();

        let td = WhisperTab::Transcription(transcription::TranscriptionTab::default());
        let rd = WhisperTab::Visualizer(visualizer::VisualizerTab::default());
        let pd = WhisperTab::Progress(progress::ProgressTab::default());
        let ed = WhisperTab::Console(console::ConsoleTab::default());
        let rc = WhisperTab::Realtime(realtime::RealtimeTab::default());
        let st = WhisperTab::Static(r#static::StaticTab::default());
        let rec = WhisperTab::Recording(recording::RecordingTab::default());
        let mut tree: DockState<WhisperTab> = DockState::new(vec![td, rd]);

        let surface = tree.main_surface_mut();

        let [top, _] = surface.split_below(NodeIndex::root(), 0.7, vec![pd, ed]);

        let [_, _] = surface.split_right(top, 0.55, vec![rc, st, rec]);

        Self {
            tree,
            closed_tabs,
            controller,
            last_save_join_handle: None,
        }
    }
}

impl eframe::App for WhisperApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // Close the app when it's in an invalid state.
        if self.controller.is_poisoned() {
            ctx.send_viewport_cmd(ViewportCommand::Close);
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            // Join the save thread if it's there & close the app.
            if let Some(join_handle) = self.last_save_join_handle.take() {
                join_handle.join().ok();
            }
        }

        let system_theme = frame.info().system_theme;
        self.controller.set_system_theme(system_theme.clone());

        let theme = preferences::get_app_theme(system_theme);

        catppuccin_egui::set_theme(ctx, theme);

        // Catppuccin uses the same color for faint_bg and inactive widgets.
        // This causes issues with striped layouts.
        let mut visuals = ctx.style().visuals.clone();
        tweak_visuals(&mut visuals, theme);
        ctx.set_visuals(visuals);

        // Repaint continuously when running a worker.
        if self.controller.is_working() {
            ctx.request_repaint();
        }

        // Tab focus.
        let mut focus_tabs: Vec<(SurfaceIndex, NodeIndex, TabIndex)> = vec![];
        let mut missing_tabs: Vec<String> = vec![];
        while let Ok(focus_tab) = self.controller.recv_focus_tab() {
            let mut found = false;
            let surfaces = self.tree.iter_surfaces();
            for (surface_index, surface) in surfaces.enumerate() {
                let tree = surface.node_tree();
                if tree.is_none() {
                    continue;
                }
                let tree = tree.unwrap();

                for (node_index, node) in tree.iter().enumerate() {
                    let tabs = node.tabs();
                    if tabs.is_none() {
                        continue;
                    }
                    let tabs = tabs.unwrap();
                    for (tab_index, tab) in tabs.iter().enumerate() {
                        if tab.matches(focus_tab) {
                            focus_tabs.push((
                                surface_index.into(),
                                node_index.into(),
                                tab_index.into(),
                            ));
                            found = true;
                        }
                    }
                }
            }

            if !found {
                missing_tabs.push(focus_tab.id())
            }
        }

        for location in focus_tabs {
            self.tree.set_active_tab(location);
        }

        for key in missing_tabs {
            let tab = self.closed_tabs.remove(&key);
            if let Some(t) = tab {
                self.tree.push_to_focused_leaf(t);
            }
        }

        let mut closed_tabs = self.closed_tabs.clone();
        let show_add = !closed_tabs.is_empty();
        let mut added_tabs = vec![];

        let n_open_tabs = self.tree.iter_all_tabs().count();

        let mut tab_viewer = tab_viewer::WhisperTabViewer::new(
            self.controller.clone(),
            &mut closed_tabs,
            &mut added_tabs,
            n_open_tabs,
        );

        DockArea::new(&mut self.tree)
            .style(Style::from_egui(ctx.style().as_ref()))
            // Quick-fix for tabs being non-recoverable if a window is closed.
            .show_window_close_buttons(false)
            .show_add_buttons(show_add)
            .show_add_popup(show_add)
            .show(ctx, &mut tab_viewer);

        self.closed_tabs = closed_tabs;

        added_tabs.drain(..).for_each(|(surface, node, tab)| {
            self.tree.set_focused_node_and_surface((surface, node));
            self.tree.push_to_focused_leaf(tab);
        });

        // Process keyboard events if the visualizer is in the focus tab.
        let focused_leaf = self.tree.find_active_focused();
        if let Some((_, tab)) = focused_leaf {
            if tab.matches(FocusTab::Visualizer) {
                let events = ctx.input(|i| i.events.clone());
                for event in events {
                    if let Event::Key {
                        key,
                        physical_key: _,
                        pressed,
                        repeat,
                        modifiers: _,
                    } = event
                    {
                        if !pressed {
                            continue;
                        }
                        if repeat {
                            continue;
                        }

                        match key {
                            Key::Space => {
                                self.controller.rotate_analysis_type(true);
                            }
                            Key::ArrowLeft => {
                                self.controller.rotate_analysis_type(false);
                            }
                            Key::ArrowRight => {
                                self.controller.rotate_analysis_type(true);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    // eframe persistence does not seem to be working in linux.
    // Atm, this will not write to disk regardless of flushing.
    // Uh,  WHAT? That's not true, afaik. TODO: look into this.
    fn save(&mut self, _storage: &mut dyn Storage) {
        if let Some(join_handle) = self.last_save_join_handle.take() {
            if let Some(result) = join_handle.join().ok() {
                if let Err(e) = result {
                    let msg = ConsoleMessage::new(
                        ConsoleMessageType::Error,
                        format!("{}", e.to_string()),
                    );
                    self.controller
                        .send_console_message(msg)
                        .expect("Console message channel should not be closed.");
                }
            };
        }

        let new_save_handle = save_app_state(&self.tree, &self.closed_tabs);
        self.last_save_join_handle = Some(new_save_handle);
    }

    fn persist_egui_memory(&self) -> bool {
        true
    }
}

// This is a fix to deal with surface0 being used for both widgets
// and faint_bg_color. Sliders and checkboxes get lost when using
// striped layouts.
fn tweak_visuals(visuals: &mut Visuals, theme: Theme) {
    visuals.faint_bg_color = theme.mantle
}

mod recording_pane;
pub(in crate::ui) mod ribble_pane;
mod transcriber_pane;

mod console_pane;
mod downloads_pane;
pub(in crate::ui) mod panes;
mod progress_pane;
mod transcription_pane;
mod user_preferences_pane;
mod visualizer_pane;

use crate::controller::ribble_controller::RibbleController;
use crate::ui::panes::ribble_pane::{PaneView, RibblePane, RibblePaneId};
use crate::utils::errors::RibbleError;
use std::collections::HashMap;
use std::f32::consts::PI;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use strum::EnumCount;

const FOCUS_ANIMATION_DURATION: f32 = 2.0;

pub(in crate::ui) struct RibbleTree {
    data_directory: PathBuf,
    tree: egui_tiles::Tree<RibblePane>,
    behavior: RibbleTreeBehavior,
    // For focus panes -> uses sin(t) to oscillate saturation to create contrast.
    period: f32,
    horiz_expansion: f32,
}

impl RibbleTree {
    const TREE_FILE: &'static str = "ribble_layout.ron";
    pub(in crate::ui) fn new(
        data_directory: &Path,
        controller: RibbleController,
    ) -> Result<Self, RibbleError> {
        let tree_file = data_directory.join(Self::TREE_FILE);
        let tree = Self::deserialize_tree(tree_file.as_path());
        let behavior = RibbleTreeBehavior::from_tree(controller, &tree);
        Ok(Self {
            data_directory: data_directory.to_path_buf(),
            tree,
            behavior,
            // TODO: test to see whether or not this is annoying and remove if it is.
            period: FOCUS_ANIMATION_DURATION,
            horiz_expansion: 2.0 * PI / FOCUS_ANIMATION_DURATION,
        })
    }
    pub(in crate::ui) fn ui(&mut self, ui: &mut egui::Ui) {
        // Mutably borrow once: add any new panes to the tree before painting.
        self.check_add_new_tabs();

        // Update the time for the focused_pane behavior.
        let time = ui.ctx().input(|i| i.time) as f32 % self.period;
        self.behavior.focus_time = 0.5 * (self.horiz_expansion * time).sin() + 0.5;
        debug_assert!(self.behavior.focus_time >= 0.0);
        debug_assert!(self.behavior.focus_time <= 1.0);

        // Unpack the struct and draw the tree.
        let RibbleTree {
            data_directory: _,
            tree,
            behavior,
            ..
        } = self;

        tree.ui(behavior, ui)
    }

    pub(in crate::ui) fn add_new_pane(&mut self, pane_id: RibblePaneId) {
        self.behavior.add_child = Some(pane_id);
    }

    // This checks for a new child and either focuses/inserts the tab
    // JUST insert at the root.
    // TODO: test the GUI here -> it might make sense to add the "focus" pane even if it's the active tab
    fn check_add_new_tabs(&mut self) {
        let RibbleTree {
            data_directory: _,
            tree,
            behavior,
            ..
        } = self;

        if behavior.add_child.is_none() {
            return;
        }

        let ribble_id = behavior.add_child.take().unwrap();

        // Check to see if there's an entry in the map.
        match behavior.opened_tabs.get(&ribble_id) {
            Some(pane_id) => {
                let tiles = &mut tree.tiles;
                // First, check that the tile is actually in the tree
                let ribble_tile = tiles.get(*pane_id);
                // If it's in the tree, make sure the tile is a pane
                if let Some(tile) = ribble_tile {
                    debug_assert!(
                        tile.is_pane(),
                        "The ribble tile should never be a container type."
                    );
                    // If there's a parent and the parent is a tab container, set it to be the active tab
                    if let Some(parent_id) = tiles.parent_of(*pane_id) {
                        if let Some(egui_tiles::Tile::Container(egui_tiles::Container::Tabs(
                                                                    container,
                                                                ))) = tiles.get_mut(parent_id)
                        {
                            container.set_active(*pane_id);
                            return;
                        }
                    }
                    // Otherwise set the pane to be "in-focus" -- to be handled by an outline color in the gui.
                    behavior.focus_non_tab_pane = Some(*pane_id);
                } else {
                    // The tile is somehow -not- in the tree, and therefore has no parent.
                    // Update the record in the hashmap after getting the new id.
                    let new_child = tiles.insert_pane(ribble_id.into());
                    Self::handle_missing_node(tree, new_child, behavior);
                    // Update the entry in the map.
                    behavior.opened_tabs.insert(ribble_id, new_child);
                }
            }
            // Not opened yet, add a pane and focus it if it's a tab.
            None => {
                let tiles = &mut tree.tiles;
                let new_child = tiles.insert_pane(ribble_id.into());

                Self::handle_missing_node(tree, new_child, behavior);
                // Add an entry into the opened_tabs map.
                behavior.opened_tabs.insert(ribble_id, new_child);
            }
        }
    }

    // Because it would be an absolute borrowing nightmare to try and wrestle this method on &mut self,
    // just let it be static and take in the parts as arguments.
    // This finds/(or re-creates) the root container node and inserts at the end.
    fn handle_missing_node(
        tree: &mut egui_tiles::Tree<RibblePane>,
        new_child: egui_tiles::TileId,
        behavior: &mut RibbleTreeBehavior,
    ) {
        let tiles = &mut tree.tiles;
        let root = tree.root.expect("The tree should never be empty.");
        let tile = tiles
            .get_mut(root)
            .expect("The root node should never be empty.");
        match tile {
            egui_tiles::Tile::Pane(_) => {
                // NOTE: if this ever triggers, that means there's some sort of issue with the Tree::gc(..) sweep.
                // It should also be the case such that
                debug_assert!(
                    tiles.len() == 1,
                    "Root is a pane, but the length of the tree is: {}; there are dangling references.",
                    tiles.len()
                );
                // Insert it as a tab; just makes everything easier.
                let new_root = tiles.insert_tab_tile(vec![root, new_child]);
                let tile = tiles
                    .get_mut(new_root)
                    .expect("The new root node was just inserted.");
                // Set the active child.
                if let egui_tiles::Tile::Container(egui_tiles::Container::Tabs(tabs)) = tile {
                    tabs.set_active(new_child)
                }

                tree.root = Some(new_root);
            }
            egui_tiles::Tile::Container(container) => {
                container.add_child(new_child);
                // If it's a -tab-, then make it the active one.
                if let egui_tiles::Container::Tabs(tabs) = container {
                    tabs.set_active(new_child);
                } else {
                    behavior.focus_non_tab_pane = Some(new_child);
                }
            }
        }
    }

    pub(in crate::ui) fn tree_serializer(&self) -> TreeSerializer {
        let canonicalized = self.data_directory.join(Self::TREE_FILE);
        TreeSerializer { out_file_path: canonicalized, tree: self.tree.clone() }
    }

    fn deserialize_tree(data_directory: &Path) -> egui_tiles::Tree<RibblePane> {
        let canonicalized = data_directory.join(Self::TREE_FILE);
        match std::fs::File::open(canonicalized.as_path()) {
            Ok(tree_file) => ron::de::from_reader(tree_file).unwrap_or_else(|e| {
                log::warn!("Error deserializing tree file: {e}");
                Self::default_tree()
            }),
            Err(e) => {
                log::warn!("Error opening tree file: {e}");
                Self::default_tree()
            }
        }
    }

    // Vertical (root) (OR) Tabs **(better idea):
    //  // Horizontal:
    //      // Vertical:
    //          // Transcription
    //          // Visualizer
    //      // Transcriber

    // Perhaps leave progress/downloads/console as "extra goodies" people can open if they want.
    // TODO: test -> I'm not sure if leaf-order is right to left, or left to right.
    fn default_tree() -> egui_tiles::Tree<RibblePane> {
        let mut tiles = egui_tiles::Tiles::default();
        let transcriber_layout = {
            let children = vec![tiles.insert_pane(RibblePaneId::Transcription.into()), tiles.insert_pane(RibblePaneId::Visualizer.into())];
            tiles.insert_vertical_tile(children)
        };

        let main_layout = {
            let children = vec![transcriber_layout, tiles.insert_pane(RibblePaneId::Transcriber.into())];
            tiles.insert_horizontal_tile(children)
        };

        let root = tiles.insert_tab_tile(vec![main_layout]);
        egui_tiles::Tree::new("ribble_tree", root, tiles)
    }
}

impl Drop for RibbleTree {
    fn drop(&mut self) {
        let tree = self.tree_serializer();
        tree.serialize();
    }
}

// Proxy object for serializing the app tree.
pub(in crate::ui) struct TreeSerializer {
    out_file_path: PathBuf,
    tree: egui_tiles::Tree<RibblePane>,
}

impl TreeSerializer {
    pub(in crate::ui) fn serialize(&self) {
        match std::fs::File::create(self.out_file_path.as_path()) {
            Ok(tree_file) => {
                let writer = BufWriter::new(tree_file);
                match ron::Options::default().to_io_writer_pretty(
                    writer,
                    &self.tree,
                    ron::ser::PrettyConfig::default(),
                ) {
                    Ok(_) => {
                        log::info!("Tree serialized to: {}", self.out_file_path.display());
                    }
                    Err(e) => {
                        log::warn!("Failed to serialize tree: {e}");
                    }
                }
            }
            Err(e) => {
                log::warn!("Failed to create tree file: {e}");
            }
        }
    }
}

pub(in crate::ui) struct RibbleTreeBehavior {
    controller: RibbleController,
    opened_tabs: HashMap<RibblePaneId, egui_tiles::TileId>,
    simplification_options: egui_tiles::SimplificationOptions,
    tab_bar_height: f32,
    gap_width: f32,
    // NOTE: this should -only- be set by the app via a "windows" menu.
    add_child: Option<RibblePaneId>,
    focus_non_tab_pane: Option<egui_tiles::TileId>,
    focus_time: f32,
}

impl RibbleTreeBehavior {
    const TAB_BAR_HEIGHT: f32 = 24.0;
    const GAP_WIDTH: f32 = 2.0;
    const FOCUS_STROKE_WIDTH: f32 = 2.0;
    // This is in seconds
    pub(in crate::ui) fn new(controller: RibbleController) -> Self {
        Self {
            controller,
            // Allocate for at least 1 of each tab.
            // It's not yet determined whether to allow duplicates or not.
            opened_tabs: HashMap::with_capacity(RibblePane::COUNT),
            simplification_options: Default::default(),
            tab_bar_height: Self::TAB_BAR_HEIGHT,
            gap_width: Self::GAP_WIDTH,
            add_child: None,
            focus_non_tab_pane: None,
            // NOTE: if this should become a settable parameter,
            // create a builder/mutator.
            focus_time: 0.0,
        }
    }

    pub(in crate::ui) fn from_tree(
        controller: RibbleController,
        tree: &egui_tiles::Tree<RibblePane>,
    ) -> Self {
        // Preallocate for at least RibbleTab::COUNT, such that there exists a bucket for each tab.
        // At any given time, all panes may be in the tree, so this might save on an allocation.
        let mut opened_tabs = HashMap::with_capacity(RibblePane::COUNT);

        // Travel the tree and grab all RibbleTabs to store their TileId
        // These are used when adding a new tab; if one already exists in the tree,
        // it'll be brought into focus, rather than adding a duplicate.
        for (tile_id, tile) in tree.tiles.iter() {
            match tile {
                egui_tiles::Tile::Pane(ribble_pane) => {
                    let ribble_id = ribble_pane.pane_id();
                    // TileId implements Copy, so this can just be dereferenced.
                    opened_tabs.insert(ribble_id, *tile_id);
                }
                egui_tiles::Tile::Container(_) => {}
            }
        }

        Self {
            controller,
            opened_tabs,
            simplification_options: Default::default(),
            tab_bar_height: Self::TAB_BAR_HEIGHT,
            gap_width: Self::GAP_WIDTH,
            add_child: None,
            focus_non_tab_pane: None,
            // TODO: remove this if it's annoying.
            focus_time: 0.0,
        }
    }

    pub(in crate::ui) fn add_new_pane(&mut self, pane_id: RibblePaneId) {
        self.add_child = Some(pane_id);
    }
}

impl egui_tiles::Behavior<RibblePane> for RibbleTreeBehavior {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        // NOTE: this is the TileId of the pane and not its parent
        tile_id: egui_tiles::TileId,
        pane: &mut RibblePane,
    ) -> egui_tiles::UiResponse {
        // It's cheap to clone the controller; just an atomic increment.
        // If it somehow becomes a bottleneck, take it in by reference.
        let resp = pane.pane_ui(ui, tile_id, self.controller.clone());
        if let Some(focus_id) = self.focus_non_tab_pane {
            // If the user has noticed that the tab they tried to open is in focus and move their
            // mouse to it, turn off the focus.
            if resp.hovered() && focus_id == tile_id {
                self.focus_non_tab_pane = None;
            }

            // This will drive the oscillating saturation to hopefully make a pane a bit more
            // noticeable.
            ui.ctx().request_repaint();
        }

        if resp.dragged() {
            egui_tiles::UiResponse::DragStarted
        } else {
            egui_tiles::UiResponse::None
        }
    }

    fn tab_title_for_pane(&mut self, pane: &RibblePane) -> egui::WidgetText {
        pane.pane_title()
    }

    fn is_tab_closable(
        &self,
        tiles: &egui_tiles::Tiles<RibblePane>,
        tile_id: egui_tiles::TileId,
    ) -> bool {
        if let Some(tile) = tiles.get(tile_id) {
            match tile {
                egui_tiles::Tile::Pane(ribble_pane) => ribble_pane.is_pane_closable(),
                // NOTE: I don't believe it's possible for this to ever be reached, but in case it
                // does, the container itself should always be closable; it's only panes that
                // should control whether they close.
                egui_tiles::Tile::Container(_) => true,
            }
        } else {
            true
        }
    }

    fn on_tab_close(
        &mut self,
        tiles: &mut egui_tiles::Tiles<RibblePane>,
        tile_id: egui_tiles::TileId,
    ) -> bool {
        if let Some(tile) = tiles.get_mut(tile_id) {
            match tile {
                egui_tiles::Tile::Pane(ribble_tab) => {
                    let close_tab = ribble_tab.on_pane_close(self.controller.clone());
                    // If it's a close-able tab, remove it from the mapping.
                    if close_tab {
                        let id = ribble_tab.pane_id();
                        self.opened_tabs.remove(&id);
                    }
                    close_tab
                }
                egui_tiles::Tile::Container(_) => true,
            }
        } else {
            true
        }
    }

    fn tab_title_for_tile(
        &mut self,
        tiles: &egui_tiles::Tiles<RibblePane>,
        tile_id: egui_tiles::TileId,
    ) -> egui::WidgetText {
        if let Some(tile) = tiles.get(tile_id) {
            match tile {
                egui_tiles::Tile::Pane(pane) => self.tab_title_for_pane(pane),
                // NOTE: this could recursively travel the active child to get the "active-est"
                // child, but that might blow the call stack.
                // It's easiest here to just default to: "Ribble: ContainerKind, which should
                // hopefully be enough to get the point across."
                // It's expected that this will primarily happen when a new tab is pushed to the
                // tree.
                egui_tiles::Tile::Container(container) => {
                    format!("Ribble: {:?}", container.kind()).into()
                }
            }
        } else {
            "MISSING TILE".into()
        }
    }

    fn tab_bar_height(&self, _style: &egui::Style) -> f32 {
        self.tab_bar_height
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        self.gap_width
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        self.simplification_options
    }

    fn paint_on_top_of_tile(
        &self,
        painter: &egui::Painter,
        style: &egui::Style,
        tile_id: egui_tiles::TileId,
        rect: eframe::emath::Rect,
    ) {
        // TODO: at the moment this is animated but it might be too annoying.
        // If so, just remove it.
        // Also: I'm not sure whether or not this color will just be white.
        // If so, only lerp the value.
        let mut color: egui::epaint::Hsva = style.visuals.selection.stroke.color.into();
        color.s = egui::lerp(color.s..=(color.s + 0.5).max(1.0), self.focus_time);
        color.v = egui::lerp(color.v..=(color.v + 0.5).max(0.8), self.focus_time);
        if let Some(focused_pane) = self.focus_non_tab_pane {
            if focused_pane == tile_id {
                painter.rect_stroke(
                    rect,
                    style.visuals.window_corner_radius,
                    egui::Stroke::new(Self::FOCUS_STROKE_WIDTH, color),
                    egui::StrokeKind::Outside,
                );
            }
        }
    }
}

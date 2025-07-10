mod recording_tab;
pub(in crate::ui) mod ribble_tab;
mod transcriber_tab;

mod console_tab;
mod model_tab;
mod progress_tab;
pub(in crate::ui) mod tabs;
mod transcription_tab;
mod user_preferences;
mod vad_configs;
mod visualizer_tab;

use crate::controller::ribble_controller::RibbleController;
use crate::ui::new_tabs::ribble_tab::{RibbleTab, RibbleTabId, TabView};
use crate::utils::errors::RibbleError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use strum::EnumCount;

#[derive(Clone)]
pub(in crate::ui) struct RibbleTree {
    data_directory: PathBuf,
    tree: egui_tiles::Tree<RibbleTab>,
    behavior: RibbleTreeBehavior,
}

impl RibbleTree {
    // TODO: decide on an appropriate name
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
        })
    }

    pub(in crate::ui) fn ui(&mut self, ui: &mut egui::Ui) {
        // Mutably borrow once: add any new tabs to the tree before painting.
        self.check_add_new_tabs();

        // Unpack the struct and draw the tree.
        let RibbleTree {
            data_directory: _,
            tree,
            behavior,
        } = self;

        tree.ui(behavior, ui)
    }

    // This checks for a new child and either focuses/inserts the tab
    fn check_add_new_tabs(&mut self) {
        let RibbleTree {
            data_directory: _,
            tree,
            behavior,
        } = self;

        if behavior.add_child_to.is_none() {
            return;
        }

        let (tile_id, ribble_id) = behavior.add_child_to.take().unwrap();

        // Check to see if there's an entry in the map.
        let pane_id = behavior.opened_tabs.get(&ribble_id);

        // Not opened yet, add a pane and focus it if it's a tab.
        if pane_id.is_none() {
            // It should be cheap enough to just copy this.
            let tiles = &mut tree.tiles;
            let new_child = tiles.insert_pane(ribble_id.into());
            let insert_at_tile = tiles.get_mut(tile_id);

            // If there's no insertion node, insert somewhere at the root.
            if insert_at_tile.is_none() {
                Self::handle_missing_node(tree, new_child, behavior);
                // Add an entry into the opened_tabs map.
                behavior.opened_tabs.insert(ribble_id, new_child);
                return;
            }

            // Otherwise, there is an insertion point.
            // If it's a container, make it into a Tabs and insert the new tab as a focused tab.
            // Otherwise, it's a Pane and needs to be contained in a (parent) tab.
            let insert_at_tile = insert_at_tile.unwrap();
            match insert_at_tile {
                egui_tiles::Tile::Pane(_) => {
                    // Try and get the Pane's parent
                    if let Some(parent) = tiles.parent_of(tile_id) {
                        if let Some(egui_tiles::Tile::Container(container)) = tiles.get_mut(parent)
                        {
                            //
                            container.add_child(new_child);
                            if let egui_tiles::Container::Tabs(tabs) = container {
                                tabs.set_active(new_child);
                            } else {
                                behavior.focus_non_tab_pane = Some(new_child);
                            }
                        } else {
                            unreachable!(
                                "It's not possible for a Pane to be a child of a pane. This shouldn't be reached"
                            );
                        }
                    } else {
                        // Otherwise, there's no parent, which means we're at the root node.
                        let root = tree.root.expect("The tree should never, ever be empty.");
                        // At least, we should be at the root node.
                        debug_assert_eq!(
                            root, tile_id,
                            "Pane has no parent, but is also not root. Pane id: {}, Root id: {}",
                            tile_id.0, root.0
                        );
                        // Insert at the root and make the parent container into tabs.
                        Self::handle_missing_node(tree, new_child, behavior);
                        // Make an entry into the hashmap so that records are maintained.
                        behavior.opened_tabs.insert(ribble_id, new_child);
                    }
                }
                egui_tiles::Tile::Container(container) => {
                    container.add_child(new_child);
                    if let egui_tiles::Container::Tabs(tabs) = container {
                        tabs.set_active(new_child);
                    } else {
                        behavior.focus_non_tab_pane = Some(new_child);
                    }
                }
            }

            // Otherwise, we do have an active pane in the tree.
            // The parent of this pane must be a container, otherwise there's something seriously wrong.
        } else {
            let pane_id = pane_id.unwrap();
            let mut tiles = tree.tiles.clone();
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
                // Otherwise set the tab to be "in-focus" -- to be handled by an outline color in the gui.
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
    }

    // Because it would be an absolute borrowing nightmare to try and wrestle this method on &mut self,
    // just let it be static and take in the parts as arguments.
    // This finds the root node (if
    fn handle_missing_node(
        tree: &mut egui_tiles::Tree<RibbleTab>,
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

    pub(in crate::ui) fn serialize_tree(&self) {
        todo!("IMPLEMENT SERIALIZER.");
    }

    fn deserialize_tree(path: &Path) -> egui_tiles::Tree<RibbleTab> {
        todo!("IMPLEMENT DESERIALIZER.");
        // Try to read the tree from disk: if it's not there, log it and either return an error or construct the default layout.
    }
    fn default_tree() -> egui_tiles::Tree<RibbleTab> {
        todo!("")
    }
}

impl Drop for RibbleTree {
    fn drop(&mut self) {
        self.serialize_tree()
    }
}

#[derive(Clone)]
pub(in crate::ui) struct RibbleTreeBehavior {
    controller: RibbleController,
    opened_tabs: HashMap<RibbleTabId, egui_tiles::TileId>,
    simplification_options: egui_tiles::SimplificationOptions,
    tab_bar_height: f32,
    gap_width: f32,
    // TODO: expose this parameter to tabs -> in the ui loop, get the parent if it's being inserted.
    // Basicially, match on whether it's a Pane ID or a Container ID.
    add_child_to: Option<(egui_tiles::TileId, RibbleTabId)>,
    focus_non_tab_pane: Option<egui_tiles::TileId>,
}

// TODO: double check the default implementation and make any changes necessary so that
// intended behaviour is maintained.
impl RibbleTreeBehavior {
    const TAB_BAR_HEIGHT: f32 = 24.0;
    const GAP_WIDTH: f32 = 2.0;
    const FOCUS_STROKE_WIDTH: f32 = 2.0;
    pub(in crate::ui) fn new(controller: RibbleController) -> Self {
        Self {
            controller,
            // Allocate for at least 1 of each tab.
            // It's not yet determined whether to allow duplicates or not.
            opened_tabs: HashMap::with_capacity(RibbleTab::COUNT),
            simplification_options: Default::default(),
            tab_bar_height: Self::TAB_BAR_HEIGHT,
            gap_width: Self::GAP_WIDTH,
            add_child_to: None,
            focus_non_tab_pane: None,
        }
    }

    pub(in crate::ui) fn from_tree(
        controller: RibbleController,
        tree: &egui_tiles::Tree<RibbleTab>,
    ) -> Self {
        // Preallocate for at least RibbleTab::COUNT, such that there exists a bucket for each tab.
        // At any given time, all tabs may be in the tree, so this might save on an allocation.
        let mut opened_tabs = HashMap::with_capacity(RibbleTab::COUNT);

        // Travel the tree and grab all RibbleTabs to store their TileId
        // These are used when adding a new tab; if one already exists in the tree,
        // it'll be brought into focus, rather than adding a duplicate.
        for (tile_id, tile) in tree.tiles.iter() {
            match tile {
                egui_tiles::Tile::Pane(ribble_tab) => {
                    let ribble_id = ribble_tab.tile_id();
                    // TileId implements Copy, so this can just be dereferenced.
                    opened_tabs.insert(ribble_id, *tile_id);
                }
                egui_tiles::Tile::Container(_) => continue,
            }
        }

        Self {
            controller,
            opened_tabs,
            simplification_options: Default::default(),
            tab_bar_height: Self::TAB_BAR_HEIGHT,
            gap_width: Self::GAP_WIDTH,
            add_child_to: None,
            focus_non_tab_pane: None,
        }
    }
}

impl egui_tiles::Behavior<RibbleTab> for RibbleTreeBehavior {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        // NOTE: this is the TileId of the pane and not its parent
        tile_id: egui_tiles::TileId,
        pane: &mut RibbleTab,
    ) -> egui_tiles::UiResponse {
        let pane_id = egui::Id::new(tile_id);

        // It's cheap to clone the controller; just an atomic increment.
        // If it somehow becomes a bottleneck, take it in by reference.
        let resp = pane.pane_ui(ui, tile_id, self.controller.clone());
        if let Some(focus_id) = self.focus_non_tab_pane {
            // If the user has noticed that the tab they tried to open is in focus and move their
            // mouse to it, turn off the focus.
            if resp.hovered() && focus_id == tile_id {
                self.focus_non_tab_pane = None;
            }
        }

        if resp.dragged() {
            egui_tiles::UiResponse::DragStarted
        } else {
            egui_tiles::UiResponse::None
        }
    }

    fn tab_title_for_pane(&mut self, pane: &RibbleTab) -> egui::WidgetText {
        pane.tab_title()
    }

    fn is_tab_closable(
        &self,
        tiles: &egui_tiles::Tiles<RibbleTab>,
        tile_id: egui_tiles::TileId,
    ) -> bool {
        if let Some(tile) = tiles.get(tile_id) {
            match tile {
                egui_tiles::Tile::Pane(ribble_tab) => ribble_tab.is_tab_closable(),
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
        tiles: &mut egui_tiles::Tiles<RibbleTab>,
        tile_id: egui_tiles::TileId,
    ) -> bool {
        if let Some(tile) = tiles.get_mut(tile_id) {
            match tile {
                egui_tiles::Tile::Pane(ribble_tab) => {
                    let close_tab = ribble_tab.on_tab_close(self.controller.clone());
                    // If it's a close-able tab, remove it from the mapping.
                    if close_tab {
                        let id = ribble_tab.tile_id();
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

    fn top_bar_right_ui(
        &mut self,
        _tiles: &egui_tiles::Tiles<RibbleTab>,
        ui: &mut egui::Ui,
        tile_id: egui_tiles::TileId,
        _tabs: &egui_tiles::Tabs,
        _scroll_offset: &mut f32,
    ) {
        // TODO: determine whether it's worth it to just travel the tiles here?
        // It might be faster to use the hashmap.
        // Alternatively, add an option for focused tabs?

        // TODO: draw a ui.button( + ) that opens a contextual menu containing a list of ClosableTabs
        // Upon selection, set self.add_child_to
        todo!("IMPLEMENT THIS BUTTON.");
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

    // TODO: if instead this is going to be animated between two colours based on sin(time),
    // accumulate time in the UI.
    fn paint_on_top_of_tile(
        &self,
        painter: &egui::Painter,
        style: &egui::Style,
        tile_id: egui_tiles::TileId,
        rect: eframe::emath::Rect,
    ) {
        if let Some(focused_pane) = self.focus_non_tab_pane {
            if focused_pane == tile_id {
                painter.rect_stroke(
                    rect,
                    style.visuals.window_corner_radius,
                    egui::Stroke::new(
                        Self::FOCUS_STROKE_WIDTH,
                        style.visuals.selection.stroke.color,
                    ),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
}

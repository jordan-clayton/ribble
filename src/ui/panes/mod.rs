mod recording_pane;
pub(in crate::ui) mod ribble_pane;
mod transcriber_pane;

mod console_pane;
mod downloads_pane;
pub(in crate::ui) mod pane_list;
mod progress_pane;
mod transcription_pane;
mod user_preferences_pane;
mod visualizer_pane;

use crate::controller::ribble_controller::RibbleController;
use crate::ui::panes::ribble_pane::{PaneView, RibblePane, RibblePaneId};
use crate::utils::errors::RibbleError;
use eframe::epaint::Hsva;
use egui::{lerp, Painter, Rect, Stroke, StrokeKind, Style};
use egui_tiles::{
    Behavior, Container, ResizeState, SimplificationOptions, Tile, TileId, Tiles, Tree, UiResponse,
};
use std::collections::HashMap;
use std::error::Error;
use std::f32::consts::PI;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use strum::{EnumCount, IntoEnumIterator};

const FOCUS_ANIMATION_DURATION: f32 = 1.0;

pub(in crate::ui) struct RibbleTree {
    data_directory: PathBuf,
    tree: Tree<RibblePane>,
    behavior: RibbleTreeBehavior,
    // For focus panes -> uses sin(t) to oscillate saturation to create contrast.
    period: f32,
    horiz_expansion: f32,
}

impl RibbleTree {
    const TREE_FILE: &'static str = "ribble_layout.ron";
    // TODO: this probably doesn't need to be a result and can just be Self
    pub(in crate::ui) fn new(data_directory: &Path, controller: RibbleController) -> Self {
        let tree = Self::deserialize_tree(data_directory);
        let behavior = RibbleTreeBehavior::from_tree(controller, &tree);

        let mut ribble_tree = Self {
            data_directory: data_directory.to_path_buf(),
            tree,
            behavior,
            period: FOCUS_ANIMATION_DURATION,
            horiz_expansion: 2.0 * PI / FOCUS_ANIMATION_DURATION,
        };

        // Do a 1-pass to check that all non-closable tabs are in the tree
        ribble_tree.check_insert_non_closable_panes();
        ribble_tree
    }

    pub(in crate::ui) fn is_invalid(&self) -> bool {
        self.tree.is_empty()
    }
    pub(in crate::ui) fn recovery_tree_exists(&self) -> bool {
        let canonicalized = self.data_directory.join(Self::TREE_FILE);
        canonicalized.exists() && canonicalized.is_file()
    }

    pub(in crate::ui) fn ui(&mut self, ui: &mut egui::Ui) {
        // Try to recover the previous layout.
        if self.tree.is_empty() {
            self.try_recover_layout();
        }

        // Check to ensure the root exists in the tiles collection
        let root = self.tree.root.expect("A non-empty tree must have a root.");
        if self.tree.tiles.get(root).is_none() {
            self.try_recover_layout();
        }

        // Do a once-over of any tabs which should be closed.
        self.check_remove_old_tabs();
        // Add any new panes to the tree before painting.
        self.check_add_new_pane();

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

    // NOTE: this probably should not be called too often.
    pub(in crate::ui) fn check_insert_non_closable_panes(&mut self) {
        // Check for an invalid tree - sometimes it can get lost if there's a panic in the UI code.
        // Fall-back to a previously serialized version, and if that fails, just reset to defaults.

        // In most cases, this function is called right after attempting to restore layout, so the
        // tree should never be empty.
        if self.tree.is_empty() {
            // This will fall back to defaults if there's no root.
            self.tree = Self::deserialize_tree(&self.data_directory);
            self.behavior =
                RibbleTreeBehavior::from_tree(self.behavior.controller.clone(), &self.tree);
        }

        // For all non-closable tabs, check to make sure they exist -somewhere- in the layout and
        // insert them if not.
        // Since it is possible for the opened_tabs and the tree to get out of sync, the tree is the authority
        // and the map will be updated to reflect the state of the tree.
        for non_closable in RibblePaneId::iter().filter(|p| !p.is_closable()) {
            match self.tree.tiles.find_pane(&non_closable.into()) {
                None => {
                    self.insert_child(non_closable);
                }
                Some(tile_id) => {
                    self.behavior
                        .opened_tabs
                        .entry(non_closable)
                        .or_insert(tile_id);
                }
            }
        }
    }

    pub(in crate::ui) fn add_new_pane(&mut self, pane_id: RibblePaneId) {
        self.behavior.add_new_pane(pane_id);
    }

    fn check_remove_old_tabs(&mut self) {
        let RibbleTree {
            data_directory: _,
            tree,
            behavior,
            ..
        } = self;

        let RibbleTreeBehavior {
            opened_tabs,
            remove_children,
            ..
        } = behavior;

        while let Some((tile_id, pane_id)) = remove_children.pop() {
            // Since this should always be a pane, there shouldn't be a need to remove
            // recursively.
            // The tree SimpleOptions should also take care of flattening containers.
            // (NOTE: THIS NEEDS TO BE TESTED).
            tree.tiles.remove(tile_id);
            opened_tabs.remove(&pane_id);
        }
    }

    fn check_add_new_pane(&mut self) {
        if self.behavior.add_child.is_none() {
            return;
        }

        let ribble_id = self.behavior.add_child.take().unwrap();
        // First, check that the tile is actually in the tree
        match self.behavior.opened_tabs.get(&ribble_id) {
            Some(pane_id) => {
                let ribble_tile = self.tree.tiles.get(*pane_id);
                // If it's in the tree, make sure the tile is a pane
                if let Some(tile) = ribble_tile {
                    debug_assert!(
                        tile.is_pane(),
                        "The ribble tile should never be a container type."
                    );
                    // If there's a parent and the parent is a tab container, set it to be the active tab
                    if let Some(parent_id) = self.tree.tiles.parent_of(*pane_id) {
                        if let Some(Tile::Container(Container::Tabs(container))) =
                            self.tree.tiles.get_mut(parent_id)
                        {
                            container.set_active(*pane_id);
                        }
                    }
                    // Set the pane to be in focus regardless -> tabs aren't really used in the application atm.
                    self.behavior.focus_non_tab_pane = Some(*pane_id);
                } else {
                    // NOTE: if the root is missing, this will automatically fall back to defaults
                    self.insert_child(ribble_id);
                }
            }
            // Not opened yet, add a pane and focus it if it's a tab.
            None => {
                // NOTE: if the root is missing, this will automatically fall back to defaults
                self.insert_child(ribble_id);
            }
        }
    }

    // This function gets called as part of the check to ensure all non-closable panes exist
    // somewhere in the tree.
    fn insert_child(&mut self, ribble_id: RibblePaneId) {
        let new_child = self.tree.tiles.insert_pane(ribble_id.into());

        // NOTE: this can fail twice (no root, no root container), but the deserialization should
        // catch an invalid tree and fall back to the default layout.
        // (There are also guards against serializing an invalid tree)
        match self.handle_missing_node(new_child) {
            Ok(_) => {
                // ADD a record into the opened tabs to prevent duplicates
                self.behavior.opened_tabs.insert(ribble_id, new_child);
            }
            Err(_) => {
                // Try to deserialize things first.
                self.tree = Self::deserialize_tree(&self.data_directory);
                self.behavior =
                    RibbleTreeBehavior::from_tree(self.behavior.controller.clone(), &self.tree);

                // If the old layout had the tab in it (and has all valid panes)
                if self.behavior.opened_tabs.contains_key(&ribble_id) {
                    return;
                }

                let new_child = self.tree.tiles.insert_pane(ribble_id.into());
                self.handle_missing_node(new_child)
                    .expect("Default layout should have a root node.");
                // ADD a record into the opened tabs to prevent duplicates
                self.behavior.opened_tabs.insert(ribble_id, new_child);
            }
        }
    }

    pub(in crate::ui) fn try_recover_layout(&mut self) {
        // This will reconstruct a default tree if the old tree is empty,
        // OR if the root node has no corresponding tile in the tree.
        self.tree = Self::deserialize_tree(&self.data_directory);
        self.behavior = RibbleTreeBehavior::from_tree(self.behavior.controller.clone(), &self.tree);
        // This will try to ensure the non-closable panes remain in the tree at all times.
        // If, somehow, there is no root/an invalid tree, this will, again fall back to the default layout.
        self.check_insert_non_closable_panes();
    }

    pub(in crate::ui) fn reset_layout(&mut self) {
        self.tree = Self::default_tree();
        self.behavior = RibbleTreeBehavior::from_tree(self.behavior.controller.clone(), &self.tree);
    }

    fn handle_missing_node(&mut self, new_child: TileId) -> Result<(), RibbleError> {
        let root = self
            .tree
            .root
            .ok_or(RibbleError::Core("Tree missing!".to_string()))?;
        match self
            .tree
            .tiles
            .get_mut(root)
            .ok_or(RibbleError::Core("Root node has no tile!".to_string()))?
        {
            Tile::Pane(_) => {
                // NOTE: if this ever triggers, that means there's some sort of issue with the Tree::gc(..) sweep.
                // It should also be the case such that
                debug_assert!(
                    self.tree.tiles.len() == 1,
                    "Root is a pane, but the length of the tree is: {}; there are dangling references.",
                    self.tree.tiles.len()
                );
                // Insert it with a horizontal layout like the default.
                let new_root = self
                    .tree
                    .tiles
                    .insert_horizontal_tile(vec![root, new_child]);
                self.tree.root = Some(new_root);
                self.behavior.focus_non_tab_pane = Some(new_child);
            }
            Tile::Container(container) => {
                container.add_child(new_child);
                // If it's a -tab-, then make it the active one first.
                if let Container::Tabs(tabs) = container {
                    tabs.set_active(new_child);
                }
                // Then focus it to alert the user (Atm, not going with any tabs at all, so the
                // above branch should never ever hit.)
                self.behavior.focus_non_tab_pane = Some(new_child);
            }
        }
        Ok(())
    }

    #[cfg(debug_assertions)]
    pub(in crate::ui) fn test_tree_recovery(&mut self) {
        // So, this is a "nuke the tree" and induce a panic to re-create conditions which
        // can sometimes cause the entire tree to bug out.
        self.clear_tree();
        // Deliberately panic with a null tree.
        assert!(self.tree.is_empty());
        self.tree.root.unwrap();
    }

    #[cfg(debug_assertions)]
    pub(in crate::ui) fn clear_tree(&mut self) {
        // This only clears the tree to try and catch an empty tree on the ui paint.
        self.tree = Tree::empty("ribble_tree");
    }

    pub(in crate::ui) fn tree_serializer(&self) -> TreeSerializer {
        let canonicalized = self.data_directory.join(Self::TREE_FILE);
        TreeSerializer {
            out_file_path: canonicalized,
            tree: self.tree.clone(),
        }
    }

    fn deserialize_tree(data_directory: &Path) -> Tree<RibblePane> {
        let canonicalized = data_directory.join(Self::TREE_FILE);
        match std::fs::File::open(canonicalized.as_path()) {
            Ok(tree_file) => {
                let tree = ron::de::from_reader(tree_file).unwrap_or_else(|e| {
                    log::warn!(
                        "Error deserializing tree file: {}\n\
                Error: {}\n\
                Error:source: {:#?}",
                        canonicalized.display(),
                        &e,
                        e.source()
                    );
                    Self::default_tree()
                });

                if tree.is_empty() {
                    log::error!("Deserialized Empty tree! Falling back to default.");
                    return Self::default_tree();
                }

                // If the tree is non-empty, there must exist a root
                let root = tree.root().expect("A non-empty tree must have a root node");
                // Check to see whether the root maps to a tile

                // If it doesn't, there's something -really- weird going on;
                // It's a bit difficult to parse what's going on with egui_tiles, but the tree can
                // become incoherent during layout panics.

                // What seems to be happening:
                //  - If there's a layout panic, the pane gets removed (or not re-inserted) into the tree.
                //  - If that leaves only 1 child in the layout, by the default SimpleOptions, this gets flattened
                //      -- Sometimes the root can get lost; sometimes the root gets detached from the tiles, not sure why.
                //      -- Sometimes the root ends up as just a pane.

                // Since there are at least 2 non-closable panes, the root has to be a container.

                // The easiest fix is to just restore the default layout if the root is not a
                // container with children.
                if tree.tiles.get_container(root).is_none() {
                    Self::default_tree()
                } else {
                    // If there's a root and the root is a container, assume it's valid.
                    // There are runtime checks (app load, before serializing, etc.) to ensure the
                    // 2 non-closable panes exist in the view somewhere.
                    tree
                }
            }
            Err(e) => {
                log::warn!(
                    "Error opening tree file: {}\n\
                Error: {}\n\
                Error source: {:#?}",
                    canonicalized.display(),
                    &e,
                    e.source()
                );
                Self::default_tree()
            }
        }
    }

    // Basic tree structure.
    //  // Horizontal:
    //      Left: Vertical:
    //          Top: Transcription Pane
    //          Bottom: Visualizer Pane
    //      Right: Transcriber Pane

    fn default_tree() -> Tree<RibblePane> {
        // NOTE: This is an unfortunate bit of cruft that will need to remain until there exists a
        // cleaner way to define layout shares.
        //
        // Without needing to explicitly define containers, this achieves a 70/30 horizontal split
        // (The code does not become any more readable when defining containers explicitly).
        //
        const LEFT_SHARE: f32 = 0.7;

        let mut tiles = Tiles::default();
        let left = {
            let children = vec![
                tiles.insert_pane(RibblePaneId::Transcription.into()),
                tiles.insert_pane(RibblePaneId::Visualizer.into()),
            ];
            tiles.insert_vertical_tile(children)
        };

        let right = tiles.insert_pane(RibblePaneId::Transcriber.into());

        let main_layout = {
            let children = vec![left, right];
            tiles.insert_horizontal_tile(children)
        };

        if let Some(pane) = tiles.get_mut(main_layout) {
            match pane {
                Tile::Container(container) => match container {
                    Container::Linear(linear) => {
                        linear.shares.set_share(left, LEFT_SHARE.clamp(0.0, 1.0));
                        linear
                            .shares
                            .set_share(right, (1.0 - LEFT_SHARE).clamp(0.0, 1.0));
                    }
                    _ => unreachable!("The main layout must always be horizontal linear"),
                },
                _ => unreachable!("The main layout must always be a container."),
            }
        }

        Tree::new("ribble_tree", main_layout, tiles)
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
    tree: Tree<RibblePane>,
}

impl TreeSerializer {
    // NOTE: this only logs errors - there are fallbacks if the tree doesn't serialize/is missing
    // If the root-node is missing/tree is empty, then this will not re-write the old layout because
    // that implies a major error/panic has happened.
    pub(in crate::ui) fn serialize(&self) {
        // CHECK FOR THE ROOT FIRST; if it's missing, the layout crashed and the tree is in an
        // invalid state (at runtime).
        //
        // This should mostly only happen on a program panic (but successful drop).
        // The previously serialized tree may be valid, and if not, will fall back to defaults.
        if self.tree.is_empty() {
            log::error!("Root node missing! Empty tree. Skipping deserialization.");
            return;
        }

        // This cannot be None if the tree is non-empty.
        let root = self
            .tree
            .root
            .expect("A non-empty tree must have a root node");

        // Check to make sure the root maps to a tile in the tree before serializing
        // If this branch is taken, then the tree is in an invalid state (at runtime).
        // The previously serialized tree may be valid, and if not, will fall back to defaults.
        if self.tree.tiles.get(root).is_none() {
            log::error!("Root node has no tile! Skipping deserialization.");
            return;
        }

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
    opened_tabs: HashMap<RibblePaneId, TileId>,
    simplification_options: SimplificationOptions,
    tab_bar_height: f32,
    gap_width: f32,
    // NOTE: this should -only- be set by the app via a "windows" menu.
    add_child: Option<RibblePaneId>,
    remove_children: Vec<(TileId, RibblePaneId)>,
    focus_non_tab_pane: Option<TileId>,
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
            remove_children: Vec::with_capacity(RibblePane::COUNT),
            focus_non_tab_pane: None,
            // NOTE: if this should become a settable parameter,
            // create a builder/mutator.
            focus_time: 0.0,
        }
    }

    pub(in crate::ui) fn from_tree(controller: RibbleController, tree: &Tree<RibblePane>) -> Self {
        // Preallocate for at least RibbleTab::COUNT, such that there exists a bucket for each tab.
        // At any given time, all panes may be in the tree, so this might save on an allocation.
        let mut opened_tabs = HashMap::with_capacity(RibblePane::COUNT);

        // Travel the tree and grab all RibbleTabs to store their TileId
        // These are used when adding a new tab; if one already exists in the tree,
        // it'll be brought into focus, rather than adding a duplicate.
        for (tile_id, tile) in tree.tiles.iter() {
            match tile {
                Tile::Pane(ribble_pane) => {
                    let ribble_id = ribble_pane.pane_id();
                    // TileId implements Copy, so this can just be dereferenced.
                    opened_tabs.insert(ribble_id, *tile_id);
                }
                Tile::Container(_) => {}
            }
        }

        Self {
            controller,
            opened_tabs,
            simplification_options: Default::default(),
            tab_bar_height: Self::TAB_BAR_HEIGHT,
            gap_width: Self::GAP_WIDTH,
            add_child: None,
            remove_children: Vec::with_capacity(RibblePane::COUNT),
            focus_non_tab_pane: None,
            // TODO: remove this if it's annoying.
            focus_time: 0.0,
        }
    }

    pub(in crate::ui) fn add_new_pane(&mut self, pane_id: RibblePaneId) {
        self.add_child = Some(pane_id);
    }
}

impl Behavior<RibblePane> for RibbleTreeBehavior {
    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        // NOTE: this is the TileId of the pane and not its parent
        tile_id: TileId,
        pane: &mut RibblePane,
    ) -> UiResponse {
        let mut should_close = false;
        // It's cheap to clone the controller; just an atomic increment.
        // If it somehow becomes a bottleneck, take it in by reference.
        let resp = pane.pane_ui(ui, &mut should_close, self.controller.clone());
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

        // If the user has requested to close the pane (and it can close), push it to the
        // remove_children vector which will get drained on the next repaint.
        if should_close {
            self.remove_children.push((tile_id, pane.pane_id()));
            ui.ctx().request_repaint();
        }

        if resp.dragged() {
            UiResponse::DragStarted
        } else {
            UiResponse::None
        }
    }

    fn tab_title_for_pane(&mut self, pane: &RibblePane) -> egui::WidgetText {
        pane.pane_title()
    }

    fn is_tab_closable(&self, tiles: &Tiles<RibblePane>, tile_id: TileId) -> bool {
        if let Some(tile) = tiles.get(tile_id) {
            match tile {
                Tile::Pane(ribble_pane) => ribble_pane.is_pane_closable(),
                // NOTE: I don't believe it's possible for this to ever be reached, but in case it
                // does, the container itself should always be closable; it's only panes that
                // should control whether they close.
                Tile::Container(_) => true,
            }
        } else {
            true
        }
    }

    fn on_tab_close(&mut self, tiles: &mut Tiles<RibblePane>, tile_id: TileId) -> bool {
        if let Some(tile) = tiles.get_mut(tile_id) {
            match tile {
                Tile::Pane(ribble_tab) => {
                    log::info!(
                        "Removing pane: {}, ID: {:#?}",
                        ribble_tab.pane_id(),
                        tile_id
                    );
                    let close_tab = ribble_tab.on_pane_close(self.controller.clone());
                    // If it's a closeable tab, remove it from the mapping.
                    if close_tab {
                        let id = ribble_tab.pane_id();
                        self.opened_tabs.remove(&id);
                    }
                    close_tab
                }
                Tile::Container(container) => {
                    log::info!(
                        "Removing container: {:#?}, ID: {:#?}",
                        container.kind(),
                        tile_id
                    );
                    true
                }
            }
        } else {
            true
        }
    }

    fn tab_title_for_tile(
        &mut self,
        tiles: &Tiles<RibblePane>,
        tile_id: TileId,
    ) -> egui::WidgetText {
        if let Some(tile) = tiles.get(tile_id) {
            match tile {
                Tile::Pane(pane) => self.tab_title_for_pane(pane),
                // For now, with tabs: set this up to be the App name + the number of children.
                // I'm not 100% sure I like this, but it does communicate more than just "Ribble"
                Tile::Container(container) => {
                    format!("Ribble: {}", container.num_children()).into()
                }
            }
        } else {
            "MISSING TILE".into()
        }
    }

    fn tab_bar_height(&self, _style: &Style) -> f32 {
        self.tab_bar_height
    }

    fn gap_width(&self, _style: &Style) -> f32 {
        self.gap_width
    }

    fn simplification_options(&self) -> SimplificationOptions {
        self.simplification_options
    }

    fn paint_on_top_of_tile(&self, painter: &Painter, style: &Style, tile_id: TileId, rect: Rect) {
        let mut color: Hsva = style.visuals.selection.stroke.color.into();
        color.s = lerp(color.s..=(color.s + 0.5).max(1.0), self.focus_time);
        color.v = lerp(color.v..=(color.v + 0.5).max(0.8), self.focus_time);
        if let Some(focused_pane) = self.focus_non_tab_pane {
            if focused_pane == tile_id {
                painter.rect_stroke(
                    rect,
                    style.visuals.window_corner_radius,
                    Stroke::new(Self::FOCUS_STROKE_WIDTH, color),
                    StrokeKind::Middle,
                );
            }
        }
    }

    fn resize_stroke(&self, style: &Style, resize_state: ResizeState) -> Stroke {
        // This is basically the same as the default, except it uses egui widget visuals
        // for idle instead of the tab bar color.
        match resize_state {
            ResizeState::Idle => style.visuals.widgets.noninteractive.bg_stroke,
            ResizeState::Hovering => style.visuals.widgets.hovered.fg_stroke,
            ResizeState::Dragging => style.visuals.widgets.active.fg_stroke,
        }
    }
}

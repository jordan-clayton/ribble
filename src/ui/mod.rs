pub(crate) mod app;
mod widgets;
mod panes;

// Since there are fewer items and the comboboxes can get a little too cramped,
// add a little bit more space to the grid spacing.
pub const GRID_ROW_SPACING_COEFF: f32 = 1.2;
const PANE_HEADING_BUTTON_SIZE: f32 = 16.0;
const MODAL_HEIGHT_PROPORTION: f32 = 0.8;
// Right now, going with a symmetric margin
// This may change in the future.
const PANE_INNER_MARGIN: f32 = 8.0;
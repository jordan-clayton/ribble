use egui::{lerp, vec2, Color32, Rgba, Sense, Stroke, Ui};

use crate::utils::constants;

// At the moment, this is pretty bare-bones.
pub fn draw_fft(ui: &mut Ui, data: &[f32; constants::NUM_BUCKETS]) {
    ui.horizontal_centered(|ui| {
        let available_size = ui.available_size();
        let spacing = ui.spacing();
        // TODO: determine whether or not to add more spacing in between widgets.
        // Possibly determine as a fraction of the width.
        let horizontal_spacing = spacing.item_spacing.x;
        let total_horizontal_spacing = horizontal_spacing * ((constants::NUM_BUCKETS + 1) as f32);
        let horizontal_space = available_size.x - total_horizontal_spacing;

        let cell_width = horizontal_space / constants::NUM_BUCKETS as f32;
        // Bounded to 800 px atm.
        let max_cell_height =
            available_size.y * constants::FFT_MAX_HEIGHT_PROPORTION.min(constants::MAX_FFT_HEIGHT);
        let min_cell_height = spacing.interact_size.y;

        let visuals = ui.style().visuals.clone();
        let high_color: Rgba = visuals.widgets.active.bg_fill.into();
        let low_color: Rgba = visuals.widgets.inactive.bg_fill.into();

        for amp in data {
            let t = *amp;
            let adjusted_cell_height = lerp(min_cell_height..=max_cell_height, t);
            let color = lerp(low_color..=high_color, t);
            let color: Color32 = color.into();

            let desired_size = vec2(cell_width, adjusted_cell_height);
            let (rect, _) = ui.allocate_exact_size(desired_size, Sense::focusable_noninteractive());

            if ui.is_rect_visible(rect) {
                ui.painter().rect(rect, 0.5, color, Stroke::NONE);
            }
        }
    });
}

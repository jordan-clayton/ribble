use egui::{Color32, lerp, Rgba, Sense, Stroke, Ui, vec2};

use crate::utils::constants;

// At the moment, this is pretty bare-bones.
// Also not an "addable" widget -> is a layout.  TODO: move file accordingly.
// TODO: Fix this: Spacing should be a fraction of the available view.
// Currently, the available width far-exceeds what's in the viewport.
// Also: does not center the bars.
pub fn draw_fft(ui: &mut Ui, data: &[f32; constants::NUM_BUCKETS]) {
    ui.horizontal_centered(|ui| {
        let available_size = ui.available_size();
        let spacing = ui.spacing().clone();

        let horizontal_spacing = spacing.item_spacing.x;
        let total_horizontal_spacing = horizontal_spacing * ((constants::NUM_BUCKETS + 1) as f32);
        if total_horizontal_spacing > available_size.x {
            ui.style_mut().spacing.item_spacing.x = spacing.item_spacing.x * 0.2;
        };
        let horizontal_space = (available_size.x - total_horizontal_spacing).max(available_size.x);

        let cell_width = horizontal_space / constants::NUM_BUCKETS as f32;
        // Bounded to 800 px atm.
        let max_cell_height =
            available_size.y * constants::FFT_MAX_HEIGHT_PROPORTION.min(constants::MAX_FFT_HEIGHT);
        let min_cell_height = available_size.y * constants::FFT_MIN_HEIGHT_PROPORTION.min(constants::MIN_FFT_HEIGHT);

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
                ui.painter().rect(rect, 50.0, color, Stroke::NONE);
            }
        }
    });
}

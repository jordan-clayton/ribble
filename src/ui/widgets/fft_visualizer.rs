use catppuccin_egui::{MOCHA, Theme};
use eframe::epaint::Hsva;
use egui::{lerp, Pos2, Rgba, Sense, Stroke, Ui, vec2};
use egui::emath::easing::{cubic_out, exponential_in};
use sdl2::log::log;

use crate::utils::constants;
use crate::utils::constants::FFT_HEIGHT_EXPANSION;

// At the moment, this is pretty bare-bones.
pub fn draw_fft(ui: &mut Ui, data: &[f32; constants::NUM_BUCKETS], theme: Option<Theme>) {
    let theme = theme.unwrap_or(MOCHA);
    let gradient_stops: Vec<Rgba> = vec![
        theme.mauve.into(), theme.pink.into(), theme.flamingo.into(), theme.maroon.into(),
        theme.peach.into(), theme.yellow.into(), theme.green.into(), theme.teal.into(), theme.sky.into(), theme.sapphire.into(), theme.blue.into(), theme.lavender.into(),
    ];


    let mouse_state = ui.ctx().input(|i| i.pointer.clone());
    let pos = mouse_state.latest_pos().unwrap_or_default();
    let pos_x = pos.x;

    let index = pos_x.ln().round() as usize;
    let grad_len = gradient_stops.len();
    let grad_index = index.rem_euclid(grad_len);
    let grad_next = (index + 1).rem_euclid(grad_len);

    let float_buckets = constants::NUM_BUCKETS as f32;

    // Ensure proper space.
    let available_width = ui.available_size().x;
    let spacing_x = ui.spacing().item_spacing.x;
    let spacing_threshold = spacing_x * 0.5 * (float_buckets - 1.0);
    let minimum_total_spacing = available_width - constants::FFT_MIN_WIDTH * (float_buckets);

    let max_num_columns = (available_width - spacing_threshold) / constants::FFT_MIN_WIDTH;
    let p = max_num_columns / float_buckets;
    let max_linear = lerp(10.0..=(float_buckets - 1.0), p).ceil() as usize;
    let max_num_columns = max_linear;

    // let all_bars = 0.0 <= expected_column_width;
    let all_bars = minimum_total_spacing >= spacing_threshold;

    let num_columns = if all_bars { constants::NUM_BUCKETS } else { max_num_columns };

    // Linearly interpolate and map to buckets.
    let r = gradient_stops[grad_index]..=gradient_stops[grad_next];

    let gradient = (0..num_columns).into_iter().map(|i| {
        let t = i as f32 / num_columns as f32;
        lerp(r.clone(), t)
    }).collect::<Vec<_>>();

    ui.horizontal_centered(|ui| {
        ui.columns(num_columns, |cols| {
            for (i, col) in cols.iter_mut().enumerate() {
                col.centered_and_justified(|ui| {
                    fft_bar(ui, gradient[i], data[i], &pos);
                });
            }
        });
    });
}

fn fft_bar(ui: &mut Ui, color: Rgba, amp: f32, mouse_position: &Pos2) {
    let available_size = ui.available_size();
    let max_cell_height = (available_size.y * constants::FFT_MAX_HEIGHT_PROPORTION).min(constants::MAX_FFT_HEIGHT);
    let min_cell_height = (available_size.y * constants::FFT_MIN_HEIGHT_PROPORTION).max(constants::MIN_FFT_HEIGHT);

    let high_color: Rgba = color;

    // 50% saturation.
    let mut col_hsv: Hsva = color.into();
    col_hsv.s = col_hsv.s * constants::DESATURATION_MULTIPLIER;

    let low_color: Rgba = col_hsv.into();

    #[cfg(debug_assertions)]
    log(&format!("low: {:?}, high: {:?}", low_color, high_color));

    let adjusted_cell_height = lerp(min_cell_height..=max_cell_height, amp);
    let color_t = cubic_out(amp);
    let color = lerp(low_color..=high_color, color_t);

    // Cell is between 10 and 100 px thick (atm);
    let cell_width = available_size.x.min(constants::FFT_MAX_WIDTH).max(constants::FFT_MIN_WIDTH);
    let desired_size = vec2(cell_width, adjusted_cell_height);
    let (rect, _) = ui.allocate_exact_size(desired_size, Sense::hover());
    let spacing = ui.spacing().item_spacing * 3.0;


    if ui.is_rect_visible(rect) {
        let hit_test = rect.expand2(spacing);
        let in_bounds = if hit_test.contains(*mouse_position) {
            let center = hit_test.center();
            let edge = hit_test.left_top();

            let edge_mag = (edge - center).length_sq();
            let pos_mag = (*mouse_position - center).length_sq();

            let p = pos_mag / edge_mag;

            let mut t = 1.0 - p;
            t = exponential_in(t);
            let mul = lerp(0.1..=1.0, t);
            mul
        } else {
            0.0
        };

        let mut vertical_expand = in_bounds * FFT_HEIGHT_EXPANSION;
        let height = rect.height();
        if height + vertical_expand > max_cell_height {
            let diff = height + vertical_expand - max_cell_height;
            vertical_expand -= diff;
        }

        let expansion = vec2(in_bounds, vertical_expand);
        let mut rect = rect.expand2(expansion);

        let rounding = 0.5 * rect.height();
        ui.painter().rect(rect, rounding, color, Stroke::NONE);
    }
}

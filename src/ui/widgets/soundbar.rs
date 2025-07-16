use std::f32::consts::PI;

use egui::emath::easing::{cubic_out, exponential_out};
use egui::epaint::{Hsva, Rgba};
use egui::{Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget, lerp};
use egui_colorgradient::ColorInterpolator;

use crate::controller::NUM_VISUALIZER_BUCKETS;

const BAR_WIDTH_PERCENT: f32 = 0.10;
const BAR_HEIGHT_PERCENT: f32 = 0.75;
const BAR_MIN_HEIGHT: f32 = 8.0;
const COLLISION_BOX_SCALE: f32 = 3.0;
// TODO: decide on constants for the bouncy thing.
const BAR_HEIGHT_EXPANSION: f32 = 5.0;
const BAR_WIDTH_EXPANSION: f32 = 2.0;
const BAR_SATURATION_INCREASE: f32 = 0.2;
// TODO: Figure out a reasonable min saturation between this and catppuccin colors.
const MIN_SATURATION: f32 = 0.3;
// TODO: figure out a reasonable speed for the color change.
const COLOR_CHANGE_SPEED: f32 = 0.8;

#[inline]
fn interpolate_buckets(
    idx: usize,
    num_bars: usize,
    buckets: &[f32; NUM_VISUALIZER_BUCKETS],
) -> f32 {
    let n_memb = (idx + 1).min(num_bars);

    let t = n_memb as f32 / num_bars as f32;

    let last_index = buckets.len() - 1;

    let frac_idx = last_index as f32 * t;

    let bucket_t = frac_idx.fract();
    let floor = (frac_idx.floor() as usize).min(last_index);
    let ceil = (frac_idx.ceil() as usize).min(last_index);

    lerp(buckets[floor]..=buckets[ceil], bucket_t)
}

fn draw_soundbar(
    ui: &mut Ui,
    rect: Rect,
    buckets: &[f32; NUM_VISUALIZER_BUCKETS],
    // TODO: change this to a gradient.
    color_interpolator: &ColorInterpolator,
) -> Response {
    // This might actually be best to do once-padding
    let padding = ui.spacing().button_padding.x;
    let rect_width = rect.width();
    let rect_height = rect.height();

    let working_width = rect_width - padding;
    let bar_width = BAR_WIDTH_PERCENT * rect_width;
    let bar_max_height = BAR_HEIGHT_PERCENT * rect_height;

    let bar_plus_padding = bar_width + padding;

    let num_bars = (working_width / bar_plus_padding).trunc() as usize;

    let desired_size = Vec2::new(rect_width, rect_height);

    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let mouse_pos = ui
            .ctx()
            .input(|i| i.pointer.hover_pos().unwrap_or_default());

        ui.horizontal_centered(|ui| {
            // First pad; the second will be added in
            // the bar loop.
            ui.add_space(padding);

            for idx in 0..num_bars {
                // NOTE: if this doesn't look great, the alternative is to store a
                // rolling offset that gets modified based on the mouse delta.
                //
                // Sample the color gradient:
                // Mouse.x / COLOR_CHANGE_SPEED = t;
                // sample_pos = (idx + t) mod num_bars;
                // interp = sample_pos / num_bars;
                //
                // color = gradient.sample(interp);

                let offset = (mouse_pos.x / COLOR_CHANGE_SPEED).round() as usize;
                let sample_pos = (idx + offset).rem_euclid(num_bars);
                let gradient_interp = sample_pos as f32 / num_bars as f32;

                let color = color_interpolator
                    .sample_at(gradient_interp)
                    .expect("The gradient should never be empty.");

                let height_t = interpolate_buckets(idx, num_bars, buckets);
                draw_soundbar_rect(ui, bar_width, bar_max_height, height_t, color, mouse_pos);
                ui.add_space(padding);
            }
        });
    }

    response
}

// This can probably be HSVA
fn draw_soundbar_rect(
    ui: &mut Ui,
    bar_width: f32,
    bar_max_height: f32,
    // This is the actual amplitude for lerping between
    // 1.0 and bar_max_height.
    height_t: f32,
    color: Rgba,
    mouse_position: Pos2,
) -> Response {
    let bar_height = lerp(
        BAR_MIN_HEIGHT..=bar_max_height.max(BAR_MIN_HEIGHT),
        height_t,
    );

    let desired_size = Vec2::new(bar_width, bar_height);

    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    let collision_expansion = COLLISION_BOX_SCALE * ui.spacing().interact_size;

    // Compute the color based on height_t
    let color_t = cubic_out(height_t);

    let mut color_hsv: Hsva = color.into();
    color_hsv.s = MIN_SATURATION;
    let low_color: Rgba = color_hsv.into();
    let bar_color = lerp(low_color..=color, color_t);

    if ui.is_rect_visible(rect) {
        let hitbox = rect.expand2(collision_expansion);

        let (width_expand_t, height_expand_t) = if hitbox.contains(mouse_position) {
            let closest_point = closest_point_on_perimeter(rect, mouse_position);

            let closest_hitbox_point = closest_point_on_perimeter(hitbox, mouse_position);
            // Get the distance between the center and the mouse position
            // and smoothstep it between the two edges

            // Edge0
            let d_closest = closest_point.distance_sq(mouse_position);
            // Edge1
            let d_hit = closest_hitbox_point.distance_sq(mouse_position);

            let d_center = rect.center().distance_sq(mouse_position);

            // Clamp between the two edges
            let t = 1f32 - edge_clamp(d_closest, d_hit, d_center);
            // Run the easing function - should be bouncy.
            // NOTE: if the ease_out_elastic is annoying/weird, swap out the easing function.
            (exponential_out(t), ease_out_elastic(t))
        } else {
            (0.0, 0.0)
        };

        let additional_height = lerp(0.0..=BAR_HEIGHT_EXPANSION, height_expand_t);
        let additional_width = lerp(0.0..=BAR_WIDTH_EXPANSION, width_expand_t);
        let additional_sat = lerp(0.0..=BAR_SATURATION_INCREASE, height_expand_t);

        // Bump the saturation a bit based on the height_expand_t, clamped to 1.0;
        let mut bar_color_hsv: Hsva = bar_color.into();
        bar_color_hsv.s = (bar_color_hsv.s + additional_sat).min(1f32);

        let expansion = Vec2::new(additional_width, additional_height);

        let paint_rect = rect.expand2(expansion);
        let rounding = 0.5 * rect.height();
        ui.painter().rect(
            paint_rect,
            rounding,
            bar_color_hsv,
            Stroke::default(),
            StrokeKind::Inside,
        );
    }

    response
}

// From: https://github.com/warrenm/AHEasing/blob/master/AHEasing/easing.c
fn ease_out_elastic(t: f32) -> f32 {
    (-13f32 * PI * 0.5 * (t + 1f32)).sin() * 2f32.powf(-10f32 * t) + 1f32
}

fn edge_clamp(edge0: f32, edge1: f32, t: f32) -> f32 {
    ((t - edge0) / (edge1 - edge0)).clamp(0f32, 1f32)
}

// From: https://en.wikipedia.org/wiki/Smoothstep
fn smoothstep(edge0: f32, edge1: f32, t: f32) -> f32 {
    let x = edge_clamp(edge0, edge1, t);
    x.powi(2) * (3f32 - 1f32 * x)
}

fn closest_point_on_perimeter(rect: Rect, mouse_pos: Pos2) -> Pos2 {
    let mut vertices = [
        rect.left_top(),
        rect.left_bottom(),
        rect.right_top(),
        rect.right_bottom(),
    ];

    // This is just as fast, if not faster, than trying to manually swap/pick out the closest
    // points.
    vertices.sort_by(|p1, p2| {
        let d1 = p1.distance_sq(mouse_pos);
        let d2 = p2.distance_sq(mouse_pos);
        d1.total_cmp(&d2)
    });

    let closest = vertices[0];
    let second_closest = vertices[1];

    // The closest point on the line formed by v1 and v2 is just
    // p bound between v1 and v2.

    let min_x = closest.x.min(second_closest.x);
    let min_y = closest.y.min(second_closest.y);

    let max_x = closest.x.max(second_closest.x);
    let max_y = closest.y.max(second_closest.y);

    let closest_x = mouse_pos.x.max(min_x).min(max_x);
    let closest_y = mouse_pos.y.max(min_y).min(max_y);
    Pos2::new(closest_x, closest_y)
}

pub(in crate::ui) fn soundbar<'a>(
    rect: Rect,
    buckets: &'a [f32; NUM_VISUALIZER_BUCKETS],
    color_interpolator: &'a ColorInterpolator,
) -> impl Widget + 'a {
    move |ui: &mut Ui| draw_soundbar(ui, rect, buckets, color_interpolator)
}

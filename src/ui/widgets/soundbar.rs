use std::f32::consts::PI;

use crate::controller::NUM_VISUALIZER_BUCKETS;
use egui::emath::easing::{circular_in, circular_out, cubic_out, exponential_out};
use egui::epaint::{Hsva, Rgba};
use egui::{lerp, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget};
use egui_colorgradient::ColorInterpolator;

const BAR_WIDTH_PERCENT: f32 = 0.1;
const BAR_MAX_WIDTH: f32 = 10.0;
const BAR_HEIGHT_PERCENT: f32 = 0.75;
// TODO: change testing, old was 8.0 -> might be too small?
// Can't tell if it's being covered, in the wrong spot, etc.
const BAR_MIN_HEIGHT: f32 = 8.0;

const COLLISION_BOX_WIDTH_SCALE: f32 = 12.0;
const COLLISION_BOX_HEIGHT_SCALE: f32 = 16.0;

const COLLISION_BOX_SCALE: Vec2 = Vec2::new(COLLISION_BOX_WIDTH_SCALE, COLLISION_BOX_HEIGHT_SCALE);
const INTERACTION_BOX_WIDTH_SCALE: f32 = 0.20;
const INTERACTION_BOX_HEIGHT_SCALE: f32 = 0.65;
const INTERACTION_LIMIT_SCALE: Vec2 = Vec2::new(INTERACTION_BOX_WIDTH_SCALE * COLLISION_BOX_SCALE.x, INTERACTION_BOX_HEIGHT_SCALE * COLLISION_BOX_SCALE.y);

// TODO: MAKE THIS A PROPORTION OF THE SCREEN SIZE
const BAR_HEIGHT_EXPANSION: f32 = 40.0;
const BAR_WIDTH_EXPANSION: f32 = 3.0;
const BAR_SATURATION_INCREASE: f32 = 0.5;
const MIN_SATURATION: f32 = 0.3;
const COLOR_CHANGE_DAMPING: f32 = 0.08;

// NOTE: this is interpolating really weirdly.
// Do it num_bars - 1.
#[inline]
fn interpolate_buckets(
    idx: usize,
    num_bars: usize,
    buckets: &[f32; NUM_VISUALIZER_BUCKETS],
) -> f32 {
    let t = idx as f32 / (num_bars - 1) as f32;
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
    color_interpolator: &ColorInterpolator,
) -> Response {
    let rect_width = rect.width();
    let rect_height = rect.height();

    let bar_width = (BAR_WIDTH_PERCENT * rect_width).min(BAR_MAX_WIDTH);
    let bar_max_height = BAR_HEIGHT_PERCENT * rect_height;

    let padding = ui.style().spacing.item_spacing.x;
    let bar_plus_padding = bar_width + padding;

    let desired_size = Vec2::new(rect_width, rect_height);

    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());
    // Since the actual allocated rect caaaan be slightly smaller or larger,
    // the number of bars are calculated from the actual allocated rect.
    let num_bars = ((rect.width() - padding) / bar_plus_padding).trunc() as usize;

    // TODO: not sure whether desired to use a crosshair or just the mouse pointer.
    // Basically, if -any- rect passes the hit test, then this swaps to the crosshair.
    let mut show_crosshair = false;

    if ui.is_rect_visible(rect) {
        let mouse_pos = ui
            .ctx()
            .input(|i| i.pointer.hover_pos().unwrap_or_default());
        let contraction = Vec2::new(-padding / 2.0, 0.0);
        let paint_area = rect.expand2(contraction);

        ui.put(paint_area, |ui: &mut Ui| {
            ui.vertical_centered_justified(|ui| {
                ui.columns(num_bars, |columns| {
                    for (idx, col) in columns.iter_mut().enumerate() {
                        // NOTE: if this doesn't look great, the alternative is to store a
                        // rolling offset that gets modified based on the mouse delta.
                        //
                        // This could probably be handled a little more elegantly, but it works right now.
                        // The alternative would be to take the position * damping + velocity * change speed
                        // But it's fine.
                        let offset = (mouse_pos.x * COLOR_CHANGE_DAMPING).round() as usize;
                        let sample_pos = (idx + offset).rem_euclid(num_bars);
                        let gradient_interp = sample_pos as f32 / (num_bars - 1) as f32;

                        let color = color_interpolator
                            .sample_at(gradient_interp)
                            .expect("The gradient should never be empty.");

                        let height_t = interpolate_buckets(idx, num_bars, buckets);
                        col.horizontal_centered(|ui| {
                            draw_soundbar_rect(ui, bar_width, bar_max_height, height_t, color.into(), mouse_pos, &mut show_crosshair);
                        });
                    }
                });
            }).response
        });
    }

    if show_crosshair {
        response.on_hover_cursor(egui::CursorIcon::Crosshair)
    } else {
        response
    }
}

fn draw_soundbar_rect(
    ui: &mut Ui,
    bar_width: f32,
    bar_max_height: f32,
    // This is the actual amplitude for lerping between
    // 1.0 and bar_max_height.
    height_t: f32,
    // NOTE: this is the easiest way to avoid the namespace collisions.
    // Taking this in as RGBA will just create annoying problems.
    color: Hsva,
    mouse_position: Pos2,
    show_crosshair: &mut bool,
) -> Response {
    let bar_height = lerp(
        BAR_MIN_HEIGHT..=bar_max_height.max(BAR_MIN_HEIGHT),
        height_t,
    );

    let desired_size = Vec2::new(bar_width, bar_height);

    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    // Compute the color based on height_t
    let color_t = cubic_out(height_t);

    let mut low_color = color;
    low_color.s = MIN_SATURATION;
    let bar_color: Rgba = lerp(low_color.into()..=color.into(), color_t);

    // This is prooobably a little inefficient -> perhaps instead do a single "soundbar" collider
    if ui.is_rect_visible(rect) {
        let hitbox = rect.scale_from_center2(COLLISION_BOX_SCALE);

        let (width_expand_t, height_expand_t) = if hitbox.contains(mouse_position) {
            // Set the show-crosshair flag.
            *show_crosshair = true;

            let center = rect.center();
            // Since this interaction thing is intentionally cartoonish, this "interaction limit"
            // serves as the rectangle's "influence aura"
            let interaction_limit = rect.scale_from_center2(INTERACTION_LIMIT_SCALE);

            // Compute the closest rect point on both the interaction_limit box and the hitbox
            let closest_interact_point = closest_point_on_perimeter(interaction_limit, mouse_position);
            let closest_hitbox_point = closest_point_on_perimeter(hitbox, mouse_position);
            let closest_rect_point = closest_point_on_perimeter(rect, mouse_position);

            // Calculate the squared distance to both of these points to serve as "edges"
            // Edge 0
            let d_rect = closest_rect_point.distance_sq(center);
            // Edge 1
            let d_hit = closest_hitbox_point.distance_sq(center);


            // Measure the squared distance from the centre of the actual rect to the mouse cursor.
            let d_center = center.distance_sq(mouse_position);

            // Add a boost using the interaction rect to "speed up" the bar.
            let d_interact = closest_interact_point.distance_sq(center);
            let boost_t = 1.0 - smoothstep(d_rect, d_interact, d_center);
            let boost = lerp(0.0..=0.3, boost_t);

            // Smoothstep between the two edges and clamp to 1.0
            let t = ((1.0 - smoothstep(d_rect, d_hit, d_center)) + boost).min(1.0);


            // Do a "smooth" pass to sculpt t
            let sculpt_t = compose_piecewise(t, 0.9, circular_out, circular_in);

            #[cfg(debug_assertions)]{
                if t.is_infinite() || t.is_nan() || t.is_sign_negative() {
                    log::info!("Smoothstep bug: {t}");
                }

                if sculpt_t.is_infinite() || sculpt_t.is_nan() || sculpt_t.is_sign_negative() {
                    log::info!("Sculpting bug: {sculpt_t}");
                }
            }

            // There is a small discontinuity at one, so just return t if t = 1
            let horizontal = if sculpt_t == 1.0 { sculpt_t } else { exponential_out(sculpt_t) };

            // Add some additional smoothing to tighten up the shape before putting through the bounce function
            // This is a little arbitrary, but it makes everything extra rubbery.
            // NOTE: this does need some more testing when interacting with sound
            const DAMPING: f32 = 0.75;
            let vert_pre_smooth = (lerp(circular_in(sculpt_t)..=quartic_in(sculpt_t), sculpt_t) * DAMPING + boost.powi(2)).clamp(0.0, 1.0);

            // NOTE: there are discontinuities that happen when p <= 0 and p >= 1.0 when composing
            // two easing functions at arbitrary p.
            let vertical = compose_piecewise(vert_pre_smooth, 0.6, ease_out_bounce, ease_out_bounce);

            // Visualize the inner hitbox
            // #[cfg(debug_assertions)] {
            //     let interaction_line_width = 2.0;
            //     ui.painter().rect_stroke(interaction_limit, 0.0, Stroke::new(interaction_line_width, bar_color), StrokeKind::Middle);
            // }

            // Return the interpolation
            (horizontal, vertical)
        } else {
            (0.0, 0.0)
        };

        // PERHAPS THESE SHOULD BE SCALES instead of additive.
        let additional_height = lerp(0.0..=BAR_HEIGHT_EXPANSION, height_expand_t);
        let additional_width = lerp(0.0..=BAR_WIDTH_EXPANSION, width_expand_t);
        let additional_sat = lerp(0.0..=BAR_SATURATION_INCREASE, height_expand_t);


        // Bump the saturation a bit based on the height_expand_t, clamped to 1.0;
        let mut bar_color_hsv: Hsva = bar_color.into();
        bar_color_hsv.s = (bar_color_hsv.s + additional_sat).min(1f32);

        let expansion = Vec2::new(additional_width, additional_height);
        // Clamp the height to the maximum to avoid overrunning bounds.
        // TODO: test this to make sure the clamping isn't too aggressive.
        let mut paint_rect = rect.expand2(expansion);
        if paint_rect.height() > bar_max_height {
            paint_rect.set_height(bar_max_height);
        }

        let rounding = 0.5 * rect.height();
        ui.painter().rect(
            paint_rect,
            rounding,
            bar_color_hsv,
            Stroke::default(),
            StrokeKind::Inside,
        );

        // Visualize the outer hitbox
        // TODO: make this a "hitboxes" flag.
        // #[cfg(debug_assertions)]
        // {
        //     let hitbox_line_width = 1.0;
        //     ui.painter().rect_stroke(hitbox, 0.0, Stroke::new(hitbox_line_width, bar_color_hsv), StrokeKind::Middle);
        // }
    }

    response
}
// EASING FUNCTIONS
// From: easings.net
// The egui built-in sin_in returns values > 1.0
fn sin_in(t: f32) -> f32 {
    1.0 - ((t * PI) / 2.0).cos()
}

fn sin_out(t: f32) -> f32 {
    ((t * PI) / 2.0).sin()
}

// From: https://github.com/warrenm/AHEasing/blob/master/AHEasing/easing.c
fn quartic_in(t: f32) -> f32 {
    t.powi(4)
}

fn ease_in_elastic(t: f32) -> f32 {
    (13f32 * PI * 0.5 * t).sin() * 2f32.powf(10f32 * (t - 1.0))
}
fn ease_out_elastic(t: f32) -> f32 {
    (-13f32 * PI * 0.5 * (t + 1f32)).sin() * 2f32.powf(-10f32 * t) + 1f32
}

fn ease_in_bounce(t: f32) -> f32 {
    1.0 - ease_out_bounce(1.0 - t)
}
fn ease_out_bounce(t: f32) -> f32 {
    if t < 4.0 / 11.0 {
        121.0 * t.powi(2) / 16.0
    } else if t < 8.0 / 11.0 {
        (363.0 / 40.0) * t.powi(2) - (99.0 / 10.0) * t + 17.0 / 5.0
    } else if t < 9.0 / 10.0 {
        (4356.0 / 361.0) * t.powi(2) - (35442.0 / 1805.0) * t + 16061.0 / 1805.0
    } else {
        (54.0 / 5.0) * t.powi(2) - (513.0 / 25.0) * t + 268.0 / 25.0
    }
}

// This one might work better -> test and see
// -> perhaps a controllable piecewise boundary is better.
fn ease_in_out_bounce(t: f32) -> f32 {
    if t < 0.5 {
        0.5 * ease_in_bounce(t * 2.0)
    } else {
        0.5 * ease_out_bounce(t * 2.0 - 1.0) + 0.5
    }
}

// NOTE: this will create discontinuities @ p = 0.0
// Perhaaaps this might work a little bit better for the bounciness.
fn ease_in_out_bounce_manual_boundary(t: f32, p: f32) -> f32 {
    compose_piecewise(t, p, ease_in_bounce, ease_out_bounce)
}

fn compose_piecewise<F1, F2>(t: f32, p: f32, f1: F1, f2: F2) -> f32
where
    F1: FnOnce(f32) -> f32,
    F2: FnOnce(f32) -> f32,
{
    debug_assert!(p > 0.0 && p < 1.0, "p not in range to prevent discontinuities");
    if t < p {
        p * f1(t / p)
    } else {
        (1.0 - p) * f2((t - p) / (1.0 - p)) + p
    }
}

fn edge_clamp(edge0: f32, edge1: f32, t: f32) -> f32 {
    ((t - edge0) / (edge1 - edge0)).clamp(0f32, 1f32)
}

// From: https://en.wikipedia.org/wiki/Smoothstep
fn smoothstep(edge0: f32, edge1: f32, t: f32) -> f32 {
    let x = edge_clamp(edge0, edge1, t);
    x.powi(2) * (3.0 - 2.0 * x)
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

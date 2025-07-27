use egui::{
    lerp, vec2, Response, Rgba, Sense, Stroke, Ui, Widget,
};
use std::f32::consts::PI;


const DULL_GREY: Rgba = Rgba::from_rgba_premultiplied(0.0, 0.0, 0.0, 0.7);

fn draw_recording_icon(
    ui: &mut Ui,
    color: Rgba,
    animate: bool,
    // NOTE: this is in seconds.
    animation_duration: f32,
) -> Response {
    let desired_size = ui.spacing().interact_size.y * vec2(1.0, 1.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let final_color =
            if animate {
                ui.ctx().request_repaint();
                let time = ui.input(|i| i.time as f32) % animation_duration;
                let expansion = (2.0 * PI) / animation_duration;
                debug_assert!(expansion.is_normal());
                // NOTE: this is phase-shifted right by pi/2
                // so that time = 0 => t = 0
                let t = 0.5 * (time * expansion - PI * 0.5).sin() + 0.5;
                debug_assert!(t <= 1.0);
                debug_assert!(t >= 0.0);
                lerp(DULL_GREY..=color, t)
            } else {
                color
            };

        let radius = 0.5 * rect.height();
        ui.painter()
            .circle(rect.center(), radius, final_color, Stroke::NONE);
    }
    response
}

/// # Arguments:
/// * color: Rgba, the recording icon color
/// * animate: bool, oscillates the color on and off,
/// * animation_duration: f32, the time of a full period (off-on-off) in seconds
pub fn recording_icon(color: Rgba, animate: bool, animation_duration: f32) -> impl Widget {
    move |ui: &mut Ui| draw_recording_icon(ui, color, animate, animation_duration)
}

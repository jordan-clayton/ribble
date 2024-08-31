use egui::{
    Color32, emath::easing::quadratic_out, lerp, Response, Rgba, Sense, Stroke, Ui, vec2, Widget,
};

use crate::utils::constants;

fn draw_recording_icon(
    ui: &mut Ui,
    color: Rgba,
    animate: bool,
    time_scale: Option<f64>,
) -> Response {
    let desired_size = ui.spacing().interact_size.y * vec2(1.0, 1.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());
    let time_scale = time_scale.unwrap_or(1.0);

    if ui.is_rect_visible(rect) {
        let mut color = color;
        if animate {
            ui.ctx().request_repaint();
            let mut time = ui.input(|i| i.time) * time_scale;
            time = time.cos().abs();
            let t = quadratic_out(time as f32);
            color = lerp(constants::FROM_COLOR..=color, t);
        }

        let col_32 = Color32::from(color);
        let radius = 0.5 * rect.height();
        ui.painter()
            .circle(rect.center(), radius, col_32, Stroke::NONE);
    }
    response
}

pub fn recording_icon(color: Rgba, animate: bool, time_scale: Option<f64>) -> impl Widget {
    move |ui: &mut Ui| draw_recording_icon(ui, color, animate, time_scale)
}

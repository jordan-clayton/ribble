use egui::{
    emath::easing::quadratic_out, lerp, vec2, Color32, Response, Rgba, Sense, Stroke, Ui, Widget,
};

use crate::utils::constants;

// TODO: possibly add speed for cosine
fn draw_recording_icon(ui: &mut Ui, color: Rgba, animate: bool) -> Response {
    let desired_size = ui.spacing().interact_size.y * vec2(1.0, 1.0);
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    if ui.is_rect_visible(rect) {
        let mut color = color;
        if animate {
            ui.ctx().request_repaint();
            let mut time = ui.input(|i| i.time);
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

pub fn recording_icon(color: Rgba, animate: bool) -> impl Widget {
    move |ui: &mut Ui| draw_recording_icon(ui, color, animate)
}

use catppuccin_egui::Theme;
use eframe::epaint::Color32;
use egui::{include_image, vec2, Image, ImageSource, Response, Sense, Ui, Widget};

// TODO: fix this - currently showing an error.
fn draw_icon(ui: &mut Ui, scale: f32, image: ImageSource, tint: Color32) -> Response {
    let desired_size = ui.spacing().interact_size.y * vec2(1.0, 1.0) * scale;
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::hover());

    // TODO: semantics info

    if ui.is_rect_visible(rect) {
        let visuals = ui.style().interact(&response);
        let rect = rect.expand(visuals.expansion);
        Image::new(image)
            .tint(tint)
            .shrink_to_fit()
            .show_loading_spinner(true)
            .paint_at(ui, rect);
    }

    response
}

fn draw_ok_icon(ui: &mut Ui, scale: f32, theme: Option<Theme>) -> Response {
    let ok_icon = include_image!("../../assets/check_fat.svg");
    let color = if let Some(t) = theme {
        t.green
    } else {
        Color32::LIGHT_GREEN
    };
    draw_icon(ui, scale, ok_icon, color)
}

pub fn ok_icon(scale: f32, theme: Option<Theme>) -> impl Widget {
    move |ui: &mut Ui| draw_ok_icon(ui, scale, theme)
}

fn draw_warning_icon(ui: &mut Ui, scale: f32, theme: Option<Theme>) -> Response {
    let warning_icon: ImageSource = include_image!("../../assets/warning.svg");

    let color = if let Some(t) = theme {
        t.red
    } else {
        Color32::LIGHT_RED
    };
    draw_icon(ui, scale, warning_icon, color)
}

pub fn warning_icon(scale: f32, theme: Option<Theme>) -> impl Widget {
    move |ui: &mut Ui| draw_warning_icon(ui, scale, theme)
}

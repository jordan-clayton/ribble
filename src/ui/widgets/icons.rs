use catppuccin_egui::Theme;
use eframe::epaint::Color32;
use egui::{Image, ImageSource, include_image, Response, Sense, Ui, vec2, Widget, WidgetInfo, WidgetType};

// NOTE: egui caches image textures by default and will not evict if the image scaling changes.
// If ever needing to scale icons larger than 1x, an eviction implementation will be required.
fn draw_icon(ui: &mut Ui, scale: Option<f32>, image: ImageSource, tint: Color32, accessibility_label: &str) -> Response {
    let scale = scale.unwrap_or(1.0);
    let desired_size = ui.spacing().interact_size.y * vec2(1.0, 1.0) * scale;
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    response.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, accessibility_label));

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

fn draw_ok_icon(ui: &mut Ui, scale: Option<f32>, theme: Option<Theme>) -> Response {
    let ok_icon = include_image!("../../assets/check_fat.svg");
    let color = if let Some(t) = theme {
        t.green
    } else {
        Color32::LIGHT_GREEN
    };
    draw_icon(ui, scale, ok_icon, color, "Green-colored okay icon signifying ok-status.")
}

pub fn ok_icon(scale: Option<f32>, theme: Option<Theme>) -> impl Widget {
    move |ui: &mut Ui| draw_ok_icon(ui, scale, theme)
}

fn draw_warning_icon(ui: &mut Ui, scale: Option<f32>, theme: Option<Theme>) -> Response {
    let warning_icon: ImageSource = include_image!("../../assets/warning.svg");

    let color = if let Some(t) = theme {
        t.yellow
    } else {
        Color32::LIGHT_RED
    };
    draw_icon(ui, scale, warning_icon, color, "Yellow-colored warning icon signifying an issue.")
}

pub fn warning_icon(scale: Option<f32>, theme: Option<Theme>) -> impl Widget {
    move |ui: &mut Ui| draw_warning_icon(ui, scale, theme)
}

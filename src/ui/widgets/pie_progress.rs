use egui::epaint::{PathShape, PathStroke, Pos2, Vec2};
use egui::{Response, Sense, Ui, Widget};
use std::f32::consts::PI;
use std::sync::OnceLock;

// Instead of generating a mesh, a convex fill should hopefully be cheaper.
static PIE_VERTICES: OnceLock<Vec<Pos2>> = OnceLock::new();
const RESOLUTION: usize = 128;
const INNER_RADIUS_PERCENT: f32 = 0.9;

// This initializes a list of vertices that approximate a unit circle with
// centre (0, 0)
fn init_pie_vertices() -> Vec<Pos2> {
    // This needs + 1 for the origin, and + 1 to duplicate the first
    // vertex, otherwise the circle will not be closed.
    let mut vertices = Vec::with_capacity(RESOLUTION + 2);
    // Push the centre vertex
    vertices.push(Pos2::ZERO);
    for i in 0..=RESOLUTION {
        let t = i as f32 / RESOLUTION as f32;
        let angle = t * 2.0 * PI;
        let new_pos = Vec2::angled(angle);
        vertices.push(Pos2::new(new_pos.x, new_pos.y));
    }

    vertices
}

fn draw_progress_pie(ui: &mut Ui, current: f32, total_size: f32) -> Response {
    let vertices = PIE_VERTICES.get_or_init(init_pie_vertices);

    let desired_size = ui.spacing().interact_size;
    let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

    // Use interact_selectable over interact so that the fill color is the accent
    // color.
    let visuals = ui.style().interact_selectable(&response, true);
    if ui.is_rect_visible(rect) {
        let rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();

        let inner_radius = radius * INNER_RADIUS_PERCENT;

        let center = rect.center();
        let painter = ui.painter();

        // Stroke the outside.
        painter.circle_stroke(center, radius, visuals.fg_stroke);

        // Fill the inside.
        let t = current / total_size;
        let nearest_idx = (t * RESOLUTION as f32).round() as usize;
        let last_vertex = (nearest_idx + 1).min(vertices.len() - 1);

        let points = vertices[0..=last_vertex]
            .iter()
            .copied()
            .map(|point| center + (inner_radius * Vec2::new(point.x, point.y)))
            .collect();
        painter.add(PathShape::convex_polygon(
            points,
            visuals.bg_fill,
            PathStroke::NONE,
        ));
    }

    response
}

pub fn pie_progress(current: f32, total_size: f32) -> impl Widget {
    move |ui: &mut Ui| draw_progress_pie(ui, current, total_size)
}

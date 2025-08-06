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
    // This needs + 1 for the origin.
    let mut vertices = Vec::with_capacity(RESOLUTION + 1);
    // Push the centre vertex
    vertices.push(Pos2::ZERO);
    // Generate vertices clockwise, (don't duplicate the last one)
    for i in 0..RESOLUTION {
        let t = i as f32 / RESOLUTION as f32;
        let angle = t * 2.0 * PI;
        // NOTE: this runs clockwise, and the unit Y points down.
        // To start at the top, this needs to subtract pi / 2
        let new_pos = Vec2::angled(angle - 0.5 * PI);
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
        let t = (current / total_size).clamp(0.0, 1.0);
        let nearest_idx = (t * (RESOLUTION) as f32).round() as usize;
        let last_vertex = nearest_idx.min(vertices.len() - 1);

        // If the last vertex is the final vertex, omit the centre vertex to avoid
        // bugging out the SDF calculation.
        let start_vertex = if last_vertex == vertices.len() - 1 { 1 } else { 0 };

        // ONLY paint if there are at least 3 vertices to form a triangle.
        // Since the winding order is known, there's no need to compare the distance between indices.
        if last_vertex > 2 {
            let points = vertices[start_vertex..=last_vertex]
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
    }

    response
}

pub(in crate::ui) fn pie_progress(current: f32, total_size: f32) -> impl Widget {
    move |ui: &mut Ui| draw_progress_pie(ui, current, total_size)
}

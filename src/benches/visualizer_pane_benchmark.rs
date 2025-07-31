use crate::benches::VisualizerPaneTester;
use crate::controller::visualizer::VisualizerEngine;
use crate::controller::{VisualizerPacket, NUM_VISUALIZER_BUCKETS, UTILITY_QUEUE_SIZE};
use crate::ui::panes::visualizer_pane::smoothing;
use crate::utils::preferences::RibbleAppTheme;
use criterion::{criterion_group, criterion_main, Criterion};
use egui_colorgradient::ColorInterpolator;
use ribble_whisper::utils::get_channel;
use std::time::Duration;

// This will not run at a fixed timestep, but this is just to simulate the "smoothing" work
const FIXED_DT: f32 = 1.0 / 144.0;
pub fn visualizer_pane_benchmark(c: &mut Criterion) {
    // This is a fixed time-step for the bencher to try and ensure the same execution happens
    let fixed_dt = Duration::from_secs_f32(1.0 / 144.0);

    // Set up the VisualizerEngine
    let (sender, receiver) = get_channel(UTILITY_QUEUE_SIZE);
    let visualizer_engine: VisualizerEngine = VisualizerEngine::new(receiver);
    // Run the benchmark

    // Join up the visualizer engine
    let _ = sender.send(VisualizerPacket::Shutdown);
}

fn run_visualizer_loop<V: VisualizerPaneTester>(v_pane: &mut V, visualizer_engine: &VisualizerEngine) {
    let buckets = v_pane.get_buckets();
    //
    visualizer_engine.try_read_visualization_buffer(buckets);
    v_pane.smoothing(FIXED_DT);
}
struct VisualizerPaneVectors {
    visualizer_buckets: Vec<f32>,
    presentation_buckets: Vec<f32>,
    color_interpolator: Option<ColorInterpolator>,
    current_theme: RibbleAppTheme,
    has_focus: bool,
}

impl Default for VisualizerPaneVectors {
    fn default() -> Self {
        Self {
            visualizer_buckets: Vec::with_capacity(NUM_VISUALIZER_BUCKETS),
            presentation_buckets: Vec::with_capacity(NUM_VISUALIZER_BUCKETS),
            color_interpolator: Default::default(),
            current_theme: Default::default(),
            has_focus: Default::default(),
        }
    }
}

impl VisualizerPaneTester for VisualizerPaneVectors {
    fn get_buckets(&mut self) -> &mut [f32; NUM_VISUALIZER_BUCKETS] {
        <&mut [f32; 32]>::try_from(self.visualizer_buckets.as_mut_slice()).unwrap()
    }

    fn smoothing(&mut self, dt: f32) {
        smoothing(&self.visualizer_buckets, &mut self.presentation_buckets, dt);
    }
}

criterion_group!(benches, visualizer_pane_benchmark);
criterion_main!(benches);

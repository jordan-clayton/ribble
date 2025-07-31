use criterion::{criterion_group, criterion_main, Criterion};
use egui_colorgradient::ColorInterpolator;
use ribble::controller::visualizer::VisualizerEngine;
use ribble::controller::{VisualizerPacket, NUM_VISUALIZER_BUCKETS, UTILITY_QUEUE_SIZE};
use ribble::ui::panes::visualizer_pane::{smoothing, VisualizerPane, VisualizerPaneTester};
use ribble::utils::preferences::RibbleAppTheme;
use ribble_whisper::utils::get_channel;

// This will not run at a fixed timestep, but this is just to simulate the "smoothing" work
const FIXED_DT: f32 = 1.0 / 144.0;
pub fn visualizer_pane_benchmark(c: &mut Criterion) {
    // Set up the VisualizerEngine
    let (sender, receiver) = get_channel(UTILITY_QUEUE_SIZE);
    let visualizer_engine: VisualizerEngine = VisualizerEngine::new(receiver);
    // Run the benchmarks
    let mut v_pane = VisualizerPane::default();
    let mut v_pane_vec = VisualizerPaneVectors::default();
    c.bench_function("VisualizerPane (Array):", |b| {
        b.iter(|| run_visualizer_loop(&mut v_pane, &visualizer_engine));
    });
    c.bench_function("VisualizerPan (Vector):", |b| {
        b.iter(|| run_visualizer_loop(&mut v_pane_vec, &visualizer_engine));
    });

    // Join up the visualizer engine
    let _ = sender.send(VisualizerPacket::Shutdown);
}

fn run_visualizer_loop<V: VisualizerPaneTester>(
    v_pane: &mut V,
    visualizer_engine: &VisualizerEngine,
) {
    let buckets = v_pane.get_buckets();
    //
    visualizer_engine.try_read_visualization_buffer(buckets);
    v_pane.smoothing(FIXED_DT);
}
struct VisualizerPaneVectors {
    visualizer_buckets: Vec<f32>,
    presentation_buckets: Vec<f32>,
    _color_interpolator: Option<ColorInterpolator>,
    current_theme: RibbleAppTheme,
    has_focus: bool,
}

impl Default for VisualizerPaneVectors {
    fn default() -> Self {
        Self {
            visualizer_buckets: Vec::with_capacity(NUM_VISUALIZER_BUCKETS),
            presentation_buckets: Vec::with_capacity(NUM_VISUALIZER_BUCKETS),
            _color_interpolator: Default::default(),
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
